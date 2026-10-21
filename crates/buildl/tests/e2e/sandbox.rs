//! Building and testing under the input sandbox, the default strategy, on real commands.
//!
//! The fixture `sandboxed` holds four targets, each there for one behaviour:
//!
//! - `visible` copies its declared input, so it builds under either strategy;
//! - `hidden` copies a file it did not declare, so it builds under `bare` and fails under
//!   `sandbox`;
//! - `partial` writes its output and then fails, and `slow` writes its output and then outlasts
//!   the timeout: neither may leave anything in the output directory.
//!
//! `partial` and `slow` fail the build, so they are only ever selected by name.

use std::env;
use std::fs;

use buildl::buildl_core::{ActionOutcome, InfraKind, Isolation, OutcomeClass, Rerun};

use crate::common::{CHAIN_LABELS, Fixture, ended};

/// How the one outcome in `outcomes` ended.
fn only(outcomes: &[ActionOutcome]) -> OutcomeClass {
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    outcomes[0].class.clone()
}

/// How the outcome of `label` ended.
fn class(outcomes: &[ActionOutcome], label: &str) -> OutcomeClass {
    outcomes
        .iter()
        .find(|outcome| outcome.label.to_string() == label)
        .unwrap_or_else(|| panic!("no outcome for {label}"))
        .class
        .clone()
}

/// A copy of `chain` that builds under `isolation`: the manifest's `bare` line stays for
/// [`Isolation::Bare`] and goes for the default, [`Isolation::Sandbox`].
fn chain(isolation: Isolation) -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    match isolation {
        Isolation::Bare => {}
        Isolation::Sandbox => chain.replace_line("buildl.toml", "strategy = \"bare\"", ""),
        other => panic!("no copy of the chain builds under {other}"),
    }
    chain
}

#[test]
fn the_default_manifest_builds_the_chain() {
    let chain = chain(Isolation::Sandbox);

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built), CHAIN_LABELS);
    assert_eq!(outcomes.len(), CHAIN_LABELS.len());
}

#[test]
fn an_undeclared_input_fails_under_sandbox_and_builds_under_bare() {
    let sandboxed = Fixture::copy_of("sandboxed", &[]);

    let outcomes = sandboxed.build_targets(&["hidden"]);

    assert!(
        matches!(only(&outcomes), OutcomeClass::ActionFailure { status } if status != 0),
        "{outcomes:?}"
    );

    sandboxed.append("buildl.toml", b"strategy = \"bare\"\n");
    let outcomes = sandboxed.build_targets(&["hidden"]);

    assert_eq!(only(&outcomes), OutcomeClass::Built);
}

#[test]
fn outputs_are_byte_identical_under_both_tiers() {
    let bare = chain(Isolation::Bare);
    let sandbox = chain(Isolation::Sandbox);

    bare.build();
    sandbox.build();

    assert_eq!(bare.snapshot().first_difference(&sandbox.snapshot()), None);
}

#[test]
fn a_target_built_under_bare_is_cached_under_sandbox() {
    let chain = chain(Isolation::Bare);
    chain.build();
    chain.replace_line("buildl.toml", "strategy = \"bare\"", "");

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), CHAIN_LABELS);
    assert_eq!(outcomes.len(), CHAIN_LABELS.len());
}

#[test]
fn a_failed_or_timed_out_action_leaves_nothing_in_out() {
    let sandboxed = Fixture::copy_of("sandboxed", &[]);

    let partial = sandboxed.build_targets(&["partial"]);
    let slow = sandboxed.build_targets(&["slow"]);

    assert_eq!(only(&partial), OutcomeClass::ActionFailure { status: 1 });
    assert_eq!(
        only(&slow),
        OutcomeClass::InfraFailure {
            kind: InfraKind::TimedOut
        }
    );
    assert!(!sandboxed.root().join("out/partial").exists());
    assert!(!sandboxed.root().join("out/slow").exists());
}

#[test]
fn leftovers_are_removed_and_nothing_is_left_after_a_build() {
    let sandboxed = Fixture::copy_of("sandboxed", &[]);
    let stale = sandboxed.scratch().dir().join("stale");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("file"), "left by a build that was killed").unwrap();

    let outcomes = sandboxed.build_targets(&["visible"]);

    assert_eq!(only(&outcomes), OutcomeClass::Built);
    assert!(!sandboxed.scratch().dir().exists());
}

#[test]
fn tests_run_sandboxed() {
    let tested = Fixture::copy_of("tested", &[]);
    tested.replace_line("buildl.toml", "strategy = \"bare\"", "");

    let outcomes = tested.test(&["same", "flagged"], Rerun::Off);

    assert_eq!(class(&outcomes, "//:same"), OutcomeClass::Built);
    // It reads `flag.txt`, which it did not declare, so the file is not there to read.
    assert!(
        matches!(
            class(&outcomes, "//:flagged"),
            OutcomeClass::ActionFailure { .. }
        ),
        "{outcomes:?}"
    );

    let bare = Fixture::copy_of("tested", &[]);
    let outcomes = bare.test(&["same", "flagged"], Rerun::Off);

    assert_eq!(class(&outcomes, "//:same"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:flagged"), OutcomeClass::Built);
}

#[test]
fn no_log_line_names_the_scratch_directory() {
    let sandboxed = Fixture::copy_of("sandboxed", &[]);
    let written = sandboxed.scratch().dir().to_path_buf();
    // On macOS the temporary directory is reached through a link: `/var` is `/private/var`.
    let canonical = fs::canonicalize(env::temp_dir())
        .unwrap()
        .join(written.file_name().unwrap());

    // A file where the output directory belongs: the sandbox cannot prepare the action, and the
    // log row carries the diagnostic it made.
    fs::write(sandboxed.root().join("out"), "in the way").unwrap();

    let outcomes = sandboxed.build_targets(&["visible"]);

    assert_eq!(
        only(&outcomes),
        OutcomeClass::InfraFailure {
            kind: InfraKind::Prepare
        }
    );
    let lines = sandboxed.log_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("making the directory of out/visible")),
        "{lines:?}"
    );
    for line in &lines {
        for place in [&written, &canonical] {
            assert!(
                !line.contains(place.to_str().unwrap()),
                "{line} names {}",
                place.display()
            );
        }
    }
}
