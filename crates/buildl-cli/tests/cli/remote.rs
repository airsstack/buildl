//! The binary against a remote cache: the summary, the switches, the usage errors, `doctor`, and
//! the token kept out of every output.
//!
//! Each test runs the built binary in workspaces whose `buildl.toml` names the in-memory server
//! of `buildl::remote::test_server`, so what a build sent to the cache and what it asked of it
//! can be read back from the server.

use std::ffi::OsStr;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use buildl::remote::test_server::{Method, ServerOptions, TestServer, TlsIdentity};

use crate::common::{code, manifest, run, sandboxed_manifest, stderr, stdout, workspace, write};

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
