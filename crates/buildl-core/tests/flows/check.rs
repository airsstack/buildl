//! `Pipeline::check`, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::path::Path;

use buildl_core::{EntryName, Error, Pipeline};

use crate::common::{FakePorts, FakeSource, file, setting, target, workspace};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

#[test]
fn check_returns_the_sorted_declarations_of_a_deterministic_workspace() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
        ("lib", file(vec![target("text", &[])], &[])),
    ]));
    let pipeline = Pipeline::<FakePorts>::new(source);
    let declarations = pipeline.check(&entry()).unwrap();
    let names: Vec<&str> = declarations.iter().map(|d| d.item().name()).collect();
    assert_eq!(names, ["a", "b", "text"]);
}

#[test]
fn a_pairs_shuffled_second_evaluation_passes_check() {
    let first = workspace(vec![(
        "",
        file(
            vec![target("a", &[]), setting("s"), target("b", &["a"])],
            &[],
        ),
    )]);
    let second = workspace(vec![(
        "",
        file(
            vec![target("b", &["a"]), target("a", &[]), setting("s")],
            &[],
        ),
    )]);
    let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
    assert_eq!(pipeline.check(&entry()).unwrap().len(), 3);
}

#[test]
fn a_second_evaluation_that_declares_more_fails_check_naming_the_file() {
    let first = workspace(vec![
        ("", file(vec![target("app", &[])], &["lib"])),
        ("lib", file(vec![], &[])),
    ]);
    let second = workspace(vec![
        ("", file(vec![target("app", &[])], &["lib"])),
        ("lib", file(vec![target("extra", &[])], &[])),
    ]);
    let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
    match pipeline.check(&entry()).unwrap_err() {
        Error::Nondeterministic {
            provenance,
            first_run,
            second_run,
        } => {
            assert_eq!(provenance.file(), Path::new("lib/build.lua"));
            assert!(first_run.is_none());
            assert_eq!(second_run.unwrap().item().name(), "extra");
        }
        other => unreachable!("expected Nondeterministic, got {other:?}"),
    }
}

#[test]
fn a_root_declaration_is_named_over_a_subdirectory_one_it_sorts_before() {
    let first = workspace(vec![
        ("", file(vec![], &["app"])),
        ("app", file(vec![setting("x")], &[])),
    ]);
    let second = workspace(vec![
        ("", file(vec![setting("r")], &["app"])),
        ("app", file(vec![setting("x")], &[])),
    ]);
    let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
    match pipeline.check(&entry()).unwrap_err() {
        Error::Nondeterministic {
            provenance,
            first_run,
            second_run,
        } => {
            assert_eq!(provenance.file(), Path::new("build.lua"));
            assert_eq!(first_run.unwrap().item().name(), "x");
            assert_eq!(second_run.unwrap().item().name(), "r");
        }
        other => unreachable!("expected Nondeterministic, got {other:?}"),
    }
}
