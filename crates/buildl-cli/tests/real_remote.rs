//! The binary against a real Remote Execution server.
//!
//! The server is the one `BUILDL_TEST_REAPI_URL` names (the CI job starts `NativeLink` there);
//! without the variable every test returns at once, so a machine with no server passes them.
//! A test that runs says so on standard error, in the line `real server: <test> against <url>`.
//! The server keeps results across tests, so each test writes an input no other test shares and
//! its actions are new to the server.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// The variable that names the server.
const URL_VARIABLE: &str = "BUILDL_TEST_REAPI_URL";

/// `//:lib` copies the input; `//:app` copies what `//:lib` made.
const TARGETS: &str = "buildl.target(\"lib\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\nbuildl.target(\"app\", { deps = { \"lib\" }, run = { \"cp\", \"$deps\", \"$out\" } })\n";

/// The server's url when `variable` holds one, and `None` when it is unset or empty.
fn server_url(variable: Option<OsString>) -> Option<String> {
    variable
        .and_then(|value| value.into_string().ok())
        .filter(|url| !url.is_empty())
}

/// The server's url from the environment of this process, `None` when `test` is to skip.
///
/// When there is a url, `test` announces on standard error that it runs against it.
fn server(test: &str) -> Option<String> {
    let url = server_url(std::env::var_os(URL_VARIABLE))?;
    eprintln!("real server: {test} against {url}");
    Some(url)
}

/// Text no other test, and no earlier run of this one, has used.
fn unique(test: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{test} {} {nonce}\n", std::process::id())
}

/// A temporary workspace that builds [`TARGETS`] followed by `extra` over `input` and names the
/// server at `url`.
fn workspace(url: &str, input: &str, extra: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let manifest = format!(
        "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\n\n[remote]\nurl = \"{url}\"\nretries = 5\nqueue_timeout = \"2m\"\n"
    );
    fs::write(root.join("buildl.toml"), manifest).unwrap();
    fs::write(root.join("build.lua"), format!("{TARGETS}{extra}")).unwrap();
    fs::write(root.join("a.txt"), input).unwrap();
    (dir, root)
}

/// The binary run with `args` in `start`, its environment cleared down to the search path.
fn run(start: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_buildl"));
    command.args(args).current_dir(start).env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

/// The status the process ended with.
fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

/// What the process wrote to its standard error, as text.
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// What the process wrote to its standard output, as text.
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn every_target_runs_on_the_server() {
    let Some(url) = server("every_target_runs_on_the_server") else {
        return;
    };
    let input = unique("every_target_runs_on_the_server");
    let (_dir, root) = workspace(&url, &input, "");

    let built = run(&root, &["build", "--remote-execute", "--remote-strict"]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(stderr(&built), "2 remote\n");
    assert_eq!(fs::read(root.join("out/lib")).unwrap(), input.as_bytes());
    assert_eq!(fs::read(root.join("out/app")).unwrap(), input.as_bytes());
}

#[test]
fn a_fresh_checkout_fetches_what_the_server_ran() {
    let Some(url) = server("a_fresh_checkout_fetches_what_the_server_ran") else {
        return;
    };
    let input = unique("a_fresh_checkout_fetches_what_the_server_ran");
    let (_first_dir, first) = workspace(&url, &input, "");
    let (_second_dir, second) = workspace(&url, &input, "");
    let ran = run(&first, &["build", "--remote-execute", "--remote-strict"]);
    assert_eq!(code(&ran), 0, "{}", stderr(&ran));
    assert_eq!(stderr(&ran), "2 remote\n");

    let fetched = run(&second, &["build", "--remote-strict"]);

    assert_eq!(code(&fetched), 0, "{}", stderr(&fetched));
    assert_eq!(stderr(&fetched), "2 fetched\n");
    for name in ["out/lib", "out/app"] {
        assert_eq!(
            fs::read(first.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap(),
            "{name}"
        );
        assert_eq!(fs::read(second.join(name)).unwrap(), input.as_bytes());
    }
}

#[test]
fn a_failing_command_fails_as_an_action_failure() {
    let Some(url) = server("a_failing_command_fails_as_an_action_failure") else {
        return;
    };
    let input = unique("a_failing_command_fails_as_an_action_failure");
    let bad = "buildl.target(\"bad\", { run = { \"cp\", \"no-such-file\", \"$out\" } })\n";
    let (_dir, root) = workspace(&url, &input, bad);

    let built = run(
        &root,
        &["build", "//:bad", "--remote-execute", "--remote-strict"],
    );

    assert_eq!(code(&built), 1, "{}", stderr(&built));
    assert!(
        stderr(&built).contains("FAILED //:bad: action failure (exit status 1)\n"),
        "{}",
        stderr(&built)
    );
    let log = run(&root, &["log", "//:bad"]);
    assert_eq!(code(&log), 0, "{}", stderr(&log));
    let shown = format!("{}{}", stdout(&log), stderr(&log));
    assert!(shown.contains("no-such-file"), "{shown}");
    assert!(shown.contains("No such file or directory"), "{shown}");
}

#[test]
fn tests_skip_without_the_variable() {
    assert_eq!(server_url(None), None);
    assert_eq!(server_url(Some(OsStr::new("").to_owned())), None);
    assert_eq!(
        server_url(Some(OsStr::new("grpc://127.0.0.1:50051").to_owned())),
        Some("grpc://127.0.0.1:50051".to_owned())
    );
}
