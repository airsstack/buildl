//! What a build after the first one runs: nothing when nothing changed, and otherwise what the
//! change reaches.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use buildl::buildl_core::{LiveOutput, OnFailure, OutcomeClass, Scope, SettingName, SettingValue};
use buildl::report::Silent;

use crate::common::{CHAIN_LABELS, CHAIN_OUTPUTS, Fixture, ended};

/// A copy of the fixture `chain`, built once.
fn built_chain() -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.build();
    chain
}

#[test]
fn a_second_build_runs_nothing() {
    let chain = built_chain();
    let first = chain.snapshot();
    assert_eq!(chain.log_lines().len(), 6);

    let (outcomes, shown) = chain.build_shown(OnFailure::Stop);

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), CHAIN_LABELS);
    assert_eq!(shown, "6 cached\n");
    assert_eq!(first.first_difference(&chain.snapshot()), None);
    assert_eq!(chain.log_lines().len(), 12);
}

#[test]
fn deleted_outputs_are_put_back_without_running() {
    let chain = built_chain();
    let first = chain.snapshot();
    fs::remove_dir_all(chain.root().join("out")).unwrap();

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), CHAIN_LABELS);
    assert_eq!(chain.files("out"), CHAIN_OUTPUTS);
    assert_eq!(first.first_difference(&chain.snapshot()), None);
    let greet = fs::metadata(chain.root().join("out/greet")).unwrap();
    assert_ne!(greet.permissions().mode() & 0o111, 0);
}

#[test]
fn an_edited_input_rebuilds_its_dependents_only() {
    let chain = built_chain();
    chain.append("lib/words/b.txt", b"kiwi\n");

    let outcomes = chain.build();

    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        ["//:greeting", "//:report", "//lib:words"]
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Cached),
        ["//:greet", "//:site", "//lib:assets"]
    );
    assert_eq!(
        fs::read(chain.root().join("out/report")).unwrap(),
        b"apple\nfig\nkiwi\npear\nwords:\nzzz\n"
    );
}

#[test]
fn an_edit_that_leaves_the_output_unchanged_stops_there() {
    let chain = built_chain();
    let first = chain.snapshot();
    chain.append("lib/words/a.txt", b"pear\n");

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built), ["//lib:words"]);
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Cached),
        [
            "//:greet",
            "//:greeting",
            "//:report",
            "//:site",
            "//lib:assets"
        ]
    );
    // Every output holds the bytes it held: the first difference is past the outputs, in the
    // action cache.
    assert_eq!(
        first.first_difference(&chain.snapshot()).as_deref(),
        Some(".buildl/cache.json differs")
    );
    assert_eq!(
        fs::read(chain.root().join("out/lib/words")).unwrap(),
        b"apple\nfig\npear\n"
    );
}

#[test]
fn a_setting_override_rebuilds_what_names_it() {
    let chain = built_chain();
    let overrides = BTreeMap::from([(
        SettingName::parse("order").unwrap(),
        SettingValue::parse("-r").unwrap(),
    )]);

    let outcomes = chain
        .open()
        .build(
            &Scope::Workspace,
            &overrides,
            OnFailure::Stop,
            LiveOutput::Off,
            &mut Silent,
        )
        .unwrap();

    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        ["//:greeting", "//:report", "//lib:words"]
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Cached),
        ["//:greet", "//:site", "//lib:assets"]
    );
    assert_eq!(
        fs::read(chain.root().join("out/lib/words")).unwrap(),
        b"pear\nfig\napple\napple\n"
    );
}
