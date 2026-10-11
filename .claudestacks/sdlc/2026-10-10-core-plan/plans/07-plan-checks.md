---
status: approved
created: 2026-10-11
depends-on: [05]
---

# Plan Checks Implementation Plan

**Goal:** An override or a want that a plan must refuse is found from plain values, before any port is asked.

**Architecture:** The phase module `plan/` starts here, with three logic files and its test fixtures. `plan/settings.rs` gives every declared setting its value for one run — the override, or else the default — refusing an override that names no setting, and picks out the settings one command refers to. `plan/tools.rs` lists the distinct programs a graph's commands name, each with the lowest-labelled target naming it; a first argument that is empty or holds a placeholder names none. `plan/authority.rs` collects every want of a graph with the lowest-labelled target wanting it, in a private `Sites` struct, and checks the wants against a ceiling in a fixed order: programs, environment variables, inputs, then the two directories the workspace writes into. `Sites` is kept because the later steps of a plan ask for exactly those programs, inputs and variables, in that order. Every function here takes values and none takes a port.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D3, D6, D14, D15, D16), §5.2 (settings), §5.3 (authority), §5.4 (the programs a graph names), §8 (the module layout, the fixtures), §9 (the rows for `plan/settings.rs`, `plan/authority.rs` and `plan/tools.rs`).

**Checkpoints:** review once, when all three tasks are done.

---

## Context an implementer needs

What plans `01`, `03` and `05` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `TargetGraph` (`nodes()` in label order, `settings()`), `Node` (public fields), `TargetGraph::new` (`pub(crate)`) | `crates/buildl-core/src/types/target_graph.rs` | what every function here reads; the constructor the fixtures use |
| `Argument::setting_references`, `Command::arguments` | `crates/buildl-core/src/types/argument.rs` | how a command's settings are found |
| `Program::of(&Argument)` | `crates/buildl-core/src/types/program.rs` | what decides whether a first argument names a program |
| `Workspace`, `Ceiling`, `Want`, `WantedSet` | `crates/buildl-core/src/types/` | what the ceiling check reads and returns |
| `Directory::contains` | `crates/buildl-core/src/types/directory.rs` | whether a ceiling entry allows a directory |
| `Error::UnknownOverride`, `Error::Refused`, `DeclarationSite::from(&Node)` | `crates/buildl-core/src/error.rs` | the two refusals of this plan |
| `mod` lines for `load`, `pipeline`, `resolve` | `crates/buildl-core/src/lib.rs` | where `mod plan;` joins them |
| 16 lines, `pipeline -> types` followed by `ports -> error` | `crates/expected-module-edges.txt` | the golden list `cargo make dod` diffs |

Gate facts that bite here. A bullet that names a lint reports one seen as a failing run while this
code was prototyped:

- Nothing outside a file's own tests calls its functions until plan `10` writes the function
  that makes a plan, so the library build reports them as dead code and the gate fails. Each
  `mod` line in `plan/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It
  must be `cfg_attr(not(test), …)`: in the test build the functions are used, and a plain
  `expect` would fail there as unfulfilled. Plan `10` removes the attributes.
- `plan/fixtures.rs` is compiled for tests only, and a fixture no test calls is dead code in the
  test build. Each task therefore adds only the builders its own tests use.
- The module-edge guard lists `crate::<module>` imports per top-level module and diffs the list
  against `crates/expected-module-edges.txt`. The first file of `plan/` imports `crate::error` and `crate::types`, so task
  1 adds two lines to that file in the same commit. An import of a sibling is written
  `crate::plan::<file>::…`, which the guard does not count as an edge.
- `mod plan;` is private in this plan: the module exports nothing yet. Plan `10` makes it `pub`.
- clippy's `unnecessary_wraps` rejects a test helper that always returns `Some`, so the helper
  `named` in `plan/authority.rs`'s tests returns the pair and its callers wrap it.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/plan/mod.rs         — [create] module declarations
crates/buildl-core/src/plan/fixtures.rs    — [create] test-only builders: commands, settings, nodes, graphs
crates/buildl-core/src/plan/settings.rs    — [create] effective, referenced, unit tests
crates/buildl-core/src/plan/tools.rs       — [create] program_of, programs, unit tests
crates/buildl-core/src/plan/authority.rs   — [create] Sites, sites, check, unit tests
crates/buildl-core/src/lib.rs              — [modify] declare the module
crates/expected-module-edges.txt           — [modify] two edges from plan
```

### Task 1 — Give every setting its value for one run

**Files:**
- Create `crates/buildl-core/src/plan/settings.rs`
- Create `crates/buildl-core/src/plan/fixtures.rs`
- Create `crates/buildl-core/src/plan/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/expected-module-edges.txt`

**Steps:**

1. Declare the phase. In `crates/buildl-core/src/lib.rs`, directly below `pub mod pipeline;`, insert:

   ```rust
   mod plan;
   ```

2. Create `crates/buildl-core/src/plan/fixtures.rs`:

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

   use crate::types::{Argument, Command, SettingName, SettingValue};

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

3. Create `crates/buildl-core/src/plan/mod.rs`:

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
   mod settings;

   #[cfg(test)]
   mod fixtures;
   ```

4. Write the failing tests. Create `crates/buildl-core/src/plan/settings.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use super::{effective, referenced};
       use crate::error::Error;
       use crate::plan::fixtures::{command, settings};

       #[test]
       fn with_no_override_every_setting_keeps_its_default() {
           let declared = settings(&[("mode", "debug"), ("test_filter", "")]);
           assert_eq!(effective(&declared, &BTreeMap::new()).unwrap(), declared);
       }

       #[test]
       fn an_override_replaces_the_default_of_the_setting_it_names() {
           let declared = settings(&[("mode", "debug"), ("test_filter", "")]);
           let overrides = settings(&[("test_filter", "TestLogin")]);
           assert_eq!(
               effective(&declared, &overrides).unwrap(),
               settings(&[("mode", "debug"), ("test_filter", "TestLogin")])
           );
       }

       #[test]
       fn an_override_naming_no_setting_is_refused() {
           let declared = settings(&[("test_filter", "")]);
           let overrides = settings(&[("test_fliter", "TestLogin")]);
           match effective(&declared, &overrides).unwrap_err() {
               Error::UnknownOverride { name } => assert_eq!(name.as_str(), "test_fliter"),
               other => unreachable!("expected UnknownOverride, got {other:?}"),
           }
       }

       #[test]
       fn of_several_unknown_overrides_the_lowest_name_is_reported() {
           let declared = settings(&[("mode", "debug")]);
           let overrides = settings(&[("zeta", "1"), ("mode", "release"), ("alpha", "2")]);
           match effective(&declared, &overrides).unwrap_err() {
               Error::UnknownOverride { name } => assert_eq!(name.as_str(), "alpha"),
               other => unreachable!("expected UnknownOverride, got {other:?}"),
           }
       }

       #[test]
       fn only_the_settings_a_command_names_are_kept() {
           let values = settings(&[("arch", "arm"), ("mode", "debug"), ("test_filter", "T")]);
           let run = command(&["go", "test", "--run=$opt:test_filter", "-tags=$opt:mode"]);
           assert_eq!(
               referenced(&run, &values),
               settings(&[("mode", "debug"), ("test_filter", "T")])
           );
           assert!(referenced(&command(&["cc", "$in"]), &values).is_empty());
       }

       #[test]
       fn a_setting_named_twice_is_kept_once() {
           let values = settings(&[("mode", "debug")]);
           let run = command(&["tool", "$opt:mode", "--again=$opt:mode"]);
           assert_eq!(referenced(&run, &values), settings(&[("mode", "debug")]));
       }

       #[test]
       fn a_reference_to_a_setting_without_a_value_is_left_out() {
           let values = settings(&[("mode", "debug")]);
           let long = format!("$opt:{}", "a".repeat(257));
           let run = command(&["tool", "$opt:other", long.as_str(), "$opt:mode"]);
           assert_eq!(referenced(&run, &values), settings(&[("mode", "debug")]));
       }
   }
   ```

5. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::settings
   error[E0432]: unresolved imports `super::effective`, `super::referenced`
   ```

6. Replace the placeholder line at the top of `crates/buildl-core/src/plan/settings.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Build settings for one run: the value each has, and which of them a command refers to.
   //!
   //! Its own file because a setting reaches a key in two steps that have nothing else in common
   //! with the rest of the phase: an override replaces a default, and a target keys only on the
   //! settings its own command names.
   //!
   //! Responsibilities: [`effective`] and [`referenced`].
   //!
   //! Non-responsibilities: whether a command refers to a setting that is declared. The graph
   //! guarantees it does.

   use std::collections::BTreeMap;

   use crate::error::{Error, Result};
   use crate::types::{Argument, Command, SettingName, SettingValue};

   /// The value every declared setting has for this run: its override, or else its default.
   ///
   /// # Errors
   ///
   /// Returns [`Error::UnknownOverride`] for an override that names no declared setting; when
   /// several do, the one whose name sorts first.
   pub(crate) fn effective(
       declared: &BTreeMap<SettingName, SettingValue>,
       overrides: &BTreeMap<SettingName, SettingValue>,
   ) -> Result<BTreeMap<SettingName, SettingValue>> {
       if let Some(name) = overrides.keys().find(|name| !declared.contains_key(*name)) {
           return Err(Error::UnknownOverride { name: name.clone() });
       }
       let mut values = declared.clone();
       values.extend(
           overrides
               .iter()
               .map(|(name, value)| (name.clone(), value.clone())),
       );
       Ok(values)
   }

   /// The settings `command` refers to, each with its value from `values`.
   ///
   /// Only these reach a target's key, so changing a setting rebuilds exactly the targets whose
   /// commands name it. A reference to a setting `values` does not hold is left out.
   pub(crate) fn referenced(
       command: &Command,
       values: &BTreeMap<SettingName, SettingValue>,
   ) -> BTreeMap<SettingName, SettingValue> {
       command
           .arguments()
           .iter()
           .flat_map(Argument::setting_references)
           .filter_map(|name| SettingName::parse(name).ok())
           .filter_map(|name| values.get(&name).map(|value| (name, value.clone())))
           .collect()
   }
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::settings
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 260 filtered out
   ```

8. Record the two module edges the new file adds. In `crates/expected-module-edges.txt`, replace:

   ```
   pipeline -> types
   ports -> error
   ```

   With:

   ```
   pipeline -> types
   plan -> error
   plan -> types
   ports -> error
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

10. Commit:

   ```
   $ git add crates/buildl-core/src/plan/settings.rs crates/buildl-core/src/plan/mod.rs crates/buildl-core/src/plan/fixtures.rs crates/buildl-core/src/lib.rs crates/expected-module-edges.txt
   $ git commit -m "feat(buildl-core): give build settings their value for one run"
   ```

### Task 2 — List the programs a graph's commands name

**Files:**
- Create `crates/buildl-core/src/plan/tools.rs`
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

2. Add the label, node and graph builders. `crates/buildl-core/src/plan/fixtures.rs` becomes:

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
       Argument, Command, Freshness, Label, NetworkAccess, Node, NodeId, OutputName, Provenance,
       SettingName, SettingValue, TargetGraph, TargetRole,
   };

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

3. Write the failing tests. Create `crates/buildl-core/src/plan/tools.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

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
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::tools
   error[E0432]: unresolved imports `super::program_of`, `super::programs`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/plan/tools.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The programs a graph's commands name.
   //!
   //! Its own file because a program is the one thing a command names that exists outside the
   //! workspace: it has to be allowed before anything runs, it has to be found, and what was found
   //! is part of every key of every target that runs it.
   //!
   //! Responsibilities: [`program_of`] and [`programs`].
   //!
   //! Non-responsibilities: whether a program may run, which is decided against the ceiling.

   use std::collections::BTreeMap;

   use crate::types::{Command, Node, Program, TargetGraph};

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
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::tools
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 267 filtered out
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
   $ git add crates/buildl-core/src/plan/tools.rs crates/buildl-core/src/plan/mod.rs crates/buildl-core/src/plan/fixtures.rs
   $ git commit -m "feat(buildl-core): list the programs a graph's commands name"
   ```

### Task 3 — Check what a graph wants against the ceiling

**Files:**
- Create `crates/buildl-core/src/plan/authority.rs`
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

2. Add four builders for names. `crates/buildl-core/src/plan/fixtures.rs` becomes:

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
       Argument, Command, Directory, EnvName, Freshness, Label, NetworkAccess, Node, NodeId,
       OutputName, Program, Provenance, SettingName, SettingValue, SourcePath, TargetGraph,
       TargetRole,
   };

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

3. Write the failing tests. Create `crates/buildl-core/src/plan/authority.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeSet;
       use std::path::Path;

       use super::{check, holds, sites};
       use crate::error::{DeclarationSite, Error};
       use crate::plan::fixtures::{command, dir, env_name, graph, label, node, path, program};
       use crate::types::{
           Ceiling, Directory, NetworkAccess, Node, TargetGraph, Want, WantedSet, Workspace,
       };

       /// A workspace writing to `out` and `.buildl`, whose ceiling allows `cc` and `ld`, `HOME`
       /// and `PATH`, reads under `src` and `lib`, and writes to its own two directories.
       fn workspace() -> Workspace {
           Workspace {
               out: dir("out"),
               state: dir(".buildl"),
               ceiling: Ceiling {
                   run: BTreeSet::from([program("cc"), program("ld")]),
                   env: BTreeSet::from([env_name("HOME"), env_name("PATH")]),
                   read: BTreeSet::from([dir("src"), dir("lib")]),
                   write: BTreeSet::from([dir("out"), dir(".buildl")]),
               },
           }
       }

       /// `//:app` linking with `ld`, reading `PATH` and `src/main.c`.
       fn app() -> Node {
           node("//:app", |node| {
               node.run = command(&["ld", "$in"]);
               node.env = vec![env_name("PATH")];
               node.inputs = vec![path("src/main.c")];
           })
       }

       /// `//lib:text` compiling with `cc`, reading `HOME`, `PATH` and `lib/text.c`, on the network.
       fn text() -> Node {
           node("//lib:text", |node| {
               node.env = vec![env_name("PATH"), env_name("HOME")];
               node.inputs = vec![path("lib/text.c")];
               node.network = NetworkAccess::Declared;
           })
       }

       fn checked(graph: &TargetGraph, workspace: &Workspace) -> crate::error::Result<WantedSet> {
           check(graph, &sites(graph), workspace)
       }

       /// What `graph` is refused under `workspace`: the want, and the label and build file named.
       fn refusal(graph: &TargetGraph, workspace: &Workspace) -> (Want, Option<(String, String)>) {
           match checked(graph, workspace).unwrap_err() {
               Error::Refused {
                   want,
                   site,
                   ceiling,
               } => {
                   assert_eq!(*ceiling, workspace.ceiling);
                   let named = site.map(|site| {
                       let DeclarationSite { label, provenance } = *site;
                       (label.to_string(), provenance.to_string())
                   });
                   (want, named)
               }
               other => unreachable!("expected Refused, got {other:?}"),
           }
       }

       fn named(label: &str, file: &str) -> (String, String) {
           (label.to_owned(), file.to_owned())
       }

       #[test]
       fn sites_name_the_lowest_labelled_target_for_each_want() {
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           let sites = sites(&graph);
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
       fn a_build_inside_the_ceiling_reports_what_it_wants() {
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           assert_eq!(
               checked(&graph, &workspace()).unwrap(),
               WantedSet {
                   programs: BTreeSet::from([program("cc"), program("ld")]),
                   env: BTreeSet::from([env_name("HOME"), env_name("PATH")]),
                   write: BTreeSet::from([dir("out"), dir(".buildl")]),
                   network: BTreeSet::from([label("//lib:text")]),
               }
           );
       }

       #[test]
       fn a_program_outside_the_ceiling_is_refused_with_its_site() {
           let mut workspace = workspace();
           workspace.ceiling.run = BTreeSet::from([program("ld")]);
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           assert_eq!(
               refusal(&graph, &workspace),
               (
                   Want::Program(program("cc")),
                   Some(named("//lib:text", "lib/build.lua"))
               )
           );
       }

       #[test]
       fn a_variable_outside_the_ceiling_is_refused_with_its_site() {
           let mut workspace = workspace();
           workspace.ceiling.env = BTreeSet::from([env_name("PATH")]);
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           assert_eq!(
               refusal(&graph, &workspace),
               (
                   Want::Env(env_name("HOME")),
                   Some(named("//lib:text", "lib/build.lua"))
               )
           );
       }

       #[test]
       fn an_input_outside_the_ceiling_is_refused_with_its_site() {
           let mut workspace = workspace();
           workspace.ceiling.read = BTreeSet::from([dir("lib")]);
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           assert_eq!(
               refusal(&graph, &workspace),
               (
                   Want::Read(path("src/main.c")),
                   Some(named("//:app", "build.lua"))
               )
           );
       }

       #[test]
       fn a_directory_written_outside_the_ceiling_is_refused_without_a_site() {
           let graph = graph(vec![app()], &[&[]]);
           let mut no_out = workspace();
           no_out.ceiling.write = BTreeSet::from([dir(".buildl")]);
           assert_eq!(refusal(&graph, &no_out), (Want::Write(dir("out")), None));
           let mut no_state = workspace();
           no_state.ceiling.write = BTreeSet::from([dir("out")]);
           assert_eq!(
               refusal(&graph, &no_state),
               (Want::Write(dir(".buildl")), None)
           );
       }

       #[test]
       fn kinds_are_checked_in_a_fixed_order() {
           let graph = graph(vec![app(), text()], &[&[], &[]]);
           let mut workspace = workspace();
           workspace.ceiling = Ceiling {
               run: BTreeSet::new(),
               env: BTreeSet::new(),
               read: BTreeSet::new(),
               write: BTreeSet::new(),
           };
           assert_eq!(refusal(&graph, &workspace).0, Want::Program(program("cc")));
           workspace.ceiling.run = BTreeSet::from([program("cc"), program("ld")]);
           assert_eq!(refusal(&graph, &workspace).0, Want::Env(env_name("HOME")));
           workspace.ceiling.env = BTreeSet::from([env_name("HOME"), env_name("PATH")]);
           assert_eq!(
               refusal(&graph, &workspace).0,
               Want::Read(path("lib/text.c"))
           );
           workspace.ceiling.read = BTreeSet::from([Directory::root()]);
           assert_eq!(refusal(&graph, &workspace).0, Want::Write(dir(".buildl")));
           workspace.ceiling.write = BTreeSet::from([Directory::root()]);
           assert!(checked(&graph, &workspace).is_ok());
       }

       #[test]
       fn the_lowest_labelled_target_wanting_a_thing_is_the_one_named() {
           let graph = graph(
               vec![
                   node("//:a", |node| node.run = command(&["docker", "build"])),
                   node("//:b", |node| node.run = command(&["docker", "push"])),
               ],
               &[&[], &[]],
           );
           assert_eq!(
               refusal(&graph, &workspace()),
               (
                   Want::Program(program("docker")),
                   Some(named("//:a", "build.lua"))
               )
           );
       }

       #[test]
       fn an_empty_ceiling_refuses_even_a_build_that_asks_for_nothing_but_its_own_directories() {
           let mut workspace = workspace();
           workspace.ceiling = Ceiling {
               run: BTreeSet::new(),
               env: BTreeSet::new(),
               read: BTreeSet::new(),
               write: BTreeSet::new(),
           };
           let graph = graph(
               vec![node("//:gen", |node| node.run = command(&["$out/gen"]))],
               &[&[]],
           );
           assert_eq!(
               refusal(&graph, &workspace),
               (Want::Write(dir(".buildl")), None)
           );
       }

       #[test]
       fn an_input_is_held_by_its_directory_and_not_by_one_sharing_a_prefix() {
           assert!(holds(&dir("lib"), &path("lib/x")));
           assert!(holds(&dir("lib"), &path("lib/text/x")));
           assert!(!holds(&dir("lib"), &path("lib2/x")));
           assert!(!holds(&dir("lib/text"), &path("lib/x")));
           assert!(!holds(&dir("lib"), &path("lib")));
           assert!(holds(&Directory::root(), &path("anything/at/all")));
           assert!(holds(&Directory::root(), &path("top")));
       }

       #[test]
       fn an_input_under_a_directory_sharing_a_prefix_with_an_allowed_one_is_refused() {
           let graph = graph(
               vec![node("//lib2:x", |node| node.inputs = vec![path("lib2/x")])],
               &[&[]],
           );
           assert_eq!(
               refusal(&graph, &workspace()),
               (
                   Want::Read(path("lib2/x")),
                   Some(named("//lib2:x", "lib2/build.lua"))
               )
           );
       }

       #[test]
       fn a_directory_is_allowed_by_an_entry_at_or_above_it() {
           let graph = graph(vec![app()], &[&[]]);
           let mut workspace = workspace();
           workspace.out = dir("build/out");
           workspace.ceiling.write = BTreeSet::from([dir("build"), dir(".buildl")]);
           assert!(checked(&graph, &workspace).is_ok());
           workspace.ceiling.write = BTreeSet::from([dir("build/out/deep"), dir(".buildl")]);
           assert_eq!(
               refusal(&graph, &workspace),
               (Want::Write(dir("build/out")), None)
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::authority
   error[E0432]: unresolved imports `super::check`, `super::holds`, `super::sites`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/plan/authority.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Authority: what a graph's targets want, and whether the workspace's ceiling allows it.
   //!
   //! Its own file because this is the check that runs before anything else is asked of the
   //! machine: a build that wants more than its workspace allows is refused here, with nothing
   //! hashed, looked up or read.
   //!
   //! Responsibilities: [`Sites`], [`sites`] and [`check`].
   //!
   //! Non-responsibilities: what a ceiling is, which is the manifest's to say, and finding the
   //! programs a build may run.

   use std::collections::{BTreeMap, BTreeSet};

   use crate::error::{DeclarationSite, Error, Result};
   use crate::plan::tools::programs;
   use crate::types::{
       Directory, EnvName, NetworkAccess, Node, Program, SourcePath, TargetGraph, Want, WantedSet,
       Workspace,
   };

   /// Everything a graph's targets want, each with the lowest-labelled target wanting it.
   ///
   /// Each map is in the order of its keys, which is the order its wants are checked and later
   /// asked for in.
   #[derive(Debug)]
   pub(crate) struct Sites<'g> {
       /// The programs commands name.
       pub(crate) programs: BTreeMap<Program, &'g Node>,
       /// The environment variables targets read.
       pub(crate) env: BTreeMap<&'g EnvName, &'g Node>,
       /// The workspace files targets read.
       pub(crate) inputs: BTreeMap<&'g SourcePath, &'g Node>,
   }

   /// Collects what `graph`'s targets want.
   pub(crate) fn sites(graph: &TargetGraph) -> Sites<'_> {
       let mut env = BTreeMap::new();
       let mut inputs = BTreeMap::new();
       for node in graph.nodes() {
           for name in &node.env {
               env.entry(name).or_insert(node);
           }
           for path in &node.inputs {
               inputs.entry(path).or_insert(node);
           }
       }
       Sites {
           programs: programs(graph),
           env,
           inputs,
       }
   }

   /// Checks every want against `workspace`'s ceiling and returns what the build wants.
   ///
   /// The directories a build writes into are the workspace's own two, `out` and `state`. A target
   /// declared to reach the network is listed in the result and compared with nothing: a ceiling
   /// has no entry for the network.
   ///
   /// # Errors
   ///
   /// Returns [`Error::Refused`] for the first want outside the ceiling. Programs are checked
   /// first, then environment variables, then inputs, then the directories written into; within
   /// each kind the lowest want first. The refusal names the lowest-labelled target wanting it,
   /// except for a directory, which no target declares.
   pub(crate) fn check(
       graph: &TargetGraph,
       sites: &Sites<'_>,
       workspace: &Workspace,
   ) -> Result<WantedSet> {
       let ceiling = &workspace.ceiling;
       let refused = |want: Want, node: Option<&Node>| Error::Refused {
           want,
           site: node.map(|node| Box::new(DeclarationSite::from(node))),
           ceiling: Box::new(ceiling.clone()),
       };

       let outside = sites
           .programs
           .iter()
           .find(|(program, _)| !ceiling.run.contains(*program));
       if let Some((program, node)) = outside {
           return Err(refused(Want::Program(program.clone()), Some(node)));
       }

       let outside = sites
           .env
           .iter()
           .find(|(name, _)| !ceiling.env.contains(**name));
       if let Some((name, node)) = outside {
           return Err(refused(Want::Env((*name).clone()), Some(node)));
       }

       let outside = sites
           .inputs
           .iter()
           .find(|(path, _)| !ceiling.read.iter().any(|allowed| holds(allowed, path)));
       if let Some((path, node)) = outside {
           return Err(refused(Want::Read((*path).clone()), Some(node)));
       }

       let write = BTreeSet::from([workspace.out.clone(), workspace.state.clone()]);
       let outside = write.iter().find(|directory| {
           !ceiling
               .write
               .iter()
               .any(|allowed| allowed.contains(directory))
       });
       if let Some(directory) = outside {
           return Err(refused(Want::Write(directory.clone()), None));
       }

       Ok(WantedSet {
           programs: sites.programs.keys().cloned().collect(),
           env: sites.env.keys().map(|name| (*name).clone()).collect(),
           write,
           network: graph
               .nodes()
               .iter()
               .filter(|node| node.network == NetworkAccess::Declared)
               .map(|node| node.label.clone())
               .collect(),
       })
   }

   /// Whether the file `path` lies in `directory` or below it.
   fn holds(directory: &Directory, path: &SourcePath) -> bool {
       directory.is_root()
           || path
               .as_str()
               .strip_prefix(directory.as_str())
               .is_some_and(|rest| rest.starts_with('/'))
   }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::authority
   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 271 filtered out
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
   $ git add crates/buildl-core/src/plan/authority.rs crates/buildl-core/src/plan/mod.rs crates/buildl-core/src/plan/fixtures.rs
   $ git commit -m "feat(buildl-core): check what a build wants against the ceiling"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- An override naming no setting is refused, the lowest name first; a command keeps exactly the
  settings it names, once each.
- The distinct programs of a graph are listed in program order, each with its lowest-labelled
  target; an empty or placeholder first argument names none.
- Every kind of want is checked inside and outside a ceiling; kinds are checked in the order
  program, variable, input, directory; the lowest-labelled target wanting a thing is the one
  named; a directory written into has no site; an empty ceiling refuses; `lib2/x` is outside a
  ceiling of `lib`.
- `crates/expected-module-edges.txt` gains exactly `plan -> error` and `plan -> types`.
- No file under `plan/` names `crate::load`, `crate::resolve` or `crate::ports`.
