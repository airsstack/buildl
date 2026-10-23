//! Targets declared from a discovery result, in a real workspace.
//!
//! The fixture `discovery` has one discovery target, `//:pkgs`, whose action copies the checked-in
//! `packages.json` to its output; its build file declares one target for each name that list
//! holds, once the list has been read.

use std::fs;

use buildl::buildl_core::{Label, OutcomeClass};
use buildl::report::plan_text;

use crate::common::{Fixture, ended};

/// A copy of `discovery`, never built.
fn fresh() -> Fixture {
    Fixture::copy_of("discovery", &[])
}

/// The labels `raw` name, read by `fixture`'s workspace.
fn labels(fixture: &Fixture, raw: &[&str]) -> Vec<Label> {
    let workspace = fixture.open();
    raw.iter()
        .map(|label| workspace.label(label).unwrap())
        .collect()
}

#[test]
fn a_fresh_workspace_graph_has_no_discovered_targets_and_notes_it() {
    let discovery = fresh();

    let (graph, missing) = discovery.open().graph().unwrap();

    assert_eq!(graph.len(), 1);
    assert!(graph.id_of(&labels(&discovery, &["//:pkgs"])[0]).is_some());
    assert_eq!(missing, labels(&discovery, &["//:pkgs"]));
    assert!(!discovery.root().join(".buildl").exists());
}

#[test]
fn build_then_graph_shows_discovered_targets() {
    let discovery = fresh();

    let built = discovery.build();

    assert_eq!(
        ended(&built, &OutcomeClass::Built),
        ["//:alpha", "//:beta", "//:pkgs"]
    );
    let (graph, missing) = discovery.open().graph().unwrap();
    assert_eq!(graph.len(), 3);
    assert_eq!(missing, Vec::<Label>::new());
    let loaded = discovery.open().check().unwrap();
    assert_eq!(loaded.declarations.len(), 3);
    assert_eq!(loaded.missing, Vec::<Label>::new());
}

#[test]
fn editing_the_list_then_build_adds_a_target() {
    let discovery = fresh();
    discovery.build();
    fs::write(
        discovery.root().join("packages.json"),
        "[\"alpha\", \"beta\", \"gamma\"]\n",
    )
    .unwrap();

    let rebuilt = discovery.build();

    assert_eq!(
        ended(&rebuilt, &OutcomeClass::Built),
        ["//:gamma", "//:pkgs"]
    );
    let (graph, _) = discovery.open().graph().unwrap();
    assert_eq!(graph.len(), 4);
}

#[test]
fn gc_after_build_keeps_discovered_targets_cached() {
    let discovery = fresh();
    discovery.build();

    let collected = discovery.open().gc().unwrap();

    assert_eq!(collected.rows_dropped, 0);
    assert_eq!(
        ended(&discovery.build(), &OutcomeClass::Cached),
        ["//:alpha", "//:beta", "//:pkgs"]
    );
}

#[test]
fn check_works_with_a_damaged_log() {
    let discovery = fresh();
    discovery.build();
    fs::write(discovery.state().log(), b"not json\n\x00\xff\n{\n").unwrap();

    let loaded = discovery.open().check().unwrap();

    assert_eq!(loaded.declarations.len(), 1);
    assert_eq!(loaded.missing, labels(&discovery, &["//:pkgs"]));
}

#[test]
fn plan_notes_a_discovery_target_that_is_not_up_to_date() {
    let discovery = fresh();
    let note = "//:pkgs is not up to date; targets declared from it may change\n";

    let before = plan_text(&discovery.try_plan().unwrap());
    assert!(before.contains(note), "{before}");

    discovery.build();
    let after = plan_text(&discovery.try_plan().unwrap());
    assert!(!after.contains("not up to date"), "{after}");
}
