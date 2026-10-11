---
status: done
created: 2026-10-11
depends-on: [04, 08]
---

# Plan Verdicts Implementation Plan

**Goal:** Every target of a graph gets its verdict, each decided after the targets it depends on.

**Architecture:** The last three logic files of `plan/`. `plan/order.rs` computes a topological order with a queue and one counter per node, never by recursion: ids are in label order, so walking them in order is not bottom-up. `plan/difference.rs` compares a target's components with the key its last build recorded and lists what differs, in a fixed order; a dependency whose products are not known yet is reported only when it was added. `plan/verdict.rs` decides one target — always dirty, never built, clean, dirty with its differences, or conditional — and walks the order to decide all of them, taking a dependency's products from its cache row only when that dependency is clean. A key is finished when every dependency is clean; otherwise it stays unfinished, awaiting the others. The cache is reached through a closure from a label to a row, so nothing here names a port.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D1, D7, D8, D19), §5.5 (order), §5.6 (verdicts, differences), §9 (the rows for `plan/order.rs`, `plan/difference.rs` and `plan/verdict.rs`).

**Checkpoints:** review once, when all three tasks are done.

---

## Context an implementer needs

What plans `02`, `04`, `07` and `08` leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `TargetGraph` (`len`, `node`, `deps` in written order, `dependents` in id order), `NodeId` (`new`, `index`) | `crates/buildl-core/src/types/` | what the order and the walk read |
| `OwnComponents`, `KeyComponents`, `DepProducts`, `Produced`, `ActionKey::new` (fallible) | `crates/buildl-core/src/types/action_key.rs` | what is compared, and how a key is finished |
| `CacheRow` (`key`, `produced`) | `crates/buildl-core/src/types/cache_row.rs` | what the last build recorded |
| `Difference`, `DirtyReason`, `Unfinished`, `PlannedKey`, `Verdict` | `crates/buildl-core/src/types/plan.rs` | what a verdict is made of |
| `Freshness` | `crates/buildl-core/src/types/declaration.rs` | whether a target always runs |
| `command`, `settings`, `label`, `node`, `graph`, `path`, `env_name`, `program` | `crates/buildl-core/src/plan/fixtures.rs` | the builders the tests use; task 2 adds two more |
| `plan/mod.rs` declaring `authority`, `components`, `inputs`, `settings` and `tools` | `crates/buildl-core/src/plan/mod.rs` | the file each task adds one `mod` line to |

Gate facts that bite here. A bullet that names a lint reports one seen as a failing run while this
code was prototyped:

- Nothing outside a file's own tests calls its functions until plan `10` writes the function
  that makes a plan, so the library build reports them as dead code and the gate fails. Each
  `mod` line in `plan/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It
  must be `cfg_attr(not(test), …)`: in the test build the functions are used, and a plain
  `expect` would fail there as unfulfilled. Plan `10` removes the attributes.
- `plan/fixtures.rs` is compiled for tests only, and a fixture no test calls is dead code in the
  test build. Each task therefore adds only the builders its own tests use.
- The 100 000-node test is the reason the order is computed with a queue; it runs in the gate.
- clippy's `type_complexity` rejects an array of `(fn(&mut OwnComponents), Difference)` pairs
  written inline, so `plan/difference.rs`'s tests name the function type `Change`; and
  `unnecessary_wraps` rejects a helper that always returns `Some`, so the helper `tool` returns
  the value and its callers wrap it.
- `ActionKey::new` returns a `Result`, so `decide` and `all` do too, although no value this crate
  builds makes it fail.
- All three files import only `crate::error`, `crate::types` and siblings under `crate::plan`, so
  `crates/expected-module-edges.txt` does not change.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/plan/order.rs        — [create] topological, unit tests
crates/buildl-core/src/plan/difference.rs   — [create] Known, between, unit tests
crates/buildl-core/src/plan/verdict.rs      — [create] Dependency, decide, all, unit tests
crates/buildl-core/src/plan/fixtures.rs     — [modify] two more builders
crates/buildl-core/src/plan/mod.rs          — [modify] declare the three modules
```

### Task 1 — Order a graph's targets after their dependencies

**Files:**
- Create `crates/buildl-core/src/plan/order.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod authority;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod components;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod inputs;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod order;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod settings;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod tools;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/plan/order.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use super::topological;
       use crate::plan::fixtures::{graph, node};
       use crate::types::NodeId;

       fn ids(raw: &[u32]) -> Vec<NodeId> {
           raw.iter().copied().map(NodeId::new).collect()
       }

       #[test]
       fn an_empty_graph_has_an_empty_order() {
           assert!(topological(&graph(Vec::new(), &[])).is_empty());
       }

       #[test]
       fn targets_that_depend_on_nothing_come_in_id_order() {
           let graph = graph(
               vec![
                   node("//:a", |_| {}),
                   node("//:b", |_| {}),
                   node("//:c", |_| {}),
               ],
               &[&[], &[], &[]],
           );
           assert_eq!(topological(&graph), ids(&[0, 1, 2]));
       }

       #[test]
       fn a_target_comes_after_a_dependency_with_a_higher_id() {
           // `//:a` (0) depends on `//:z` (1): ids are in label order, not dependency order.
           let graph = graph(
               vec![node("//:a", |_| {}), node("//:z", |_| {})],
               &[&[1], &[]],
           );
           assert_eq!(topological(&graph), ids(&[1, 0]));
       }

       #[test]
       fn every_node_of_a_diamond_comes_after_all_it_depends_on() {
           // 0 -> 1 -> 3 and 0 -> 2 -> 3: `//:d` is built first and `//:a` last.
           let graph = graph(
               vec![
                   node("//:a", |_| {}),
                   node("//:b", |_| {}),
                   node("//:c", |_| {}),
                   node("//:d", |_| {}),
               ],
               &[&[1, 2], &[3], &[3], &[]],
           );
           assert_eq!(topological(&graph), ids(&[3, 1, 2, 0]));
       }

       #[test]
       fn a_long_chain_is_ordered_without_recursion() {
           let length = 100_000_u32;
           let nodes = (0..length)
               .map(|index| node(&format!("//:n{index:06}"), |_| {}))
               .collect();
           // Each node depends on the next, so the last node comes first.
           let lists: Vec<Vec<u32>> = (0..length)
               .map(|index| {
                   if index + 1 < length {
                       vec![index + 1]
                   } else {
                       Vec::new()
                   }
               })
               .collect();
           let edges: Vec<&[u32]> = lists.iter().map(Vec::as_slice).collect();
           let order = topological(&graph(nodes, &edges));
           assert_eq!(order.len(), 100_000);
           assert_eq!(order.first(), Some(&NodeId::new(length - 1)));
           assert_eq!(order.last(), Some(&NodeId::new(0)));
           assert!(order.windows(2).all(|pair| pair[0] > pair[1]));
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::order
   error[E0432]: unresolved import `super::topological`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/plan/order.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The order a graph's targets are visited in: every target after all it depends on.
   //!
   //! Its own file because it is a graph algorithm that knows nothing about keys: a verdict needs
   //! the verdicts of a target's dependencies first, and a target's id says nothing about where it
   //! sits among them.
   //!
   //! Responsibilities: [`topological`].
   //!
   //! Non-responsibilities: detecting a cycle. A graph has none by the time it exists.

   use std::collections::VecDeque;

   use crate::types::{NodeId, TargetGraph};

   /// Every node of `graph`, each after all the nodes it depends on.
   ///
   /// Computed with a queue and one counter per node, never by recursion, so a long dependency
   /// chain cannot exhaust the call stack. Among nodes that are ready at the same time the lower id
   /// comes first, so the same graph always yields the same order.
   pub(crate) fn topological(graph: &TargetGraph) -> Vec<NodeId> {
       let ids = || (0_u32..).take(graph.len()).map(NodeId::new);
       let mut waiting: Vec<usize> = ids()
           .map(|id| graph.deps(id).map_or(0, <[NodeId]>::len))
           .collect();
       let mut ready: VecDeque<NodeId> = ids()
           .filter(|id| waiting.get(id.index()) == Some(&0))
           .collect();

       let mut order = Vec::with_capacity(graph.len());
       while let Some(id) = ready.pop_front() {
           order.push(id);
           for dependent in graph.dependents(id).unwrap_or_default() {
               if let Some(count) = waiting.get_mut(dependent.index()) {
                   *count = count.saturating_sub(1);
                   if *count == 0 {
                       ready.push_back(*dependent);
                   }
               }
           }
       }
       order
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::order
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 293 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/plan/order.rs crates/buildl-core/src/plan/mod.rs
   $ git commit -m "feat(buildl-core): order a graph's targets after their dependencies"
   ```

### Task 2 — List how a key differs from the recorded one

**Files:**
- Create `crates/buildl-core/src/plan/difference.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`
- Modify `crates/buildl-core/src/plan/fixtures.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod authority;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod components;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod difference;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod inputs;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod order;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod settings;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod tools;

   #[cfg(test)]
   mod fixtures;
   ```

2. Add builders for own components and for a produced output. `crates/buildl-core/src/plan/fixtures.rs` becomes:

   ```rust
   //! Values built by hand, shared by this phase's unit tests.
   //!
   //! Its own file because several test modules need the same builders. Compiled for tests only,
   //! so it carries no test module of its own.

   #![expect(
       clippy::unwrap_used,
       reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
   )]

   use std::collections::BTreeMap;
   use std::path::PathBuf;

   use crate::types::{
       Argument, Command, Digest, Directory, EnvName, Freshness, Label, NetworkAccess, Node, NodeId,
       OutputName, OwnComponents, Platform, Produced, Program, Provenance, SettingName, SettingValue,
       SourcePath, TargetGraph, TargetRole,
   };

   /// The own components of a target running `cc` on `linux/x86_64` and producing `output`, with
   /// nothing else, after `change` is applied.
   pub(crate) fn components(output: &str, change: impl FnOnce(&mut OwnComponents)) -> OwnComponents {
       let mut own = OwnComponents {
           command: command(&["cc"]),
           inputs: Vec::new(),
           outputs: vec![OutputName::parse(output).unwrap()],
           env: BTreeMap::new(),
           tool: None,
           settings: BTreeMap::new(),
           network: NetworkAccess::Sealed,
           platform: Platform::parse("linux/x86_64").unwrap(),
       };
       change(&mut own);
       own
   }

   /// An output named `name` whose content was `content`.
   pub(crate) fn produced(name: &str, content: &str) -> Produced {
       Produced {
           name: OutputName::parse(name).unwrap(),
           digest: Digest::of(content.as_bytes()),
       }
   }

   /// A directory from known-valid text.
   pub(crate) fn dir(raw: &str) -> Directory {
       Directory::parse(raw).unwrap()
   }

   /// A workspace file path from known-valid text.
   pub(crate) fn path(raw: &str) -> SourcePath {
       SourcePath::parse(raw).unwrap()
   }

   /// An environment variable name from known-valid text.
   pub(crate) fn env_name(raw: &str) -> EnvName {
       EnvName::parse(raw).unwrap()
   }

   /// A program name from known-valid text.
   pub(crate) fn program(raw: &str) -> Program {
       Program::parse(raw).unwrap()
   }

   /// A label from known-valid text.
   pub(crate) fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   /// A build target running `cc`, with one output named after it and nothing else, declared by
   /// the build file of its label's directory, after `change` is applied.
   pub(crate) fn node(raw: &str, change: impl FnOnce(&mut Node)) -> Node {
       let label = label(raw);
       let directory = label.directory().clone();
       let file = if directory.is_root() {
           PathBuf::from("build.lua")
       } else {
           PathBuf::from(format!("{directory}/build.lua"))
       };
       let mut node = Node {
           provenance: Provenance::new(file, directory),
           role: TargetRole::Build,
           run: command(&["cc"]),
           description: None,
           inputs: Vec::new(),
           outputs: vec![OutputName::parse(label.name().as_str()).unwrap()],
           env: Vec::new(),
           network: NetworkAccess::Sealed,
           freshness: Freshness::Cached,
           label,
       };
       change(&mut node);
       node
   }

   /// A graph of `nodes`, which must be in label order. `edges` holds one list per node, in the
   /// same order: the positions of the nodes it depends on.
   pub(crate) fn graph(nodes: Vec<Node>, edges: &[&[u32]]) -> TargetGraph {
       let edges = edges
           .iter()
           .map(|deps| deps.iter().copied().map(NodeId::new).collect())
           .collect();
       TargetGraph::new(nodes, edges, BTreeMap::new(), BTreeMap::new())
   }

   /// A command from known-valid arguments.
   pub(crate) fn command(arguments: &[&str]) -> Command {
       Command::new(
           arguments
               .iter()
               .map(|argument| Argument::parse(*argument).unwrap())
               .collect(),
       )
       .unwrap()
   }

   /// Build settings from known-valid `(name, value)` pairs.
   pub(crate) fn settings(pairs: &[(&str, &str)]) -> BTreeMap<SettingName, SettingValue> {
       pairs
           .iter()
           .map(|(name, value)| {
               (
                   SettingName::parse(*name).unwrap(),
                   SettingValue::parse(*value).unwrap(),
               )
           })
           .collect()
   }
   ```

3. Write the failing tests. Create `crates/buildl-core/src/plan/difference.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{Known, between, changed, reordered};
       use crate::plan::fixtures::{
           command, components, env_name, label, path, produced, program, settings,
       };
       use crate::types::{
           DepProducts, Difference, Digest, EnvValue, InputDigest, KeyComponents, Label,
           NetworkAccess, OutputName, OwnComponents, Platform, Produced, SettingName, ToolIdentity,
       };
       use std::collections::BTreeMap;

       fn input(raw: &str, content: &str) -> InputDigest {
           InputDigest {
               path: path(raw),
               digest: Digest::of(content.as_bytes()),
           }
       }

       fn tool(fingerprint: &str) -> ToolIdentity {
           ToolIdentity {
               program: program("cc"),
               fingerprint: Digest::of(fingerprint.as_bytes()),
           }
       }

       /// One edit to a target's own components.
       type Change = fn(&mut OwnComponents);

       /// The components of a target with something in every field.
       fn full() -> OwnComponents {
           components("app", |own| {
               own.inputs = vec![input("src/z.c", "z"), input("src/a.c", "a")];
               own.env = BTreeMap::from([
                   (env_name("HOME"), None),
                   (env_name("PATH"), Some(EnvValue::parse("/bin").unwrap())),
               ]);
               own.tool = Some(tool("cc 14.2"));
               own.settings = settings(&[("mode", "debug"), ("test_filter", "T")]);
           })
       }

       /// The recorded key: [`full`], depending on `//lib:text` then `//lib:util`.
       fn recorded() -> KeyComponents {
           KeyComponents {
               own: full(),
               deps: vec![
                   DepProducts {
                       label: label("//lib:text"),
                       outputs: vec![produced("text", "text v1")],
                   },
                   DepProducts {
                       label: label("//lib:util"),
                       outputs: vec![produced("util", "util v1")],
                   },
               ],
           }
       }

       /// The dependencies of [`recorded`], all with the products it recorded.
       fn same_deps(previous: &KeyComponents) -> Vec<(Label, Option<Vec<Produced>>)> {
           previous
               .deps
               .iter()
               .map(|dep| (dep.label.clone(), Some(dep.outputs.clone())))
               .collect()
       }

       fn known(deps: &[(Label, Option<Vec<Produced>>)]) -> Vec<Known<'_>> {
           deps.iter()
               .map(|(label, products)| Known {
                   label,
                   products: products.as_deref(),
               })
               .collect()
       }

       /// The differences of `own` with the recorded dependencies unchanged.
       fn own_differences(own: &OwnComponents) -> Vec<Difference> {
           let previous = recorded();
           let deps = same_deps(&previous);
           between(own, &known(&deps), &previous)
       }

       /// The differences of the recorded own components with `deps`.
       fn dep_differences(deps: &[(Label, Option<Vec<Produced>>)]) -> Vec<Difference> {
           between(&full(), &known(deps), &recorded())
       }

       #[test]
       fn equal_keys_have_no_difference() {
           assert!(own_differences(&full()).is_empty());
       }

       #[test]
       fn each_own_component_is_reported_alone_when_it_alone_changes() {
           let cases: [(Change, Difference); 12] = [
               (
                   |own| own.command = command(&["cc", "-O2"]),
                   Difference::Command,
               ),
               (
                   |own| own.inputs = vec![input("src/z.c", "z2"), input("src/a.c", "a")],
                   Difference::Input(path("src/z.c")),
               ),
               (
                   |own| own.inputs.push(input("src/new.c", "n")),
                   Difference::Input(path("src/new.c")),
               ),
               (
                   |own| own.inputs = vec![input("src/z.c", "z")],
                   Difference::Input(path("src/a.c")),
               ),
               (|own| own.inputs.reverse(), Difference::InputOrder),
               (
                   |own| own.outputs = vec![OutputName::parse("app2").unwrap()],
                   Difference::Outputs,
               ),
               (
                   |own| {
                       own.env
                           .insert(env_name("HOME"), Some(EnvValue::parse("").unwrap()));
                   },
                   Difference::Env(env_name("HOME")),
               ),
               (
                   |own| {
                       own.env.remove(&env_name("PATH"));
                   },
                   Difference::Env(env_name("PATH")),
               ),
               (|own| own.tool = Some(tool("cc 15.1")), Difference::Tool),
               (
                   |own| own.settings = settings(&[("mode", "release"), ("test_filter", "T")]),
                   Difference::Setting(SettingName::parse("mode").unwrap()),
               ),
               (
                   |own| own.network = NetworkAccess::Declared,
                   Difference::Network,
               ),
               (
                   |own| own.platform = Platform::parse("macos/aarch64").unwrap(),
                   Difference::Platform,
               ),
           ];
           for (change, difference) in cases {
               let mut own = full();
               change(&mut own);
               assert_eq!(own_differences(&own), [difference]);
           }
       }

       #[test]
       fn a_dependency_is_reported_when_added_removed_or_producing_something_else() {
           let previous = recorded();
           let mut produced_other = same_deps(&previous);
           produced_other[1].1 = Some(vec![produced("util", "util v2")]);
           assert_eq!(
               dep_differences(&produced_other),
               [Difference::Dep(label("//lib:util"))]
           );

           let mut added = same_deps(&previous);
           added.push((label("//lib:new"), Some(vec![produced("new", "new v1")])));
           assert_eq!(
               dep_differences(&added),
               [Difference::Dep(label("//lib:new"))]
           );

           let removed = &same_deps(&previous)[..1];
           assert_eq!(
               dep_differences(removed),
               [Difference::Dep(label("//lib:util"))]
           );
       }

       #[test]
       fn the_same_dependencies_in_another_order_are_reported_as_an_order_change() {
           let mut deps = same_deps(&recorded());
           deps.reverse();
           assert_eq!(dep_differences(&deps), [Difference::DepOrder]);
       }

       #[test]
       fn a_dependency_with_unknown_products_is_reported_only_when_it_was_added() {
           let previous = recorded();
           let mut unknown = same_deps(&previous);
           unknown[0].1 = None;
           assert!(dep_differences(&unknown).is_empty());

           unknown.push((label("//lib:new"), None));
           assert_eq!(
               dep_differences(&unknown),
               [Difference::Dep(label("//lib:new"))]
           );
       }

       #[test]
       fn differences_come_in_one_fixed_order() {
           let own = components("app2", |own| {
               own.command = command(&["cc", "-O2"]);
               own.inputs = vec![
                   input("src/b.c", "b"),
                   input("src/a.c", "a2"),
                   input("src/z.c", "z"),
               ];
               own.env = BTreeMap::from([(env_name("PATH"), None), (env_name("CC"), None)]);
               own.tool = Some(tool("cc 15.1"));
               own.settings = settings(&[("arch", "arm"), ("test_filter", "U")]);
               own.network = NetworkAccess::Declared;
               own.platform = Platform::parse("macos/aarch64").unwrap();
           });
           let deps = [
               (label("//lib:util"), Some(vec![produced("util", "util v2")])),
               (label("//lib:text"), None),
               (label("//lib:added"), None),
           ];
           assert_eq!(
               between(&own, &known(&deps), &recorded()),
               [
                   Difference::Command,
                   Difference::Input(path("src/a.c")),
                   Difference::Input(path("src/b.c")),
                   Difference::Outputs,
                   Difference::Env(env_name("CC")),
                   Difference::Env(env_name("HOME")),
                   Difference::Env(env_name("PATH")),
                   Difference::Tool,
                   Difference::Setting(SettingName::parse("arch").unwrap()),
                   Difference::Setting(SettingName::parse("mode").unwrap()),
                   Difference::Setting(SettingName::parse("test_filter").unwrap()),
                   Difference::Network,
                   Difference::Platform,
                   Difference::Dep(label("//lib:added")),
                   Difference::Dep(label("//lib:util")),
               ]
           );
       }

       #[test]
       fn an_order_change_is_reported_beside_a_changed_member() {
           let mut own = full();
           own.inputs = vec![input("src/a.c", "a2"), input("src/z.c", "z")];
           assert_eq!(
               own_differences(&own),
               [Difference::Input(path("src/a.c")), Difference::InputOrder]
           );

           let mut deps = same_deps(&recorded());
           deps.reverse();
           deps[0].1 = Some(vec![produced("util", "util v2")]);
           assert_eq!(
               dep_differences(&deps),
               [Difference::Dep(label("//lib:util")), Difference::DepOrder]
           );
       }

       #[test]
       fn changed_lists_each_differing_key_once_in_key_order() {
           let now = BTreeMap::from([("a", 1), ("b", 2), ("d", 4)]);
           let then = BTreeMap::from([("b", 3), ("c", 3), ("d", 4)]);
           assert_eq!(changed(&now, &then), ["a", "b", "c"]);
           assert!(changed(&now, &now).is_empty());
       }

       #[test]
       fn reordered_means_the_same_items_in_another_order() {
           assert!(reordered(&[1, 2, 3], &[3, 1, 2]));
           assert!(!reordered(&[1, 2, 3], &[1, 2, 3]));
           assert!(!reordered(&[1, 2, 3], &[1, 2]));
           assert!(!reordered(&[1, 2, 3], &[1, 2, 4]));
           assert!(!reordered::<u8>(&[], &[]));
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::difference
   error[E0432]: unresolved imports `super::Known`, `super::between`, `super::changed`, `super::reordered`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/plan/difference.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! How a target's key differs from the key its last build recorded.
   //!
   //! Its own file because this comparison is what explains a rebuild: a digest says only that two
   //! keys differ, and the components kept beside it say where.
   //!
   //! Responsibilities: [`Known`] and [`between`].
   //!
   //! Non-responsibilities: deciding what a difference means for a target, which is its verdict.

   use std::collections::BTreeMap;

   use crate::types::{Difference, Digest, KeyComponents, Label, OwnComponents, Produced, SourcePath};

   /// One dependency as a comparison sees it: its label, and its products when they are known.
   ///
   /// A borrowed view, made for one comparison. The value a verdict is decided from also carries
   /// the dependency's id and owns its products; a comparison needs neither.
   #[derive(Debug, Clone, Copy)]
   pub(crate) struct Known<'a> {
       /// The dependency.
       pub(crate) label: &'a Label,
       /// What it produced; `None` when that is not known before the build.
       pub(crate) products: Option<&'a [Produced]>,
   }

   /// Every way a key made of `own` and `deps` is known to differ from `previous`.
   ///
   /// The differences come in a fixed order: the command; each input that was added, removed or
   /// changed, in path order; the inputs' order; the outputs; each environment variable, in name
   /// order; the tool; each setting, in name order; the network; the platform; each dependency
   /// that was added, removed or produced something else, in label order; the dependencies' order.
   ///
   /// A dependency whose products are not known cannot be said to have produced something else, so
   /// it is reported only when it was added. When every dependency's products are known the list
   /// is empty exactly when the two keys are equal.
   pub(crate) fn between(
       own: &OwnComponents,
       deps: &[Known<'_>],
       previous: &KeyComponents,
   ) -> Vec<Difference> {
       let old = &previous.own;
       let mut found = Vec::new();

       if own.command != old.command {
           found.push(Difference::Command);
       }
       found.extend(
           changed(&input_digests(own), &input_digests(old))
               .into_iter()
               .map(|path| Difference::Input(path.clone())),
       );
       if reordered(&input_paths(own), &input_paths(old)) {
           found.push(Difference::InputOrder);
       }
       if own.outputs != old.outputs {
           found.push(Difference::Outputs);
       }
       found.extend(changed(&own.env, &old.env).into_iter().map(Difference::Env));
       if own.tool != old.tool {
           found.push(Difference::Tool);
       }
       found.extend(
           changed(&own.settings, &old.settings)
               .into_iter()
               .map(Difference::Setting),
       );
       if own.network != old.network {
           found.push(Difference::Network);
       }
       if own.platform != old.platform {
           found.push(Difference::Platform);
       }

       let then: BTreeMap<&Label, &[Produced]> = previous
           .deps
           .iter()
           .map(|dep| (&dep.label, dep.outputs.as_slice()))
           .collect();
       // A dependency whose products are not known is given the recorded ones, so that it compares
       // equal when it was there before and is missing from `now` when it was not.
       let now: BTreeMap<&Label, &[Produced]> = deps
           .iter()
           .filter_map(|dep| {
               let products = dep.products.or_else(|| then.get(dep.label).copied())?;
               Some((dep.label, products))
           })
           .collect();
       let added_unknown = deps
           .iter()
           .filter(|dep| dep.products.is_none() && !then.contains_key(dep.label))
           .map(|dep| dep.label);
       let mut differing: Vec<&Label> = changed(&now, &then)
           .into_iter()
           .chain(added_unknown)
           .collect();
       differing.sort();
       found.extend(
           differing
               .into_iter()
               .map(|label| Difference::Dep(label.clone())),
       );
       let labels_now: Vec<&Label> = deps.iter().map(|dep| dep.label).collect();
       let labels_then: Vec<&Label> = previous.deps.iter().map(|dep| &dep.label).collect();
       if reordered(&labels_now, &labels_then) {
           found.push(Difference::DepOrder);
       }

       found
   }

   /// The inputs of `components`, each path with its digest.
   fn input_digests(components: &OwnComponents) -> BTreeMap<&SourcePath, Digest> {
       components
           .inputs
           .iter()
           .map(|input| (&input.path, input.digest))
           .collect()
   }

   /// The paths of the inputs of `components`, in written order.
   fn input_paths(components: &OwnComponents) -> Vec<&SourcePath> {
       components.inputs.iter().map(|input| &input.path).collect()
   }

   /// The keys held by only one of two maps, or by both with different values, in key order.
   fn changed<K: Ord + Clone, V: PartialEq>(now: &BTreeMap<K, V>, then: &BTreeMap<K, V>) -> Vec<K> {
       let mut keys: Vec<K> = now
           .keys()
           .chain(then.keys())
           .filter(|key| now.get(*key) != then.get(*key))
           .cloned()
           .collect();
       keys.sort();
       keys.dedup();
       keys
   }

   /// Whether two lists hold the same items in a different order.
   fn reordered<T: Ord + Clone>(now: &[T], then: &[T]) -> bool {
       let sorted = |items: &[T]| {
           let mut items = items.to_vec();
           items.sort();
           items
       };
       now != then && sorted(now) == sorted(then)
   }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::difference
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 298 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

8. Commit:

   ```
   $ git add crates/buildl-core/src/plan/difference.rs crates/buildl-core/src/plan/mod.rs crates/buildl-core/src/plan/fixtures.rs
   $ git commit -m "feat(buildl-core): list how a key differs from the recorded one"
   ```

### Task 3 — Decide a verdict for every target

**Files:**
- Create `crates/buildl-core/src/plan/verdict.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod authority;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod components;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod difference;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod inputs;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod order;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod settings;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod tools;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod verdict;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/plan/verdict.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use super::{Dependency, all, decide};
       use crate::plan::fixtures::{command, components, graph, label, node, produced};
       use crate::plan::order::topological;
       use crate::types::{
           ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, Freshness,
           KeyComponents, Label, NodeId, OwnComponents, PlannedKey, Produced, Unfinished, Verdict,
       };

       /// The key of `own` over `deps`, each a label and what it produced.
       fn key(own: OwnComponents, deps: &[(&str, Vec<Produced>)]) -> ActionKey {
           let deps = deps
               .iter()
               .map(|(raw, outputs)| DepProducts {
                   label: label(raw),
                   outputs: outputs.clone(),
               })
               .collect();
           ActionKey::new(KeyComponents { own, deps }).unwrap()
       }

       /// The row of a build under `key` whose one output had the content `content`.
       fn row(key: ActionKey, content: &str) -> CacheRow {
           CacheRow::new(key, vec![Digest::of(content.as_bytes())]).unwrap()
       }

       fn clean<'a>(id: u32, label: &'a Label, name: &str, content: &str) -> Dependency<'a> {
           Dependency {
               id: NodeId::new(id),
               label,
               products: Some(vec![produced(name, content)]),
           }
       }

       const fn not_clean(id: u32, label: &Label) -> Dependency<'_> {
           Dependency {
               id: NodeId::new(id),
               label,
               products: None,
           }
       }

       fn app() -> OwnComponents {
           components("app", |_| {})
       }

       #[test]
       fn a_target_that_always_runs_is_dirty_whatever_was_recorded() {
           let recorded = row(key(app(), &[]), "app v1");
           let verdict = decide(Freshness::Always, app(), &[], Some(&recorded)).unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::Always,
                   key: PlannedKey::Finished(key(app(), &[])),
               }
           );
       }

       #[test]
       fn a_target_with_no_row_was_never_built() {
           let verdict = decide(Freshness::Cached, app(), &[], None).unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::NeverBuilt,
                   key: PlannedKey::Finished(key(app(), &[])),
               }
           );
       }

       #[test]
       fn a_never_built_target_awaits_its_dependencies_that_are_not_clean() {
           let text = label("//lib:text");
           let util = label("//lib:util");
           let deps = [not_clean(4, &text), clean(2, &util, "util", "util v1")];
           let verdict = decide(Freshness::Cached, app(), &deps, None).unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::NeverBuilt,
                   key: PlannedKey::Unfinished(Unfinished {
                       own: app(),
                       awaiting: vec![NodeId::new(4)],
                   }),
               }
           );
       }

       #[test]
       fn a_target_whose_key_equals_the_recorded_one_is_clean() {
           let text = label("//lib:text");
           let deps = [clean(1, &text, "text", "text v1")];
           let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
           let verdict = decide(
               Freshness::Cached,
               app(),
               &deps,
               Some(&row(recorded.clone(), "app v1")),
           )
           .unwrap();
           assert_eq!(verdict, Verdict::Clean { key: recorded });
       }

       #[test]
       fn a_settled_target_whose_key_differs_is_dirty_with_every_difference() {
           let recorded = row(key(app(), &[]), "app v1");
           let changed = components("app", |own| own.command = command(&["cc", "-O2"]));
           let verdict = decide(Freshness::Cached, changed.clone(), &[], Some(&recorded)).unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::Changed(vec![Difference::Command]),
                   key: PlannedKey::Finished(key(changed, &[])),
               }
           );
       }

       #[test]
       fn a_clean_dependency_that_produced_something_else_makes_its_dependent_dirty() {
           let text = label("//lib:text");
           let deps = [clean(1, &text, "text", "text v2")];
           let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
           let verdict = decide(
               Freshness::Cached,
               app(),
               &deps,
               Some(&row(recorded, "app v1")),
           )
           .unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::Changed(vec![Difference::Dep(label("//lib:text"))]),
                   key: PlannedKey::Finished(key(
                       app(),
                       &[("//lib:text", vec![produced("text", "text v2")])]
                   )),
               }
           );
       }

       #[test]
       fn an_unsettled_target_with_a_known_difference_is_dirty_and_its_key_unfinished() {
           let text = label("//lib:text");
           let deps = [not_clean(1, &text)];
           let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
           let changed = components("app", |own| own.command = command(&["cc", "-O2"]));
           let verdict = decide(
               Freshness::Cached,
               changed.clone(),
               &deps,
               Some(&row(recorded, "app v1")),
           )
           .unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::Changed(vec![Difference::Command]),
                   key: PlannedKey::Unfinished(Unfinished {
                       own: changed,
                       awaiting: vec![NodeId::new(1)],
                   }),
               }
           );
       }

       #[test]
       fn an_unsettled_target_with_nothing_known_to_differ_is_conditional() {
           let text = label("//lib:text");
           let util = label("//lib:util");
           let other = label("//lib:other");
           let deps = [
               not_clean(7, &util),
               clean(1, &text, "text", "text v1"),
               not_clean(3, &other),
           ];
           let recorded = key(
               app(),
               &[
                   ("//lib:util", vec![produced("util", "util v1")]),
                   ("//lib:text", vec![produced("text", "text v1")]),
                   ("//lib:other", vec![produced("other", "other v1")]),
               ],
           );
           let verdict = decide(
               Freshness::Cached,
               app(),
               &deps,
               Some(&row(recorded, "app v1")),
           )
           .unwrap();
           assert_eq!(
               verdict,
               Verdict::Conditional(Unfinished {
                   own: app(),
                   awaiting: vec![NodeId::new(7), NodeId::new(3)],
               }),
               "awaited dependencies keep the order they were written in"
           );
       }

       #[test]
       fn an_always_target_with_an_unclean_dependency_keeps_an_unfinished_key() {
           let text = label("//lib:text");
           let deps = [not_clean(1, &text)];
           let verdict = decide(Freshness::Always, app(), &deps, None).unwrap();
           assert_eq!(
               verdict,
               Verdict::Dirty {
                   reason: DirtyReason::Always,
                   key: PlannedKey::Unfinished(Unfinished {
                       own: app(),
                       awaiting: vec![NodeId::new(1)],
                   }),
               }
           );
       }

       /// `//:app` (0) depends on `//:lib` (1), which depends on `//:z` (3); `//:solo` (2) stands
       /// alone. Ids are in label order, so the chain runs against them.
       fn chain() -> crate::types::TargetGraph {
           graph(
               vec![
                   node("//:app", |_| {}),
                   node("//:lib", |_| {}),
                   node("//:solo", |_| {}),
                   node("//:z", |_| {}),
               ],
               &[&[1], &[3], &[], &[]],
           )
       }

       fn own_of_chain() -> Vec<OwnComponents> {
           ["app", "lib", "solo", "z"]
               .map(|output| components(output, |_| {}))
               .to_vec()
       }

       /// The rows a build of [`chain`] records, with `z`'s output content as given.
       fn rows_of_chain(z_content: &str) -> BTreeMap<Label, CacheRow> {
           let z = row(key(components("z", |_| {}), &[]), z_content);
           let lib = row(
               key(components("lib", |_| {}), &[("//:z", z.produced())]),
               "lib v1",
           );
           let app = row(
               key(components("app", |_| {}), &[("//:lib", lib.produced())]),
               "app v1",
           );
           let solo = row(key(components("solo", |_| {}), &[]), "solo v1");
           BTreeMap::from([
               (label("//:app"), app),
               (label("//:lib"), lib),
               (label("//:solo"), solo),
               (label("//:z"), z),
           ])
       }

       fn verdicts_of_chain(
           own: Vec<OwnComponents>,
           rows: &BTreeMap<Label, CacheRow>,
       ) -> Vec<Verdict> {
           let graph = chain();
           let order = topological(&graph);
           all(&graph, own, &order, |label| rows.get(label)).unwrap()
       }

       #[test]
       fn with_no_rows_every_target_was_never_built_and_awaits_its_dependency() {
           let verdicts = verdicts_of_chain(own_of_chain(), &BTreeMap::new());
           let never = |output: &str, awaiting: &[u32]| Verdict::Dirty {
               reason: DirtyReason::NeverBuilt,
               key: if awaiting.is_empty() {
                   PlannedKey::Finished(key(components(output, |_| {}), &[]))
               } else {
                   PlannedKey::Unfinished(Unfinished {
                       own: components(output, |_| {}),
                       awaiting: awaiting.iter().copied().map(NodeId::new).collect(),
                   })
               },
           };
           assert_eq!(
               verdicts,
               [
                   never("app", &[1]),
                   never("lib", &[3]),
                   never("solo", &[]),
                   never("z", &[]),
               ]
           );
       }

       #[test]
       fn with_the_rows_of_a_build_every_target_is_clean() {
           let rows = rows_of_chain("z v1");
           let verdicts = verdicts_of_chain(own_of_chain(), &rows);
           for (verdict, raw) in verdicts.iter().zip(["//:app", "//:lib", "//:solo", "//:z"]) {
               let recorded = rows.get(&label(raw)).unwrap().key().clone();
               assert_eq!(verdict, &Verdict::Clean { key: recorded }, "{raw}");
           }
       }

       #[test]
       fn everything_downstream_of_a_dirty_target_is_conditional() {
           let rows = rows_of_chain("z v1");
           let mut own = own_of_chain();
           own[3].command = command(&["cc", "-O2"]);
           let verdicts = verdicts_of_chain(own, &rows);
           assert!(matches!(
               &verdicts[3],
               Verdict::Dirty {
                   reason: DirtyReason::Changed(differences),
                   key: PlannedKey::Finished(_),
               } if differences == &[Difference::Command]
           ));
           assert_eq!(
               verdicts[1],
               Verdict::Conditional(Unfinished {
                   own: components("lib", |_| {}),
                   awaiting: vec![NodeId::new(3)],
               })
           );
           assert_eq!(
               verdicts[0],
               Verdict::Conditional(Unfinished {
                   own: components("app", |_| {}),
                   awaiting: vec![NodeId::new(1)],
               })
           );
           assert!(matches!(verdicts[2], Verdict::Clean { .. }));
       }

       #[test]
       fn a_dependency_rebuilt_to_the_same_output_leaves_its_dependents_clean() {
           let mut rows = rows_of_chain("z v1");
           let mut own = own_of_chain();
           own[3].command = command(&["cc", "-O2"]);
           // `//:z` was rebuilt under its new key and produced the same bytes.
           rows.insert(label("//:z"), row(key(own[3].clone(), &[]), "z v1"));
           let verdicts = verdicts_of_chain(own, &rows);
           assert!(
               verdicts
                   .iter()
                   .all(|verdict| matches!(verdict, Verdict::Clean { .. })),
               "{verdicts:?}"
           );
       }

       #[test]
       fn a_dependency_rebuilt_to_another_output_makes_its_dependent_dirty() {
           let mut rows = rows_of_chain("z v1");
           let own = own_of_chain();
           rows.insert(label("//:z"), row(key(own[3].clone(), &[]), "z v2"));
           let verdicts = verdicts_of_chain(own, &rows);
           assert!(matches!(verdicts[3], Verdict::Clean { .. }));
           assert!(matches!(
               &verdicts[1],
               Verdict::Dirty {
                   reason: DirtyReason::Changed(differences),
                   key: PlannedKey::Finished(_),
               } if differences == &[Difference::Dep(label("//:z"))]
           ));
           assert!(matches!(verdicts[0], Verdict::Conditional(_)));
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::verdict
   error[E0432]: unresolved imports `super::Dependency`, `super::all`, `super::decide`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/plan/verdict.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Verdicts: what a build would do with each target.
   //!
   //! Its own file because this is the decision the whole phase exists to make: with a target's
   //! own components, what its dependencies produced and what its last build recorded, say whether
   //! it is clean, must run, or runs only if a dependency's output changes.
   //!
   //! Responsibilities: [`Dependency`], [`decide`] and [`all`].
   //!
   //! Non-responsibilities: listing how two keys differ, and the order targets are visited in.
   //! Each has its own sibling file.

   use crate::error::Result;
   use crate::plan::difference::{Known, between};
   use crate::types::{
       ActionKey, CacheRow, DepProducts, DirtyReason, Freshness, KeyComponents, Label, NodeId,
       OwnComponents, PlannedKey, Produced, TargetGraph, Unfinished, Verdict,
   };

   /// One dependency of the target being decided.
   ///
   /// The owned value: it carries the id a key awaits the dependency by, and the products read
   /// from its cache row. A comparison borrows the label and the products from it and leaves the
   /// id.
   #[derive(Debug)]
   pub(crate) struct Dependency<'a> {
       /// The dependency's id.
       pub(crate) id: NodeId,
       /// Its label.
       pub(crate) label: &'a Label,
       /// What its last build produced, when it is clean; `None` when it is not, because what it
       /// will produce is not known before it runs.
       pub(crate) products: Option<Vec<Produced>>,
   }

   /// The verdict for one target.
   ///
   /// `deps` holds the target's dependencies in the order they were written, and `row` what its
   /// last build recorded. The key is finished when every dependency is clean; otherwise it is left
   /// unfinished, awaiting the dependencies that are not. The rules, tried in order:
   ///
   /// 1. a target declared to always run is dirty;
   /// 2. a target with no row is dirty: it was never built;
   /// 3. with a finished key, it is clean when the key's digest equals the row's, and dirty with
   ///    every difference otherwise;
   /// 4. with an unfinished key, it is dirty when something already known differs, and conditional
   ///    when nothing does.
   ///
   /// # Errors
   ///
   /// Returns [`Error::CanonicalJson`](crate::Error::CanonicalJson) when a finished key cannot be
   /// serialized.
   pub(crate) fn decide(
       freshness: Freshness,
       own: OwnComponents,
       deps: &[Dependency<'_>],
       row: Option<&CacheRow>,
   ) -> Result<Verdict> {
       let known: Vec<Known<'_>> = deps
           .iter()
           .map(|dep| Known {
               label: dep.label,
               products: dep.products.as_deref(),
           })
           .collect();
       let differences = row.map(|row| between(&own, &known, row.key().components()));

       let awaiting: Vec<NodeId> = deps
           .iter()
           .filter(|dep| dep.products.is_none())
           .map(|dep| dep.id)
           .collect();
       let key = if awaiting.is_empty() {
           let deps = deps
               .iter()
               .map(|dep| DepProducts {
                   label: dep.label.clone(),
                   outputs: dep.products.clone().unwrap_or_default(),
               })
               .collect();
           PlannedKey::Finished(ActionKey::new(KeyComponents { own, deps })?)
       } else {
           PlannedKey::Unfinished(Unfinished { own, awaiting })
       };

       if freshness == Freshness::Always {
           return Ok(Verdict::Dirty {
               reason: DirtyReason::Always,
               key,
           });
       }
       let (Some(row), Some(differences)) = (row, differences) else {
           return Ok(Verdict::Dirty {
               reason: DirtyReason::NeverBuilt,
               key,
           });
       };
       Ok(match key {
           PlannedKey::Finished(key) if key.digest() == row.key().digest() => Verdict::Clean { key },
           PlannedKey::Unfinished(unfinished) if differences.is_empty() => {
               Verdict::Conditional(unfinished)
           }
           key => Verdict::Dirty {
               reason: DirtyReason::Changed(differences),
               key,
           },
       })
   }

   /// The verdict for every target of `graph`, in id order.
   ///
   /// `own` holds one set of components per node, in id order; `order` is the graph's nodes with
   /// each after all it depends on; `row` answers with what a target's last build recorded.
   ///
   /// # Errors
   ///
   /// Returns whatever [`decide`] returns.
   pub(crate) fn all<'r>(
       graph: &TargetGraph,
       own: Vec<OwnComponents>,
       order: &[NodeId],
       row: impl Fn(&Label) -> Option<&'r CacheRow>,
   ) -> Result<Vec<Verdict>> {
       let mut own: Vec<Option<OwnComponents>> = own.into_iter().map(Some).collect();
       let mut verdicts: Vec<Option<Verdict>> = vec![None; graph.len()];
       for id in order {
           let components = own.get_mut(id.index()).and_then(Option::take);
           let (Some(node), Some(components)) = (graph.node(*id), components) else {
               continue;
           };
           let deps: Vec<Dependency<'_>> = graph
               .deps(*id)
               .unwrap_or_default()
               .iter()
               .filter_map(|dep| {
                   let label = &graph.node(*dep)?.label;
                   let clean = matches!(verdicts.get(dep.index()), Some(Some(Verdict::Clean { .. })));
                   let products = clean.then(|| row(label)).flatten().map(CacheRow::produced);
                   Some(Dependency {
                       id: *dep,
                       label,
                       products,
                   })
               })
               .collect();
           let verdict = decide(node.freshness, components, &deps, row(&node.label))?;
           if let Some(slot) = verdicts.get_mut(id.index()) {
               *slot = Some(verdict);
           }
       }
       Ok(verdicts.into_iter().flatten().collect())
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::verdict
   test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 307 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/plan/verdict.rs crates/buildl-core/src/plan/mod.rs
   $ git commit -m "feat(buildl-core): decide a verdict for every target"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- Every node of a diamond comes after all it depends on; a node comes after a dependency with a
  higher id; a chain of 100 000 nodes is ordered.
- Each of the eleven differences is reported alone when its component alone changes, and together
  in the one fixed order; a dependency with unknown products is reported only when added.
- Every row of the verdict rule is covered: always, never built, clean, dirty when settled, dirty
  and conditional when not; a clean dependency that produced something else dirties its dependent;
  awaited dependencies keep written order.
- Over a graph: with no rows everything is never built; with a build's rows everything is clean;
  downstream of a dirty target everything is conditional; a dependency rebuilt to the same output
  leaves its dependents clean, and one rebuilt to another output dirties its dependent.
- `crates/expected-module-edges.txt` is unchanged.

## Review findings

One reviewer pass over the plan's whole diff, 2026-10-11. Verdict: compliant with the plan and the spec, blocking set empty; `decide` and `all` match spec §5.6 row for row. The reviewer's own gate run: `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 4.92 seconds.`), `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`).

- missing guard (risk) — `all` returns `Ok` with fewer verdicts when `own` is short or `order` leaves a node out, and the verdicts that remain shift position; `Plan::new` compares no lengths. Unreachable while the function plan `10` writes passes one component set per node and `topological(graph)`; nothing enforces either — `plan/verdict.rs`, `all`
- spec coverage (risk) — spec §9's "together, in the stated order" is not pinned for `InputOrder`: emitting it after `Outputs` leaves every test green. One case that reorders the inputs and changes the outputs would pin it — `plan/difference.rs`, tests
- doc accuracy (nit) — "the list is empty exactly when the two keys are equal" holds only while no input path or dependency label repeats. No graph has such repeats, so it is a matter of the sentence only — `plan/difference.rs`, `between`
- performance (nit) — `between` runs before the `Always` check, so an always-run target with a row pays for a comparison whose result is dropped — `plan/verdict.rs`, `decide`
- strong-types (nit) — `dep.products.clone().unwrap_or_default()` is a fallback the finished branch can never take — `plan/verdict.rs`, `decide`

No finding was fixed: none is in the blocking set.

## Probe results

- **Each rule of spec §5.6 is pinned by a test.** The reviewer mutated a scratch copy: `Always` swapped with `NeverBuilt` → 1 test fails; the conditional guard made always true → 1 test fails; a dependency's products taken from its row whether or not it is clean → 2 tests fail; the order's queue popped from the back → 2 tests fail. Baseline `cargo test -p buildl-core --lib plan::` → `67 passed; 0 failed`. Not against the plan.
- **A node missing from `order` shortens the result silently.** The reviewer's probe on the four-node chain with node 1 left out of `order`: `graph.len()=4 verdicts.len()=3`, and `verdicts[1]` is `//:solo`'s. Not against the plan, which makes `all`'s caller responsible; recorded as the first finding above.
- **The place of `InputOrder` in the stated order is not pinned.** The reviewer's reversion emitting `InputOrder` after `Outputs`: exit `0`, `67 passed; 0 failed`. Against the plan's verification summary, which says the eleven differences are reported in the stated order: ten of the eleven positions are pinned.
- **A repeated input path gives a dirty verdict with no difference.** The reviewer's probe with one path written twice: `between = []`, `decide = Dirty { reason: Changed([]) }`. Not reachable: `Node::inputs` holds no repeats.
- **The three tasks leave the tree the plan was written from.** `diff -rq <prototype snapshot after plan 09>/crates crates` names only `error.rs`, `ports/port_set.rs` and `types/action_key.rs`, the three files plans `02`, `05` and `06` record as departing from their text. Not against the plan.
- Each task's red step was run before its green one: `cargo test -p buildl-core --lib plan::order` → ``error[E0432]: unresolved import `super::topological` ``, then `5 passed; 0 failed`; `plan::difference` → ``error[E0432]: unresolved imports `super::Known`, `super::between`, `super::changed`, `super::reordered` ``, then `9 passed; 0 failed`; `plan::verdict` → ``error[E0432]: unresolved imports `super::Dependency`, `super::all`, `super::decide` ``, then `14 passed; 0 failed`.

## Deviations

- 2026-10-11 — the commit steps were not run. The work was left in the working tree, uncommitted, because committing is the author's act in this run.
- 2026-10-11 — each quoted `filtered out` figure came out one higher than the plan's (`294`, `299`, `308`): plan `06`'s fix added one unit test to the crate. The `passed` figures match.
- 2026-10-11 — after the pull request's first CI run, four test assertions depart from the plan's text: one `assert!(….is_empty())` in `plan/order.rs` and three in `plan/difference.rs` became `assert_eq!(…, [])`, the last of them `assert_eq!(changed(&now, &now), Vec::<&str>::new())` because `[]` alone leaves the element type open. Rust 1.99, which the pull request's CI installed while the plan was written and run on 1.98, adds clippy's `assert_is_empty`, and the gate failed on it there. The assertions check the same thing and now show the value when they fail. `cargo make dod` on 1.99 after the change: exit `0`, `[cargo-make] INFO - Build Done in 29.17 seconds.`
