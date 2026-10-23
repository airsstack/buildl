//! The assembly: from every declaration to one target graph.
//!
//! Its own file because it is where this phase's steps meet and where their order is fixed:
//! index the names, number the targets, check every reference while building nodes and edges,
//! then search the edges for a cycle.
//!
//! Responsibilities: [`resolve`], and the private steps that number targets and build one node.
//!
//! Non-responsibilities: what a name collision is, what a reference may name, and how a cycle
//! is found. Each has its own sibling file.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DeclarationSite, Error, Result};
use crate::resolve::cycle;
use crate::resolve::index::{Index, Item, index};
use crate::resolve::references::{self, Holder};
use crate::types::{
    Action, Declaration, Label, Node, NodeId, Origin, PluginRef, Target, TargetGraph,
};

/// Builds the target graph of a workspace from the declarations of its build files.
///
/// Every build or test target becomes a node, numbered in label order, holding the command it
/// runs: its own, or a copy of the rule it names. Every dependency becomes an edge, in the order
/// it was written; a dependency on an alias becomes an edge to the target the alias names.
/// Aliases and build settings are kept beside the nodes. Each node keeps the origin of its
/// declaration and the plugins that hold it: its own chain, then the chain of the rule it names.
/// The result does not depend on the order of `declarations`.
///
/// # Errors
///
/// Returns the first failure met, in this order:
///
/// 1. [`Error::DuplicateLabel`], then [`Error::DuplicateSetting`], for a name declared more
///    than once;
/// 2. [`Error::TooManyTargets`] when the targets outnumber the ids a graph has;
/// 3. walking the declarations in label order, the first reference that is wrong:
///    [`Error::UnknownReference`], [`Error::WrongReferenceKind`] or [`Error::UnknownSetting`].
///    Within a target the rule reference is checked first, then its dependencies in written
///    order, then the settings its own command names. Within a rule, the settings its command
///    names are checked first, then the settings its description names;
/// 4. [`Error::DependencyCycle`] when the dependencies loop.
///
/// # Examples
///
/// ```
/// use buildl_core::{Label, resolve};
///
/// let graph = resolve(Vec::new())?;
/// assert!(graph.is_empty());
/// assert_eq!(graph.id_of(&Label::parse("//:app")?), None);
/// # Ok::<(), buildl_core::Error>(())
/// ```
pub fn resolve(declarations: Vec<Declaration>) -> Result<TargetGraph> {
    let index = index(declarations)?;
    let ids = number(&index)?;

    let mut nodes = Vec::with_capacity(ids.len());
    let mut edges = Vec::with_capacity(ids.len());
    let mut aliases = BTreeMap::new();
    for (label, entry) in &index.labels {
        let holder = Holder {
            label,
            provenance: &entry.provenance,
            origin: &entry.origin,
        };
        match &entry.item {
            Item::Target(target) => {
                let (node, deps) = node(&index, &ids, holder, &entry.origin, target)?;
                nodes.push(node);
                edges.push(deps);
            }
            Item::Rule(rule) => {
                references::settings(&index, holder, &rule.run)?;
                if let Some(description) = &rule.description {
                    references::description_settings(&index, holder, description)?;
                }
            }
            Item::Alias(alias) => {
                let target = references::alias_target(&index, holder, &alias.target)?;
                if let Some(id) = ids.get(target) {
                    aliases.insert(label.clone(), *id);
                }
            }
        }
    }

    if let Some(cycle) = cycle::find(&edges) {
        let path = cycle
            .iter()
            .filter_map(|id| nodes.get(id.index()))
            .map(DeclarationSite::from)
            .collect();
        return Err(Error::DependencyCycle { path });
    }
    Ok(TargetGraph::new(nodes, edges, aliases, index.settings))
}

/// Numbers the targets of `index` in label order.
fn number(index: &Index) -> Result<BTreeMap<&Label, NodeId>> {
    let targets = index
        .labels
        .iter()
        .filter(|(_, entry)| matches!(entry.item, Item::Target(_)))
        .map(|(label, _)| label);
    let count = targets.clone().count();
    targets
        .enumerate()
        .map(|(position, label)| Ok((label, node_id(position, count)?)))
        .collect()
}

/// The id of the target at `position` among `count` targets.
fn node_id(position: usize, count: usize) -> Result<NodeId> {
    u32::try_from(position)
        .map(NodeId::new)
        .map_err(|_| Error::TooManyTargets { count })
}

/// Builds the node of `target` and the ids it depends on, checking each of its references.
fn node(
    index: &Index,
    ids: &BTreeMap<&Label, NodeId>,
    holder: Holder<'_>,
    origin: &Origin,
    target: &Target,
) -> Result<(Node, Vec<NodeId>)> {
    let (run, description, own_command, rule_origin) = match &target.action {
        Action::UseRule(reference) => {
            let (rule, rule_origin) = references::rule(index, holder, reference)?;
            (
                &rule.run,
                rule.description.as_ref(),
                None,
                Some(rule_origin),
            )
        }
        Action::Run(command) => (command, None, Some(command), None),
    };

    let mut deps = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in &target.deps {
        let resolved = references::dependency(index, holder, reference)?;
        if let Some(id) = ids.get(resolved)
            && seen.insert(*id)
        {
            deps.push(*id);
        }
    }

    if let Some(command) = own_command {
        references::settings(index, holder, command)?;
    }

    let node = Node {
        label: target.label.clone(),
        provenance: holder.provenance.clone(),
        origin: origin.clone(),
        narrowing: narrowing(origin, rule_origin),
        role: target.role,
        run: run.clone(),
        description: description.cloned(),
        inputs: without_repeats(&target.inputs),
        outputs: without_repeats(&target.outputs),
        env: without_repeats(&target.env),
        network: target.network,
        freshness: target.freshness,
    };
    Ok((node, deps))
}

/// The plugins whose caps hold a target: the chain of its `origin`, then each plugin of the chain
/// of the rule it names, when it names one, that the first chain does not already hold.
fn narrowing(origin: &Origin, rule_origin: Option<&Origin>) -> Vec<PluginRef> {
    let chain = |origin: &Origin| match origin {
        Origin::BuildFile => Vec::new(),
        Origin::Plugins(chain) => chain.as_slice().to_vec(),
    };
    let mut joined = chain(origin);
    for plugin in rule_origin.map(chain).unwrap_or_default() {
        if !joined.contains(&plugin) {
            joined.push(plugin);
        }
    }
    joined
}

/// `items` in the order given, keeping the first occurrence of each.
fn without_repeats<T: Ord + Clone>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .filter(|item| seen.insert(*item))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::Path;

    use super::{node_id, resolve, without_repeats};
    use crate::error::{DeclarationField, Error};
    use crate::resolve::fixtures::{
        alias, command, declared_in, label, made_by, plugin, rule, setting, setting_item, target,
        target_item, target_with,
    };
    use crate::types::{
        Action, Declaration, Declared, Description, EnvName, NodeId, Origin, OutputName, Rule,
        SourcePath, TargetGraph,
    };

    fn id(index: u32) -> NodeId {
        NodeId::new(index)
    }

    /// A rule running `run`, shown as `description` while it runs.
    fn described_rule(raw: &str, run: &[&str], description: &str) -> Declaration {
        let label = label(raw);
        declared_in(
            label.directory().as_str(),
            Declared::Rule(Rule {
                label: label.clone(),
                run: command(run),
                description: Some(Description::parse(description).unwrap()),
            }),
        )
    }

    /// The labels `label` depends on in `graph`, in edge order.
    fn deps_of(graph: &TargetGraph, raw: &str) -> Vec<String> {
        let id = graph.id_of(&label(raw)).unwrap();
        graph
            .deps(id)
            .unwrap()
            .iter()
            .map(|dep| graph.node(*dep).unwrap().label.to_string())
            .collect()
    }

    fn labels(graph: &TargetGraph) -> Vec<String> {
        graph
            .nodes()
            .iter()
            .map(|node| node.label.to_string())
            .collect()
    }

    #[test]
    fn nothing_declared_resolves_to_an_empty_graph() {
        assert!(resolve(Vec::new()).unwrap().is_empty());
    }

    #[test]
    fn targets_are_numbered_in_label_order_and_rules_and_aliases_are_not_nodes() {
        let graph = resolve(vec![
            target("//lib:text", &[]),
            rule("//:cc", &["cc"]),
            alias("//:default", "//:app"),
            target("//:app", &["//lib:text"]),
            setting("test_filter"),
        ])
        .unwrap();
        assert_eq!(labels(&graph), ["//:app", "//lib:text"]);
        assert_eq!(graph.id_of(&label("//:app")), Some(id(0)));
        assert_eq!(graph.id_of(&label("//:default")), Some(id(0)));
        assert_eq!(graph.id_of(&label("//:cc")), None);
        assert_eq!(graph.dependents(id(1)), Some(&[id(0)][..]));
        assert_eq!(graph.settings().len(), 1);
        assert_eq!(
            graph.node(id(1)).unwrap().provenance.file(),
            Path::new("lib/build.lua")
        );
    }

    #[test]
    fn a_target_naming_a_rule_runs_the_rules_command_under_its_description() {
        let graph = resolve(vec![
            described_rule("//tools:cc", &["cc", "-c", "$in"], "compile $in"),
            target_with("//:main.o", |target| {
                target.action = Action::UseRule(label("//tools:cc"));
            }),
            target("//:plain", &[]),
        ])
        .unwrap();
        let main = graph
            .node(graph.id_of(&label("//:main.o")).unwrap())
            .unwrap();
        assert_eq!(main.run, command(&["cc", "-c", "$in"]));
        assert_eq!(
            main.description.as_ref().map(Description::as_str),
            Some("compile $in")
        );
        let plain = graph
            .node(graph.id_of(&label("//:plain")).unwrap())
            .unwrap();
        assert_eq!(plain.run, command(&["cc"]));
        assert!(plain.description.is_none());
    }

    fn using_rule(raw: &str, rule: &str) -> Declaration {
        target_with(raw, |target| target.action = Action::UseRule(label(rule)))
    }

    #[test]
    fn a_hand_written_target_naming_a_plugin_rule_has_build_file_origin_and_that_plugin_in_narrowing()
     {
        let graph = resolve(vec![
            made_by(vec![plugin("go")], rule("//tools:go", &["go"])),
            using_rule("//:app", "//tools:go"),
        ])
        .unwrap();
        let node = graph.node(graph.id_of(&label("//:app")).unwrap()).unwrap();
        assert_eq!(node.origin, Origin::BuildFile);
        assert_eq!(node.narrowing, [plugin("go")]);
    }

    #[test]
    fn narrowing_joins_target_and_rule_chains_without_repeats() {
        let chain = vec![plugin("ourgo"), plugin("go")];
        let graph = resolve(vec![
            made_by(vec![plugin("go")], rule("//tools:go", &["go"])),
            made_by(chain.clone(), using_rule("//:app", "//tools:go")),
        ])
        .unwrap();
        let node = graph.node(graph.id_of(&label("//:app")).unwrap()).unwrap();
        assert_eq!(node.origin, Origin::plugins(chain.clone()).unwrap());
        assert_eq!(node.narrowing, chain);
    }

    #[test]
    fn a_target_with_its_own_command_and_a_build_file_origin_narrows_to_nothing() {
        let graph = resolve(vec![target("//:app", &[])]).unwrap();
        assert_eq!(graph.nodes()[0].origin, Origin::BuildFile);
        assert_eq!(graph.nodes()[0].narrowing, []);
    }

    #[test]
    fn dependencies_keep_their_written_order_and_lose_their_repeats() {
        let graph = resolve(vec![
            target("//:a", &[]),
            target("//:b", &[]),
            target("//:c", &[]),
            target("//:app", &["//:c", "//:a", "//:c", "//:b", "//:a"]),
        ])
        .unwrap();
        assert_eq!(deps_of(&graph, "//:app"), ["//:c", "//:a", "//:b"]);
    }

    #[test]
    fn a_dependency_on_an_alias_is_an_edge_to_the_target_it_names() {
        let graph = resolve(vec![
            target("//:app", &[]),
            alias("//:default", "//:app"),
            target("//:test", &["//:default", "//:app"]),
        ])
        .unwrap();
        assert_eq!(deps_of(&graph, "//:test"), ["//:app"]);
    }

    #[test]
    fn a_nodes_lists_keep_their_written_order_and_lose_their_repeats() {
        let graph = resolve(vec![target_with("//:app", |target| {
            target.inputs = ["z.c", "a.c", "z.c"]
                .map(|path| SourcePath::parse(path).unwrap())
                .to_vec();
            target.outputs = ["out.b", "out.a", "out.b"]
                .map(|name| OutputName::parse(name).unwrap())
                .to_vec();
            target.env = ["PATH", "HOME", "PATH"]
                .map(|name| EnvName::parse(name).unwrap())
                .to_vec();
        })])
        .unwrap();
        let node = &graph.nodes()[0];
        let inputs: Vec<&str> = node.inputs.iter().map(SourcePath::as_str).collect();
        let outputs: Vec<&str> = node.outputs.iter().map(OutputName::as_str).collect();
        let env: Vec<&str> = node.env.iter().map(EnvName::as_str).collect();
        assert_eq!(inputs, ["z.c", "a.c"]);
        assert_eq!(outputs, ["out.b", "out.a"]);
        assert_eq!(env, ["PATH", "HOME"]);
    }

    #[test]
    fn without_repeats_keeps_the_first_of_each() {
        assert_eq!(without_repeats(&[3, 1, 3, 2, 1]), [3, 1, 2]);
        assert_eq!(without_repeats::<u8>(&[]), [0_u8; 0]);
    }

    #[test]
    fn the_order_of_the_declarations_does_not_change_the_graph() {
        let declarations = || {
            vec![
                target("//:app", &["//lib:text", "//:main.o"]),
                target("//:main.o", &[]),
                target("//lib:text", &[]),
                alias("//:default", "//:app"),
                rule("//:cc", &["cc", "$opt:mode"]),
                setting("mode"),
            ]
        };
        let forward = resolve(declarations()).unwrap();
        let mut reversed = declarations();
        reversed.reverse();
        assert_eq!(resolve(reversed).unwrap(), forward);
    }

    #[test]
    fn a_duplicate_is_reported_before_a_bad_reference() {
        let err = resolve(vec![
            target("//:app", &["//:nope"]),
            target("//:z", &[]),
            target("//:z", &[]),
        ])
        .unwrap_err();
        assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
    }

    #[test]
    fn a_bad_reference_is_reported_before_a_cycle() {
        let err = resolve(vec![
            target("//:a", &["//:b"]),
            target("//:b", &["//:a"]),
            target("//:z", &["//:nope"]),
        ])
        .unwrap_err();
        assert!(matches!(err, Error::UnknownReference { .. }), "{err:?}");
    }

    #[test]
    fn the_bad_reference_of_the_lowest_label_is_the_one_reported() {
        let err = resolve(vec![
            target("//:z", &["//:nope"]),
            alias("//:m", "//:nope"),
            target("//:b", &["//:missing"]),
        ])
        .unwrap_err();
        match err {
            Error::UnknownReference {
                site, reference, ..
            } => {
                assert_eq!(site.label, label("//:b"));
                assert_eq!(reference, label("//:missing"));
            }
            other => unreachable!("expected UnknownReference, got {other:?}"),
        }
    }

    #[test]
    fn within_a_target_the_rule_then_the_deps_then_its_own_settings_are_checked() {
        // A wrong rule reference wins over a wrong dependency.
        let err = resolve(vec![target_with("//:app", |target| {
            target.action = Action::UseRule(label("//:no-rule"));
            target.deps = vec![label("//:no-dep")];
        })])
        .unwrap_err();
        match err {
            Error::UnknownReference { field, .. } => assert_eq!(field, DeclarationField::Rule),
            other => unreachable!("expected UnknownReference, got {other:?}"),
        }
        // A wrong dependency wins over an undeclared setting, and the first written dependency
        // over a later one.
        let err = resolve(vec![target_with("//:app", |target| {
            target.action = Action::Run(command(&["go", "$opt:nope"]));
            target.deps = vec![label("//:second"), label("//:first")];
        })])
        .unwrap_err();
        match err {
            Error::UnknownReference { reference, .. } => assert_eq!(reference, label("//:second")),
            other => unreachable!("expected UnknownReference, got {other:?}"),
        }
        // With its dependencies right, the target's own command is checked.
        let err = resolve(vec![target_with("//:app", |target| {
            target.action = Action::Run(command(&["go", "$opt:nope"]));
        })])
        .unwrap_err();
        assert!(matches!(err, Error::UnknownSetting { .. }), "{err:?}");
    }

    #[test]
    fn a_rules_command_is_checked_once_at_the_rule_even_with_no_user() {
        let err = resolve(vec![rule("//tools:go", &["go", "$opt:nope"])]).unwrap_err();
        match err {
            Error::UnknownSetting { site, name, .. } => {
                assert_eq!(site.label, label("//tools:go"));
                assert_eq!(site.provenance.file(), Path::new("tools/build.lua"));
                assert_eq!(name, "nope");
            }
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
    }

    #[test]
    fn a_rules_description_is_checked_at_the_rule_after_its_command() {
        let err = resolve(vec![
            described_rule("//tools:cc", &["cc"], "compile $in for $opt:nope"),
            setting("mode"),
        ])
        .unwrap_err();
        match err {
            Error::UnknownSetting { site, name, .. } => {
                assert_eq!(site.label, label("//tools:cc"));
                assert_eq!(site.provenance.file(), Path::new("tools/build.lua"));
                assert_eq!(name, "nope");
            }
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
        let err = resolve(vec![described_rule(
            "//tools:cc",
            &["cc", "$opt:first"],
            "$opt:second",
        )])
        .unwrap_err();
        match err {
            Error::UnknownSetting { name, .. } => assert_eq!(name, "first"),
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
        assert!(
            resolve(vec![
                described_rule("//tools:cc", &["cc"], "compile $in in $opt:mode"),
                setting("mode"),
            ])
            .is_ok(),
            "a declared setting passes"
        );
    }

    #[test]
    fn a_target_using_a_rule_adds_no_settings_check_of_its_own() {
        // `//:a` sorts before the rule: were the rule's command checked at its user, the error
        // would name `//:a`.
        let err = resolve(vec![
            target_with("//:a", |target| {
                target.action = Action::UseRule(label("//:go"));
            }),
            rule("//:go", &["go", "$opt:nope"]),
        ])
        .unwrap_err();
        match err {
            Error::UnknownSetting { site, .. } => assert_eq!(site.label, label("//:go")),
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
    }

    #[test]
    fn an_alias_for_something_that_is_not_a_target_is_the_aliases_own_error() {
        let err = resolve(vec![target("//:app", &["//:x"]), alias("//:x", "//:nope")]).unwrap_err();
        match err {
            Error::UnknownReference { site, field, .. } => {
                assert_eq!(site.label, label("//:x"));
                assert_eq!(field, DeclarationField::Target);
            }
            other => unreachable!("expected UnknownReference, got {other:?}"),
        }
    }

    fn cycle_path(err: Error) -> Vec<(String, String)> {
        match err {
            Error::DependencyCycle { path } => path
                .iter()
                .map(|site| (site.label.to_string(), site.provenance.to_string()))
                .collect(),
            other => unreachable!("expected DependencyCycle, got {other:?}"),
        }
    }

    fn step(label: &str, file: &str) -> (String, String) {
        (label.to_owned(), file.to_owned())
    }

    #[test]
    fn a_cycle_names_every_target_on_it_and_its_build_file_lowest_label_first() {
        let err = resolve(vec![
            target("//lib:b", &["//:c"]),
            target("//:c", &["//:a"]),
            target("//:a", &["//lib:b"]),
            target("//:free", &["//:a"]),
        ])
        .unwrap_err();
        assert_eq!(
            cycle_path(err),
            [
                step("//:a", "build.lua"),
                step("//lib:b", "lib/build.lua"),
                step("//:c", "build.lua"),
            ]
        );
    }

    #[test]
    fn a_target_depending_on_itself_is_a_cycle_of_one() {
        let err = resolve(vec![target("//:a", &["//:a"])]).unwrap_err();
        assert_eq!(cycle_path(err), [step("//:a", "build.lua")]);
    }

    #[test]
    fn a_cycle_closed_through_an_alias_names_the_targets_only() {
        let err = resolve(vec![target("//:a", &["//:x"]), alias("//:x", "//:a")]).unwrap_err();
        assert_eq!(cycle_path(err), [step("//:a", "build.lua")]);
    }

    #[test]
    fn a_duplicate_across_hand_built_files_names_each_file() {
        let err = resolve(vec![
            declared_in("lib", target_item("//:a", |_| {})),
            declared_in("", target_item("//:a", |_| {})),
            declared_in("", setting_item("s")),
        ])
        .unwrap_err();
        match err {
            Error::DuplicateLabel {
                label: found,
                sites,
            } => {
                assert_eq!(found, label("//:a"));
                assert_eq!(sites.len(), 2);
            }
            other => unreachable!("expected DuplicateLabel, got {other:?}"),
        }
    }

    #[test]
    fn a_position_within_u32_is_an_id() {
        assert_eq!(node_id(0, 1).unwrap(), id(0));
        let last = usize::try_from(u32::MAX).unwrap();
        assert_eq!(node_id(last, last + 1).unwrap(), id(u32::MAX));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn a_position_past_u32_is_too_many_targets() {
        let past = usize::try_from(u32::MAX).unwrap() + 1;
        match node_id(past, past + 1).unwrap_err() {
            Error::TooManyTargets { count } => assert_eq!(count, 4_294_967_297),
            other => unreachable!("expected TooManyTargets, got {other:?}"),
        }
    }
}
