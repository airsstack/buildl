//! Workspaces written for one test, and the binary run in them.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::iter;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A temporary workspace holding each of `files`, and its canonical root.
pub(crate) fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    for (path, text) in files {
        write(&root, path, text);
    }
    (dir, root)
}

/// Writes `text` to `relative` under `root`, making its directory.
pub(crate) fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Writes `text` to `relative` under `root` as a program anyone may run.
pub(crate) fn program(root: &Path, relative: &str, text: &str) {
    write(root, relative, text);
    fs::set_permissions(root.join(relative), fs::Permissions::from_mode(0o755)).unwrap();
}

/// Adds `bytes` to the end of `relative` under `root`.
pub(crate) fn append(root: &Path, relative: &str, bytes: &str) {
    let path = root.join(relative);
    let mut text = fs::read(&path).unwrap();
    text.extend_from_slice(bytes.as_bytes());
    fs::write(path, text).unwrap();
}

/// A manifest that allows `programs`, every read, and the two writes a build makes, and runs
/// one action at a time with nothing between the action and the machine.
pub(crate) fn manifest(programs: &[&str]) -> String {
    manifest_with(programs, "strategy = \"bare\"\n")
}

/// A manifest like [`manifest`] that names no strategy, so every action runs in the input
/// sandbox.
pub(crate) fn sandboxed_manifest(programs: &[&str]) -> String {
    manifest_with(programs, "")
}

/// A manifest like [`manifest`] whose `[execution]` table holds `lines` after `workers = 1`.
pub(crate) fn manifest_with(programs: &[&str], lines: &str) -> String {
    let programs: Vec<String> = programs.iter().map(|name| format!("{name:?}")).collect();
    format!(
        "[ceiling]\nproc.run = [{}]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\n{lines}",
        programs.join(", ")
    )
}

/// The search path of this test process with `first` put before it.
pub(crate) fn path_with(first: &Path) -> OsString {
    let rest = env::var_os("PATH").unwrap_or_default();
    let directories = iter::once(first.to_path_buf()).chain(env::split_paths(&rest));
    env::join_paths(directories).unwrap()
}

/// The binary run with `args` in `start`, its environment cleared down to the search path of
/// this process and `variables`.
fn command(start: &Path, args: &[&str], variables: &[(&str, &OsStr)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_buildl"));
    command.args(args).current_dir(start).env_clear();
    if let Some(path) = env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.envs(variables.iter().copied());
    command
}

/// Runs the binary with `args` in `start`: stdout and stderr captured, stdin empty.
///
/// The process gets the search path of this one and the variables of `variables`, and nothing
/// else of this process's environment, so what a test does not set is unset.
pub(crate) fn run(start: &Path, args: &[&str], variables: &[(&str, &OsStr)]) -> Output {
    command(start, args, variables).output().unwrap()
}

/// Where a process whose home is `home` keeps what it remembers across workspaces.
pub(crate) fn data_home(home: &Path) -> PathBuf {
    home.join(".local/share")
}

/// Runs the binary like [`run`], with `home` as its `HOME` and [`data_home`] of it as its
/// `XDG_DATA_HOME`, and its standard input a pipe that holds nothing.
///
/// Both are set on the child alone, so a build that remembers an approval writes under `home`
/// and never under the real one. `home` must be an absolute path.
pub(crate) fn run_at_home(start: &Path, args: &[&str], home: &Path) -> Output {
    assert!(home.is_absolute(), "{}", home.display());
    let mut command = command(
        start,
        args,
        &[
            ("HOME", home.as_os_str()),
            ("XDG_DATA_HOME", data_home(home).as_os_str()),
        ],
    );
    command.stdin(Stdio::piped());
    command.output().unwrap()
}

/// The status the process ended with.
pub(crate) fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

/// What the process wrote to its standard output, as text.
pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// What the process wrote to its standard error, as text.
pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
