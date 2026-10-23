//! The wants of a plugin's targets that a plugin does not allow.
//!
//! Its own file because it reports rather than refuses: the plan stops at the first violation,
//! while a report of a workspace's plugins lists every one of them.
//!
//! Responsibilities: [`plugin_violations`].
//!
//! Non-responsibilities: refusing a build, which the plan's check does; collecting what a
//! target wants beyond its program, which is [`wanted`](super::wanted)'s; and holding a want to
//! the workspace's ceiling, which [`outside`](super::outside) and the plan's check do.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::DeclarationSite;
use crate::grants::wanted::program_of;
use crate::placement::layout::Layout;
use crate::types::{
    NetworkAccess, NodeId, PluginCap, PluginNetwork, PluginViolation, SettingName, SettingValue,
    TargetGraph, Want, Workspace,
};

/// Every want of a plugin's target that one plugin holding it does not allow.
///
/// A target is held by each plugin in its `narrowing`: the chain that declared it, and the
/// chain of the rule it names. Its wants are the program its command runs, each environment
/// variable it reads, and the network when it declares it. A program one of its dependencies
/// builds is no want, as in the ceiling check. A want is allowed by a plugin only when the
/// plugin's manifest lists it and the workspace's cap for that plugin lists it too; a plugin
/// with no cap in `workspace` allows nothing, and the network is allowed only by a cap that
/// allows it. A target with an empty `narrowing` is held by no plugin.
///
/// The violations come in label order, then chain order, then program, variables in written
/// order, and network. Each names the plugin that does not allow the want, and what that plugin
/// allows of the same kind.
pub(crate) fn plugin_violations(
    graph: &TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
    layout: &Layout,
    workspace: &Workspace,
) -> Vec<PluginViolation> {
    let no_cap = PluginCap::default();
    let mut found = Vec::new();
    for (node, id) in graph.nodes().iter().zip((0_u32..).map(NodeId::new)) {
        if node.narrowing.is_empty() {
            continue;
        }
        let program = program_of(graph, node, id, settings, layout);
        let site = DeclarationSite::from(node);
        for member in &node.narrowing {
            let cap = workspace.plugins.get(&member.name).unwrap_or(&no_cap);
            let mut violation = |want: Want, allowed: BTreeSet<String>| {
                found.push(PluginViolation {
                    want,
                    site: site.clone(),
                    plugin: member.clone(),
                    allowed,
                });
            };
            if let Some(program) = &program
                && !(member.run.contains(program) && cap.run.contains(program))
            {
                violation(Want::Program(program.clone()), both(&member.run, &cap.run));
            }
            for name in &node.env {
                if !(member.env.contains(name) && cap.env.contains(name)) {
                    violation(Want::Env(name.clone()), both(&member.env, &cap.env));
                }
            }
            if node.network == NetworkAccess::Declared && cap.network != PluginNetwork::Allowed {
                violation(Want::Network(node.label.clone()), BTreeSet::new());
            }
        }
    }
    found
}

/// The entries both `manifest` and `cap` list, as text.
fn both<T: Ord + ToString>(manifest: &BTreeSet<T>, cap: &BTreeSet<T>) -> BTreeSet<String> {
    manifest
        .intersection(cap)
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::plugin_violations;
    use crate::grants::fixtures::{command, dir, env_name, graph, node, program};
    use crate::placement::layout::Layout;
    use crate::types::{
        ApproverMode, Ceiling, Execution, NetworkAccess, Node, Origin, PluginCap, PluginName,
        PluginNetwork, PluginRef, PluginVersion, PluginViolation, TargetGraph, Want, Workspace,
    };

    /// The plugin `name` at version 1.3.0, whose manifest lists `run` and `env`.
    fn plugin(name: &str, run: &[&str], env: &[&str]) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse("1.3.0").unwrap(),
            run: run.iter().map(|text| program(text)).collect(),
            env: env.iter().map(|text| env_name(text)).collect(),
        }
    }

    /// The cap letting a plugin run `run` and read `env`, with the network as `network`.
    fn cap(run: &[&str], env: &[&str], network: PluginNetwork) -> PluginCap {
        PluginCap {
            run: run.iter().map(|text| program(text)).collect(),
            env: env.iter().map(|text| env_name(text)).collect(),
            network,
        }
    }

    /// A workspace holding `caps`, one per plugin name.
    fn workspace(caps: Vec<(&str, PluginCap)>) -> Workspace {
        Workspace {
            out: dir("out"),
            state: dir(".buildl"),
            ceiling: Ceiling {
                run: BTreeSet::new(),
                env: BTreeSet::new(),
                read: BTreeSet::new(),
                write: BTreeSet::new(),
            },
            execution: Execution::default(),
            approver: ApproverMode::Manifest,
            plugins: caps
                .into_iter()
                .map(|(name, cap)| (PluginName::parse(name).unwrap(), cap))
                .collect(),
        }
    }

    /// A target `raw` running `run`, declared through the chain `chain`.
    fn by(raw: &str, run: &[&str], chain: &[PluginRef], change: impl FnOnce(&mut Node)) -> Node {
        node(raw, |node| {
            node.run = command(run);
            node.origin = Origin::plugins(chain.to_vec()).unwrap();
            node.narrowing = chain.to_vec();
            change(node);
        })
    }

    fn violations(graph: &TargetGraph, workspace: &Workspace) -> Vec<PluginViolation> {
        let layout = Layout::of(graph, &workspace.out).unwrap();
        plugin_violations(graph, &BTreeMap::new(), &layout, workspace)
    }

    fn texts(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|text| (*text).to_owned()).collect()
    }

    /// What a violation names: the want, the target, the plugin, and what the plugin allows.
    fn summary(violation: &PluginViolation) -> (Want, String, String, BTreeSet<String>) {
        (
            violation.want.clone(),
            violation.site.label.to_string(),
            violation.plugin.name.to_string(),
            violation.allowed.clone(),
        )
    }

    #[test]
    fn a_program_outside_the_manifest_is_refused() {
        let go = plugin("go", &["go"], &[]);
        let graph = graph(vec![by("//:app", &["curl"], &[go], |_| {})], &[&[]]);
        let workspace = workspace(vec![(
            "go",
            cap(&["go", "curl"], &[], PluginNetwork::Sealed),
        )]);
        let found = violations(&graph, &workspace);
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [(
                Want::Program(program("curl")),
                "//:app".to_owned(),
                "go".to_owned(),
                texts(&["go"])
            )]
        );
    }

    #[test]
    fn a_program_outside_the_workspace_cap_is_refused() {
        let go = plugin("go", &["go", "curl"], &[]);
        let graph = graph(vec![by("//:app", &["curl"], &[go], |_| {})], &[&[]]);
        let workspace = workspace(vec![("go", cap(&["go"], &[], PluginNetwork::Sealed))]);
        let found = violations(&graph, &workspace);
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [(
                Want::Program(program("curl")),
                "//:app".to_owned(),
                "go".to_owned(),
                texts(&["go"])
            )]
        );
    }

    #[test]
    fn a_plugin_with_no_cap_allows_nothing() {
        let go = plugin("go", &["go"], &[]);
        let graph = graph(vec![by("//:app", &["go"], &[go], |_| {})], &[&[]]);
        let found = violations(&graph, &workspace(Vec::new()));
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [(
                Want::Program(program("go")),
                "//:app".to_owned(),
                "go".to_owned(),
                BTreeSet::new()
            )]
        );
    }

    #[test]
    fn allowed_is_manifest_intersect_cap() {
        let go = plugin("go", &["go", "gofmt", "vet"], &[]);
        let graph = graph(vec![by("//:app", &["curl"], &[go], |_| {})], &[&[]]);
        let workspace = workspace(vec![(
            "go",
            cap(&["go", "vet", "curl"], &[], PluginNetwork::Sealed),
        )]);
        let found = violations(&graph, &workspace);
        assert_eq!(found[0].allowed, texts(&["go", "vet"]));
    }

    #[test]
    fn every_narrowing_member_is_checked() {
        let ourgo = plugin("ourgo", &["go", "gofmt"], &[]);
        let go = plugin("go", &["go"], &[]);
        let graph = graph(vec![by("//:app", &["gofmt"], &[ourgo, go], |_| {})], &[&[]]);
        let workspace = workspace(vec![
            ("ourgo", cap(&["go", "gofmt"], &[], PluginNetwork::Sealed)),
            ("go", cap(&["go", "gofmt"], &[], PluginNetwork::Sealed)),
        ]);
        let found = violations(&graph, &workspace);
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [(
                Want::Program(program("gofmt")),
                "//:app".to_owned(),
                "go".to_owned(),
                texts(&["go"])
            )]
        );
    }

    #[test]
    fn a_member_earlier_in_the_chain_is_listed_first() {
        let ourgo = plugin("ourgo", &["go"], &[]);
        let go = plugin("go", &["go"], &[]);
        let graph = graph(vec![by("//:app", &["cc"], &[ourgo, go], |_| {})], &[&[]]);
        let found = violations(&graph, &workspace(Vec::new()));
        let plugins: Vec<String> = found.iter().map(|v| v.plugin.name.to_string()).collect();
        assert_eq!(plugins, ["ourgo", "go"]);
    }

    #[test]
    fn a_plugin_rule_used_by_a_hand_written_target_is_checked() {
        let go = plugin("go", &["go"], &[]);
        let target = node("//:app", |node| {
            node.run = command(&["curl"]);
            node.narrowing = vec![go];
        });
        let graph = graph(vec![target], &[&[]]);
        let workspace = workspace(vec![(
            "go",
            cap(&["go", "curl"], &[], PluginNetwork::Sealed),
        )]);
        let found = violations(&graph, &workspace);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].want, Want::Program(program("curl")));
        assert_eq!(found[0].site.origin, Origin::BuildFile);
    }

    #[test]
    fn a_variable_needs_the_manifest_and_the_cap() {
        let go = plugin("go", &["go"], &["GOPATH", "HOME"]);
        let graph = graph(
            vec![by("//:app", &["go"], &[go], |node| {
                node.env = vec![env_name("GOPATH"), env_name("HOME"), env_name("PATH")];
            })],
            &[&[]],
        );
        let workspace = workspace(vec![(
            "go",
            cap(&["go"], &["GOPATH", "PATH"], PluginNetwork::Sealed),
        )]);
        let found = violations(&graph, &workspace);
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [
                (
                    Want::Env(env_name("HOME")),
                    "//:app".to_owned(),
                    "go".to_owned(),
                    texts(&["GOPATH"])
                ),
                (
                    Want::Env(env_name("PATH")),
                    "//:app".to_owned(),
                    "go".to_owned(),
                    texts(&["GOPATH"])
                ),
            ]
        );
    }

    #[test]
    fn network_needs_the_cap() {
        let go = plugin("go", &["go"], &[]);
        let fetch = |go: &PluginRef| {
            by("//:fetch", &["go"], std::slice::from_ref(go), |node| {
                node.network = NetworkAccess::Declared;
            })
        };

        let sealed = workspace(vec![("go", cap(&["go"], &[], PluginNetwork::Sealed))]);
        let found = violations(&graph(vec![fetch(&go)], &[&[]]), &sealed);
        assert_eq!(
            found.iter().map(summary).collect::<Vec<_>>(),
            [(
                Want::Network(crate::grants::fixtures::label("//:fetch")),
                "//:fetch".to_owned(),
                "go".to_owned(),
                BTreeSet::new()
            )]
        );

        let allowed = workspace(vec![("go", cap(&["go"], &[], PluginNetwork::Allowed))]);
        assert_eq!(
            violations(&graph(vec![fetch(&go)], &[&[]]), &allowed),
            Vec::new()
        );
    }

    #[test]
    fn a_built_program_is_not_held_to_a_plugin() {
        let go = plugin("go", &["go"], &[]);
        let generator = node("//:gen", |node| node.run = command(&["cc"]));
        let tool = by("//:use", &["out/gen/gen"], &[go], |_| {});
        let graph = graph(vec![generator, tool], &[&[], &[0]]);
        let workspace = workspace(vec![("go", cap(&["go"], &[], PluginNetwork::Sealed))]);
        assert_eq!(violations(&graph, &workspace), Vec::new());
    }

    #[test]
    fn build_file_targets_are_not_checked() {
        let graph = graph(
            vec![node("//:app", |node| {
                node.run = command(&["curl"]);
                node.env = vec![env_name("HOME")];
                node.network = NetworkAccess::Declared;
            })],
            &[&[]],
        );
        assert_eq!(violations(&graph, &workspace(Vec::new())), Vec::new());
    }

    #[test]
    fn violations_are_listed_in_label_order() {
        let go = plugin("go", &["go"], &[]);
        let graph = graph(
            vec![
                by("//:a", &["curl"], std::slice::from_ref(&go), |_| {}),
                by("//:b", &["curl"], std::slice::from_ref(&go), |_| {}),
                by("//lib:c", &["curl"], &[go], |_| {}),
            ],
            &[&[], &[], &[]],
        );
        let workspace = workspace(vec![("go", cap(&["go"], &[], PluginNetwork::Sealed))]);
        let labels: Vec<String> = violations(&graph, &workspace)
            .iter()
            .map(|violation| violation.site.label.to_string())
            .collect();
        assert_eq!(labels, ["//:a", "//:b", "//lib:c"]);
    }
}
