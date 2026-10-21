//! `buildl verify`, run as a process: what it says, and the status it ends with.

use crate::common::{
    code, manifest, manifest_with, program, run, sandboxed_manifest, stderr, stdout, workspace,
};

/// One target, `//:copy`, that copies `a.txt`.
const COPY: &str =
    "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

/// One target, `//:stamp`, that writes the process id of the shell that ran it, so a second run
/// does not reproduce the first.
const STAMP: &str = "buildl.target(\"stamp\", { inputs = { \"stamp.sh\" }, run = { \"sh\", \"$in\", \"$out\" } })\n";

/// The summary line of a verify that checked `verified` targets and found them all reproduced.
fn all_verified(verified: usize) -> String {
    format!("{verified} verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled\n")
}

#[test]
fn verify_of_a_reproducible_workspace_exits_zero() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);

    let verified = run(&root, &["verify"], &[]);

    assert_eq!(code(&verified), 0);
    assert_eq!(stdout(&verified), "");
    assert_eq!(
        stderr(&verified),
        format!("verified //:copy\n{}", all_verified(1))
    );
}

#[test]
fn a_mismatch_exits_one_and_names_target_and_output() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["sh"])),
        ("build.lua", STAMP),
    ]);
    program(&root, "stamp.sh", "echo $$ > \"$1\"\n");
    assert_eq!(code(&run(&root, &["build"], &[])), 0);

    let verified = run(&root, &["verify"], &[]);

    assert_eq!(code(&verified), 1);
    assert_eq!(stdout(&verified), "");
    assert_eq!(
        stderr(&verified),
        "MISMATCH //:stamp: output stamp differs: bytes\n\
         0 verified, 1 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled\n"
    );
}

#[test]
fn verify_under_bare_exits_five_naming_the_line_to_write() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);

    let verified = run(&root, &["verify"], &[]);

    assert_eq!(code(&verified), 5);
    assert_eq!(stdout(&verified), "");
    assert_eq!(
        stderr(&verified),
        concat!(
            "error: manifest: buildl.toml: [execution] strategy is \"bare\", ",
            "and verify re-runs actions in the input sandbox only: ",
            "write strategy = \"sandbox\" under [execution]\n",
        )
    );
}

#[test]
fn verify_under_container_exits_five() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            &manifest_with(&["cp"], "strategy = \"container\"\n"),
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);

    let verified = run(&root, &["verify"], &[]);

    assert_eq!(code(&verified), 5);
    assert_eq!(stdout(&verified), "");
    assert!(
        stderr(&verified)
            .starts_with("error: manifest: buildl.toml: [execution] strategy is \"container\""),
        "{}",
        stderr(&verified)
    );
}

#[test]
fn sample_zero_is_a_usage_error() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);

    let verified = run(&root, &["verify", "--sample", "0"], &[]);

    assert_eq!(code(&verified), 2);
    assert_eq!(stdout(&verified), "");
    assert!(
        stderr(&verified).starts_with("error: invalid value '0' for '--sample <N>'"),
        "{}",
        stderr(&verified)
    );
}

#[test]
fn sample_takes_a_number() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &sandboxed_manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);

    let verified = run(&root, &["verify", "--sample", "1"], &[]);

    assert_eq!(code(&verified), 0);
    assert_eq!(
        stderr(&verified),
        format!("verified //:copy\n{}", all_verified(1))
    );
}
