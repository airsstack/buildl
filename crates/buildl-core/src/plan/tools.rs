//! The programs a graph's commands name.
//!
//! Its own file because a program is the one thing a command names that exists outside the
//! workspace: it has to be allowed before anything runs, it has to be found, and what was found
//! is part of every key of every target that runs it.
//!
//! Responsibilities: [`program_of`], [`programs`] and [`fingerprints`].
//!
//! Non-responsibilities: whether a program may run, which is decided against the ceiling, and
//! finding one, which is the [`ToolResolver`] port's.

use std::collections::BTreeMap;

use crate::error::{DeclarationSite, Error, Result};
use crate::ports::ToolResolver;
use crate::types::{Command, Digest, Node, Program, SettingName, SettingValue, TargetGraph};

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

/// Every distinct program `graph`'s commands name under `settings`, each with the
/// lowest-labelled target naming it.
pub(crate) fn programs<'g>(
    graph: &'g TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
) -> BTreeMap<Program, &'g Node> {
    let mut named = BTreeMap::new();
    for node in graph.nodes() {
        if let Some(program) = program_of(&node.run, settings) {
            named.entry(program).or_insert(node);
        }
    }
    named
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
    use std::collections::BTreeMap;

    use super::{program_of, programs};
    use crate::plan::fixtures::{command, graph, node, settings};
    use crate::types::Program;

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
        let named: Vec<(String, String)> = programs(&graph, &BTreeMap::new())
            .into_iter()
            .map(|(program, node)| (program.to_string(), node.label.to_string()))
            .collect();
        assert_eq!(
            named,
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
        let listed = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            programs(&graph, &settings(pairs))
                .into_iter()
                .map(|(program, node)| (program.to_string(), node.label.to_string()))
                .collect()
        };
        assert_eq!(
            listed(&[("tool", "cc"), ("linker", "ld")]),
            [
                ("cc".to_owned(), "//:a".to_owned()),
                ("ld".to_owned(), "//:c".to_owned()),
            ]
        );
        assert_eq!(
            listed(&[("tool", "clang"), ("linker", "")]),
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
        assert!(programs(&graph, &BTreeMap::new()).is_empty());
    }
}
