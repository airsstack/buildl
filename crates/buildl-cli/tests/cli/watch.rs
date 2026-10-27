//! `buildl watch`, run as a process that stays up between the lines it writes.
//!
//! The binary is started on a workspace of one target that copies a file, and its standard
//! error is read line by line on a thread of its own, so every wait here is bounded and a
//! failing test ends instead of hanging.

use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use crate::common::{append, manifest, workspace};

/// How long a test waits for the next line, or for the process to end, before it fails.
const BOUND: Duration = Duration::from_secs(60);

/// One target, `//:copy`, that copies `a.txt`.
const COPY: &str =
    "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

/// The line a watch opens with.
pub(crate) const BUILDING: &str = "watch: building";

/// The line a watch writes when a build is done and nothing has changed since.
pub(crate) const WAITING: &str = "watch: waiting for changes";

/// A running `buildl watch` and the lines it writes to standard error.
///
/// Dropping it kills the process and reaps it, so a test that fails part way leaves nothing
/// running.
pub(crate) struct Watching {
    child: Child,
    lines: Receiver<String>,
}

impl Watching {
    /// Starts the binary with `args` in `root`, its environment cleared down to the search path
    /// of this process.
    pub(crate) fn start(root: &Path, args: &[&str]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_buildl"));
        command
            .args(args)
            .current_dir(root)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(path) = env::var_os("PATH") {
            command.env("PATH", path);
        }
        let mut child = command.spawn().unwrap();
        let reader = BufReader::new(child.stderr.take().unwrap());
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    /// The lines up to and including the next `last`.
    pub(crate) fn until(&self, last: &str) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let received = self.lines.recv_timeout(BOUND);
            assert!(
                received.is_ok(),
                "waiting for {last:?} after {seen:?}: {received:?}"
            );
            let line = received.unwrap();
            let done = line == last;
            seen.push(line);
            if done {
                return seen;
            }
        }
    }

    /// Sends the process `SIGINT`, as a terminal does for Ctrl-C.
    fn interrupt(&self) {
        let sent = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success(), "kill -INT: {sent}");
    }

    /// The lines the process writes until it closes its standard error, and how it ended.
    fn finish(&mut self) -> (Vec<String>, ExitStatus) {
        let mut rest = Vec::new();
        loop {
            match self.lines.recv_timeout(BOUND) {
                Ok(line) => rest.push(line),
                Err(error) => {
                    assert_eq!(
                        error,
                        RecvTimeoutError::Disconnected,
                        "the process is still running after {rest:?}"
                    );
                    break;
                }
            }
        }
        (rest, self.child.wait().unwrap())
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A workspace of the one target.
fn copy_workspace() -> (tempfile::TempDir, PathBuf) {
    workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ])
}

#[test]
fn watch_builds_then_waits() {
    let (_dir, root) = copy_workspace();
    let watch = Watching::start(&root, &["watch"]);
    assert_eq!(watch.until(WAITING), [BUILDING, "1 built", WAITING]);
}

#[test]
fn a_watch_is_quiet_while_nothing_changes() {
    let (_dir, root) = copy_workspace();
    let watch = Watching::start(&root, &["watch"]);
    watch.until(WAITING);
    assert_eq!(
        watch.lines.recv_timeout(Duration::from_secs(3)),
        Err(RecvTimeoutError::Timeout)
    );
}

#[test]
fn an_edit_rebuilds() {
    let (_dir, root) = copy_workspace();
    let watch = Watching::start(&root, &["watch"]);
    watch.until(WAITING);
    append(&root, "a.txt", "more\n");
    assert_eq!(
        watch.until(WAITING),
        ["watch: a.txt changed", "1 built", WAITING]
    );
}

#[test]
fn sigint_ends_watch_without_a_summary() {
    let (_dir, root) = copy_workspace();
    let mut watch = Watching::start(&root, &["watch"]);
    watch.until(WAITING);
    watch.interrupt();
    let (rest, status) = watch.finish();
    assert_eq!(status.signal(), Some(2), "{status}");
    assert!(rest.is_empty(), "{rest:?}");
}

#[test]
fn a_usage_error_exits_2() {
    let (_dir, root) = copy_workspace();
    let mut watch = Watching::start(&root, &["watch", "--no-remote", "--remote-upload"]);
    let (lines, status) = watch.finish();
    assert_eq!(status.code(), Some(2), "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.starts_with("watch:")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line
            .contains("--no-remote cannot be combined with --remote-upload or --remote-strict")),
        "{lines:?}"
    );
}
