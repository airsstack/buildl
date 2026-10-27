//! A result a remote cache holds, checked and stored as if a local run had made it.
//!
//! Its own file because a remote answer is trusted for nothing: every path, size and hash in it
//! was written by another machine, and only what passes every check here may reach the content
//! store. The checks and the store writes are two steps with two different kinds of failure: a
//! remote that answers wrongly, and a content store that cannot keep what it was given. The
//! first step takes no content store, so an unchecked byte cannot be stored.
//!
//! Responsibilities: [`usable`], which decides whether an answer may stand for a task;
//! [`unasked`] and [`unusable`], which say why an answer cannot: it names a path nobody asked
//! for, or its outputs are ones a build cannot keep; [`fetch`], which downloads what the answer
//! names and checks it, returning [`Checked`] or [`Unfetched`]; [`Checked::keep`], which stores
//! it and returns what a run would have; and [`logs`], which does the same for the logs of a
//! run that left nothing to place, or whose outputs are refused.
//!
//! Non-responsibilities: asking the remote for the answer, deciding when to ask, and what a
//! failure does to the build, which the worker does; and encoding anything for the wire, which
//! is the remote adapter's.

use core::slice;

use crate::error::{Error, PortName, Result};
use crate::execute::perform::{Kept, OutputsFrom, Printed, Task, Unkept, collected_from, written};
use crate::ports::{ContentStore, RemoteCache};
use crate::types::{
    Blob, DecodedTree, Diagnostic, Digest, FileMode, Found, FoundEntry, FoundKind, OutputManifest,
    OutputPath, OutputSource, RemoteHit, RemoteTreeFile, StoredFile, TargetRole, Tree, TreeEntry,
};

/// The error for a result that cannot be used, saying what is wrong with it.
fn refused(what: impl Into<String>) -> Error {
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(what),
    }
}

/// Whether `hit` may stand for `task`.
///
/// It may when the command exited with status zero, it names every declared output path exactly
/// once, as a file or as a directory, and no other path, and it names no symbolic link. The
/// output of a discovery target must be a file, so a directory is never usable for one.
///
/// A target whose output is its standard output reads the other way: its path is declared, so
/// that a server reports a file the command wrote there, but the result must not name it, or
/// any other path, and must hold a standard output digest, since an absent one cannot be told
/// from empty output.
pub(crate) fn usable(hit: &RemoteHit, task: &Task) -> bool {
    let (outputs, role) = (&task.action.outputs, task.role);
    if hit.exit_code != 0 || !hit.symlinks.is_empty() {
        return false;
    }
    if task.action.output_source == OutputSource::Stdout {
        return hit.stdout.is_some() && hit.files.is_empty() && hit.directories.is_empty();
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

/// The first path `hit` names, as a file, a directory or a link, that is not one of `outputs`.
///
/// A result for the action that was sent names only the paths its command listed, so a path
/// outside them means the server answered for something else.
pub(crate) fn unasked<'hit>(hit: &'hit RemoteHit, outputs: &[OutputPath]) -> Option<&'hit str> {
    hit.files
        .iter()
        .map(|file| file.path.as_str())
        .chain(hit.directories.iter().map(|dir| dir.path.as_str()))
        .chain(hit.symlinks.iter().map(String::as_str))
        .find(|named| !outputs.iter().any(|output| output.as_str() == *named))
}

/// Why the outputs of `hit` cannot be kept for `task`, as the failure a local run leaving the
/// same would have: `None` when every declared output is there as a file or a directory a build
/// can keep. The exit status is not looked at.
///
/// The outputs are examined in declared order and the first that fails decides. A directory is
/// judged by being a directory; what it holds is checked when it is fetched.
///
/// For a target whose output is its standard output nothing may be at the path, and anything
/// there is [`OutcomeClass::OutputWritten`], whether or not the result holds a standard output
/// digest. A result that holds none and nothing at the path is not judged here: it is not the
/// action's fault, and [`fetch`] refuses it.
///
/// [`OutcomeClass::OutputWritten`]: crate::types::OutcomeClass::OutputWritten
pub(crate) fn unusable(hit: &RemoteHit, task: &Task) -> Option<Unkept> {
    let found: Vec<Found> = task
        .action
        .outputs
        .iter()
        .map(|at| found_at(hit, at.as_str()))
        .collect();
    let names = &task.key.components().own.outputs;
    match task.action.output_source {
        OutputSource::Command => {
            collected_from(task.role, &OutputsFrom::Command, names, found).err()
        }
        OutputSource::Stdout => written(names, &found),
    }
}

/// What `hit` reports at the output path `at`.
///
/// A directory is reported as a tree of one file, whatever it holds: its content is only known
/// once its tree is downloaded, and [`fetch`] judges that.
fn found_at(hit: &RemoteHit, at: &str) -> Found {
    if hit.symlinks.iter().any(|link| link == at) {
        Found::Symlink
    } else if let Some(file) = hit.files.iter().find(|file| file.path == at) {
        Found::File(StoredFile {
            digest: file.blob.digest,
            mode: file.mode,
        })
    } else if let Some(dir) = hit.directories.iter().find(|dir| dir.path == at) {
        Found::Tree(vec![FoundEntry {
            path: "tree".to_owned(),
            kind: FoundKind::File(StoredFile {
                digest: dir.tree.digest,
                mode: FileMode::Plain,
            }),
        }])
    } else {
        Found::Nothing
    }
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

/// Why a result was not fetched.
#[derive(Debug)]
pub(crate) enum Unfetched {
    /// A directory the result names holds something a build cannot place, so that a local run
    /// leaving the same would be refused: the failure it would have, and not a fault of the
    /// remote.
    Unkept(Unkept),
    /// The remote answered wrongly, or could not be asked.
    Failed(Error),
}

impl Unfetched {
    /// The error for a remote that answered with a result it cannot be used for.
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Failed(error) => error,
            Self::Unkept(unkept) => refused(unkept.diagnostic.map_or_else(
                || "the result holds an output a build cannot keep".to_owned(),
                |diagnostic| diagnostic.to_string(),
            )),
        }
    }
}

impl From<Error> for Unfetched {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
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
/// For a target whose output is its standard output the output is the standard output blob
/// itself, a plain file, so it is downloaded once and stored once.
///
/// # Errors
///
/// Returns [`Unfetched::Unkept`] when a directory holds a symlink or an empty directory, the
/// first declared output that does deciding which. Otherwise returns [`Unfetched::Failed`] with
/// an [`Error::Adapter`] naming [`PortName::RemoteCache`] when `hit` is not [`usable`] for
/// `task`, when a blob differs from the length or hash it was named with, when a tree is not
/// one, and whatever `remote` returns.
pub(crate) fn fetch<R: RemoteCache>(
    hit: &RemoteHit,
    task: &Task,
    remote: &R,
) -> core::result::Result<Checked, Unfetched> {
    let declared = &task.action.outputs;
    if !usable(hit, task) {
        return Err(refused(format!(
            "the result it holds does not stand for {}",
            task.action.label
        ))
        .into());
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
    let names = &task.key.components().own.outputs;
    let mut unkept: Option<(usize, Unkept)> = None;
    for (dir, bytes) in hit.directories.iter().zip(got) {
        let entries = match remote.decode_tree(&bytes)? {
            DecodedTree::Files(files) => {
                trees.push(files);
                continue;
            }
            DecodedTree::Symlink(path) => vec![FoundEntry {
                path,
                kind: FoundKind::Symlink,
            }],
            // The directory itself holds nothing.
            DecodedTree::EmptyDirectory(path) if path.is_empty() => Vec::new(),
            DecodedTree::EmptyDirectory(path) => vec![FoundEntry {
                path,
                kind: FoundKind::EmptyDirectory,
            }],
            DecodedTree::Unreadable => {
                return Err(
                    refused(format!("{} is not a directory a build can place", dir.path)).into(),
                );
            }
        };
        let (at, name) = declared
            .iter()
            .position(|output| output.as_str() == dir.path)
            .and_then(|at| Some((at, names.get(at)?)))
            .ok_or_else(|| refused(format!("the result names no output at {}", dir.path)))?;
        if let Err(refusal) = collected_from(
            task.role,
            &OutputsFrom::Command,
            slice::from_ref(name),
            vec![Found::Tree(entries)],
        ) && unkept.as_ref().is_none_or(|(first, _)| at < *first)
        {
            unkept = Some((at, refusal));
        }
    }
    if let Some((_, refusal)) = unkept {
        return Err(Unfetched::Unkept(refusal));
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

    let manifests = match task.action.output_source {
        OutputSource::Command => declared
            .iter()
            .map(|at| manifest_of(at.as_str(), hit, &trees))
            .collect::<Result<Vec<_>>>()?,
        OutputSource::Stdout => {
            let printed = hit
                .stdout
                .ok_or_else(|| refused("the result holds no standard output"))?;
            collected_from(
                task.role,
                &OutputsFrom::Stdout(printed.digest),
                names,
                declared.iter().map(|_| Found::Nothing).collect(),
            )
            .map_err(Unfetched::Unkept)?
        }
    };
    Ok(Checked {
        contents,
        stdout,
        stderr,
        manifests,
    })
}

/// The logs of a run that left nothing to place, once downloaded and checked and before
/// anything is stored.
#[derive(Debug)]
pub(crate) struct Logs {
    /// What the run wrote to standard output.
    stdout: Vec<u8>,
    /// What the run wrote to standard error.
    stderr: Vec<u8>,
}

impl Logs {
    /// Stores both logs, and returns their digests.
    ///
    /// # Errors
    ///
    /// Returns whatever `cas` returns.
    pub(crate) fn keep<C: ContentStore>(self, cas: &C) -> Result<Printed> {
        Ok(Printed {
            stdout: cas.put(&self.stdout)?,
            stderr: cas.put(&self.stderr)?,
        })
    }
}

/// Downloads and checks the logs of a run that left no result to place.
///
/// A log the server did not keep stands for empty content. Nothing is stored here; see
/// [`Logs::keep`].
///
/// # Errors
///
/// Returns [`Error::Adapter`] naming [`PortName::RemoteCache`] when a blob differs from the
/// length or hash it was named with, and whatever `remote` returns.
pub(crate) fn logs<R: RemoteCache>(
    stdout: Option<Blob>,
    stderr: Option<Blob>,
    remote: &R,
) -> Result<Logs> {
    let named: Vec<Blob> = stdout.into_iter().chain(stderr).collect();
    let mut got = downloaded(remote, &named)?.into_iter();
    Ok(Logs {
        stdout: stdout.and_then(|_| got.next()).unwrap_or_default(),
        stderr: stderr.and_then(|_| got.next()).unwrap_or_default(),
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

    use super::{Unfetched, fetch, logs, unasked, unusable, usable};
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, key, label, output_path};
    use crate::execute::perform::{OutputsFrom, Task, collected_from};
    use crate::ports::{BlobSource, ContentStore, RemoteCache};
    use crate::types::{
        Blob, DecodedTree, Diagnostic, Digest, FileMode, Found, FoundEntry, FoundKind,
        NetworkAccess, NodeId, OutcomeClass, OutputManifest, OutputName, OutputPath, OutputRefusal,
        OutputSource, ReadyAction, RemoteAction, RemoteHit, RemoteOutputDirectory,
        RemoteOutputFile, RemoteTreeFile, RemoteUpload, StoredFile, TargetRole, TreePath,
    };

    /// A remote that serves scripted bytes, and notes every download and decode asked of it.
    #[derive(Default)]
    struct Remote {
        /// The bytes served under each digest, which a test may make differ from the digest.
        served: BTreeMap<Digest, Vec<u8>>,
        /// What `decode_tree` answers for tree bytes, by their digest.
        trees: BTreeMap<Digest, DecodedTree>,
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
        fn serve_tree(&mut self, label: &str, decoded: DecodedTree) -> Blob {
            let blob = self.serve(label.as_bytes());
            self.trees.insert(blob.digest, decoded);
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

        fn decode_tree(&self, bytes: &[u8]) -> Result<DecodedTree> {
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
                output_source: OutputSource::Command,
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
        assert!(usable(&hit, &task));
    }

    #[test]
    fn a_non_zero_exit_is_unusable() {
        let task = task(TargetRole::Build, &["bin"]);
        for exit_code in [1, -1, 137] {
            let mut hit = hit(vec![output_file("bin", Blob::of(b"bin"))], Vec::new());
            hit.exit_code = exit_code;
            assert!(!usable(&hit, &task), "{exit_code}");
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
            assert!(!usable(&hit, &task), "{what}");
        }
    }

    #[test]
    fn an_output_symlink_is_unusable() {
        let task = task(TargetRole::Build, &["bin"]);
        let mut hit = hit(vec![output_file("bin", Blob::of(b"bin"))], Vec::new());
        hit.symlinks = vec!["link".to_owned()];
        assert!(!usable(&hit, &task));
    }

    #[test]
    fn a_directory_for_a_discovery_output_is_unusable() {
        let task = task(TargetRole::Discovery, &["found"]);
        let directory = hit(
            Vec::new(),
            vec![output_directory("found", Blob::of(b"found"))],
        );
        let file = hit(vec![output_file("found", Blob::of(b"found"))], Vec::new());
        assert!(!usable(&directory, &task));
        assert!(usable(&file, &task));
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
        let refused = fetch(&hit, &task, &remote).unwrap_err().into_error();
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
        let refused = fetch(&hit, &task, &remote).unwrap_err().into_error();
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

    /// The class a fetch that refuses a directory ends in.
    fn refusal_of(unfetched: Unfetched) -> OutcomeClass {
        match unfetched {
            Unfetched::Unkept(unkept) => unkept.class,
            Unfetched::Failed(error) => unreachable!("expected a refusal, got {error}"),
        }
    }

    #[test]
    fn a_tree_holding_a_symlink_is_refused_as_a_local_run_would_refuse_it() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let tree = remote.serve_tree(
            "tree with a link",
            DecodedTree::Symlink("sub/link".to_owned()),
        );
        let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);

        let unfetched = fetch(&hit, &task, &remote).unwrap_err();

        let Unfetched::Unkept(unkept) = unfetched else {
            unreachable!("expected a refusal");
        };
        assert_eq!(
            unkept.class,
            OutcomeClass::OutputRefused {
                name: OutputName::parse("inc").unwrap(),
                refusal: OutputRefusal::Symlink,
            }
        );
        let diagnostic = unkept.diagnostic.unwrap().to_string();
        assert!(diagnostic.contains("inc/sub/link"), "{diagnostic}");
        assert_eq!(remote.rounds().len(), 1, "no file is downloaded");
    }

    #[test]
    fn a_tree_holding_an_empty_directory_is_refused_as_a_local_run_would_refuse_it() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let nested =
            remote.serve_tree("nested", DecodedTree::EmptyDirectory("sub/none".to_owned()));
        let itself = remote.serve_tree("itself", DecodedTree::EmptyDirectory(String::new()));
        let class = |tree| {
            let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);
            refusal_of(fetch(&hit, &task, &remote).unwrap_err())
        };
        let refused = OutcomeClass::OutputRefused {
            name: OutputName::parse("inc").unwrap(),
            refusal: OutputRefusal::EmptyDirectory,
        };
        assert_eq!(class(nested), refused);
        assert_eq!(class(itself), refused);
    }

    #[test]
    fn of_two_refused_directories_the_first_declared_decides() {
        let task = task(TargetRole::Build, &["a", "b"]);
        let mut remote = Remote::default();
        let link = remote.serve_tree("link", DecodedTree::Symlink("l".to_owned()));
        let empty = remote.serve_tree("empty", DecodedTree::EmptyDirectory("e".to_owned()));
        let hit = hit(
            Vec::new(),
            vec![output_directory("b", empty), output_directory("a", link)],
        );

        assert_eq!(
            refusal_of(fetch(&hit, &task, &remote).unwrap_err()),
            OutcomeClass::OutputRefused {
                name: OutputName::parse("a").unwrap(),
                refusal: OutputRefusal::Symlink,
            }
        );
    }

    #[test]
    fn a_tree_that_cannot_be_read_is_a_failed_call() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let tree = remote.serve_tree("not a tree", DecodedTree::Unreadable);
        let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);

        let unfetched = fetch(&hit, &task, &remote).unwrap_err();

        assert!(matches!(unfetched, Unfetched::Failed(_)), "{unfetched:?}");
    }

    #[test]
    fn a_refusal_stands_as_an_error_for_a_lookup() {
        let task = task(TargetRole::Build, &["inc"]);
        let mut remote = Remote::default();
        let tree = remote.serve_tree("tree with a link", DecodedTree::Symlink("link".to_owned()));
        let hit = hit(Vec::new(), vec![output_directory("inc", tree)]);

        let error = fetch(&hit, &task, &remote).unwrap_err().into_error();

        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::RemoteCache,
                ..
            }
        ));
        assert!(error.to_string().contains("inc/link"), "{error}");
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
            DecodedTree::Files(vec![
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
            DecodedTree::Files(vec![
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
        let local = collected_from(
            TargetRole::Build,
            &OutputsFrom::Command,
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

    #[test]
    fn a_path_nobody_asked_for_is_named() {
        let task = task(TargetRole::Build, &["bin"]);
        let blob = Blob::of(b"x");
        let mut hit = hit(vec![output_file("bin", blob)], Vec::new());
        assert_eq!(unasked(&hit, declared(&task)), None);

        hit.files.push(output_file("other", blob));
        assert_eq!(unasked(&hit, declared(&task)), Some(at("other").as_str()));

        hit.files.pop();
        hit.symlinks.push("link".to_owned());
        assert_eq!(unasked(&hit, declared(&task)), Some("link"));

        hit.symlinks.clear();
        hit.directories.push(output_directory("dir", blob));
        assert_eq!(unasked(&hit, declared(&task)), Some(at("dir").as_str()));
    }

    #[test]
    fn a_result_with_every_output_has_nothing_to_refuse() {
        let task = task(TargetRole::Build, &["bin", "inc"]);
        let blob = Blob::of(b"x");
        let hit = hit(
            vec![output_file("bin", blob)],
            vec![output_directory("inc", blob)],
        );
        assert_eq!(unusable(&hit, &task), None);
    }

    #[test]
    fn the_first_declared_output_that_cannot_be_kept_decides() {
        let task = task(TargetRole::Build, &["a", "b", "c"]);
        let blob = Blob::of(b"x");
        let mut hit = hit(vec![output_file("a", blob)], Vec::new());
        hit.symlinks.push(at("c"));

        let refused = unusable(&hit, &task).unwrap();
        assert_eq!(
            refused.class,
            OutcomeClass::OutputMissing {
                name: OutputName::parse("b").unwrap()
            },
            "b is missing before c is a link"
        );

        hit.files.push(output_file("b", blob));
        let refused = unusable(&hit, &task).unwrap();
        assert_eq!(
            refused.class,
            OutcomeClass::OutputRefused {
                name: OutputName::parse("c").unwrap(),
                refusal: OutputRefusal::Symlink,
            }
        );
    }

    #[test]
    fn a_directory_where_a_discovery_target_needs_a_file_is_refused() {
        let task = task(TargetRole::Discovery, &["found"]);
        let hit = hit(
            Vec::new(),
            vec![output_directory("found", Blob::of(b"found"))],
        );
        let refused = unusable(&hit, &task).unwrap();
        assert_eq!(
            refused.class,
            OutcomeClass::OutputRefused {
                name: OutputName::parse("found").unwrap(),
                refusal: OutputRefusal::NotAFile,
            }
        );
    }

    /// A task whose one output, `name`, is what its command prints.
    fn printing(name: &str) -> Task {
        let mut task = task(TargetRole::Build, &[name]);
        task.action.output_source = OutputSource::Stdout;
        task
    }

    /// A result of a stdout target: nothing at its path, and `stdout` printed.
    fn printed_hit(stdout: Blob) -> RemoteHit {
        RemoteHit {
            stdout: Some(stdout),
            ..hit(Vec::new(), Vec::new())
        }
    }

    #[test]
    fn a_stdout_hit_takes_its_output_from_stdout() {
        let task = printing("pkgs");
        let mut remote = Remote::default();
        let stdout = remote.serve(b"{\"name\":\"a\"}\n");
        let hit = printed_hit(stdout);
        assert!(usable(&hit, &task));
        let store = Store::default();

        let (outputs, printed) = fetch(&hit, &task, &remote).unwrap().keep(&store).unwrap();

        let manifest = OutputManifest::File(StoredFile {
            digest: stdout.digest,
            mode: FileMode::Plain,
        });
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].manifest, manifest);
        assert_eq!(outputs[0].digest, Digest::of(&manifest.to_bytes().unwrap()));
        assert_eq!(printed.stdout, stdout.digest, "one blob is output and log");
        assert!(store.contains(&stdout.digest));
        assert_eq!(remote.rounds(), vec![vec![stdout]], "downloaded once");
    }

    #[test]
    fn a_stdout_hit_naming_the_path_is_output_written() {
        let task = printing("pkgs");
        let blob = Blob::of(b"x");
        let written = OutcomeClass::OutputWritten {
            name: OutputName::parse("pkgs").unwrap(),
        };
        let mut as_file = printed_hit(blob);
        as_file.files = vec![output_file("pkgs", blob)];
        let mut as_directory = printed_hit(blob);
        as_directory.directories = vec![output_directory("pkgs", blob)];
        let mut as_link = printed_hit(blob);
        as_link.symlinks = vec![at("pkgs")];

        for (hit, what) in [
            (as_file, "a file"),
            (as_directory, "a directory"),
            (as_link, "a link"),
        ] {
            assert_eq!(unusable(&hit, &task).unwrap().class, written, "{what}");
            assert!(!usable(&hit, &task), "{what}");
        }
    }

    #[test]
    fn a_stdout_hit_without_stdout_naming_the_path_is_output_written() {
        let task = printing("pkgs");
        let mut hit = hit(Vec::new(), Vec::new());
        hit.files = vec![output_file("pkgs", Blob::of(b"x"))];

        assert_eq!(
            unusable(&hit, &task).unwrap().class,
            OutcomeClass::OutputWritten {
                name: OutputName::parse("pkgs").unwrap()
            }
        );
    }

    #[test]
    fn a_stdout_hit_without_stdout_is_unusable() {
        let task = printing("pkgs");
        let hit = hit(Vec::new(), Vec::new());

        assert!(!usable(&hit, &task));
        assert_eq!(unusable(&hit, &task), None, "not a fault of the action");
        let refused = fetch(&hit, &task, &Remote::default()).unwrap_err();
        assert!(matches!(refused, Unfetched::Failed(_)), "{refused:?}");
    }

    #[test]
    fn a_stdout_hit_with_a_failed_exit_or_another_path_is_unusable() {
        let task = printing("pkgs");
        let blob = Blob::of(b"x");
        let mut failed = printed_hit(blob);
        failed.exit_code = 1;
        let mut other = printed_hit(blob);
        other.files = vec![output_file("other", blob)];

        assert!(!usable(&failed, &task));
        assert!(!usable(&other, &task));
    }

    #[test]
    fn an_executed_stdout_result_is_not_judged_output_missing() {
        let task = printing("pkgs");
        let hit = printed_hit(Blob::of(b"printed"));

        assert_eq!(unusable(&hit, &task), None);
    }

    #[test]
    fn logs_are_checked_and_a_log_not_kept_is_empty() {
        let mut remote = Remote::default();
        let stdout = remote.serve(b"out\n");
        let store = Store::default();

        let printed = logs(Some(stdout), None, &remote)
            .unwrap()
            .keep(&store)
            .unwrap();
        assert_eq!(printed.stdout, Digest::of(b"out\n"));
        assert_eq!(printed.stderr, Digest::of(b""));
        assert!(store.contains(&printed.stdout));

        let named = Blob::of(b"named");
        remote.served.insert(named.digest, b"other".to_vec());
        assert!(logs(None, Some(named), &remote).is_err());
        assert_eq!(remote.rounds().len(), 2);

        let none = logs(None, None, &remote).unwrap().keep(&store).unwrap();
        assert_eq!(none.stdout, Digest::of(b""));
        assert_eq!(
            remote.rounds().len(),
            2,
            "nothing asked for, nothing downloaded"
        );
    }
}
