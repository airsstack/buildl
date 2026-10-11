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
use crate::types::{Command, Digest, Node, Program, TargetGraph};

/// The program `command` names: its first argument, when that is a program name.
///
/// A first argument that is empty or holds a placeholder names no program before the command is
/// expanded, so such a command names none here.
pub(crate) fn program_of(command: &Command) -> Option<Program> {
    command.arguments().first().and_then(Program::of)
}

/// Every distinct program `graph`'s commands name, each with the lowest-labelled target naming
/// it.
pub(crate) fn programs(graph: &TargetGraph) -> BTreeMap<Program, &Node> {
    let mut named = BTreeMap::new();
    for node in graph.nodes() {
        if let Some(program) = program_of(&node.run) {
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
    use super::{program_of, programs};
    use crate::plan::fixtures::{command, graph, node};
    use crate::types::Program;

    #[test]
    fn a_command_names_the_program_in_its_first_argument() {
        let named = program_of(&command(&["/usr/bin/cc", "-c", "$in"]));
        assert_eq!(named.as_ref().map(Program::as_str), Some("/usr/bin/cc"));
    }

    #[test]
    fn an_empty_or_placeholder_first_argument_names_no_program() {
        for first in ["", "$out/app", "bin/$opt:tool"] {
            assert_eq!(program_of(&command(&[first, "cc"])), None, "{first:?}");
        }
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
        let named: Vec<(String, String)> = programs(&graph)
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
    fn a_target_whose_command_names_no_program_adds_none() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["$out/gen"])),
                node("//:b", |node| node.run = command(&["", "x"])),
            ],
            &[&[], &[]],
        );
        assert!(programs(&graph).is_empty());
    }
}
