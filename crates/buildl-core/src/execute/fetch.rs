//! A result a remote cache holds, checked and stored as if a local run had made it.
//!
//! Its own file because a remote answer is trusted for nothing: every path, size and hash in it
//! was written by another machine, and only what passes every check here may reach the content
//! store. The checks and the store writes are two steps with two different kinds of failure: a
//! remote that answers wrongly, and a content store that cannot keep what it was given. The
//! first step takes no content store, so an unchecked byte cannot be stored.
//!
//! Responsibilities: [`usable`], which decides whether an answer may stand for a task;
//! [`fetch`], which downloads what the answer names and checks it, returning [`Checked`]; and
//! [`Checked::keep`], which stores it and returns what a run would have.
//!
//! Non-responsibilities: asking the remote for the answer, deciding when to ask, and what a
//! failure does to the build, which the worker does; and encoding anything for the wire, which
//! is the remote adapter's.

use crate::error::{Error, PortName, Result};
use crate::execute::perform::{Kept, Printed, Task};
use crate::ports::{ContentStore, RemoteCache};
use crate::types::{
    Blob, Diagnostic, Digest, OutputManifest, OutputPath, RemoteHit, RemoteTreeFile, StoredFile,
    TargetRole, Tree, TreeEntry,
};

/// The error for a result that cannot be used, saying what is wrong with it.
fn refused(what: impl Into<String>) -> Error {
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(what),
    }
}

/// Whether `hit` may stand for a task with `role` that declares `outputs`.
///
/// It may when the command exited with status zero, it names every declared output path exactly
/// once, as a file or as a directory, and no other path, and it names no symbolic link. The
/// output of a discovery target must be a file, so a directory is never usable for one.
pub(crate) fn usable(hit: &RemoteHit, outputs: &[OutputPath], role: TargetRole) -> bool {
    if hit.exit_code != 0 || !hit.symlinks.is_empty() {
        return false;
    }
    if role == TargetRole::Discovery && !hit.directories.is_empty() {
        return false;
    }
    let mut named: Vec<&str> = hit
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(hit.directories.iter().map(|dir| dir.path.as_str()))
        .collect();
    named.sort_unstable();
    let mut declared: Vec<&str> = outputs.iter().map(OutputPath::as_str).collect();
    declared.sort_unstable();
    named == declared
}

/// Downloads `blobs`, and returns their bytes once each has the length and the SHA-256 it was
/// named with.
fn downloaded<R: RemoteCache>(remote: &R, blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
    if blobs.is_empty() {
        return Ok(Vec::new());
    }
    let got = remote.download(blobs)?;
    if got.len() != blobs.len() {
        return Err(refused(format!(
            "the remote answered with {} blobs for the {} asked for",
            got.len(),
            blobs.len()
        )));
    }
    for (blob, bytes) in blobs.iter().zip(&got) {
        if bytes.len() as u64 != blob.size {
            return Err(refused(format!(
                "blob {} has length {}, not the {} it was named with",
                blob.digest,
                bytes.len(),
                blob.size
            )));
        }
        if Digest::of(bytes) != blob.digest {
            return Err(refused(format!(
                "blob {} has a different SHA-256 from the one it was named with",
                blob.digest
            )));
        }
    }
    Ok(got)
}

/// The manifest of the output at `path`, from the files and trees of a checked hit.
fn manifest_of(
    path: &str,
    hit: &RemoteHit,
    trees: &[Vec<RemoteTreeFile>],
) -> Result<OutputManifest> {
    if let Some(file) = hit.files.iter().find(|file| file.path == path) {
        return Ok(OutputManifest::File(StoredFile {
            digest: file.blob.digest,
            mode: file.mode,
        }));
    }
    let at = hit
        .directories
        .iter()
        .position(|dir| dir.path == path)
        .ok_or_else(|| refused(format!("the result names no output at {path}")))?;
    let entries = trees[at]
        .iter()
        .map(|file| TreeEntry {
            path: file.path.clone(),
            digest: file.blob.digest,
            mode: file.mode,
        })
        .collect();
    Tree::new(entries)
        .map(OutputManifest::Tree)
        .map_err(|error| {
            refused(format!(
                "{path} is not a directory a build can place: {error}"
            ))
        })
}

/// What a remote answer holds once every check has passed and nothing is stored yet.
#[derive(Debug)]
pub(crate) struct Checked {
    /// The content of every file, each once.
    contents: Vec<Vec<u8>>,
    /// What the original run wrote to standard output.
    stdout: Vec<u8>,
    /// What the original run wrote to standard error.
    stderr: Vec<u8>,
    /// The manifest of each declared output, in declared order.
    manifests: Vec<OutputManifest>,
}

impl Checked {
    /// Stores everything, and returns the outputs and the printed content a local run would have
    /// left.
    ///
    /// # Errors
    ///
    /// Returns whatever `cas` returns, and [`Error::CanonicalJson`] for a manifest that cannot
    /// be serialized.
    pub(crate) fn keep<C: ContentStore>(self, cas: &C) -> Result<(Vec<Kept>, Printed)> {
        for bytes in &self.contents {
            cas.put(bytes)?;
        }
        let printed = Printed {
            stdout: cas.put(&self.stdout)?,
            stderr: cas.put(&self.stderr)?,
        };
        let outputs = self
            .manifests
            .into_iter()
            .map(|manifest| {
                let digest = cas.put(&manifest.to_bytes()?)?;
                Ok(Kept { digest, manifest })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((outputs, printed))
    }
}

/// Downloads and checks everything `hit` names for `task`.
///
/// The bytes come in two rounds: the logs and each directory's tree first, then, once each tree
/// is checked and read, every file. A blob is checked against its length and its SHA-256, and
/// nothing is handed back until every blob has passed. A hit without a standard output or error
/// digest stands for empty content. Nothing is stored here; see [`Checked::keep`].
///
/// # Errors
///
/// Returns [`Error::Adapter`] naming [`PortName::RemoteCache`] when `hit` is not [`usable`] for
/// `task`, when a blob differs from the length or hash it was named with, when a tree holds
/// what a build cannot place or is not a tree, and whatever `remote` returns.
pub(crate) fn fetch<R: RemoteCache>(hit: &RemoteHit, task: &Task, remote: &R) -> Result<Checked> {
    let declared = &task.action.outputs;
    if !usable(hit, declared, task.role) {
        return Err(refused(format!(
            "the result it holds does not stand for {}",
            task.action.label
        )));
    }

    // Round one: the logs and the tree blobs.
    let first: Vec<Blob> = hit
        .stdout
        .into_iter()
        .chain(hit.stderr)
        .chain(hit.directories.iter().map(|dir| dir.tree))
        .collect();
    let mut got = downloaded(remote, &first)?.into_iter();
    let stdout = hit.stdout.and_then(|_| got.next()).unwrap_or_default();
    let stderr = hit.stderr.and_then(|_| got.next()).unwrap_or_default();
    let mut trees = Vec::with_capacity(hit.directories.len());
    for (dir, bytes) in hit.directories.iter().zip(got) {
        let files = remote.decode_tree(&bytes)?.ok_or_else(|| {
            refused(format!(
                "{} holds a symlink or an empty directory",
                dir.path
            ))
        })?;
        trees.push(files);
    }

    // Round two: every file, once.
    let mut blobs: Vec<Blob> = hit
        .files
        .iter()
        .map(|file| file.blob)
        .chain(trees.iter().flatten().map(|file| file.blob))
        .collect();
    blobs.sort_unstable();
    blobs.dedup();
    let contents = downloaded(remote, &blobs)?;

    let manifests = declared
        .iter()
        .map(|at| manifest_of(at.as_str(), hit, &trees))
        .collect::<Result<Vec<_>>>()?;
    Ok(Checked {
        contents,
        stdout,
        stderr,
        manifests,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{fetch, usable};
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, key, label, output_path};
    use crate::execute::perform::{Task, collected};
    use crate::ports::{BlobSource, ContentStore, RemoteCache};
    use crate::types::{
        Blob, Diagnostic, Digest, FileMode, Found, FoundEntry, FoundKind, NetworkAccess, NodeId,
        OutputManifest, OutputName, OutputPath, ReadyAction, RemoteAction, RemoteHit,
        RemoteOutputDirectory, RemoteOutputFile, RemoteTreeFile, RemoteUpload, StoredFile,
        TargetRole, TreePath,
    };

    /// A remote that serves scripted bytes, and notes every download and decode asked of it.
    #[derive(Default)]
    struct Remote {
        /// The bytes served under each digest, which a test may make differ from the digest.
        served: BTreeMap<Digest, Vec<u8>>,
        /// What `decode_tree` answers for tree bytes, by their digest.
        trees: BTreeMap<Digest, Option<Vec<RemoteTreeFile>>>,
        downloads: Mutex<Vec<Vec<Blob>>>,
        decoded: Mutex<u32>,
    }

    impl Remote {
        /// Serves `bytes` under their own digest and returns the blob that names them.
        fn serve(&mut self, bytes: &[u8]) -> Blob {
            let blob = Blob::of(bytes);
            self.served.insert(blob.digest, bytes.to_vec());
            blob
        }

        /// Serves a tree blob and scripts what it decodes to.
        fn serve_tree(&mut self, label: &str, files: Option<Vec<RemoteTreeFile>>) -> Blob {
            let blob = self.serve(label.as_bytes());
            self.trees.insert(blob.digest, files);
            blob
        }

        fn rounds(&self) -> Vec<Vec<Blob>> {
            self.downloads.lock().unwrap().clone()
        }
    }

    fn unscripted(call: &str) -> Error {
        Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new(format!("{call} is not scripted")),
        }
    }

    impl RemoteCache for Remote {
        fn lookup(&self, _action: &RemoteAction) -> Result<Option<RemoteHit>> {
            Err(unscripted("lookup"))
        }

        fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
            self.downloads.lock().unwrap().push(blobs.to_vec());
            blobs
                .iter()
                .map(|blob| {
                    self.served
                        .get(&blob.digest)
                        .cloned()
                        .ok_or_else(|| unscripted("download of an unserved blob"))
                })
                .collect()
        }

        fn decode_tree(&self, bytes: &[u8]) -> Result<Option<Vec<RemoteTreeFile>>> {
            *self.decoded.lock().unwrap() += 1;
            self.trees
                .get(&Digest::of(bytes))
                .cloned()
                .ok_or_else(|| unscripted("decode_tree of an unknown tree"))
        }

        fn upload(
            &self,
            _action: &RemoteAction,
            _produced: &RemoteUpload,
            _blobs: &dyn BlobSource,
        ) -> Result<crate::types::Uploaded> {
            Err(unscripted("upload"))
        }

        fn execute(&self, _action: &RemoteAction) -> Result<crate::types::Executed> {
            Err(unscripted("execute"))
        }
    }

    /// A store that keeps what it is given.
    #[derive(Default)]
    struct Store(Mutex<BTreeMap<Digest, Vec<u8>>>);

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

    const TARGET: &str = "//app:app";

    /// A task with `role` declaring the outputs `names`.
    fn task(role: TargetRole, names: &[&str]) -> Task {
        Task {
            node: NodeId::new(0),
            role,
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

    fn at(name: &str) -> String {
        output_path(TARGET, name).as_str().to_owned()
    }

    fn output_file(name: &str, blob: Blob) -> RemoteOutputFile {
        RemoteOutputFile {
            path: at(name),
            blob,
            mode: FileMode::Plain,
        }
    }

    fn output_directory(name: &str, tree: Blob) -> RemoteOutputDirectory {
        RemoteOutputDirectory {
            path: at(name),
            tree,
        }
    }

    fn tree_file(path: &str, blob: Blob, mode: FileMode) -> RemoteTreeFile {
        RemoteTreeFile {
            path: TreePath::parse(path).unwrap(),
            blob,
            mode,
        }
    }

    /// A result that exited cleanly with the given files and directories and kept no logs.
    fn hit(files: Vec<RemoteOutputFile>, directories: Vec<RemoteOutputDirectory>) -> RemoteHit {
        RemoteHit {
            exit_code: 0,
            files,
            directories,
            symlinks: Vec::new(),
            stdout: None,
            stderr: None,
        }
    }

    fn declared(task: &Task) -> &[OutputPath] {
        &task.action.outputs
    }

    #[test]
    fn a_hit_with_every_output_once_is_usable() {
        let task = task(TargetRole::Build, &["bin", "inc"]);
        let hit = hit(
            vec![output_file("bin", Blob::of(b"bin"))],
            vec![output_directory("inc", Blob::of(b"inc"))],
        );
        assert!(usable(&hit, declared(&task), task.role));
    }

    #[test]
    fn a_non_zero_exit_is_unusable() {
        let task = task(TargetRole::Build, &["bin"]);
        for exit_code in [1, -1, 137] {
            let mut hit = hit(vec![output_file("bin", Blob::of(b"bin"))], Vec::new());
            hit.exit_code = exit_code;
            assert!(!usable(&hit, declared(&task), task.role), "{exit_code}");
        }
    }

    #[test]
    fn a_missing_or_extra_output_is_unusable() {
        let task = task(TargetRole::Build, &["a", "b"]);
        let blob = Blob::of(b"x");
        let missing = hit(vec![output_file("a", blob)], Vec::new());
        let extra = hit(
            vec![
                output_file("a", blob),
                output_file("b", blob),
                output_file("c", blob),
            ],
            Vec::new(),
        );
        let twice = hit(
            vec![
                output_file("a", blob),
                output_file("a", blob),
                output_file("b", blob),
            ],
            Vec::new(),
        );
        let both = hit(
            vec![output_file("a", blob), output_file("b", blob)],
            vec![output_directory("b", blob)],
        );
        for (hit, what) in [
            (missing, "one missing"),
            (extra, "one extra"),
            (twice, "one twice"),
            (both, "one as a file and a directory"),
        ] {
            assert!(!usable(&hit, declared(&task), task.role), "{what}");
        }
    }

    #[test]
    fn an_output_symlink_is_unusable() {
        let task = task(TargetRole::Build, &["bin"]);
        let mut hit = hit(vec![output_file("bin", Blob::of(b"bin"))], Vec::new());
        hit.symlinks = vec!["link".to_owned()];
        assert!(!usable(&hit, declared(&task), task.role));
    }

    #[test]
    fn a_directory_for_a_discovery_output_is_unusable() {
        let task = task(TargetRole::Discovery, &["found"]);
        let directory = hit(
            Vec::new(),
            vec![output_directory("found", Blob::of(b"found"))],
        );
        let file = hit(vec![output_file("found", Blob::of(b"found"))], Vec::new());
        assert!(!usable(&directory, declared(&task), task.role));
        assert!(usable(&file, declared(&task), task.role));
    }

    #[test]
    fn a_blob_whose_hash_differs_is_refused() {
        let task = task(TargetRole::Build, &["bin", "log"]);
        let mut remote = Remote::default();
        let good = remote.serve(b"good bytes");
        let named = Blob::of(b"named bytes");
        remote.served.insert(named.digest, b"other bytes".to_vec());
        assert_eq!(b"named bytes".len(), b"other bytes".len());
        let hit = hit(
            vec![output_file("bin", good), output_file("log", named)],
            Vec::new(),
        );
        let refused = fetch(&hit, &task, &remote).unwrap_err();
        assert!(matches!(
            refused,
            Error::Adapter {
                port: PortName::RemoteCache,
                ..
            }
        ));
    }

    #[test]
    fn a_blob_whose_length_differs_is_refused() {
        let task = task(TargetRole::Build, &["bin"]);
        let mut remote = Remote::default();
        let named = Blob::of(b"bin");
        remote.served.insert(named.digest, b"bin and more".to_vec());
        let hit = hit(vec![output_file("bin", named)], Vec::new());
        let refused = fetch(&hit, &task, &remote).unwrap_err();
        assert!(refused.to_string().contains("length"), "{refused}");
    }

    #[test]
    fn a_tree_blob_is_checked_like_any_other() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let named = Blob::of(b"the tree");
        remote.served.insert(named.digest, b"THE TREE".to_vec());
        let hit = hit(Vec::new(), vec![output_directory("inc", named)]);
        assert!(fetch(&hit, &task, &remote).is_err());
        assert_eq!(*remote.decoded.lock().unwrap(), 0, "decode_tree not called");
    }

    #[test]
    fn a_tree_holding_a_symlink_is_unusable() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let tree = remote.serve_tree("tree with a link", None);
        let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);
        let refused = fetch(&hit, &task, &remote).unwrap_err();
        assert!(refused.to_string().contains("inc"), "{refused}");
    }

    #[test]
    fn file_blobs_are_asked_for_only_after_their_tree() {
        let task = task(TargetRole::Build, &["bin", "inc"]);
        let mut remote = Remote::default();
        let bin = remote.serve(b"bin");
        let a = remote.serve(b"a");
        let b = remote.serve(b"b");
        let stdout = remote.serve(b"compiling\n");
        let tree = remote.serve_tree(
            "inc tree",
            Some(vec![
                tree_file("a.h", a, FileMode::Plain),
                tree_file("sub/b.h", b, FileMode::Plain),
            ]),
        );
        let mut hit = hit(
            vec![output_file("bin", bin)],
            vec![output_directory("inc", tree)],
        );
        hit.stdout = Some(stdout);
        fetch(&hit, &task, &remote).unwrap();

        let rounds = remote.rounds();
        assert_eq!(rounds.len(), 2, "{rounds:?}");
        assert_eq!(rounds[0], vec![stdout, tree]);
        let mut second = rounds[1].clone();
        second.sort();
        let mut files = vec![bin, a, b];
        files.sort();
        assert_eq!(second, files);
    }

    #[test]
    fn a_fetched_file_and_its_logs_are_stored_as_a_local_run_would() {
        let task = task(TargetRole::Build, &["bin"]);
        let mut remote = Remote::default();
        let bin = remote.serve(b"bin");
        let stderr = remote.serve(b"warning\n");
        let mut hit = hit(
            vec![RemoteOutputFile {
                mode: FileMode::Executable,
                ..output_file("bin", bin)
            }],
            Vec::new(),
        );
        hit.stderr = Some(stderr);
        let store = Store::default();
        let (outputs, printed) = fetch(&hit, &task, &remote).unwrap().keep(&store).unwrap();

        let manifest = OutputManifest::File(StoredFile {
            digest: Digest::of(b"bin"),
            mode: FileMode::Executable,
        });
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].manifest, manifest);
        assert_eq!(outputs[0].digest, Digest::of(&manifest.to_bytes().unwrap()));
        assert!(store.contains(&outputs[0].digest));
        assert!(store.contains(&Digest::of(b"bin")));
        assert_eq!(printed.stdout, Digest::of(b""), "no digest: empty content");
        assert_eq!(printed.stderr, Digest::of(b"warning\n"));
        assert!(store.contains(&printed.stderr));
        assert!(store.contains(&printed.stdout));
    }

    #[test]
    fn a_fetched_tree_has_the_manifest_a_local_run_would_store() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let go = remote.serve(b"go");
        let page = remote.serve(b"page");
        let tree = remote.serve_tree(
            "inc tree",
            Some(vec![
                tree_file("index.html", page, FileMode::Plain),
                tree_file("bin/go", go, FileMode::Executable),
            ]),
        );
        let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);
        let store = Store::default();
        let (outputs, _printed) = fetch(&hit, &task, &remote).unwrap().keep(&store).unwrap();

        let entry = |path: &str, content: &[u8], mode| FoundEntry {
            path: path.to_owned(),
            kind: FoundKind::File(StoredFile {
                digest: Digest::of(content),
                mode,
            }),
        };
        let names: Vec<OutputName> = vec![OutputName::parse("inc").unwrap()];
        let local = collected(
            TargetRole::Build,
            &names,
            vec![Found::Tree(vec![
                entry("bin/go", b"go", FileMode::Executable),
                entry("index.html", b"page", FileMode::Plain),
            ])],
        )
        .unwrap();
        assert_eq!(outputs[0].manifest, local[0]);
        assert_eq!(outputs[0].digest, Digest::of(&local[0].to_bytes().unwrap()));
        assert!(store.contains(&Digest::of(b"go")));
        assert!(store.contains(&Digest::of(b"page")));
    }
}
