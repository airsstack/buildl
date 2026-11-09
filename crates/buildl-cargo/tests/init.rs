//! `cargo buildl init`, run as the built binary in a temporary copy of the fixture workspace.
//!
//! The `PATH` of every run is an empty folder, so the plugin fetch cannot start `git` and ends
//! with 3 before anything reaches the network. Cargo is started by the path in `CARGO`.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const BEGIN: &str =
    "-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.";
const END: &str = "-- cargo-buildl: end generated";
const OWN_TARGETS: &str = "-- Your own targets go below. `cargo buildl sync` never changes them.";

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

/// `cargo-buildl buildl <args>` run in `cwd` with no `git` on the `PATH`.
fn run(cwd: &Path, args: &[&str]) -> Output {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let no_programs = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_cargo-buildl"))
        .arg("buildl")
        .args(args)
        .env("CARGO", cargo)
        .env("PATH", no_programs.path())
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Every file under `dir` with its bytes, and every folder with `None`, by relative path.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                into.insert(relative, None);
                walk(root, &path, into);
            } else {
                into.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(dir, dir, &mut into);
    into
}

#[test]
fn init_writes_the_three_files_then_fails_the_fetch_with_3() {
    let dir = workspace();

    let output = run(dir.path(), &["init"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    let stderr = stderr(&output);
    assert!(
        stderr.starts_with("init: wrote build.lua, buildl.toml, .gitignore\n"),
        "{stderr}"
    );
    assert!(stderr.contains("cargo buildl plugins fetch"), "{stderr}");

    let name = dir.path().file_name().unwrap().to_string_lossy();
    let toml = fs::read_to_string(dir.path().join("buildl.toml")).unwrap();
    assert!(
        toml.starts_with(&format!(
            "[workspace]\nname   = \"{name}\"\nentry  = \"build.lua\"\n"
        )),
        "{toml}"
    );
    assert!(
        toml.contains("rev         = \"plugin-rust-v0.2.0\"\n"),
        "{toml}"
    );
    let lua = fs::read_to_string(dir.path().join("build.lua")).unwrap();
    assert!(lua.starts_with(&format!("{BEGIN}\n")), "{lua}");
    assert!(lua.contains("fixture-app"), "{lua}");
    assert!(lua.ends_with(&format!("{END}\n\n{OWN_TARGETS}\n")), "{lua}");
    assert_eq!(
        fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        "/out/\n/.buildl/\n"
    );
}

#[test]
fn a_second_init_is_5_and_writes_nothing() {
    let dir = workspace();
    assert_eq!(run(dir.path(), &["init"]).status.code(), Some(3));
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["init"]);

    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "init: buildl.toml already exists; this folder is set up already, \
         or remove buildl.toml to start over\n"
    );
    assert_eq!(snapshot(dir.path()), before);

    fs::remove_file(dir.path().join("buildl.toml")).unwrap();
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["init"]);
    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "init: build.lua already exists; this folder is set up already, \
         or remove build.lua to start over\n"
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn no_cargo_lock_is_5_and_writes_nothing() {
    let dir = workspace();
    fs::remove_file(dir.path().join("Cargo.lock")).unwrap();
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["init"]);

    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "init: no Cargo.lock beside Cargo.toml; run cargo generate-lockfile\n"
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn a_failed_write_leaves_the_folder_as_it_was() {
    let dir = workspace();
    fs::create_dir(dir.path().join(".gitignore")).unwrap();
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["init"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
    assert!(!dir.path().join("build.lua").exists());
    assert!(!dir.path().join("buildl.toml").exists());
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn init_from_a_member_folder_writes_at_the_root() {
    let dir = workspace();

    let output = run(&dir.path().join("app"), &["init"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(dir.path().join("build.lua").is_file());
    assert!(dir.path().join("buildl.toml").is_file());
    assert!(dir.path().join(".gitignore").is_file());
    assert!(!dir.path().join("app/build.lua").exists());
    assert!(!dir.path().join("app/buildl.toml").exists());
    assert!(!dir.path().join("app/.gitignore").exists());
}

#[test]
fn sync_check_is_0_right_after_init() {
    let dir = workspace();
    assert_eq!(run(dir.path(), &["init"]).status.code(), Some(3));

    let output = run(dir.path(), &["sync", "--check"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "sync: build.lua is up to date\n");
}
