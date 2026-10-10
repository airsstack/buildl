---
status: done
created: 2026-10-10
---

# Graph Types Implementation Plan

**Goal:** `buildl-core` holds the target graph as a type: its nodes, its edges, its accessors, its label-keyed JSON form.

**Architecture:** One new file, `types/target_graph.rs`. `Node` is plain data with public fields; `TargetGraph` keeps its six fields private, because its invariants (label order, in-bounds edges, no cycle) hold only for a graph the Resolve phase built. Its one constructor is `pub(crate)` and derives the reverse edges and the label index; nothing calls it outside the tests until plan `06`, so it carries a temporary `expect(dead_code)` that plan `06` removes. `Serialize` is written by hand through two private view structs, so the output is keyed by label and holds no `NodeId`; there is no `Deserialize`.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §3.1 (`Node`), §3.2 (`TargetGraph`), §3.3 (serialized form), §9 (the `types/target_graph.rs` row); decisions D1, D3, D7, D9, D10.

**Checkpoints:** review once, when task 1 is done.

---

## Context an implementer needs

What already exists and is used here:

| Item | Where | Used as |
|---|---|---|
| `Label`, `NodeId`, `Provenance`, `TargetRole`, `NetworkAccess`, `Freshness` | `crates/buildl-core/src/types/` | a node's fields and the graph's keys |
| `Command`, `Description`, `SourcePath`, `OutputName`, `EnvName`, `SettingName`, `SettingValue` | `crates/buildl-core/src/types/` | a node's fields and the settings table |
| `json::canonical::to_string` | `crates/buildl-core/src/json/canonical.rs` | the serializer the JSON test goes through |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- `TargetGraph::new` is `pub(crate)` and has no caller outside the tests yet. Without the
  `cfg_attr(not(test), expect(dead_code, …))` shown below, clippy fails the library build; with a
  plain `expect(dead_code)` the *test* build fails instead, because there the function is used.
- `nodes.iter().zip(0_u32..)` puts the slice first on purpose: `zip` stops as soon as the slice
  ends and never asks the range for one id too many.
- The doc comment on `TargetGraph` does not link to a `resolve` function, because none exists yet.
  Plan `06` adds the link.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/target_graph.rs   — [create] Node, TargetGraph, accessors, Serialize, unit tests
crates/buildl-core/src/types/mod.rs            — [modify] module, re-exports, doc index
crates/buildl-core/src/lib.rs                  — [modify] re-export Node and TargetGraph
```

### Task 1 — Add `Node` and `TargetGraph`

**Files:**
- Create `crates/buildl-core/src/types/target_graph.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Index the new types in the module doc of `crates/buildl-core/src/types/mod.rs`. Insert:

   ```rust
   //! - [`TargetGraph`] and [`Node`] — the runnable targets and the dependencies between them.
   ```

   immediately after:

   ```rust
   //! - [`BuildFile`], [`Evaluated`] and the staged values — what crosses the build-file port.
   ```

2. Declare the module in the same file. Insert:

   ```rust
   pub mod target_graph;
   ```

   immediately before:

   ```rust
   pub mod target_name;
   ```

3. Re-export the two types from the same file. Insert:

   ```rust
   pub use target_graph::{Node, TargetGraph};
   ```

   immediately after:

   ```rust
   pub use source_path::SourcePath;
   ```

4. Re-export them from the crate root, in `crates/buildl-core/src/lib.rs`. Replace:

   ```rust
       Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
       SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

   with:

   ```rust
       Freshness, Label, NetworkAccess, Node, NodeId, OutputName, Provenance, Rule, Setting,
       SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget, Target, TargetGraph, TargetName,
       TargetRole, Timestamp, Written,
   };
   ```

5. Write the failing tests. Create `crates/buildl-core/src/types/target_graph.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

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
   ```

6. Run them and confirm they fail to build, because neither type exists:

   ```
   $ cargo test -p buildl-core --lib types::target_graph
   error[E0432]: unresolved imports `super::Node`, `super::TargetGraph`
   error[E0432]: unresolved imports `target_graph::Node`, `target_graph::TargetGraph`
   ```

7. Replace the placeholder line at the top of `crates/buildl-core/src/types/target_graph.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
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
   /// files were evaluated in. A graph is built by the phase that resolves declarations, which
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
       #[cfg_attr(
           not(test),
           expect(
               dead_code,
               reason = "called by the phase that builds the graph, which is not written yet"
           )
       )]
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
   ```

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::target_graph
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 137 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

10. Commit:

   ```
   $ git add crates/buildl-core/src/types/target_graph.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add Node and TargetGraph"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `cargo test -p buildl-core --lib types::target_graph` runs seven tests, all passing.
- The JSON test pins the exact bytes of a three-target graph whose labels `//a:z` and `//a-b:x`
  sort one way as labels and the other way as text, and the output holds no number.

## Review findings

One reviewer pass over the task's three files, 2026-10-10. Verdict: spec compliant, blocking set empty. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0 (`buildl-core` unit 144 passed, flows 13); `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). It compared `types/target_graph.rs` with this plan's steps 5 and 7 byte for byte: 14655 bytes each.

- nit — the module doc lists "the four types able to appear as a map key"; `SettingName` is now a fifth, keying the serialized `settings` map. The guarantee holds, the list is stale. Not fixed: the file is outside this plan — `crates/buildl-core/src/json/canonical.rs:27`
- nit — the `Serialize` doc says adding a target "changes only that target's lines"; the canonical form is one line, so "entry" is the accurate word. Not fixed: the text is this plan's and spec §3.3's — `crates/buildl-core/src/types/target_graph.rs:202`
- nit — a breach of `new`'s documented preconditions is absorbed silently (an out-of-range dependency is skipped in `reverse` and dropped from the JSON; `zip` truncates on a length mismatch). By design per spec §3.2; no test pins those paths. Left for the review of the plan that adds the caller — `crates/buildl-core/src/types/target_graph.rs:87`

## Probe results

No separate probe was run. Every fact the task asserts about existing code is put under test by the task's own cycle, and both runs matched the plan:

- red — `cargo test -p buildl-core --lib types::target_graph` — `error[E0432]: unresolved imports super::Node, super::TargetGraph` and `error[E0432]: unresolved imports target_graph::Node, target_graph::TargetGraph`
- green — `cargo test -p buildl-core --lib types::target_graph` — `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 137 filtered out`

## Deviations

- 2026-10-10 — step 10 (the commit) was not run during execution. The commit is the author's to make; the task's three files were left in the working tree.
