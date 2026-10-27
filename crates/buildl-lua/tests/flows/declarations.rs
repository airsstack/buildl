//! A build file's calls come back as exactly the staged values they wrote.

use buildl_core::{
    DeclarationOrder, Evaluated, Freshness, NetworkAccess, Origin, OutputSource, StagedAlias,
    StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
    StagedTarget, TargetRole, Written,
};

use crate::common::{evaluate, fixture, workspace};

fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
    texts.iter().map(|text| Written::new(*text)).collect()
}

const fn declared(order: u32, item: StagedItem) -> StagedDeclaration {
    StagedDeclaration {
        order: DeclarationOrder::new(order),
        origin: Origin::BuildFile,
        item,
    }
}

#[test]
fn every_primitive_and_field_is_staged_as_written() {
    let expected = StagedFile {
        declarations: vec![
            declared(
                1,
                StagedItem::Rule(StagedRule {
                    name: Written::new("cc"),
                    run: strings(&["cc", "-c", "$in", "-o", "$out"]),
                    description: Some(Written::new("compile $in")),
                }),
            ),
            declared(
                2,
                StagedItem::Target(StagedTarget {
                    name: Written::new("main.o"),
                    role: TargetRole::Build,
                    rule: Some(Written::new("cc")),
                    run: None,
                    inputs: strings(&["src/main.c"]),
                    deps: Vec::new(),
                    outputs: None,
                    env: Vec::new(),
                    network: NetworkAccess::Sealed,
                    output_source: OutputSource::Command,
                    freshness: Freshness::Cached,
                }),
            ),
            declared(
                3,
                StagedItem::Target(StagedTarget {
                    name: Written::new("app"),
                    role: TargetRole::Build,
                    rule: None,
                    run: Some(strings(&["cc", "$deps", "-o", "$out"])),
                    inputs: Vec::new(),
                    deps: strings(&["main.o", "//lib:text"]),
                    outputs: Some(strings(&["app"])),
                    env: strings(&["HOME"]),
                    network: NetworkAccess::Declared,
                    output_source: OutputSource::Command,
                    freshness: Freshness::Always,
                }),
            ),
            declared(
                4,
                StagedItem::Target(StagedTarget {
                    name: Written::new("app_test"),
                    role: TargetRole::Test,
                    rule: None,
                    run: Some(strings(&["$out/app", "--self-test"])),
                    inputs: Vec::new(),
                    deps: strings(&["app"]),
                    outputs: None,
                    env: Vec::new(),
                    network: NetworkAccess::Sealed,
                    output_source: OutputSource::Command,
                    freshness: Freshness::Cached,
                }),
            ),
            declared(
                5,
                StagedItem::Alias(StagedAlias {
                    name: Written::new("default"),
                    target: Written::new("app"),
                }),
            ),
            declared(
                6,
                StagedItem::Setting(StagedSetting {
                    name: Written::new("mode"),
                    default: Written::new("debug"),
                }),
            ),
        ],
        subdirs: vec![StagedSubdir {
            path: Written::new("lib"),
            order: DeclarationOrder::new(0),
        }],
    };
    assert_eq!(
        evaluate(&fixture("every_field")).unwrap(),
        Evaluated::Staged(expected)
    );
}

#[test]
fn a_nested_sources_list_is_spliced_into_inputs() {
    let Evaluated::Staged(staged) = evaluate(&fixture("flattened")).unwrap() else {
        panic!("the fixture has a build file");
    };
    let StagedItem::Target(target) = &staged.declarations[0].item else {
        panic!("the fixture declares a target");
    };
    assert_eq!(target.inputs, strings(&["src/a.c", "src/b.c", "go.mod"]));
}

#[test]
fn evaluating_a_file_twice_stages_the_same_values() {
    let root = fixture("every_field");
    assert_eq!(evaluate(&root).unwrap(), evaluate(&root).unwrap());
}

#[test]
fn a_missing_build_file_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(evaluate(&root).unwrap(), Evaluated::Absent);
}

#[test]
fn a_build_file_declares_one_target_per_decoded_value() {
    let (_dir, root) = workspace(
        "local stream = '{\"n\":\"a\"}\\n{\"n\":\"b\"}\\n{\"n\":\"c\"}'\n\
         for _, value in ipairs(buildl.decode_all(stream)) do\n\
           buildl.target(value.n, { run = { 'x' } })\n\
         end",
    );
    let Evaluated::Staged(staged) = evaluate(&root).unwrap() else {
        panic!("the workspace has a build file");
    };
    let names: Vec<&str> = staged
        .declarations
        .iter()
        .map(|declaration| match &declaration.item {
            StagedItem::Target(target) => target.name.as_written(),
            other => panic!("expected a target, got {other:?}"),
        })
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
}
