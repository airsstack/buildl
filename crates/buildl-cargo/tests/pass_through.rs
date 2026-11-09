//! The `cargo-buildl` binary, run as cargo runs it: with `buildl` as its first argument.
//!
//! Each test runs the built binary in a temporary workspace whose `buildl.toml` it writes, with
//! standard output and standard error piped, and reads the status as the number a shell sees.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::fs;
use std::process::{Command, Output};

/// A manifest that grants nothing and runs one action at a time.
const MANIFEST: &str = "[ceiling]\nproc.run = []\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\n";

/// `cargo-buildl` run with `args` in a new workspace that holds [`MANIFEST`].
fn run(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("buildl.toml"), MANIFEST).unwrap();
    Command::new(env!("CARGO_BIN_EXE_cargo-buildl"))
        .args(args)
        .current_dir(dir.path())
        .output()
        .unwrap()
}

#[test]
fn clean_passes_through() {
    let output = run(&["buildl", "clean"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "clean: out holds nothing\n"
    );
}

#[test]
fn a_usage_error_exits_2() {
    let output = run(&["buildl", "bogus"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn help_comes_from_cargo_buildl() {
    let output = run(&["buildl", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("cargo buildl"), "{help}");
    let listed = |name: &str| {
        help.lines()
            .any(|line| line.trim_start().starts_with(&format!("{name} ")))
    };
    assert!(listed("build"), "{help}");
    assert!(listed("test"), "{help}");
}
