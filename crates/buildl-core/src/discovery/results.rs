//! What each discovery target last produced, read back from the log and the content store.
//!
//! Its own file because it is a function over two ports, the build log's history and the content
//! store, that sits beside the pipeline and reads no build file.
//!
//! Responsibilities: [`results`].
//!
//! Non-responsibilities: choosing the run, which the `history` module's `last_success` does; and
//! loading build files with the results, which the caller does.

use std::collections::{BTreeMap, BTreeSet};

use crate::history::last_success;
use crate::ports::{ContentStore, RunHistory};
use crate::types::{Discovered, Label, LogLine, OutputManifest};

/// The last result of each of `labels`, and the labels that have none.
///
/// A label's result is the bytes of the one file its newest successful run produced. A log that
/// cannot be read, a damaged line, a blob the store does not hold or cannot read, and an output
/// that is a directory each leave the label without a result; none is an error, so a workspace
/// whose state is unreadable declares as if nothing had run.
///
/// The labels without a result come back in label order, each once.
pub(crate) fn results<H: RunHistory, C: ContentStore>(
    history: &H,
    cas: &C,
    labels: &[Label],
) -> (Discovered, Vec<Label>) {
    let lines = history.lines().unwrap_or_default();
    let mut found = BTreeMap::new();
    let mut missing = BTreeSet::new();
    let wanted: BTreeSet<&Label> = labels.iter().collect();
    for label in wanted {
        match read(cas, &lines, label) {
            Some(bytes) => {
                found.insert(label.clone(), bytes);
            }
            None => {
                missing.insert(label.clone());
            }
        }
    }
    (Discovered::new(found), missing.into_iter().collect())
}

/// The bytes of the file `label` last produced, or `None` when the log names no such run or the
/// store does not hold a file under it.
fn read<C: ContentStore>(cas: &C, lines: &[LogLine], label: &Label) -> Option<Vec<u8>> {
    let manifest = cas.get(&last_success(lines, label)?).ok()??;
    match OutputManifest::from_bytes(&manifest)? {
        OutputManifest::File(file) => cas.get(&file.digest).ok()?,
        OutputManifest::Tree(_) => None,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::results;
    use crate::error::{Error, PortName, Result};
    use crate::ports::{ContentStore, RunHistory};
    use crate::types::{
        Argument, Command, Diagnostic, Digest, Discovered, FileMode, Label, LogLine, LoggedRun,
        OutcomeClass, OutputManifest, OutputName, OutputPath, Produced, StoredFile, Timestamp,
        Tree, TreeEntry, TreePath,
    };

    /// A log that hands over `lines`, or cannot be read.
    struct Log(Option<Vec<LogLine>>);

    impl RunHistory for Log {
        fn lines(&self) -> Result<Vec<LogLine>> {
            self.0.clone().ok_or_else(|| Error::Adapter {
                port: PortName::RunHistory,
                diagnostic: Diagnostic::new("unreadable"),
            })
        }
    }

    /// A store holding blobs, by the digest of their bytes.
    #[derive(Default)]
    struct Blobs(BTreeMap<Digest, Vec<u8>>);

    impl Blobs {
        fn hold(&mut self, bytes: &[u8]) -> Digest {
            let digest = Digest::of(bytes);
            self.0.insert(digest, bytes.to_vec());
            digest
        }

        /// Holds a file of `content` and the manifest naming it; the manifest's digest.
        fn hold_file(&mut self, content: &[u8]) -> Digest {
            let digest = self.hold(content);
            self.hold_manifest(&OutputManifest::File(StoredFile {
                digest,
                mode: FileMode::Plain,
            }))
        }

        fn hold_manifest(&mut self, manifest: &OutputManifest) -> Digest {
            self.hold(&manifest.to_bytes().unwrap())
        }
    }

    fn unused() -> Error {
        Error::Adapter {
            port: PortName::ContentStore,
            diagnostic: Diagnostic::new("not used by a read"),
        }
    }

    impl ContentStore for Blobs {
        fn put(&self, _bytes: &[u8]) -> Result<Digest> {
            Err(unused())
        }

        fn contains(&self, digest: &Digest) -> bool {
            self.0.contains_key(digest)
        }

        fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(digest).cloned())
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Err(unused())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Err(unused())
        }
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// A successful run of `raw` whose one output has the manifest `manifest`.
    fn built(raw: &str, manifest: Digest) -> LogLine {
        LogLine::Event(LoggedRun {
            at: Timestamp::from_unix_nanos(1),
            class: OutcomeClass::Built,
            command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
            diagnostic: None,
            file: "build.lua".to_owned(),
            finished: None,
            key: None,
            label: label(raw),
            outputs: vec![Produced {
                name: OutputName::parse("pkgs.json").unwrap(),
                digest: manifest,
            }],
            started: None,
            stderr: None,
            stdout: None,
        })
    }

    #[test]
    fn results_reads_the_file_bytes() {
        let mut cas = Blobs::default();
        let manifest = cas.hold_file(b"[\"a\"]");
        let log = Log(Some(vec![built("//svc:pkgs", manifest)]));
        let (found, missing) = results(&log, &cas, &[label("//svc:pkgs")]);
        assert_eq!(
            found,
            Discovered::new(BTreeMap::from([(label("//svc:pkgs"), b"[\"a\"]".to_vec())]))
        );
        assert_eq!(missing, Vec::<Label>::new());
    }

    #[test]
    fn an_unreadable_log_is_no_result() {
        let (found, missing) = results(&Log(None), &Blobs::default(), &[label("//svc:pkgs")]);
        assert_eq!(found, Discovered::empty());
        assert_eq!(missing, [label("//svc:pkgs")]);
    }

    #[test]
    fn a_missing_blob_is_no_result() {
        let mut cas = Blobs::default();
        let manifest = cas.hold_file(b"x");
        let gone = Digest::of(b"gone");
        let log = Log(Some(vec![
            built("//svc:no_manifest", gone),
            built("//svc:ok", manifest),
        ]));

        let mut no_file = Blobs::default();
        let orphan = no_file.hold_manifest(&OutputManifest::File(StoredFile {
            digest: gone,
            mode: FileMode::Plain,
        }));
        let orphaned = Log(Some(vec![built("//svc:no_file", orphan)]));

        let labels = [label("//svc:no_manifest"), label("//svc:ok")];
        let (found, missing) = results(&log, &cas, &labels);
        assert_eq!(found.get(&label("//svc:ok")), Some(b"x".as_slice()));
        assert_eq!(missing, [label("//svc:no_manifest")]);

        let (found, missing) = results(&orphaned, &no_file, &[label("//svc:no_file")]);
        assert_eq!(found, Discovered::empty());
        assert_eq!(missing, [label("//svc:no_file")]);
    }

    #[test]
    fn a_tree_manifest_is_no_result() {
        let mut cas = Blobs::default();
        let inner = cas.hold(b"x");
        let tree = Tree::new(vec![TreeEntry {
            path: TreePath::parse("pkgs.json").unwrap(),
            digest: inner,
            mode: FileMode::Plain,
        }])
        .unwrap();
        let manifest = cas.hold_manifest(&OutputManifest::Tree(tree));
        let log = Log(Some(vec![built("//svc:pkgs", manifest)]));
        let (found, missing) = results(&log, &cas, &[label("//svc:pkgs")]);
        assert_eq!(found, Discovered::empty());
        assert_eq!(missing, [label("//svc:pkgs")]);
    }

    #[test]
    fn missing_lists_labels_without_a_result_in_label_order() {
        let mut cas = Blobs::default();
        let manifest = cas.hold_file(b"x");
        let log = Log(Some(vec![built("//svc:b", manifest)]));
        let labels = [
            label("//svc:c"),
            label("//svc:b"),
            label("//svc:a"),
            label("//svc:c"),
        ];
        let (found, missing) = results(&log, &cas, &labels);
        assert_eq!(found.get(&label("//svc:b")), Some(b"x".as_slice()));
        assert_eq!(missing, [label("//svc:a"), label("//svc:c")]);
    }
}
