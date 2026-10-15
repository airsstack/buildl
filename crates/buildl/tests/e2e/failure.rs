//! What one failing action does to the rest of a build, and to the action cache.
//!
//! The fixture `failing` runs one action at a time, and its failing target, `//:bad`, comes
//! first in label order among the targets that can start. So a build that stops at the first
//! failure has begun nothing else when it stops.

use buildl::buildl_core::{Label, OnFailure, OutcomeClass, Summary, Tally};

use crate::common::{Fixture, ended};

/// What a build shows for `//:bad`. Its command prints nothing, so this is all of it.
const BAD: &str = "FAILED //:bad: action failure (exit status 1)\n    \
                   declared in build.lua\n    \
                   command: false\n";

/// The class of a target that was never begun because `//:bad` failed.
fn skipped_for_bad() -> OutcomeClass {
    OutcomeClass::Skipped {
        because: Label::parse("//:bad").unwrap(),
    }
}

#[test]
fn a_failure_stops_the_build_by_default() {
    let failing = Fixture::copy_of("failing", &[]);

    let (outcomes, shown) = failing.build_shown(OnFailure::Stop);

    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 1,
                skipped: 4,
                built: 0,
                cached: 0
            },
            tests: Tally::default()
        }
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::ActionFailure { status: 1 }),
        ["//:bad"]
    );
    assert_eq!(
        ended(&outcomes, &skipped_for_bad()),
        ["//:after", "//:after2", "//:good", "//:good2"]
    );
    assert_eq!(shown, format!("{BAD}1 failed, 4 skipped\n"));
    assert_eq!(failing.files("out"), Vec::<String>::new());
    assert_eq!(failing.recorded(), Vec::<String>::new());
}

#[test]
fn keep_going_builds_what_does_not_depend_on_the_failure() {
    let failing = Fixture::copy_of("failing", &[]);

    let (outcomes, shown) = failing.build_shown(OnFailure::KeepGoing);

    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 1,
                skipped: 2,
                built: 2,
                cached: 0
            },
            tests: Tally::default()
        }
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        ["//:good", "//:good2"]
    );
    assert_eq!(
        ended(&outcomes, &skipped_for_bad()),
        ["//:after", "//:after2"]
    );
    assert_eq!(shown, format!("{BAD}1 failed, 2 skipped, 2 built\n"));
    assert_eq!(failing.files("out"), ["out/good", "out/good2"]);
    assert_eq!(failing.recorded(), ["//:good", "//:good2"]);
}

#[test]
fn a_failed_target_runs_again_and_leaves_no_row() {
    let failing = Fixture::copy_of("failing", &[]);
    failing.build_shown(OnFailure::KeepGoing);

    let (outcomes, shown) = failing.build_shown(OnFailure::KeepGoing);

    assert_eq!(
        ended(&outcomes, &OutcomeClass::ActionFailure { status: 1 }),
        ["//:bad"]
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Cached),
        ["//:good", "//:good2"]
    );
    assert_eq!(
        ended(&outcomes, &skipped_for_bad()),
        ["//:after", "//:after2"]
    );
    assert_eq!(shown, format!("{BAD}1 failed, 2 skipped, 2 cached\n"));
    assert_eq!(failing.recorded(), ["//:good", "//:good2"]);
}
