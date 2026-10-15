//! Running the tests of a real workspace: real `cp` and `cmp` commands, a real state directory.
//!
//! The fixture `tested` holds `copy`, a build target every test needs, and four tests of its
//! output, each there for one behaviour shown on real commands:
//!
//! - `same` compares equal files and declares no output, so it writes no file and must still
//!   pass and be cached;
//! - `differs` compares different files, so `cmp` exits 1, and it is never cached;
//! - `unreported` declares `report.txt` and exits 0 without writing it, so it is held to its
//!   declaration;
//! - `flagged` reads `flag.txt` without declaring it, so editing that file does not change its
//!   key (isolation tier 1 does not see an undeclared read), and only a rerun can show the
//!   edit.

use buildl::buildl_core::{ActionOutcome, OutcomeClass, Rerun};

use crate::common::{Fixture, ended};

/// How the outcome of `label` ended.
fn class(outcomes: &[ActionOutcome], label: &str) -> OutcomeClass {
    outcomes
        .iter()
        .find(|outcome| outcome.label.to_string() == label)
        .unwrap_or_else(|| panic!("no outcome for {label}"))
        .class
        .clone()
}

#[test]
fn a_passing_test_with_no_outputs_passes_and_is_cached_next_run() {
    let tested = Fixture::copy_of("tested", &[]);

    let first = tested.test(&["same"], Rerun::Off);
    let second = tested.test(&["same"], Rerun::Off);

    assert_eq!(class(&first, "//:same"), OutcomeClass::Built);
    assert_eq!(class(&second, "//:same"), OutcomeClass::Cached);
}

#[test]
fn a_failing_test_is_never_cached() {
    let tested = Fixture::copy_of("tested", &[]);

    let first = tested.test(&["differs"], Rerun::Off);
    let second = tested.test(&["differs"], Rerun::Off);

    let failed = OutcomeClass::ActionFailure { status: 1 };
    assert_eq!(class(&first, "//:differs"), failed);
    assert_eq!(class(&second, "//:differs"), failed);
    assert!(!tested.recorded().contains(&"//:differs".to_owned()));
}

#[test]
fn a_test_that_declares_an_output_is_held_to_it() {
    let tested = Fixture::copy_of("tested", &[]);

    let outcomes = tested.test(&["unreported"], Rerun::Off);

    let OutcomeClass::OutputMissing { name } = class(&outcomes, "//:unreported") else {
        panic!("//:unreported did not end as OutputMissing");
    };
    assert_eq!(name.to_string(), "report.txt");
}

#[test]
fn a_build_of_the_workspace_runs_no_test() {
    let tested = Fixture::copy_of("tested", &[]);

    let outcomes = tested.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built), ["//:copy"]);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(tested.log_lines().len(), 1);
}

#[test]
fn a_failed_rerun_runs_again_next_time() {
    let tested = Fixture::copy_of("tested", &[]);

    let first = tested.test(&["flagged"], Rerun::Off);
    std::fs::write(tested.root().join("flag.txt"), "b\n").unwrap();
    let unchanged = tested.test(&["flagged"], Rerun::Off);
    let rerun = tested.test(&["flagged"], Rerun::Tests);
    let rows_after_rerun = tested.recorded();
    let next = tested.test(&["flagged"], Rerun::Off);

    let failed = OutcomeClass::ActionFailure { status: 1 };
    assert_eq!(class(&first, "//:flagged"), OutcomeClass::Built);
    assert_eq!(class(&unchanged, "//:flagged"), OutcomeClass::Cached);
    assert_eq!(class(&rerun, "//:flagged"), failed);
    assert!(!rows_after_rerun.contains(&"//:flagged".to_owned()));
    assert_eq!(class(&next, "//:flagged"), failed);
}
