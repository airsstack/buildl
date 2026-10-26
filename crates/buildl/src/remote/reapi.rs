//! The remote cache adapter: a [`RemoteCache`] over the REAPI.
//!
//! [`Reapi`] turns the core's remote values into REAPI messages and back, and sends them with
//! the blocking [`Client`]. The input root of an action is read from the workspace on disk and
//! the output and log blobs of an upload from the [`BlobSource`] the core hands in; nothing is
//! read from another adapter. The client connects on the first call, so a cache that is down is
//! the failure of that call and not of building the adapter.
//!
//! Responsibilities: the four calls of [`RemoteCache`], and the text of their failures.
//!
//! Non-responsibilities: judging a result the cache returns, which is the core's, and the
//! messages' shape, which `encode` and `input_tree` decide.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use buildl_core::{
    Blob, BlobSource, Diagnostic, Digest, Error, Executed, Hasher, PortName, RemoteAction,
    RemoteCache, RemoteContent, RemoteFile, RemoteHit, RemoteSettings, RemoteTreeFile,
    RemoteUpload, Result, RootPath, Uploaded,
};

use super::client::Client;
use super::encode::{self, Encoded, wire_digest};
use super::error::RemoteError;
use super::input_tree::{Input, InputTree, Unreadable, input_tree, measure};
use super::proto::build::bazel::remote::execution::v2 as wire;
use super::vocabulary::Method;

/// A remote cache reached over the REAPI.
pub struct Reapi {
    client: Client,
    root: PathBuf,
    write_group: usize,
}

/// The cache's address and the workspace root; the login token is never shown.
impl fmt::Debug for Reapi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reapi")
            .field("url", &self.client.url())
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Reapi {
    /// An adapter of the cache `settings` name, for the workspace at `root`.
    ///
    /// Nothing is sent: the channel is connected by the first call. A relative `ca` or token
    /// file, and the input files of an action, are resolved against `root`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RemoteToken`] when the token cannot be read, and an
    /// [`Error::Adapter`] naming the remote cache when the adapter cannot be set up.
    pub fn new(settings: &RemoteSettings, root: &Path) -> Result<Self> {
        match Client::new(settings, root) {
            Ok(client) => Ok(Self {
                client,
                root: root.to_path_buf(),
                write_group: WRITE_GROUP,
            }),
            Err(RemoteError::Token(error)) => Err(error),
            Err(error) => Err(adapter(format!("{}: {error}", settings.url()))),
        }
    }
}

#[cfg(all(test, feature = "test-server"))]
impl Reapi {
    /// This adapter, writing its groups of blobs at `bytes` and not at the usual size.
    const fn with_write_group(mut self, bytes: usize) -> Self {
        self.write_group = bytes;
        self
    }
}

/// A failure of the remote cache with the given text.
fn adapter(text: String) -> Error {
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(text),
    }
}

/// The most bytes of blobs, read from the content store or the workspace, held together before
/// they are written.
///
/// A group is written before a blob that would carry it past this size, so a blob larger than
/// this goes alone. The encoded messages of an upload are held in memory from the start, outside
/// the group.
const WRITE_GROUP: usize = 32 * 1024 * 1024;

/// Where the bytes of a blob an upload sends come from.
enum Source {
    /// A message the adapter encoded.
    Message(Vec<u8>),
    /// The local content store holds them.
    Store(Digest),
    /// An input file in the workspace, which must still hold the bytes it was hashed as.
    Input(RootPath),
}

/// The blobs an upload names, each once, in the order they are sent.
#[derive(Default)]
struct Parcel {
    entries: Vec<(wire::Digest, Source)>,
    named: BTreeSet<String>,
}

impl Parcel {
    fn add(&mut self, digest: wire::Digest, source: Source) {
        if self.named.insert(digest.hash.clone()) {
            self.entries.push((digest, source));
        }
    }

    fn add_message(&mut self, message: Encoded) {
        self.add(message.digest(), Source::Message(message.bytes));
    }

    fn add_stored(&mut self, blob: &Blob) -> std::result::Result<(), RemoteError> {
        self.add(wire_digest(blob)?, Source::Store(blob.digest));
        Ok(())
    }

    /// The blobs of the upload up to the input-root `Directory` messages, in the order the
    /// protocol is told them: the input files, the output files, the `Tree` of each directory
    /// output, stdout and stderr, then `directories`.
    fn of(
        inputs: &[Input<'_>],
        produced: &RemoteUpload,
        trees: Vec<Encoded>,
        directories: Vec<(wire::Digest, Vec<u8>)>,
    ) -> std::result::Result<Self, RemoteError> {
        let mut parcel = Self::default();
        for input in inputs {
            let blob = Blob {
                digest: input.file.digest,
                size: input.size,
            };
            parcel.add(wire_digest(&blob)?, Source::Input(input.file.path.clone()));
        }
        for output in &produced.outputs {
            match &output.content {
                RemoteContent::File { blob, .. } => parcel.add_stored(blob)?,
                RemoteContent::Tree(tree) => {
                    for file in tree.files() {
                        parcel.add_stored(&file.blob)?;
                    }
                }
            }
        }
        for tree in trees {
            parcel.add_message(tree);
        }
        parcel.add_stored(&produced.stdout)?;
        parcel.add_stored(&produced.stderr)?;
        for (digest, bytes) in directories {
            parcel.add(digest, Source::Message(bytes));
        }
        Ok(parcel)
    }
}

/// What reading the input files of an action found.
enum Inputs<'a> {
    /// Every file holds what its digest says; these are the files as they were read.
    Held(Vec<Input<'a>>),
    /// This file no longer holds what its digest says.
    Changed(RootPath),
}

/// The bytes of a blob about to be written, or the input file that changed.
enum Loaded {
    Bytes(Vec<u8>),
    Changed(RootPath),
}

/// A sink that hashes what is written to it.
struct Hashing(Hasher);

impl io::Write for Hashing {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Whether the text of `error` says which call failed, a call made again included: the text of
/// a failure that gave up is that of its last try.
fn names_own_call(error: &RemoteError) -> bool {
    match error {
        RemoteError::Status { .. }
        | RemoteError::TimedOut { .. }
        | RemoteError::Protocol { .. }
        | RemoteError::Blob { .. } => true,
        RemoteError::GaveUp { last, .. } => names_own_call(last),
        _ => false,
    }
}

/// The digest and size of the file at `path`, hashed as it is read, and its permission bits as
/// the open file reports them.
fn digest_file(path: &Path) -> io::Result<(Blob, u32)> {
    let mut file = File::open(path)?;
    let mode = file.metadata()?.permissions().mode();
    let mut sink = Hashing(Hasher::new());
    let size = io::copy(&mut file, &mut sink)?;
    let blob = Blob {
        digest: sink.0.finish(),
        size,
    };
    Ok((blob, mode))
}

/// The action, its command and the input root, encoded.
struct Described {
    command: Encoded,
    tree: InputTree,
    action: Encoded,
}

impl Reapi {
    /// The error for `error`, which came from `call`: an adapter error whose text begins with
    /// the cache's address and names the call.
    ///
    /// A failure that names its own call is not given another. The core puts the name of the
    /// operation in front of the text.
    fn failed(&self, call: Method, error: RemoteError) -> Error {
        let url = self.client.url();
        match error {
            RemoteError::Token(error) => error,
            error if names_own_call(&error) => adapter(format!("{url}: {error}")),
            error => adapter(format!("{url}: {call}: {error}")),
        }
    }

    /// The error for a local `error` that kept `call` from being made: `what` could not be done
    /// to `path`.
    fn io_failure(&self, call: Method, what: &str, path: &RootPath, error: &io::Error) -> Error {
        adapter(format!(
            "{}: {call}: cannot {what} {path}: {error}",
            self.client.url()
        ))
    }

    /// `action`, whose input files were measured as `inputs`, as the messages that name it to
    /// the cache. `call` is the first call the caller goes on to make.
    fn describe(
        &self,
        call: Method,
        action: &RemoteAction,
        inputs: &[Input<'_>],
    ) -> Result<Described> {
        let command = Encoded::of(&encode::command(action));
        let tree = input_tree(inputs).map_err(|error| self.failed(call, error))?;
        let message = encode::action(&command.digest(), &tree.root, action);
        Ok(Described {
            command,
            tree,
            action: Encoded::of(&message),
        })
    }

    /// Hashes each input file as it is read from the workspace.
    fn read_inputs<'a>(&self, files: &'a [RemoteFile]) -> Result<Inputs<'a>> {
        let mut held = Vec::with_capacity(files.len());
        for file in files {
            match digest_file(&self.root.join(file.path.as_str())) {
                Ok((blob, mode)) if blob.digest == file.digest => {
                    held.push(Input::new(file, blob.size, mode));
                }
                Ok(_) => return Ok(Inputs::Changed(file.path.clone())),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Ok(Inputs::Changed(file.path.clone()));
                }
                Err(error) => {
                    return Err(self.io_failure(
                        Method::FindMissingBlobs,
                        "read input file",
                        &file.path,
                        &error,
                    ));
                }
            }
        }
        Ok(Inputs::Held(held))
    }

    /// Writes the blobs of `group` to the cache.
    fn write(&self, group: &[(wire::Digest, Vec<u8>)]) -> Result<()> {
        self.client
            .write_blobs(group)
            .map_err(|error| self.failed(Method::BatchUpdateBlobs, error))
    }

    /// The bytes `source` stands for, as the blob named `digest`.
    fn load(
        &self,
        digest: &wire::Digest,
        source: Source,
        blobs: &dyn BlobSource,
    ) -> Result<Loaded> {
        match source {
            Source::Message(bytes) => Ok(Loaded::Bytes(bytes)),
            Source::Store(held) => blobs.read(&held)?.map(Loaded::Bytes).ok_or_else(|| {
                adapter(format!(
                    "{}: {}: the content store holds no blob {held}",
                    self.client.url(),
                    Method::BatchUpdateBlobs
                ))
            }),
            Source::Input(path) => match fs::read(self.root.join(path.as_str())) {
                Ok(bytes) if Digest::of(&bytes).to_string() == digest.hash => {
                    Ok(Loaded::Bytes(bytes))
                }
                Ok(_) => Ok(Loaded::Changed(path)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Loaded::Changed(path)),
                Err(error) => {
                    Err(self.io_failure(Method::BatchUpdateBlobs, "read input file", &path, &error))
                }
            },
        }
    }
}

impl RemoteCache for Reapi {
    fn lookup(&self, action: &RemoteAction) -> Result<Option<RemoteHit>> {
        let inputs = match measure(&self.root, &action.files) {
            Ok(inputs) => inputs,
            // An input that is gone is a target the cache cannot hold a result for, not a
            // fault of the cache.
            Err(Unreadable { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(Unreadable { path, source }) => {
                return Err(self.io_failure(
                    Method::GetActionResult,
                    "read input file",
                    path,
                    &source,
                ));
            }
        };
        let described = self.describe(Method::GetActionResult, action, &inputs)?;
        let found = self
            .client
            .get_action_result(&described.action.digest())
            .map_err(|error| self.failed(Method::GetActionResult, error))?;
        found
            .map(|result| encode::hit(&result))
            .transpose()
            .map_err(|error| self.failed(Method::GetActionResult, error))
    }

    fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
        if blobs.is_empty() {
            return Ok(Vec::new());
        }
        let digests = blobs
            .iter()
            .map(wire_digest)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| self.failed(Method::BatchReadBlobs, error))?;
        self.client
            .read_blobs(&digests)
            .map_err(|error| self.failed(Method::BatchReadBlobs, error))
    }

    fn decode_tree(&self, bytes: &[u8]) -> Result<Option<Vec<RemoteTreeFile>>> {
        Ok(encode::tree_files(bytes))
    }

    fn upload(
        &self,
        action: &RemoteAction,
        produced: &RemoteUpload,
        blobs: &dyn BlobSource,
    ) -> Result<Uploaded> {
        let inputs = match self.read_inputs(&action.files)? {
            Inputs::Held(inputs) => inputs,
            Inputs::Changed(path) => return Ok(Uploaded::InputChanged(path)),
        };
        let described = self.describe(Method::FindMissingBlobs, action, &inputs)?;
        let messages = encode::action_result(produced)
            .map_err(|error| self.failed(Method::FindMissingBlobs, error))?;

        let mut parcel = Parcel::of(
            &inputs,
            produced,
            messages.trees,
            described.tree.directories,
        )
        .map_err(|error| self.failed(Method::FindMissingBlobs, error))?;
        parcel.add_message(described.command);
        let action_digest = described.action.digest();
        parcel.add_message(described.action);

        let named: Vec<wire::Digest> = parcel.entries.iter().map(|(d, _)| d.clone()).collect();
        let missing: BTreeSet<String> = self
            .client
            .find_missing_blobs(&named)
            .map_err(|error| self.failed(Method::FindMissingBlobs, error))?
            .into_iter()
            .map(|digest| digest.hash)
            .collect();

        let mut group = Vec::new();
        let mut held = 0_usize;
        for (digest, source) in parcel.entries {
            if !missing.contains(&digest.hash) {
                continue;
            }
            let bytes = match self.load(&digest, source, blobs)? {
                Loaded::Bytes(bytes) => bytes,
                Loaded::Changed(path) => return Ok(Uploaded::InputChanged(path)),
            };
            if !group.is_empty() && held.saturating_add(bytes.len()) > self.write_group {
                self.write(&group)?;
                group.clear();
                held = 0;
            }
            held += bytes.len();
            group.push((digest, bytes));
        }
        if !group.is_empty() {
            self.write(&group)?;
        }

        self.client
            .update_action_result(&action_digest, messages.result)
            .map_err(|error| self.failed(Method::UpdateActionResult, error))?;
        Ok(Uploaded::Done)
    }

    fn execute(&self, _action: &RemoteAction) -> Result<Executed> {
        Err(adapter(format!(
            "{}: execution is not available",
            self.client.url()
        )))
    }
}

#[cfg(all(test, feature = "test-server"))]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs;
    use std::net::TcpListener;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    use buildl_core::{
        Argument, Blob, BlobSource, Command, Digest, Directory, Error, FileMode, HostPath, Label,
        OutputName, OutputPath, Platform, PlatformProperties, PlatformPropertyName, PortName,
        RemoteAction, RemoteCache, RemoteContent, RemoteFile, RemoteOutput, RemoteOutputDirectory,
        RemoteOutputFile, RemotePlatform, RemoteSettings, RemoteTree, RemoteTreeFile, RemoteUpload,
        RemoteUrl, Result, RootPath, SourcePath, Timeout, TokenSource, TreePath, Uploaded,
    };

    use super::Reapi;
    use crate::remote::encode::{self, Encoded};
    use crate::remote::input_tree::{InputTree, input_tree, measure};
    use crate::remote::test_server::{Fault, Method, ServerOptions, TestServer};

    const SOURCE: &[u8] = b"int main(void) { return 0; }\n";
    const HEADER: &[u8] = b"#define ONE 1\n";
    const OBJECT: &[u8] = b"object code";
    const LOG: &[u8] = b"compiling\n";

    /// A content store held in memory.
    struct Held(BTreeMap<Digest, Vec<u8>>);

    impl Held {
        fn of(contents: &[&[u8]]) -> Self {
            Self(
                contents
                    .iter()
                    .map(|bytes| (Digest::of(bytes), bytes.to_vec()))
                    .collect(),
            )
        }
    }

    impl BlobSource for Held {
        fn read(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(digest).cloned())
        }
    }

    fn output_path(name: &str) -> OutputPath {
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &Label::parse("//:gen").unwrap(),
            &OutputName::parse(name).unwrap(),
        )
    }

    fn source(path: &str, bytes: &[u8]) -> RemoteFile {
        RemoteFile {
            path: RootPath::from(&SourcePath::parse(path).unwrap()),
            digest: Digest::of(bytes),
            mode: None,
        }
    }

    /// A workspace holding the action's two input files.
    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.c"), SOURCE).unwrap();
        fs::write(dir.path().join("src/one.h"), HEADER).unwrap();
        dir
    }

    fn action() -> RemoteAction {
        RemoteAction {
            salt: Digest::of(b"key"),
            command: Command::new(vec![
                Argument::parse("cc").unwrap(),
                Argument::parse("-c").unwrap(),
            ])
            .unwrap(),
            env: BTreeMap::new(),
            outputs: vec![output_path("obj"), output_path("dir")],
            files: vec![source("src/main.c", SOURCE), source("src/one.h", HEADER)],
            platform: RemotePlatform::of(&Platform::parse("linux/x86_64").unwrap()),
            timeout: Timeout::parse("90s").unwrap(),
            properties: PlatformProperties::default(),
        }
    }

    fn produced() -> RemoteUpload {
        RemoteUpload {
            outputs: vec![
                RemoteOutput {
                    path: output_path("obj"),
                    content: RemoteContent::File {
                        blob: Blob::of(OBJECT),
                        mode: FileMode::Executable,
                    },
                },
                RemoteOutput {
                    path: output_path("dir"),
                    content: RemoteContent::Tree(RemoteTree::new(vec![RemoteTreeFile {
                        path: TreePath::parse("inner.txt").unwrap(),
                        blob: Blob::of(LOG),
                        mode: FileMode::Plain,
                    }])),
                },
            ],
            stdout: Blob::of(LOG),
            stderr: Blob::of(b""),
        }
    }

    fn store() -> Held {
        Held::of(&[OBJECT, LOG, b""])
    }

    fn settings_for(address: std::net::SocketAddr) -> RemoteSettings {
        let url = RemoteUrl::parse(&format!("grpc://{address}")).unwrap();
        RemoteSettings::builder(url)
            .timeout(Timeout::parse("5s").unwrap())
            .retries(buildl_core::RetryCount::from_integer(0).unwrap())
            .build()
            .unwrap()
    }

    fn reapi(server: &TestServer, root: &Path) -> Reapi {
        Reapi::new(&settings_for(server.address()), root).unwrap()
    }

    /// The input tree of `files` as they are on disk now.
    fn tree_of(root: &Path, files: &[RemoteFile]) -> InputTree {
        input_tree(&measure(root, files).unwrap()).unwrap()
    }

    #[test]
    fn a_lookup_of_an_unknown_action_misses() {
        let server = TestServer::start().unwrap();
        let dir = workspace();

        let found = reapi(&server, dir.path()).lookup(&action()).unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn an_uploaded_result_is_found_by_the_same_action() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        let sent = produced();

        let stored = remote.upload(&action(), &sent, &store()).unwrap();
        let found = remote.lookup(&action()).unwrap().unwrap();

        assert_eq!(stored, Uploaded::Done);
        let tree = encode::action_result(&sent).unwrap().trees.remove(0);
        assert_eq!(found.exit_code, 0);
        assert_eq!(
            found.files,
            [RemoteOutputFile {
                path: "out/obj".to_owned(),
                blob: Blob::of(OBJECT),
                mode: FileMode::Executable,
            }]
        );
        assert_eq!(
            found.directories,
            [RemoteOutputDirectory {
                path: "out/dir".to_owned(),
                tree: tree.blob,
            }]
        );
        assert_eq!(found.stdout, Some(Blob::of(LOG)));
        assert_eq!(found.stderr, Some(Blob::of(b"")));
        assert_eq!(found.symlinks, Vec::<String>::new());
        assert_eq!(
            server.blob(&tree.blob.digest).as_deref(),
            Some(tree.bytes.as_slice())
        );
    }

    #[test]
    fn a_result_uploaded_with_properties_is_found_only_with_them() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        let mut pooled = action();
        pooled.properties = PlatformProperties::new(BTreeMap::from([(
            PlatformPropertyName::parse("pool").unwrap(),
            "large".to_owned(),
        )]));

        remote.upload(&pooled, &produced(), &store()).unwrap();

        assert!(remote.lookup(&pooled).unwrap().is_some());
        assert_eq!(remote.lookup(&action()).unwrap(), None);
    }

    #[test]
    fn an_upload_sends_the_action_and_command_before_the_result() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        let described = action();

        remote.upload(&described, &produced(), &store()).unwrap();

        let command = Encoded::of(&encode::command(&described));
        let tree = tree_of(dir.path(), &described.files);
        let message = encode::action(&command.digest(), &tree.root, &described);
        let action_blob = Encoded::of(&message);
        assert_eq!(
            server.blob(&command.blob.digest).as_deref(),
            Some(command.bytes.as_slice())
        );
        assert_eq!(
            server.blob(&action_blob.blob.digest).as_deref(),
            Some(action_blob.bytes.as_slice())
        );
        let calls = server.calls();
        let result = calls
            .iter()
            .position(|call| call.method == Method::UpdateActionResult)
            .unwrap();
        assert_eq!(result, calls.len() - 1, "the result is the last call");
        assert!(
            calls[..result]
                .iter()
                .any(|call| matches!(call.method, Method::BatchUpdateBlobs | Method::Write)),
            "blobs are written before the result"
        );
        assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 1);
    }

    #[test]
    fn an_upload_sends_the_input_files_and_directories() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let described = action();

        reapi(&server, dir.path())
            .upload(&described, &produced(), &store())
            .unwrap();

        for bytes in [SOURCE, HEADER] {
            assert_eq!(
                server.blob(&Digest::of(bytes)).as_deref(),
                Some(bytes),
                "an input file"
            );
        }
        let tree = tree_of(dir.path(), &described.files);
        assert_ne!(tree.directories.len(), 0);
        for (digest, bytes) in &tree.directories {
            let held = server.blob(&Digest::from_hex(&digest.hash).unwrap());
            assert_eq!(held.as_deref(), Some(bytes.as_slice()), "{}", digest.hash);
        }
    }

    #[test]
    fn an_upload_sends_only_what_the_server_lacks() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        remote.upload(&action(), &produced(), &store()).unwrap();
        let writes = |server: &TestServer| {
            server.calls_to(Method::BatchUpdateBlobs).len() + server.calls_to(Method::Write).len()
        };
        let written = writes(&server);
        assert!(written > 0);

        remote.upload(&action(), &produced(), &store()).unwrap();

        assert_eq!(writes(&server), written, "the second upload writes no blob");
        assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 2);
    }

    #[test]
    fn an_input_changed_on_disk_is_reported_and_nothing_is_written() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        let described = action();
        fs::write(dir.path().join("src/one.h"), b"#define ONE 2\n").unwrap();

        let outcome = remote.upload(&described, &produced(), &store()).unwrap();

        assert_eq!(
            outcome,
            Uploaded::InputChanged(RootPath::from(&SourcePath::parse("src/one.h").unwrap()))
        );
        assert_eq!(server.calls_to(Method::BatchUpdateBlobs).len(), 0);
        assert_eq!(server.calls_to(Method::Write).len(), 0);
        assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 0);
    }

    #[test]
    fn an_input_changed_between_the_hash_and_the_send_is_reported() {
        let server = TestServer::start().unwrap();
        server.stall(Method::FindMissingBlobs, Duration::from_millis(600));
        let dir = workspace();
        let remote = reapi(&server, dir.path());
        let header = dir.path().join("src/one.h");

        let outcome = thread::scope(|scope| {
            let upload = scope.spawn(|| remote.upload(&action(), &produced(), &store()));
            // The upload has hashed the file by the time it asks which blobs are missing.
            while server.calls_to(Method::FindMissingBlobs).is_empty() {
                thread::sleep(Duration::from_millis(5));
            }
            fs::write(&header, b"#define ONE 3\n").unwrap();
            upload.join().unwrap().unwrap()
        });

        assert_eq!(
            outcome,
            Uploaded::InputChanged(RootPath::from(&SourcePath::parse("src/one.h").unwrap()))
        );
        assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 0);
    }

    #[test]
    fn an_upload_of_several_write_groups_writes_everything() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let remote = reapi(&server, dir.path()).with_write_group(1);
        let described = action();

        let outcome = remote.upload(&described, &produced(), &store()).unwrap();

        assert_eq!(outcome, Uploaded::Done);
        let writes = server.calls_to(Method::BatchUpdateBlobs).len();
        assert!(writes > 1, "one write for each blob, got {writes}");
        for bytes in [SOURCE, HEADER, OBJECT, LOG, b"".as_slice()] {
            assert_eq!(
                server.blob(&Digest::of(bytes)).as_deref(),
                Some(bytes),
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
        let tree = encode::action_result(&produced()).unwrap().trees.remove(0);
        assert_eq!(
            server.blob(&tree.blob.digest).as_deref(),
            Some(tree.bytes.as_slice())
        );
        for (digest, bytes) in &tree_of(dir.path(), &described.files).directories {
            let held = server.blob(&Digest::from_hex(&digest.hash).unwrap());
            assert_eq!(held.as_deref(), Some(bytes.as_slice()));
        }
        assert!(remote.lookup(&described).unwrap().is_some());
    }

    #[test]
    fn a_lookup_with_an_input_gone_from_disk_misses_and_asks_nothing() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        fs::remove_file(dir.path().join("src/one.h")).unwrap();

        let found = reapi(&server, dir.path()).lookup(&action()).unwrap();

        assert_eq!(found, None);
        assert_eq!(server.calls_to(Method::GetActionResult).len(), 0);
    }

    #[test]
    fn an_input_that_cannot_be_read_at_lookup_names_the_url_and_the_call() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        // A path through a regular file fails with "not a directory", which is not "not found".
        let mut broken = action();
        broken.files = vec![RemoteFile {
            path: RootPath::from(&SourcePath::parse("src/main.c/inner.c").unwrap()),
            digest: Digest::of(SOURCE),
            mode: None,
        }];

        let error = reapi(&server, dir.path()).lookup(&broken).unwrap_err();

        let text = error.to_string();
        assert!(text.contains(&server.address().to_string()), "{text}");
        assert!(text.contains("GetActionResult"), "{text}");
        assert!(text.contains("src/main.c/inner.c"), "{text}");
    }

    #[test]
    fn a_write_that_streams_names_the_stream_and_not_the_batch() {
        let server = TestServer::start_with(ServerOptions {
            max_batch_total_size_bytes: 10,
            ..ServerOptions::default()
        })
        .unwrap();
        server.fail(Method::Write, Fault::Internal);
        let dir = workspace();

        let error = reapi(&server, dir.path())
            .upload(&action(), &produced(), &store())
            .unwrap_err();

        let text = error.to_string();
        assert!(text.contains("ByteStream.Write"), "{text}");
        assert!(!text.contains("BatchUpdateBlobs"), "{text}");
        assert!(text.contains(&server.address().to_string()), "{text}");
    }

    #[test]
    fn the_debug_text_of_an_adapter_leaves_the_token_out() {
        let dir = workspace();
        fs::write(dir.path().join("token"), "s3cret-token").unwrap();
        let url = RemoteUrl::parse("grpcs://127.0.0.1:1").unwrap();
        let settings = RemoteSettings::builder(url)
            .token(TokenSource::File(HostPath::parse("token").unwrap()))
            .build()
            .unwrap();

        let shown = format!("{:?}", Reapi::new(&settings, dir.path()).unwrap());

        assert!(!shown.contains("s3cret-token"), "{shown}");
        assert!(shown.contains("127.0.0.1:1"), "{shown}");
    }

    #[test]
    fn a_download_returns_blobs_in_the_order_asked() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let mut contents: Vec<&[u8]> = vec![b"first blob", b"second blob", b"third blob"];
        contents.sort_by_key(|bytes| std::cmp::Reverse(Digest::of(bytes)));
        for bytes in &contents {
            let _ = server.put_blob(bytes);
        }
        let blobs: Vec<Blob> = contents.iter().map(|bytes| Blob::of(bytes)).collect();

        let read = reapi(&server, dir.path()).download(&blobs).unwrap();

        let expected: Vec<Vec<u8>> = contents.iter().map(|bytes| bytes.to_vec()).collect();
        assert_eq!(read, expected);
    }

    #[test]
    fn a_server_error_names_the_url_and_the_call() {
        let server = TestServer::start().unwrap();
        server.fail(Method::GetActionResult, Fault::Unavailable);
        let dir = workspace();

        let error = reapi(&server, dir.path()).lookup(&action()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("expected an adapter error, got {error:?}");
        };
        assert_eq!(port, PortName::RemoteCache);
        let text = diagnostic.to_string();
        assert!(text.contains(&server.address().to_string()), "{text}");
        assert!(text.contains("GetActionResult"), "{text}");
    }

    #[test]
    fn a_call_given_up_after_retries_is_named_once() {
        let server = TestServer::start().unwrap();
        server.fail(Method::GetActionResult, Fault::Unavailable);
        let dir = workspace();
        let url = RemoteUrl::parse(&format!("grpc://{}", server.address())).unwrap();
        let settings = RemoteSettings::builder(url)
            .timeout(Timeout::parse("5s").unwrap())
            .retries(buildl_core::RetryCount::from_integer(1).unwrap())
            .build()
            .unwrap();

        let error = Reapi::new(&settings, dir.path())
            .unwrap()
            .lookup(&action())
            .unwrap_err();

        let Error::Adapter { diagnostic, .. } = error else {
            unreachable!("expected an adapter error, got {error:?}");
        };
        let text = diagnostic.to_string();
        assert_eq!(text.matches("GetActionResult").count(), 1, "{text}");
        assert!(text.contains("after 2 calls"), "{text}");
    }

    #[test]
    fn a_cache_that_is_down_fails_the_first_call_and_not_the_adapter() {
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let dir = workspace();
        let remote = Reapi::new(&settings_for(address), dir.path()).unwrap();

        let error = remote.lookup(&action()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("expected an adapter error, got {error:?}");
        };
        assert_eq!(port, PortName::RemoteCache);
        let text = diagnostic.to_string();
        assert_eq!(text.matches(&address.to_string()).count(), 1, "{text}");
        assert!(text.contains("GetActionResult: cannot connect"), "{text}");
    }

    #[test]
    fn a_tree_decodes_to_its_files() {
        let server = TestServer::start().unwrap();
        let dir = workspace();
        let tree = encode::action_result(&produced()).unwrap().trees.remove(0);

        let files = reapi(&server, dir.path()).decode_tree(&tree.bytes).unwrap();

        assert_eq!(
            files,
            Some(vec![RemoteTreeFile {
                path: TreePath::parse("inner.txt").unwrap(),
                blob: Blob::of(LOG),
                mode: FileMode::Plain,
            }])
        );
    }
}
