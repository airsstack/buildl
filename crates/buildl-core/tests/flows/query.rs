//! `pipeline::query` and `Pipeline::query`, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeSet;

use buildl_core::{
    Direction, EntryName, Error, Label, Pipeline, Query, StagedItem, Written, pipeline,
};

use crate::common::{FakePorts, FakeSource, alias, file, ports, target_with, workspace};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

fn asked(direction: Direction, raw: &str) -> Query {
    Query::new(direction, label(raw))
}

fn labels(raw: &[&str]) -> BTreeSet<Label> {
    raw.iter().map(|raw| label(raw)).collect()
}

/// The root file's declarations: `//:app` needing `//:main.o` and `//lib:text`, and an alias
/// `//:default` for `//:app`.
fn root_items() -> Vec<StagedItem> {
    vec![
        target_with("main.o", |target| {
            target.inputs = vec![Written::new("main.c")];
        }),
        target_with("app", |target| {
            target.deps = vec![Written::new("main.o"), Written::new("//lib:text")];
        }),
        alias("default", "app"),
    ]
}

fn lib_items() -> Vec<StagedItem> {
    vec![target_with("text", |target| {
        target.inputs = vec![Written::new("text.c")];
    })]
}

fn fixture() -> FakeSource {
    FakeSource::new(workspace(vec![
        ("", file(root_items(), &["lib"])),
        ("lib", file(lib_items(), &[])),
    ]))
}

#[test]
fn deps_answer_what_a_target_needs_across_build_files() {
    let source = fixture();
    let expected = labels(&["//:main.o", "//lib:text"]);
    for start in ["//:app", "//:default"] {
        let answer = pipeline::query(&source, &entry(), &asked(Direction::Deps, start)).unwrap();
        assert_eq!(answer, expected, "deps({start})");
    }
}

#[test]
fn rdeps_answer_what_needs_a_target_across_build_files() {
    let source = fixture();
    let answer =
        pipeline::query(&source, &entry(), &asked(Direction::Rdeps, "//lib:text")).unwrap();
    assert_eq!(answer, labels(&["//:app"]));

    let answer = pipeline::query(&source, &entry(), &asked(Direction::Rdeps, "//:app")).unwrap();
    assert_eq!(answer, labels(&[]));
}

#[test]
fn a_pipeline_answers_what_the_free_function_answers() {
    let question = asked(Direction::Rdeps, "//:main.o");
    let free = pipeline::query(&fixture(), &entry(), &question).unwrap();
    let method = Pipeline::<FakePorts>::new(ports(fixture()))
        .query(&entry(), &question)
        .unwrap();
    assert_eq!(free, labels(&["//:app"]));
    assert_eq!(method, free);
}

#[test]
fn a_target_naming_nothing_is_refused_with_the_nearest_one() {
    let error =
        pipeline::query(&fixture(), &entry(), &asked(Direction::Deps, "//lib:txt")).unwrap_err();
    match error {
        Error::UnknownTarget { label, suggestion } => {
            assert_eq!(label.to_string(), "//lib:txt");
            assert_eq!(suggestion.unwrap().to_string(), "//lib:text");
        }
        other => unreachable!("expected UnknownTarget, got {other:?}"),
    }
}

#[test]
fn a_missing_build_file_stops_the_query_before_it_is_answered() {
    let source = FakeSource::new(workspace(vec![("", file(root_items(), &["lib"]))]));
    let error = pipeline::query(&source, &entry(), &asked(Direction::Deps, "//:app")).unwrap_err();
    assert!(
        matches!(error, Error::MissingBuildFile { .. }),
        "expected MissingBuildFile, got {error:?}"
    );
}
