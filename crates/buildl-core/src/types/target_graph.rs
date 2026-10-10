//! The target graph: a workspace's runnable targets and the dependencies between them.
//!
//! Its own file because the graph is a different value from the declarations it is built from:
//! a declaration names its dependencies by label, a graph holds them as edges between nodes, and
//! every phase after the graph is built addresses a target by its [`NodeId`].
//!
//! Responsibilities: [`Node`], [`TargetGraph`], its accessors, and its serialized form.
//!
//! Non-responsibilities: deciding what a graph holds. Which labels collide, what a reference
//! names and whether the dependencies loop are decided by the phase that builds the graph; this
//! file stores the result.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};

use crate::types::{
    Command, Description, EnvName, Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance,
    SettingName, SettingValue, SourcePath, TargetRole,
};

/// One runnable target: what a build or test target became once its references were resolved.
///
/// A node holds the command it runs, whether the target wrote it inline or named a rule. Its
/// dependencies are not here: they are the graph's edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The target's name.
    pub label: Label,
    /// The build file that declared it.
    pub provenance: Provenance,
    /// Whether it builds or tests.
    pub role: TargetRole,
    /// The command it runs: its own, or the one of the rule it named.
    pub run: Command,
    /// The status-line text of the rule it named; `None` for an inline command, or when the
    /// rule has none.
    pub description: Option<Description>,
    /// The workspace files it reads, in written order without repeats.
    pub inputs: Vec<SourcePath>,
    /// The files it produces, in written order without repeats.
    pub outputs: Vec<OutputName>,
    /// The environment variables it reads, in written order without repeats.
    pub env: Vec<EnvName>,
    /// Whether it may reach the network.
    pub network: NetworkAccess,
    /// Whether its result may be reused.
    pub freshness: Freshness,
}

/// The targets of a workspace and the dependencies between them.
///
/// Nodes are numbered in [`Label`] order, so a [`NodeId`] does not depend on the order build
/// files were evaluated in. A graph is obtained from [`resolve`](fn@crate::resolve), which
/// guarantees that its dependencies form no cycle and that every edge names a node of the same
/// graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetGraph {
    nodes: Vec<Node>,
    edges: Vec<Vec<NodeId>>,
    reverse: Vec<Vec<NodeId>>,
    by_label: BTreeMap<Label, NodeId>,
    aliases: BTreeMap<Label, NodeId>,
    settings: BTreeMap<SettingName, SettingValue>,
}

impl TargetGraph {
    /// Assembles a graph from its nodes and their dependencies.
    ///
    /// `edges` holds one list per node, in the same order as `nodes`. The reverse edges and the
    /// label index are derived here. The caller guarantees the rest: `nodes` is in label order,
    /// every edge and alias names a node of `nodes`, no list repeats an id, and the edges form
    /// no cycle.
    pub(crate) fn new(
        nodes: Vec<Node>,
        edges: Vec<Vec<NodeId>>,
        aliases: BTreeMap<Label, NodeId>,
        settings: BTreeMap<SettingName, SettingValue>,
    ) -> Self {
        let mut reverse = vec![Vec::new(); nodes.len()];
        for (deps, id) in edges.iter().zip(0_u32..) {
            for dep in deps {
                if let Some(dependents) = reverse.get_mut(dep.index()) {
                    dependents.push(NodeId::new(id));
                }
            }
        }
        let by_label = nodes
            .iter()
            .zip(0_u32..)
            .map(|(node, id)| (node.label.clone(), NodeId::new(id)))
            .collect();
        Self {
            nodes,
            edges,
            reverse,
            by_label,
            aliases,
            settings,
        }
    }

    /// The number of targets.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph holds no target.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node, in id order.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The node `id` names; `None` when this graph did not issue `id`.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// The nodes `id` depends on, in the order its dependencies were written; `None` when this
    /// graph did not issue `id`.
    ///
    /// The length of the list is the node's dependency count.
    #[must_use]
    pub fn deps(&self, id: NodeId) -> Option<&[NodeId]> {
        self.edges.get(id.index()).map(Vec::as_slice)
    }

    /// The nodes that depend on `id`, in id order; `None` when this graph did not issue `id`.
    #[must_use]
    pub fn dependents(&self, id: NodeId) -> Option<&[NodeId]> {
        self.reverse.get(id.index()).map(Vec::as_slice)
    }

    /// The id of the target `label` names, directly or as an alias.
    #[must_use]
    pub fn id_of(&self, label: &Label) -> Option<NodeId> {
        self.by_label
            .get(label)
            .or_else(|| self.aliases.get(label))
            .copied()
    }

    /// Every alias, with the id of the target it names.
    #[must_use]
    pub const fn aliases(&self) -> &BTreeMap<Label, NodeId> {
        &self.aliases
    }

    /// Every declared build setting, with its default.
    #[must_use]
    pub const fn settings(&self) -> &BTreeMap<SettingName, SettingValue> {
        &self.settings
    }
}

/// One target as it is written to JSON: a [`Node`] without its label, with its dependencies
/// as labels.
#[derive(Serialize)]
struct TargetView<'a> {
    deps: Vec<&'a Label>,
    description: Option<&'a Description>,
    env: &'a [EnvName],
    freshness: Freshness,
    inputs: &'a [SourcePath],
    network: NetworkAccess,
    outputs: &'a [OutputName],
    provenance: &'a Provenance,
    role: TargetRole,
    run: &'a Command,
}

/// A graph as it is written to JSON: every reference a label, no id anywhere.
#[derive(Serialize)]
struct GraphView<'a> {
    aliases: BTreeMap<&'a Label, &'a Label>,
    settings: &'a BTreeMap<SettingName, SettingValue>,
    targets: BTreeMap<&'a Label, TargetView<'a>>,
}

impl TargetGraph {
    /// The label of the node `id` names.
    fn label_of(&self, id: NodeId) -> Option<&Label> {
        self.node(id).map(|node| &node.label)
    }
}

/// Serializes keyed by label, so that adding a target changes only that target's lines.
///
/// Ids are positions in an arena and shift when a target is added; labels do not. There is no
/// `Deserialize`: a graph is rebuilt from the build files on every run, never read back.
impl Serialize for TargetGraph {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let aliases = self
            .aliases
            .iter()
            .filter_map(|(alias, id)| Some((alias, self.label_of(*id)?)))
            .collect();
        let targets = self
            .nodes
            .iter()
            .zip(&self.edges)
            .map(|(node, deps)| {
                let view = TargetView {
                    deps: deps.iter().filter_map(|id| self.label_of(*id)).collect(),
                    description: node.description.as_ref(),
                    env: &node.env,
                    freshness: node.freshness,
                    inputs: &node.inputs,
                    network: node.network,
                    outputs: &node.outputs,
                    provenance: &node.provenance,
                    role: node.role,
                    run: &node.run,
                };
                (&node.label, view)
            })
            .collect();
        GraphView {
            aliases,
            settings: &self.settings,
            targets,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{Node, TargetGraph};
    use crate::types::{
        Argument, Command, Description, EnvName, Freshness, Label, NetworkAccess, NodeId,
        OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetRole,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn node(raw: &str) -> Node {
        let label = label(raw);
        let directory = label.directory().clone();
        let file = if directory.is_root() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(format!("{directory}/build.lua"))
        };
        Node {
            provenance: Provenance::new(file, directory),
            role: TargetRole::Build,
            run: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            description: None,
            inputs: Vec::new(),
            outputs: vec![OutputName::parse(label.name().as_str()).unwrap()],
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
            label,
        }
    }

    fn id(index: u32) -> NodeId {
        NodeId::new(index)
    }

    /// `//:app` (0) depends on `//a-b:x` (2) then `//a:z` (1), written in that order.
    fn graph() -> TargetGraph {
        let nodes = vec![node("//:app"), node("//a:z"), node("//a-b:x")];
        let edges = vec![vec![id(2), id(1)], Vec::new(), Vec::new()];
        let aliases = BTreeMap::from([(label("//:default"), id(0))]);
        let settings = BTreeMap::from([(
            SettingName::parse("test_filter").unwrap(),
            SettingValue::parse("").unwrap(),
        )]);
        TargetGraph::new(nodes, edges, aliases, settings)
    }

    #[test]
    fn new_derives_the_reverse_edges_and_the_label_index() {
        let graph = graph();
        assert_eq!(graph.len(), 3);
        assert!(!graph.is_empty());
        assert_eq!(graph.deps(id(0)), Some(&[id(2), id(1)][..]));
        assert_eq!(graph.dependents(id(0)), Some(&[][..]));
        assert_eq!(graph.dependents(id(1)), Some(&[id(0)][..]));
        assert_eq!(graph.dependents(id(2)), Some(&[id(0)][..]));
        assert_eq!(graph.id_of(&label("//a:z")), Some(id(1)));
        assert_eq!(graph.id_of(&label("//a-b:x")), Some(id(2)));
    }

    #[test]
    fn reverse_edges_are_in_id_order_whatever_order_the_edges_were_written_in() {
        let nodes = vec![node("//:a"), node("//:b"), node("//:c")];
        let edges = vec![Vec::new(), vec![id(0)], vec![id(1), id(0)]];
        let graph = TargetGraph::new(nodes, edges, BTreeMap::new(), BTreeMap::new());
        assert_eq!(graph.dependents(id(0)), Some(&[id(1), id(2)][..]));
        assert_eq!(graph.dependents(id(1)), Some(&[id(2)][..]));
    }

    #[test]
    fn a_node_is_reached_by_its_id_and_by_its_position() {
        let graph = graph();
        assert_eq!(graph.node(id(1)).unwrap().label, label("//a:z"));
        assert_eq!(graph.nodes()[2].label, label("//a-b:x"));
        assert_eq!(graph.deps(id(0)).unwrap().len(), 2);
    }

    #[test]
    fn an_id_the_graph_did_not_issue_answers_none() {
        let graph = graph();
        assert!(graph.node(id(3)).is_none());
        assert!(graph.deps(id(3)).is_none());
        assert!(graph.dependents(id(3)).is_none());
    }

    #[test]
    fn id_of_finds_a_target_by_its_label_or_through_an_alias() {
        let graph = graph();
        assert_eq!(graph.id_of(&label("//:app")), Some(id(0)));
        assert_eq!(graph.id_of(&label("//:default")), Some(id(0)));
        assert_eq!(graph.id_of(&label("//:nope")), None);
        assert_eq!(graph.aliases().len(), 1);
        assert_eq!(graph.settings().len(), 1);
    }

    #[test]
    fn an_empty_graph_holds_nothing() {
        let graph = TargetGraph::new(Vec::new(), Vec::new(), BTreeMap::new(), BTreeMap::new());
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
        assert_eq!(
            crate::json::canonical::to_string(&graph).unwrap(),
            r#"{"aliases":{},"settings":{},"targets":{}}"#
        );
    }

    #[test]
    fn serializes_keyed_by_label_with_deps_in_written_order_and_no_id() {
        let mut graph_nodes = vec![node("//:app"), node("//a:z"), node("//a-b:x")];
        graph_nodes[0].role = TargetRole::Test;
        graph_nodes[0].description = Some(Description::parse("link $out").unwrap());
        graph_nodes[0].inputs = vec![SourcePath::parse("main.c").unwrap()];
        graph_nodes[0].env = vec![EnvName::parse("HOME").unwrap()];
        graph_nodes[0].network = NetworkAccess::Declared;
        graph_nodes[0].freshness = Freshness::Always;
        let edges = vec![vec![id(2), id(1)], Vec::new(), Vec::new()];
        let aliases = BTreeMap::from([(label("//:default"), id(0))]);
        let settings = BTreeMap::from([(
            SettingName::parse("test_filter").unwrap(),
            SettingValue::parse("").unwrap(),
        )]);
        let graph = TargetGraph::new(graph_nodes, edges, aliases, settings);
        let json = crate::json::canonical::to_string(&graph).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"aliases":{"//:default":"//:app"},"settings":{"test_filter":""},"targets":{"#,
                r#""//:app":{"deps":["//a-b:x","//a:z"],"description":"link $out","#,
                r#""env":["HOME"],"freshness":"Always","inputs":["main.c"],"#,
                r#""network":"Declared","outputs":["app"],"#,
                r#""provenance":{"directory":"","file":"build.lua"},"role":"Test","run":["cc"]},"#,
                r#""//a-b:x":{"deps":[],"description":null,"env":[],"freshness":"Cached","#,
                r#""inputs":[],"network":"Sealed","outputs":["x"],"#,
                r#""provenance":{"directory":"a-b","file":"a-b/build.lua"},"role":"Build","#,
                r#""run":["cc"]},"#,
                r#""//a:z":{"deps":[],"description":null,"env":[],"freshness":"Cached","#,
                r#""inputs":[],"network":"Sealed","outputs":["z"],"#,
                r#""provenance":{"directory":"a","file":"a/build.lua"},"role":"Build","#,
                r#""run":["cc"]}}}"#
            )
        );
    }
}
