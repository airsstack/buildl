---
status: approved
created: 2026-10-11
depends-on: [01, 04]
---

# Before Run Implementation Plan

**Goal:** What a build is about to run is settled from plain values, before any port is asked.

**Architecture:** The phase module `execute/` starts here, with three logic files and its test fixtures. `execute/layout.rs` holds `Layout`: one path per output of every target, computed with `OutputPath::of`, and the check that no two outputs would be written to one place — the same path, or a path another output needs as a directory. `execute/expand.rs` holds `expand`, which replaces a declared command's placeholders with workspace-relative text: `$in`, `$out` and `$deps` are lists of paths, spliced one argument per path when a list is a whole argument and joined by one space anywhere else, and `$opt:name` is a setting's value. It returns texts, not a `Command`, because an expanded command may hold no argument at all. `execute/admission.rs` holds `admit`, which turns those texts into a `Command` or refuses them: a command that names no program once expanded, or a program the workspace's ceiling does not allow. That is the same `Error::Refused` Plan raises, so a program first named by expansion is refused before anything runs like any other. Every function here takes values and none takes a port; nothing calls them yet.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D1, D2, D4, D5, D20), §5.2 (layout), §5.3 (expansion), §5.4 (admission), §7 (the renderings of `OutputConflict`, `NoProgram` and `Refused`), §8 (the module layout, the fixtures, the edge file), §9 (the rows for `execute/layout.rs`, `execute/expand.rs` and `execute/admission.rs`).

**Checkpoints:** review once, when all three tasks are done.

---

## Context an implementer needs

What plans `01` and `04` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `OutputPath` (`Ord`, `Clone`, `Display` is its text, `as_str`); built only by `OutputPath::of(&Directory, &Label, &OutputName)`, which joins the three texts with `/` and lets a root directory contribute nothing | `crates/buildl-core/src/types/output_path.rs:35` | what `Layout` holds; two of the three lists `expand` reads |
| `Error::OutputConflict { path: OutputPath, first: Box<DeclarationSite>, second: Box<DeclarationSite> }`, rendered `{path}: {first label} ({first file}) and {second label} ({second file}) cannot both write here` | `crates/buildl-core/src/error.rs:252` | what `Layout::of` returns for two outputs in one place |
| `Error::NoProgram { site: Box<DeclarationSite> }`, rendered `{file}: {label}: the command names no program once expanded` | `crates/buildl-core/src/error.rs:267` | what `admit` returns for a command with no program |
| `Error::Refused { want: Want, site: Option<Box<DeclarationSite>>, ceiling: Box<Ceiling> }` | `crates/buildl-core/src/error.rs:201` | what `admit` returns for a program outside the ceiling; `plan/authority.rs:72` builds the same shape |
| `DeclarationSite::from(&Node)`: the node's label and build file | `crates/buildl-core/src/error.rs:406` | the site of every error here |
| `TargetGraph` (`nodes()` in label order, `deps(id) -> Option<&[NodeId]>` in written order), `Node` (public fields), `TargetGraph::new` (`pub(crate)`) | `crates/buildl-core/src/types/target_graph.rs` | what `Layout` reads; the constructor the fixtures use |
| `Argument::setting_references()`: the names an argument refers to with `$opt:`, in the order they appear; a name is the longest run of ASCII letters, digits, `-` and `_`, and an `$opt:` followed by none is not a reference | `crates/buildl-core/src/types/argument.rs:72` | where `expand` takes every setting name from |
| `Argument::parse`, `Command::new` (refuses an empty vector), `Command::arguments` | `crates/buildl-core/src/types/argument.rs` | how `admit` builds the command |
| `Program::of(&Argument)`: `None` for an argument that is empty or holds `$` | `crates/buildl-core/src/types/program.rs:72` | what decides whether a first argument names a program |
| `Ceiling` with `run: BTreeSet<Program>`; `Want::Program` | `crates/buildl-core/src/types/authority.rs` | what `admit` checks the program against |
| `SettingName::parse`, `SettingValue::as_str` | `crates/buildl-core/src/types/setting.rs` | how `expand` looks a setting up |
| `pub mod error;` followed by `pub mod json;` | `crates/buildl-core/src/lib.rs` | where `pub mod execute;` joins them |
| 20 lines, `error -> types` followed by `json -> error` | `crates/expected-module-edges.txt` | the golden list `cargo make dod` diffs |

Gate facts that bite here. A bullet that names a lint reports one seen as a failing run while this
code was prototyped:

- Nothing outside a file's own tests calls its functions until the function that runs a build is
  written, so the library build reports them as dead code and the gate fails. Each `mod` line in
  `execute/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It must be
  `cfg_attr(not(test), …)`: in the test build the functions are used, and a plain `expect` would
  fail there as unfulfilled. The plan that writes that function removes the attributes.
- `execute/fixtures.rs` is compiled for tests only, and a fixture no test calls is dead code in
  the test build. Each task therefore adds only the builders its own tests use. The file repeats
  a few builders `plan/fixtures.rs` has: a phase module names no other phase, its test fixtures
  included.
- The module-edge guard lists `crate::<module>` imports per top-level module and diffs the list
  against `crates/expected-module-edges.txt`. The first file of `execute/` imports
  `crate::error` and `crate::types`, so task 1 adds two lines to that file in the same commit.
  An import of a sibling is written `crate::execute::<file>::…`, which the guard does not count
  as an edge.
- `pub mod execute;` is public from the start and exports nothing yet. The crate doc's module
  table and status paragraph in `lib.rs` are left alone by this plan.
- clippy's `assert_is_empty` rejects `assert!(list.is_empty())` on a slice or a `Vec`, so the
  tests compare with `[]`.
- The `$opt:` name rule is not restated: `types/grammar.rs:25` keeps it private to `types`, and
  `expand` takes the names from `Argument::setting_references`. What `expand` does repeat is the
  marker itself, as the constant `SETTING` (`opt:`, the text after the `$`), because
  `types/argument.rs:22` keeps its own private.
- The test module of `execute/expand.rs` unwraps nothing, so it carries no
  `expect(clippy::unwrap_used)`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/execute/mod.rs         — [create] module declarations
crates/buildl-core/src/execute/fixtures.rs    — [create] test-only builders: directories, labels, outputs, paths, settings, commands, nodes, graphs
crates/buildl-core/src/execute/layout.rs      — [create] Layout, unit tests
crates/buildl-core/src/execute/expand.rs      — [create] Lists, expand, unit tests
crates/buildl-core/src/execute/admission.rs   — [create] admit, unit tests
crates/buildl-core/src/lib.rs                 — [modify] declare the module
crates/expected-module-edges.txt              — [modify] two edges from execute
```

### Task 1 — Place every output and refuse two in one place

**Files:**
- Create `crates/buildl-core/src/execute/layout.rs`
- Create `crates/buildl-core/src/execute/fixtures.rs`
- Create `crates/buildl-core/src/execute/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/expected-module-edges.txt`

**Steps:**

1. Declare the phase. In `crates/buildl-core/src/lib.rs`, directly below `pub mod error;`, insert:

   ```rust
   pub mod execute;
   ```

2. Create `crates/buildl-core/src/execute/fixtures.rs`:

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
       Argument, Command, Directory, Freshness, Label, NetworkAccess, Node, NodeId, OutputName,
       Provenance, TargetGraph, TargetRole,
   };

   /// A directory from known-valid text.
   pub(crate) fn dir(raw: &str) -> Directory {
       Directory::parse(raw).unwrap()
   }

   /// A label from known-valid text.
   pub(crate) fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   /// Output names from known-valid text, in the order given.
   pub(crate) fn outputs(names: &[&str]) -> Vec<OutputName> {
       names
           .iter()
           .map(|name| OutputName::parse(*name).unwrap())
           .collect()
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
           outputs: outputs(&[label.name().as_str()]),
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
   ```

3. Create `crates/buildl-core/src/execute/mod.rs`:

   ```rust
   //! Execute, the fourth phase: from a plan to a finished build.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //! Recording what an action produced is the last step of running it, so it lives here too.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod layout;

   #[cfg(test)]
   mod fixtures;
   ```

4. Write the failing tests. Create `crates/buildl-core/src/execute/layout.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Layout;
       use crate::error::Error;
       use crate::execute::fixtures::{dir, graph, node, outputs};
       use crate::types::{Directory, NodeId, OutputPath, TargetGraph};

       fn texts(paths: &[OutputPath]) -> Vec<&str> {
           paths.iter().map(OutputPath::as_str).collect()
       }

       /// What `graph` is refused for under `out`: the path, and the two labels named, in order.
       fn conflict(graph: &TargetGraph) -> (String, String, String) {
           match Layout::of(graph, &dir("out")).unwrap_err() {
               Error::OutputConflict {
                   path,
                   first,
                   second,
               } => (
                   path.to_string(),
                   first.label.to_string(),
                   second.label.to_string(),
               ),
               other => unreachable!("expected OutputConflict, got {other:?}"),
           }
       }

       fn named(path: &str, first: &str, second: &str) -> (String, String, String) {
           (path.to_owned(), first.to_owned(), second.to_owned())
       }

       #[test]
       fn every_output_lives_under_the_output_directory_and_its_labels_directory() {
           let graph = graph(
               vec![
                   node("//:app", |node| node.outputs = outputs(&["app", "app.map"])),
                   node("//lib:text", |_| {}),
               ],
               &[&[], &[]],
           );
           let layout = Layout::of(&graph, &dir("out")).unwrap();
           assert_eq!(
               texts(layout.outputs(NodeId::new(0))),
               ["out/app", "out/app.map"]
           );
           assert_eq!(texts(layout.outputs(NodeId::new(1))), ["out/lib/text"]);
           assert_eq!(layout.outputs(NodeId::new(2)), []);

           let at_root = Layout::of(&graph, &Directory::root()).unwrap();
           assert_eq!(texts(at_root.outputs(NodeId::new(1))), ["lib/text"]);
       }

       #[test]
       fn dependency_outputs_come_in_edge_order_then_in_written_order() {
           let graph = graph(
               vec![
                   node("//:a", |node| node.outputs = outputs(&["z", "a"])),
                   node("//:app", |_| {}),
                   node("//:c", |_| {}),
               ],
               &[&[], &[2, 0], &[]],
           );
           let layout = Layout::of(&graph, &dir("out")).unwrap();
           assert_eq!(
               texts(&layout.deps(&graph, NodeId::new(1))),
               ["out/c", "out/z", "out/a"]
           );
           assert_eq!(layout.deps(&graph, NodeId::new(0)), []);
           assert_eq!(layout.deps(&graph, NodeId::new(3)), []);
       }

       #[test]
       fn two_targets_may_not_claim_one_path() {
           let graph = graph(
               vec![
                   node("//:app", |node| node.outputs = outputs(&["bin/tool"])),
                   node("//bin:tool", |_| {}),
               ],
               &[&[], &[]],
           );
           assert_eq!(
               conflict(&graph),
               named("out/bin/tool", "//:app", "//bin:tool")
           );
       }

       #[test]
       fn an_output_may_not_lie_under_another() {
           let graph = graph(
               vec![node("//:lib", |_| {}), node("//lib:text", |_| {})],
               &[&[], &[]],
           );
           assert_eq!(conflict(&graph), named("out/lib", "//:lib", "//lib:text"));
           assert_eq!(
               Layout::of(&graph, &dir("out")).unwrap_err().to_string(),
               "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
           );
       }

       #[test]
       fn the_lower_label_is_named_first_whichever_path_it_claims() {
           let graph = graph(
               vec![
                   node("//:a", |node| node.outputs = outputs(&["sub/x"])),
                   node("//:z", |node| node.outputs = outputs(&["sub"])),
               ],
               &[&[], &[]],
           );
           assert_eq!(conflict(&graph), named("out/sub", "//:a", "//:z"));
       }

       #[test]
       fn a_name_that_only_starts_like_another_is_no_conflict() {
           let graph = graph(
               vec![
                   node("//:lib", |node| node.outputs = outputs(&["lib", "lib.o"])),
                   node("//:lib2", |_| {}),
                   node("//lib3:text", |_| {}),
               ],
               &[&[], &[], &[]],
           );
           assert!(Layout::of(&graph, &dir("out")).is_ok());
       }

       #[test]
       fn a_conflict_is_found_past_a_path_that_sorts_between_the_two() {
           let graph = graph(
               vec![
                   node("//:lib", |node| node.outputs = outputs(&["lib", "lib.o"])),
                   node("//lib:text", |_| {}),
               ],
               &[&[], &[]],
           );
           assert_eq!(conflict(&graph), named("out/lib", "//:lib", "//lib:text"));
       }

       #[test]
       fn one_target_claiming_a_path_twice_names_itself_twice() {
           let same = graph(
               vec![node("//:app", |node| {
                   node.outputs = outputs(&["app", "app"]);
               })],
               &[&[]],
           );
           assert_eq!(conflict(&same), named("out/app", "//:app", "//:app"));

           let under = graph(
               vec![node("//:app", |node| {
                   node.outputs = outputs(&["gen/x", "gen"]);
               })],
               &[&[]],
           );
           assert_eq!(conflict(&under), named("out/gen", "//:app", "//:app"));
       }

       #[test]
       fn the_lowest_conflicting_path_is_the_one_reported() {
           let graph = graph(
               vec![
                   node("//:a", |node| node.outputs = outputs(&["m"])),
                   node("//:b", |node| node.outputs = outputs(&["m", "k"])),
                   node("//:c", |node| node.outputs = outputs(&["k"])),
               ],
               &[&[], &[], &[]],
           );
           assert_eq!(conflict(&graph), named("out/k", "//:b", "//:c"));
       }
   }
   ```

5. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::layout
   error[E0432]: unresolved import `super::Layout`
   ```

6. Replace the placeholder line at the top of `crates/buildl-core/src/execute/layout.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Layout: where every output of every target lives, and that no two of them collide.
   //!
   //! Its own file because an output's place is decided once, before anything runs, from the
   //! workspace's output directory and the target's label, and a build whose targets would write
   //! over one another is refused here rather than found out halfway through.
   //!
   //! Responsibilities: [`Layout`].
   //!
   //! Non-responsibilities: the rule that turns a label and a name into a path, which is
   //! [`OutputPath::of`]; and putting anything at a path, which is a port's.

   use std::collections::BTreeMap;

   use crate::error::{DeclarationSite, Error, Result};
   use crate::types::{Directory, Node, NodeId, OutputPath, TargetGraph};

   /// The path of every output of every target of one graph.
   #[derive(Debug)]
   pub(crate) struct Layout {
       /// One list per node, in id order: the node's outputs, in the order they were written.
       paths: Vec<Vec<OutputPath>>,
   }

   impl Layout {
       /// Places every output of `graph` under `out`.
       ///
       /// # Errors
       ///
       /// Returns [`Error::OutputConflict`] when two outputs would be written to one place: the
       /// same path, or a path another output needs as a directory. The outputs are taken in path
       /// order, then label order, and the first that collides with one taken before it is the one
       /// reported, with the shorter of the two paths and the two targets, lower label first. A
       /// target that declares both outputs is named twice.
       pub(crate) fn of(graph: &TargetGraph, out: &Directory) -> Result<Self> {
           let paths: Vec<Vec<OutputPath>> = graph
               .nodes()
               .iter()
               .map(|node| {
                   node.outputs
                       .iter()
                       .map(|name| OutputPath::of(out, &node.label, name))
                       .collect()
               })
               .collect();

           let mut claims: Vec<(&OutputPath, &Node)> = graph
               .nodes()
               .iter()
               .zip(&paths)
               .flat_map(|(node, paths)| paths.iter().map(move |path| (path, node)))
               .collect();
           claims.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.label.cmp(&b.1.label)));

           let mut taken: BTreeMap<&str, (&OutputPath, &Node)> = BTreeMap::new();
           for (path, node) in claims {
               if let Some((shorter, holder)) = collision(&taken, path.as_str()) {
                   let (first, second) = if holder.label <= node.label {
                       (holder, node)
                   } else {
                       (node, holder)
                   };
                   return Err(Error::OutputConflict {
                       path: shorter.clone(),
                       first: Box::new(DeclarationSite::from(first)),
                       second: Box::new(DeclarationSite::from(second)),
                   });
               }
               taken.insert(path.as_str(), (path, node));
           }
           Ok(Self { paths })
       }

       /// The paths of the outputs of the node `id` names, in the order they were written; empty
       /// for an id the graph did not issue.
       pub(crate) fn outputs(&self, id: NodeId) -> &[OutputPath] {
           self.paths.get(id.index()).map_or(&[], Vec::as_slice)
       }

       /// The paths of every output of every node `id` depends on: the dependencies in the order
       /// they were written, and each one's outputs in the order they were written.
       pub(crate) fn deps(&self, graph: &TargetGraph, id: NodeId) -> Vec<OutputPath> {
           graph
               .deps(id)
               .unwrap_or_default()
               .iter()
               .flat_map(|dep| self.outputs(*dep))
               .cloned()
               .collect()
       }
   }

   /// The path already taken that `path` collides with, and the target that took it: `path`
   /// itself, or a directory above it.
   ///
   /// A path sorts after every directory above it, so a collision is always with a path taken
   /// earlier; and since the paths taken so far collide with one another nowhere, there is at most
   /// one.
   fn collision<'a>(
       taken: &BTreeMap<&str, (&'a OutputPath, &'a Node)>,
       path: &str,
   ) -> Option<(&'a OutputPath, &'a Node)> {
       let above = path.match_indices('/').filter_map(|(at, _)| path.get(..at));
       core::iter::once(path)
           .chain(above)
           .find_map(|candidate| taken.get(candidate).copied())
   }
   ```

   The outputs are sorted by path and then by label before they are compared, which is what makes the first collision found the one with the lowest path. A path sorts after every directory above it, so each output is compared only with what was taken before it: itself, and each of its parent directories. `lib.o` sorts between `lib` and `lib/text`, which is why the comparison looks the parents up rather than looking at the previous entry.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::layout
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 349 filtered out; finished in 0.00s
   ```

8. Record the two new module edges. In `crates/expected-module-edges.txt`, directly below `error -> types`, insert:

   ```
   execute -> error
   execute -> types
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

10. Commit:

    ```
    $ git add crates/buildl-core/src/execute crates/buildl-core/src/lib.rs crates/expected-module-edges.txt
    $ git commit -m "feat(buildl-core): place every output and refuse two in one place"
    ```

### Task 2 — Expand a command's placeholders

**Files:**
- Create `crates/buildl-core/src/execute/expand.rs`
- Modify `crates/buildl-core/src/execute/fixtures.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Add the builders this task's tests use: `path`, `output_path` and `settings`. `crates/buildl-core/src/execute/fixtures.rs` becomes:

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
       Argument, Command, Directory, Freshness, Label, NetworkAccess, Node, NodeId, OutputName,
       OutputPath, Provenance, SettingName, SettingValue, SourcePath, TargetGraph, TargetRole,
   };

   /// A directory from known-valid text.
   pub(crate) fn dir(raw: &str) -> Directory {
       Directory::parse(raw).unwrap()
   }

   /// A label from known-valid text.
   pub(crate) fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   /// Output names from known-valid text, in the order given.
   pub(crate) fn outputs(names: &[&str]) -> Vec<OutputName> {
       names
           .iter()
           .map(|name| OutputName::parse(*name).unwrap())
           .collect()
   }

   /// A workspace file path from known-valid text.
   pub(crate) fn path(raw: &str) -> SourcePath {
       SourcePath::parse(raw).unwrap()
   }

   /// Where the output `name` of the target `raw` lives in a workspace whose outputs go under
   /// `out`.
   pub(crate) fn output_path(raw: &str, name: &str) -> OutputPath {
       OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
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
           outputs: outputs(&[label.name().as_str()]),
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
   ```

2. Declare the module. `crates/buildl-core/src/execute/mod.rs` becomes:

   ```rust
   //! Execute, the fourth phase: from a plan to a finished build.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //! Recording what an action produced is the last step of running it, so it lives here too.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod expand;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod layout;

   #[cfg(test)]
   mod fixtures;
   ```

3. Write the failing tests. Create `crates/buildl-core/src/execute/expand.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use std::collections::BTreeMap;

       use super::{Lists, expand};
       use crate::execute::fixtures::{command, output_path, path, settings};
       use crate::types::{OutputPath, SettingName, SettingValue, SourcePath};

       /// `declared`, expanded with no setting and these three lists.
       fn expanded(
           declared: &[&str],
           inputs: &[SourcePath],
           outputs: &[OutputPath],
           deps: &[OutputPath],
       ) -> Vec<String> {
           with_settings(declared, &BTreeMap::new(), inputs, outputs, deps)
       }

       fn with_settings(
           declared: &[&str],
           settings: &BTreeMap<SettingName, SettingValue>,
           inputs: &[SourcePath],
           outputs: &[OutputPath],
           deps: &[OutputPath],
       ) -> Vec<String> {
           let lists = Lists {
               inputs,
               outputs,
               deps,
           };
           expand(&command(declared), settings, &lists)
       }

       #[test]
       fn a_compile_command_names_its_input_and_its_output() {
           assert_eq!(
               expanded(
                   &["cc", "-c", "$in", "-o", "$out"],
                   &[path("lib/a.c")],
                   &[output_path("//lib:text", "text")],
                   &[],
               ),
               ["cc", "-c", "lib/a.c", "-o", "out/lib/text"]
           );
       }

       #[test]
       fn a_link_command_names_every_output_of_its_dependencies() {
           assert_eq!(
               expanded(
                   &["cc", "$deps", "-o", "$out"],
                   &[],
                   &[output_path("//:app", "app")],
                   &[
                       output_path("//:main.o", "main.o"),
                       output_path("//lib:text", "text")
                   ],
               ),
               ["cc", "out/main.o", "out/lib/text", "-o", "out/app"]
           );
       }

       #[test]
       fn a_test_command_runs_what_its_dependency_produced_with_a_setting() {
           assert_eq!(
               with_settings(
                   &["$deps", "--filter=$opt:test_filter"],
                   &settings(&[("test_filter", "Login")]),
                   &[],
                   &[],
                   &[output_path("//:app", "app")],
               ),
               ["out/app", "--filter=Login"]
           );
       }

       #[test]
       fn an_empty_list_alone_leaves_no_argument() {
           assert_eq!(expanded(&["gen", "$in"], &[], &[], &[]), ["gen"]);
           assert_eq!(
               expanded(&["$deps"], &[], &[], &[]),
               Vec::<String>::new(),
               "a command may expand to nothing"
           );
       }

       #[test]
       fn a_list_alone_becomes_one_argument_per_path() {
           assert_eq!(
               expanded(
                   &["cc", "$in"],
                   &[path("src/b.c"), path("src/a.c")],
                   &[],
                   &[]
               ),
               ["cc", "src/b.c", "src/a.c"]
           );
       }

       #[test]
       fn a_list_inside_other_text_is_joined_by_one_space() {
           let inputs = [path("src/b.c"), path("src/a.c")];
           assert_eq!(
               expanded(&["--srcs=$in", "--none=$deps"], &inputs, &[], &[]),
               ["--srcs=src/b.c src/a.c", "--none="]
           );
       }

       #[test]
       fn two_placeholders_in_one_argument_are_both_expanded() {
           assert_eq!(
               expanded(
                   &["$in:$out", "$out/app", "$in$deps"],
                   &[path("a.c")],
                   &[output_path("//:app", "bin")],
                   &[output_path("//:lib", "lib")],
               ),
               ["a.c:out/bin", "out/bin/app", "a.cout/lib"]
           );
       }

       #[test]
       fn a_placeholder_is_the_longest_run_of_letters_after_the_dollar() {
           assert_eq!(
               expanded(
                   &["$input", "$outs", "$dep", "$IN", "$in2", "$out.map", "$$in"],
                   &[path("a.c")],
                   &[output_path("//:app", "app")],
                   &[],
               ),
               [
                   "$input",
                   "$outs",
                   "$dep",
                   "$IN",
                   "a.c2",
                   "out/app.map",
                   "$a.c"
               ]
           );
       }

       #[test]
       fn a_dollar_that_starts_no_placeholder_stays_text() {
           assert_eq!(
               with_settings(
                   &["$", "a$", "$1", "$opt:", "$opt:/x", "$opt", "é$é"],
                   &settings(&[("x", "X")]),
                   &[],
                   &[],
                   &[],
               ),
               ["$", "a$", "$1", "$opt:", "$opt:/x", "$opt", "é$é"]
           );
       }

       #[test]
       fn a_setting_reference_becomes_the_settings_value() {
           assert_eq!(
               with_settings(
                   &["$opt:mode", "-O$opt:level-$opt:mode", "$opt:empty"],
                   &settings(&[("mode", "debug"), ("level", "2"), ("empty", "")]),
                   &[],
                   &[],
                   &[],
               ),
               ["debug", "-O$opt:level-debug", ""],
               "a name runs through `-`, so `level-` is one name, and it is not a setting"
           );
       }

       #[test]
       fn a_reference_to_a_setting_that_is_not_held_stays_text() {
           assert_eq!(
               with_settings(
                   &["$opt:missing", "--a=$opt:missing,$opt:mode"],
                   &settings(&[("mode", "debug")]),
                   &[],
                   &[],
                   &[],
               ),
               ["$opt:missing", "--a=$opt:missing,debug"]
           );
       }

       #[test]
       fn substituted_text_is_never_expanded_again() {
           assert_eq!(
               with_settings(
                   &["$opt:flag", "--x=$opt:flag", "$opt:nested"],
                   &settings(&[("flag", "$in"), ("nested", "$opt:flag")]),
                   &[path("a.c")],
                   &[],
                   &[],
               ),
               ["$in", "--x=$in", "$opt:flag"]
           );
       }

       #[test]
       fn a_setting_named_like_a_list_is_still_a_setting() {
           assert_eq!(
               with_settings(
                   &["$opt:in", "$opt:out$in"],
                   &settings(&[("in", "I"), ("out", "O")]),
                   &[path("a.c")],
                   &[],
                   &[],
               ),
               ["I", "Oa.c"]
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::expand
   error[E0432]: unresolved imports `super::Lists`, `super::expand`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/execute/expand.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Expansion: a declared command's placeholders replaced by what they stand for.
   //!
   //! Its own file because a build file never holds a real path: a command is written with `$in`,
   //! `$out`, `$deps` and `$opt:name`, and what an action is handed is the command with each of
   //! them replaced. The replacement is workspace-relative text, the same wherever the action
   //! runs, so it is known before anything does.
   //!
   //! Responsibilities: [`Lists`] and [`expand`].
   //!
   //! Non-responsibilities: whether the expanded command names a program that may run; and which
   //! characters a setting's name is made of, which [`Argument::setting_references`] decides.

   use std::collections::BTreeMap;

   use crate::types::{Argument, Command, OutputPath, SettingName, SettingValue, SourcePath};

   /// What follows the `$` of a reference to a build setting, before the setting's name.
   const SETTING: &str = "opt:";

   /// The three lists of paths one target's placeholders stand for.
   #[derive(Debug, Clone, Copy)]
   pub(crate) struct Lists<'a> {
       /// What `$in` stands for: the target's inputs, in the order they were written.
       pub(crate) inputs: &'a [SourcePath],
       /// What `$out` stands for: where the target's own outputs live, in the order they were
       /// written.
       pub(crate) outputs: &'a [OutputPath],
       /// What `$deps` stands for: where every output of every dependency lives.
       pub(crate) deps: &'a [OutputPath],
   }

   impl<'a> Lists<'a> {
       /// The list a placeholder's word names: `in`, `out` or `deps`, and nothing else.
       fn named(&self, word: &str) -> Option<Vec<&'a str>> {
           match word {
               "in" => Some(self.inputs.iter().map(SourcePath::as_str).collect()),
               "out" => Some(self.outputs.iter().map(OutputPath::as_str).collect()),
               "deps" => Some(self.deps.iter().map(OutputPath::as_str).collect()),
               _ => None,
           }
       }
   }

   /// `command` with its placeholders replaced, as the texts of its arguments.
   ///
   /// Each argument is read once, left to right. At a `$`:
   ///
   /// - `$opt:` and a setting's name becomes the value `settings` holds for it, and stays as it is
   ///   when `settings` holds none;
   /// - `$` and a run of ASCII letters that is exactly `in`, `out` or `deps` becomes that list of
   ///   `lists`. The run is the longest one, so `$input` is text and `$out/app` is the outputs
   ///   followed by `/app`;
   /// - anything else is text.
   ///
   /// An argument that is one list and nothing else becomes one argument per path, and none when
   /// the list is empty. Anywhere else a list becomes its paths joined by one space. Text that was
   /// substituted is never read again, so a setting whose value is `$in` yields `$in`.
   ///
   /// The result may hold no argument at all, and its first may not name a program: both are for
   /// the caller to refuse.
   pub(crate) fn expand(
       command: &Command,
       settings: &BTreeMap<SettingName, SettingValue>,
       lists: &Lists<'_>,
   ) -> Vec<String> {
       let mut expanded = Vec::new();
       for argument in command.arguments() {
           let alone = argument
               .as_str()
               .strip_prefix('$')
               .and_then(|word| lists.named(word));
           match alone {
               Some(paths) => expanded.extend(paths.into_iter().map(str::to_owned)),
               None => expanded.push(within(argument, settings, lists)),
           }
       }
       expanded
   }

   /// One argument with every placeholder inside it replaced.
   ///
   /// The names of the settings it refers to are taken from [`Argument::setting_references`], in
   /// the order they appear, and matched against the text as it is read: this function finds the
   /// same `$opt:` occurrences in the same order, so the next name is always the one at hand.
   fn within(
       argument: &Argument,
       settings: &BTreeMap<SettingName, SettingValue>,
       lists: &Lists<'_>,
   ) -> String {
       let mut references = argument.setting_references().into_iter().peekable();
       let mut text = String::new();
       let mut rest = argument.as_str();
       while let Some((before, after)) = rest.split_once('$') {
           text.push_str(before);
           rest = if let Some((name, tail)) = reference(after, references.peek().copied()) {
               references.next();
               match value(settings, name) {
                   Some(value) => text.push_str(value.as_str()),
                   None => text.extend(["$", SETTING, name]),
               }
               tail
           } else {
               let tail = after.trim_start_matches(|letter: char| letter.is_ascii_alphabetic());
               let word = after.strip_suffix(tail).unwrap_or_default();
               if let Some(paths) = lists.named(word) {
                   text.push_str(&paths.join(" "));
                   tail
               } else {
                   text.push('$');
                   after
               }
           };
       }
       text.push_str(rest);
       text
   }

   /// The setting name `after` starts a reference to, with the text that follows the name, when
   /// that name is `next`: the next name the argument refers to.
   ///
   /// A `$opt:` followed by no name is not a reference, and then `after` cannot continue with
   /// `next`: a name starts with a character a name may hold, and the text here does not.
   fn reference<'t>(after: &'t str, next: Option<&'t str>) -> Option<(&'t str, &'t str)> {
       let name = next?;
       let tail = after.strip_prefix(SETTING)?.strip_prefix(name)?;
       Some((name, tail))
   }

   /// The value `settings` holds for the setting written as `name`.
   fn value<'s>(
       settings: &'s BTreeMap<SettingName, SettingValue>,
       name: &str,
   ) -> Option<&'s SettingValue> {
       SettingName::parse(name)
           .ok()
           .and_then(|name| settings.get(&name))
   }
   ```

   `within` reads an argument one `$` at a time and never reads what it wrote, which is why a setting whose value is `$in` yields `$in`. It takes the setting names from `Argument::setting_references` instead of deciding where a name ends: both walk the same text left to right and meet the same `$opt:` occurrences in the same order, and a `$opt:` with no name after it cannot be mistaken for the next reference, because that reference's name starts with a character a name may hold and the text at hand does not.

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::expand
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 358 filtered out; finished in 0.00s
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): expand a command's placeholders"
   ```

### Task 3 — Admit an expanded command or refuse it

**Files:**
- Create `crates/buildl-core/src/execute/admission.rs`
- Modify `crates/buildl-core/src/execute/fixtures.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Add the builder this task's tests use: `program`. `crates/buildl-core/src/execute/fixtures.rs` becomes:

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
       Argument, Command, Directory, Freshness, Label, NetworkAccess, Node, NodeId, OutputName,
       OutputPath, Program, Provenance, SettingName, SettingValue, SourcePath, TargetGraph,
       TargetRole,
   };

   /// A directory from known-valid text.
   pub(crate) fn dir(raw: &str) -> Directory {
       Directory::parse(raw).unwrap()
   }

   /// A label from known-valid text.
   pub(crate) fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   /// Output names from known-valid text, in the order given.
   pub(crate) fn outputs(names: &[&str]) -> Vec<OutputName> {
       names
           .iter()
           .map(|name| OutputName::parse(*name).unwrap())
           .collect()
   }

   /// A workspace file path from known-valid text.
   pub(crate) fn path(raw: &str) -> SourcePath {
       SourcePath::parse(raw).unwrap()
   }

   /// A program name from known-valid text.
   pub(crate) fn program(raw: &str) -> Program {
       Program::parse(raw).unwrap()
   }

   /// Where the output `name` of the target `raw` lives in a workspace whose outputs go under
   /// `out`.
   pub(crate) fn output_path(raw: &str, name: &str) -> OutputPath {
       OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
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
           outputs: outputs(&[label.name().as_str()]),
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
   ```

2. Declare the module. `crates/buildl-core/src/execute/mod.rs` becomes:

   ```rust
   //! Execute, the fourth phase: from a plan to a finished build.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //! Recording what an action produced is the last step of running it, so it lives here too.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod admission;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod expand;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod layout;

   #[cfg(test)]
   mod fixtures;
   ```

3. Write the failing tests. Create `crates/buildl-core/src/execute/admission.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeSet;

       use super::admit;
       use crate::error::{Error, NameKind};
       use crate::execute::fixtures::{command, node, program};
       use crate::types::{Ceiling, Want};

       /// A ceiling allowing `run` and nothing else.
       fn ceiling(run: &[&str]) -> Ceiling {
           Ceiling {
               run: run.iter().map(|raw| program(raw)).collect(),
               env: BTreeSet::new(),
               read: BTreeSet::new(),
               write: BTreeSet::new(),
           }
       }

       fn texts(arguments: &[&str]) -> Vec<String> {
           arguments
               .iter()
               .map(|argument| (*argument).to_owned())
               .collect()
       }

       /// Asserts that `expanded`, as the command of `//:app`, names no program, whatever may run.
       fn names_no_program(expanded: &[&str]) {
           let app = node("//:app", |_| {});
           let error = admit(texts(expanded), &app, &ceiling(&["cc", "out/app"])).unwrap_err();
           assert_eq!(
               error.to_string(),
               "build.lua: //:app: the command names no program once expanded"
           );
           match error {
               Error::NoProgram { site } => assert_eq!(site.label, app.label),
               other => unreachable!("expected NoProgram, got {other:?}"),
           }
       }

       #[test]
       fn a_command_whose_program_may_run_is_admitted_as_it_is() {
           let text = node("//lib:text", |_| {});
           assert_eq!(
               admit(texts(&["cc", "-c", "lib/a.c"]), &text, &ceiling(&["cc"])).unwrap(),
               command(&["cc", "-c", "lib/a.c"])
           );
       }

       #[test]
       fn only_the_first_argument_has_to_name_a_program() {
           let app = node("//:app", |_| {});
           assert_eq!(
               admit(texts(&["cc", "", "$input"]), &app, &ceiling(&["cc"])).unwrap(),
               command(&["cc", "", "$input"])
           );
       }

       #[test]
       fn a_program_first_named_by_expansion_is_held_to_the_ceiling() {
           let test = node("//:app_test", |_| {});
           let expanded = texts(&["out/app", "--filter=Login"]);

           assert_eq!(
               admit(expanded.clone(), &test, &ceiling(&["cc", "out/app"])).unwrap(),
               command(&["out/app", "--filter=Login"])
           );

           let allowed = ceiling(&["cc"]);
           let error = admit(expanded, &test, &allowed).unwrap_err();
           assert_eq!(
               error.to_string(),
               "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
           );
           match error {
               Error::Refused {
                   want,
                   site,
                   ceiling,
               } => {
                   assert_eq!(want, Want::Program(program("out/app")));
                   assert_eq!(site.unwrap().label, test.label);
                   assert_eq!(*ceiling, allowed);
               }
               other => unreachable!("expected Refused, got {other:?}"),
           }
       }

       #[test]
       fn a_command_that_expanded_to_no_argument_names_no_program() {
           names_no_program(&[]);
       }

       #[test]
       fn an_empty_first_argument_names_no_program() {
           names_no_program(&["", "out/app"]);
       }

       #[test]
       fn a_first_argument_still_holding_a_dollar_names_no_program() {
           names_no_program(&["$input"]);
           names_no_program(&["$opt:missing", "cc"]);
           names_no_program(&["out/$"]);
       }

       #[test]
       fn a_command_naming_no_program_is_not_asked_of_the_ceiling() {
           let app = node("//:app", |_| {});
           assert!(matches!(
               admit(Vec::new(), &app, &ceiling(&[])).unwrap_err(),
               Error::NoProgram { .. }
           ));
       }

       #[test]
       fn text_no_argument_can_hold_is_refused_as_an_invalid_argument() {
           let app = node("//:app", |_| {});
           match admit(texts(&["cc", "a\0b"]), &app, &ceiling(&["cc"])).unwrap_err() {
               Error::InvalidName { kind, .. } => assert_eq!(kind, NameKind::Argument),
               other => unreachable!("expected InvalidName, got {other:?}"),
           }
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::admission
   error[E0432]: unresolved import `super::admit`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/execute/admission.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Admission: whether an expanded command names a program, and whether that program may run.
   //!
   //! Its own file because a command can name its program only once its placeholders are expanded
   //! — a test that runs `$deps` runs what its dependency built — and a build is refused for what
   //! it wants before any of it runs. So every expanded command is held to the workspace's ceiling
   //! here, the ones whose program was written out in the build file included.
   //!
   //! Responsibilities: [`admit`].
   //!
   //! Non-responsibilities: expanding a command; and what a ceiling allows, which is the
   //! manifest's to say.

   use crate::error::{DeclarationSite, Error, Result};
   use crate::types::{Argument, Ceiling, Command, Node, Program, Want};

   /// The command `node` runs, from the texts its declared command `expanded` to.
   ///
   /// The first argument is the program the action runs. Every other argument is kept as it is,
   /// an empty one included.
   ///
   /// # Errors
   ///
   /// Returns [`Error::NoProgram`] when `expanded` holds no argument, or when its first argument
   /// is empty or still holds a `$`, with `node` as the site. Otherwise returns
   /// [`Error::Refused`], with the program, `node` as the site and `ceiling`, when `ceiling` does
   /// not allow the program to run.
   ///
   /// Returns [`Error::InvalidName`] for a text no argument can hold. Expansion puts together
   /// texts that were each validated when they were declared, so it yields none.
   pub(crate) fn admit(expanded: Vec<String>, node: &Node, ceiling: &Ceiling) -> Result<Command> {
       let arguments = expanded
           .into_iter()
           .map(Argument::parse)
           .collect::<Result<Vec<Argument>>>()?;
       let Some(program) = arguments.first().and_then(Program::of) else {
           return Err(Error::NoProgram {
               site: Box::new(DeclarationSite::from(node)),
           });
       };
       if !ceiling.run.contains(&program) {
           return Err(Error::Refused {
               want: Want::Program(program),
               site: Some(Box::new(DeclarationSite::from(node))),
               ceiling: Box::new(ceiling.clone()),
           });
       }
       Command::new(arguments)
   }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::admission
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 371 filtered out; finished in 0.00s
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): admit an expanded command or refuse it"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- Every output lives at the output directory, the directory of its label and its name, joined by
  `/`; two outputs at one path, and an output under another, are refused with the shorter path
  and both targets, lower label first; `lib` beside `lib2` is not a conflict; a target that
  declares both outputs is named twice; of several conflicts the one with the lowest path is
  reported.
- `cc -c $in -o $out`, `cc $deps -o $out`, `$deps --filter=$opt:test_filter` and `gen $in`
  expand to `cc -c lib/a.c -o out/lib/text`, `cc out/main.o out/lib/text -o out/app`,
  `out/app --filter=Login` and `gen`; a list alone is one argument per path and none when it is
  empty; a list inside other text is joined by one space; `$input`, a lone `$` and `$opt:` with
  no name stay text; a setting's value is never expanded again.
- A command that expanded to nothing, or whose first argument is empty or still holds `$`, is
  `NoProgram`; a program outside the ceiling is `Refused` with the target's site, rendered
  `build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)`.
- `crates/expected-module-edges.txt` gains exactly `execute -> error` and `execute -> types`.
- No file under `execute/` names `crate::load`, `crate::resolve`, `crate::plan` or
  `crate::ports`.
