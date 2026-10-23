//! Which plugins' targets are behind each want of a graph.
//!
//! Its own file because attribution is computed beside the wanted set, never inside it: the set
//! keeps the shape an approval remembers, and this map only explains it.
//!
//! Responsibilities: [`attribution`].
//!
//! Non-responsibilities: deciding what a target wants, which is [`wanted`](super::wanted)'s;
//! holding a want to a plugin's caps, which [`plugins`](super::plugins) does; and rendering,
//! which is [`Attribution`]'s.

use std::collections::BTreeMap;

use crate::grants::wanted::program_of;
use crate::placement::layout::Layout;
use crate::types::{
    Attribution, NetworkAccess, NodeId, Origin, SettingName, SettingValue, TargetGraph, Want,
};

/// The attribution of `graph`'s wants, when its build settings have the values `settings` gives
/// them and its outputs are placed as `layout` places them.
///
/// A target declared by plugins wants the program its command runs, each variable it reads, and
/// the network when it declares it, as the wanted set counts them: a program one of its
/// dependencies builds is no want. Each want is attributed to the chain in the target's
/// `origin`, with the lowest-labelled target of that origin wanting it. A target written in a
/// build file attributes nothing, even when it names a plugin's rule and so answers to that
/// plugin's caps.
pub(crate) fn attribution(
    graph: &TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
    layout: &Layout,
) -> Attribution {
    let mut attribution = Attribution::default();
    for (node, id) in graph.nodes().iter().zip((0_u32..).map(NodeId::new)) {
        let Origin::Plugins(chain) = &node.origin else {
            continue;
        };
        if let Some(program) = program_of(graph, node, id, settings, layout) {
            attribution.record(Want::Program(program), chain, &node.label);
        }
        for name in &node.env {
            attribution.record(Want::Env(name.clone()), chain, &node.label);
        }
        if node.network == NetworkAccess::Declared {
            attribution.record(Want::Network(node.label.clone()), chain, &node.label);
        }
    }
    attribution
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::attribution;
    use crate::grants::fixtures::{command, dir, env_name, graph, label, node, program};
    use crate::placement::layout::Layout;
    use crate::types::{
        Attribution, NetworkAccess, Node, Origin, PluginName, PluginRef, PluginVersion,
        TargetGraph, Want,
    };

    /// The plugin `name` at `version`.
    fn plugin(name: &str, version: &str) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse(version).unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }
    }

    /// A target `raw` running `run`, declared through `chain`.
    fn by(raw: &str, run: &[&str], chain: &[PluginRef], change: impl FnOnce(&mut Node)) -> Node {
        node(raw, |node| {
            node.run = command(run);
            node.origin = Origin::plugins(chain.to_vec()).unwrap();
            node.narrowing = chain.to_vec();
            change(node);
        })
    }

    fn attributed(graph: &TargetGraph) -> Attribution {
        let layout = Layout::of(graph, &dir("out")).unwrap();
        attribution(graph, &BTreeMap::new(), &layout)
    }

    /// The text `attribution` shows, one entry per line.
    fn lines(attribution: &Attribution) -> Vec<String> {
        attribution.to_string().lines().map(str::to_owned).collect()
    }

    #[test]
    fn a_plugin_program_names_its_chain_and_lowest_target() {
        let go = plugin("go", "1.2.0");
        let graph = graph(
            vec![
                by("//:b", &["go"], std::slice::from_ref(&go), |_| {}),
                by("//lib:a", &["go"], &[go], |_| {}),
            ],
            &[&[], &[]],
        );
        assert_eq!(
            lines(&attributed(&graph)),
            ["  go: declared by plugin go@1.2.0 for //:b"]
        );
    }

    #[test]
    fn two_chains_wanting_one_program_both_appear() {
        let go = plugin("go", "1.2.0");
        let ourgo = [plugin("ourgo", "0.1.0"), go.clone()];
        let graph = graph(
            vec![
                by("//:a", &["go"], &ourgo, |_| {}),
                by("//:b", &["go"], &[go], |_| {}),
            ],
            &[&[], &[]],
        );
        assert_eq!(
            lines(&attributed(&graph)),
            [
                "  go: declared by plugin go@1.2.0 for //:b",
                "  go: declared by plugin ourgo@0.1.0 > go@1.2.0 for //:a",
            ]
        );
    }

    #[test]
    fn build_file_only_wants_have_no_entry() {
        let go = plugin("go", "1.2.0");
        let graph = graph(
            vec![
                node("//:app", |node| {
                    node.run = command(&["curl"]);
                    node.env = vec![env_name("HOME")];
                    node.network = NetworkAccess::Declared;
                }),
                by("//:gen", &["go"], &[go], |_| {}),
            ],
            &[&[], &[]],
        );
        let attribution = attributed(&graph);
        assert!(attribution.get(&Want::Program(program("curl"))).is_none());
        assert!(attribution.get(&Want::Env(env_name("HOME"))).is_none());
        assert!(attribution.get(&Want::Network(label("//:app"))).is_none());
        assert_eq!(lines(&attribution).len(), 1);
    }

    #[test]
    fn network_targets_are_attributed_by_label() {
        let go = plugin("go", "1.2.0");
        let fetch = |raw: &str| {
            by(raw, &["go"], std::slice::from_ref(&go), |node| {
                node.network = NetworkAccess::Declared;
                node.env = vec![env_name("GOPATH")];
            })
        };
        let graph = graph(vec![fetch("//:one"), fetch("//:two")], &[&[], &[]]);
        assert_eq!(
            lines(&attributed(&graph)),
            [
                "  go: declared by plugin go@1.2.0 for //:one",
                "  GOPATH: declared by plugin go@1.2.0 for //:one",
                "  //:one: declared by plugin go@1.2.0 for //:one",
                "  //:two: declared by plugin go@1.2.0 for //:two",
            ]
        );
    }

    #[test]
    fn a_hand_written_target_using_a_plugin_rule_is_not_attributed() {
        let go = plugin("go", "1.2.0");
        let graph = graph(
            vec![node("//:app", |node| {
                node.run = command(&["go"]);
                node.env = vec![env_name("GOPATH")];
                node.narrowing = vec![go];
            })],
            &[&[]],
        );
        assert!(attributed(&graph).is_empty());
    }

    #[test]
    fn a_program_a_dependency_built_is_not_attributed() {
        let go = plugin("go", "1.2.0");
        let generator = node("//:gen", |node| node.run = command(&["cc"]));
        let tool = by("//:use", &["out/gen/gen"], &[go], |_| {});
        let graph = graph(vec![generator, tool], &[&[], &[0]]);
        assert!(attributed(&graph).is_empty());
    }
}
