//! The binary against a remote cache: the summary, the switches, the usage errors, `doctor`, the
//! token kept out of every output, and remote execution end to end.
//!
//! Each test runs the built binary in workspaces whose `buildl.toml` names the in-memory server
//! of `buildl::remote::test_server`, so what a build sent to the cache and what it asked of it
//! can be read back from the server.

use std::ffi::OsStr;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use buildl::remote::test_server::{Exec, Fault, Method, ServerOptions, TestServer, TlsIdentity};

use crate::common::{code, manifest, run, sandboxed_manifest, stderr, stdout, workspace, write};
use crate::watch::{BUILDING, WAITING, Watching};

/// One target, `//:copy`, that copies `a.txt`.
const COPY: &str =
    "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

/// The test authority that signs the server's certificate.
const CA: &str = include_str!("../../../buildl/tests/fixtures/tls/ca.pem");

/// A token no output may hold.
const TOKEN: &str = "s3cr3t-t0ken-d0-not-print";

/// The variable the token is read from.
const TOKEN_VARIABLE: &str = "BUILDL_TEST_REMOTE_TOKEN";

/// A workspace that builds `//:copy` and whose `buildl.toml` is `manifest` followed by a
/// `[remote]` table of `remote`.
fn checkout(manifest: &str, remote: &str) -> (tempfile::TempDir, PathBuf) {
    workspace(&[
        ("buildl.toml", &format!("{manifest}\n[remote]\n{remote}")),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ])
}

/// A workspace that runs in the sandbox and names the cache at `url`.
fn sandboxed(url: &str) -> (tempfile::TempDir, PathBuf) {
    checkout(&sandboxed_manifest(&["cp"]), &format!("url = \"{url}\"\n"))
}

/// The `grpc://` url of `server`.
fn url_of(server: &TestServer) -> String {
    format!("grpc://{}", server.address())
}

/// A `grpc://` url nothing listens at.
fn closed_url() -> String {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("grpc://{}", closed.local_addr().unwrap());
    drop(closed);
    url
}

/// A server that serves TLS with the test certificate.
fn tls_server() -> TestServer {
    TestServer::start_with(ServerOptions {
        tls: Some(TlsIdentity {
            certificate_pem: include_str!("../../../buildl/tests/fixtures/tls/server.pem"),
            key_pem: include_str!("../../../buildl/tests/fixtures/tls/server.key"),
        }),
        ..ServerOptions::default()
    })
    .unwrap()
}

/// A workspace that runs in the sandbox and names the TLS cache at `server`, trusting the test
/// authority and reading its token from [`TOKEN_VARIABLE`].
fn with_token(server: &TestServer) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!(
            "url = \"grpcs://{}\"\nca = \"ca.pem\"\ntoken = {{ env = \"{TOKEN_VARIABLE}\" }}\n",
            server.address()
        ),
    );
    write(&root, "ca.pem", CA);
    (dir, root)
}

/// The text of every file under `dir`, each as lossy UTF-8.
fn texts_under(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut texts = Vec::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            texts.extend(texts_under(&path));
        } else {
            texts.push(String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned());
        }
    }
    texts
}

#[test]
fn build_reports_fetched_targets_in_the_summary() {
    let server = TestServer::start().unwrap();
    let (_first_dir, first) = sandboxed(&url_of(&server));
    let (_second_dir, second) = sandboxed(&url_of(&server));
    assert_eq!(code(&run(&first, &["build", "--remote-upload"], &[])), 0);

    let fetched = run(&second, &["build"], &[]);

    assert_eq!(code(&fetched), 0, "{}", stderr(&fetched));
    assert!(
        stderr(&fetched).contains("1 fetched"),
        "{}",
        stderr(&fetched)
    );
    assert!(!stderr(&fetched).contains("built"), "{}", stderr(&fetched));
    assert_eq!(
        fs::read(first.join("out/copy")).unwrap(),
        fs::read(second.join("out/copy")).unwrap()
    );
}

#[test]
fn remote_upload_seeds_the_server() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = sandboxed(&url_of(&server));

    let built = run(&root, &["build", "--remote-upload"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 1);
}

#[test]
fn the_upload_variable_seeds_the_server() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = sandboxed(&url_of(&server));

    let built = run(
        &root,
        &["build"],
        &[("BUILDL_REMOTE_UPLOAD", OsStr::new("1"))],
    );

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 1);
}

#[test]
fn no_remote_builds_without_connecting() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = sandboxed(&url_of(&server));

    let built = run(&root, &["build", "--no-remote"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn a_strict_failure_exits_as_infrastructure() {
    let (_dir, root) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!("url = \"{}\"\nretries = 0\n", closed_url()),
    );

    let built = run(&root, &["build", "--remote-strict"], &[]);

    assert_eq!(code(&built), 3, "{}", stderr(&built));
}

#[test]
fn a_missing_token_exits_as_usage() {
    let (_dir, root) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!("url = \"grpcs://127.0.0.1:1\"\ntoken = {{ env = \"{TOKEN_VARIABLE}\" }}\n"),
    );

    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 2, "{}", stderr(&built));
    assert!(
        stderr(&built).contains(TOKEN_VARIABLE),
        "{}",
        stderr(&built)
    );
    assert!(!root.join("out").exists(), "something was built");
}

#[test]
fn uploading_under_bare_is_refused_with_the_usage_exit() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(
        &manifest(&["cp"]),
        &format!("url = \"{}\"\n", url_of(&server)),
    );

    let built = run(&root, &["build", "--remote-upload"], &[]);

    assert_eq!(code(&built), 2, "{}", stderr(&built));
    assert!(stderr(&built).contains("bare"), "{}", stderr(&built));
    assert!(!root.join("out").exists(), "something was built");
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn a_token_with_a_grpc_url_is_refused() {
    let (_dir, root) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!("url = \"grpc://127.0.0.1:1\"\ntoken = {{ env = \"{TOKEN_VARIABLE}\" }}\n"),
    );

    let built = run(&root, &["build"], &[(TOKEN_VARIABLE, OsStr::new(TOKEN))]);

    assert_eq!(code(&built), 5, "{}", stderr(&built));
    let text = stderr(&built);
    let manifest = std::fs::read_to_string(root.join("buildl.toml")).unwrap();
    let line = manifest
        .lines()
        .position(|line| line.starts_with("token"))
        .unwrap()
        + 1;
    assert!(text.contains(&format!("buildl.toml:{line}:")), "{text}");
    assert!(text.contains("remote.token"), "{text}");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!root.join("out").exists(), "something was built");
}

#[test]
fn doctor_prints_the_remote_section() {
    let server = tls_server();
    let (_dir, root) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!(
            concat!(
                "url = \"grpcs://{}\"\n",
                "instance = \"ci\"\n",
                "ca = \"ca.pem\"\n",
                "header = \"x-cache-token\"\n",
                "timeout = \"45s\"\n",
                "token = {{ env = \"{}\" }}\n",
            ),
            server.address(),
            TOKEN_VARIABLE
        ),
    );

    let with = run(
        &root,
        &["doctor"],
        &[
            (TOKEN_VARIABLE, OsStr::new(TOKEN)),
            ("BUILDL_REMOTE_UPLOAD", OsStr::new("1")),
        ],
    );
    let without = run(&root, &["doctor"], &[]);

    assert_eq!(code(&with), 0, "{}", stderr(&with));
    assert!(
        stdout(&with).contains(&format!(
            concat!(
                "remote\n",
                "  url       grpcs://{}\n",
                "  instance  ci\n",
                "  ca        ca.pem\n",
                "  header    x-cache-token\n",
                "  timeout   45s\n",
                "  token     set\n",
                "  upload    on\n",
                "  strict    off\n",
                "  --remote-upload and --remote-strict turn either on for one run\n",
            ),
            server.address()
        )),
        "{}",
        stdout(&with)
    );
    assert!(
        stdout(&without).contains("  token     not set\n  upload    off\n"),
        "{}",
        stdout(&without)
    );
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn doctor_says_none_when_the_workspace_names_no_remote() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);

    let output = run(&root, &["doctor"], &[]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("remote: none\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn the_token_appears_in_no_output_and_not_in_the_log() {
    let server = tls_server();
    let (_dir, root) = with_token(&server);
    let variables = [
        (TOKEN_VARIABLE, OsStr::new(TOKEN)),
        ("BUILDL_REMOTE_UPLOAD", OsStr::new("1")),
    ];
    let mut outputs = vec![run(&root, &["build"], &variables)];
    assert_eq!(code(&outputs[0]), 0, "{}", stderr(&outputs[0]));
    assert!(
        server
            .calls()
            .iter()
            .any(|call| call.headers.values().any(|value| value.contains(TOKEN))),
        "the token was never sent: {:?}",
        server.calls()
    );

    // A strict failure: the same token against a server that is not there.
    let (_down_dir, down) = checkout(
        &sandboxed_manifest(&["cp"]),
        &format!(
            "url = \"grpcs://{}\"\nretries = 0\ntoken = {{ env = \"{TOKEN_VARIABLE}\" }}\n",
            closed_url().trim_start_matches("grpc://")
        ),
    );
    let strict = run(
        &down,
        &["build", "--remote-strict"],
        &[(TOKEN_VARIABLE, OsStr::new(TOKEN))],
    );
    assert_eq!(code(&strict), 3, "{}", stderr(&strict));
    outputs.push(strict);
    outputs.push(run(&root, &["doctor"], &variables));

    for output in &outputs {
        assert!(!stdout(output).contains(TOKEN), "{}", stdout(output));
        assert!(!stderr(output).contains(TOKEN), "{}", stderr(output));
    }
    for state in [root.join(".buildl"), down.join(".buildl")] {
        for text in texts_under(&state) {
            assert!(!text.contains(TOKEN), "the state holds the token");
        }
    }
}

#[test]
fn a_bad_upload_variable_exits_as_usage() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = sandboxed(&url_of(&server));

    let built = run(
        &root,
        &["build"],
        &[("BUILDL_REMOTE_UPLOAD", OsStr::new("yes"))],
    );

    assert_eq!(code(&built), 2, "{}", stderr(&built));
    assert!(
        stderr(&built).contains("BUILDL_REMOTE_UPLOAD"),
        "{}",
        stderr(&built)
    );
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

/// Two targets, `//:one` and `//:two`, that copy `a.txt`.
const TWO: &str = concat!(
    "buildl.target(\"one\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
    "buildl.target(\"two\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
);

/// The environment variables a run is given.
type Variables<'a> = &'a [(&'a str, &'a OsStr)];

/// A workspace of [`TWO`] that runs in the sandbox, with `remote` as its `[remote]` table.
fn two_targets(remote: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = checkout(&sandboxed_manifest(&["cp"]), remote);
    write(&root, "build.lua", TWO);
    (dir, root)
}

/// The `[remote]` lines that name `server` and turn execution on.
fn executing_at(server: &TestServer) -> String {
    format!("url = \"{}\"\nexecute = true\n", url_of(server))
}

#[test]
fn remote_execute_runs_every_target_on_the_server() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = two_targets(&format!("url = \"{}\"\n", url_of(&server)));

    let built = run(&root, &["build", "--remote-execute"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(stderr(&built), "2 remote\n");
    assert_eq!(server.calls_to(Method::Execute).len(), 2);
    assert_eq!(fs::read(root.join("out/one")).unwrap(), b"a\n");
    assert_eq!(fs::read(root.join("out/two")).unwrap(), b"a\n");
}

#[test]
fn a_second_checkout_fetches_what_the_server_ran() {
    let server = TestServer::start().unwrap();
    let remote = format!("url = \"{}\"\n", url_of(&server));
    let (_first_dir, first) = two_targets(&remote);
    let (_second_dir, second) = two_targets(&remote);
    assert_eq!(code(&run(&first, &["build", "--remote-execute"], &[])), 0);

    let fetched = run(&second, &["build"], &[]);

    assert_eq!(code(&fetched), 0, "{}", stderr(&fetched));
    assert_eq!(stderr(&fetched), "2 fetched\n");
    assert_eq!(server.calls_to(Method::Execute).len(), 2);
    for name in ["out/one", "out/two"] {
        assert_eq!(
            fs::read(first.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap()
        );
    }
}

#[test]
fn the_manifest_switch_runs_remotely() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = two_targets(&executing_at(&server));

    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(stderr(&built), "2 remote\n");
    assert_eq!(server.calls_to(Method::Execute).len(), 2);
}

#[test]
fn remote_execution_under_bare_exits_as_usage_from_each_source() {
    let server = TestServer::start().unwrap();
    let url = url_of(&server);
    let cases: [(&str, &[&str], Variables); 3] = [
        ("url = \"URL\"\nexecute = true\n", &["build"], &[]),
        (
            "url = \"URL\"\n",
            &["build"],
            &[("BUILDL_REMOTE_EXECUTE", OsStr::new("1"))],
        ),
        ("url = \"URL\"\n", &["build", "--remote-execute"], &[]),
    ];
    for (remote, args, variables) in cases {
        let (_dir, root) = checkout(&manifest(&["cp"]), &remote.replace("URL", &url));

        let built = run(&root, args, variables);

        assert_eq!(code(&built), 2, "{args:?}: {}", stderr(&built));
        assert!(stderr(&built).contains("bare"), "{}", stderr(&built));
        assert!(!root.join("out").exists(), "something was built");
    }
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn test_under_bare_with_execution_exits_as_usage() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(&manifest(&["sh"]), &executing_at(&server));
    write(
        &root,
        "build.lua",
        "buildl.test(\"one\", { run = { \"sh\", \"-c\", \"echo one\" } })\n",
    );

    let tested = run(&root, &["test"], &[]);

    assert_eq!(code(&tested), 2, "{}", stderr(&tested));
    assert!(stderr(&tested).contains("bare"), "{}", stderr(&tested));
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn a_bare_manifest_that_executes_still_checks_plans_and_doctors() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(&manifest(&["cp"]), &executing_at(&server));

    for command in ["check", "plan", "doctor"] {
        let output = run(&root, &[command], &[]);

        assert_eq!(code(&output), 0, "{command}: {}", stderr(&output));
    }
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn every_execution_usage_error_exits_as_usage() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = sandboxed(&url_of(&server));
    let (_bare_dir, no_table) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    let cases: [(&Path, &[&str], Variables, &str); 5] = [
        (
            &root,
            &["build", "--remote-execute", "--no-remote"],
            &[],
            "--no-remote cannot be combined with --remote-execute",
        ),
        (
            &root,
            &["build", "--remote-execute", "--no-remote-execute"],
            &[],
            "--remote-execute cannot be combined with --no-remote-execute",
        ),
        (
            &no_table,
            &["build", "--remote-execute"],
            &[],
            "--remote-jobs need a [remote] table",
        ),
        (
            &root,
            &["build", "--remote-jobs", "4"],
            &[],
            "--remote-jobs needs remote execution",
        ),
        (
            &root,
            &["build"],
            &[("BUILDL_REMOTE_EXECUTE", OsStr::new("yes"))],
            "BUILDL_REMOTE_EXECUTE",
        ),
    ];
    for (start, args, variables, message) in cases {
        let built = run(start, args, variables);

        assert_eq!(code(&built), 2, "{args:?}: {}", stderr(&built));
        assert!(
            stderr(&built).contains(message),
            "{args:?}: {}",
            stderr(&built)
        );
        assert!(!start.join("out").exists(), "something was built");
    }
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn a_server_that_cannot_execute_warns_and_builds_locally() {
    let server = TestServer::start_with(ServerOptions {
        exec: Exec::Disabled,
        ..ServerOptions::default()
    })
    .unwrap();
    let (_dir, root) = two_targets(&executing_at(&server));

    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    let text = stderr(&built);
    let warnings: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("warning:"))
        .collect();
    assert_eq!(warnings.len(), 1, "{text}");
    assert!(warnings[0].contains("execute"), "{text}");
    assert!(text.contains("2 built"), "{text}");
    assert!(!text.contains("remote\n"), "{text}");
    assert_eq!(server.calls_to(Method::Execute).len(), 0);
}

#[test]
fn a_strict_execute_failure_exits_as_infrastructure() {
    let server = TestServer::start().unwrap();
    server.fail(Method::Execute, Fault::PermissionDenied);
    let (_dir, root) = two_targets(&format!("{}retries = 0\n", executing_at(&server)));

    let built = run(&root, &["build", "--remote-strict"], &[]);

    assert_eq!(code(&built), 3, "{}", stderr(&built));
}

#[test]
fn a_remote_action_failure_fails_the_build_as_an_action_failure() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(&sandboxed_manifest(&["sh"]), &executing_at(&server));
    write(
        &root,
        "build.lua",
        "buildl.target(\"bad\", { run = { \"sh\", \"-c\", \"echo printed-remotely; exit 1\" } })\n",
    );

    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 1, "{}", stderr(&built));
    assert!(
        stderr(&built).contains("FAILED //:bad: action failure"),
        "{}",
        stderr(&built)
    );
    let log = run(&root, &["log", "bad"], &[]);
    assert_eq!(code(&log), 0, "{}", stderr(&log));
    assert!(
        format!("{}{}", stdout(&log), stderr(&log)).contains("printed-remotely"),
        "{}{}",
        stdout(&log),
        stderr(&log)
    );
}

#[test]
fn test_runs_tests_remotely() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(
        &sandboxed_manifest(&["sh"]),
        &format!("url = \"{}\"\n", url_of(&server)),
    );
    write(
        &root,
        "build.lua",
        concat!(
            "buildl.test(\"one\", { run = { \"sh\", \"-c\", \"echo one\" } })\n",
            "buildl.test(\"two\", { run = { \"sh\", \"-c\", \"echo two\" } })\n",
        ),
    );

    let tested = run(&root, &["test", "--remote-execute"], &[]);

    assert_eq!(code(&tested), 0, "{}", stderr(&tested));
    assert!(
        stderr(&tested).contains("tests: 2 passed (2 remote)"),
        "{}",
        stderr(&tested)
    );
    assert_eq!(server.calls_to(Method::Execute).len(), 2);
}

#[test]
fn watch_takes_remote_execute() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = two_targets(&format!("url = \"{}\"\n", url_of(&server)));

    let watch = Watching::start(&root, &["watch", "--remote-execute"]);

    assert_eq!(watch.until(WAITING), [BUILDING, "2 remote", WAITING]);
    assert_eq!(server.calls_to(Method::Execute).len(), 2);
}

#[test]
fn watch_under_bare_with_execution_exits_as_usage() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(
        &manifest(&["cp"]),
        &format!("url = \"{}\"\n", url_of(&server)),
    );

    let mut watch = Watching::start(&root, &["watch", "--remote-execute"]);
    let (lines, status) = watch.finish();

    assert_eq!(status.code(), Some(2), "{lines:?}");
    assert!(lines.iter().all(|line| line != BUILDING), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("bare")), "{lines:?}");
    assert!(!root.join("out").exists(), "something was built");
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}

#[test]
fn watch_under_bare_with_upload_exits_as_usage() {
    let server = TestServer::start().unwrap();
    let (_dir, root) = checkout(
        &manifest(&["cp"]),
        &format!("url = \"{}\"\n", url_of(&server)),
    );

    let mut watch = Watching::start(&root, &["watch", "--remote-upload"]);
    let (lines, status) = watch.finish();

    assert_eq!(status.code(), Some(2), "{lines:?}");
    assert!(lines.iter().all(|line| line != BUILDING), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains("bare")), "{lines:?}");
    assert!(!root.join("out").exists(), "something was built");
    assert!(server.calls().is_empty(), "{:?}", server.calls());
}
