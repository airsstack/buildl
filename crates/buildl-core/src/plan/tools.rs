//! The programs a graph's commands name.
//!
//! Its own file because a program is the one thing a command names that exists outside the
//! workspace: it has to be allowed before anything runs, it has to be found, and what was found
//! is part of every key of every target that runs it.
//!
//! Responsibilities: [`program_of`], [`naming`] and [`fingerprints`].
//!
//! Non-responsibilities: whether a program may run, which is decided against the ceiling; which
//! programs are built, which is Grants'; and finding one, which is the [`ToolResolver`] port's.
//! A program a dependency builds is no tool of the machine: it is never listed here, so it is
//! never looked up or fingerprinted.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DeclarationSite, Error, Result};
use crate::grants::built::built;
use crate::placement::layout::Layout;
use crate::ports::ToolResolver;
use crate::types::{
    Command, Digest, Label, Node, NodeId, Program, SettingName, SettingValue, TargetGraph,
};

/// The program `command` names under `settings`: its first argument, with each reference to a
/// setting replaced by that setting's value, when that is a program name.
///
/// A setting's value is known before anything runs, so a program named through one is as
/// settled as one written out. A first argument that is then empty or still holds a `$` names
/// no program here: one holding `$in`, `$out` or `$deps` runs a file of the workspace or of the
/// build, whose path is known only when the command is expanded.
pub(crate) fn program_of(
    command: &Command,
    settings: &BTreeMap<SettingName, SettingValue>,
) -> Option<Program> {
    let first = command.arguments().first()?;
    Program::of(&first.with_settings(settings))
}

/// What `graph`'s commands name under some settings: the programs of the machine, and the
/// targets that run a program a dependency built.
#[derive(Debug)]
pub(crate) struct Named<'g> {
    /// Every distinct program named that is not built, each with the lowest-labelled target
    /// naming it.
    pub(crate) programs: BTreeMap<Program, &'g Node>,
    /// The label of every target whose command's program is built for it.
    pub(crate) built_runners: BTreeSet<&'g Label>,
}

/// What `graph`'s commands name under `settings`, in one pass over the targets.
///
/// A program is built for a target when it is, or lies below, an output of one of the target's
/// dependencies as `layout` places them. A built program is left out of
/// [`Named::programs`]: it is no tool of the machine.
pub(crate) fn naming<'g>(
    graph: &'g TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
    layout: &Layout,
) -> Named<'g> {
    let mut programs = BTreeMap::new();
    let mut built_runners = BTreeSet::new();
    for (node, id) in graph.nodes().iter().zip((0_u32..).map(NodeId::new)) {
        let Some(program) = program_of(&node.run, settings) else {
            continue;
        };
        if built(&program, &layout.deps(graph, id)) {
            built_runners.insert(&node.label);
        } else {
            programs.entry(program).or_insert(node);
        }
    }
    Named {
        programs,
        built_runners,
    }
}

/// The fingerprint of every program in `programs`, asked for in program order.
///
/// # Errors
///
/// Returns [`Error::MissingTool`] for the first program `tools` cannot find, naming the
/// lowest-labelled target whose command names it, and whatever `tools` itself returns.
pub(crate) fn fingerprints<T: ToolResolver>(
    programs: &BTreeMap<Program, &Node>,
    tools: &T,
) -> Result<BTreeMap<Program, Digest>> {
    let mut found = BTreeMap::new();
    for (program, node) in programs {
        let Some(fingerprint) = tools.fingerprint(program)? else {
            return Err(Error::MissingTool {
                site: Box::new(DeclarationSite::from(*node)),
                program: program.clone(),
            });
        };
        found.insert(program.clone(), fingerprint);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{naming, program_of};
    use crate::grants::fixtures::{command, dir, graph, node, settings};
    use crate::placement::layout::Layout;
    use crate::types::{Program, TargetGraph};

    /// The programs `graph` names under `pairs`, each with its lowest-labelled target.
    fn listed(graph: &TargetGraph, pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        let layout = Layout::of(graph, &dir("out")).unwrap();
        naming(graph, &settings(pairs), &layout)
            .programs
            .into_iter()
            .map(|(program, node)| (program.to_string(), node.label.to_string()))
            .collect()
    }

    /// The labels of the targets whose program is built for them.
    fn runners(graph: &TargetGraph, pairs: &[(&str, &str)]) -> Vec<String> {
        let layout = Layout::of(graph, &dir("out")).unwrap();
        naming(graph, &settings(pairs), &layout)
            .built_runners
            .into_iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The program a command starting with `first` names under these `(name, value)` settings.
    fn named(first: &str, pairs: &[(&str, &str)]) -> Option<String> {
        program_of(&command(&[first, "-c", "$in"]), &settings(pairs))
            .as_ref()
            .map(Program::to_string)
    }

    #[test]
    fn a_command_names_the_program_in_its_first_argument() {
        assert_eq!(named("/usr/bin/cc", &[]).as_deref(), Some("/usr/bin/cc"));
    }

    #[test]
    fn a_first_argument_naming_a_setting_names_the_program_the_setting_holds() {
        let held = [("tool", "clang"), ("dir", "/opt/llvm/bin")];
        assert_eq!(named("$opt:tool", &held).as_deref(), Some("clang"));
        assert_eq!(
            named("$opt:dir/$opt:tool", &held).as_deref(),
            Some("/opt/llvm/bin/clang")
        );
        assert_eq!(
            named("$opt:tool", &[("tool", "gcc")]).as_deref(),
            Some("gcc")
        );
    }

    #[test]
    fn an_empty_first_argument_or_one_still_holding_a_dollar_names_no_program() {
        let held = [("tool", "clang"), ("none", ""), ("list", "$in")];
        for first in [
            "",
            "$out/app",
            "$deps",
            "$in",
            "$out/$opt:tool",
            "$opt:none",
            "$opt:list",
            "$opt:missing",
            "$opt:",
            "bin/$tool",
        ] {
            assert_eq!(named(first, &held), None, "{first:?}");
        }
        assert_eq!(named("bin/$opt:tool", &[]), None);
    }

    #[test]
    fn each_distinct_program_is_listed_once_with_its_lowest_labelled_target() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["ld", "$in"])),
                node("//:b", |node| node.run = command(&["cc", "-c"])),
                node("//:c", |node| node.run = command(&["cc", "-O2"])),
                node("//lib:d", |node| node.run = command(&["ar"])),
            ],
            &[&[], &[], &[], &[]],
        );
        assert_eq!(
            listed(&graph, &[]),
            [
                ("ar".to_owned(), "//lib:d".to_owned()),
                ("cc".to_owned(), "//:b".to_owned()),
                ("ld".to_owned(), "//:a".to_owned()),
            ]
        );
    }

    #[test]
    fn a_program_named_through_a_setting_is_listed_with_the_ones_written_out() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["$opt:tool", "$in"])),
                node("//:b", |node| node.run = command(&["cc", "-c"])),
                node("//:c", |node| node.run = command(&["$opt:linker"])),
            ],
            &[&[], &[], &[]],
        );
        assert_eq!(
            listed(&graph, &[("tool", "cc"), ("linker", "ld")]),
            [
                ("cc".to_owned(), "//:a".to_owned()),
                ("ld".to_owned(), "//:c".to_owned()),
            ]
        );
        assert_eq!(
            listed(&graph, &[("tool", "clang"), ("linker", "")]),
            [
                ("cc".to_owned(), "//:b".to_owned()),
                ("clang".to_owned(), "//:a".to_owned()),
            ]
        );
    }

    #[test]
    fn a_target_whose_command_names_no_program_adds_none() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["$out/gen"])),
                node("//:b", |node| node.run = command(&["", "x"])),
            ],
            &[&[], &[]],
        );
        assert_eq!(listed(&graph, &[]), []);
    }

    #[test]
    fn a_program_a_dependency_builds_is_not_listed_and_its_target_is_a_runner() {
        let graph = graph(
            vec![
                node("//:gen", |_| {}),
                node("//:own", |node| node.run = command(&["out/gen", "$out"])),
                node("//:set", |node| node.run = command(&["$opt:tool"])),
                node("//:stray", |node| node.run = command(&["out/gen"])),
            ],
            &[&[], &[0], &[0], &[]],
        );
        let held = [("tool", "out/gen")];
        assert_eq!(
            listed(&graph, &held),
            [
                ("cc".to_owned(), "//:gen".to_owned()),
                ("out/gen".to_owned(), "//:stray".to_owned()),
            ],
            "only the target that does not depend on //:gen names it as a tool"
        );
        assert_eq!(runners(&graph, &held), ["//:own", "//:set"]);
    }
}
