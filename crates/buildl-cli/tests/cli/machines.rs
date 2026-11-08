//! The same workspace planned in two folders, as two machines would: each folder has its own
//! temporary directory, home and search path, and each target's key must come out the same.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{code, path_with, run, stdout, workspace};

/// The names of the targets in the workspace, each independent of the others so that `plan`
/// gives every one a finished key.
const TARGETS: [&str; 3] = ["one", "two", "three"];

/// A manifest whose ceiling reads `HOME`, `PATH` and `RUSTFLAGS`, the first two unkeyed.
const MANIFEST: &str = "[ceiling]\nproc.run = [\"cp\"]\nenv.read = [\"HOME\", \"PATH\", \"RUSTFLAGS\"]\nenv.unkeyed = [\"HOME\", \"PATH\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n";

/// The `env` marks of a target that keeps `HOME` and `PATH` out of its key.
const ENV_UNKEYED: &str = "HOME = \"unkeyed\", PATH = \"unkeyed\"";

/// The `env` marks of a target that keeps `HOME` and `PATH` out of its key and puts `RUSTFLAGS`
/// in it.
const ENV_WITH_FLAGS: &str = "HOME = \"unkeyed\", PATH = \"unkeyed\", RUSTFLAGS = \"key\"";

/// A build file with one target per name in [`TARGETS`], each with the `env` marks `env`.
fn build_file(env: &str) -> String {
    let mut text = String::new();
    for name in TARGETS {
        let _ = writeln!(
            text,
            "buildl.target(\"{name}\", {{ inputs = {{ \"{name}.txt\" }}, env = {{ {env} }}, run = {{ \"cp\", \"$in\", \"$out\" }} }})"
        );
    }
    text
}

/// A workspace holding the manifest, a build file whose targets carry the `env` marks `env`,
/// and one input file per target.
fn workspace_with_env(env: &str) -> (tempfile::TempDir, PathBuf) {
    let build = build_file(env);
    let names: Vec<String> = TARGETS.iter().map(|name| format!("{name}.txt")).collect();
    let mut files = vec![("buildl.toml", MANIFEST), ("build.lua", build.as_str())];
    files.extend(names.iter().map(|name| (name.as_str(), "input\n")));
    workspace(&files)
}

/// The key of each target in the `plan --json` text, in the order of [`TARGETS`].
///
/// Scans the text for the layout buildl always prints: canonical JSON with sorted keys, so a
/// target's `key` object holds `components` and then `digest`. Panics on a target with no key.
fn keys(plan: &str) -> Vec<String> {
    TARGETS
        .iter()
        .map(|name| {
            let target = format!("\"//:{name}\":{{\"key\":{{\"components\":");
            let found = plan.find(&target);
            assert!(found.is_some(), "{name} has no finished key: {plan}");
            let from = found.unwrap();
            // In that layout the digest follows the components object, whose own input digests
            // sit in objects of their own.
            let marker = "}},\"digest\":\"";
            let start = plan[from..].find(marker).unwrap() + from + marker.len();
            let length = plan[start..].find('"').unwrap();
            plan[start..start + length].to_owned()
        })
        .collect()
}

/// One machine: where its temporary files and home are, a folder put before its search path,
/// and the value its `RUSTFLAGS` holds.
struct Machine<'a> {
    home: &'a Path,
    tmp: &'a Path,
    extra_bin: &'a Path,
    flags: &'a str,
}

/// Plans the workspace at `root` on `machine` and returns the keys of its targets.
fn plan_on(root: &Path, machine: &Machine<'_>) -> Vec<String> {
    let path = path_with(machine.extra_bin);
    let output = run(
        root,
        &["plan", "--json"],
        &[
            ("HOME", machine.home.as_os_str()),
            ("TMPDIR", machine.tmp.as_os_str()),
            ("PATH", path.as_os_str()),
            ("RUSTFLAGS", OsStr::new(machine.flags)),
        ],
    );
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    keys(&stdout(&output))
}

/// Plans the same workspace, its targets marked `env`, in two folders as two machines that
/// differ in `TMPDIR`, `HOME` and search path and hold `first_flags` and `second_flags` in
/// `RUSTFLAGS`, and returns each machine's keys.
fn on_two_machines(env: &str, first_flags: &str, second_flags: &str) -> (Vec<String>, Vec<String>) {
    let (_first_dir, first_root) = workspace_with_env(env);
    let (_second_dir, second_root) = workspace_with_env(env);
    let (_scratch, scratch) = workspace(&[]);
    let folder = |name: &str| {
        let folder = scratch.join(name);
        fs::create_dir(&folder).unwrap();
        folder
    };
    let (home_a, home_b) = (folder("home-a"), folder("home-b"));
    let (tmp_a, tmp_b) = (folder("tmp-a"), folder("tmp-b"));
    let (bin_a, bin_b) = (folder("bin-a"), folder("bin-b"));
    let first = plan_on(
        &first_root,
        &Machine {
            home: &home_a,
            tmp: &tmp_a,
            extra_bin: &bin_a,
            flags: first_flags,
        },
    );
    let second = plan_on(
        &second_root,
        &Machine {
            home: &home_b,
            tmp: &tmp_b,
            extra_bin: &bin_b,
            flags: second_flags,
        },
    );
    (first, second)
}

#[test]
fn two_machines_plan_the_same_keys() {
    let (first, second) = on_two_machines(ENV_UNKEYED, "-O", "-O");
    assert_eq!(first, second);
}

#[test]
fn a_key_variable_still_differs() {
    let (first, second) = on_two_machines(ENV_WITH_FLAGS, "-O1", "-O2");
    for (one, other) in first.iter().zip(&second) {
        assert_ne!(one, other);
    }
}
