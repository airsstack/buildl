//! What the facade does before a build starts: opening a workspace, reading a target, answering a
//! query, and refusing a build it must not begin.

use buildl::Workspace;
use buildl::buildl_core::{Error, PortName, Want};
use buildl::report::query_text;
use buildl::state::StateLock;

use crate::common::Fixture;

/// What `raw` asks of `workspace`, one label a line.
fn answer(workspace: &Workspace, raw: &str) -> String {
    let query = workspace.query_of(raw).unwrap();
    query_text(&workspace.query(&query).unwrap())
}

#[test]
fn opening_a_workspace_writes_nothing() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let workspace = Workspace::open(chain.root()).unwrap();
    let declarations = workspace.check().unwrap();
    let graph = workspace.graph().unwrap();

    assert_eq!(workspace.root(), chain.root());
    assert_eq!(declarations.len(), 9);
    assert_eq!(graph.len(), 6);
    assert!(!chain.root().join(".buildl").exists());
    assert!(!chain.root().join("out").exists());
}

#[test]
fn a_relative_target_resolves_from_where_it_was_opened() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let workspace = Workspace::open(chain.root().join("lib")).unwrap();

    assert_eq!(workspace.root(), chain.root());
    assert_eq!(workspace.label("words").unwrap().to_string(), "//lib:words");
}

#[test]
fn a_workspace_another_buildl_holds_is_refused() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let _held = StateLock::acquire(chain.state()).unwrap();

    for refused in [chain.try_build().map(drop), chain.try_plan().map(drop)] {
        let error = refused.unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error.to_string().ends_with("is in use by another buildl"),
            "{error}"
        );
    }
    let workspace = chain.open();
    assert_eq!(workspace.check().unwrap().len(), 9);
    assert_eq!(workspace.graph().unwrap().len(), 6);
    assert!(!chain.root().join("out").exists());
}

#[test]
fn build_refuses_a_strategy_it_cannot_run_and_plan_does_not() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.replace_line("buildl.toml", "strategy = \"bare\"", "");

    let plan = chain.try_plan().unwrap();
    let error = chain.try_build().unwrap_err();

    assert_eq!(plan.len(), 6);
    assert!(
        matches!(
            error,
            Error::Adapter {
                port: PortName::Manifest,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(!chain.state().cache().exists());
    assert!(!chain.root().join("out").exists());
}

#[test]
fn a_state_directory_outside_the_ceiling_is_refused_before_it_is_made() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.replace_line(
        "buildl.toml",
        "fs.write = [\"out\", \".buildl\"]",
        "fs.write = [\"out\"]",
    );

    for refused in [chain.try_plan().map(drop), chain.try_build().map(drop)] {
        let error = refused.unwrap_err();
        assert!(
            matches!(
                error,
                Error::Refused {
                    want: Want::Write(_),
                    site: None,
                    ..
                }
            ),
            "{error:?}"
        );
    }
    assert!(!chain.root().join(".buildl").exists());
}

#[test]
fn a_query_answers_from_the_whole_graph_and_writes_nothing() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let workspace = chain.open();

    assert_eq!(
        answer(&workspace, "deps(//:report)"),
        "//:greet\n//:greeting\n//lib:words\n"
    );
    assert_eq!(
        answer(&workspace, "rdeps(//:text)"),
        "//:greeting\n//:report\n"
    );
    assert_eq!(answer(&workspace, "rdeps(//:report)"), "");
    assert!(!chain.root().join(".buildl").exists());
    assert!(!chain.root().join("out").exists());
}

#[test]
fn a_query_reads_its_target_from_where_it_was_opened() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let workspace = Workspace::open(chain.root().join("lib")).unwrap();

    assert_eq!(
        answer(&workspace, "rdeps(words)"),
        "//:greeting\n//:report\n"
    );
    assert_eq!(answer(&workspace, "deps(:assets)"), "");
}

#[test]
fn a_query_in_neither_form_is_refused_as_written() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let error = chain.open().query_of("dep(//:report)").unwrap_err();

    assert!(
        matches!(&error, Error::InvalidQuery { expression } if expression == "dep(//:report)"),
        "{error:?}"
    );
}

#[test]
fn a_query_for_a_target_nothing_declares_suggests_the_nearest() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let workspace = chain.open();
    let query = workspace.query_of("deps(//:reprt)").unwrap();

    let error = workspace.query(&query).unwrap_err();

    assert_eq!(
        error.to_string(),
        "target //:reprt was asked for but is not declared (did you mean //:report?)"
    );
}

#[test]
fn a_query_is_answered_while_another_buildl_holds_the_workspace() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let _held = StateLock::acquire(chain.state()).unwrap();

    assert_eq!(answer(&chain.open(), "deps(//:site)"), "//lib:assets\n");
}
