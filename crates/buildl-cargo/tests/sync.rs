//! `cargo buildl sync`, run as the built binary in a temporary copy of the fixture workspace.
//!
//! The fixture holds two members, a static library and a binary. Each test writes a `build.lua`
//! with markers and text of the engineer's own, runs the binary with cargo from the `CARGO`
//! variable, and reads the status as the number a shell sees.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const BEGIN: &str =
    "-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.";
const END: &str = "-- cargo-buildl: end generated";

/// What comes before the section in the engineer's `build.lua`.
const HEAD: &str = "-- my build file\nlocal mine = 1\n\n";
/// What comes after it.
const TAIL: &str = "\n-- my own targets\nlocal also_mine = 2\n";

/// A `build.lua` with an empty section between the markers.
fn build_file() -> String {
    format!("{HEAD}{BEGIN}\n{END}\n{TAIL}")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A new copy of the fixture workspace.
fn workspace() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workspace");
    copy_dir(&fixture, dir.path());
    dir
}

fn build_lua(dir: &TempDir) -> PathBuf {
    dir.path().join("build.lua")
}

/// `cargo-buildl buildl sync [--check]` run in `cwd`.
fn run(cwd: &Path, check: bool) -> Output {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-buildl"));
    command
        .args(["buildl", "sync"])
        .env("CARGO", cargo)
        .current_dir(cwd);
    if check {
        command.arg("--check");
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Adds a second binary to the `app` member.
fn add_a_bin(dir: &TempDir) {
    let manifest = dir.path().join("app/Cargo.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[[bin]]\nname = \"extra\"\npath = \"src/extra.rs\"\n");
    fs::write(manifest, text).unwrap();
    fs::write(dir.path().join("app/src/extra.rs"), "fn main() {}\n").unwrap();
}

#[test]
fn check_is_0_when_up_to_date() {
    let dir = workspace();
    fs::write(build_lua(&dir), build_file()).unwrap();
    let synced = run(dir.path(), false);
    assert_eq!(synced.status.code(), Some(0), "{}", stderr(&synced));
    let written = fs::read_to_string(build_lua(&dir)).unwrap();

    let output = run(dir.path(), true);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "sync: build.lua is up to date\n");
    assert_eq!(fs::read_to_string(build_lua(&dir)).unwrap(), written);
}

#[test]
fn check_is_5_after_a_new_bin_then_0_after_sync() {
    let dir = workspace();
    fs::write(build_lua(&dir), build_file()).unwrap();
    assert_eq!(run(dir.path(), false).status.code(), Some(0));
    let before = fs::read_to_string(build_lua(&dir)).unwrap();
    assert!(!before.contains("extra"), "{before}");

    add_a_bin(&dir);
    let stale = run(dir.path(), true);
    assert_eq!(stale.status.code(), Some(5), "{}", stderr(&stale));
    assert_eq!(
        stderr(&stale),
        "sync: build.lua is out of date with Cargo.toml; run cargo buildl sync\n"
    );
    assert_eq!(fs::read_to_string(build_lua(&dir)).unwrap(), before);

    let synced = run(dir.path(), false);
    assert_eq!(synced.status.code(), Some(0), "{}", stderr(&synced));
    assert_eq!(stderr(&synced), "sync: build.lua updated\n");
    let after = fs::read_to_string(build_lua(&dir)).unwrap();
    assert!(after.contains("extra"), "{after}");
    assert!(after.starts_with(&format!("{HEAD}{BEGIN}\n")), "{after}");
    assert!(after.ends_with(&format!("{END}\n{TAIL}")), "{after}");

    let current = run(dir.path(), true);
    assert_eq!(current.status.code(), Some(0), "{}", stderr(&current));
}

#[test]
fn sync_from_a_member_folder_writes_at_the_root() {
    let dir = workspace();
    fs::write(build_lua(&dir), build_file()).unwrap();

    let output = run(&dir.path().join("app"), false);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "sync: build.lua updated\n");
    let written = fs::read_to_string(build_lua(&dir)).unwrap();
    assert!(written.starts_with(HEAD), "{written}");
    assert!(written.contains("fixture-app"), "{written}");
    assert!(!dir.path().join("app/build.lua").exists());
    assert!(!dir.path().join("buildl.toml").exists());
}

#[test]
fn no_build_lua_is_5() {
    let dir = workspace();
    for check in [false, true] {
        let output = run(dir.path(), check);
        assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
        assert_eq!(
            stderr(&output),
            "sync: no build.lua; run cargo buildl init\n"
        );
    }
    assert!(!build_lua(&dir).exists());
}

#[test]
fn missing_markers_is_5() {
    let dir = workspace();
    let text = "-- no markers here\n";
    fs::write(build_lua(&dir), text).unwrap();

    let output = run(dir.path(), false);

    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "sync: build.lua has no generated section; add the lines \
         \"-- cargo-buildl: begin generated\" and \"-- cargo-buildl: end generated\" \
         where it goes\n"
    );
    assert_eq!(fs::read_to_string(build_lua(&dir)).unwrap(), text);

    fs::write(build_lua(&dir), format!("{END}\n{BEGIN}\n")).unwrap();
    let output = run(dir.path(), true);
    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "sync: build.lua must hold one begin line and, after it, one end line\n"
    );
}

#[test]
fn a_cargo_that_cannot_run_is_3_with_an_error_line() {
    let dir = workspace();
    fs::write(build_lua(&dir), build_file()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-buildl"))
        .args(["buildl", "sync"])
        .env("CARGO", dir.path().join("no-such-cargo"))
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read_to_string(build_lua(&dir)).unwrap(), build_file());
}
