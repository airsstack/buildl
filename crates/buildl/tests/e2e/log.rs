//! Reading the log of a real workspace: what a target printed on its last run, read from the
//! build log and the content store alone.
//!
//! The fixture `logged` holds `hello`, whose action prints a line on each stream, and `quiet`,
//! which prints nothing, so a stream and the absence of one can both be told from what is read
//! back. `greeting` is an alias, which the log has no run for. Neither `chain` nor `failing` has
//! an action that prints.

use buildl::buildl_core::{Error, OutcomeClass};
use buildl::state::StateLock;

use crate::common::Fixture;

/// A copy of `logged` that has been built once.
fn built() -> Fixture {
    let logged = Fixture::copy_of("logged", &[]);
    logged.build();
    logged
}

/// The label `raw` names, read by `fixture`'s workspace.
fn label(fixture: &Fixture, raw: &str) -> buildl::buildl_core::Label {
    fixture.open().label(raw).unwrap()
}

#[test]
fn log_after_a_build_shows_what_each_target_printed() {
    let logged = built();
    let workspace = logged.open();

    let hello = workspace.log(&label(&logged, "//:hello")).unwrap();
    let quiet = workspace.log(&label(&logged, "//:quiet")).unwrap();

    assert_eq!(hello.last.run.class, OutcomeClass::Built);
    assert_eq!(hello.stdout, b"said hello.txt\n");
    assert_eq!(hello.stderr, b"warned\n");
    assert_eq!(quiet.last.run.class, OutcomeClass::Built);
    assert_eq!(quiet.stdout, b"");
    assert_eq!(quiet.stderr, b"");
}

#[test]
fn log_after_a_cached_build_shows_the_real_run() {
    let logged = built();
    logged.build();

    let shown = logged.open().log(&label(&logged, "//:hello")).unwrap();

    assert_eq!(shown.last.run.class, OutcomeClass::Cached);
    assert_eq!(shown.stdout, b"said hello.txt\n");
    assert_eq!(shown.stderr, b"warned\n");
}

#[test]
fn log_while_a_build_holds_the_lock_still_reads() {
    let logged = built();
    let _held = StateLock::acquire(logged.state(), logged.scratch()).unwrap();

    let shown = logged.open().log(&label(&logged, "//:hello")).unwrap();

    assert_eq!(shown.stdout, b"said hello.txt\n");
}

#[test]
fn log_with_a_blob_removed_is_missing_output() {
    let logged = built();
    let hello = label(&logged, "//:hello");
    let stdout = logged
        .open()
        .log(&hello)
        .unwrap()
        .last
        .run
        .stdout
        .unwrap()
        .to_string();
    let blob = logged.state().blobs().join(&stdout[..2]).join(&stdout[2..]);
    std::fs::remove_file(blob).unwrap();

    let error = logged.open().log(&hello).unwrap_err();

    assert!(
        matches!(&error, Error::MissingOutput { label } if *label == hello),
        "{error:?}"
    );
}

#[test]
fn log_creates_no_state_directory() {
    let logged = Fixture::copy_of("logged", &[]);

    let error = logged.open().log(&label(&logged, "//:hello")).unwrap_err();

    assert!(matches!(error, Error::NoRun { .. }), "{error:?}");
    assert!(!logged.root().join(".buildl").exists());
}

#[test]
fn log_reads_with_a_build_file_broken_since() {
    let logged = built();
    logged.append("build.lua", b"this is not Lua\n");
    assert!(logged.open().check().is_err());

    let shown = logged.open().log(&label(&logged, "//:hello")).unwrap();

    assert_eq!(shown.last.run.class, OutcomeClass::Built);
    assert_eq!(shown.stdout, b"said hello.txt\n");
}

#[test]
fn log_shows_a_target_removed_from_the_build_files_since_it_ran() {
    let logged = built();
    logged.replace_line(
        "build.lua",
        "b.target(\"quiet\", { deps = { \"hello\" }, run = { \"cp\", \"$deps\", \"$out\" } })",
        "-- removed",
    );
    let workspace = logged.open();

    let shown = workspace.log(&label(&logged, "//:quiet")).unwrap();

    assert_eq!(shown.last.run.class, OutcomeClass::Built);
    assert!(
        workspace
            .graph()
            .unwrap()
            .0
            .id_of(&label(&logged, "//:quiet"))
            .is_none()
    );
}

#[test]
fn log_of_an_alias_is_no_run() {
    let logged = built();
    let greeting = label(&logged, "//:greeting");

    let error = logged.open().log(&greeting).unwrap_err();

    assert!(
        matches!(&error, Error::NoRun { label, .. } if *label == greeting),
        "{error:?}"
    );
}
