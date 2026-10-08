//! Load, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::path::Path;

use buildl_core::{
    DeclarationField, Declared, EntryName, Error, EvaluationFailure, EvaluationLimit, json, load,
};

use crate::common::{FakeFile, FakeSource, dir, file, setting, target, workspace};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn names(source: &FakeSource) -> Vec<String> {
    load(source, &entry())
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
    let a = json::canonical::to_vec(&load(&ordered, &entry()).unwrap()).unwrap();
    let b = json::canonical::to_vec(&load(&shuffled, &entry()).unwrap()).unwrap();
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
    load(&source, &entry()).unwrap();
    assert_eq!(source.evaluated(), [dir(""), dir("a"), dir("a/x")]);
}

#[test]
fn a_subdir_cannot_leave_the_workspace() {
    for escape in ["../x", "/x"] {
        let source = FakeSource::new(workspace(vec![
            ("", file(vec![], &["lib"])),
            ("lib", file(vec![], &[escape])),
        ]));
        match load(&source, &entry()).unwrap_err() {
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
    match load(&source, &entry()).unwrap_err() {
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
    let error = load(&source, &entry()).unwrap_err();
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
        let error = load(&source, &entry()).unwrap_err();
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
    match load(&source, &entry()).unwrap_err() {
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
