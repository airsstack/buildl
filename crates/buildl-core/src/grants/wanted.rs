//! What a graph's targets want: the programs they run, the variables they read and the files
//! they read, each with the target that wants it.
//!
//! Its own file because it is the one account of the wanted set's sites: the plan holds every
//! one of them to the ceiling before anything is asked of the machine.
//!
//! Responsibilities: [`Sites`] and [`sites`]; and [`wanted`], the wanted set the sites make.
//!
//! Non-responsibilities: holding a want to the ceiling, which the plan's check does; finding a
//! program on the machine; and deciding what a program is, which is Placement's expansion and
//! the "built" rule.

use std::collections::{BTreeMap, BTreeSet};

use crate::grants::built::built;
use crate::placement::expand::{Lists, expand};
use crate::placement::layout::Layout;
use crate::types::{
    Argument, EnvName, NetworkAccess, Node, NodeId, Program, SettingName, SettingValue, SourcePath,
    TargetGraph, WantedSet, Workspace,
};

/// Everything a graph's targets want, each with the lowest-labelled target wanting it.
///
/// Each map is in the order of its keys, which is the order its wants are checked and later
/// asked for in.
#[derive(Debug)]
pub(crate) struct Sites<'g> {
    /// The programs commands run, whether written out, named through a build setting, or named
    /// through `$in`, `$out` or `$deps`; never one a dependency of the target builds.
    pub(crate) programs: BTreeMap<Program, &'g Node>,
    /// The environment variables targets read.
    pub(crate) env: BTreeMap<&'g EnvName, &'g Node>,
    /// The workspace files targets read.
    pub(crate) inputs: BTreeMap<&'g SourcePath, &'g Node>,
}

/// Collects what `graph`'s targets want when its build settings have the values `settings`
/// gives them and its outputs are placed as `layout` places them.
///
/// A program is the first text each command expands to, so one named through `$in` or `$out` is
/// wanted like one written out. A command that names no program is left out: running it is
/// refused when it would run.
pub(crate) fn sites<'g>(
    graph: &'g TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
    layout: &Layout,
) -> Sites<'g> {
    let mut programs = BTreeMap::new();
    let mut env = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    for (node, id) in graph.nodes().iter().zip((0_u32..).map(NodeId::new)) {
        let dep_paths = layout.deps(graph, id);
        let lists = Lists {
            inputs: &node.inputs,
            outputs: layout.outputs(id),
            deps: &dep_paths,
        };
        let program = expand(&node.run, settings, &lists)
            .into_iter()
            .next()
            .and_then(|first| Argument::parse(first).ok())
            .as_ref()
            .and_then(Program::of);
        if let Some(program) = program.filter(|program| !built(program, &dep_paths)) {
            programs.entry(program).or_insert(node);
        }
        for name in &node.env {
            env.entry(name).or_insert(node);
        }
        for path in &node.inputs {
            inputs.entry(path).or_insert(node);
        }
    }
    Sites {
        programs,
        env,
        inputs,
    }
}

/// The wanted set of `graph`, whose wants are `sites`, built in `workspace`.
///
/// The directories a build writes into are the workspace's own two, `out` and `state`. The
/// network is wanted by each target declared to reach it.
pub(crate) fn wanted(graph: &TargetGraph, sites: &Sites<'_>, workspace: &Workspace) -> WantedSet {
    WantedSet {
        programs: sites.programs.keys().cloned().collect(),
        env: sites.env.keys().map(|name| (*name).clone()).collect(),
        write: BTreeSet::from([workspace.out.clone(), workspace.state.clone()]),
        network: graph
            .nodes()
            .iter()
            .filter(|node| node.network == NetworkAccess::Declared)
            .map(|node| node.label.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::Path;

    use super::sites;
    use crate::grants::fixtures::{command, dir, env_name, graph, node, path, settings};
    use crate::placement::layout::Layout;
    use crate::types::{NetworkAccess, SettingName, SettingValue, TargetGraph};

    /// The programs `graph` wants, each with the label of the target wanting it.
    fn wanted(
        graph: &TargetGraph,
        settings: &BTreeMap<SettingName, SettingValue>,
    ) -> Vec<(String, String)> {
        let layout = Layout::of(graph, &dir("out")).unwrap();
        sites(graph, settings, &layout)
            .programs
            .iter()
            .map(|(program, node)| (program.to_string(), node.label.to_string()))
            .collect()
    }

    fn pair(program: &str, label: &str) -> (String, String) {
        (program.to_owned(), label.to_owned())
    }

    #[test]
    fn sites_name_the_lowest_labelled_target_for_each_want() {
        let app = node("//:app", |node| {
            node.run = command(&["ld", "$in"]);
            node.env = vec![env_name("PATH")];
            node.inputs = vec![path("src/main.c")];
        });
        let text = node("//lib:text", |node| {
            node.env = vec![env_name("PATH"), env_name("HOME")];
            node.inputs = vec![path("lib/text.c")];
            node.network = NetworkAccess::Declared;
        });
        let graph = graph(vec![app, text], &[&[], &[]]);
        let layout = Layout::of(&graph, &dir("out")).unwrap();
        let sites = sites(&graph, &BTreeMap::new(), &layout);
        let programs: Vec<(&str, String)> = sites
            .programs
            .iter()
            .map(|(program, node)| (program.as_str(), node.label.to_string()))
            .collect();
        let env: Vec<(&str, String)> = sites
            .env
            .iter()
            .map(|(name, node)| (name.as_str(), node.label.to_string()))
            .collect();
        let inputs: Vec<(&str, &Path)> = sites
            .inputs
            .iter()
            .map(|(path, node)| (path.as_str(), node.provenance.file()))
            .collect();
        assert_eq!(
            programs,
            [("cc", "//lib:text".to_owned()), ("ld", "//:app".to_owned())]
        );
        assert_eq!(
            env,
            [
                ("HOME", "//lib:text".to_owned()),
                ("PATH", "//:app".to_owned())
            ]
        );
        assert_eq!(
            inputs,
            [
                ("lib/text.c", Path::new("lib/build.lua")),
                ("src/main.c", Path::new("build.lua")),
            ]
        );
    }

    #[test]
    fn a_program_named_through_in_is_wanted() {
        let graph = graph(
            vec![node("//:run", |node| {
                node.run = command(&["$in", "--fast"]);
                node.inputs = vec![path("tools/run.sh")];
            })],
            &[&[]],
        );
        assert_eq!(
            wanted(&graph, &BTreeMap::new()),
            [pair("tools/run.sh", "//:run")]
        );
    }

    #[test]
    fn a_program_named_through_deps_is_not_wanted() {
        let graph = graph(
            vec![
                node("//:gen", |_| {}),
                node("//:use", |node| node.run = command(&["$deps", "$out"])),
            ],
            &[&[], &[0]],
        );
        assert_eq!(wanted(&graph, &BTreeMap::new()), [pair("cc", "//:gen")]);
    }

    #[test]
    fn a_written_out_dependency_output_is_not_wanted() {
        let graph = graph(
            vec![
                node("//:gen", |_| {}),
                node("//:use", |node| node.run = command(&["out/gen", "$out"])),
            ],
            &[&[], &[0]],
        );
        assert_eq!(wanted(&graph, &BTreeMap::new()), [pair("cc", "//:gen")]);
    }

    #[test]
    fn a_setting_naming_a_dependency_output_is_not_wanted() {
        let graph = graph(
            vec![
                node("//:gen", |_| {}),
                node("//:use", |node| {
                    node.run = command(&["$opt:tool", "$out"]);
                }),
            ],
            &[&[], &[0]],
        );
        assert_eq!(
            wanted(&graph, &settings(&[("tool", "out/gen")])),
            [pair("cc", "//:gen")]
        );
    }

    #[test]
    fn a_written_out_output_of_a_target_not_depended_on_is_wanted() {
        let graph = graph(
            vec![
                node("//:gen", |_| {}),
                node("//:use", |node| node.run = command(&["out/gen", "$out"])),
            ],
            &[&[], &[]],
        );
        assert_eq!(
            wanted(&graph, &BTreeMap::new()),
            [pair("cc", "//:gen"), pair("out/gen", "//:use")]
        );
    }

    #[test]
    fn a_program_named_through_out_is_wanted() {
        let graph = graph(
            vec![node("//:gen", |node| {
                node.run = command(&["$out", "--check"]);
            })],
            &[&[]],
        );
        assert_eq!(
            wanted(&graph, &BTreeMap::new()),
            [pair("out/gen", "//:gen")]
        );
    }

    #[test]
    fn a_command_naming_no_program_is_left_out() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["$opt:missing", "x"])),
                node("//:b", |node| node.run = command(&["", "x"])),
                node("//:c", |node| node.run = command(&["$in"])),
            ],
            &[&[], &[], &[]],
        );
        assert_eq!(wanted(&graph, &BTreeMap::new()), []);
    }

    #[test]
    fn each_program_names_its_lowest_labelled_target() {
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
            wanted(&graph, &BTreeMap::new()),
            [
                pair("ar", "//lib:d"),
                pair("cc", "//:b"),
                pair("ld", "//:a")
            ]
        );
    }
}
