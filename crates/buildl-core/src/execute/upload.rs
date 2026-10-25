//! What a run that succeeded tells the remote cache, and what the cache answers.
//!
//! Its own file because the local content store knows a file by its digest alone, while a remote
//! cache names every file with its size. The sizes are read here, where the bytes are, and the
//! result is handed over as a plain value together with a way to read the bytes it names.
//!
//! Responsibilities: [`upload_of`], which describes a run in the terms a remote cache stores it;
//! [`StoredBlobs`], which lets a remote adapter read the local content store; and [`upload`],
//! which sends the description and says what the answer means for the build.
//!
//! Non-responsibilities: deciding when to upload, and what a failed call does to the worker that
//! met it, which the worker does; and the order of the calls on the wire, which the remote
//! adapter owns.

use crate::error::{Error, PortName, Result};
use crate::execute::perform::{Kept, Printed, Task};
use crate::ports::{BlobSource, ContentStore, RemoteCache};
use crate::types::{
    Blob, Diagnostic, Digest, Label, OutputManifest, RemoteAction, RemoteCall, RemoteContent,
    RemoteNotice, RemoteOutput, RemoteTree, RemoteTreeFile, RemoteUpload, Uploaded,
};

/// What `task` produced, in the terms a remote cache stores a result.
///
/// `kept` holds one entry per declared output, in declared order, and `printed` what the action
/// wrote. Each output keeps the path it is declared at, and a directory lists its files in path
/// order. The size of every file and of both logs is read from `cas`.
///
/// # Errors
///
/// Returns [`Error::Adapter`] naming [`PortName::ContentStore`] when `kept` does not hold one
/// entry per declared output, or when a file or a log is not held, and whatever `cas` returns
/// when it cannot read one.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the worker does not upload yet")
)]
pub(crate) fn upload_of<C: ContentStore>(
    task: &Task,
    kept: &[Kept],
    printed: &Printed,
    cas: &C,
) -> Result<RemoteUpload> {
    let declared = &task.action.outputs;
    if kept.len() != declared.len() {
        return Err(Error::Adapter {
            port: PortName::ContentStore,
            diagnostic: Diagnostic::new(format!(
                "{} produced {} outputs but declares {}",
                task.action.label,
                kept.len(),
                declared.len()
            )),
        });
    }
    let outputs = declared
        .iter()
        .zip(kept)
        .map(|(path, kept)| {
            let content = match &kept.manifest {
                OutputManifest::File(file) => RemoteContent::File {
                    blob: blob_of(&file.digest, cas)?,
                    mode: file.mode,
                },
                OutputManifest::Tree(tree) => {
                    let files = tree
                        .entries()
                        .iter()
                        .map(|entry| {
                            Ok(RemoteTreeFile {
                                path: entry.path.clone(),
                                blob: blob_of(&entry.digest, cas)?,
                                mode: entry.mode,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    RemoteContent::Tree(RemoteTree::new(files))
                }
            };
            Ok(RemoteOutput {
                path: path.clone(),
                content,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RemoteUpload {
        outputs,
        stdout: blob_of(&printed.stdout, cas)?,
        stderr: blob_of(&printed.stderr, cas)?,
    })
}

/// The blob that the content held under `digest` is: the digest, and the length of what `cas`
/// holds there.
fn blob_of<C: ContentStore>(digest: &Digest, cas: &C) -> Result<Blob> {
    let bytes = cas.get(digest)?.ok_or_else(|| Error::Adapter {
        port: PortName::ContentStore,
        diagnostic: Diagnostic::new(format!("{digest} is not held, and an upload names it")),
    })?;
    Ok(Blob {
        digest: *digest,
        size: bytes.len() as u64,
    })
}

/// The local content store, as a remote adapter reads the blobs of an upload.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the worker does not upload yet")
)]
pub(crate) struct StoredBlobs<'a, C>(pub(crate) &'a C);

impl<C: ContentStore> BlobSource for StoredBlobs<'_, C> {
    fn read(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
        self.0.get(digest)
    }
}

/// Sends what a run produced to the remote cache as the result for `action`, and says what the
/// answer means for the build.
///
/// `label` is the target that ran. `Ok(None)` means the result and everything it names are
/// stored. `Ok(Some(notice))` means an input file changed after its digest was taken and nothing
/// was stored: a [`RemoteNotice::InputChanged`], which is not a failure and leaves the remote
/// on. A failed call is `Err` of a [`RemoteNotice::Failed`] for [`RemoteCall::Upload`], carrying
/// the adapter's own message.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the worker does not upload yet")
)]
pub(crate) fn upload<R: RemoteCache, C: ContentStore>(
    remote: &R,
    label: &Label,
    action: &RemoteAction,
    produced: &RemoteUpload,
    cas: &C,
) -> core::result::Result<Option<RemoteNotice>, RemoteNotice> {
    match remote.upload(action, produced, &StoredBlobs(cas)) {
        Ok(Uploaded::Done) => Ok(None),
        Ok(Uploaded::InputChanged(path)) => Ok(Some(RemoteNotice::InputChanged {
            label: label.clone(),
            path,
        })),
        Err(error) => Err(RemoteNotice::Failed {
            call: RemoteCall::Upload,
            diagnostic: match error {
                Error::Adapter { diagnostic, .. } => diagnostic,
                other => Diagnostic::new(other.to_string()),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{StoredBlobs, upload, upload_of};
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, file, kept, key, label, output_path, tree};
    use crate::execute::perform::{Printed, Task};
    use crate::ports::{BlobSource, ContentStore, RemoteCache};
    use crate::types::{
        Blob, Diagnostic, Digest, FileMode, NetworkAccess, NodeId, OutputPath, Platform,
        ReadyAction, RemoteAction, RemoteCall, RemoteContent, RemoteHit, RemoteNotice,
        RemoteOutput, RemotePlatform, RemoteTree, RemoteTreeFile, RemoteUpload, RootPath,
        SourcePath, TargetRole, Timeout, TreePath, Uploaded,
    };

    const TARGET: &str = "//app:app";

    /// A store that keeps what it is given.
    #[derive(Default)]
    struct Store(Mutex<BTreeMap<Digest, Vec<u8>>>);

    impl Store {
        /// A store holding each of `contents`.
        fn holding(contents: &[&str]) -> Self {
            let store = Self::default();
            for content in contents {
                store.put(content.as_bytes()).unwrap();
            }
            store
        }
    }

    impl ContentStore for Store {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            let digest = Digest::of(bytes);
            self.0.lock().unwrap().insert(digest, bytes.to_vec());
            Ok(digest)
        }

        fn contains(&self, digest: &Digest) -> bool {
            self.0.lock().unwrap().contains_key(digest)
        }

        fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(digest).cloned())
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Ok(())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Ok(())
        }
    }

    /// A remote whose upload gives the scripted answer and notes what it was sent and what the
    /// blob source read for it.
    struct Remote {
        answer: Mutex<Option<Result<Uploaded>>>,
        sent: Mutex<Vec<RemoteUpload>>,
        read_back: Mutex<Vec<Option<Vec<u8>>>>,
    }

    impl Remote {
        fn answering(answer: Result<Uploaded>) -> Self {
            Self {
                answer: Mutex::new(Some(answer)),
                sent: Mutex::new(Vec::new()),
                read_back: Mutex::new(Vec::new()),
            }
        }
    }

    impl RemoteCache for Remote {
        fn lookup(&self, _action: &RemoteAction) -> Result<Option<RemoteHit>> {
            unreachable!("an upload asks for no result")
        }

        fn download(&self, _blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
            unreachable!("an upload downloads nothing")
        }

        fn decode_tree(&self, _bytes: &[u8]) -> Result<Option<Vec<RemoteTreeFile>>> {
            unreachable!("an upload decodes no tree")
        }

        fn upload(
            &self,
            _action: &RemoteAction,
            produced: &RemoteUpload,
            blobs: &dyn BlobSource,
        ) -> Result<Uploaded> {
            self.sent.lock().unwrap().push(produced.clone());
            self.read_back
                .lock()
                .unwrap()
                .push(blobs.read(&produced.stdout.digest)?);
            self.answer.lock().unwrap().take().unwrap()
        }
    }

    /// A task declaring the outputs `names`.
    fn task(names: &[&str]) -> Task {
        Task {
            node: NodeId::new(0),
            role: TargetRole::Build,
            key: key(names),
            action: ReadyAction {
                label: label(TARGET),
                command: command(&["cc"]),
                description: None,
                env: BTreeMap::new(),
                inputs: Vec::new(),
                deps: Vec::new(),
                outputs: names.iter().map(|name| output_path(TARGET, name)).collect(),
                network: NetworkAccess::Sealed,
            },
        }
    }

    fn printed() -> Printed {
        Printed {
            stdout: Digest::of(b"compiling\n"),
            stderr: Digest::of(b"warning: unused\n"),
        }
    }

    fn tree_file(path: &str, content: &str) -> RemoteTreeFile {
        RemoteTreeFile {
            path: TreePath::parse(path).unwrap(),
            blob: Blob::of(content.as_bytes()),
            mode: FileMode::Plain,
        }
    }

    /// A description of nothing in particular, for a remote that does not look at it.
    fn action() -> RemoteAction {
        RemoteAction {
            salt: key(&["app"]).digest(),
            command: command(&["cc"]),
            env: BTreeMap::new(),
            outputs: Vec::new(),
            files: Vec::new(),
            platform: RemotePlatform::of(&Platform::parse("linux/x86_64").unwrap()),
            timeout: Timeout::parse("1m").unwrap(),
        }
    }

    fn empty_upload() -> RemoteUpload {
        RemoteUpload {
            outputs: Vec::new(),
            stdout: Blob::of(b"compiling\n"),
            stderr: Blob::of(b""),
        }
    }

    #[test]
    fn an_upload_names_every_output_with_sizes() {
        let task = task(&["bin", "inc"]);
        let store = Store::holding(&[
            "binary bytes",
            "a.h body",
            "dir/b.h",
            "compiling\n",
            "warning: unused\n",
        ]);
        let outputs = [
            kept(file("binary bytes")),
            kept(tree(&[("dir/b.h", "dir/b.h"), ("a.h", "a.h body")])),
        ];

        let upload = upload_of(&task, &outputs, &printed(), &store).unwrap();

        assert_eq!(
            upload.outputs,
            [
                RemoteOutput {
                    path: output_path(TARGET, "bin"),
                    content: RemoteContent::File {
                        blob: Blob::of(b"binary bytes"),
                        mode: FileMode::Plain,
                    },
                },
                RemoteOutput {
                    path: output_path(TARGET, "inc"),
                    content: RemoteContent::Tree(RemoteTree::new(vec![
                        tree_file("a.h", "a.h body"),
                        tree_file("dir/b.h", "dir/b.h"),
                    ])),
                },
            ]
        );
        let RemoteContent::File { blob, .. } = &upload.outputs[0].content else {
            unreachable!("the first output was a file");
        };
        assert_eq!(blob.size, "binary bytes".len() as u64);
        let RemoteContent::Tree(files) = &upload.outputs[1].content else {
            unreachable!("the second output was a directory");
        };
        let sizes: Vec<u64> = files.files().iter().map(|f| f.blob.size).collect();
        assert_eq!(sizes, ["a.h body".len() as u64, "dir/b.h".len() as u64]);
    }

    #[test]
    fn an_upload_names_stdout_and_stderr() {
        let store = Store::holding(&["compiling\n", "warning: unused\n"]);

        let upload = upload_of(&task(&[]), &[], &printed(), &store).unwrap();

        assert_eq!(upload.outputs, []);
        assert_eq!(upload.stdout, Blob::of(b"compiling\n"));
        assert_eq!(upload.stdout.size, 10);
        assert_eq!(upload.stderr, Blob::of(b"warning: unused\n"));
        assert_eq!(upload.stderr.size, 16);
    }

    #[test]
    fn a_blob_the_store_does_not_hold_cannot_be_described() {
        let store = Store::holding(&["compiling\n", "warning: unused\n"]);

        let said = upload_of(&task(&["bin"]), &[kept(file("lost"))], &printed(), &store);

        assert!(matches!(
            said,
            Err(Error::Adapter {
                port: PortName::ContentStore,
                ..
            })
        ));
    }

    #[test]
    fn a_different_number_of_outputs_than_declared_is_refused() {
        let store = Store::holding(&["compiling\n", "warning: unused\n", "x"]);

        let said = upload_of(
            &task(&["bin", "inc"]),
            &[kept(file("x"))],
            &printed(),
            &store,
        );

        assert!(matches!(
            said,
            Err(Error::Adapter {
                port: PortName::ContentStore,
                ..
            })
        ));
    }

    #[test]
    fn the_blob_source_reads_the_local_store() {
        let store = Store::holding(&["held bytes"]);
        let source = StoredBlobs(&store);

        assert_eq!(
            source.read(&Digest::of(b"held bytes")).unwrap(),
            Some(b"held bytes".to_vec())
        );
        assert_eq!(source.read(&Digest::of(b"never stored")).unwrap(), None);
    }

    #[test]
    fn a_stored_result_has_nothing_to_say_and_the_remote_reads_the_store() {
        let store = Store::holding(&["compiling\n"]);
        let remote = Remote::answering(Ok(Uploaded::Done));
        let produced = empty_upload();

        let said = upload(&remote, &label(TARGET), &action(), &produced, &store);

        assert_eq!(said, Ok(None));
        assert_eq!(*remote.sent.lock().unwrap(), [produced]);
        assert_eq!(
            *remote.read_back.lock().unwrap(),
            [Some(b"compiling\n".to_vec())]
        );
    }

    #[test]
    fn an_input_changed_answer_is_a_notice_not_a_failure() {
        let store = Store::holding(&["compiling\n"]);
        let changed = RootPath::from(&SourcePath::parse("src/main.c").unwrap());
        let remote = Remote::answering(Ok(Uploaded::InputChanged(changed.clone())));

        let said = upload(&remote, &label(TARGET), &action(), &empty_upload(), &store);

        assert_eq!(
            said,
            Ok(Some(RemoteNotice::InputChanged {
                label: label(TARGET),
                path: changed,
            })),
            "an answer, not an error: the worker leaves the remote on"
        );
    }

    #[test]
    fn a_failed_call_is_a_failure_of_the_upload_call_with_the_adapters_message() {
        let store = Store::holding(&["compiling\n"]);
        let remote = Remote::answering(Err(Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new("the server refused the blob"),
        }));

        let said = upload(&remote, &label(TARGET), &action(), &empty_upload(), &store);

        assert_eq!(
            said,
            Err(RemoteNotice::Failed {
                call: RemoteCall::Upload,
                diagnostic: Diagnostic::new("the server refused the blob"),
            })
        );
    }
}
