//! `buildl_core::load` over a real workspace, through this adapter.

use std::path::PathBuf;

use buildl_core::{
    Action, Argument, Command, Declaration, Declared, Directory, EntryName, Freshness, Label,
    NetworkAccess, OutputName, Provenance, SourcePath, Target, TargetRole, load,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::common::fixture;

fn command(arguments: &[&str]) -> Command {
    Command::new(
        arguments
            .iter()
            .map(|argument| Argument::parse(*argument).unwrap())
            .collect(),
    )
    .unwrap()
}

fn target(
    file: &str,
    directory: &str,
    label: &str,
    run: &[&str],
    inputs: &[&str],
    deps: &[&str],
    output: &str,
) -> Declaration {
    let directory = if directory.is_empty() {
        Directory::root()
    } else {
        Directory::parse(directory).unwrap()
    };
    Declaration::new(
        Provenance::new(PathBuf::from(file), directory),
        Declared::Target(Target {
            label: Label::parse(label).unwrap(),
            role: TargetRole::Build,
            action: Action::Run(command(run)),
            inputs: inputs
                .iter()
                .map(|input| SourcePath::parse(*input).unwrap())
                .collect(),
            deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
            outputs: vec![OutputName::parse(output).unwrap()],
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        }),
    )
}

#[test]
fn load_turns_a_real_workspace_into_declarations() {
    let source = LuaSource::new(fixture("workspace"), DeclarationLimits::default());
    let declarations = load(&source, &EntryName::parse("build.lua").unwrap()).unwrap();
    let expected = vec![
        target(
            "build.lua",
            "",
            "//:app",
            &["cc", "$in", "$deps", "-o", "$out"],
            &["main.c"],
            &["//lib:text"],
            "app",
        ),
        target(
            "lib/build.lua",
            "lib",
            "//lib:text",
            &["cc", "-c", "$in", "-o", "$out"],
            &["lib/src/text.c"],
            &[],
            "text",
        ),
    ];
    assert_eq!(declarations, expected);
}
