//! What loading does with the ignore rules, driven through [`FakeIgnore`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use buildl_core::{
    DeclarationField, EntryKind, EntryName, Error, IgnoredPath, Origin, Pipeline, SourcePath,
    StagedItem, Written,
};

use crate::common::{
    FakeIgnore, FakePorts, FakeSource, file, file_by, plugin, ports, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// A pipeline over `source` that asks `rules` about every path.
fn pipeline(source: FakeSource, rules: FakeIgnore) -> Pipeline<FakePorts> {
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));
    pipeline.ports_mut().ignore = rules;
    pipeline
}

/// A target named `app` reading `inputs`.
fn reading(inputs: &[&str]) -> StagedItem {
    target_with("app", |target| {
        target.inputs = inputs.iter().map(|input| Written::new(*input)).collect();
    })
}

#[test]
fn an_ignored_input_is_refused() {
    let source = FakeSource::new(workspace(vec![(
        "",
        file(vec![reading(&["notes/a.md"])], &[]),
    )]));
    let rules = FakeIgnore::ignoring(&[("notes/a.md", ".buildlignore", "notes/")]);
    let mut pipeline = pipeline(source, rules);

    let error = pipeline.check(&entry()).unwrap_err();

    match &error {
        Error::Ignored { path, ignoring, .. } => {
            assert_eq!(
                **path,
                IgnoredPath::Input(SourcePath::parse("notes/a.md").unwrap())
            );
            assert_eq!(ignoring.file(), ".buildlignore");
            assert_eq!(ignoring.pattern(), "notes/");
        }
        other => unreachable!("expected Ignored, got {other:?}"),
    }
    assert_eq!(
        error.to_string(),
        "build.lua: inputs: notes/a.md is ignored by .buildlignore (notes/)"
    );
}

#[test]
fn an_input_in_a_subdirectory_is_asked_with_its_full_path() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![], &["lib"])),
        ("lib", file(vec![reading(&["x.txt"])], &[])),
    ]));
    let mut pipeline = pipeline(source, FakeIgnore::default());

    pipeline.check(&entry()).unwrap();

    let asked = pipeline.ports().ignore.asked();
    assert!(
        asked.contains(&("lib/x.txt".to_owned(), EntryKind::File)),
        "{asked:?}"
    );
}

#[test]
fn a_plugin_written_input_is_checked_like_any_other() {
    let origin = Origin::plugins(vec![plugin("go", "1.2.0")]).unwrap();
    let source = FakeSource::new(workspace(vec![(
        "",
        file_by(&origin, vec![reading(&["notes/a.md"])], &[]),
    )]));
    let rules = FakeIgnore::ignoring(&[("notes/a.md", ".buildlignore", "notes/")]);
    let mut pipeline = pipeline(source, rules);

    match pipeline.check(&entry()).unwrap_err() {
        Error::Ignored {
            origin: refused,
            path,
            ..
        } => {
            assert_eq!(refused, origin);
            assert_eq!(path.to_string(), "notes/a.md");
        }
        other => unreachable!("expected Ignored, got {other:?}"),
    }
}

#[test]
fn a_failing_rule_set_fails_the_load() {
    let source = FakeSource::new(workspace(vec![("", file(vec![reading(&["a.c"])], &[]))]));
    let mut pipeline = pipeline(
        source,
        FakeIgnore::failing("src/.buildlignore", "not UTF-8"),
    );

    match pipeline.check(&entry()).unwrap_err() {
        Error::IgnoreFile { file, reason, .. } => {
            assert_eq!(file, "src/.buildlignore");
            assert_eq!(reason, "not UTF-8");
        }
        other => unreachable!("expected IgnoreFile, got {other:?}"),
    }
}

#[test]
fn a_grammar_refusal_comes_first() {
    let source = FakeSource::new(workspace(vec![("", file(vec![reading(&["../x"])], &[]))]));
    let mut pipeline = pipeline(
        source,
        FakeIgnore::ignoring(&[("../x", ".buildlignore", "x")]),
    );

    match pipeline.check(&entry()).unwrap_err() {
        Error::InvalidDeclaration { field, .. } => assert_eq!(field, DeclarationField::Input),
        other => unreachable!("expected InvalidDeclaration, got {other:?}"),
    }
    assert_eq!(pipeline.ports().ignore.asked(), []);
}

#[test]
fn the_root_build_file_is_read_whatever_the_rules_say() {
    let source = FakeSource::new(workspace(vec![("", file(vec![reading(&[])], &[]))]));
    let rules = FakeIgnore::ignoring(&[("build.lua", ".buildlignore", "build.lua")]);
    let mut pipeline = pipeline(source, rules);

    assert_eq!(pipeline.check(&entry()).unwrap().len(), 1);
}
