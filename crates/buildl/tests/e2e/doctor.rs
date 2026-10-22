//! `doctor` through the facade: what it reports under each approver, and that it leaves the
//! workspace and the remembered approvals as it found them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use buildl::buildl_core::{EnvName, Error, WantedSet};

use crate::common::{Fake, Fixture};

/// A copy of `chain` whose approver is `mode`.
fn chain(mode: &str) -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.approver(mode);
    chain
}

/// Every file below the root of `fixture`, with its bytes.
fn tree(fixture: &Fixture) -> BTreeMap<String, Vec<u8>> {
    fixture
        .files(".")
        .into_iter()
        .map(|path| {
            let bytes = fs::read(fixture.root().join(&path)).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn doctor_writes_nothing() {
    let chain = chain("interactive");
    let before = tree(&chain);

    chain.open().doctor().unwrap();

    assert_eq!(tree(&chain), before);
    assert!(!chain.root().join(".buildl").exists());
    assert!(!chain.approvals_file().exists());
}

#[test]
fn doctor_under_manifest_approves_everything() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let doctor = chain.open().doctor().unwrap();

    assert!(!doctor.checkup().wanted().is_empty());
    assert!(doctor.checkup().outside().is_empty());
    assert_eq!(doctor.unapproved(), &WantedSet::default());
    assert_eq!(doctor.root(), chain.root());
}

#[test]
fn doctor_under_deny_approves_nothing() {
    let chain = chain("deny");

    let doctor = chain.open().doctor().unwrap();

    assert!(!doctor.checkup().wanted().is_empty());
    assert_eq!(doctor.unapproved(), doctor.checkup().wanted());
}

#[test]
fn doctor_under_interactive_reads_the_remembered_approvals() {
    let chain = chain("interactive");
    assert_eq!(
        chain.open().doctor().unwrap().unapproved(),
        chain.open().doctor().unwrap().checkup().wanted()
    );
    chain
        .try_build_asking(&mut Fake::answering(&["y"]))
        .unwrap();
    assert_eq!(
        chain.open().doctor().unwrap().unapproved(),
        &WantedSet::default()
    );
    // A target now reads one more variable, which the ceiling allows.
    chain.replace_line(
        "buildl.toml",
        "env.read = [\"BUILDL_FIXTURE_FLAVOR\"]",
        "env.read = [\"BUILDL_FIXTURE_FLAVOR\", \"X\"]",
    );
    chain.replace_line(
        "build.lua",
        "  env    = { \"BUILDL_FIXTURE_FLAVOR\" },",
        "  env    = { \"BUILDL_FIXTURE_FLAVOR\", \"X\" },",
    );

    let doctor = chain.open().doctor().unwrap();

    assert_eq!(
        doctor.unapproved(),
        &WantedSet {
            env: BTreeSet::from([EnvName::parse("X").unwrap()]),
            ..WantedSet::default()
        }
    );
}

#[test]
fn doctor_with_a_corrupt_approvals_file_is_corrupt_state() {
    let chain = chain("interactive");
    let file = chain.approvals_file();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "not what buildl writes").unwrap();

    let error = chain.open().doctor().unwrap_err();

    assert!(matches!(error, Error::CorruptState { .. }), "{error:?}");
    assert!(error.to_string().contains(&file.display().to_string()));
}
