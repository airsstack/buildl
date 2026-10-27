//! Running one action from a blob store, in a temporary directory.
//!
//! [`prepare`] reads an `Action`, its `Command` and its input root out of a store of blobs and
//! names every blob the store lacks, so the test server can refuse an action it cannot run as a
//! real server would. [`Job::run`] then writes the input root into a fresh temporary directory,
//! runs the command there with its own environment variables and nothing of the host's, and
//! reads back the logs and each output the command listed.
//!
//! Responsibilities: resolving an action's blobs, laying out its input root, running the
//! process under a time limit, and encoding what it left behind as an `ActionResult` and the
//! blobs that result names.
//!
//! Non-responsibilities: the gRPC service, the operations a client waits on, and keeping the
//! result or its blobs; the test server does all three.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use buildl_core::Digest as BlobDigest;
use prost::{Message, Name};

use super::proto::build::bazel::remote::execution::v2::{
    Action, ActionResult, Command, Digest, Directory, DirectoryNode, FileNode, OutputDirectory,
    OutputFile, OutputSymlink, SymlinkNode, Tree,
};

/// The blobs a store holds, by lowercase hexadecimal hash.
pub(super) type Blobs = BTreeMap<String, Vec<u8>>;

/// How long the process is left between two looks at whether it has ended.
const POLL: Duration = Duration::from_millis(5);

/// The exit code reported for a process that did not exit by itself, or could not be started.
const NO_EXIT: i32 = -1;

/// The protocol's digest of `bytes`.
fn wire_digest(bytes: &[u8]) -> Digest {
    Digest {
        hash: BlobDigest::of(bytes).to_string(),
        size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
    }
}

/// Why an action cannot be run.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Blobs the store lacks, once each, in hash order.
    Missing(Vec<Digest>),
    /// A message that cannot be run as it is: it does not decode, or it names a path that would
    /// leave the directory it is laid out in.
    Malformed(String),
}

/// The blobs an action needs that a store lacks, once each.
#[derive(Default)]
struct Absent(BTreeMap<String, i64>);

impl Absent {
    fn note(&mut self, digest: &Digest) {
        self.0.insert(digest.hash.clone(), digest.size_bytes);
    }

    fn into_digests(self) -> Vec<Digest> {
        self.0
            .into_iter()
            .map(|(hash, size_bytes)| Digest { hash, size_bytes })
            .collect()
    }
}

/// An action whose every blob is in hand, ready to run.
#[derive(Debug)]
pub(super) struct Job {
    command: Command,
    /// The paths to read back, relative to the working directory, each once, in order.
    outputs: Vec<String>,
    /// The `Directory` messages of the input root, by hash.
    directories: BTreeMap<String, Directory>,
    /// The bytes of each input file, by hash.
    files: Blobs,
    /// The hash of the root `Directory`; `None` for an action without inputs.
    root: Option<String>,
    /// The longest the process may run. Only the process itself is killed at the limit, not
    /// what it started, so a command meant to outlast it should `exec` its last program.
    timeout: Option<Duration>,
    /// Whether the action asks that its result not be kept.
    do_not_cache: bool,
}

/// What a run left behind.
#[derive(Debug)]
pub(super) struct Execution {
    /// The result to answer with.
    pub(super) result: ActionResult,
    /// The bytes of every blob the result names, for the store to keep.
    pub(super) blobs: Vec<Vec<u8>>,
    /// Whether the process was killed for running past its limit.
    pub(super) timed_out: bool,
}

impl Job {
    /// Whether the action asks that its result not be kept.
    pub(super) const fn do_not_cache(&self) -> bool {
        self.do_not_cache
    }

    /// Runs the command in a fresh temporary directory, which is removed afterwards.
    ///
    /// A command that runs past its limit is killed and the run is reported as timed out. Only
    /// the direct child is killed: a program that command started outlives it, so a command
    /// meant to time out should `exec` its last program rather than wait for it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the directory, a file of it or a log cannot be
    /// written or read. A command that cannot be started is not an error: it ends with exit code
    /// `-1` and the reason on its standard error.
    pub(super) fn run(&self) -> io::Result<Execution> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path().join("root");
        fs::create_dir(&root)?;
        self.write_inputs(&root)?;
        let working = root.join(&self.command.working_directory);
        fs::create_dir_all(&working)?;
        for output in &self.outputs {
            if let Some(parent) = working.join(output).parent() {
                fs::create_dir_all(parent)?;
            }
        }

        let stdout_path = sandbox.path().join("stdout");
        let stderr_path = sandbox.path().join("stderr");
        let (exit_code, timed_out) = self.spawn(&working, &stdout_path, &stderr_path)?;
        let stdout = fs::read(&stdout_path)?;
        let stderr = fs::read(&stderr_path)?;

        let mut result = ActionResult {
            exit_code,
            stdout_digest: Some(wire_digest(&stdout)),
            stderr_digest: Some(wire_digest(&stderr)),
            ..ActionResult::default()
        };
        let mut blobs = vec![stdout, stderr];
        if !timed_out {
            let collected = collect_outputs(&working, &self.outputs)?;
            result.output_files = collected.files;
            result.output_symlinks = collected.symlinks;
            result.output_directories = collected.directories;
            blobs.extend(collected.blobs);
        }
        Ok(Execution {
            result,
            blobs,
            timed_out,
        })
    }

    /// Runs the process with its logs in the two files, and returns its exit code and whether it
    /// was killed for running past its limit.
    fn spawn(
        &self,
        working: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> io::Result<(i32, bool)> {
        let program = self.command.arguments.first().map_or("", String::as_str);
        let mut process = Process::new(program);
        process
            .args(self.command.arguments.iter().skip(1))
            .current_dir(working)
            .env_clear()
            .envs(
                self.command
                    .environment_variables
                    .iter()
                    .map(|variable| (&variable.name, &variable.value)),
            )
            .stdin(Stdio::null())
            .stdout(fs::File::create(stdout_path)?)
            .stderr(fs::File::create(stderr_path)?);
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                fs::write(stderr_path, format!("cannot run {program}: {error}\n"))?;
                return Ok((NO_EXIT, false));
            }
        };

        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok((status.code().unwrap_or(NO_EXIT), false));
            }
            if self.timeout.is_some_and(|limit| started.elapsed() >= limit) {
                // The process may have ended since the look above; either way it is gone.
                let _ = child.kill();
                let _ = child.wait();
                return Ok((NO_EXIT, true));
            }
            std::thread::sleep(POLL);
        }
    }

    /// Writes the input root under `root`, which exists.
    fn write_inputs(&self, root: &Path) -> io::Result<()> {
        let Some(hash) = &self.root else {
            return Ok(());
        };
        self.write_directory(hash, root)
    }

    fn write_directory(&self, hash: &str, path: &Path) -> io::Result<()> {
        let directory = self
            .directories
            .get(hash)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("directory {hash}")))?;
        fs::create_dir_all(path)?;
        for file in &directory.files {
            let hash = file
                .digest
                .as_ref()
                .map_or("", |digest| digest.hash.as_str());
            let bytes = self
                .files
                .get(hash)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("file {hash}")))?;
            let target = path.join(&file.name);
            fs::write(&target, bytes)?;
            let mode = if file.is_executable { 0o755 } else { 0o644 };
            fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        }
        for link in &directory.symlinks {
            symlink(&link.target, path.join(&link.name))?;
        }
        for node in &directory.directories {
            let hash = node
                .digest
                .as_ref()
                .map_or("", |digest| digest.hash.as_str());
            self.write_directory(hash, &path.join(&node.name))?;
        }
        Ok(())
    }
}

/// Reads `action` and everything it needs out of `blobs`.
///
/// # Errors
///
/// Returns [`Refusal::Missing`] naming every blob the store lacks (the action itself, or its
/// command, its directories and its files), and [`Refusal::Malformed`] for a message that cannot
/// be run.
pub(super) fn prepare(blobs: &Blobs, action: &Digest) -> Result<Job, Refusal> {
    let mut absent = Absent::default();
    let Some(action_message) = fetch::<Action>(blobs, action, &mut absent)? else {
        return Err(Refusal::Missing(absent.into_digests()));
    };
    let command_digest = action_message
        .command_digest
        .as_ref()
        .ok_or_else(|| Refusal::Malformed("the action names no command".to_owned()))?;
    let command = fetch::<Command>(blobs, command_digest, &mut absent)?;

    let mut directories = BTreeMap::new();
    let mut files = Blobs::new();
    let root = match &action_message.input_root_digest {
        Some(digest) => {
            gather(blobs, digest, &mut absent, &mut directories, &mut files)?;
            Some(digest.hash.clone())
        }
        None => None,
    };

    let command = match command {
        Some(command) if absent.0.is_empty() => command,
        _ => return Err(Refusal::Missing(absent.into_digests())),
    };
    let outputs = outputs_of(&command)?;
    if command.arguments.is_empty() {
        return Err(Refusal::Malformed(
            "the command has no arguments".to_owned(),
        ));
    }
    if !is_relative_directory(&command.working_directory) {
        return Err(Refusal::Malformed(format!(
            "the working directory {:?} leaves the input root",
            command.working_directory
        )));
    }
    Ok(Job {
        command,
        outputs,
        directories,
        files,
        root,
        timeout: action_message.timeout.as_ref().and_then(limit_of),
        do_not_cache: action_message.do_not_cache,
    })
}

/// `message` decoded from the blob `digest` names, or `None` after noting the blob as absent.
fn fetch<M: Message + Default + Name>(
    blobs: &Blobs,
    digest: &Digest,
    absent: &mut Absent,
) -> Result<Option<M>, Refusal> {
    let Some(bytes) = blobs.get(&digest.hash) else {
        absent.note(digest);
        return Ok(None);
    };
    M::decode(bytes.as_slice()).map(Some).map_err(|error| {
        Refusal::Malformed(format!(
            "blob {} is not a {}: {error}",
            digest.hash,
            M::NAME
        ))
    })
}

/// Collects the `Directory` messages under `digest` and the bytes of their files, noting every
/// blob that is not in `blobs`.
fn gather(
    blobs: &Blobs,
    digest: &Digest,
    absent: &mut Absent,
    directories: &mut BTreeMap<String, Directory>,
    files: &mut Blobs,
) -> Result<(), Refusal> {
    if directories.contains_key(&digest.hash) {
        return Ok(());
    }
    let Some(directory) = fetch::<Directory>(blobs, digest, absent)? else {
        return Ok(());
    };
    for file in &directory.files {
        check_name(&file.name)?;
        let digest = file
            .digest
            .as_ref()
            .ok_or_else(|| Refusal::Malformed(format!("file {:?} has no digest", file.name)))?;
        match blobs.get(&digest.hash) {
            Some(bytes) => {
                files.insert(digest.hash.clone(), bytes.clone());
            }
            None => absent.note(digest),
        }
    }
    for link in &directory.symlinks {
        check_name(&link.name)?;
    }
    for node in &directory.directories {
        check_name(&node.name)?;
        let child = node.digest.as_ref().ok_or_else(|| {
            Refusal::Malformed(format!("directory {:?} has no digest", node.name))
        })?;
        gather(blobs, child, absent, directories, files)?;
    }
    directories.insert(digest.hash.clone(), directory);
    Ok(())
}

/// The output paths of `command`, each once and in order: `output_paths`, or the older
/// `output_files` and `output_directories` when it is empty.
#[expect(
    deprecated,
    reason = "a v2.0 client lists outputs only in these fields"
)]
fn outputs_of(command: &Command) -> Result<Vec<String>, Refusal> {
    let mut outputs: Vec<String> = if command.output_paths.is_empty() {
        command
            .output_files
            .iter()
            .chain(&command.output_directories)
            .cloned()
            .collect()
    } else {
        command.output_paths.clone()
    };
    if let Some(path) = outputs.iter().find(|path| !is_relative_path(path)) {
        return Err(Refusal::Malformed(format!(
            "the output path {path:?} leaves the working directory"
        )));
    }
    outputs.sort();
    outputs.dedup();
    Ok(outputs)
}

/// How long `duration` allows, or `None` when it sets no limit.
fn limit_of(duration: &prost_types::Duration) -> Option<Duration> {
    let seconds = u64::try_from(duration.seconds).ok()?;
    let nanos = u32::try_from(duration.nanos).ok()?;
    let limit = Duration::new(seconds, nanos);
    (!limit.is_zero()).then_some(limit)
}

/// Whether `name` is one ordinary component of a path.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

fn check_name(name: &str) -> Result<(), Refusal> {
    if is_name(name) {
        Ok(())
    } else {
        Err(Refusal::Malformed(format!("{name:?} is not a file name")))
    }
}

/// Whether `path` is one or more ordinary components, so that it stays inside the directory it
/// is joined to. An empty path is not.
fn is_relative_path(path: &str) -> bool {
    path.split('/').all(is_name)
}

/// Whether `path` names a directory inside the one it is joined to: the empty path, which is
/// that directory itself, or a relative path.
fn is_relative_directory(path: &str) -> bool {
    path.is_empty() || is_relative_path(path)
}

/// What a command left at its output paths.
#[derive(Debug, Default)]
struct Collected {
    files: Vec<OutputFile>,
    symlinks: Vec<OutputSymlink>,
    directories: Vec<OutputDirectory>,
    /// The bytes of every file and `Tree` the outputs name.
    blobs: Vec<Vec<u8>>,
}

/// Reads each of `paths`, which are sorted, relative to `working`. A path with nothing at it is
/// left out.
fn collect_outputs(working: &Path, paths: &[String]) -> io::Result<Collected> {
    let mut collected = Collected::default();
    for path in paths {
        let full = working.join(path);
        let metadata = match fs::symlink_metadata(&full) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if metadata.is_symlink() {
            collected.symlinks.push(OutputSymlink {
                path: path.clone(),
                target: fs::read_link(&full)?.to_string_lossy().into_owned(),
                ..OutputSymlink::default()
            });
        } else if metadata.is_dir() {
            let mut children = Vec::new();
            let root = directory_message(&full, &mut children, &mut collected.blobs)?;
            let tree = Tree {
                root: Some(root),
                children,
            }
            .encode_to_vec();
            collected.directories.push(OutputDirectory {
                path: path.clone(),
                tree_digest: Some(wire_digest(&tree)),
                ..OutputDirectory::default()
            });
            collected.blobs.push(tree);
        } else {
            let bytes = fs::read(&full)?;
            collected.files.push(OutputFile {
                path: path.clone(),
                digest: Some(wire_digest(&bytes)),
                is_executable: metadata.permissions().mode() & 0o111 != 0,
                ..OutputFile::default()
            });
            collected.blobs.push(bytes);
        }
    }
    Ok(collected)
}

/// The `Directory` message of the directory at `path`, with its entries sorted by name. Every
/// directory below it is pushed onto `children` and the bytes of every file onto `blobs`.
fn directory_message(
    path: &Path,
    children: &mut Vec<Directory>,
    blobs: &mut Vec<Vec<u8>>,
) -> io::Result<Directory> {
    let mut entries = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    let mut directory = Directory::default();
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_symlink() {
            directory.symlinks.push(SymlinkNode {
                name,
                target: fs::read_link(entry.path())?.to_string_lossy().into_owned(),
                node_properties: None,
            });
        } else if metadata.is_dir() {
            let child = directory_message(&entry.path(), children, blobs)?;
            directory.directories.push(DirectoryNode {
                name,
                digest: Some(wire_digest(&child.encode_to_vec())),
            });
            children.push(child);
        } else {
            let bytes = fs::read(entry.path())?;
            directory.files.push(FileNode {
                name,
                digest: Some(wire_digest(&bytes)),
                is_executable: metadata.permissions().mode() & 0o111 != 0,
                node_properties: None,
            });
            blobs.push(bytes);
        }
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    /// Stores `bytes` in `blobs` and returns its digest.
    fn put(blobs: &mut Blobs, bytes: &[u8]) -> Digest {
        let digest = wire_digest(bytes);
        blobs.insert(digest.hash.clone(), bytes.to_vec());
        digest
    }

    fn put_message(blobs: &mut Blobs, message: &impl Message) -> Digest {
        put(blobs, &message.encode_to_vec())
    }

    fn file(name: &str, digest: Digest, is_executable: bool) -> FileNode {
        FileNode {
            name: name.to_owned(),
            digest: Some(digest),
            is_executable,
            node_properties: None,
        }
    }

    fn sh(script: &str) -> Command {
        Command {
            arguments: vec!["/bin/sh".to_owned(), "-c".to_owned(), script.to_owned()],
            ..Command::default()
        }
    }

    /// The action for `command` over `root`, stored with them in `blobs`.
    fn action(blobs: &mut Blobs, command: &Command, root: Option<Digest>) -> Digest {
        let command_digest = put_message(blobs, command);
        put_message(
            blobs,
            &Action {
                command_digest: Some(command_digest),
                input_root_digest: root,
                ..Action::default()
            },
        )
    }

    /// The action for `command` over the empty input root.
    fn job_for(command: &Command) -> Job {
        let mut blobs = Blobs::new();
        let action = action(&mut blobs, command, None);
        prepare(&blobs, &action).unwrap()
    }

    #[test]
    fn every_missing_blob_is_listed_once_in_hash_order() {
        let mut blobs = Blobs::new();
        let shared = wire_digest(b"shared");
        let other = wire_digest(b"other");
        let command = wire_digest(&sh("true").encode_to_vec());
        let root = put_message(
            &mut blobs,
            &Directory {
                files: vec![
                    file("a", shared.clone(), false),
                    file("b", shared.clone(), false),
                    file("c", other.clone(), false),
                ],
                ..Directory::default()
            },
        );
        let action = put_message(
            &mut blobs,
            &Action {
                command_digest: Some(command.clone()),
                input_root_digest: Some(root),
                ..Action::default()
            },
        );

        let Err(Refusal::Missing(missing)) = prepare(&blobs, &action) else {
            panic!("the action cannot be run");
        };

        let mut expected = vec![shared, other, command];
        expected.sort_by(|left, right| left.hash.cmp(&right.hash));
        assert_eq!(missing, expected);
    }

    #[test]
    fn a_missing_action_is_the_only_blob_listed() {
        let action = wire_digest(b"no such action");
        let Err(refusal) = prepare(&Blobs::new(), &action) else {
            panic!("the action cannot be run");
        };
        assert_eq!(refusal, Refusal::Missing(vec![action]));
    }

    #[test]
    fn a_name_that_leaves_the_directory_is_malformed() {
        for name in ["..", "a/b", "", "."] {
            let mut blobs = Blobs::new();
            let data = put(&mut blobs, b"x");
            let root = put_message(
                &mut blobs,
                &Directory {
                    files: vec![file(name, data, false)],
                    ..Directory::default()
                },
            );
            let action = action(&mut blobs, &sh("true"), Some(root));
            assert!(
                matches!(prepare(&blobs, &action), Err(Refusal::Malformed(_))),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_command_that_cannot_be_laid_out_is_malformed() {
        let mut no_arguments = sh("true");
        no_arguments.arguments.clear();
        let mut escaping_directory = sh("true");
        escaping_directory.working_directory = "../up".to_owned();
        let mut escaping_output = sh("true");
        escaping_output.output_paths = vec!["/etc/passwd".to_owned()];

        for command in [no_arguments, escaping_directory, escaping_output] {
            let mut blobs = Blobs::new();
            let action = action(&mut blobs, &command, None);
            assert!(matches!(
                prepare(&blobs, &action),
                Err(Refusal::Malformed(_))
            ));
        }
    }

    #[test]
    #[expect(
        deprecated,
        reason = "the test builds a command the way a v2.0 client does"
    )]
    fn the_older_output_lists_are_used_when_output_paths_is_empty() {
        let mut command = sh("true");
        command.output_files = vec!["b".to_owned(), "a".to_owned()];
        command.output_directories = vec!["d".to_owned(), "a".to_owned()];
        assert_eq!(outputs_of(&command).unwrap(), ["a", "b", "d"]);
    }

    #[test]
    fn a_duration_of_zero_or_less_sets_no_limit() {
        let duration = |seconds, nanos| prost_types::Duration { seconds, nanos };
        assert_eq!(limit_of(&duration(0, 0)), None);
        assert_eq!(limit_of(&duration(-1, 0)), None);
        assert_eq!(
            limit_of(&duration(2, 500_000_000)),
            Some(Duration::from_millis(2500))
        );
    }

    #[test]
    fn the_input_root_is_written_with_its_modes_directories_and_symlinks() {
        let mut blobs = Blobs::new();
        let plain = put(&mut blobs, b"plain");
        let tool = put(&mut blobs, b"#!/bin/sh");
        let inner = put_message(
            &mut blobs,
            &Directory {
                files: vec![file("tool", tool, true)],
                ..Directory::default()
            },
        );
        let root = put_message(
            &mut blobs,
            &Directory {
                files: vec![file("plain", plain, false)],
                directories: vec![DirectoryNode {
                    name: "bin".to_owned(),
                    digest: Some(inner),
                }],
                symlinks: vec![SymlinkNode {
                    name: "link".to_owned(),
                    target: "plain".to_owned(),
                    node_properties: None,
                }],
                node_properties: None,
            },
        );
        let action = action(&mut blobs, &sh("true"), Some(root));
        let job = prepare(&blobs, &action).unwrap();
        let dir = tempfile::tempdir().unwrap();

        job.write_inputs(dir.path()).unwrap();

        assert_eq!(fs::read(dir.path().join("plain")).unwrap(), b"plain");
        let tool = dir.path().join("bin/tool");
        assert_eq!(fs::read(&tool).unwrap(), b"#!/bin/sh");
        assert_eq!(
            fs::metadata(&tool).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(dir.path().join("plain"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert_eq!(
            fs::read_link(dir.path().join("link")).unwrap().to_str(),
            Some("plain")
        );
    }

    #[test]
    fn outputs_are_files_symlinks_and_directories_and_an_absent_path_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(path.join("a.txt"), b"alpha").unwrap();
        fs::write(path.join("run"), b"#!/bin/sh").unwrap();
        fs::set_permissions(path.join("run"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("a.txt", path.join("link")).unwrap();
        fs::create_dir_all(path.join("out/sub")).unwrap();
        fs::write(path.join("out/f"), b"f").unwrap();
        fs::write(path.join("out/sub/g"), b"g").unwrap();
        let paths: Vec<String> = ["a.txt", "gone", "link", "out", "run"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();

        let collected = collect_outputs(path, &paths).unwrap();

        let files: Vec<_> = collected
            .files
            .iter()
            .map(|each| (each.path.as_str(), each.is_executable))
            .collect();
        assert_eq!(files, [("a.txt", false), ("run", true)]);
        assert_eq!(collected.files[0].digest, Some(wire_digest(b"alpha")));
        assert_eq!(collected.symlinks.len(), 1);
        assert_eq!(collected.symlinks[0].path, "link");
        assert_eq!(collected.symlinks[0].target, "a.txt");
        assert_eq!(collected.directories.len(), 1);
        assert_eq!(collected.directories[0].path, "out");

        let tree_digest = collected.directories[0].tree_digest.clone().unwrap();
        let tree_bytes = collected
            .blobs
            .iter()
            .find(|bytes| wire_digest(bytes) == tree_digest)
            .unwrap();
        let tree = Tree::decode(tree_bytes.as_slice()).unwrap();
        let root = tree.root.unwrap();
        assert_eq!(root.files[0].name, "f");
        assert_eq!(root.directories[0].name, "sub");
        assert_eq!(tree.children.len(), 1);
        assert_eq!(
            root.directories[0].digest,
            Some(wire_digest(&tree.children[0].encode_to_vec()))
        );
        assert_eq!(tree.children[0].files[0].name, "g");
        assert!(collected.blobs.contains(&b"g".to_vec()));
    }

    #[test]
    fn a_run_reports_its_exit_code_logs_and_outputs() {
        let mut command = sh("printf out; printf err >&2; printf data > o; exit 4");
        command.output_paths = vec!["o".to_owned()];

        let execution = job_for(&command).run().unwrap();

        assert!(!execution.timed_out);
        assert_eq!(execution.result.exit_code, 4);
        assert_eq!(execution.result.stdout_digest, Some(wire_digest(b"out")));
        assert_eq!(execution.result.stderr_digest, Some(wire_digest(b"err")));
        assert_eq!(execution.result.output_files[0].path, "o");
        assert!(execution.blobs.contains(&b"data".to_vec()));
    }

    #[test]
    fn a_run_works_in_the_working_directory() {
        let mut command = sh("printf here > o");
        command.working_directory = "a/b".to_owned();
        command.output_paths = vec!["o".to_owned()];

        let execution = job_for(&command).run().unwrap();

        assert_eq!(execution.result.output_files[0].path, "o");
        assert_eq!(
            execution.result.output_files[0].digest,
            Some(wire_digest(b"here"))
        );
    }

    #[test]
    fn the_directories_leading_to_an_output_are_created() {
        let mut command = sh("printf deep > sub/o");
        command.output_paths = vec!["sub/o".to_owned()];

        let execution = job_for(&command).run().unwrap();

        assert_eq!(execution.result.exit_code, 0);
        assert_eq!(execution.result.output_files[0].path, "sub/o");
        assert_eq!(
            execution.result.output_files[0].digest,
            Some(wire_digest(b"deep"))
        );
    }

    #[test]
    fn a_process_past_its_limit_is_killed_and_its_logs_kept() {
        let mut blobs = Blobs::new();
        let command = put_message(&mut blobs, &sh("printf early; exec sleep 30"));
        let action = put_message(
            &mut blobs,
            &Action {
                command_digest: Some(command),
                timeout: Some(prost_types::Duration {
                    seconds: 3,
                    nanos: 0,
                }),
                ..Action::default()
            },
        );

        let execution = prepare(&blobs, &action).unwrap().run().unwrap();

        assert!(execution.timed_out);
        assert_eq!(execution.result.stdout_digest, Some(wire_digest(b"early")));
        assert_eq!(execution.result.output_files, []);
    }

    #[test]
    fn a_program_that_cannot_start_ends_with_the_reason_on_its_error_output() {
        let mut command = sh("true");
        command.arguments = vec!["/no/such/program".to_owned()];

        let execution = job_for(&command).run().unwrap();

        assert_eq!(execution.result.exit_code, NO_EXIT);
        let stderr = &execution.blobs[1];
        assert!(String::from_utf8_lossy(stderr).contains("/no/such/program"));
    }

    #[test]
    fn the_action_decides_whether_its_result_is_kept() {
        let mut blobs = Blobs::new();
        let command = put_message(&mut blobs, &sh("true"));
        let action = put_message(
            &mut blobs,
            &Action {
                command_digest: Some(command),
                do_not_cache: true,
                ..Action::default()
            },
        );
        assert!(prepare(&blobs, &action).unwrap().do_not_cache());
        assert!(!job_for(&sh("true")).do_not_cache());
    }
}
