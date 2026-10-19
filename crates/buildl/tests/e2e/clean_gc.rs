//! Cleaning the output directory of a real workspace: what `clean` removes, what it leaves, and
//! when it refuses.
//!
//! The fixture `logged` is used because every one of its targets succeeds, so a build of it
//! leaves a whole output directory and a whole store.

use std::fs;

use buildl::buildl_core::{Error, OutcomeClass};
use buildl::state::Cleaned;

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
