//! Load, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;
use std::path::Path;

use buildl_core::{
    Declaration, DeclarationField, Declared, Direction, Directory, EntryName, Error,
    EvaluationFailure, EvaluationLimit, Label, LiveOutput, OnFailure, Pipeline, Query, Scope, json,
    load, pipeline,
};

use crate::common::{
    FakeDispatcher, FakeFile, FakeHistory, FakePorts, FakeSource, Loss, Order, Pace, dir, file,
    root_only, setting, target, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn names(source: &FakeSource) -> Vec<String> {
    load(source, &mut FakeDispatcher::default(), &entry())
        .unwrap()
        .iter()
        .map(|declaration| match declaration.item() {
            Declared::Target(target) => target.label.to_string(),
            Declared::Setting(setting) => format!("setting {}", setting.name),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn every_reached_file_yields_one_sorted_list_of_declarations() {
    let source = FakeSource::new(workspace(vec![
        (
            "",
            file(vec![target("app", &["main.o", "//lib:text"])], &["lib"]),
        ),
        ("lib", file(vec![target("text", &[])], &[])),
    ]));
    assert_eq!(names(&source), ["//:app", "//lib:text"]);
}

#[test]
fn subdirs_requested_out_of_order_still_sort_by_directory_then_name() {
    let source = FakeSource::new(workspace(vec![
        (
            "",
            file(vec![target("z", &[]), target("a", &[])], &["zeta", "alpha"]),
        ),
        ("zeta", file(vec![target("m", &[])], &[])),
        ("alpha", file(vec![setting("b"), target("a", &[])], &[])),
    ]));
    assert_eq!(
        names(&source),
        ["//:a", "//:z", "//alpha:a", "setting b", "//zeta:m"]
    );
}

#[test]
fn a_shuffled_file_loads_to_byte_identical_canonical_json() {
    let ordered = FakeSource::new(workspace(vec![
        (
            "",
            file(
                vec![target("a", &[]), target("b", &["a"]), setting("s")],
                &["x", "y"],
            ),
        ),
        ("x", file(vec![], &[])),
        ("y", file(vec![], &[])),
    ]));
    let shuffled = FakeSource::new(workspace(vec![
        (
            "",
            file(
                vec![setting("s"), target("b", &["a"]), target("a", &[])],
                &["y", "x"],
            ),
        ),
        ("x", file(vec![], &[])),
        ("y", file(vec![], &[])),
    ]));
    let a =
        json::canonical::to_vec(&load(&ordered, &mut FakeDispatcher::default(), &entry()).unwrap())
            .unwrap();
    let b = json::canonical::to_vec(
        &load(&shuffled, &mut FakeDispatcher::default(), &entry()).unwrap(),
    )
    .unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_directory_requested_twice_is_evaluated_once() {
    // The root reaches `a/x` directly and again through `a`. A cycle cannot be written at all: a
    // `subdir` path is relative to its own directory and cannot climb out of it.
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![], &["a", "a/x", "a"])),
        ("a", file(vec![], &["x"])),
        ("a/x", file(vec![], &[])),
    ]));
    load(&source, &mut FakeDispatcher::default(), &entry()).unwrap();
    assert_eq!(source.evaluated(), [dir(""), dir("a"), dir("a/x")]);
}

#[test]
fn a_subdir_cannot_leave_the_workspace() {
    for escape in ["../x", "/x"] {
        let source = FakeSource::new(workspace(vec![
            ("", file(vec![], &["lib"])),
            ("lib", file(vec![], &[escape])),
        ]));
        match load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err() {
            Error::InvalidDeclaration {
                provenance, field, ..
            } => {
                assert_eq!(field, DeclarationField::Subdir);
                assert_eq!(provenance.file(), Path::new("lib/build.lua"));
            }
            other => unreachable!("expected InvalidDeclaration, got {other:?}"),
        }
    }
}

#[test]
fn a_missing_root_names_no_requester() {
    let source = FakeSource::new(workspace(vec![]));
    match load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err() {
        Error::MissingBuildFile {
            directory,
            requested_by,
        } => {
            assert!(directory.is_root());
            assert!(requested_by.is_none());
        }
        other => unreachable!("expected MissingBuildFile, got {other:?}"),
    }
}

#[test]
fn a_missing_subdir_names_the_file_that_requested_it() {
    let source = FakeSource::new(workspace(vec![("", file(vec![], &["lib"]))]));
    let error = load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "no build file in //lib (requested by build.lua)"
    );
}

#[test]
fn the_requester_of_a_shared_missing_directory_does_not_depend_on_call_order() {
    // `a` and `a/b` both request `a/b/c`, which does not exist; the root's calls come in either
    // order, and the requester named is the same.
    for root_calls in [["a/b", "a"], ["a", "a/b"]] {
        let source = FakeSource::new(workspace(vec![
            ("", file(vec![], &root_calls)),
            ("a", file(vec![], &["b/c"])),
            ("a/b", file(vec![], &["c"])),
        ]));
        let error = load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "no build file in //a/b/c (requested by a/build.lua)"
        );
    }
}

#[test]
fn an_evaluation_failure_passes_through_unchanged() {
    let failure = EvaluationFailure::LimitReached {
        limit: EvaluationLimit::Staging,
    };
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![], &["lib"])),
        ("lib", FakeFile::Fails(failure.clone())),
    ]));
    match load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err() {
        Error::Evaluation {
            provenance,
            failure: found,
            ..
        } => {
            assert_eq!(found, failure);
            assert_eq!(provenance.file(), Path::new("lib/build.lua"));
        }
        other => unreachable!("expected Evaluation, got {other:?}"),
    }
}

/// Every way the fake dispatcher can order and pace the tasks it is handed.
fn dispatchers() -> Vec<FakeDispatcher> {
    let mut all = Vec::new();
    for order in [Order::OldestFirst, Order::NewestFirst] {
        for pace in [Pace::OneAtATime, Pace::Together] {
            all.push(FakeDispatcher {
                order,
                pace,
                ..FakeDispatcher::default()
            });
        }
    }
    all
}

const fn failing(limit: EvaluationLimit) -> FakeFile {
    FakeFile::Fails(EvaluationFailure::LimitReached { limit })
}

/// Seven directories at two depths.
fn wide() -> BTreeMap<Directory, FakeFile> {
    workspace(vec![
        (
            "",
            file(
                vec![target("app", &["//lib/core:base"])],
                &["tools", "lib", "app"],
            ),
        ),
        ("lib", file(vec![target("lib", &[])], &["util", "core"])),
        ("lib/core", file(vec![target("base", &[])], &[])),
        ("lib/util", file(vec![setting("mode")], &[])),
        ("app", file(vec![target("ui", &[])], &["view"])),
        ("app/view", file(vec![target("page", &["//app:ui"])], &[])),
        ("tools", file(vec![target("gen", &[])], &[])),
    ])
}

#[test]
fn every_order_and_pace_loads_the_same_declarations() {
    let expected: Vec<Declaration> = load(
        &FakeSource::new(wide()),
        &mut FakeDispatcher::default(),
        &entry(),
    )
    .unwrap();
    assert_eq!(expected.len(), 7);
    for mut dispatcher in dispatchers() {
        let how = format!("{:?} {:?}", dispatcher.order, dispatcher.pace);
        let source = FakeSource::new(wide());
        let found = load(&source, &mut dispatcher, &entry()).unwrap();
        assert_eq!(found, expected, "{how}");
        assert_eq!(source.evaluated().len(), 7, "{how}: every file, once");
    }
}

#[test]
fn every_order_and_pace_reports_the_same_error() {
    // `lib/core` and `app/view` fail, and `tools/gen` is missing; each is reached through a
    // parent that loads, so the error reported is the dispatcher's to choose unless results are
    // taken in serial order.
    let files = || {
        workspace(vec![
            ("", file(vec![target("app", &[])], &["tools", "lib", "app"])),
            ("lib", file(vec![], &["core", "util"])),
            ("lib/core", failing(EvaluationLimit::Staging)),
            ("lib/util", file(vec![], &[])),
            ("app", file(vec![], &["view"])),
            ("app/view", failing(EvaluationLimit::Memory)),
            ("tools", file(vec![], &["gen"])),
        ])
    };
    let failure = |dispatcher: &mut FakeDispatcher| {
        let error = load(&FakeSource::new(files()), dispatcher, &entry()).unwrap_err();
        (error.to_string(), format!("{error:?}"))
    };
    let expected = failure(&mut FakeDispatcher::default());
    for mut dispatcher in dispatchers() {
        let how = format!("{:?} {:?}", dispatcher.order, dispatcher.pace);
        assert_eq!(failure(&mut dispatcher), expected, "{how}");
    }
}

#[test]
fn a_task_taken_after_the_error_evaluates_nothing() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![], &["a", "b", "c"])),
        ("a", failing(EvaluationLimit::Staging)),
        ("b", file(vec![], &[])),
        ("c", file(vec![], &[])),
    ]));
    match load(&source, &mut FakeDispatcher::default(), &entry()).unwrap_err() {
        Error::Evaluation { provenance, .. } => {
            assert_eq!(provenance.file(), Path::new("a/build.lua"));
        }
        other => unreachable!("expected Evaluation, got {other:?}"),
    }
    assert_eq!(source.evaluated(), [dir(""), dir("a")]);
}

#[test]
fn a_lost_load_task_is_lost_build_files() {
    for loss in [Loss::Unreported, Loss::Unrun] {
        let source = FakeSource::new(workspace(vec![
            ("", file(vec![], &["a", "b"])),
            ("a", file(vec![], &[])),
            ("b", file(vec![], &[])),
        ]));
        let mut dispatcher = FakeDispatcher {
            lose: BTreeMap::from([(1, loss)]),
            ..FakeDispatcher::default()
        };
        match load(&source, &mut dispatcher, &entry()).unwrap_err() {
            Error::LostBuildFiles { directories } => {
                assert_eq!(directories, [dir("a")], "{loss:?}");
            }
            other => unreachable!("{loss:?}: expected LostBuildFiles, got {other:?}"),
        }
    }
}

/// A pipeline that has built `//:old`, a target [`wide`] does not declare.
fn built() -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![target("old", &[])]);
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();
    pipeline
}

#[test]
fn check_graph_query_and_gc_load_in_any_order() {
    let mut jumbled = FakeDispatcher {
        order: Order::NewestFirst,
        pace: Pace::Together,
        ..FakeDispatcher::default()
    };
    let source = || FakeSource::new(wide());

    let checked = pipeline::check(&source(), &mut FakeDispatcher::default(), &entry()).unwrap();
    assert_eq!(
        checked,
        pipeline::check(&source(), &mut jumbled, &entry()).unwrap()
    );

    let canonical = |graph| json::canonical::to_vec(&graph).unwrap();
    let graphed = pipeline::graph(&source(), &mut FakeDispatcher::default(), &entry()).unwrap();
    assert_eq!(
        canonical(graphed),
        canonical(pipeline::graph(&source(), &mut jumbled, &entry()).unwrap())
    );

    let question = Query::new(Direction::Rdeps, Label::parse("//lib/core:base").unwrap());
    let queried = pipeline::query(
        &source(),
        &mut FakeDispatcher::default(),
        &entry(),
        &question,
    )
    .unwrap();
    assert!(!queried.is_empty());
    assert_eq!(
        queried,
        pipeline::query(&source(), &mut jumbled, &entry(), &question).unwrap()
    );

    let collected = |dispatcher: &mut FakeDispatcher| {
        let mut pipeline = built();
        let ports = pipeline.ports_mut();
        pipeline::gc(
            &source(),
            dispatcher,
            &entry(),
            &mut ports.cache,
            &ports.cas,
            &ports.cas,
            &FakeHistory::default(),
        )
        .unwrap()
    };
    let swept = collected(&mut FakeDispatcher::default());
    assert!(swept.rows_dropped > 0, "the sweep has something to drop");
    assert_eq!(swept, collected(&mut jumbled));
}
