//! `Pipeline::graph`, driven through the build-file port by [`FakeSource`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::path::Path;

use buildl_core::{
    DeclarationField, DeclaredKind, EntryName, Error, EvaluationFailure, Pipeline, Provenance,
    StagedItem, TargetGraph, TargetRole, Written, json,
};

use crate::common::{
    FakeFile, FakePorts, FakeSource, alias, file, rule, setting, target, target_with, workspace,
};

/// The serialized graph of [`fixture`], pinned.
const GOLDEN: &str = include_str!("golden/graph.json");

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn graph(source: FakeSource) -> buildl_core::Result<TargetGraph> {
    Pipeline::<FakePorts>::new(source).graph(&entry())
}

/// The root file's declarations: a rule, a target using it, a target with dependencies in two
/// directories, an alias, a test target naming a setting, and the setting.
fn root_items() -> Vec<StagedItem> {
    vec![
        rule(
            "cc",
            &["cc", "-c", "$in", "-o", "$out"],
            Some("compile $in"),
        ),
        target_with("main.o", |target| {
            target.run = None;
            target.rule = Some(Written::new("cc"));
            target.inputs = vec![Written::new("main.c")];
        }),
        target_with("app", |target| {
            target.run = Some(["cc", "$deps", "-o", "$out"].map(Written::new).to_vec());
            target.deps = vec![Written::new("main.o"), Written::new("//lib:text")];
        }),
        alias("default", "app"),
        target_with("app_test", |target| {
            target.role = TargetRole::Test;
            target.run = Some(
                ["$out/app", "--filter=$opt:test_filter"]
                    .map(Written::new)
                    .to_vec(),
            );
            target.deps = vec![Written::new("default")];
        }),
        setting("test_filter"),
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
fn a_workspace_resolves_to_the_pinned_graph() {
    let graph = graph(fixture()).unwrap();
    assert_eq!(graph.len(), 4);
    let json = json::canonical::to_string(&graph).unwrap();
    assert_eq!(json, GOLDEN.trim_end());
}

#[test]
fn declaring_in_another_order_yields_the_same_bytes() {
    let mut reversed = root_items();
    reversed.reverse();
    let source = FakeSource::new(workspace(vec![
        ("", file(reversed, &["lib"])),
        ("lib", file(lib_items(), &[])),
    ]));
    let json = json::canonical::to_string(&graph(source).unwrap()).unwrap();
    assert_eq!(json, GOLDEN.trim_end());
}

#[test]
fn a_label_declared_twice_in_one_file_names_that_file_twice() {
    let source = FakeSource::new(workspace(vec![(
        "",
        file(vec![target("a", &[]), target("a", &[])], &[]),
    )]));
    match graph(source).unwrap_err() {
        Error::DuplicateLabel { label, sites } => {
            assert_eq!(label.to_string(), "//:a");
            let files: Vec<&Path> = sites.iter().map(Provenance::file).collect();
            assert_eq!(files, [Path::new("build.lua"), Path::new("build.lua")]);
        }
        other => unreachable!("expected DuplicateLabel, got {other:?}"),
    }
}

#[test]
fn a_setting_declared_in_two_files_names_both() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![setting("mode")], &["lib"])),
        ("lib", file(vec![setting("mode")], &[])),
    ]));
    match graph(source).unwrap_err() {
        Error::DuplicateSetting { name, sites } => {
            assert_eq!(name.as_str(), "mode");
            let files: Vec<&Path> = sites.iter().map(Provenance::file).collect();
            assert_eq!(files, [Path::new("build.lua"), Path::new("lib/build.lua")]);
        }
        other => unreachable!("expected DuplicateSetting, got {other:?}"),
    }
}

#[test]
fn a_misspelled_dependency_names_its_file_and_the_nearest_label() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("main.o", &[])], &["app"])),
        ("app", file(vec![target("bin", &["//:mian.o"])], &[])),
    ]));
    let error = graph(source).unwrap_err();
    assert_eq!(
        error.to_string(),
        "app/build.lua: //app:bin: dep //:mian.o is not declared (did you mean //:main.o?)"
    );
    match error {
        Error::UnknownReference {
            site,
            field,
            reference,
            suggestion,
        } => {
            assert_eq!(site.provenance.file(), Path::new("app/build.lua"));
            assert_eq!(site.label.to_string(), "//app:bin");
            assert_eq!(field, DeclarationField::Dep);
            assert_eq!(reference.to_string(), "//:mian.o");
            assert_eq!(suggestion.unwrap().to_string(), "//:main.o");
        }
        other => unreachable!("expected UnknownReference, got {other:?}"),
    }
}

#[test]
fn a_dependency_naming_a_rule_names_both_files() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("app", &["//tools:cc"])], &["tools"])),
        ("tools", file(vec![rule("cc", &["cc"], None)], &[])),
    ]));
    let error = graph(source).unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:app: dep //tools:cc names a rule, declared in tools/build.lua"
    );
    match error {
        Error::WrongReferenceKind {
            site,
            found,
            declared_in,
            ..
        } => {
            assert_eq!(site.provenance.file(), Path::new("build.lua"));
            assert_eq!(found, DeclaredKind::Rule);
            assert_eq!(declared_in.file(), Path::new("tools/build.lua"));
        }
        other => unreachable!("expected WrongReferenceKind, got {other:?}"),
    }
}

#[test]
fn a_cycle_across_two_directories_names_every_target_and_its_file() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("b", &["//lib:c"])], &["lib"])),
        (
            "lib",
            file(vec![target("c", &["a"]), target("a", &["//:b"])], &[]),
        ),
    ]));
    let error = graph(source).unwrap_err();
    assert_eq!(
        error.to_string(),
        "dependency cycle: //:b (build.lua) -> //lib:c (lib/build.lua) -> \
         //lib:a (lib/build.lua) -> //:b"
    );
    match error {
        Error::DependencyCycle { path } => {
            let steps: Vec<(String, &Path)> = path
                .iter()
                .map(|site| (site.label.to_string(), site.provenance.file()))
                .collect();
            assert_eq!(
                steps,
                [
                    ("//:b".to_owned(), Path::new("build.lua")),
                    ("//lib:c".to_owned(), Path::new("lib/build.lua")),
                    ("//lib:a".to_owned(), Path::new("lib/build.lua")),
                ]
            );
        }
        other => unreachable!("expected DependencyCycle, got {other:?}"),
    }
}

#[test]
fn a_build_file_that_fails_to_evaluate_is_the_load_error_unchanged() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("app", &[])], &["lib"])),
        ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
    ]));
    match graph(source).unwrap_err() {
        Error::Evaluation {
            provenance,
            failure,
            ..
        } => {
            assert_eq!(provenance.file(), Path::new("lib/build.lua"));
            assert_eq!(failure, EvaluationFailure::Syntax);
        }
        other => unreachable!("expected Evaluation, got {other:?}"),
    }
}
