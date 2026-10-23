//! `Pipeline::test`, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    ActionOutcome, Digest, EntryName, Error, LiveOutput, OnFailure, OutcomeClass, Pipeline, Rerun,
    Scope, TargetRole, Written,
};

use crate::common::{
    Answer, Ends, FakeApprover, FakePorts, classes, ending, expected, label, root_only, script,
    skipped, target, target_with,
};

/// Two build targets, `//:app` and `//:docs`, and two tests: `//:app_test`, which needs
/// `//:app`, and `//:unit`, which needs nothing.
fn suite() -> Pipeline<FakePorts> {
    let test = |name: &str, deps: &[&str]| {
        target_with(name, |staged| {
            staged.role = TargetRole::Test;
            staged.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
        })
    };
    root_only(vec![
        target("app", &[]),
        target("docs", &[]),
        test("app_test", &["app"]),
        test("unit", &[]),
    ])
}

/// A test run of `scope` with no setting overridden.
fn tested(
    pipeline: &mut Pipeline<FakePorts>,
    scope: &Scope,
    on_failure: OnFailure,
    rerun: Rerun,
) -> Vec<ActionOutcome> {
    pipeline
        .test(
            &EntryName::parse("build.lua").unwrap(),
            scope,
            &BTreeMap::new(),
            on_failure,
            LiveOutput::Off,
            rerun,
        )
        .unwrap()
}

/// The key digest of the last event logged for the target `raw` names.
fn key_of(pipeline: &Pipeline<FakePorts>, raw: &str) -> Option<Digest> {
    let wanted = label(raw);
    pipeline
        .ports()
        .log
        .events
        .iter()
        .rev()
        .find(|event| event.outcome.label == wanted)
        .unwrap()
        .key
}

#[test]
fn a_test_run_records_the_whole_graph_and_runs_only_tests() {
    let mut pipeline = suite();

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:app_test", OutcomeClass::Built),
            ("//:unit", OutcomeClass::Built),
        ])
    );
    let ports = pipeline.ports();
    assert!(!ports.strategy.ran().contains(&label("//:docs")));
    assert_eq!(ports.graph.recorded.len(), 1);
    let recorded = String::from_utf8(ports.graph.recorded[0].clone()).unwrap();
    assert!(recorded.contains("//:docs"), "the whole graph is recorded");
}

#[test]
fn a_test_run_returns_each_outcome_with_its_role() {
    let mut pipeline = suite();

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    let roles: Vec<(String, TargetRole)> = outcomes
        .iter()
        .map(|outcome| (outcome.label.to_string(), outcome.role))
        .collect();
    assert_eq!(
        roles,
        [
            ("//:app".to_owned(), TargetRole::Build),
            ("//:app_test".to_owned(), TargetRole::Test),
            ("//:unit".to_owned(), TargetRole::Test),
        ]
    );
}

#[test]
fn a_test_run_keeps_going_past_a_failing_test() {
    let mut pipeline = suite();
    script(&mut pipeline, "//:unit", ending(Ends::Exits(1)));

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:app_test", OutcomeClass::Built),
            ("//:unit", OutcomeClass::ActionFailure { status: 1 }),
        ])
    );
}

#[test]
fn a_stopped_test_run_starts_nothing_new_after_a_failure() {
    let mut pipeline = suite();
    script(&mut pipeline, "//:app", ending(Ends::Exits(1)));

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::Stop,
        Rerun::Off,
    );

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::ActionFailure { status: 1 }),
            ("//:app_test", skipped("//:app")),
            ("//:unit", skipped("//:app")),
        ])
    );
    assert_eq!(pipeline.ports().strategy.ran(), [label("//:app")]);
}

#[test]
fn a_failed_dependency_skips_the_tests_above_it() {
    let mut pipeline = suite();
    script(&mut pipeline, "//:app", ending(Ends::Exits(1)));

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::ActionFailure { status: 1 }),
            ("//:app_test", skipped("//:app")),
            ("//:unit", OutcomeClass::Built),
        ])
    );
}

/// Three runs of the workspace: `Off`, `Off`, then `Rerun::Tests`. Returns the pipeline, the
/// third run's outcomes, and the keys the first run logged for `//:app_test` and `//:unit`.
fn three_runs() -> (Pipeline<FakePorts>, Vec<ActionOutcome>, [Option<Digest>; 2]) {
    let mut pipeline = suite();
    let scope = Scope::Workspace;
    tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Off);
    let first_keys = [
        key_of(&pipeline, "//:app_test"),
        key_of(&pipeline, "//:unit"),
    ];
    assert!(first_keys.iter().all(Option::is_some));
    let second = tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Off);
    assert!(
        second
            .iter()
            .all(|outcome| outcome.class == OutcomeClass::Cached),
        "the second run is served from the cache"
    );
    let third = tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Tests);
    (pipeline, third, first_keys)
}

#[test]
fn a_rerun_runs_a_cached_test_again_and_records_it() {
    let (pipeline, third, first_keys) = three_runs();

    assert_eq!(
        classes(&third),
        expected(&[
            ("//:app", OutcomeClass::Cached),
            ("//:app_test", OutcomeClass::Built),
            ("//:unit", OutcomeClass::Built),
        ])
    );
    let ports = pipeline.ports();
    for raw in ["//:app_test", "//:unit"] {
        assert_eq!(ports.strategy.runs(&label(raw)), 2, "{raw} ran twice");
        let recorded = ports
            .cache
            .recorded()
            .iter()
            .filter(|recorded| **recorded == label(raw))
            .count();
        assert_eq!(recorded, 2, "{raw} was recorded twice");
    }
    assert_eq!(
        [
            key_of(&pipeline, "//:app_test"),
            key_of(&pipeline, "//:unit")
        ],
        first_keys,
        "a rerun is not a key component"
    );
}

#[test]
fn a_rerun_does_not_rebuild_a_cached_dependency() {
    let (pipeline, third, _) = three_runs();

    assert_eq!(classes(&third)["//:app"], OutcomeClass::Cached);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:app")), 1);
}

#[test]
fn a_failed_rerun_forgets_the_row_so_the_next_run_runs_it() {
    let mut pipeline = suite();
    let scope = Scope::Workspace;
    tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Off);
    script(&mut pipeline, "//:unit", ending(Ends::Exits(1)));

    let failed = tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Tests);

    assert_eq!(
        classes(&failed)["//:unit"],
        OutcomeClass::ActionFailure { status: 1 }
    );
    let cache = &pipeline.ports().cache;
    assert_eq!(cache.forgotten(), [label("//:unit")]);
    assert!(!cache.rows.contains_key(&label("//:unit")));
    script(&mut pipeline, "//:unit", ending(Ends::Exits(0)));

    let next = tested(&mut pipeline, &scope, OnFailure::KeepGoing, Rerun::Off);

    assert_eq!(classes(&next)["//:unit"], OutcomeClass::Built);
}

#[test]
fn a_test_run_asks_the_approver_once() {
    let mut pipeline = suite();

    tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    assert_eq!(pipeline.ports().approver.asked.len(), 1);
}

#[test]
fn a_denied_test_run_runs_nothing() {
    let mut pipeline = suite();
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Deny("no".to_owned()));

    let error = pipeline
        .test(
            &EntryName::parse("build.lua").unwrap(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::KeepGoing,
            LiveOutput::Off,
            Rerun::Off,
        )
        .unwrap_err();

    assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
    assert_eq!(pipeline.ports().approver.asked.len(), 1);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.handed(), []);
    assert_eq!(ports.cache.recorded(), []);
    assert_eq!(ports.log.events, []);
}

#[test]
fn a_test_run_naming_a_discovery_target_is_not_a_test() {
    let mut pipeline = root_only(vec![target_with("pkgs", |staged| {
        staged.role = TargetRole::Discovery;
    })]);

    let error = pipeline
        .test(
            &EntryName::parse("build.lua").unwrap(),
            &Scope::Targets(BTreeSet::from([label("//:pkgs")])),
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
            Rerun::Off,
        )
        .unwrap_err();

    assert!(matches!(error, Error::NotATest { .. }), "{error:?}");
    assert_eq!(pipeline.ports().strategy.ran(), []);
}

#[test]
fn a_workspace_test_run_leaves_discovery_targets_out() {
    let mut pipeline = root_only(vec![
        target_with("pkgs", |staged| staged.role = TargetRole::Discovery),
        target_with("unit", |staged| staged.role = TargetRole::Test),
    ]);

    let outcomes = tested(
        &mut pipeline,
        &Scope::Workspace,
        OnFailure::KeepGoing,
        Rerun::Off,
    );

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:unit", OutcomeClass::Built)])
    );
}
