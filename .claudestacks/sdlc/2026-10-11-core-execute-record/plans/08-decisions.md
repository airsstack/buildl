---
status: done
created: 2026-10-11
depends-on: [01, 02, 03, 06, 07]
---

# Decisions Implementation Plan

**Goal:** Every decision between a target starting and its outcome being logged is a function over plain values.

**Architecture:** Three more logic files in `execute/`, none of which takes a port. `execute/settle.rs` answers whether a target that may start must run: `finished_key` finishes the target's key from the rows its dependencies' builds recorded, and `is_cached` says the recorded result still stands when the target is not declared to run always, its row has that key, and everything the row names is still stored — asked through a closure, as `plan/verdict.rs` asks for a row. `execute/perform.rs` holds what crosses between the scheduling thread and a worker — `Task` going out, `Worked` coming back, both whole values that borrow nothing — and decides how an action ended from what a worker saw: `exited` from the exit, `collected` from what was found at each output, `failed_to` from a port's error. `Ended::Produced` carries the outputs together with what the action printed, so a result that produced its outputs without its printed output being stored cannot be built. `execute/record.rs` builds what is kept: the cache row, the outcome for each way a target's build can end, and the log event, whose key and two instants follow from `Progress`, how far the target got. The thin functions that call a port — run the action, put an output in place, store the row, append the event — are not here: they arrive beside these decisions with the function that runs a build, and the flows that test them.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D6, D8, D9, D12, D14, D18, D19), §3.4 (which fields an outcome sets), §3.5 (the event's `key`, `started` and `finished`), §5.6 (settling), §5.7 (one action on a worker), §5.8 (recording), §5.9 (the diagnostic of a dependency with no row), §9 (the rows for `execute/settle.rs`, `execute/perform.rs` and `execute/record.rs`).

**Checkpoints:** review once, when all three tasks are done.

---

## Context an implementer needs

What plans `01`, `02`, `03` and `06` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `OwnComponents`, `KeyComponents { own, deps }`, `DepProducts { label, outputs: Vec<Produced> }`, `ActionKey::new(KeyComponents) -> Result<ActionKey>`, `ActionKey::digest()` | `crates/buildl-core/src/types/action_key.rs:69`, `:90`, `:57`, `:144`, `:154` | what `finished_key` builds |
| `CacheRow::new(key, outputs, stdout, stderr) -> Option<CacheRow>` (`None` unless one digest per declared output), `key()`, `stdout()`, `stderr()`, `digests()` (outputs in order, then `stdout`, then `stderr`), `produced()` | `crates/buildl-core/src/types/cache_row.rs:35`, `:51`, `:57`, `:63`, `:72`, `:82` | what `finished_key` and `is_cached` read; what `row` builds; what a succeeded outcome is taken from |
| `Freshness { Cached, Always }` (`Copy`) | `crates/buildl-core/src/types/declaration.rs:42` | the one thing about a target `is_cached` needs beside its key |
| `Error::Adapter { port: PortName, diagnostic: Diagnostic }`, rendered `{port}: {diagnostic}`; `PortName::ActionCache` renders `action cache`, `ContentStore` renders `content store`, `ExecStrategy` renders `exec strategy` | `crates/buildl-core/src/error.rs:273`, `:433`–`:455` | the failure of a dependency with no row; the errors the tests hand to `failed_to`, `unstored` and `unplaced` |
| `Exit { Exited(i32), TimedOut }` (`Copy`) | `crates/buildl-core/src/types/raw_outcome.rs:20` | what `exited` decides from |
| `InfraKind { Prepare, Run, TimedOut, Store }` (`Copy`), `OutcomeClass`, `ActionOutcome { label, class, diagnostic, outputs, stdout, stderr }` | `crates/buildl-core/src/types/outcome.rs:22`, `:40`, `:79` | the classes the decisions name; what `record.rs` builds |
| `OutcomeEvent { at, provenance, command, key, started, finished, outcome }` | `crates/buildl-core/src/types/outcome_event.rs:24` | what `event` builds |
| `ReadyAction` (public fields, `Clone`, `PartialEq`) | `crates/buildl-core/src/types/ready_action.rs:24` | what a `Task` carries |
| `Diagnostic::new`, `Timestamp::from_unix_nanos`, `Provenance`, `Node::provenance` | `crates/buildl-core/src/types/` | the message, the instants and the build file of an event |
| `execute/mod.rs` declaring `admission`, `expand`, `layout` and `schedule`, each under a `cfg_attr(not(test), expect(dead_code, …))`, then `#[cfg(test)] mod fixtures;` | `crates/buildl-core/src/execute/mod.rs` | the file each task adds one `mod` line to |
| `dir`, `label`, `outputs`, `path`, `program`, `output_path`, `settings`, `command`, `node`, `graph` | `crates/buildl-core/src/execute/fixtures.rs` | the builders this plan's tests use; task 1 adds `own`, `key` and `row` |
| `execute -> error` and `execute -> types` | `crates/expected-module-edges.txt` | the edges `execute/` already has; this plan adds none |

Gate facts that bite here. A bullet that names a lint reports one seen as a failing run while this
code was prototyped:

- Nothing outside a file's own tests calls its functions until the function that runs a build is
  written, so the library build reports them as dead code and the gate fails. Each `mod` line in
  `execute/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It must be
  `cfg_attr(not(test), …)`: in the test build the functions are used, and a plain `expect` would
  fail there as unfulfilled. The plan that writes that function removes the attributes.
- In the test build a type that only tests could use must be used by one. `Task`, `Worked` and
  `Performed` are carried, not decided on, so without the test
  `a_task_and_its_result_can_be_handed_to_another_thread` the gate fails with
  `struct Task is never constructed`, `enum Worked is never used` and
  `struct Performed is never constructed`. That test also pins what a dispatcher will ask of
  them: `Send`, and no borrow.
- clippy's `option_if_let_else` rejects a `match` over an `Option` with one arm per variant, so
  `collected` ends in `map_or_else`.
- `execute/fixtures.rs` is compiled for tests only, and a fixture no test calls is dead code in
  the test build. Task 1 adds the three builders its tests use; tasks 2 and 3 use those and add
  none.
- `execute/record.rs` has a function named `row`, and so do the fixtures, so the test module of
  `record.rs` imports the fixture as `recorded`.
- The three files import only `crate::error`, `crate::types` and siblings under
  `crate::execute`, so `crates/expected-module-edges.txt` does not change. No file here names
  `crate::ports`.
- The test module of `execute/perform.rs` unwraps nothing, so it carries no
  `expect(clippy::unwrap_used)`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/execute/settle.rs     — [create] finished_key, is_cached, unit tests
crates/buildl-core/src/execute/perform.rs    — [create] Task, Worked, Performed, Span, Printed, Ended, exited, collected, failed_to, unit tests
crates/buildl-core/src/execute/record.rs     — [create] row, the outcomes, Progress, event, unit tests
crates/buildl-core/src/execute/fixtures.rs   — [modify] own, key, row
crates/buildl-core/src/execute/mod.rs        — [modify] declare the three modules
```

Each task declares its module the same way. The block to add to `crates/buildl-core/src/execute/mod.rs` is the attribute the other `mod` lines there carry, followed by the `mod` line, placed so that the `mod` lines stay in alphabetical order.

### Task 1 — Decide whether a started target must run

**Files:**
- Create `crates/buildl-core/src/execute/settle.rs`
- Modify `crates/buildl-core/src/execute/fixtures.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Add the builders this task's tests use: `own`, `key` and `row`. `crates/buildl-core/src/execute/fixtures.rs` becomes:

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
       ActionKey, Argument, CacheRow, Command, Digest, Directory, Freshness, KeyComponents, Label,
       NetworkAccess, Node, NodeId, OutputName, OutputPath, OwnComponents, Platform, Program,
       Provenance, SettingName, SettingValue, SourcePath, TargetGraph, TargetRole,
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

   /// The own key components of a target running `cc` on `linux/x86_64` and declaring the outputs
   /// `names`, with nothing else.
   pub(crate) fn own(names: &[&str]) -> OwnComponents {
       OwnComponents {
           command: command(&["cc"]),
           inputs: Vec::new(),
           outputs: outputs(names),
           env: BTreeMap::new(),
           tool: None,
           settings: BTreeMap::new(),
           network: NetworkAccess::Sealed,
           platform: Platform::parse("linux/x86_64").unwrap(),
       }
   }

   /// The key of a target with the components [`own`] gives `names` and no dependency.
   pub(crate) fn key(names: &[&str]) -> ActionKey {
       ActionKey::new(KeyComponents {
           own: own(names),
           deps: Vec::new(),
       })
       .unwrap()
   }

   /// The row of a build under [`key`] in which each output's content was its own name, and the
   /// action printed `out` and `err`.
   pub(crate) fn row(names: &[&str]) -> CacheRow {
       CacheRow::new(
           key(names),
           names
               .iter()
               .map(|name| Digest::of(name.as_bytes()))
               .collect(),
           Digest::of(b"out"),
           Digest::of(b"err"),
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

2. Declare the module. In `crates/buildl-core/src/execute/mod.rs`, as the last of the phase's `mod` lines, directly above the blank line that precedes `#[cfg(test)]`, insert:

   ```rust
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod settle;
   ```

3. Write the failing tests. Create `crates/buildl-core/src/execute/settle.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{finished_key, is_cached};
       use crate::error::{Error, PortName};
       use crate::execute::fixtures::{command, key, label, own, row};
       use crate::types::{ActionKey, DepProducts, Digest, Freshness, KeyComponents};

       #[test]
       fn a_key_is_finished_from_what_each_dependency_produced() {
           let lib = label("//:lib");
           let main = label("//:main.o");
           let lib_row = row(&["lib.a", "lib.map"]);
           let main_row = row(&["main.o"]);

           let finished = finished_key(
               &own(&["app"]),
               &[(&main, Some(&main_row)), (&lib, Some(&lib_row))],
           )
           .unwrap();

           let expected = ActionKey::new(KeyComponents {
               own: own(&["app"]),
               deps: vec![
                   DepProducts {
                       label: main.clone(),
                       outputs: main_row.produced(),
                   },
                   DepProducts {
                       label: lib.clone(),
                       outputs: lib_row.produced(),
                   },
               ],
           })
           .unwrap();
           assert_eq!(finished, expected);

           let swapped = finished_key(
               &own(&["app"]),
               &[(&lib, Some(&lib_row)), (&main, Some(&main_row))],
           )
           .unwrap();
           assert_ne!(
               swapped.digest(),
               finished.digest(),
               "the dependencies are kept in the order given"
           );
       }

       #[test]
       fn a_target_that_depends_on_nothing_is_keyed_by_its_own_components() {
           assert_eq!(finished_key(&own(&["app"]), &[]).unwrap(), key(&["app"]));
       }

       #[test]
       fn a_dependency_with_no_row_is_the_action_caches_failure() {
           let lib = label("//:lib");
           let main = label("//:main.o");
           let main_row = row(&["main.o"]);
           let error = finished_key(
               &own(&["app"]),
               &[(&main, Some(&main_row)), (&lib, None), (&main, None)],
           )
           .unwrap_err();
           assert_eq!(
               error.to_string(),
               "action cache: //:lib succeeded and has no row"
           );
           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::ActionCache),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
       }

       #[test]
       fn a_target_is_cached_when_its_row_has_its_key_and_everything_the_row_names_is_held() {
           let row = row(&["app", "app.map"]);
           assert!(is_cached(
               Freshness::Cached,
               &key(&["app", "app.map"]),
               Some(&row),
               |_| true
           ));
       }

       #[test]
       fn a_target_declared_to_always_run_is_never_cached() {
           let row = row(&["app"]);
           assert!(!is_cached(
               Freshness::Always,
               &key(&["app"]),
               Some(&row),
               |_| true
           ));
       }

       #[test]
       fn a_target_with_no_row_is_not_cached() {
           assert!(!is_cached(Freshness::Cached, &key(&["app"]), None, |_| {
               true
           }));
       }

       #[test]
       fn a_target_whose_key_is_not_its_rows_is_not_cached() {
           let row = row(&["app"]);
           let mut changed = own(&["app"]);
           changed.command = command(&["cc", "-O2"]);
           let now = ActionKey::new(KeyComponents {
               own: changed,
               deps: Vec::new(),
           })
           .unwrap();
           assert!(!is_cached(Freshness::Cached, &now, Some(&row), |_| true));
       }

       #[test]
       fn a_target_is_not_cached_when_any_digest_of_its_row_is_gone() {
           let row = row(&["app", "app.map"]);
           let key = key(&["app", "app.map"]);
           for (gone, what) in [
               (Digest::of(b"app"), "the first output"),
               (Digest::of(b"app.map"), "the second output"),
               (Digest::of(b"out"), "what the action printed"),
               (Digest::of(b"err"), "what the action printed as errors"),
           ] {
               assert!(
                   !is_cached(Freshness::Cached, &key, Some(&row), |digest| *digest
                       != gone),
                   "{what}"
               );
           }
           assert!(
               is_cached(Freshness::Cached, &key, Some(&row), |digest| *digest
                   != Digest::of(b"something else")),
               "content the row does not name"
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::settle
   error[E0432]: unresolved imports `super::finished_key`, `super::is_cached`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/execute/settle.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Settling a target that may start: must it run, or does what its last build recorded still
   //! stand?
   //!
   //! Its own file because the question can only be answered once a target's dependencies have
   //! finished: a key holds what each dependency produced, and that is known only then. So the
   //! key is finished here, during the build, and compared with the recorded one.
   //!
   //! Responsibilities: [`finished_key`] and [`is_cached`].
   //!
   //! Non-responsibilities: reading a row, and asking whether content is still stored. Both
   //! arrive through ports; the decisions here are over the values those ports returned.

   use crate::error::{Error, PortName, Result};
   use crate::types::{
       ActionKey, CacheRow, DepProducts, Diagnostic, Digest, Freshness, KeyComponents, Label,
       OwnComponents,
   };

   /// The key of a target whose dependencies have all succeeded: `own`, and what each dependency
   /// produced.
   ///
   /// `deps` holds each dependency's label with the row its build recorded, in the order the
   /// dependencies were written.
   ///
   /// # Errors
   ///
   /// Returns [`Error::Adapter`], naming [`PortName::ActionCache`], for the first dependency with
   /// no row: a dependency that succeeded was recorded or found recorded, so its row must exist.
   /// Returns [`Error::CanonicalJson`] when the key cannot be serialized.
   pub(crate) fn finished_key(
       own: &OwnComponents,
       deps: &[(&Label, Option<&CacheRow>)],
   ) -> Result<ActionKey> {
       let deps = deps
           .iter()
           .map(|(label, row)| {
               row.map(|row| DepProducts {
                   label: (*label).clone(),
                   outputs: row.produced(),
               })
               .ok_or_else(|| Error::Adapter {
                   port: PortName::ActionCache,
                   diagnostic: Diagnostic::new(format!("{label} succeeded and has no row")),
               })
           })
           .collect::<Result<Vec<_>>>()?;
       ActionKey::new(KeyComponents {
           own: own.clone(),
           deps,
       })
   }

   /// Whether a target need not run: what its last build recorded still stands.
   ///
   /// It does when the target is not declared to run on every build, `row` exists, `key` has the
   /// digest of the row's key, and `held` answers true for every digest the row holds: each
   /// output, and what the action printed. A row whose content is gone is no reason to skip the
   /// action, since there would be nothing to put in place.
   pub(crate) fn is_cached(
       freshness: Freshness,
       key: &ActionKey,
       row: Option<&CacheRow>,
       held: impl Fn(&Digest) -> bool,
   ) -> bool {
       freshness != Freshness::Always
           && row.is_some_and(|row| {
               row.key().digest() == key.digest() && row.digests().all(|digest| held(&digest))
           })
   }
   ```

   `finished_key` takes each dependency's label with its row already looked up, so it asks no port: its caller reads the rows. A dependency reaches this function only after it succeeded, and a target that succeeded was recorded by this build or found recorded, so a missing row is the cache breaking its own promise, not something about the target. `is_cached` compares digests of keys, which is what makes a target whose dependency was rebuilt to the same bytes cached, and asks `held` about every digest of the row, so a row whose content has left the store sends the target back to run.

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::settle
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 393 filtered out; finished in 0.01s
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

8. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): decide whether a started target must run"
   ```

### Task 2 — Decide how an action ended on a worker

**Files:**
- Create `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/execute/mod.rs`, directly below the `mod layout;` line, insert:

   ```rust
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod perform;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/execute/perform.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use std::collections::BTreeMap;

       use super::{Ended, Performed, Printed, Span, Task, Worked, collected, exited, failed_to};
       use crate::error::{Error, PortName};
       use crate::execute::fixtures::{command, key, label, output_path, outputs};
       use crate::types::{
           Diagnostic, Digest, Exit, InfraKind, NetworkAccess, NodeId, OutcomeClass, ReadyAction,
           Timestamp,
       };

       fn printed() -> Printed {
           Printed {
               stdout: Digest::of(b"compiling\n"),
               stderr: Digest::of(b""),
           }
       }

       fn strategy_error(text: &str) -> Error {
           Error::Adapter {
               port: PortName::ExecStrategy,
               diagnostic: Diagnostic::new(text),
           }
       }

       /// Stands for handing a value to another thread: it must be `Send` and borrow nothing.
       fn handed_over<T: Send + 'static>(value: T) -> T {
           value
       }

       #[test]
       fn an_exit_with_status_zero_decides_nothing_yet() {
           assert_eq!(exited(Exit::Exited(0), printed()), None);
       }

       #[test]
       fn an_exit_with_another_status_is_the_actions_failure() {
           for status in [1, 2, -1, 255] {
               assert_eq!(
                   exited(Exit::Exited(status), printed()),
                   Some(Ended::Failed {
                       class: OutcomeClass::ActionFailure { status },
                       diagnostic: None,
                       printed: Some(printed()),
                   })
               );
           }
       }

       #[test]
       fn a_timeout_is_the_builders_failure_and_carries_no_message() {
           assert_eq!(
               exited(Exit::TimedOut, printed()),
               Some(Ended::Failed {
                   class: OutcomeClass::InfraFailure {
                       kind: InfraKind::TimedOut,
                   },
                   diagnostic: None,
                   printed: Some(printed()),
               })
           );
       }

       #[test]
       fn a_ports_failure_is_the_builders_and_its_text_is_the_message() {
           let error = strategy_error("//:app: no directory to run in");
           for (kind, printed) in [
               (InfraKind::Prepare, None),
               (InfraKind::Run, None),
               (InfraKind::Store, None),
               (InfraKind::Store, Some(printed())),
           ] {
               assert_eq!(
                   failed_to(kind, &error, printed),
                   Ended::Failed {
                       class: OutcomeClass::InfraFailure { kind },
                       diagnostic: Some(Diagnostic::new(
                           "exec strategy: //:app: no directory to run in"
                       )),
                       printed,
                   }
               );
           }
       }

       #[test]
       fn an_action_that_left_every_output_produced_them() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           assert_eq!(
               collected(
                   &outputs(&["app", "app.map"]),
                   &[Some(app), Some(map)],
                   printed()
               ),
               Ended::Produced {
                   outputs: vec![app, map],
                   printed: printed(),
               }
           );
           assert_eq!(
               collected(&[], &[], printed()),
               Ended::Produced {
                   outputs: Vec::new(),
                   printed: printed(),
               },
               "a target that declares no output"
           );
       }

       #[test]
       fn an_answer_of_the_wrong_length_is_the_builders_failure() {
           let app = Digest::of(b"app");
           for (entries, text) in [
               (vec![Some(app)], "the strategy answered for 1 outputs of 2"),
               (
                   vec![Some(app), None, Some(app)],
                   "the strategy answered for 3 outputs of 2",
               ),
               (Vec::new(), "the strategy answered for 0 outputs of 2"),
           ] {
               assert_eq!(
                   collected(&outputs(&["app", "app.map"]), &entries, printed()),
                   Ended::Failed {
                       class: OutcomeClass::InfraFailure {
                           kind: InfraKind::Store,
                       },
                       diagnostic: Some(Diagnostic::new(text)),
                       printed: Some(printed()),
                   }
               );
           }
       }

       #[test]
       fn an_output_the_action_did_not_leave_is_named() {
           let names = outputs(&["app", "app.map", "app.sym"]);
           let app = Digest::of(b"app");
           let missing = |name: &str| Ended::Failed {
               class: OutcomeClass::OutputMissing {
                   name: outputs(&[name]).remove(0),
               },
               diagnostic: None,
               printed: Some(printed()),
           };
           assert_eq!(
               collected(&names, &[Some(app), None, Some(app)], printed()),
               missing("app.map"),
               "one missing"
           );
           assert_eq!(
               collected(&names, &[Some(app), None, None], printed()),
               missing("app.map"),
               "two missing: the first, in the order the outputs were written"
           );
           assert_eq!(
               collected(&names, &[None, None, None], printed()),
               missing("app"),
               "none left"
           );
       }

       #[test]
       fn a_task_and_its_result_can_be_handed_to_another_thread() {
           let node = NodeId::new(3);
           let span = Span {
               started: Timestamp::from_unix_nanos(10),
               finished: Timestamp::from_unix_nanos(20),
           };
           let action = ReadyAction {
               label: label("//:app"),
               command: command(&["cc", "-o", "out/app"]),
               env: BTreeMap::new(),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: vec![output_path("//:app", "app")],
               network: NetworkAccess::Sealed,
           };

           let task = handed_over(Task {
               node,
               key: key(&["app"]),
               action: action.clone(),
           });
           assert_eq!(task.node, node);
           assert_eq!(task.key, key(&["app"]));
           assert_eq!(task.action, action);

           match handed_over(Worked::NotBegun(task.node)) {
               Worked::NotBegun(not_begun) => assert_eq!(not_begun, node),
               Worked::Performed(performed) => unreachable!("expected NotBegun, got {performed:?}"),
           }

           let ended = Ended::Produced {
               outputs: vec![Digest::of(b"app")],
               printed: printed(),
           };
           let worked = handed_over(Worked::Performed(Box::new(Performed {
               node: task.node,
               key: task.key,
               span,
               ended: ended.clone(),
           })));
           match worked {
               Worked::Performed(performed) => {
                   assert_eq!(performed.node, node);
                   assert_eq!(performed.key, key(&["app"]));
                   assert_eq!(performed.span.started, Timestamp::from_unix_nanos(10));
                   assert_eq!(performed.span.finished, Timestamp::from_unix_nanos(20));
                   assert_eq!(performed.ended, ended);
               }
               Worked::NotBegun(not_begun) => unreachable!("expected Performed, got {not_begun:?}"),
           }
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::perform
   error[E0432]: unresolved imports `super::Ended`, `super::Performed`, `super::Printed`, `super::Span`, `super::Task`, `super::Worked`, `super::collected`, `super::exited`, `super::failed_to`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/execute/perform.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! One action on a worker: what is handed over, what comes back, and how the action ended.
   //!
   //! Its own file because an action is run away from the thread that schedules the build: a
   //! task goes out and a result comes back, and both must be whole values that borrow nothing.
   //! How the action ended is decided here from what the worker saw, in a fixed order, so the
   //! same exit and the same collected outputs always end the same way.
   //!
   //! Responsibilities: [`Task`], [`Worked`], [`Performed`], [`Span`], [`Printed`] and [`Ended`];
   //! and the decisions [`exited`], [`collected`] and [`failed_to`].
   //!
   //! Non-responsibilities: running the action and storing what it left, which arrive through
   //! ports; and recording the result, which happens on the thread that schedules the build.

   use crate::error::Error;
   use crate::types::{
       ActionKey, Diagnostic, Digest, Exit, InfraKind, NodeId, OutcomeClass, OutputName, ReadyAction,
       Timestamp,
   };

   /// One action to run, as it is handed to a worker.
   #[derive(Debug, PartialEq, Eq)]
   pub(crate) struct Task {
       /// The target.
       pub(crate) node: NodeId,
       /// The target's finished key.
       pub(crate) key: ActionKey,
       /// What to run, and where its inputs and outputs are.
       pub(crate) action: ReadyAction,
   }

   /// What a worker hands back for one task.
   #[derive(Debug, PartialEq, Eq)]
   pub(crate) enum Worked {
       /// The build had stopped before the worker began the action: nothing was run, and nothing
       /// is known but the target.
       NotBegun(NodeId),
       /// The worker began the action. Boxed, so that a task that was not begun is not as large
       /// as one that ran.
       Performed(Box<Performed>),
   }

   /// One action a worker began, with how it ended.
   #[derive(Debug, PartialEq, Eq)]
   pub(crate) struct Performed {
       /// The target.
       pub(crate) node: NodeId,
       /// The key the task was handed over with.
       pub(crate) key: ActionKey,
       /// When the worker began and when it was done.
       pub(crate) span: Span,
       /// How the action ended.
       pub(crate) ended: Ended,
   }

   /// When a worker began an action, and when it was done with it.
   ///
   /// One value and not two optional instants: an action a worker began has both, and a target no
   /// worker began has neither.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) struct Span {
       /// Read before the worker did anything for the action.
       pub(crate) started: Timestamp,
       /// Read after the last thing the worker did for the action.
       pub(crate) finished: Timestamp,
   }

   /// The digests of what an action printed, once both are stored.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) struct Printed {
       /// What it wrote to standard output.
       pub(crate) stdout: Digest,
       /// What it wrote to standard error.
       pub(crate) stderr: Digest,
   }

   /// How an action a worker began ended, before anything about it is recorded.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) enum Ended {
       /// It exited with status zero and left every output it declares, and all of it is stored.
       Produced {
           /// The digest of each declared output, in declared order.
           outputs: Vec<Digest>,
           /// What it printed.
           printed: Printed,
       },
       /// It did not get that far.
       Failed {
           /// Why.
           class: OutcomeClass,
           /// A message for display, when the builder itself failed and said how.
           diagnostic: Option<Diagnostic>,
           /// What it printed, when the action ran and that was stored.
           printed: Option<Printed>,
       },
   }

   /// How an action ended because a port failed with `error` while the builder did `kind` for it.
   ///
   /// The error's text becomes the message. `printed` is what the action printed, when it had
   /// already run and that was stored.
   pub(crate) fn failed_to(kind: InfraKind, error: &Error, printed: Option<Printed>) -> Ended {
       Ended::Failed {
           class: OutcomeClass::InfraFailure { kind },
           diagnostic: Some(Diagnostic::new(error.to_string())),
           printed,
       }
   }

   /// How an action that ended with `exit` ended, when the exit alone decides it.
   ///
   /// `None` for an exit with status zero: what the action left then decides. An action that ran
   /// out of time is the builder's failure; any other status is the action's own.
   pub(crate) const fn exited(exit: Exit, printed: Printed) -> Option<Ended> {
       let class = match exit {
           Exit::Exited(0) => return None,
           Exit::Exited(status) => OutcomeClass::ActionFailure { status },
           Exit::TimedOut => OutcomeClass::InfraFailure {
               kind: InfraKind::TimedOut,
           },
       };
       Some(Ended::Failed {
           class,
           diagnostic: None,
           printed: Some(printed),
       })
   }

   /// How an action that exited with status zero ended, from what was found at each of its
   /// outputs.
   ///
   /// `names` are the outputs the target declares and `entries` what was found for them, in the
   /// same order: a digest, or `None` where the action left nothing. An answer that does not hold
   /// one entry per output is the builder's failure. Otherwise the first output with nothing
   /// there, in declared order, is the one named as missing.
   pub(crate) fn collected(
       names: &[OutputName],
       entries: &[Option<Digest>],
       printed: Printed,
   ) -> Ended {
       if entries.len() != names.len() {
           return Ended::Failed {
               class: OutcomeClass::InfraFailure {
                   kind: InfraKind::Store,
               },
               diagnostic: Some(Diagnostic::new(format!(
                   "the strategy answered for {} outputs of {}",
                   entries.len(),
                   names.len()
               ))),
               printed: Some(printed),
           };
       }
       names
           .iter()
           .zip(entries)
           .find_map(|(name, entry)| entry.is_none().then_some(name))
           .map_or_else(
               || Ended::Produced {
                   outputs: entries.iter().flatten().copied().collect(),
                   printed,
               },
               |missing| Ended::Failed {
                   class: OutcomeClass::OutputMissing {
                       name: missing.clone(),
                   },
                   diagnostic: None,
                   printed: Some(printed),
               },
           )
   }
   ```

   The three functions are the rows of one table, tried top to bottom by whoever runs the action: a port's error while preparing, running or storing is `failed_to`; an exit that decides by itself is `exited`; and only an exit with status zero reaches `collected`. `exited` and `collected` take what the action printed as a plain `Printed`, not an `Option`: both are reached only after the action ran and its two streams were stored. `failed_to` takes an `Option` because it serves both sides of that point. A timeout carries no message, and a missing output is the action's doing, so it carries none either.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::perform
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 401 filtered out; finished in 0.00s
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): decide how an action ended on a worker"
   ```

### Task 3 — Build the row, the outcome and the event of a target

**Files:**
- Create `crates/buildl-core/src/execute/record.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/execute/mod.rs`, directly below the `mod perform;` line, insert:

   ```rust
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod record;
   ```

   With the plan that writes the schedule landed, `crates/buildl-core/src/execute/mod.rs` now reads:

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
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod perform;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod record;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod schedule;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod settle;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/execute/record.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{
           Progress, built, cached, event, failed, mismatched, row, skipped, unplaced, unstored,
       };
       use crate::error::{Error, PortName};
       use crate::execute::fixtures::{command, key, label, node, row as recorded};
       use crate::execute::perform::{Printed, Span};
       use crate::types::{
           ActionOutcome, Diagnostic, Digest, InfraKind, OutcomeClass, OutcomeEvent, OutputName,
           Timestamp,
       };

       fn printed() -> Printed {
           Printed {
               stdout: Digest::of(b"compiling\n"),
               stderr: Digest::of(b""),
           }
       }

       fn store_error(text: &str) -> Error {
           Error::Adapter {
               port: PortName::ContentStore,
               diagnostic: Diagnostic::new(text),
           }
       }

       /// The outcome of `//:app` with `class` and nothing else.
       fn bare(class: OutcomeClass) -> ActionOutcome {
           ActionOutcome {
               label: label("//:app"),
               class,
               diagnostic: None,
               outputs: Vec::new(),
               stdout: None,
               stderr: None,
           }
       }

       const STORE: OutcomeClass = OutcomeClass::InfraFailure {
           kind: InfraKind::Store,
       };

       #[test]
       fn a_row_holds_the_key_the_outputs_and_what_was_printed() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           let row = row(key(&["app", "app.map"]), vec![app, map], printed()).unwrap();
           assert_eq!(row.key(), &key(&["app", "app.map"]));
           assert_eq!(
               row.digests().collect::<Vec<_>>(),
               [app, map, printed().stdout, printed().stderr]
           );
       }

       #[test]
       fn a_row_is_not_made_for_the_wrong_number_of_outputs() {
           let app = Digest::of(b"app");
           assert_eq!(row(key(&["app", "app.map"]), vec![app], printed()), None);
           assert_eq!(row(key(&["app"]), vec![app, app], printed()), None);
       }

       #[test]
       fn a_built_targets_outcome_is_what_its_row_holds() {
           let row = recorded(&["app", "app.map"]);
           assert_eq!(
               built(label("//:app"), &row),
               ActionOutcome {
                   outputs: row.produced(),
                   stdout: Some(row.stdout()),
                   stderr: Some(row.stderr()),
                   ..bare(OutcomeClass::Built)
               }
           );
       }

       #[test]
       fn a_cached_targets_outcome_is_taken_from_its_row() {
           let row = recorded(&["app", "app.map"]);
           assert_eq!(
               cached(label("//:app"), &row),
               ActionOutcome {
                   outputs: row.produced(),
                   stdout: Some(row.stdout()),
                   stderr: Some(row.stderr()),
                   ..bare(OutcomeClass::Cached)
               }
           );
       }

       #[test]
       fn a_failure_on_a_worker_keeps_its_class_and_has_no_outputs() {
           let missing = OutcomeClass::OutputMissing {
               name: OutputName::parse("app.map").unwrap(),
           };
           for class in [OutcomeClass::ActionFailure { status: 2 }, missing] {
               assert_eq!(
                   failed(label("//:app"), class.clone(), None, Some(printed())),
                   ActionOutcome {
                       stdout: Some(printed().stdout),
                       stderr: Some(printed().stderr),
                       ..bare(class)
                   }
               );
           }

           let prepare = OutcomeClass::InfraFailure {
               kind: InfraKind::Prepare,
           };
           let message = Diagnostic::new("exec strategy: //:app: no directory to run in");
           assert_eq!(
               failed(
                   label("//:app"),
                   prepare.clone(),
                   Some(message.clone()),
                   None
               ),
               ActionOutcome {
                   diagnostic: Some(message),
                   ..bare(prepare)
               },
               "an action that never ran printed nothing"
           );
       }

       #[test]
       fn a_built_target_that_could_not_be_stored_is_the_builders_failure() {
           assert_eq!(
               unstored(
                   label("//:app"),
                   &store_error("out/app cannot be written"),
                   printed()
               ),
               ActionOutcome {
                   diagnostic: Some(Diagnostic::new("content store: out/app cannot be written")),
                   stdout: Some(printed().stdout),
                   stderr: Some(printed().stderr),
                   ..bare(STORE)
               }
           );
       }

       #[test]
       fn a_cached_target_that_could_not_be_placed_reports_what_its_row_printed() {
           let row = recorded(&["app"]);
           assert_eq!(
               unplaced(
                   label("//:app"),
                   &store_error("out/app cannot be written"),
                   &row
               ),
               ActionOutcome {
                   diagnostic: Some(Diagnostic::new("content store: out/app cannot be written")),
                   stdout: Some(row.stdout()),
                   stderr: Some(row.stderr()),
                   ..bare(STORE)
               }
           );
       }

       #[test]
       fn a_row_that_does_not_match_its_key_is_the_builders_failure() {
           assert_eq!(
               mismatched(label("//:app"), printed()),
               ActionOutcome {
                   diagnostic: Some(Diagnostic::new("the row does not match its key")),
                   stdout: Some(printed().stdout),
                   stderr: Some(printed().stderr),
                   ..bare(STORE)
               }
           );
       }

       #[test]
       fn a_skipped_target_names_the_failure_and_nothing_else() {
           assert_eq!(
               skipped(label("//:app"), label("//lib:text")),
               bare(OutcomeClass::Skipped {
                   because: label("//lib:text"),
               })
           );
       }

       #[test]
       fn an_event_says_how_far_the_target_got() {
           let at = Timestamp::from_unix_nanos(30);
           let node = node("//lib:text", |_| {});
           let command = command(&["cc", "-c", "lib/a.c", "-o", "out/lib/text"]);
           let key = Digest::of(b"key");
           let span = Span {
               started: Timestamp::from_unix_nanos(10),
               finished: Timestamp::from_unix_nanos(20),
           };
           let outcome = bare(OutcomeClass::Built);
           let logged = |progress| event(at, &node.provenance, &command, progress, outcome.clone());
           let expected = |key, started, finished| OutcomeEvent {
               at,
               provenance: node.provenance.clone(),
               command: command.clone(),
               key,
               started,
               finished,
               outcome: outcome.clone(),
           };

           assert_eq!(
               logged(Progress::Began { key, span }),
               expected(Some(key), Some(span.started), Some(span.finished)),
               "a target a worker began has its key and both instants"
           );
           assert_eq!(
               logged(Progress::Keyed { key }),
               expected(Some(key), None, None),
               "a target no worker began has its key and no instant"
           );
           assert_eq!(
               logged(Progress::Skipped),
               expected(None, None, None),
               "a skipped target has neither"
           );
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::record
   error[E0432]: unresolved imports `super::Progress`, `super::built`, `super::cached`, `super::event`, `super::failed`, `super::mismatched`, `super::row`, `super::skipped`, `super::unplaced`, `super::unstored`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/execute/record.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Recording: the row, the outcome and the log event of one target, built from values.
   //!
   //! Its own file because recording is the last step of building a target and the one a later
   //! build depends on: the row is what the next build compares against, the outcome is what the
   //! build hands back, and the event is what the log keeps. Each is put together here in one
   //! place, so that what a class of outcome carries is decided once.
   //!
   //! Responsibilities: [`row`]; the outcomes [`built`], [`cached`], [`failed`], [`unstored`],
   //! [`unplaced`], [`mismatched`] and [`skipped`]; and [`event`] with [`Progress`].
   //!
   //! Non-responsibilities: putting an output in place, storing a row and appending an event,
   //! which arrive through ports; and the order those happen in.

   use crate::error::Error;
   use crate::execute::perform::{Printed, Span};
   use crate::types::{
       ActionKey, ActionOutcome, CacheRow, Command, Diagnostic, Digest, InfraKind, Label,
       OutcomeClass, OutcomeEvent, Provenance, Timestamp,
   };

   /// The class of a target whose outputs, or whose row, could not be stored or put in place.
   const NOT_STORED: OutcomeClass = OutcomeClass::InfraFailure {
       kind: InfraKind::Store,
   };

   /// How far the build of one target got, which decides what its event can say.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) enum Progress {
       /// A worker began its action.
       Began {
           /// The digest of the key the action ran under.
           key: Digest,
           /// When the worker began and when it was done.
           span: Span,
       },
       /// Its key was finished and no worker began it: what its last build recorded still stood.
       Keyed {
           /// The digest of that key.
           key: Digest,
       },
       /// It was skipped. A skipped target is logged without a key, whether or not one had been
       /// finished for it.
       Skipped,
   }

   /// The row to record for an action that ran under `key`, left `outputs` and printed `printed`;
   /// `None` when `outputs` does not hold one digest per output the key declares.
   pub(crate) fn row(key: ActionKey, outputs: Vec<Digest>, printed: Printed) -> Option<CacheRow> {
       CacheRow::new(key, outputs, printed.stdout, printed.stderr)
   }

   /// The outcome of a target that ran and whose `row` was recorded.
   pub(crate) fn built(label: Label, row: &CacheRow) -> ActionOutcome {
       succeeded(label, OutcomeClass::Built, row)
   }

   /// The outcome of a target that did not run: everything it reports is what `row`, recorded by
   /// its last build, holds.
   pub(crate) fn cached(label: Label, row: &CacheRow) -> ActionOutcome {
       succeeded(label, OutcomeClass::Cached, row)
   }

   /// The outcome of a target whose action did not get as far as producing its outputs: `class`
   /// and `diagnostic` as the worker decided them, and what the action printed when that was
   /// stored.
   pub(crate) fn failed(
       label: Label,
       class: OutcomeClass,
       diagnostic: Option<Diagnostic>,
       printed: Option<Printed>,
   ) -> ActionOutcome {
       ActionOutcome {
           label,
           class,
           diagnostic,
           outputs: Vec::new(),
           stdout: printed.map(|printed| printed.stdout),
           stderr: printed.map(|printed| printed.stderr),
       }
   }

   /// The outcome of a target that ran and produced its outputs, when a port then failed with
   /// `error` while an output was put in place or the row was stored.
   pub(crate) fn unstored(label: Label, error: &Error, printed: Printed) -> ActionOutcome {
       failed(
           label,
           NOT_STORED,
           Some(Diagnostic::new(error.to_string())),
           Some(printed),
       )
   }

   /// The outcome of a target that did not need to run, when a port failed with `error` while one
   /// of its outputs was put in place. What it printed is taken from `row`.
   pub(crate) fn unplaced(label: Label, error: &Error, row: &CacheRow) -> ActionOutcome {
       unstored(
           label,
           error,
           Printed {
               stdout: row.stdout(),
               stderr: row.stderr(),
           },
       )
   }

   /// The outcome of a target that ran, when no row could be made from its key and what it left.
   pub(crate) fn mismatched(label: Label, printed: Printed) -> ActionOutcome {
       failed(
           label,
           NOT_STORED,
           Some(Diagnostic::new("the row does not match its key")),
           Some(printed),
       )
   }

   /// The outcome of a target that was never begun because the target `because` failed.
   pub(crate) fn skipped(label: Label, because: Label) -> ActionOutcome {
       failed(label, OutcomeClass::Skipped { because }, None, None)
   }

   /// The log event for `outcome`, recorded at `at`, of a target declared in `provenance` whose
   /// expanded command is `command`.
   ///
   /// `progress` decides the rest: the key's digest is logged unless the target was skipped, and
   /// the two instants only when a worker began the action.
   pub(crate) fn event(
       at: Timestamp,
       provenance: &Provenance,
       command: &Command,
       progress: Progress,
       outcome: ActionOutcome,
   ) -> OutcomeEvent {
       let (key, span) = match progress {
           Progress::Began { key, span } => (Some(key), Some(span)),
           Progress::Keyed { key } => (Some(key), None),
           Progress::Skipped => (None, None),
       };
       OutcomeEvent {
           at,
           provenance: provenance.clone(),
           command: command.clone(),
           key,
           started: span.map(|span| span.started),
           finished: span.map(|span| span.finished),
           outcome,
       }
   }

   /// The outcome of a target whose outputs are in place: `class`, with what `row` holds.
   fn succeeded(label: Label, class: OutcomeClass, row: &CacheRow) -> ActionOutcome {
       ActionOutcome {
           label,
           class,
           diagnostic: None,
           outputs: row.produced(),
           stdout: Some(row.stdout()),
           stderr: Some(row.stderr()),
       }
   }
   ```

   Which outcome a target gets is decided by who calls which function; what each outcome carries is decided here, once. `built` and `cached` take everything from the row, so a cached target reports exactly what the build that recorded it did. `unstored` is for a target that ran: what it printed is known from its worker. `unplaced` is for a target that did not run: what it printed is known only from its row. `event` takes `Progress` in place of an optional key and two optional instants, so an event with one instant, or a skipped target logged with a key, cannot be written down.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::record
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 409 filtered out; finished in 0.00s
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): build the row, the outcome and the event of a target"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- A key is finished from each dependency's label and what its row says it produced, in the order
  given; a dependency with no row is `Error::Adapter` naming the action cache, rendered
  `action cache: //:lib succeeded and has no row`.
- A target is cached exactly when it is not declared to run always, it has a row, its key's
  digest is the row's key's, and every digest of the row is held; each of those failing alone
  makes it run.
- An exit with status zero decides nothing; another status is `ActionFailure` with that status;
  a timeout is `InfraFailure { TimedOut }` with no message; a port's error is `InfraFailure` of
  the kind given, with the error's text as the message.
- An answer for the wrong number of outputs is `InfraFailure { Store }` with
  `the strategy answered for <n> outputs of <m>`; with an output missing it is `OutputMissing`
  naming the first one in declared order; otherwise the outputs' digests, with what was printed.
- A task and a worker's result are `Send` and borrow nothing.
- A row is made only for one digest per declared output. A built and a cached outcome carry what
  the row holds; a failure on a worker keeps its class and has no outputs; a target that ran and
  could not be stored, a cached target that could not be placed, and a row that does not match
  its key are all `InfraFailure { Store }`, the last with `the row does not match its key`; a
  cached target that could not be placed reports what its row printed; a skipped target names
  the failure and nothing else.
- An event has its key and both instants for a target a worker began, its key and no instant for
  one no worker began, and neither for a skipped one.
- `crates/expected-module-edges.txt` is unchanged, and no file under `execute/` names
  `crate::ports`, `crate::load`, `crate::resolve`, `crate::plan` or `crate::pipeline`.

## Review findings

Reviewed once, 2026-10-12, when all three tasks were done. The reviewer re-ran the gate:
`cargo make dod` exit 0 (`[cargo-make] INFO - Build Done in 5.02 seconds.`), `cargo deny check`
exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan:
`settle.rs`, `perform.rs`, `record.rs`, `fixtures.rs` and `mod.rs` are line for line the plan's
final code blocks.

- nit — the module doc says how an action ended "is decided here … in a fixed order", while the
  file holds three separate functions and the order between them belongs to a caller not yet
  written — `execute/perform.rs:5`. Left as it is: plan 09 adds that caller, which makes the
  sentence true.
- nit — `failed_to(kind: InfraKind, ..)` accepts `InfraKind::TimedOut`, and `Ended::Failed.class`
  and `record::failed(class)` accept `Built` and `Cached`; no caller should pass those —
  `execute/perform.rs:101`. Left as it is: private to the crate, no caller does, and the shape
  is the plan's.
- nit — the test helper `printed()` is written twice — `execute/perform.rs:184`,
  `execute/record.rs:179`. Left as it is: the plan asked for exactly this.
- outside this plan — the doc of `OutcomeEvent::key` (plan 02's file) says a skipped target's
  key "was never finished"; `Progress::Skipped` here says it may have been, and spec §5.9
  agrees with this file — `types/outcome_event.rs:32`. Fixed: see Deviations.

## Probe results

No separate probe was run. The facts each task asserts about existing code are ones its own
compile and test runs depend on:

- Task 1 — red: `error[E0432]: unresolved imports `super::finished_key`, `super::is_cached`` at
  `execute/settle.rs:10:17`. Green: `test result: ok. 8 passed; 0 failed; 0 ignored; 0
  measured; 393 filtered out; finished in 0.00s` — as the plan quotes.
- Task 2 — red: `error[E0432]: unresolved imports `super::Ended`, `super::Performed`,
  `super::Printed`, `super::Span`, `super::Task`, `super::Worked`, `super::collected`,
  `super::exited`, `super::failed_to`` at `execute/perform.rs:7:17`. Green: `test result: ok. 8
  passed; 0 failed; 0 ignored; 0 measured; 401 filtered out; finished in 0.00s` — as the plan
  quotes.
- Task 3 — red: `error[E0432]: unresolved imports `super::Progress`, `super::built`,
  `super::cached`, `super::event`, `super::failed`, `super::mismatched`, `super::row`,
  `super::skipped`, `super::unplaced`, `super::unstored``. Green: `test result: ok. 10 passed; 0
  failed; 0 ignored; 0 measured; 409 filtered out; finished in 0.00s` — as the plan quotes.
- After each task: `cargo fmt --all -- --check` printed nothing, `cargo make dod` ended
  `[cargo-make] INFO - Build Done in … seconds.`, `cargo deny check` ended `advisories ok, bans
  ok, licenses ok, sources ok`.

## Deviations

- 2026-10-12 — the three tasks' commit steps were skipped. The work is in the tree,
  uncommitted; committing is left to the author.
- 2026-10-12 — this plan started while plans 06 and 07 were `executing` (built and reviewed),
  not `done`: the author asked for plans 06 to 10 in one run.
- 2026-10-12 — one doc sentence outside this plan's files was corrected after the review: in
  `types/outcome_event.rs`, the doc of `OutcomeEvent::key` now reads "`None` for a target that
  was skipped, whether or not a key had been finished for it", to agree with
  `Progress::Skipped` and spec §5.9. No code changed.
