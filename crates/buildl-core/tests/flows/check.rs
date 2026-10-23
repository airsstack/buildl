//! `Pipeline::check`, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::path::Path;

use buildl_core::{EntryName, Error, Label, Pipeline};

use crate::common::{
    FakeCas, FakeDispatcher, FakeHistory, FakePorts, FakeSource, file, ports, setting, target,
    workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

#[test]
fn check_returns_the_sorted_declarations_of_a_deterministic_workspace() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
        ("lib", file(vec![target("text", &[])], &[])),
    ]));
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));
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
    let mut pipeline =
        Pipeline::<FakePorts>::new(ports(FakeSource::with_second_run(first, second)));
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
    let mut pipeline =
        Pipeline::<FakePorts>::new(ports(FakeSource::with_second_run(first, second)));
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
    let mut pipeline =
        Pipeline::<FakePorts>::new(ports(FakeSource::with_second_run(first, second)));
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

#[test]
fn a_caller_holding_only_a_source_checks_like_a_pipeline() {
    let agreeing = || {
        FakeSource::new(workspace(vec![
            ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
            ("lib", file(vec![target("text", &[])], &[])),
        ]))
    };
    let alone = buildl_core::pipeline::check(
        &agreeing(),
        &mut FakeDispatcher::default(),
        &entry(),
        &FakeHistory::default(),
        &FakeCas::default(),
    )
    .unwrap()
    .declarations;
    let through = Pipeline::<FakePorts>::new(ports(agreeing()))
        .check(&entry())
        .unwrap();
    assert_eq!(alone, through);

    let first = workspace(vec![("", file(vec![target("app", &[])], &[]))]);
    let second = workspace(vec![(
        "",
        file(vec![target("app", &[]), target("extra", &[])], &[]),
    )]);
    let differing = FakeSource::with_second_run(first, second);
    assert!(matches!(
        buildl_core::pipeline::check(
            &differing,
            &mut FakeDispatcher::default(),
            &entry(),
            &FakeHistory::default(),
            &FakeCas::default()
        ),
        Err(Error::Nondeterministic { .. })
    ));
}

#[test]
fn a_caller_holding_only_a_source_gets_the_graph_a_pipeline_gets() {
    let fixture = || {
        FakeSource::new(workspace(vec![
            ("", file(vec![target("app", &["//lib:text"])], &["lib"])),
            ("lib", file(vec![target("text", &[])], &[])),
        ]))
    };
    let (alone, missing) = buildl_core::pipeline::graph(
        &fixture(),
        &mut FakeDispatcher::default(),
        &entry(),
        &FakeHistory::default(),
        &FakeCas::default(),
    )
    .unwrap();
    assert_eq!(missing, Vec::<Label>::new());
    let through = Pipeline::<FakePorts>::new(ports(fixture()))
        .graph(&entry())
        .unwrap();
    assert_eq!(
        buildl_core::json::canonical::to_vec(&alone).unwrap(),
        buildl_core::json::canonical::to_vec(&through).unwrap()
    );
}
