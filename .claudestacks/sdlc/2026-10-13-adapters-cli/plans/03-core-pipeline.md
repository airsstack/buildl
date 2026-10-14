---
status: done
created: 2026-10-14
depends-on: [02]
---

# Core Pipeline Implementation Plan

**Goal:** The pipeline of `buildl-core` runs a build the way a command line asks for it: scoped, recorded, reported while it runs.

**Architecture:** `resolve::narrow(graph, scope)` answers a `Scope` with a graph like any other: the whole one borrowed, or the named targets with everything they depend on, renumbered in label order, so nothing after it knows whether it was handed a workspace or a part of one. `Pipeline::plan` and `Pipeline::build` take the scope and narrow between Resolve and Plan; `build` first hands the whole graph to the `GraphRecord` port, and `execute` takes both graphs, so two outputs claiming one place are refused across the workspace whatever was asked for. `pipeline::check` and `pipeline::graph` are the bodies of the two methods that reach nothing but build files, as functions over a `DeclarationSource` alone. Execute reports through the `Reporter` port, always on the thread that called it. A worker says `Told::Began` through the dispatcher as it begins an action, and the scheduling thread turns that into `started`. Every outcome is logged first and reported after, with what its action printed read back from the content store by `record::shown`, so a cached target shows what it printed when it last ran. `begun` is called once nothing can refuse the build any more, and `summary` last, on success only. Under `LiveOutput::OneAction` one `AtomicBool` is the place of the one action shown live: the worker that takes it passes a sink to `ExecStrategy::run`, each run of bytes travels as `Told::Printed` to `printed`, that target's outcome is reported with nothing left to show, the events of targets that end meanwhile are held and reported after it, and only then does the scheduling thread give the place back. What is stored and returned is the same with or without it. Each task is one behaviour through every layer it needs, because a crate-private item nothing uses fails the gate as dead code.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`, `cargo-deny`. No new dependency.

**Content authority:** spec §2.5 (the scheduling side), §2.6 (narrowing), §2.7 (`Pipeline`, `execute`, the free functions, the error order of `build`), §2.10 (the files; the status text and the module table of `lib.rs`), §2.11 (the thirteen flows of `tests/flows/report.rs`, the two of `check.rs`, the unit tests of `narrow.rs` and `record.rs`).

---

## Context an implementer needs

What plans `01` and `02` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Scope::{Workspace, Targets(BTreeSet<Label>)}`, `LiveOutput::{Off, OneAction}` | `crates/buildl-core/src/types/{scope,live_output}.rs` | the two new arguments of a run |
| `Turn<M, R>`, `Channel`, `LiveSink<'_>`, `Captured<'_>` | `crates/buildl-core/src/types/` | what crosses the dispatcher, the strategy and the reporter |
| `Error::UnknownTarget { label, suggestion }` | `crates/buildl-core/src/error.rs` | what `narrow` refuses a label with |
| `TargetGraph::new(nodes, edges, aliases, settings)` (crate-private), `id_of(&Label) -> Option<NodeId>` (a target or an alias), `deps(NodeId) -> Option<&[NodeId]>`, `node`, `nodes`, `aliases`, `settings`, `len`, `is_empty` | `crates/buildl-core/src/types/target_graph.rs` | what `narrow` reads a graph with and builds one from |
| `nearest(wanted, candidates) -> Option<&T>` (crate-private) | `crates/buildl-core/src/resolve/suggest.rs` | the suggestion of `UnknownTarget` |
| the test builders `target`, `alias`, `setting`, `label` | `crates/buildl-core/src/resolve/fixtures.rs` | the graph of `narrow`'s unit tests |
| `Dispatcher::dispatch(work, step)` with `work: Fn(T, &(dyn Fn(M) + Sync)) -> R` and `step: FnMut(Turn<M, R>) -> Vec<T>` | `crates/buildl-core/src/ports/dispatcher.rs` | how a worker tells the scheduling thread something |
| `ExecStrategy::run(dir, action, live: Option<LiveSink<'_>>)` | `crates/buildl-core/src/ports/exec_strategy.rs` | where a worker hands the live sink |
| `Reporter` with `begun`, `started`, `printed`, `finished`, `summary`; `GraphRecord::record` | `crates/buildl-core/src/ports/{reporter,graph_record}.rs` | the two ports this plan starts calling |
| `PortSet` with `reporter` and `graph` | `crates/buildl-core/src/ports/port_set.rs` | where `execute` and `Pipeline::build` find them |
| `execute(graph, plan, on_failure, ports)`, the private `Build` with `step(Turn<(), Worked>)`, `record`, `finish`, `skip`, `conclude`, `outcomes` | `crates/buildl-core/src/execute/assembly.rs` | what tasks 5 to 9 change |
| `perform(task, strategy, cas, clock, stop)`, `Task`, `Worked`, `Performed` | `crates/buildl-core/src/execute/perform.rs` | what a worker runs; tasks 7 and 8 change it |
| `append(log, clock, provenance, command, progress, outcome) -> Result<()>` | `crates/buildl-core/src/execute/record.rs` | what task 6 has return the event it logged |
| `Pipeline::{check, graph, plan, build}` and the private `first_difference` | `crates/buildl-core/src/pipeline/driver.rs` | what tasks 2 to 5, 8 and 10 change |
| `FakeReporter` with `calls: Vec<Reported>`, `FakeGraphRecord` with `recorded` and `broken`, `FakeDispatcher::pace`, `Ends::Signaled`, `Script`, `FakeStrategy::ran()`, `FakeTools::lookups()`, `FakeLog::{events, broken}`, `FakeCas::{digests, blob}`, the builders `file`, `ports`, `program`, `target_with`, `workspace` | `crates/buildl-core/tests/flows/common.rs` | what the flows of `tests/flows/report.rs` are written against |

`grep -rn 'execute(\|\.plan(\|\.build(' --include='*.rs' crates/buildl-core/src crates/buildl crates/buildl-cli crates/buildl-lua` finds one call of `execute`, in `pipeline/driver.rs`, and no call of `Pipeline::plan` or `Pipeline::build` outside the flows (the `.build()` it finds in `crates/buildl-lua` are builders of the Lua engine). So a changed signature changes `driver.rs` and the flows, and no other crate.

Facts of the gate that bite here:

- The workspace denies warnings, dead code included. A `pub(crate)` function, a variant or a field that nothing but a test uses fails `cargo make dod`, so no task adds one ahead of its use: each task goes through every layer its behaviour needs.
- Clippy's `needless_pass_by_value` refuses a function that takes a value and only reads it. That is why task 7's `told` and task 8's `show` take a reference, and why each takes its argument by value from the task in which it starts to keep part of it.
- `guard-module-edges` compares the crate's module-to-module imports with `crates/expected-module-edges.txt`. Every import this plan adds is on that list already (`resolve -> error`, `resolve -> types`, `execute -> ports`, `execute -> types`, `execute -> error`, `pipeline -> ports`, `pipeline -> resolve`, `pipeline -> types`), so the file does not change. An import is always spelled `crate::<module>::<item>`.
- `resolve/mod.rs` declares a module named `narrow` and re-exports a function named `narrow`, so a rustdoc link to the function is written ``[`narrow`](fn@narrow)``; a bare ``[`narrow`]`` is ambiguous and fails the doc step.
- `cargo fmt --all` sorts and rewraps a `use` list and joins a call onto one line when it fits. A step that adds a name or an argument runs it; the blocks below show the result.
- `Pipeline::build` gains its two arguments in two tasks, the scope in task 4 and the live-output choice in task 8, because each needs its behaviour to be a real argument. Task 8 therefore adds an argument to calls that tasks 4 to 7 wrote; its steps list every one.
- The golden files under `tests/flows/golden/` do not change: a narrowed graph and a reported build serialize what they did.
- A test added in one task may hold an assertion that only a later task makes meaningful; where that is so the step says it.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes, in a copy of the workspace walked through these tasks in order; a failing run is quoted by the lines that name the cause, not in full, and `…` in a quoted line stands for the rest of a long value. `cargo make dod` was run after every task and exited `0` with the counts each gate step gives. `cargo deny check` was run on the tree plan `01` starts from and on the tree this plan leaves, and printed `advisories ok, bans ok, licenses ok, sources ok` both times; no task of the three plans changes a manifest or the lockfile.

Before task 1 the three `buildl-core` results of `cargo make dod` read `544 passed` (unit tests), `114 passed` (flows) and `13 passed` (doctests).

## File structure

```
crates/buildl-core/src/resolve/narrow.rs      — [create] narrow and its tests
crates/buildl-core/src/resolve/mod.rs         — [modify] declare and re-export narrow
crates/buildl-core/src/pipeline/driver.rs     — [modify] check and graph as functions; the scope, the live-output choice and the graph record in plan and build
crates/buildl-core/src/pipeline/mod.rs        — [modify] re-export check and graph
crates/buildl-core/src/execute/assembly.rs    — [modify] the whole graph; the reporter's calls; the action shown live and the outcomes held for it
crates/buildl-core/src/execute/perform.rs     — [modify] what a worker says; the place of the action shown live
crates/buildl-core/src/execute/record.rs      — [modify] append returns its event; shown and its tests
crates/buildl-core/src/lib.rs                 — [modify] re-export narrow; the module table and the status text
crates/buildl-core/tests/flows/report.rs      — [create] the thirteen flows of a scoped, recorded, reported build
crates/buildl-core/tests/flows/main.rs        — [modify] declare the report flows
crates/buildl-core/tests/flows/check.rs       — [modify] two flows for the functions over a source alone
crates/buildl-core/tests/flows/plan.rs        — [modify] every plan names its scope
crates/buildl-core/tests/flows/build.rs       — [modify] every build names its scope and its live-output choice
```

### Task 1 — Narrow a graph to the targets a run was asked for

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/resolve/mod.rs`
- Create `crates/buildl-core/src/resolve/narrow.rs`

**Steps:**

1. Write the failing tests: the module's documentation and its test module, with the function still to come. Create `crates/buildl-core/src/resolve/narrow.rs`:

   ```rust
   //! Narrowing: from a workspace's whole graph to the part one run was asked for.
   //!
   //! Its own file because it is the one step of this phase that reads a value of the invocation:
   //! which targets were asked for. It answers with a graph like any other, so nothing after it
   //! knows whether it was handed a whole workspace or a part of one.
   //!
   //! Responsibilities: [`narrow`].
   //!
   //! Non-responsibilities: reading build files and checking references, which happen for the
   //! whole workspace before a graph exists; and deciding whether a target needs to run.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeSet;

       use super::narrow;
       use crate::error::Error;
       use crate::resolve::assembly::resolve;
       use crate::resolve::fixtures::{alias, label, setting, target};
       use crate::types::{NodeId, Scope, TargetGraph};

       /// `//:a` depends on `//:b`, `//:b` on `//:c`; `//:z` stands alone; `//:all` names `//:a`.
       fn graph() -> TargetGraph {
           resolve(vec![
               target("//:a", &["//:b"]),
               target("//:b", &["//:c"]),
               target("//:c", &[]),
               target("//:z", &[]),
               alias("//:all", "//:a"),
               setting("mode"),
           ])
           .unwrap()
       }

       fn only(labels: &[&str]) -> Scope {
           Scope::Targets(labels.iter().map(|raw| label(raw)).collect())
       }

       fn labels(graph: &TargetGraph) -> Vec<String> {
           graph
               .nodes()
               .iter()
               .map(|node| node.label.to_string())
               .collect()
       }

       #[test]
       fn the_whole_workspace_is_the_graph_itself() {
           let graph = graph();
           let narrowed = narrow(&graph, &Scope::Workspace).unwrap();
           assert_eq!(*narrowed, graph);
       }

       #[test]
       fn a_target_brings_everything_it_depends_on_and_nothing_else() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:b"])).unwrap();
           assert_eq!(labels(&narrowed), ["//:b", "//:c"]);
           assert_eq!(narrowed.deps(NodeId::new(0)), Some(&[NodeId::new(1)][..]));
           assert!(narrowed.aliases().is_empty());
       }

       #[test]
       fn an_alias_names_its_target_and_is_kept_with_it() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:all"])).unwrap();
           assert_eq!(labels(&narrowed), ["//:a", "//:b", "//:c"]);
           assert_eq!(narrowed.id_of(&label("//:all")), Some(NodeId::new(0)));
       }

       #[test]
       fn asking_for_every_target_gives_back_an_equal_graph() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:a", "//:z"])).unwrap();
           assert_eq!(*narrowed, graph);
       }

       #[test]
       fn no_label_is_no_target() {
           let graph = graph();
           let narrowed = narrow(&graph, &Scope::Targets(BTreeSet::new())).unwrap();
           assert!(narrowed.is_empty());
       }

       #[test]
       fn a_label_naming_nothing_is_refused_with_the_nearest_one() {
           let graph = graph();
           let error = narrow(&graph, &only(&["//:al"])).unwrap_err();
           match error {
               Error::UnknownTarget {
                   label: asked,
                   suggestion,
               } => {
                   assert_eq!(asked, label("//:al"));
                   assert_eq!(suggestion, Some(label("//:a")));
               }
               other => unreachable!("expected UnknownTarget, got {other:?}"),
           }
       }
   }
   ```

2. Declare the module, so the tests are compiled. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: [`resolve`].
   //!
   //! Non-responsibilities: evaluating build files and validating one declaration on its own,
   //! which the phase before this one does; and anything about whether a target needs to run.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod assembly;
   mod cycle;
   mod index;
   pub mod narrow;
   mod references;
   mod suggest;

   #[cfg(test)]
   mod fixtures;

   pub use assembly::resolve;
   ```

3. Run the tests and confirm they fail to build, because there is no `narrow`:

   ```
   $ cargo test -p buildl-core --lib resolve::narrow
   error[E0432]: unresolved import `super::narrow`
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

4. Write the function. It marks what is wanted with an explicit stack, numbers the kept targets again in label order, which is the order they already stand in, keeps each one's dependencies in written order and every alias whose target is kept, and keeps every setting: a setting belongs to the workspace, not to a target. `crates/buildl-core/src/resolve/narrow.rs` becomes:

   ```rust
   //! Narrowing: from a workspace's whole graph to the part one run was asked for.
   //!
   //! Its own file because it is the one step of this phase that reads a value of the invocation:
   //! which targets were asked for. It answers with a graph like any other, so nothing after it
   //! knows whether it was handed a whole workspace or a part of one.
   //!
   //! Responsibilities: [`narrow`].
   //!
   //! Non-responsibilities: reading build files and checking references, which happen for the
   //! whole workspace before a graph exists; and deciding whether a target needs to run.

   use std::borrow::Cow;
   use std::collections::BTreeMap;

   use crate::error::{Error, Result};
   use crate::resolve::suggest::nearest;
   use crate::types::{Label, NodeId, Scope, TargetGraph};

   /// The graph of what `scope` asks for: `graph` itself for [`Scope::Workspace`], and otherwise
   /// the targets its labels name, each directly or as an alias, with everything they depend on.
   ///
   /// The narrowed graph is a graph like any other: its targets are numbered in label order, each
   /// keeps its dependencies in the order they were written, an alias is kept when the target it
   /// names is, and every build setting is kept, since a setting belongs to the workspace and not
   /// to a target. A target outside it is not part of the run at all.
   ///
   /// # Errors
   ///
   /// Returns [`Error::UnknownTarget`] for the first label of `scope`, in label order, that names
   /// neither a target nor an alias of `graph`.
   pub fn narrow<'g>(graph: &'g TargetGraph, scope: &Scope) -> Result<Cow<'g, TargetGraph>> {
       let Scope::Targets(labels) = scope else {
           return Ok(Cow::Borrowed(graph));
       };
       let mut wanted = vec![false; graph.len()];
       let mut stack = labels
           .iter()
           .map(|label| graph.id_of(label).ok_or_else(|| unknown(graph, label)))
           .collect::<Result<Vec<NodeId>>>()?;
       while let Some(id) = stack.pop() {
           if let Some(seen) = wanted.get_mut(id.index())
               && !*seen
           {
               *seen = true;
               stack.extend(graph.deps(id).unwrap_or_default());
           }
       }

       let renumbered: BTreeMap<NodeId, NodeId> = (0_u32..)
           .map(NodeId::new)
           .zip(&wanted)
           .filter(|(_, wanted)| **wanted)
           .map(|(id, _)| id)
           .zip((0_u32..).map(NodeId::new))
           .collect();
       let nodes = renumbered
           .keys()
           .filter_map(|id| graph.node(*id))
           .cloned()
           .collect();
       let edges = renumbered
           .keys()
           .map(|id| {
               graph
                   .deps(*id)
                   .unwrap_or_default()
                   .iter()
                   .filter_map(|dep| renumbered.get(dep))
                   .copied()
                   .collect()
           })
           .collect();
       let aliases = graph
           .aliases()
           .iter()
           .filter_map(|(alias, id)| Some((alias.clone(), *renumbered.get(id)?)))
           .collect();
       Ok(Cow::Owned(TargetGraph::new(
           nodes,
           edges,
           aliases,
           graph.settings().clone(),
       )))
   }

   /// The error for `label`, which names nothing in `graph`, with the nearest label that does.
   fn unknown(graph: &TargetGraph, label: &Label) -> Error {
       let declared = graph
           .nodes()
           .iter()
           .map(|node| &node.label)
           .chain(graph.aliases().keys());
       Error::UnknownTarget {
           label: label.clone(),
           suggestion: nearest(&label.to_string(), declared).cloned(),
       }
   }

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeSet;

       use super::narrow;
       use crate::error::Error;
       use crate::resolve::assembly::resolve;
       use crate::resolve::fixtures::{alias, label, setting, target};
       use crate::types::{NodeId, Scope, TargetGraph};

       /// `//:a` depends on `//:b`, `//:b` on `//:c`; `//:z` stands alone; `//:all` names `//:a`.
       fn graph() -> TargetGraph {
           resolve(vec![
               target("//:a", &["//:b"]),
               target("//:b", &["//:c"]),
               target("//:c", &[]),
               target("//:z", &[]),
               alias("//:all", "//:a"),
               setting("mode"),
           ])
           .unwrap()
       }

       fn only(labels: &[&str]) -> Scope {
           Scope::Targets(labels.iter().map(|raw| label(raw)).collect())
       }

       fn labels(graph: &TargetGraph) -> Vec<String> {
           graph
               .nodes()
               .iter()
               .map(|node| node.label.to_string())
               .collect()
       }

       #[test]
       fn the_whole_workspace_is_the_graph_itself() {
           let graph = graph();
           let narrowed = narrow(&graph, &Scope::Workspace).unwrap();
           assert_eq!(*narrowed, graph);
       }

       #[test]
       fn a_target_brings_everything_it_depends_on_and_nothing_else() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:b"])).unwrap();
           assert_eq!(labels(&narrowed), ["//:b", "//:c"]);
           assert_eq!(narrowed.deps(NodeId::new(0)), Some(&[NodeId::new(1)][..]));
           assert!(narrowed.aliases().is_empty());
       }

       #[test]
       fn an_alias_names_its_target_and_is_kept_with_it() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:all"])).unwrap();
           assert_eq!(labels(&narrowed), ["//:a", "//:b", "//:c"]);
           assert_eq!(narrowed.id_of(&label("//:all")), Some(NodeId::new(0)));
       }

       #[test]
       fn asking_for_every_target_gives_back_an_equal_graph() {
           let graph = graph();
           let narrowed = narrow(&graph, &only(&["//:a", "//:z"])).unwrap();
           assert_eq!(*narrowed, graph);
       }

       #[test]
       fn no_label_is_no_target() {
           let graph = graph();
           let narrowed = narrow(&graph, &Scope::Targets(BTreeSet::new())).unwrap();
           assert!(narrowed.is_empty());
       }

       #[test]
       fn a_label_naming_nothing_is_refused_with_the_nearest_one() {
           let graph = graph();
           let error = narrow(&graph, &only(&["//:al"])).unwrap_err();
           match error {
               Error::UnknownTarget {
                   label: asked,
                   suggestion,
               } => {
                   assert_eq!(asked, label("//:al"));
                   assert_eq!(suggestion, Some(label("//:a")));
               }
               other => unreachable!("expected UnknownTarget, got {other:?}"),
           }
       }
   }
   ```

5. Re-export the function and name it among the module's responsibilities. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: [`resolve`]; and [`narrow`](fn@narrow), which cuts a graph down to the
   //! targets one run was asked for.
   //!
   //! Non-responsibilities: evaluating build files and validating one declaration on its own,
   //! which the phase before this one does; and anything about whether a target needs to run.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod assembly;
   mod cycle;
   mod index;
   pub mod narrow;
   mod references;
   mod suggest;

   #[cfg(test)]
   mod fixtures;

   pub use assembly::resolve;
   pub use narrow::narrow;
   ```

6. Re-export it from the crate root. In `crates/buildl-core/src/lib.rs`:

   Replace:

   ```rust
       ExecStrategy, GraphRecord, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
   };
   pub use resolve::resolve;
   pub use types::{
       Action, ActionKey, ActionOutcome, Alias, Argument, BuildFile, CacheRow, Captured, Ceiling,
   ```

   With:

   ```rust
       ExecStrategy, GraphRecord, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
   };
   pub use resolve::{narrow, resolve};
   pub use types::{
       Action, ActionKey, ActionOutcome, Alias, Argument, BuildFile, CacheRow, Captured, Ceiling,
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::narrow
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 544 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `550 passed` (unit tests), `114 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/resolve/mod.rs crates/buildl-core/src/resolve/narrow.rs
   $ git commit -m "feat(buildl-core): narrow a graph to the targets a run was asked for"
   ```

### Task 2 — Run check and graph for a caller that holds only a source

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/src/pipeline/mod.rs`
- Modify `crates/buildl-core/tests/flows/check.rs`

**Steps:**

1. Write the failing flows: each function gives what the method of the same name gives. Append to `crates/buildl-core/tests/flows/check.rs`:

   Replace:

   ```rust
               assert_eq!(second_run.unwrap().item().name(), "r");
           }
           other => unreachable!("expected Nondeterministic, got {other:?}"),
       }
   }
   ```

   With:

   ```rust
               assert_eq!(second_run.unwrap().item().name(), "r");
           }
           other => unreachable!("expected Nondeterministic, got {other:?}"),
       }
   }

   #[test]
   fn a_caller_holding_only_a_source_checks_like_a_pipeline() {
       let agreeing = || {
           FakeSource::new(workspace(vec![
               ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
               ("lib", file(vec![target("text", &[])], &[])),
           ]))
       };
       let alone = buildl_core::pipeline::check(&agreeing(), &entry()).unwrap();
       let through = Pipeline::<FakePorts>::new(ports(agreeing()))
           .check(&entry())
           .unwrap();
       assert_eq!(alone, through);

       let first = workspace(vec![("", file(vec![target("app", &[])], &[]))]);
       let second = workspace(vec![(
           "",
           file(vec![target("app", &[]), target("extra", &[])], &[]),
       )]);
       let differing = FakeSource::with_second_run(first, second);
       assert!(matches!(
           buildl_core::pipeline::check(&differing, &entry()),
           Err(Error::Nondeterministic { .. })
       ));
   }

   #[test]
   fn a_caller_holding_only_a_source_gets_the_graph_a_pipeline_gets() {
       let fixture = || {
           FakeSource::new(workspace(vec![
               ("", file(vec![target("app", &["//lib:text"])], &["lib"])),
               ("lib", file(vec![target("text", &[])], &[])),
           ]))
       };
       let alone = buildl_core::pipeline::graph(&fixture(), &entry()).unwrap();
       let through = Pipeline::<FakePorts>::new(ports(fixture()))
           .graph(&entry())
           .unwrap();
       assert_eq!(
           buildl_core::json::canonical::to_vec(&alone).unwrap(),
           buildl_core::json::canonical::to_vec(&through).unwrap()
       );
   }
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --test flows check::a_caller
   error[E0425]: cannot find function `check` in module `buildl_core::pipeline`
   error[E0425]: cannot find function `graph` in module `buildl_core::pipeline`
   error: could not compile `buildl-core` (test "flows") due to 3 previous errors
   ```

3. Move the bodies of the two methods into functions over a source, and have the methods call them. The functions stand after the `impl` block and before the documentation of `first_difference`, which stays with its function. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
   //!
   //! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`],
   //! [`Pipeline::plan`] and [`Pipeline::build`].
   //!
   //! Non-responsibilities: the phases' own logic, which each phase's module holds.
   ```

   With:

   ```rust
   //!
   //! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`],
   //! [`Pipeline::plan`] and [`Pipeline::build`]; and [`check`] and [`graph`], the two commands
   //! that reach nothing but build files, for a caller that holds only a source of them.
   //!
   //! Non-responsibilities: the phases' own logic, which each phase's module holds.
   ```

   Replace:

   ```rust
   use crate::load::{declaration_order, load};
   use crate::plan::plan;
   use crate::ports::{PortSet, Ports};
   use crate::resolve::resolve;
   use crate::types::{
   ```

   With:

   ```rust
   use crate::load::{declaration_order, load};
   use crate::plan::plan;
   use crate::ports::{DeclarationSource, PortSet, Ports};
   use crate::resolve::resolve;
   use crate::types::{
   ```

   Replace:

   ```rust
       /// declare different things.
       pub fn check(&self, entry: &EntryName) -> Result<Vec<Declaration>> {
           let first = load(&self.ports.source, entry)?;
           let second = load(&self.ports.source, entry)?;
           first_difference(&first, &second).map_or(Ok(first), Err)
       }

       /// Loads the workspace once and resolves its declarations into the target graph.
   ```

   With:

   ```rust
       /// declare different things.
       pub fn check(&self, entry: &EntryName) -> Result<Vec<Declaration>> {
           check(&self.ports.source, entry)
       }

       /// Loads the workspace once and resolves its declarations into the target graph.
   ```

   Replace:

   ```rust
       /// dependency cycle.
       pub fn graph(&self, entry: &EntryName) -> Result<TargetGraph> {
           resolve(load(&self.ports.source, entry)?)
       }

       /// Loads the workspace once, resolves it, and plans a build of the graph.
   ```

   With:

   ```rust
       /// dependency cycle.
       pub fn graph(&self, entry: &EntryName) -> Result<TargetGraph> {
           graph(&self.ports.source, entry)
       }

       /// Loads the workspace once, resolves it, and plans a build of the graph.
   ```

   Replace:

   ```rust
           execute(&graph, &plan, on_failure, &mut self.ports)
       }
   }

   /// The error naming the first place two sorted declaration lists differ, if they do.
   ```

   With:

   ```rust
           execute(&graph, &plan, on_failure, &mut self.ports)
       }
   }

   /// Loads the workspace twice through `source` and returns its declarations when both loads
   /// agree: what [`Pipeline::check`] does, for a caller that holds a source and no other port.
   ///
   /// A command that only evaluates build files reads no state a build keeps, so it needs none of
   /// the ports that hold it.
   ///
   /// # Errors
   ///
   /// Returns whatever either load returns, and [`Error::Nondeterministic`] when the two loads
   /// declare different things.
   pub fn check<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let first = load(source, entry)?;
       let second = load(source, entry)?;
       first_difference(&first, &second).map_or(Ok(first), Err)
   }

   /// Loads the workspace once through `source` and resolves its declarations into the target
   /// graph: what [`Pipeline::graph`] does, for a caller that holds a source and no other port.
   ///
   /// # Errors
   ///
   /// Returns whatever the load returns, and otherwise whatever [`resolve`] returns.
   pub fn graph<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<TargetGraph> {
       resolve(load(source, entry)?)
   }

   /// The error naming the first place two sorted declaration lists differ, if they do.
   ```

4. Re-export them from the module. They are reached as `buildl_core::pipeline::check` and `buildl_core::pipeline::graph`, and are not re-exported from the crate root. `crates/buildl-core/src/pipeline/mod.rs` becomes:

   ```rust
   //! The pipeline: the phases run in sequence, each command a prefix of that sequence.
   //!
   //! Its own directory because it is the only module that names more than one phase: the phases
   //! communicate through values, and the pipeline is what passes each one's output to the next.
   //!
   //! Responsibilities: [`Pipeline`], [`check`] and [`graph`].
   //!
   //! Non-responsibilities: any phase's logic, and any port's implementation.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod driver;

   pub use driver::{Pipeline, check, graph};
   ```

5. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows check::a_caller
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `550 passed` (unit tests), `116 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/src/pipeline/mod.rs crates/buildl-core/tests/flows/check.rs
   $ git commit -m "feat(buildl-core): run check and graph for a caller that holds only a source"
   ```

### Task 3 — Plan the part of a workspace a caller asks for

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/main.rs`
- Modify `crates/buildl-core/tests/flows/plan.rs`
- Create `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flow, in a new file that the next tasks add to. Its module doc names what the file holds once this plan is done. Create `crates/buildl-core/tests/flows/report.rs`:

   ```rust
   //! What a build reports while it runs, what it records of its graph, and building part of a
   //! workspace: `Pipeline::build` and `Pipeline::plan` driven by the fakes of [`crate::common`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::collections::BTreeMap;

   use buildl_core::{
       Digest, EntryName, FileStat, Label, Pipeline, Scope, SettingName, SettingValue, SourcePath,
       Timestamp, Written, json,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, file, ports, program, target_with, workspace,
   };

   type Overrides = BTreeMap<SettingName, SettingValue>;

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   fn only(labels: &[&str]) -> Scope {
       Scope::Targets(labels.iter().map(|raw| label(raw)).collect())
   }

   fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str) {
       let path = SourcePath::parse(raw).unwrap();
       let content = FakeContent {
           bytes: raw.as_bytes().to_vec(),
           stat: FileStat {
               modified: Timestamp::from_unix_nanos(100),
               size: u64::try_from(raw.len()).unwrap(),
           },
       };
       let ports = pipeline.ports_mut();
       ports.digester.files.insert(path.clone(), content);
       ports.strategy.files.insert(path, raw.as_bytes().to_vec());
   }

   /// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone.
   fn chain() -> Pipeline<FakePorts> {
       let reading = |name: &str, input: &str, deps: &[&str]| {
           target_with(name, |target| {
               target.inputs = vec![Written::new(input)];
               target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
           })
       };
       let source = FakeSource::new(workspace(vec![(
           "",
           file(
               vec![
                   reading("gen", "gen.c", &[]),
                   reading("lib", "lib.c", &["gen"]),
                   reading("app", "main.c", &["lib"]),
                   reading("docs", "README", &[]),
               ],
               &[],
           ),
       )]));
       let mut pipeline = Pipeline::new(ports(source));
       for name in ["gen.c", "lib.c", "main.c", "README"] {
           put(&mut pipeline, name);
       }
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       pipeline
   }

   #[test]
   fn a_plan_of_one_target_holds_only_what_that_target_needs() {
       let mut pipeline = chain();

       let plan = pipeline
           .plan(&entry(), &only(&["//:lib"]), &Overrides::new())
           .unwrap();

       assert_eq!(plan.len(), 2);
       let text = json::canonical::to_string(&plan).unwrap();
       assert!(text.contains("\"//:gen\"") && text.contains("\"//:lib\""));
       assert!(!text.contains("\"//:app\"") && !text.contains("\"//:docs\""));
       assert!(
           pipeline.ports().graph.recorded.is_empty(),
           "a plan records no graph"
       );
   }
   ```

2. Declare it. `crates/buildl-core/tests/flows/main.rs` becomes:

   ```rust
   //! Pipeline flows through the ports, driven by fakes built from the public API alone.
   //!
   //! One test binary, so the fakes are shared by every flow without being compiled once per file.
   //! That the fakes compile at all is itself a check: an adapter outside this crate can build staged
   //! targets, settings and subdirs and implement every port the same way.

   pub mod common;

   mod build;
   mod check;
   mod graph;
   mod load;
   mod plan;
   mod report;
   ```

3. Run it and confirm it fails to build, because a plan takes no scope:

   ```
   $ cargo test -p buildl-core --test flows report::
   error[E0061]: this method takes 2 arguments but 3 arguments were supplied
   error: could not compile `buildl-core` (test "flows") due to 1 previous error
   ```

4. Give `Pipeline::plan` its scope and narrow between Resolve and Plan. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
   use crate::plan::plan;
   use crate::ports::{DeclarationSource, PortSet, Ports};
   use crate::resolve::resolve;
   use crate::types::{
       ActionOutcome, Declaration, EntryName, OnFailure, Plan, SettingName, SettingValue, TargetGraph,
   };

   /// The pipeline over one chosen implementation of each port.
   ```

   With:

   ```rust
   use crate::plan::plan;
   use crate::ports::{DeclarationSource, PortSet, Ports};
   use crate::resolve::{narrow, resolve};
   use crate::types::{
       ActionOutcome, Declaration, EntryName, OnFailure, Plan, Scope, SettingName, SettingValue,
       TargetGraph,
   };

   /// The pipeline over one chosen implementation of each port.
   ```

   Replace:

   ```rust
       }

       /// Loads the workspace once, resolves it, and plans a build of the graph.
       ///
       /// `overrides` gives values to build settings for this run. Takes `&mut self` because a plan
       /// records the digest of every file it had to hash.
       ///
       /// # Errors
       ///
       /// Returns whatever the load returns, then whatever [`resolve`] returns, and otherwise
       /// whatever [`plan`](fn@plan) returns: an override naming no setting, a want outside the
       /// workspace's ceiling, a program or an input that cannot be found, or a port's own failure.
       pub fn plan(
           &mut self,
           entry: &EntryName,
           overrides: &BTreeMap<SettingName, SettingValue>,
       ) -> Result<Plan> {
           let graph = resolve(load(&self.ports.source, entry)?)?;
           plan(&graph, overrides, &mut self.ports)
       }
   ```

   With:

   ```rust
       }

       /// Loads the workspace once, resolves it, and plans a build of the part of the graph `scope`
       /// asks for.
       ///
       /// `overrides` gives values to build settings for this run. A target outside `scope` is not
       /// planned, so nothing it reads has to exist. Takes `&mut self` because a plan records the
       /// digest of every file it had to hash.
       ///
       /// # Errors
       ///
       /// Returns whatever the load returns, then whatever [`resolve`] returns, then
       /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias,
       /// and otherwise whatever [`plan`](fn@plan) returns: an override naming no setting, a want
       /// outside the workspace's ceiling, a program that cannot be found, an input that is missing
       /// or is not a regular file, or a port's own failure.
       pub fn plan(
           &mut self,
           entry: &EntryName,
           scope: &Scope,
           overrides: &BTreeMap<SettingName, SettingValue>,
       ) -> Result<Plan> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           let graph = narrow(&whole, scope)?;
           plan(&graph, overrides, &mut self.ports)
       }
   ```

5. Name the scope in every plan the existing flows make: each call of `plan` gains `&Scope::Workspace` as its second argument, and rustfmt breaks the calls that no longer fit on a line. In `crates/buildl-core/tests/flows/plan.rs`:

   Replace:

   ```rust
       Verdict, Want, Written, json,
   };
   use buildl_core::{Execution, Isolation, Manifest, Timeout, Workers};

   use crate::common::{
   ```

   With:

   ```rust
       Verdict, Want, Written, json,
   };
   use buildl_core::{Execution, Isolation, Manifest, Scope, Timeout, Workers};

   use crate::common::{
   ```

   Replace:

   ```rust
   /// A plan of the workspace with no setting overridden.
   fn planned(pipeline: &mut Pipeline<FakePorts>) -> Plan {
       pipeline.plan(&entry(), &Overrides::new()).unwrap()
   }

   /// What a verdict says, without its key: the reason, and the labels of the targets awaited.
   ```

   With:

   ```rust
   /// A plan of the workspace with no setting overridden.
   fn planned(pipeline: &mut Pipeline<FakePorts>) -> Plan {
       pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap()
   }

   /// What a verdict says, without its key: the reason, and the labels of the targets awaited.
   ```

   Replace:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, overrides: &Overrides, version: &str) {
       let graph = pipeline.graph(&entry()).unwrap();
       let plan = pipeline.plan(&entry(), overrides).unwrap();
       let mut built = BTreeSet::new();
       while built.len() < graph.len() {
   ```

   With:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, overrides: &Overrides, version: &str) {
       let graph = pipeline.graph(&entry()).unwrap();
       let plan = pipeline
           .plan(&entry(), &Scope::Workspace, overrides)
           .unwrap();
       let mut built = BTreeSet::new();
       while built.len() < graph.len() {
   ```

   Replace:

   ```rust
       build(&mut pipeline, &Overrides::new(), "v1");

       let plan = pipeline
           .plan(&entry(), &overrides(&[("mode", "release")]))
           .unwrap();

       let mode = SettingName::parse("mode").unwrap();
   ```

   With:

   ```rust
       build(&mut pipeline, &Overrides::new(), "v1");

       let plan = pipeline
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("mode", "release")]),
           )
           .unwrap();

       let mode = SettingName::parse("mode").unwrap();
   ```

   Replace:

   ```rust
       pipeline.ports_mut().manifest.workspace.ceiling.run.clear();

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       pipeline.ports_mut().manifest.workspace.ceiling.run.clear();

       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
           target.env = vec![Written::new("AWS_SECRET")];
       })]);
       let error = reads_secret.plan(&entry(), &Overrides::new()).unwrap_err();
       assert_eq!(
           error.to_string(),
   ```

   With:

   ```rust
           target.env = vec![Written::new("AWS_SECRET")];
       })]);
       let error = reads_secret
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert_eq!(
           error.to_string(),
   ```

   Replace:

   ```rust
       let ceiling = &mut reads_outside.ports_mut().manifest.workspace.ceiling;
       ceiling.read = BTreeSet::from([dir("src")]);
       let error = reads_outside.plan(&entry(), &Overrides::new()).unwrap_err();
       assert_eq!(
           error.to_string(),
   ```

   With:

   ```rust
       let ceiling = &mut reads_outside.ports_mut().manifest.workspace.ceiling;
       ceiling.read = BTreeSet::from([dir("src")]);
       let error = reads_outside
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert_eq!(
           error.to_string(),
   ```

   Replace:

   ```rust
       ceiling.write = BTreeSet::from([dir(".buildl")]);
       let error = writes_outside
           .plan(&entry(), &Overrides::new())
           .unwrap_err();
       assert_eq!(
   ```

   With:

   ```rust
       ceiling.write = BTreeSet::from([dir(".buildl")]);
       let error = writes_outside
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert_eq!(
   ```

   Replace:

   ```rust
       pipeline.ports_mut().tools.fingerprints.clear();

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       pipeline.ports_mut().tools.fingerprints.clear();

       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
               .insert(path("lib.c"), found);

           let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

           assert_eq!(error.to_string(), text);
   ```

   With:

   ```rust
               .insert(path("lib.c"), found);

           let error = pipeline
               .plan(&entry(), &Scope::Workspace, &Overrides::new())
               .unwrap_err();

           assert_eq!(error.to_string(), text);
   ```

   Replace:

   ```rust
       pipeline.ports_mut().digester.files.remove(&path("lib.c"));

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       pipeline.ports_mut().digester.files.remove(&path("lib.c"));

       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
       }

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(error.to_string(), "host: ALPHA does not hold valid text");
   ```

   With:

   ```rust
       }

       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();

       assert_eq!(error.to_string(), "host: ALPHA does not hold valid text");
   ```

   Replace:

   ```rust
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       let unknown = overrides(&[("bogus", "1")]);
       let error = pipeline.plan(&entry(), &unknown).unwrap_err();
       assert!(matches!(error, Error::UnknownOverride { .. }), "{error}");

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("docker"))),
   ```

   With:

   ```rust
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       let unknown = overrides(&[("bogus", "1")]);
       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &unknown)
           .unwrap_err();
       assert!(matches!(error, Error::UnknownOverride { .. }), "{error}");

       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert!(
           matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("docker"))),
   ```

   Replace:

   ```rust
       let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
       ceiling.run.insert(program("docker"));
       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::MissingTool { program: missing, .. } if missing == &program("cc")),
   ```

   With:

   ```rust
       let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
       ceiling.run.insert(program("docker"));
       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert!(
           matches!(&error, Error::MissingTool { program: missing, .. } if missing == &program("cc")),
   ```

   Replace:

   ```rust
               .insert(program(name), Digest::of(name.as_bytes()));
       }
       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::MissingInput { path: missing, .. } if missing == &path("missing.c")),
   ```

   With:

   ```rust
               .insert(program(name), Digest::of(name.as_bytes()));
       }
       let error = pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err();
       assert!(
           matches!(&error, Error::MissingInput { path: missing, .. } if missing == &path("missing.c")),
   ```

   Replace:

   ```rust
       let mut pipeline = root_only(vec![setting("mode"), running("a", &["cc", "$opt:mode"])]);

       let error = pipeline
           .plan(&entry(), &overrides(&[("mod", "release")]))
           .unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       let mut pipeline = root_only(vec![setting("mode"), running("a", &["cc", "$opt:mode"])]);

       let error = pipeline
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("mod", "release")]),
           )
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
           .plan(
               &entry(),
               &overrides(&[("mode", "release"), ("unused", "busy")]),
           )
   ```

   With:

   ```rust
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("mode", "release"), ("unused", "busy")]),
           )
   ```

   Replace:

   ```rust
       build(&mut pipeline, &Overrides::new(), "v1");

       let plan = pipeline
           .plan(&entry(), &overrides(&[("tool", "clang")]))
           .unwrap();

       let tool = SettingName::parse("tool").unwrap();
   ```

   With:

   ```rust
       build(&mut pipeline, &Overrides::new(), "v1");

       let plan = pipeline
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("tool", "clang")]),
           )
           .unwrap();

       let tool = SettingName::parse("tool").unwrap();
   ```

   Replace:

   ```rust
       let mut pipeline = tool_by_setting();

       let error = pipeline
           .plan(&entry(), &overrides(&[("tool", "docker")]))
           .unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       let mut pipeline = tool_by_setting();

       let error = pipeline
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("tool", "docker")]),
           )
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
       tools.fingerprints.remove(&program("clang"));

       let error = pipeline
           .plan(&entry(), &overrides(&[("tool", "clang")]))
           .unwrap_err();

       assert_eq!(
   ```

   With:

   ```rust
       tools.fingerprints.remove(&program("clang"));

       let error = pipeline
           .plan(
               &entry(),
               &Scope::Workspace,
               &overrides(&[("tool", "clang")]),
           )
           .unwrap_err();

       assert_eq!(
   ```

   Replace:

   ```rust
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       match pipeline.plan(&entry(), &Overrides::new()).unwrap_err() {
           Error::Evaluation {
               provenance,
   ```

   With:

   ```rust
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       match pipeline
           .plan(&entry(), &Scope::Workspace, &Overrides::new())
           .unwrap_err()
       {
           Error::Evaluation {
               provenance,
   ```

6. Run the new flow, then every flow, and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   $ cargo test -p buildl-core --test flows
   test result: ok. 117 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `550 passed` (unit tests), `117 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/main.rs crates/buildl-core/tests/flows/plan.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): plan the part of a workspace a caller asks for"
   ```

### Task 4 — Build the part of a workspace a caller asks for

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/build.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flows: a build of no target builds nothing, a label nothing declares is refused before anything runs, and a whole build after a build of one target finds that part cached. They share a helper, `build`. Two of their assertions mean more later than now: that the reporter received no call is true of every build until task 6, and that two builds record the same bytes compares nothing with nothing until task 10. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   )]

   use std::collections::BTreeMap;

   use buildl_core::{
       Digest, EntryName, FileStat, Label, Pipeline, Scope, SettingName, SettingValue, SourcePath,
       Timestamp, Written, json,
   };

   use crate::common::{
   ```

   With:

   ```rust
   )]

   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       Digest, EntryName, Error, FileStat, Label, OnFailure, Pipeline, Scope, SettingName,
       SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
   ```

   Replace:

   ```rust
   }

   #[test]
   fn a_plan_of_one_target_holds_only_what_that_target_needs() {
   ```

   With:

   ```rust
   }

   fn build(pipeline: &mut Pipeline<FakePorts>, scope: &Scope) -> Summary {
       let outcomes = pipeline
           .build(&entry(), scope, &Overrides::new(), OnFailure::Stop)
           .unwrap();
       Summary::of(&outcomes)
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
       let mut pipeline = chain();
       build(&mut pipeline, &only(&["//:lib"]));

       let summary = build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
           summary,
           Summary {
               failed: 0,
               skipped: 0,
               built: 2,
               cached: 2
           }
       );
       assert_eq!(
           pipeline.ports().graph.recorded.first(),
           pipeline.ports().graph.recorded.get(1),
           "both builds record the same bytes"
       );
   }

   #[test]
   fn a_plan_of_one_target_holds_only_what_that_target_needs() {
   ```

   Replace:

   ```rust
           "a plan records no graph"
       );
   }
   ```

   With:

   ```rust
           "a plan records no graph"
       );
   }

   #[test]
   fn asking_for_a_target_nothing_declares_is_refused_and_nothing_runs() {
       let mut pipeline = chain();

       let error = pipeline
           .build(
               &entry(),
               &only(&["//:libs"]),
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err();

       match error {
           Error::UnknownTarget {
               label: asked,
               suggestion,
           } => {
               assert_eq!(asked, label("//:libs"));
               assert_eq!(suggestion, Some(label("//:lib")));
           }
           other => unreachable!("expected UnknownTarget, got {other:?}"),
       }
       assert_eq!(pipeline.ports().strategy.ran(), []);
       assert_eq!(pipeline.ports().reporter.calls, []);
   }

   #[test]
   fn asking_for_no_target_builds_nothing() {
       let mut pipeline = chain();

       let summary = build(&mut pipeline, &Scope::Targets(BTreeSet::new()));

       assert_eq!(
           summary,
           Summary {
               failed: 0,
               skipped: 0,
               built: 0,
               cached: 0
           }
       );
   }
   ```

2. Run them and confirm they fail to build, because a build takes no scope:

   ```
   $ cargo test -p buildl-core --test flows report::
   error[E0061]: this method takes 3 arguments but 4 arguments were supplied
   error: could not compile `buildl-core` (test "flows") due to 2 previous errors
   ```

3. Give `Pipeline::build` its scope and narrow between Resolve and Plan. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
       }

       /// Loads the workspace once, resolves it, plans a build of the graph, and builds it.
       ///
       /// `overrides` gives values to build settings for this run, and `on_failure` says what a
       /// failed action does to the rest of the build. The outcomes are one per target, in id
       /// order; a target that failed or was skipped is an outcome, not an error.
       ///
       /// # Errors
       ///
       /// Returns whatever the load returns, then whatever [`resolve`] returns, then whatever
       /// [`plan`](fn@plan) returns, and otherwise whatever [`execute`](fn@execute) returns: two
       /// outputs that would be written to one place, a command that names no program or one
       /// outside the workspace's ceiling once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
           entry: &EntryName,
           overrides: &BTreeMap<SettingName, SettingValue>,
           on_failure: OnFailure,
       ) -> Result<Vec<ActionOutcome>> {
           let graph = resolve(load(&self.ports.source, entry)?)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&graph, &plan, on_failure, &mut self.ports)
   ```

   With:

   ```rust
       }

       /// Loads the workspace once, resolves it, plans a build of the part of the graph `scope` asks
       /// for, and builds that part.
       ///
       /// `overrides` gives values to build settings for this run, and `on_failure` says what a
       /// failed action does to the rest of the build. The outcomes are one per target of that part,
       /// in id order; a target that failed or was skipped is an outcome, not an error. A target
       /// outside `scope` is left alone: it has no outcome, and its row and outputs stay.
       ///
       /// # Errors
       ///
       /// Returns the first of: whatever the load returns; whatever [`resolve`] returns;
       /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias;
       /// whatever [`plan`](fn@plan) returns; and whatever [`execute`](fn@execute) returns: two
       /// outputs that would be written to one place, a command that names no program or one outside
       /// the workspace's ceiling once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
           entry: &EntryName,
           scope: &Scope,
           overrides: &BTreeMap<SettingName, SettingValue>,
           on_failure: OnFailure,
       ) -> Result<Vec<ActionOutcome>> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&graph, &plan, on_failure, &mut self.ports)
   ```

4. Name the scope in every build the existing flows make: each call of `build` gains `&Scope::Workspace` as its second argument. In `crates/buildl-core/tests/flows/build.rs`:

   Replace:

   ```rust
   use std::path::Path;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileMode, FileStat, Freshness,
   ```

   With:

   ```rust
   use std::path::Path;

   use buildl_core::Scope;
   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileMode, FileStat, Freshness,
   ```

   Replace:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
       pipeline
           .build(&entry(), &Overrides::new(), on_failure)
           .unwrap()
   }
   ```

   With:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
       pipeline
           .build(&entry(), &Scope::Workspace, &Overrides::new(), on_failure)
           .unwrap()
   }
   ```

   Replace:

   ```rust
       pipeline.ports_mut().log.broken = true;

       let error = pipeline
           .build(&entry(), &Overrides::new(), OnFailure::KeepGoing)
           .unwrap_err();

       assert_eq!(error.to_string(), "event log: the log cannot be written");
   ```

   With:

   ```rust
       pipeline.ports_mut().log.broken = true;

       let error = pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &Overrides::new(),
               OnFailure::KeepGoing,
           )
           .unwrap_err();

       assert_eq!(error.to_string(), "event log: the log cannot be written");
   ```

   Replace:

   ```rust
   fn refused(pipeline: &mut Pipeline<FakePorts>) -> Error {
       pipeline
           .build(&entry(), &Overrides::new(), OnFailure::Stop)
           .unwrap_err()
   }
   ```

   With:

   ```rust
   fn refused(pipeline: &mut Pipeline<FakePorts>) -> Error {
       pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err()
   }
   ```

   Replace:

   ```rust
       )]);
       pipeline
           .build(&entry(), &overrides, OnFailure::Stop)
           .unwrap()
   }
   ```

   With:

   ```rust
       )]);
       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap()
   }
   ```

   Replace:

   ```rust
               pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

               let error = pipeline
                   .build(&entry(), &Overrides::new(), on_failure)
                   .unwrap_err();

               assert_eq!(
   ```

   With:

   ```rust
               pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

               let error = pipeline
                   .build(&entry(), &Scope::Workspace, &Overrides::new(), on_failure)
                   .unwrap_err();

               assert_eq!(
   ```

   Replace:

   ```rust
           pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

           let error = pipeline
               .build(&entry(), &Overrides::new(), OnFailure::Stop)
               .unwrap_err();

           assert!(matches!(error, Error::DispatcherFault { .. }), "{loss:?}");
   ```

   With:

   ```rust
           pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

           let error = pipeline
               .build(
                   &entry(),
                   &Scope::Workspace,
                   &Overrides::new(),
                   OnFailure::Stop,
               )
               .unwrap_err();

           assert!(matches!(error, Error::DispatcherFault { .. }), "{loss:?}");
   ```

   Replace:

   ```rust
       )]);
       pipeline
           .build(&entry(), &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       )]);
       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   Replace:

   ```rust
       build(&mut defaulted, OnFailure::Stop);
       overridden
           .build(&entry(), &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       build(&mut defaulted, OnFailure::Stop);
       overridden
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   Replace:

   ```rust
       )]);

       pipeline
           .build(&entry(), &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       )]);

       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

5. Run the new flows, then every flow, and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   $ cargo test -p buildl-core --test flows
   test result: ok. 120 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `550 passed` (unit tests), `120 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/build.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): build the part of a workspace a caller asks for"
   ```

### Task 5 — Keep the outputs of the whole workspace apart, whatever is asked for

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flow: `//:a` writes `out/lib`, the directory `//lib:t` owns, and a build of `//:a` alone must still be refused. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   }

   #[test]
   fn asking_for_no_target_builds_nothing() {
       let mut pipeline = chain();
   ```

   With:

   ```rust
   }

   #[test]
   fn two_targets_claiming_one_place_are_refused_even_when_only_one_is_asked_for() {
       let source = FakeSource::new(workspace(vec![
           (
               "",
               file(
                   vec![target_with("a", |target| {
                       target.outputs = Some(vec![Written::new("lib")]);
                   })],
                   &["lib"],
               ),
           ),
           ("lib", file(vec![target_with("t", |_| {})], &[])),
       ]));
       let mut pipeline = Pipeline::new(ports(source));
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));

       let error = pipeline
           .build(
               &entry(),
               &only(&["//:a"]),
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err();

       assert!(
           matches!(error, Error::OutputConflict { .. }),
           "expected OutputConflict, got {error:?}"
       );
       assert_eq!(pipeline.ports().strategy.ran(), []);
   }

   #[test]
   fn asking_for_no_target_builds_nothing() {
       let mut pipeline = chain();
   ```

2. Run it and confirm it fails: the narrowed graph holds one target, so nothing collides and the build goes through.

   ```
   $ cargo test -p buildl-core --test flows report::
   test report::two_targets_claiming_one_place_are_refused_even_when_only_one_is_asked_for ... FAILED
   called `Result::unwrap_err()` on an `Ok` value: [ActionOutcome { label: Label { directory: Directory(""), name: TargetName("a") }, class: Built, …
   test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 116 filtered out
   ```

3. Hand `execute` the whole graph as well, and lay it out first, for its refusal only. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   /// is no part of the key.
   ///
   /// # Errors
   ///
   /// Before anything runs, returns the first failure met, in this order:
   ///
   /// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
   /// 2. whatever the manifest port returns;
   /// 3. [`Error::OutputConflict`] when two outputs would be written to one place;
   /// 4. for the first target, in id order, whose expanded command cannot run:
   ///    [`Error::NoProgram`] when it names no program, and [`Error::Refused`] when the program
   ```

   With:

   ```rust
   /// is no part of the key.
   ///
   /// `whole` is the graph of the whole workspace, the one `graph` was narrowed from, or `graph`
   /// itself. A target of `whole` that `graph` leaves out is no part of the build: it has no
   /// outcome, no event and no row, and whatever it left in the workspace stays.
   ///
   /// # Errors
   ///
   /// Before anything runs, returns the first failure met, in this order:
   ///
   /// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
   /// 2. whatever the manifest port returns;
   /// 3. [`Error::OutputConflict`] when two outputs of `whole` would be written to one place;
   /// 4. for the first target, in id order, whose expanded command cannot run:
   ///    [`Error::NoProgram`] when it names no program, and [`Error::Refused`] when the program
   ```

   Replace:

   ```rust
   /// with a target missing. What ended before is recorded and logged already, and stays.
   pub fn execute<P: Ports>(
       graph: &TargetGraph,
       plan: &Plan,
   ```

   With:

   ```rust
   /// with a target missing. What ended before is recorded and logged already, and stays.
   pub fn execute<P: Ports>(
       whole: &TargetGraph,
       graph: &TargetGraph,
       plan: &Plan,
   ```

   Replace:

   ```rust
       }
       let workspace = ports.manifest.workspace()?;
       let layout = Layout::of(graph, &workspace.out)?;
       let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;
   ```

   With:

   ```rust
       }
       let workspace = ports.manifest.workspace()?;
       Layout::of(whole, &workspace.out)?;
       let layout = Layout::of(graph, &workspace.out)?;
       let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;
   ```

4. Pass it. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
       /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias;
       /// whatever [`plan`](fn@plan) returns; and whatever [`execute`](fn@execute) returns: two
       /// outputs that would be written to one place, a command that names no program or one outside
       /// the workspace's ceiling once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
   ```

   With:

   ```rust
       /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias;
       /// whatever [`plan`](fn@plan) returns; and whatever [`execute`](fn@execute) returns: two
       /// outputs of the workspace that would be written to one place, whether or not both targets
       /// were asked for, a command that names no program or one outside the workspace's ceiling
       /// once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
   ```

   Replace:

   ```rust
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&graph, &plan, on_failure, &mut self.ports)
       }
   }
   ```

   With:

   ```rust
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&whole, &graph, &plan, on_failure, &mut self.ports)
       }
   }
   ```

5. Run the flows of the file and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `550 passed` (unit tests), `121 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): keep the outputs of the whole workspace apart in a build of a part"
   ```

### Task 6 — Report each outcome with what its action printed, and the count last

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/src/execute/record.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flows. On a second build every target is cached, never starts, and is reported with what it printed when it ran; a build that ends in an error reports no count. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       Digest, EntryName, Error, FileStat, Label, OnFailure, Pipeline, Scope, SettingName,
       SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, file, ports, program, target_with, workspace,
   };

   type Overrides = BTreeMap<SettingName, SettingValue>;
   ```

   With:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       Digest, EntryName, Error, FileStat, Label, OnFailure, OutcomeClass, Pipeline, Scope,
       SettingName, SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, Reported, Script, file, ports, program, target_with,
       workspace,
   };

   type Overrides = BTreeMap<SettingName, SettingValue>;
   ```

   Replace:

   ```rust
           .unwrap();
       Summary::of(&outcomes)
   }

   #[test]
   ```

   With:

   ```rust
           .unwrap();
       Summary::of(&outcomes)
   }

   fn finished(raw: &str, class: OutcomeClass, stdout: &str, stderr: &str) -> Reported {
       Reported::Finished {
           label: label(raw),
           class,
           stdout: stdout.as_bytes().to_vec(),
           stderr: stderr.as_bytes().to_vec(),
       }
   }

   #[test]
   fn a_cached_target_never_starts_and_is_reported_with_what_it_printed_when_it_ran() {
       let mut pipeline = chain();
       pipeline.ports_mut().strategy.scripts.insert(
           label("//:gen"),
           Script {
               stderr: Some(b"gen.c:1: warning: unused\n".to_vec()),
               ..Script::default()
           },
       );
       build(&mut pipeline, &Scope::Workspace);
       pipeline.ports_mut().reporter.calls.clear();

       build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
           pipeline.ports().reporter.calls,
           [
               Reported::Begun(4),
               finished("//:docs", OutcomeClass::Cached, "//:docs\n", ""),
               finished(
                   "//:gen",
                   OutcomeClass::Cached,
                   "//:gen\n",
                   "gen.c:1: warning: unused\n"
               ),
               finished("//:lib", OutcomeClass::Cached, "//:lib\n", ""),
               finished("//:app", OutcomeClass::Cached, "//:app\n", ""),
               Reported::Summary(Summary {
                   failed: 0,
                   skipped: 0,
                   built: 0,
                   cached: 4
               }),
           ]
       );
   }

   #[test]
   ```

   Replace:

   ```rust
   }

   #[test]
   fn asking_for_no_target_builds_nothing() {
       let mut pipeline = chain();
   ```

   With:

   ```rust
   }

   #[test]
   fn a_build_that_fails_as_a_whole_reports_no_summary() {
       let mut pipeline = chain();
       pipeline.ports_mut().log.broken = true;

       let failed = pipeline.build(
           &entry(),
           &Scope::Workspace,
           &Overrides::new(),
           OnFailure::Stop,
       );

       assert!(failed.is_err());
       let calls = &pipeline.ports().reporter.calls;
       assert!(
           !calls
               .iter()
               .any(|call| matches!(call, Reported::Summary(_))),
           "{calls:?}"
       );
   }

   #[test]
   fn asking_for_no_target_builds_nothing() {
       let mut pipeline = chain();
   ```

2. Run them and confirm the first fails, because nothing is reported. The second passes already, since a build that reports nothing reports no count: it is there to hold once one is reported.

   ```
   $ cargo test -p buildl-core --test flows report::
   test report::a_cached_target_never_starts_and_is_reported_with_what_it_printed_when_it_ran ... FAILED
   assertion `left == right` failed
     left: []
    right: [Begun(4), Finished { label: Label { directory: Directory(""), name: TargetName("docs") }, class: Cached, …
   test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 116 filtered out
   ```

3. Write the failing unit tests of what reads the printed bytes back: nothing is read for an action that printed nothing or whose output was not stored, held content comes back, and content that is not held is the content store's failure. They use a store of their own, which writes down every digest it is asked to read. In the `tests` module of `crates/buildl-core/src/execute/record.rs`:

   Replace:

   ```rust
       use std::sync::{Mutex, PoisonError};

       use super::{
           Progress, built, cached, event, failed, mismatched, place, row, skipped, unplaced, unstored,
       };
       use crate::error::{Error, PortName, Result};
   ```

   With:

   ```rust
       use std::sync::{Mutex, PoisonError};

       use super::{
           Progress, built, cached, event, failed, mismatched, place, row, shown, skipped, unplaced,
           unstored,
       };
       use crate::error::{Error, PortName, Result};
   ```

   Replace:

   ```rust
                   return Err(store_error(&format!("{at} cannot be written")));
               }
               Ok(())
           }
   ```

   With:

   ```rust
                   return Err(store_error(&format!("{at} cannot be written")));
               }
               Ok(())
           }
       }

       /// A store that holds `held` and nothing else, and writes down every digest it is asked
       /// to read.
       #[derive(Default)]
       struct Holding {
           held: Vec<Vec<u8>>,
           read: Mutex<Vec<Digest>>,
       }

       impl Holding {
           fn read(&self) -> Vec<Digest> {
               self.read
                   .lock()
                   .unwrap_or_else(PoisonError::into_inner)
                   .clone()
           }
       }

       impl ContentStore for Holding {
           fn put(&self, bytes: &[u8]) -> Result<Digest> {
               Ok(Digest::of(bytes))
           }

           fn contains(&self, digest: &Digest) -> bool {
               self.held.iter().any(|bytes| Digest::of(bytes) == *digest)
           }

           fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
               self.read
                   .lock()
                   .unwrap_or_else(PoisonError::into_inner)
                   .push(*digest);
               Ok(self
                   .held
                   .iter()
                   .find(|bytes| Digest::of(bytes) == *digest)
                   .cloned())
           }

           fn clear(&self, _at: &OutputPath) -> Result<()> {
               Ok(())
           }

           fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
               Ok(())
           }
   ```

   Replace:

   ```rust
           );
       }
   }
   ```

   With:

   ```rust
           );
       }

       #[test]
       fn nothing_is_read_for_an_action_that_printed_nothing_or_whose_output_was_not_stored() {
           let store = Holding::default();

           assert_eq!(shown(&store, &label("//:app"), None).unwrap(), b"");
           assert_eq!(
               shown(&store, &label("//:app"), Some(Digest::of(b""))).unwrap(),
               b""
           );
           assert!(store.read().is_empty(), "the store was not asked");
       }

       #[test]
       fn what_an_action_printed_is_read_back_from_the_store() {
           let store = Holding {
               held: vec![b"compiling\n".to_vec()],
               ..Holding::default()
           };
           let digest = Digest::of(b"compiling\n");

           assert_eq!(
               shown(&store, &label("//:app"), Some(digest)).unwrap(),
               b"compiling\n"
           );
           assert_eq!(store.read(), [digest]);
       }

       #[test]
       fn printed_content_that_is_not_held_is_the_content_stores_failure() {
           let store = Holding::default();
           let digest = Digest::of(b"compiling\n");

           let error = shown(&store, &label("//:app"), Some(digest)).unwrap_err();

           assert_eq!(
               error.to_string(),
               format!("content store: what //:app printed is not held: {digest}")
           );
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::ContentStore,
                   ..
               }
           ));
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::record
   error[E0432]: unresolved import `super::shown`
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

5. Have `append` return the event it logged, and write `shown`. In `crates/buildl-core/src/execute/record.rs`:

   Replace:

   ```rust
   //! target's outputs in place and only then records its row, [`restore`] puts a cached
   //! target's outputs in place, and [`append`] logs an outcome whatever it is. An output that
   //! cannot be put in place whole is removed from its path again.
   //!
   //! Non-responsibilities: how content is put in place, how a row is kept and how the log is
   //! written, which are the ports'.

   use crate::error::{Error, Result};
   use crate::execute::perform::{Kept, NOT_STORED, Printed, Span};
   use crate::ports::{ActionCache, Clock, ContentStore, EventLog};
   ```

   With:

   ```rust
   //! target's outputs in place and only then records its row, [`restore`] puts a cached
   //! target's outputs in place, and [`append`] logs an outcome whatever it is. An output that
   //! cannot be put in place whole is removed from its path again. [`shown`] reads back what an
   //! action printed, for whoever shows its outcome.
   //!
   //! Non-responsibilities: how content is put in place, how a row is kept and how the log is
   //! written, which are the ports'.

   use crate::error::{Error, PortName, Result};
   use crate::execute::perform::{Kept, NOT_STORED, Printed, Span};
   use crate::ports::{ActionCache, Clock, ContentStore, EventLog};
   ```

   Replace:

   ```rust
   }

   /// Appends the event for `outcome` to `log`, reading `clock` once for when it was recorded.
   ///
   /// # Errors
   ```

   With:

   ```rust
   }

   /// Appends the event for `outcome` to `log`, reading `clock` once for when it was recorded, and
   /// returns the event once it is logged.
   ///
   /// # Errors
   ```

   Replace:

   ```rust
       progress: Progress,
       outcome: ActionOutcome,
   ) -> Result<()> {
       log.append(&event(clock.now(), provenance, command, progress, outcome))
   }

   /// Puts each output of `manifests` at its path of `paths`, pairwise, in the order the outputs
   ```

   With:

   ```rust
       progress: Progress,
       outcome: ActionOutcome,
   ) -> Result<OutcomeEvent> {
       let event = event(clock.now(), provenance, command, progress, outcome);
       log.append(&event)?;
       Ok(event)
   }

   /// What a reporter is still to be shown of what an action printed: the content `digest`
   /// names, read from `cas`.
   ///
   /// Nothing is read, and nothing is returned, for an action that printed nothing or whose
   /// output was not stored.
   ///
   /// # Errors
   ///
   /// Returns whatever `cas` returns, and [`Error::Adapter`], naming
   /// [`PortName::ContentStore`], when the content is not held: an outcome names only content
   /// that was stored, or found stored, moments before.
   pub(crate) fn shown<C: ContentStore>(
       cas: &C,
       label: &Label,
       digest: Option<Digest>,
   ) -> Result<Vec<u8>> {
       match digest {
           Some(digest) if digest != Digest::of(&[]) => {
               cas.get(&digest)?.ok_or_else(|| Error::Adapter {
                   port: PortName::ContentStore,
                   diagnostic: Diagnostic::new(format!("what {label} printed is not held: {digest}")),
               })
           }
           _ => Ok(Vec::new()),
       }
   }

   /// Puts each output of `manifests` at its path of `paths`, pairwise, in the order the outputs
   ```

6. Report from the scheduling thread: that the build begins, right before the dispatcher is called; each outcome, after its event is appended; and the count, after the check that no target was lost. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   //! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
   //! build has stopped; the bookkeeping between two results — which targets wait to be settled,
   //! which have their outcome, and the one error that fails the whole build; and refusing to
   //! report a build whose dispatcher returned with a target left without an outcome.
   //!
   //! Non-responsibilities: what each step decides. Each has its own sibling file.
   ```

   With:

   ```rust
   //! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
   //! build has stopped; the bookkeeping between two results — which targets wait to be settled,
   //! which have their outcome, and the one error that fails the whole build; having the build
   //! reported as it goes, an outcome only once its event is logged; and refusing to report a
   //! build whose dispatcher returned with a target left without an outcome.
   //!
   //! Non-responsibilities: what each step decides. Each has its own sibling file.
   ```

   Replace:

   ```rust
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Task, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   use crate::execute::settle::{Settled, settle};
   use crate::ports::{Dispatcher, Manifest, PortSet, Ports};
   use crate::types::{
       ActionOutcome, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label, Node, NodeId,
       OnFailure, OutputPath, OwnComponents, Plan, PlannedKey, ReadyAction, TargetGraph, Turn,
       Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   With:

   ```rust
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Task, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, shown, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   use crate::execute::settle::{Settled, settle};
   use crate::ports::{Dispatcher, Manifest, PortSet, Ports, Reporter};
   use crate::types::{
       ActionOutcome, Captured, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label, Node,
       NodeId, OnFailure, OutcomeEvent, OutputPath, OwnComponents, Plan, PlannedKey, ReadyAction,
       Summary, TargetGraph, Turn, Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   Replace:

   ```rust
   /// outcome, no event and no row, and whatever it left in the workspace stays.
   ///
   /// # Errors
   ///
   ```

   With:

   ```rust
   /// outcome, no event and no row, and whatever it left in the workspace stays.
   ///
   /// The build is shown through the reporter port, always from the calling thread: that it begins,
   /// once nothing can refuse it any more; each target's outcome, once its event is logged; and the
   /// count of the outcomes last, unless the build ends in an error. An outcome is reported with
   /// what its action printed, read back from the content store, for a target that ran as for one
   /// that is cached.
   ///
   /// # Errors
   ///
   ```

   Replace:

   ```rust
   /// Once the build is under way, returns the first of: the log port's failure to append an
   /// event; [`Error::CanonicalJson`] for a key that cannot be serialized; [`Error::Adapter`],
   /// naming the action cache, for a dependency that succeeded and has no row; and the content
   /// store's failure to read the manifest of a recorded output. After such an error no port is
   /// asked anything more. Otherwise returns whatever the dispatcher port itself returns.
   ///
   /// When the dispatcher returns success and a target has no outcome, returns
   ```

   With:

   ```rust
   /// Once the build is under way, returns the first of: the log port's failure to append an
   /// event; [`Error::CanonicalJson`] for a key that cannot be serialized; [`Error::Adapter`],
   /// naming the action cache, for a dependency that succeeded and has no row; the content
   /// store's failure to read the manifest of a recorded output; and its failure to read what a
   /// target printed, which is [`Error::Adapter`] naming the content store when that content is
   /// no longer held. After such an error no port is asked anything more, and no count is
   /// reported. Otherwise returns whatever the dispatcher port itself returns.
   ///
   /// When the dispatcher returns success and a target has no outcome, returns
   ```

   Replace:

   ```rust
           dispatcher,
           clock,
           ..
       } = ports;
   ```

   With:

   ```rust
           dispatcher,
           clock,
           reporter,
           ..
       } = ports;
   ```

   Replace:

   ```rust
           log,
           clock,
           stop: &stop,
       };
       let dispatched = dispatcher.dispatch(
           |task, _tell| perform(task, strategy, cas, clock, &stop),
   ```

   With:

   ```rust
           log,
           clock,
           reporter,
           stop: &stop,
       };
       build.reporter.begun(graph.len());
       let dispatched = dispatcher.dispatch(
           |task, _tell| perform(task, strategy, cas, clock, &stop),
   ```

   Replace:

   ```rust
       /// The clock, shared with the workers.
       clock: &'a P::Clock,
       /// Set once nothing more may begin; the workers read it.
       stop: &'a AtomicBool,
   ```

   With:

   ```rust
       /// The clock, shared with the workers.
       clock: &'a P::Clock,
       /// What shows the build. Only this thread calls it.
       reporter: &'a mut P::Reporter,
       /// Set once nothing more may begin; the workers read it.
       stop: &'a AtomicBool,
   ```

   Replace:

   ```rust
       }

       /// Keeps `outcome` as the outcome of `node`, and appends its event to the log.
       fn conclude(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
           let (Some(target), Some(slot)) = (
   ```

   With:

   ```rust
       }

       /// Keeps `outcome` as the outcome of `node`, appends its event to the log, and only then
       /// has it reported.
       fn conclude(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
           let (Some(target), Some(slot)) = (
   ```

   Replace:

   ```rust
           };
           *slot = Some(outcome.clone());
           append(
               self.log,
               self.clock,
               &target.node.provenance,
               &target.action.command,
               progress,
               outcome,
           )
       }

       /// What the build comes to, once the dispatcher has returned with `dispatched`: the error
   ```

   With:

   ```rust
           };
           *slot = Some(outcome.clone());
           let event = append(
               self.log,
               self.clock,
               &target.node.provenance,
               &target.action.command,
               progress,
               outcome,
           )?;
           self.report(&event)
       }

       /// Reports `event` with what its action printed, read back from the content store: for a
       /// target that ran as for one that is cached.
       fn report(&mut self, event: &OutcomeEvent) -> Result<()> {
           let label = &event.outcome.label;
           let stdout = shown(self.cas, label, event.outcome.stdout)?;
           let stderr = shown(self.cas, label, event.outcome.stderr)?;
           self.reporter.finished(
               event,
               Captured {
                   stdout: &stdout,
                   stderr: &stderr,
               },
           );
           Ok(())
       }

       /// What the build comes to, once the dispatcher has returned with `dispatched`: the error
   ```

   Replace:

   ```rust
               return Err(Error::DispatcherFault { targets });
           }
           Ok(self.outcomes.into_iter().flatten().collect())
       }
   }
   ```

   With:

   ```rust
               return Err(Error::DispatcherFault { targets });
           }
           let outcomes: Vec<ActionOutcome> = self.outcomes.into_iter().flatten().collect();
           self.reporter.summary(Summary::of(&outcomes));
           Ok(outcomes)
       }
   }
   ```

7. Run the flows of the file and the unit tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   $ cargo test -p buildl-core --lib execute::record
   test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 534 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `123 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/src/execute/record.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): report each outcome with what its action printed"
   ```

### Task 7 — Report each action as a worker begins it

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flows: the exact calls of a build of four targets, and of one in which an action is ended by a signal and what depends on it is skipped. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, Reported, Script, file, ports, program, target_with,
       workspace,
   };
   ```

   With:

   ```rust
   };

   use crate::common::{
       Ends, FakeContent, FakePorts, FakeSource, Reported, Script, file, ports, program, target_with,
       workspace,
   };
   ```

   Replace:

   ```rust
   }

   fn finished(raw: &str, class: OutcomeClass, stdout: &str, stderr: &str) -> Reported {
       Reported::Finished {
   ```

   With:

   ```rust
   }

   fn started(raw: &str) -> Reported {
       Reported::Started(label(raw), None)
   }

   fn finished(raw: &str, class: OutcomeClass, stdout: &str, stderr: &str) -> Reported {
       Reported::Finished {
   ```

   Replace:

   ```rust
           stderr: stderr.as_bytes().to_vec(),
       }
   }

   #[test]
   ```

   With:

   ```rust
           stderr: stderr.as_bytes().to_vec(),
       }
   }

   const ALL_BUILT: Summary = Summary {
       failed: 0,
       skipped: 0,
       built: 4,
       cached: 0,
   };

   #[test]
   fn a_build_reports_each_action_as_it_begins_and_ends_and_the_summary_last() {
       let mut pipeline = chain();

       build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
           pipeline.ports().reporter.calls,
           [
               Reported::Begun(4),
               started("//:docs"),
               finished("//:docs", OutcomeClass::Built, "//:docs\n", ""),
               started("//:gen"),
               finished("//:gen", OutcomeClass::Built, "//:gen\n", ""),
               started("//:lib"),
               finished("//:lib", OutcomeClass::Built, "//:lib\n", ""),
               started("//:app"),
               finished("//:app", OutcomeClass::Built, "//:app\n", ""),
               Reported::Summary(ALL_BUILT),
           ]
       );
   }

   #[test]
   ```

   Replace:

   ```rust
               }),
           ]
       );
   }
   ```

   With:

   ```rust
               }),
           ]
       );
   }

   #[test]
   fn a_failure_and_what_it_skips_are_reported_and_a_skipped_target_never_starts() {
       let mut pipeline = chain();
       pipeline.ports_mut().strategy.scripts.insert(
           label("//:gen"),
           Script {
               ends: Ends::Signaled(11),
               stderr: Some(b"segmentation fault\n".to_vec()),
               ..Script::default()
           },
       );

       let outcomes = pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &Overrides::new(),
               OnFailure::KeepGoing,
           )
           .unwrap();

       let killed = OutcomeClass::Killed { signal: 11 };
       let because = || OutcomeClass::Skipped {
           because: label("//:gen"),
       };
       assert_eq!(
           pipeline.ports().reporter.calls,
           [
               Reported::Begun(4),
               started("//:docs"),
               finished("//:docs", OutcomeClass::Built, "//:docs\n", ""),
               started("//:gen"),
               finished("//:gen", killed, "//:gen\n", "segmentation fault\n"),
               finished("//:app", because(), "", ""),
               finished("//:lib", because(), "", ""),
               Reported::Summary(Summary {
                   failed: 1,
                   skipped: 2,
                   built: 1,
                   cached: 0
               }),
           ]
       );
       assert_eq!(Summary::of(&outcomes).failed, 1);
       assert!(
           !pipeline.ports().cache.rows.contains_key(&label("//:gen")),
           "a killed action leaves no row"
       );
   }
   ```

2. Run them and confirm they fail, because no `started` is reported:

   ```
   $ cargo test -p buildl-core --test flows report::
   test report::a_failure_and_what_it_skips_are_reported_and_a_skipped_target_never_starts ... FAILED
   test report::a_build_reports_each_action_as_it_begins_and_ends_and_the_summary_last ... FAILED
   assertion `left == right` failed
     left: [Begun(4), Finished { label: Label { directory: Directory(""), name: TargetName("docs") }, class: Built, …
    right: [Begun(4), Started(Label { directory: Directory(""), name: TargetName("docs") }, None), Finished { …
   test result: FAILED. 7 passed; 2 failed; 0 ignored; 0 measured; 116 filtered out
   ```

3. Have a worker say that it began an action, through what the dispatcher hands it. In `crates/buildl-core/src/execute/perform.rs`:

   Replace:

   ```rust
   //! same exit and the same collected outputs always end the same way.
   //!
   //! Responsibilities: [`Task`], [`Worked`], [`Performed`], [`Span`], [`Printed`], [`Kept`] and
   //! [`Ended`]; the decisions [`exited`], [`collected`] with [`Unkept`], and [`failed_to`]; and
   //! [`perform`], which takes one task through the ports in that fixed order.
   //!
   //! Non-responsibilities: how an action is run and where what it left is stored, which are the
   ```

   With:

   ```rust
   //! same exit and the same collected outputs always end the same way.
   //!
   //! Responsibilities: [`Task`], [`Told`], [`Worked`], [`Performed`], [`Span`], [`Printed`],
   //! [`Kept`] and [`Ended`]; the decisions [`exited`], [`collected`] with [`Unkept`], and
   //! [`failed_to`]; and [`perform`], which takes one task through the ports in that fixed order
   //! and says what it is doing while it does.
   //!
   //! Non-responsibilities: how an action is run and where what it left is stored, which are the
   ```

   Replace:

   ```rust
       Tree, TreeEntry, TreePath,
   };

   /// One action to run, as it is handed to a worker.
   ```

   With:

   ```rust
       Tree, TreeEntry, TreePath,
   };

   /// What a worker says about an action while it is still running it.
   #[derive(Debug, PartialEq, Eq)]
   pub(crate) enum Told {
       /// It began the action of this target.
       Began(NodeId),
   }

   /// One action to run, as it is handed to a worker.
   ```

   Replace:

   ```rust
   /// `cas`, and the clock is read again, however the action ended: an action a worker began
   /// always carries both instants.
   pub(crate) fn perform<S, C, K>(
       task: Task,
       strategy: &S,
       cas: &C,
       clock: &K,
       stop: &AtomicBool,
   ) -> Worked
   where
   ```

   With:

   ```rust
   /// `cas`, and the clock is read again, however the action ended: an action a worker began
   /// always carries both instants.
   ///
   /// A worker that begins an action says so through `tell`.
   pub(crate) fn perform<S, C, K>(
       task: Task,
       strategy: &S,
       cas: &C,
       clock: &K,
       stop: &AtomicBool,
       tell: &(dyn Fn(Told) + Sync),
   ) -> Worked
   where
   ```

   Replace:

   ```rust
           return Worked::NotBegun(task.node);
       }
       let started = clock.now();
       let ended = run(&task, strategy, cas);
       let finished = clock.now();
       Worked::Performed(Box::new(Performed {
           node: task.node,
           key: task.key,
           span: Span { started, finished },
   ```

   With:

   ```rust
           return Worked::NotBegun(task.node);
       }
       let node = task.node;
       tell(Told::Began(node));
       let started = clock.now();
       let ended = run(&task, strategy, cas);
       let finished = clock.now();
       Worked::Performed(Box::new(Performed {
           node,
           key: task.key,
           span: Span { started, finished },
   ```

4. Take what a worker says as a turn, and report it. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   use crate::execute::expand::{Lists, expand, expand_description};
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Task, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, shown, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   ```

   With:

   ```rust
   use crate::execute::expand::{Lists, expand, expand_description};
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Task, Told, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, shown, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   ```

   Replace:

   ```rust
   ///
   /// The build is shown through the reporter port, always from the calling thread: that it begins,
   /// once nothing can refuse it any more; each target's outcome, once its event is logged; and the
   /// count of the outcomes last, unless the build ends in an error. An outcome is reported with
   /// what its action printed, read back from the content store, for a target that ran as for one
   /// that is cached.
   ///
   /// # Errors
   ```

   With:

   ```rust
   ///
   /// The build is shown through the reporter port, always from the calling thread: that it begins,
   /// once nothing can refuse it any more; each action as a worker begins it; each target's outcome,
   /// once its event is logged; and the count of the outcomes last, unless the build ends in an
   /// error. An outcome is reported with what its action printed, read back from the content store,
   /// for a target that ran as for one that is cached.
   ///
   /// # Errors
   ```

   Replace:

   ```rust
       build.reporter.begun(graph.len());
       let dispatched = dispatcher.dispatch(
           |task, _tell| perform(task, strategy, cas, clock, &stop),
           |turn| build.step(turn),
       );
   ```

   With:

   ```rust
       build.reporter.begun(graph.len());
       let dispatched = dispatcher.dispatch(
           |task, tell| perform(task, strategy, cas, clock, &stop, tell),
           |turn| build.step(turn),
       );
   ```

   Replace:

   ```rust
   }

   impl<P: Ports> Build<'_, P> {
       /// Takes one turn: the beginning of the build, then the result of each task. Returns the
       /// tasks that became ready. A worker says nothing while it runs a task.
       ///
       /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
       /// result is dropped, and no task is returned.
       fn step(&mut self, turn: Turn<(), Worked>) -> Vec<Task> {
           if self.fatal.is_some() {
               return Vec::new();
   ```

   With:

   ```rust
   }

   impl<P: Ports> Build<'_, P> {
       /// Takes one turn: the beginning of the build, then each thing a worker says and the result
       /// of each task. Returns the tasks that became ready.
       ///
       /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
       /// message or a result is dropped, and no task is returned.
       fn step(&mut self, turn: Turn<Told, Worked>) -> Vec<Task> {
           if self.fatal.is_some() {
               return Vec::new();
   ```

   Replace:

   ```rust
                   self.drain(&mut tasks)
               }
               Turn::Told(()) => Ok(()),
               Turn::Done(Worked::NotBegun(node)) => self.abandon(node),
               Turn::Done(Worked::Performed(performed)) => self
   ```

   With:

   ```rust
                   self.drain(&mut tasks)
               }
               Turn::Told(told) => {
                   self.told(&told);
                   Ok(())
               }
               Turn::Done(Worked::NotBegun(node)) => self.abandon(node),
               Turn::Done(Worked::Performed(performed)) => self
   ```

   Replace:

   ```rust
                   Vec::new()
               }
           }
       }
   ```

   With:

   ```rust
                   Vec::new()
               }
           }
       }

       /// Reports what a worker said about an action it is running: that it began.
       fn told(&mut self, told: &Told) {
           let Told::Began(node) = *told;
           if let Some(target) = self.targets.get(node.index()) {
               self.reporter.started(&target.action);
           }
       }
   ```

5. Run the flows of the file and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `125 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/src/execute/perform.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): report each action as a worker begins it"
   ```

### Task 8 — Show one action as it prints

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/build.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flow, and give every build of the file its live-output choice: the helper `build` takes it, and each direct call of `Pipeline::build` gains `LiveOutput::Off` as its last argument. The new flow builds one workspace quietly and one live, and holds the second to the first: the same rows, events and stored bytes, with what the action printed handed over while it ran and not again. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       Digest, EntryName, Error, FileStat, Label, OnFailure, OutcomeClass, Pipeline, Scope,
       SettingName, SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
   ```

   With:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       Channel, Digest, EntryName, Error, FileStat, Label, LiveOutput, OnFailure, OutcomeClass,
       Pipeline, Scope, SettingName, SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
   ```

   Replace:

   ```rust
   }

   fn build(pipeline: &mut Pipeline<FakePorts>, scope: &Scope) -> Summary {
       let outcomes = pipeline
           .build(&entry(), scope, &Overrides::new(), OnFailure::Stop)
           .unwrap();
       Summary::of(&outcomes)
   ```

   With:

   ```rust
   }

   fn build(pipeline: &mut Pipeline<FakePorts>, scope: &Scope, live: LiveOutput) -> Summary {
       let outcomes = pipeline
           .build(&entry(), scope, &Overrides::new(), OnFailure::Stop, live)
           .unwrap();
       Summary::of(&outcomes)
   ```

   Replace:

   ```rust
   }

   const ALL_BUILT: Summary = Summary {
       failed: 0,
   ```

   With:

   ```rust
   }

   fn printed(raw: &str, channel: Channel, bytes: &str) -> Reported {
       Reported::Printed(label(raw), channel, bytes.as_bytes().to_vec())
   }

   const ALL_BUILT: Summary = Summary {
       failed: 0,
   ```

   Replace:

   ```rust
       let mut pipeline = chain();

       build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
   ```

   With:

   ```rust
       let mut pipeline = chain();

       build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

       assert_eq!(
   ```

   Replace:

   ```rust
           },
       );
       build(&mut pipeline, &Scope::Workspace);
       pipeline.ports_mut().reporter.calls.clear();

       build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
   ```

   With:

   ```rust
           },
       );
       build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);
       pipeline.ports_mut().reporter.calls.clear();

       build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

       assert_eq!(
   ```

   Replace:

   ```rust
               &Overrides::new(),
               OnFailure::KeepGoing,
           )
           .unwrap();
   ```

   With:

   ```rust
               &Overrides::new(),
               OnFailure::KeepGoing,
               LiveOutput::Off,
           )
           .unwrap();
   ```

   Replace:

   ```rust
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
       let mut pipeline = chain();
       build(&mut pipeline, &only(&["//:lib"]));

       let summary = build(&mut pipeline, &Scope::Workspace);

       assert_eq!(
   ```

   With:

   ```rust
   }

   #[test]
   fn live_output_shows_an_action_as_it_prints_and_stores_exactly_what_a_quiet_build_stores() {
       let script = || Script {
           stdout: Some(b"one\ntwo\n".to_vec()),
           stderr: Some(b"warn\n".to_vec()),
           ..Script::default()
       };
       let mut quiet = chain();
       let mut live = chain();
       for pipeline in [&mut quiet, &mut live] {
           pipeline
               .ports_mut()
               .strategy
               .scripts
               .insert(label("//:docs"), script());
       }

       build(&mut quiet, &Scope::Workspace, LiveOutput::Off);
       build(&mut live, &Scope::Workspace, LiveOutput::OneAction);

       let calls = &live.ports().reporter.calls;
       assert_eq!(
           calls.get(..6).unwrap(),
           [
               Reported::Begun(4),
               started("//:docs"),
               printed("//:docs", Channel::Stdout, "o"),
               printed("//:docs", Channel::Stderr, "warn\n"),
               printed("//:docs", Channel::Stdout, "ne\ntwo\n"),
               finished("//:docs", OutcomeClass::Built, "", ""),
           ],
           "what was shown live is not handed over again"
       );
       assert_eq!(calls.last(), Some(&Reported::Summary(ALL_BUILT)));

       assert_eq!(live.ports().cache.rows, quiet.ports().cache.rows);
       assert_eq!(live.ports().log.events, quiet.ports().log.events);
       assert_eq!(live.ports().cas.digests(), quiet.ports().cas.digests());
       assert_eq!(
           live.ports().cas.blob(&Digest::of(b"one\ntwo\n")),
           Some(b"one\ntwo\n".to_vec())
       );
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
       let mut pipeline = chain();
       build(&mut pipeline, &only(&["//:lib"]), LiveOutput::Off);

       let summary = build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

       assert_eq!(
   ```

   Replace:

   ```rust
               &only(&["//:libs"]),
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err();

       match error {
   ```

   With:

   ```rust
               &only(&["//:libs"]),
               &Overrides::new(),
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap_err();

       match error {
   ```

   Replace:

   ```rust
               &only(&["//:a"]),
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err();

       assert!(
   ```

   With:

   ```rust
               &only(&["//:a"]),
               &Overrides::new(),
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap_err();

       assert!(
   ```

   Replace:

   ```rust
           &Overrides::new(),
           OnFailure::Stop,
       );

       assert!(failed.is_err());
   ```

   With:

   ```rust
           &Overrides::new(),
           OnFailure::Stop,
           LiveOutput::Off,
       );

       assert!(failed.is_err());
   ```

   Replace:

   ```rust
       let mut pipeline = chain();

       let summary = build(&mut pipeline, &Scope::Targets(BTreeSet::new()));

       assert_eq!(
   ```

   With:

   ```rust
       let mut pipeline = chain();

       let summary = build(
           &mut pipeline,
           &Scope::Targets(BTreeSet::new()),
           LiveOutput::Off,
       );

       assert_eq!(
   ```

2. Run the flows of the file and confirm they fail to build, because a build takes no such choice:

   ```
   $ cargo test -p buildl-core --test flows report::
   error[E0061]: this method takes 4 arguments but 5 arguments were supplied
   error: could not compile `buildl-core` (test "flows") due to 5 previous errors
   ```

3. Give a worker what it shares with the scheduling thread, and the place of the one action shown live. A worker that takes the place hands the strategy a sink that says each run of bytes through the dispatcher, and its result says the place was taken. In `crates/buildl-core/src/execute/perform.rs`:

   Replace:

   ```rust
   //! same exit and the same collected outputs always end the same way.
   //!
   //! Responsibilities: [`Task`], [`Told`], [`Worked`], [`Performed`], [`Span`], [`Printed`],
   //! [`Kept`] and [`Ended`]; the decisions [`exited`], [`collected`] with [`Unkept`], and
   //! [`failed_to`]; and [`perform`], which takes one task through the ports in that fixed order
   //! and says what it is doing while it does.
   //!
   //! Non-responsibilities: how an action is run and where what it left is stored, which are the
   ```

   With:

   ```rust
   //! same exit and the same collected outputs always end the same way.
   //!
   //! Responsibilities: [`Task`], [`Told`], [`Shared`], [`Worked`], [`Performed`], [`Span`],
   //! [`Printed`], [`Kept`] and [`Ended`]; the decisions [`exited`], [`collected`] with
   //! [`Unkept`], and [`failed_to`]; and [`perform`], which takes one task through the ports in
   //! that fixed order and says what it is doing while it does.
   //!
   //! Non-responsibilities: how an action is run and where what it left is stored, which are the
   ```

   Replace:

   ```rust
   use crate::ports::{Clock, ContentStore, ExecStrategy};
   use crate::types::{
       ActionKey, Diagnostic, Digest, Exit, Found, FoundEntry, FoundKind, InfraKind, NodeId,
       OutcomeClass, OutputManifest, OutputName, OutputRefusal, RawOutcome, ReadyAction, Timestamp,
       Tree, TreeEntry, TreePath,
   };

   /// What a worker says about an action while it is still running it.
   ```

   With:

   ```rust
   use crate::ports::{Clock, ContentStore, ExecStrategy};
   use crate::types::{
       ActionKey, Channel, Diagnostic, Digest, Exit, Found, FoundEntry, FoundKind, InfraKind,
       LiveSink, NodeId, OutcomeClass, OutputManifest, OutputName, OutputRefusal, RawOutcome,
       ReadyAction, Timestamp, Tree, TreeEntry, TreePath,
   };

   /// What a worker says about an action while it is still running it.
   ```

   Replace:

   ```rust
       /// It began the action of this target.
       Began(NodeId),
   }

   /// One action to run, as it is handed to a worker.
   ```

   With:

   ```rust
       /// It began the action of this target.
       Began(NodeId),
       /// The action of this target wrote these bytes. Said only for the action shown live.
       Printed {
           /// The target.
           node: NodeId,
           /// Which output it wrote to.
           channel: Channel,
           /// What it wrote.
           bytes: Vec<u8>,
       },
   }

   /// What the thread that schedules a build shares with its workers.
   #[derive(Debug, Clone, Copy)]
   pub(crate) struct Shared<'a> {
       /// Set once nothing more may begin.
       pub(crate) stop: &'a AtomicBool,
       /// The one place of the action shown live, taken while set; `None` when no action is shown
       /// live. A worker takes it as it begins an action, and only the scheduling thread gives it
       /// back, once that action's outcome has been reported.
       pub(crate) live: Option<&'a AtomicBool>,
   }

   /// One action to run, as it is handed to a worker.
   ```

   Replace:

   ```rust
       /// When the worker began and when it was done.
       pub(crate) span: Span,
       /// How the action ended.
       pub(crate) ended: Ended,
   ```

   With:

   ```rust
       /// When the worker began and when it was done.
       pub(crate) span: Span,
       /// Whether the action held the place of the action shown live.
       pub(crate) live: bool,
       /// How the action ended.
       pub(crate) ended: Ended,
   ```

   Replace:

   ```rust
   /// Runs one task on the thread it is called from, and returns what came of it.
   ///
   /// A task handed over after `stop` was set is not begun: the clock is not read and no port is
   /// asked anything. Otherwise the clock is read, the action is taken through `strategy` and
   /// `cas`, and the clock is read again, however the action ended: an action a worker began
   /// always carries both instants.
   ///
   /// A worker that begins an action says so through `tell`.
   pub(crate) fn perform<S, C, K>(
       task: Task,
       strategy: &S,
       cas: &C,
       clock: &K,
       stop: &AtomicBool,
       tell: &(dyn Fn(Told) + Sync),
   ) -> Worked
   ```

   With:

   ```rust
   /// Runs one task on the thread it is called from, and returns what came of it.
   ///
   /// A task handed over once `shared` says nothing more may begin is not begun: the clock is not
   /// read and no port is asked anything. Otherwise the clock is read, the action is taken through
   /// `strategy` and `cas`, and the clock is read again, however the action ended: an action a
   /// worker began always carries both instants.
   ///
   /// A worker that begins an action says so through `tell`. When an action may be shown live and
   /// no other is, this one takes that place: everything it writes is said through `tell` as it is
   /// written, and the result says the place was taken. The place is not given back here.
   pub(crate) fn perform<S, C, K>(
       task: Task,
       strategy: &S,
       cas: &C,
       clock: &K,
       shared: Shared<'_>,
       tell: &(dyn Fn(Told) + Sync),
   ) -> Worked
   ```

   Replace:

   ```rust
       K: Clock,
   {
       if stop.load(Ordering::SeqCst) {
           return Worked::NotBegun(task.node);
       }
       let node = task.node;
       tell(Told::Began(node));
       let started = clock.now();
       let ended = run(&task, strategy, cas);
       let finished = clock.now();
       Worked::Performed(Box::new(Performed {
           node,
           key: task.key,
           span: Span { started, finished },
           ended,
       }))
   ```

   With:

   ```rust
       K: Clock,
   {
       if shared.stop.load(Ordering::SeqCst) {
           return Worked::NotBegun(task.node);
       }
       let live = shared.live.is_some_and(|place| {
           place
               .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
               .is_ok()
       });
       let node = task.node;
       tell(Told::Began(node));
       let printed: LiveSink<'_> = &|channel, bytes| {
           tell(Told::Printed {
               node,
               channel,
               bytes: bytes.to_vec(),
           });
       };
       let started = clock.now();
       let ended = run(&task, strategy, cas, live.then_some(printed));
       let finished = clock.now();
       Worked::Performed(Box::new(Performed {
           node,
           key: task.key,
           span: Span { started, finished },
           live,
           ended,
       }))
   ```

   Replace:

   ```rust
   /// decides it: a directory to run in, the run, storing what it printed, its exit, storing the
   /// files it left, what those are, and storing the manifest of each output.
   fn run<S: ExecStrategy, C: ContentStore>(task: &Task, strategy: &S, cas: &C) -> Ended {
       let action = &task.action;
       let dir = match strategy.prepare(action) {
           Ok(dir) => dir,
           Err(error) => return failed_to(InfraKind::Prepare, &error, None),
       };
       let raw = match strategy.run(&dir, action, None) {
           Ok(raw) => raw,
           Err(error) => return failed_to(InfraKind::Run, &error, None),
   ```

   With:

   ```rust
   /// decides it: a directory to run in, the run, storing what it printed, its exit, storing the
   /// files it left, what those are, and storing the manifest of each output.
   fn run<S: ExecStrategy, C: ContentStore>(
       task: &Task,
       strategy: &S,
       cas: &C,
       live: Option<LiveSink<'_>>,
   ) -> Ended {
       let action = &task.action;
       let dir = match strategy.prepare(action) {
           Ok(dir) => dir,
           Err(error) => return failed_to(InfraKind::Prepare, &error, None),
       };
       let raw = match strategy.run(&dir, action, live) {
           Ok(raw) => raw,
           Err(error) => return failed_to(InfraKind::Run, &error, None),
   ```

   Replace:

   ```rust
               key: task.key,
               span,
               ended: ended.clone(),
           })));
   ```

   With:

   ```rust
               key: task.key,
               span,
               live: false,
               ended: ended.clone(),
           })));
   ```

4. Give `execute` the choice. The scheduling thread reports what the live action printed, reports that target's outcome with nothing left to show, and gives the place back only once that outcome is reported. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   //!
   //! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
   //! build has stopped; the bookkeeping between two results — which targets wait to be settled,
   //! which have their outcome, and the one error that fails the whole build; having the build
   //! reported as it goes, an outcome only once its event is logged; and refusing to report a
   //! build whose dispatcher returned with a target left without an outcome.
   //!
   //! Non-responsibilities: what each step decides. Each has its own sibling file.
   ```

   With:

   ```rust
   //!
   //! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
   //! build has stopped, and the place of the one action shown as it prints; the bookkeeping
   //! between two results — which targets wait to be settled, which have their outcome, and the
   //! one error that fails the whole build; having the build reported as it goes, an outcome only
   //! once its event is logged; and refusing to report a build whose dispatcher returned with a
   //! target left without an outcome.
   //!
   //! Non-responsibilities: what each step decides. Each has its own sibling file.
   ```

   Replace:

   ```rust
   use crate::execute::expand::{Lists, expand, expand_description};
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Task, Told, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, shown, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   use crate::execute::settle::{Settled, settle};
   use crate::ports::{Dispatcher, Manifest, PortSet, Ports, Reporter};
   use crate::types::{
       ActionOutcome, Captured, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label, Node,
       NodeId, OnFailure, OutcomeEvent, OutputPath, OwnComponents, Plan, PlannedKey, ReadyAction,
       Summary, TargetGraph, Turn, Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   With:

   ```rust
   use crate::execute::expand::{Lists, expand, expand_description};
   use crate::execute::layout::Layout;
   use crate::execute::perform::{Ended, Performed, Shared, Task, Told, Worked, perform};
   use crate::execute::record::{Progress, append, failed, restore, shown, skipped, store};
   use crate::execute::schedule::{Finish, Schedule};
   use crate::execute::settle::{Settled, settle};
   use crate::ports::{Dispatcher, Manifest, PortSet, Ports, Reporter};
   use crate::types::{
       ActionOutcome, Captured, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label,
       LiveOutput, Node, NodeId, OnFailure, OutcomeEvent, OutputPath, OwnComponents, Plan, PlannedKey,
       ReadyAction, Summary, TargetGraph, Turn, Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   Replace:

   ```rust
   /// for a target that ran as for one that is cached.
   ///
   /// # Errors
   ///
   ```

   With:

   ```rust
   /// for a target that ran as for one that is cached.
   ///
   /// Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints: what it
   /// writes is reported while it runs, and its outcome is then reported with nothing left to show.
   /// What is stored and what is returned are the same under either choice.
   ///
   /// # Errors
   ///
   ```

   Replace:

   ```rust
       plan: &Plan,
       on_failure: OnFailure,
       ports: &mut PortSet<P>,
   ) -> Result<Vec<ActionOutcome>> {
   ```

   With:

   ```rust
       plan: &Plan,
       on_failure: OnFailure,
       live: LiveOutput,
       ports: &mut PortSet<P>,
   ) -> Result<Vec<ActionOutcome>> {
   ```

   Replace:

   ```rust
       let (cas, strategy, clock) = (&*cas, &*strategy, &*clock);
       let stop = AtomicBool::new(false);
       let mut build = Build::<P> {
           graph,
   ```

   With:

   ```rust
       let (cas, strategy, clock) = (&*cas, &*strategy, &*clock);
       let stop = AtomicBool::new(false);
       let place = AtomicBool::new(false);
       let shared = Shared {
           stop: &stop,
           live: match live {
               LiveOutput::Off => None,
               LiveOutput::OneAction => Some(&place),
           },
       };
       let mut build = Build::<P> {
           graph,
   ```

   Replace:

   ```rust
           clock,
           reporter,
           stop: &stop,
       };
       build.reporter.begun(graph.len());
       let dispatched = dispatcher.dispatch(
           |task, tell| perform(task, strategy, cas, clock, &stop, tell),
           |turn| build.step(turn),
       );
   ```

   With:

   ```rust
           clock,
           reporter,
           shared,
           live: None,
       };
       build.reporter.begun(graph.len());
       let dispatched = dispatcher.dispatch(
           |task, tell| perform(task, strategy, cas, clock, shared, tell),
           |turn| build.step(turn),
       );
   ```

   Replace:

   ```rust
       /// What shows the build. Only this thread calls it.
       reporter: &'a mut P::Reporter,
       /// Set once nothing more may begin; the workers read it.
       stop: &'a AtomicBool,
   }

   impl<P: Ports> Build<'_, P> {
   ```

   With:

   ```rust
       /// What shows the build. Only this thread calls it.
       reporter: &'a mut P::Reporter,
       /// What the workers read: whether anything more may begin, and the place of the action
       /// shown live.
       shared: Shared<'a>,
       /// The target whose action is being shown live: set when the first of what it printed is
       /// reported, cleared when its outcome is.
       live: Option<NodeId>,
   }

   impl<P: Ports> Build<'_, P> {
   ```

   Replace:

   ```rust
               }
               Turn::Told(told) => {
                   self.told(&told);
                   Ok(())
               }
   ```

   With:

   ```rust
               }
               Turn::Told(told) => {
                   self.told(told);
                   Ok(())
               }
   ```

   Replace:

   ```rust
               Err(error) => {
                   self.fatal = Some(error);
                   self.stop.store(true, Ordering::SeqCst);
                   Vec::new()
               }
           }
       }

       /// Reports what a worker said about an action it is running: that it began.
       fn told(&mut self, told: &Told) {
           let Told::Began(node) = *told;
           if let Some(target) = self.targets.get(node.index()) {
               self.reporter.started(&target.action);
           }
       }
   ```

   With:

   ```rust
               Err(error) => {
                   self.fatal = Some(error);
                   self.shared.stop.store(true, Ordering::SeqCst);
                   Vec::new()
               }
           }
       }

       /// Reports what a worker said about an action it is running: that it began, or what the
       /// action shown live just wrote.
       fn told(&mut self, told: Told) {
           match told {
               Told::Began(node) => {
                   if let Some(target) = self.targets.get(node.index()) {
                       self.reporter.started(&target.action);
                   }
               }
               Told::Printed {
                   node,
                   channel,
                   bytes,
               } => {
                   if let Some(target) = self.targets.get(node.index()) {
                       self.live = Some(node);
                       self.reporter.printed(&target.node.label, channel, &bytes);
                   }
               }
           }
       }
   ```

   Replace:

   ```rust
           };
           let label = target.node.label.clone();
           let progress = Progress::Began {
               key: performed.key.digest(),
   ```

   With:

   ```rust
           };
           let label = target.node.label.clone();
           let was_live = performed.live;
           let progress = Progress::Began {
               key: performed.key.digest(),
   ```

   Replace:

   ```rust
               } => failed(label, class, diagnostic, printed),
           };
           self.finish(node, progress, outcome)
       }

       /// Gives `node` its outcome, and tells the schedule how it finished: every target that
   ```

   With:

   ```rust
               } => failed(label, class, diagnostic, printed),
           };
           self.finish(node, progress, outcome)?;
           if was_live && let Some(place) = self.shared.live {
               // Given back only now, with the outcome reported: the next action to take the
               // place cannot have anything it prints shown before this one has ended.
               place.store(false, Ordering::SeqCst);
           }
           Ok(())
       }

       /// Gives `node` its outcome, and tells the schedule how it finished: every target that
   ```

   Replace:

   ```rust
           }
           if self.schedule.stopped().is_some() {
               self.stop.store(true, Ordering::SeqCst);
           }
           self.queue.extend(effects.started);
   ```

   With:

   ```rust
           }
           if self.schedule.stopped().is_some() {
               self.shared.stop.store(true, Ordering::SeqCst);
           }
           self.queue.extend(effects.started);
   ```

   Replace:

   ```rust
               outcome,
           )?;
           self.report(&event)
       }

       /// Reports `event` with what its action printed, read back from the content store: for a
   ```

   With:

   ```rust
               outcome,
           )?;
           self.show(node, &event)
       }

       /// Reports the event of `node`. When `node` is the target shown live, its event is reported
       /// with nothing left to show, since all of it was shown as it was written.
       fn show(&mut self, node: NodeId, event: &OutcomeEvent) -> Result<()> {
           if self.live == Some(node) {
               self.reporter.finished(event, Captured::default());
               self.live = None;
               return Ok(());
           }
           self.report(event)
       }

       /// Reports `event` with what its action printed, read back from the content store: for a
   ```

5. Give `Pipeline::build` the choice and pass it on. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
   use crate::resolve::{narrow, resolve};
   use crate::types::{
       ActionOutcome, Declaration, EntryName, OnFailure, Plan, Scope, SettingName, SettingValue,
       TargetGraph,
   };

   /// The pipeline over one chosen implementation of each port.
   ```

   With:

   ```rust
   use crate::resolve::{narrow, resolve};
   use crate::types::{
       ActionOutcome, Declaration, EntryName, LiveOutput, OnFailure, Plan, Scope, SettingName,
       SettingValue, TargetGraph,
   };

   /// The pipeline over one chosen implementation of each port.
   ```

   Replace:

   ```rust
       /// for, and builds that part.
       ///
       /// `overrides` gives values to build settings for this run, and `on_failure` says what a
       /// failed action does to the rest of the build. The outcomes are one per target of that part,
       /// in id order; a target that failed or was skipped is an outcome, not an error. A target
       /// outside `scope` is left alone: it has no outcome, and its row and outputs stay.
       ///
       /// # Errors
   ```

   With:

   ```rust
       /// for, and builds that part.
       ///
       /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
       /// action does to the rest of the build, and `live` whether one running action at a time is
       /// shown as it prints. The outcomes are one per target of that part, in id order; a target
       /// that failed or was skipped is an outcome, not an error. A target outside `scope` is left
       /// alone: it has no outcome, and its row and outputs stay.
       ///
       /// # Errors
   ```

   Replace:

   ```rust
           overrides: &BTreeMap<SettingName, SettingValue>,
           on_failure: OnFailure,
       ) -> Result<Vec<ActionOutcome>> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&whole, &graph, &plan, on_failure, &mut self.ports)
       }
   }
   ```

   With:

   ```rust
           overrides: &BTreeMap<SettingName, SettingValue>,
           on_failure: OnFailure,
           live: LiveOutput,
       ) -> Result<Vec<ActionOutcome>> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
           execute(&whole, &graph, &plan, on_failure, live, &mut self.ports)
       }
   }
   ```

6. Name the choice in every build the existing flows make: each call of `build` gains `LiveOutput::Off` as its last argument. In `crates/buildl-core/tests/flows/build.rs`:

   Replace:

   ```rust
   use std::path::Path;

   use buildl_core::Scope;
   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileMode, FileStat, Freshness,
   ```

   With:

   ```rust
   use std::path::Path;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileMode, FileStat, Freshness,
   ```

   Replace:

   ```rust
       json,
   };

   use crate::common::{
   ```

   With:

   ```rust
       json,
   };
   use buildl_core::{LiveOutput, Scope};

   use crate::common::{
   ```

   Replace:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
       pipeline
           .build(&entry(), &Scope::Workspace, &Overrides::new(), on_failure)
           .unwrap()
   }
   ```

   With:

   ```rust
   fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
       pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &Overrides::new(),
               on_failure,
               LiveOutput::Off,
           )
           .unwrap()
   }
   ```

   Replace:

   ```rust
               &Overrides::new(),
               OnFailure::KeepGoing,
           )
           .unwrap_err();
   ```

   With:

   ```rust
               &Overrides::new(),
               OnFailure::KeepGoing,
               LiveOutput::Off,
           )
           .unwrap_err();
   ```

   Replace:

   ```rust
               &Overrides::new(),
               OnFailure::Stop,
           )
           .unwrap_err()
   ```

   With:

   ```rust
               &Overrides::new(),
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap_err()
   ```

   Replace:

   ```rust
       )]);
       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap()
   }
   ```

   With:

   ```rust
       )]);
       pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &overrides,
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap()
   }
   ```

   Replace:

   ```rust
               pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

               let error = pipeline
                   .build(&entry(), &Scope::Workspace, &Overrides::new(), on_failure)
                   .unwrap_err();

               assert_eq!(
   ```

   With:

   ```rust
               pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

               let error = pipeline
                   .build(
                       &entry(),
                       &Scope::Workspace,
                       &Overrides::new(),
                       on_failure,
                       LiveOutput::Off,
                   )
                   .unwrap_err();

               assert_eq!(
   ```

   Replace:

   ```rust
                   &Overrides::new(),
                   OnFailure::Stop,
               )
               .unwrap_err();
   ```

   With:

   ```rust
                   &Overrides::new(),
                   OnFailure::Stop,
                   LiveOutput::Off,
               )
               .unwrap_err();
   ```

   Replace:

   ```rust
       )]);
       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       )]);
       pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &overrides,
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap();

       assert_eq!(
   ```

   Replace:

   ```rust
       build(&mut defaulted, OnFailure::Stop);
       overridden
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       build(&mut defaulted, OnFailure::Stop);
       overridden
           .build(
               &entry(),
               &Scope::Workspace,
               &overrides,
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap();

       assert_eq!(
   ```

   Replace:

   ```rust
       )]);

       pipeline
           .build(&entry(), &Scope::Workspace, &overrides, OnFailure::Stop)
           .unwrap();

       assert_eq!(
   ```

   With:

   ```rust
       )]);

       pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &overrides,
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap();

       assert_eq!(
   ```

7. Run the flows of the file, then every flow, and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   $ cargo test -p buildl-core --test flows
   test result: ok. 126 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   With the fake dispatcher's default pace no target ends while another is shown live, so nothing is reported in the middle of what an action prints. The next task holds that for a build in which several actions are under way at once.

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `126 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/src/execute/perform.rs crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/build.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): show one action as it prints"
   ```

### Task 9 — Hold the outcomes of the others while one action is shown live

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flow. With every waiting task under way together, `//:gen` ends before `//:docs`, which is shown live: its outcome is logged at once and reported after that of `//:docs`. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   };

   use crate::common::{
       Ends, FakeContent, FakePorts, FakeSource, Reported, Script, file, ports, program, target_with,
       workspace,
   };

   type Overrides = BTreeMap<SettingName, SettingValue>;
   ```

   With:

   ```rust
   };

   use crate::common::{
       Ends, FakeContent, FakePorts, FakeSource, Pace, Reported, Script, file, ports, program,
       target_with, workspace,
   };

   type Overrides = BTreeMap<SettingName, SettingValue>;
   ```

   Replace:

   ```rust
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
   ```

   With:

   ```rust
   }

   /// Whether, from the first `Printed` for a target until that target's `Finished`, every
   /// `Printed` and `Finished` is about that target.
   fn never_interleaved(calls: &[Reported]) -> bool {
       let mut live: Option<&Label> = None;
       calls.iter().all(|call| match call {
           Reported::Printed(label, ..) => {
               let same = live.is_none_or(|open| open == label);
               live = Some(label);
               same
           }
           Reported::Finished { label, .. } => {
               let same = live.is_none_or(|open| open == label);
               live = None;
               same
           }
           Reported::Begun(_) | Reported::Started(..) | Reported::Summary(_) => true,
       })
   }

   #[test]
   fn while_one_action_is_shown_live_the_outcomes_of_the_others_wait_for_it_to_end() {
       let mut pipeline = chain();
       pipeline.ports_mut().dispatcher.pace = Pace::Together;

       build(&mut pipeline, &Scope::Workspace, LiveOutput::OneAction);

       let calls = &pipeline.ports().reporter.calls;
       assert_eq!(
           calls.get(..7).unwrap(),
           [
               Reported::Begun(4),
               started("//:docs"),
               printed("//:docs", Channel::Stdout, "/"),
               printed("//:docs", Channel::Stdout, "/:docs\n"),
               started("//:gen"),
               finished("//:docs", OutcomeClass::Built, "", ""),
               finished("//:gen", OutcomeClass::Built, "//:gen\n", ""),
           ],
           "//:gen ended first, and is reported once //:docs, shown live, has ended"
       );
       assert!(never_interleaved(calls), "{calls:?}");
       assert_eq!(calls.last(), Some(&Reported::Summary(ALL_BUILT)));
       let logged: Vec<String> = pipeline
           .ports()
           .log
           .events
           .iter()
           .map(|event| event.outcome.label.to_string())
           .collect();
       assert_eq!(
           logged.get(..2).unwrap(),
           ["//:gen", "//:docs"],
           "the log is written as each target ends, whatever is being shown"
       );
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
   ```

2. Run it and confirm it fails, because the outcome of `//:gen` is reported as soon as it is logged:

   ```
   $ cargo test -p buildl-core --test flows report::
   test report::while_one_action_is_shown_live_the_outcomes_of_the_others_wait_for_it_to_end ... FAILED
   assertion `left == right` failed: //:gen ended first, and is reported once //:docs, shown live, has ended
   test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 116 filtered out
   ```

3. Hold the event of a target that ends while another is shown live, and report the held events, oldest first, once that one's outcome is. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   /// Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints: what it
   /// writes is reported while it runs, and its outcome is then reported with nothing left to show.
   /// What is stored and what is returned are the same under either choice.
   ///
   /// # Errors
   ```

   With:

   ```rust
   /// Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints: what it
   /// writes is reported while it runs, and its outcome is then reported with nothing left to show.
   /// The outcome of a target that ends meanwhile is logged at once and reported after that
   /// action's. What is stored and what is returned are the same under either choice.
   ///
   /// # Errors
   ```

   Replace:

   ```rust
           shared,
           live: None,
       };
       build.reporter.begun(graph.len());
   ```

   With:

   ```rust
           shared,
           live: None,
           held: Vec::new(),
       };
       build.reporter.begun(graph.len());
   ```

   Replace:

   ```rust
       /// reported, cleared when its outcome is.
       live: Option<NodeId>,
   }

   impl<P: Ports> Build<'_, P> {
   ```

   With:

   ```rust
       /// reported, cleared when its outcome is.
       live: Option<NodeId>,
       /// The events of targets that ended while another was being shown live, oldest first:
       /// logged already, and reported once that one has ended.
       held: Vec<OutcomeEvent>,
   }

   impl<P: Ports> Build<'_, P> {
   ```

   Replace:

   ```rust
               outcome,
           )?;
           self.show(node, &event)
       }

       /// Reports the event of `node`. When `node` is the target shown live, its event is reported
       /// with nothing left to show, since all of it was shown as it was written.
       fn show(&mut self, node: NodeId, event: &OutcomeEvent) -> Result<()> {
           if self.live == Some(node) {
               self.reporter.finished(event, Captured::default());
               self.live = None;
               return Ok(());
           }
           self.report(event)
       }

       /// Reports `event` with what its action printed, read back from the content store: for a
   ```

   With:

   ```rust
               outcome,
           )?;
           self.show(node, event)
       }

       /// Reports the event of `node`, at once unless another target is being shown live.
       ///
       /// While one is, the event is held, so that nothing is shown in the middle of what that
       /// action prints. When `node` is the one shown live, its event is reported with nothing
       /// left to show, since all of it was shown as it was written, and then every held event is
       /// reported, oldest first.
       fn show(&mut self, node: NodeId, event: OutcomeEvent) -> Result<()> {
           match self.live {
               Some(live) if live != node => {
                   self.held.push(event);
                   Ok(())
               }
               Some(_) => {
                   self.reporter.finished(&event, Captured::default());
                   self.live = None;
                   let held = core::mem::take(&mut self.held);
                   held.iter().try_for_each(|event| self.report(event))
               }
               None => self.report(&event),
           }
       }

       /// Reports `event` with what its action printed, read back from the content store: for a
   ```

4. Run the flows of the file and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `127 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): hold the outcomes of the others while one action is shown live"
   ```

### Task 10 — Record the graph of the whole workspace on every build

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/report.rs`

**Steps:**

1. Write the failing flows: a build of one target records the whole workspace's graph, in the bytes `graph` gives, and builds only what that target needs; a graph that cannot be recorded fails the build before anything is planned. In `crates/buildl-core/tests/flows/report.rs`:

   Replace:

   ```rust
   use buildl_core::{
       Channel, Digest, EntryName, Error, FileStat, Label, LiveOutput, OnFailure, OutcomeClass,
       Pipeline, Scope, SettingName, SettingValue, SourcePath, Summary, Timestamp, Written, json,
   };

   use crate::common::{
   ```

   With:

   ```rust
   use buildl_core::{
       Channel, Digest, EntryName, Error, FileStat, Label, LiveOutput, OnFailure, OutcomeClass,
       Pipeline, PortName, Scope, SettingName, SettingValue, SourcePath, Summary, Timestamp, Written,
       json,
   };

   use crate::common::{
   ```

   Replace:

   ```rust
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
       let mut pipeline = chain();
   ```

   With:

   ```rust
   }

   #[test]
   fn a_build_of_one_target_builds_what_it_needs_and_nothing_else() {
       let mut pipeline = chain();
       // What the targets outside the request read does not have to exist.
       for unneeded in ["main.c", "README"] {
           let path = SourcePath::parse(unneeded).unwrap();
           pipeline.ports_mut().digester.files.remove(&path);
       }
       let whole = json::canonical::to_vec(&pipeline.graph(&entry()).unwrap()).unwrap();

       let outcomes = pipeline
           .build(
               &entry(),
               &only(&["//:lib"]),
               &Overrides::new(),
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap();

       let built: Vec<String> = outcomes
           .iter()
           .map(|outcome| outcome.label.to_string())
           .collect();
       assert_eq!(built, ["//:gen", "//:lib"]);
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.ran(), [label("//:gen"), label("//:lib")]);
       assert_eq!(
           ports.cache.rows.keys().collect::<Vec<_>>(),
           [&label("//:gen"), &label("//:lib")]
       );
       assert_eq!(ports.log.events.len(), 2);
       assert_eq!(
           ports.reporter.calls.last(),
           Some(&Reported::Summary(Summary {
               failed: 0,
               skipped: 0,
               built: 2,
               cached: 0
           }))
       );
       assert_eq!(
           ports.graph.recorded,
           [whole],
           "the graph recorded is the whole workspace's, in the bytes `graph` gives"
       );
   }

   #[test]
   fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
       let mut pipeline = chain();
   ```

   Replace:

   ```rust
   }

   #[test]
   fn a_build_that_fails_as_a_whole_reports_no_summary() {
       let mut pipeline = chain();
   ```

   With:

   ```rust
   }

   #[test]
   fn a_graph_that_cannot_be_recorded_fails_the_build_before_anything_is_planned() {
       let mut pipeline = chain();
       pipeline.ports_mut().graph.broken = true;

       let error = pipeline
           .build(
               &entry(),
               &Scope::Workspace,
               &Overrides::new(),
               OnFailure::Stop,
               LiveOutput::Off,
           )
           .unwrap_err();

       assert!(
           matches!(
               error,
               Error::Adapter {
                   port: PortName::GraphRecord,
                   ..
               }
           ),
           "expected the graph record's failure, got {error:?}"
       );
       assert_eq!(pipeline.ports().tools.lookups(), [], "nothing was planned");
   }

   #[test]
   fn a_build_that_fails_as_a_whole_reports_no_summary() {
       let mut pipeline = chain();
   ```

2. Run them and confirm they fail, because nothing is recorded:

   ```
   $ cargo test -p buildl-core --test flows report::
   test report::a_build_of_one_target_builds_what_it_needs_and_nothing_else ... FAILED
   test report::a_graph_that_cannot_be_recorded_fails_the_build_before_anything_is_planned ... FAILED
   assertion `left == right` failed: the graph recorded is the whole workspace's, in the bytes `graph` gives
     left: []
   called `Result::unwrap_err()` on an `Ok` value: [ActionOutcome { label: Label { directory: Directory(""), name: TargetName("app") }, class: Built, …
   test result: FAILED. 11 passed; 2 failed; 0 ignored; 0 measured; 116 filtered out
   ```

3. Record the whole graph right after Resolve, before the scope is looked at. In `crates/buildl-core/src/pipeline/driver.rs`:

   Replace:

   ```rust
   use crate::load::{declaration_order, load};
   use crate::plan::plan;
   use crate::ports::{DeclarationSource, PortSet, Ports};
   use crate::resolve::{narrow, resolve};
   use crate::types::{
   ```

   With:

   ```rust
   use crate::load::{declaration_order, load};
   use crate::plan::plan;
   use crate::ports::{DeclarationSource, GraphRecord, PortSet, Ports};
   use crate::resolve::{narrow, resolve};
   use crate::types::{
   ```

   Replace:

   ```rust
       }

       /// Loads the workspace once, resolves it, plans a build of the part of the graph `scope` asks
       /// for, and builds that part.
       ///
       /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
   ```

   With:

   ```rust
       }

       /// Loads the workspace once, resolves it, records the graph of the whole workspace, plans a
       /// build of the part of the graph `scope` asks for, and builds that part.
       ///
       /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
   ```

   Replace:

   ```rust
       /// alone: it has no outcome, and its row and outputs stay.
       ///
       /// # Errors
       ///
       /// Returns the first of: whatever the load returns; whatever [`resolve`] returns;
       /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias;
       /// whatever [`plan`](fn@plan) returns; and whatever [`execute`](fn@execute) returns: two
       /// outputs of the workspace that would be written to one place, whether or not both targets
       /// were asked for, a command that names no program or one outside the workspace's ceiling
       /// once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
   ```

   With:

   ```rust
       /// alone: it has no outcome, and its row and outputs stay.
       ///
       /// The graph is recorded through the graph-record port before anything is planned, and it is
       /// the whole workspace's whatever `scope` asks for, so the record never depends on which
       /// targets one build was asked for.
       ///
       /// # Errors
       ///
       /// Returns the first of: whatever the load returns; whatever [`resolve`] returns; the
       /// graph-record port's failure; [`Error::UnknownTarget`] for a label of `scope` that names
       /// neither a target nor an alias; whatever [`plan`](fn@plan) returns; and whatever
       /// [`execute`](fn@execute) returns: two outputs of the workspace that would be written to one
       /// place, whether or not both targets were asked for, a command that names no program or one
       /// outside the workspace's ceiling once it is expanded, or a port's own failure.
       pub fn build(
           &mut self,
   ```

   Replace:

   ```rust
       ) -> Result<Vec<ActionOutcome>> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
   ```

   With:

   ```rust
       ) -> Result<Vec<ActionOutcome>> {
           let whole = resolve(load(&self.ports.source, entry)?)?;
           self.ports.graph.record(&whole)?;
           let graph = narrow(&whole, scope)?;
           let plan = plan(&graph, overrides, &mut self.ports)?;
   ```

4. Run the flows of the file and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows report::
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `129 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/report.rs
   $ git commit -m "feat(buildl-core): record the graph of the whole workspace on every build"
   ```

### Task 11 — Say in the crate's documentation what it holds now

**Files:**
- Modify `crates/buildl-core/src/lib.rs`

This task changes prose only, so it has no failing test; the doc step of the gate checks every link in the rustdoc.

**Steps:**

1. Bring the module table and the status text of the crate's documentation up to what the crate does. `crates/buildl-core/README.md` says the same things for a reader of the repository and is not touched here: plan `12` changes it. In `crates/buildl-core/src/lib.rs`:

   Replace:

   ```rust
   //! |---|---|
   //! | [`error`] | the one error enum and the crate-wide `Result` alias |
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan, ready actions, outcomes, log events — and the as-written values a build file stages |
   //! | [`json`] | the one canonical serializer every byte of output goes through, and the one reader that takes such bytes back |
   //! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each, with the set that holds one instance of each |
   //! | [`load`](mod@load) | Load, the first phase: build files to validated, sorted declarations |
   //! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph |
   //! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
   //! | [`execute`](mod@execute) | Execute and Record, the last phase: a plan to one outcome per target, each result recorded as the last step of its action |
   //! | [`pipeline`] | the phases run in sequence over one bundle of ports, one method per command |
   //!
   //! Each phase has its own module, named for the phase. Five rules keep that arrangement
   ```

   With:

   ```rust
   //! |---|---|
   //! | [`error`] | the one error enum and the crate-wide `Result` alias |
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan, ready actions, outcomes, log events, what one run was asked for — and the as-written values a build file stages |
   //! | [`json`] | the one canonical serializer every byte of output goes through, and the one reader that takes such bytes back |
   //! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each, with the set that holds one instance of each |
   //! | [`load`](mod@load) | Load, the first phase: build files to validated, sorted declarations |
   //! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph, and that graph cut down to the targets one run was asked for |
   //! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
   //! | [`execute`](mod@execute) | Execute and Record, the last phase: a plan to one outcome per target, each result recorded as the last step of its action and reported once it is logged |
   //! | [`pipeline`] | the phases run in sequence over one bundle of ports, one method per command, and the two commands that reach only build files as functions over a source alone |
   //!
   //! Each phase has its own module, named for the phase. Five rules keep that arrangement
   ```

   Replace:

   ```rust
   //! a failure does to the rest of the build is the caller's [`OnFailure`].
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.
   ```

   With:

   ```rust
   //! a failure does to the rest of the build is the caller's [`OnFailure`].
   //!
   //! A run works on the whole workspace or on the targets a [`Scope`] names, with everything they
   //! depend on: [`narrow`](fn@narrow) cuts the graph down before Plan, so [`Pipeline::plan`] and
   //! [`Pipeline::build`] neither plan nor build a target outside it, while two outputs claiming one
   //! place are still refused across the whole workspace. A build writes down the graph of the whole
   //! workspace through the [`GraphRecord`] port before it plans, and is shown through the
   //! [`Reporter`] port as it goes: that it began, each action as a worker begins it, each target's
   //! outcome with what its action printed — for a cached target as for one that ran — and the count
   //! last. Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints. An
   //! action a signal ended fails its target as [`OutcomeClass::Killed`], and an input that is not a
   //! regular file is refused by Plan from what the [`Digester`] reports as an [`InputStat`].
   //! [`Hasher`] takes the digest of bytes handed in pieces. [`pipeline::check`] and
   //! [`pipeline::graph`] run the first two commands for a caller that holds only a
   //! [`DeclarationSource`].
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.
   ```

2. Run the doctests and confirm they pass:

   ```
   $ cargo test -p buildl-core --doc
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

3. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `553 passed` (unit tests), `129 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

4. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs
   $ git commit -m "docs(buildl-core): say in the crate documentation what it holds now"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0` after every task; `crates/buildl-core/Cargo.toml`, `Cargo.lock`, `crates/expected-edges.txt`, `crates/expected-module-edges.txt` (24 edges) and the three files under `tests/flows/golden/` are unchanged, and no file outside `crates/buildl-core` changes. The crate still names no I/O, process, thread, environment or clock API: `guard-core-purity` passes with every ban on.
- `narrow`, `pipeline::check`, `pipeline::graph`, `Pipeline::plan(entry, scope, overrides)`, `Pipeline::build(entry, scope, overrides, on_failure, live)` and `execute(whole, graph, plan, on_failure, live, ports)` have the signatures of spec §2.6 and §2.7.
- The error order of `build`: Load, Resolve, the graph record, `UnknownTarget`, Plan's, `ForeignPlan`, the manifest, `OutputConflict` over the whole graph, `NoProgram` or `Refused`, then as before.
- A reporter is called on the calling thread only, in the order `begun`, any number of `started`, `printed` and `finished`, then `summary`; a build refused before it runs makes no call, and one that ends in an error makes no `summary`. From the first `printed` for a target until that target's `finished`, no `printed` and no `finished` is about another target.
- Every event is appended to the log before any byte is read for display. What a build stores, records and returns is the same under `LiveOutput::Off` and `LiveOutput::OneAction`.
- A target outside the scope has no outcome, no event, no report call and no count, and its row and outputs stay.
- Tests of `buildl-core`: 553 unit tests (the 544 plan `02` left, six of `narrow` and three of `shown`); 129 flows (the 114 there were, the thirteen of `tests/flows/report.rs` and the two of `check.rs`); 13 doctests.
- What the adapter plans and the composition root are written against:

  | Item | Public API |
  |---|---|
  | `buildl_core::narrow` | `fn narrow<'g>(graph: &'g TargetGraph, scope: &Scope) -> Result<Cow<'g, TargetGraph>>` |
  | `buildl_core::pipeline::check` | `fn check<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>>` |
  | `buildl_core::pipeline::graph` | `fn graph<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<TargetGraph>` |
  | `Pipeline::check`, `Pipeline::graph` | unchanged: `(&self, entry: &EntryName)` |
  | `Pipeline::plan` | `(&mut self, entry: &EntryName, scope: &Scope, overrides: &BTreeMap<SettingName, SettingValue>) -> Result<Plan>` |
  | `Pipeline::build` | `(&mut self, entry: &EntryName, scope: &Scope, overrides: &BTreeMap<SettingName, SettingValue>, on_failure: OnFailure, live: LiveOutput) -> Result<Vec<ActionOutcome>>` |
  | `buildl_core::execute` | `fn execute<P: Ports>(whole: &TargetGraph, graph: &TargetGraph, plan: &Plan, on_failure: OnFailure, live: LiveOutput, ports: &mut PortSet<P>) -> Result<Vec<ActionOutcome>>` |
