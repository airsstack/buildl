//! Cleaning the output directory of a real workspace and collecting its store: what `clean` and
//! `gc` remove, what they leave, and when they refuse.
//!
//! The fixture `logged` is used because every one of its targets succeeds, so a build of it
//! leaves a whole output directory and a whole store.

use std::fs;

use buildl::Cleaned;
use buildl::buildl_core::{Error, OutcomeClass, PortName};
use buildl::state::StateLock;

use crate::common::{Fixture, ended};

/// A copy of `logged` that has been built once.
fn built() -> Fixture {
    let logged = Fixture::copy_of("logged", &[]);
    logged.build();
    logged
}

#[test]
fn clean_then_build_restores_every_output_from_the_store() {
    let logged = built();
    let outputs = logged.files("out");
    let store = logged.files(".buildl/cas");
    let cache = fs::read(logged.state().cache()).unwrap();
    assert_eq!(outputs, ["out/hello", "out/quiet"]);

    let cleaned = logged.open().clean().unwrap();

    assert_eq!(cleaned, Cleaned::Removed);
    assert!(!logged.root().join("out").exists());
    assert_eq!(logged.files(".buildl/cas"), store);
    assert_eq!(fs::read(logged.state().cache()).unwrap(), cache);

    let rebuilt = logged.build();

    assert_eq!(
        ended(&rebuilt, &OutcomeClass::Cached),
        ["//:hello", "//:quiet"]
    );
    assert_eq!(logged.files("out"), outputs);
}

#[test]
fn clean_without_an_output_directory_is_absent() {
    let logged = Fixture::copy_of("logged", &[]);

    let cleaned = logged.open().clean().unwrap();

    assert_eq!(cleaned, Cleaned::Absent);
}

#[test]
fn clean_removes_staging_leftovers() {
    let logged = built();
    let staging = logged.state().blob_staging();
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("blob"), b"cut").unwrap();
    assert_eq!(logged.staged(), ["cas/tmp/blob"]);

    logged.open().clean().unwrap();

    assert_eq!(logged.staged(), Vec::<String>::new());
}

#[test]
fn clean_is_refused_when_the_ceiling_leaves_out_the_output_directory() {
    let logged = built();
    let outputs = logged.files("out");
    let manifest = fs::read_to_string(logged.root().join("buildl.toml")).unwrap();
    let narrowed = manifest.replace(
        "fs.write = [\"out\", \".buildl\"]",
        "fs.write = [\".buildl\"]",
    );
    assert_ne!(manifest, narrowed);
    fs::write(logged.root().join("buildl.toml"), narrowed).unwrap();

    let error = logged.open().clean().unwrap_err();

    assert!(matches!(error, Error::Refused { .. }), "{error:?}");
    assert_eq!(logged.files("out"), outputs);
}

/// The line of `build.lua` that declares `quiet` in a copy of `logged`: `quiet` copies a file of
/// its own, so the bytes it stores are its alone.
const QUIET: &str = "b.target(\"quiet\", { deps = { \"hello\" }, inputs = { \"quiet.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })";

/// A copy of `logged` whose `quiet` stores bytes of its own, built once, and whose build file
/// then stops declaring `quiet`.
fn without_quiet() -> Fixture {
    let logged = Fixture::copy_of("logged", &[]);
    fs::write(logged.root().join("quiet.txt"), "only quiet\n").unwrap();
    logged.replace_line(
        "build.lua",
        "b.target(\"quiet\", { deps = { \"hello\" }, run = { \"cp\", \"$deps\", \"$out\" } })",
        QUIET,
    );
    logged.build();
    logged.replace_line("build.lua", QUIET, "");
    logged
}

#[test]
fn gc_drops_an_undeclared_targets_row_and_its_only_blobs() {
    let logged = without_quiet();

    let collected = logged.open().gc().unwrap();

    assert_eq!(collected.rows_dropped, 1);
    assert!(collected.blobs_removed > 0, "{collected:?}");
    assert_eq!(logged.recorded(), ["//:hello"]);
    assert_eq!(ended(&logged.build(), &OutcomeClass::Cached), ["//:hello"]);
}

#[test]
fn log_of_a_dropped_target_still_reads_after_gc() {
    let logged = without_quiet();
    let quiet = logged.open().label("//:quiet").unwrap();
    let before = logged.open().log(&quiet).unwrap();

    logged.open().gc().unwrap();

    assert_eq!(logged.open().log(&quiet).unwrap(), before);
}

#[test]
fn a_second_gc_removes_nothing() {
    let logged = without_quiet();
    logged.open().gc().unwrap();

    let collected = logged.open().gc().unwrap();

    assert_eq!(
        (
            collected.rows_dropped,
            collected.blobs_removed,
            collected.bytes_removed
        ),
        (0, 0, 0)
    );
}

#[test]
fn outputs_in_out_still_read_after_gc() {
    let logged = without_quiet();
    let read = |logged: &Fixture| -> Vec<(String, Vec<u8>)> {
        logged
            .files("out")
            .into_iter()
            .map(|file| {
                let bytes = fs::read(logged.root().join(&file)).unwrap();
                (file, bytes)
            })
            .collect()
    };
    let before = read(&logged);
    assert_eq!(before.len(), 2);

    logged.open().gc().unwrap();

    assert_eq!(read(&logged), before);
}

#[test]
fn gc_removes_staging_leftovers() {
    let logged = built();
    let staging = logged.state().blob_staging();
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("blob"), b"cut").unwrap();
    assert_eq!(logged.staged(), ["cas/tmp/blob"]);

    logged.open().gc().unwrap();

    assert_eq!(logged.staged(), Vec::<String>::new());
}

#[test]
fn a_broken_build_file_stops_gc_and_leaves_the_store() {
    let logged = built();
    let store = logged.files(".buildl/cas");
    logged.append("build.lua", b"error(\"broken\")\n");
    let expected = logged.open().check().unwrap_err().to_string();

    let error = logged.open().gc().unwrap_err();

    assert_eq!(error.to_string(), expected);
    assert_eq!(logged.files(".buildl/cas"), store);
}

#[test]
fn gc_is_refused_while_the_lock_is_held() {
    let logged = built();
    let _held = StateLock::acquire(logged.state(), logged.scratch()).unwrap();

    let error = logged.open().gc().unwrap_err();

    assert!(
        matches!(
            &error,
            Error::Adapter {
                port: PortName::ActionCache,
                ..
            }
        ),
        "{error:?}"
    );
}
