---
status: executing
created: 2026-10-11
---

# Outcome Types Implementation Plan

**Goal:** The outcome of one target's build exists as a value, with the form the build log writes it in.

**Architecture:** Two new files under `types/`. `types/outcome.rs` holds what a build returns: `OutcomeClass`, the six ways the build of one target can end; `InfraKind`, which part of the work the builder itself could not do; `ActionOutcome`, a class with the target's label, a message for display, what the target produced and the digests of what its action printed; and `Summary`, the count of a build's outcomes. There is no class for a refusal: a build that wants something its workspace does not allow is an error before anything runs, never an outcome. `OutcomeClass::succeeded` and `Summary::of` are decisions about the type's own kind, so they live with it. `OutcomeClass` and `InfraKind` derive `Serialize`, because serde's default form for an enum — the name for a variant without fields, an object of one key otherwise — is exactly the form the log needs. `types/outcome_event.rs` holds `OutcomeEvent`, one line of the build log: an outcome with when it was recorded, the build file, the expanded command, the key's digest and the worker's two clock reads. It serializes by hand through a private view, as `types/plan.rs` does for a plan, into one flat object of twelve keys that names every target by its label, and it has no `Deserialize`: nothing here reads a log back.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D5, D12, D15, D18, D19), §3.4 (outcomes), §3.5 (the log event), §9 (the rows for `types/outcome.rs` and `types/outcome_event.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What exists that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Label`, `OutputName` (both serialize as their text) | `crates/buildl-core/src/types/` | the fields of `OutcomeClass::Skipped` and `OutcomeClass::OutputMissing` |
| `Diagnostic` (`as_str`; it does not implement `Serialize`) | `crates/buildl-core/src/types/diagnostic.rs` | `ActionOutcome::diagnostic`; an event writes it as its text |
| `Produced { name, digest }` (derives `Serialize`) | `crates/buildl-core/src/types/action_key.rs` | `ActionOutcome::outputs`, and the entries of an event's `outputs` |
| `Digest` (`Copy`; serializes and displays as lowercase hex) | `crates/buildl-core/src/types/digest.rs` | the key and the two printed-output digests |
| `Timestamp` (`Copy`; serializes as a bare number of nanoseconds) | `crates/buildl-core/src/types/timestamp.rs` | `at`, `started`, `finished` |
| `Provenance` (`Display` is the build file's path) | `crates/buildl-core/src/types/provenance.rs` | the event's `file` key |
| `Command` (serializes as the array of its arguments) | `crates/buildl-core/src/types/argument.rs` | the event's `command` key |
| `json::canonical::to_string` (object keys sorted at every level) | `crates/buildl-core/src/json/canonical.rs` | what the tests serialize through |
| `PlanView` and `impl Serialize for Plan` | `crates/buildl-core/src/types/plan.rs` | the pattern `OutcomeEvent` follows: a private `#[derive(Serialize)]` view, filled in a hand-written `serialize` |
| The Responsibilities list of the module doc | `crates/buildl-core/src/types/mod.rs` | where each task adds its bullet |
| The `pub use types::{…}` list | `crates/buildl-core/src/lib.rs` | where each task adds its names |

Gate facts that bite here:

- `crates/buildl-core/src/types/mod.rs` and `crates/buildl-core/src/lib.rs` keep their `mod`
  lines and `pub use` lists sorted. `cargo fmt --all` sorts and rewraps a `pub use a::{…}` list
  and a run of `pub mod` or `pub use` lines, so each task adds its line or name anywhere in the
  right list and lets `cargo fmt --all` place it. A task creates the file a `mod` line names
  before it runs `cargo fmt --all`.
- A doc bullet in `types/mod.rs` that links a type that does not exist yet fails `cargo make dod`
  at its rustdoc step. Each task therefore adds only the links for the types it creates.
- clippy runs with `-D warnings` at pedantic and nursery level, on test code too. Two of its
  lints were met while this code was prototyped, and the code below is what passed:
  `redundant_closure_for_method_calls` wants `.map(Diagnostic::as_str)`, not a closure calling
  `as_str`; `unnecessary_wraps` refuses a test helper that always returns `Some`, so the tests'
  `instant` helper returns a `Timestamp` and each use wraps it.
- The test filter `types::outcome` also selects the tests of `types::outcome_event` once task 2
  has landed. Task 1's quoted run was captured before task 2 existed.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/outcome.rs         — [create] InfraKind, OutcomeClass, ActionOutcome, Summary, unit tests
crates/buildl-core/src/types/outcome_event.rs   — [create] OutcomeEvent, its serialized form, unit tests
crates/buildl-core/src/types/mod.rs             — [modify] declare and re-export the two modules
crates/buildl-core/src/lib.rs                   — [modify] re-export the five types
```

### Task 1 — Add the outcome of one target and the summary of a build

**Files:**
- Create `crates/buildl-core/src/types/outcome.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod outcome;` to the `pub mod` lines and `pub use outcome::{ActionOutcome, InfraKind, OutcomeClass, Summary};` to the `pub use` lines, and add one bullet at the end of the module doc's Responsibilities list, directly above the blank `//!` line that precedes `Non-responsibilities`:

   ```rust
   //! - [`ActionOutcome`], [`OutcomeClass`] and [`InfraKind`] — how the build of one target ended;
   //!   [`Summary`] — the count of a build's outcomes.
   ```

2. Re-export them. In `crates/buildl-core/src/lib.rs`, add `ActionOutcome`, `InfraKind`, `OutcomeClass` and `Summary` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/outcome.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{ActionOutcome, InfraKind, OutcomeClass, Summary};
       use crate::json::canonical;
       use crate::types::{Label, OutputName};

       /// One class of every kind, in the order the enum declares them.
       fn every_class() -> [OutcomeClass; 6] {
           [
               OutcomeClass::Cached,
               OutcomeClass::Built,
               OutcomeClass::ActionFailure { status: 2 },
               OutcomeClass::OutputMissing {
                   name: OutputName::parse("bin/app").unwrap(),
               },
               OutcomeClass::InfraFailure {
                   kind: InfraKind::Store,
               },
               OutcomeClass::Skipped {
                   because: Label::parse("//lib:text").unwrap(),
               },
           ]
       }

       /// An outcome of `class` for `//:app`, holding nothing else.
       fn outcome(class: OutcomeClass) -> ActionOutcome {
           ActionOutcome {
               label: Label::parse("//:app").unwrap(),
               class,
               diagnostic: None,
               outputs: Vec::new(),
               stdout: None,
               stderr: None,
           }
       }

       #[test]
       fn only_a_cached_and_a_built_target_succeeded() {
           let succeeded = every_class().map(|class| class.succeeded());
           assert_eq!(succeeded, [true, true, false, false, false, false]);
       }

       #[test]
       fn every_kind_of_infrastructure_failure_is_a_failure() {
           for kind in [
               InfraKind::Prepare,
               InfraKind::Run,
               InfraKind::TimedOut,
               InfraKind::Store,
           ] {
               assert!(!OutcomeClass::InfraFailure { kind }.succeeded());
           }
       }

       #[test]
       fn a_summary_counts_each_class_once() {
           let outcomes: Vec<ActionOutcome> = every_class().into_iter().map(outcome).collect();
           assert_eq!(
               Summary::of(&outcomes),
               Summary {
                   failed: 3,
                   skipped: 1,
                   built: 1,
                   cached: 1,
               }
           );
       }

       #[test]
       fn a_summary_of_no_outcome_counts_nothing() {
           assert_eq!(
               Summary::of(&[]),
               Summary {
                   failed: 0,
                   skipped: 0,
                   built: 0,
                   cached: 0,
               }
           );
       }

       #[test]
       fn a_summary_adds_outcomes_of_one_class() {
           let outcomes = [
               outcome(OutcomeClass::Built),
               outcome(OutcomeClass::Built),
               outcome(OutcomeClass::ActionFailure { status: 1 }),
               outcome(OutcomeClass::Cached),
               outcome(OutcomeClass::Built),
           ];
           assert_eq!(
               Summary::of(&outcomes),
               Summary {
                   failed: 1,
                   skipped: 0,
                   built: 3,
                   cached: 1,
               }
           );
       }

       #[test]
       fn a_class_without_fields_serializes_as_its_name() {
           assert_eq!(
               canonical::to_string(&OutcomeClass::Cached).unwrap(),
               r#""Cached""#
           );
           assert_eq!(
               canonical::to_string(&OutcomeClass::Built).unwrap(),
               r#""Built""#
           );
       }

       #[test]
       fn a_class_with_a_field_serializes_as_an_object_of_one_key() {
           let rendered: Vec<String> = every_class()
               .iter()
               .skip(2)
               .map(|class| canonical::to_string(class).unwrap())
               .collect();
           assert_eq!(
               rendered,
               [
                   r#"{"ActionFailure":{"status":2}}"#,
                   r#"{"OutputMissing":{"name":"bin/app"}}"#,
                   r#"{"InfraFailure":{"kind":"Store"}}"#,
                   r#"{"Skipped":{"because":"//lib:text"}}"#,
               ]
           );
       }

       #[test]
       fn every_kind_of_infrastructure_failure_serializes_as_its_name() {
           let rendered: Vec<String> = [
               InfraKind::Prepare,
               InfraKind::Run,
               InfraKind::TimedOut,
               InfraKind::Store,
           ]
           .iter()
           .map(|kind| canonical::to_string(kind).unwrap())
           .collect();
           assert_eq!(
               rendered,
               [r#""Prepare""#, r#""Run""#, r#""TimedOut""#, r#""Store""#]
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::outcome
   error[E0432]: unresolved imports `super::ActionOutcome`, `super::InfraKind`, `super::OutcomeClass`, `super::Summary`
   error[E0432]: unresolved imports `outcome::ActionOutcome`, `outcome::InfraKind`, `outcome::OutcomeClass`, `outcome::Summary`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/outcome.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! How the build of one target ended, and the count of a build's outcomes.
   //!
   //! Its own file because an outcome is what a build hands back: one per target, whether the
   //! target ran, was taken from the cache, failed, or was never begun. A target that fails is an
   //! outcome and not an error, so one failure does not hide what the rest of the build did.
   //!
   //! Responsibilities: [`ActionOutcome`], [`OutcomeClass`] with [`OutcomeClass::succeeded`],
   //! [`InfraKind`], and [`Summary`] with [`Summary::of`].
   //!
   //! Non-responsibilities: deciding which class a target's build ends in, which the phase that
   //! executes does; and rendering an outcome for a person, which belongs to whoever shows it.

   use serde::Serialize;

   use crate::types::{Diagnostic, Digest, Label, OutputName, Produced};

   /// Which part of building one target the builder itself could not do.
   ///
   /// Non-exhaustive: a builder that can fail another way gains a variant.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
   #[non_exhaustive]
   pub enum InfraKind {
       /// The directory to run the action in could not be made.
       Prepare,
       /// The action could not be run.
       Run,
       /// The action ran longer than it was allowed to.
       TimedOut,
       /// An output, something the action printed, or the record of the build could not be stored
       /// or put in place.
       Store,
   }

   /// How the build of one target ended.
   ///
   /// Serializes as its name when it has no field, and otherwise as an object of one key, the
   /// name, holding the fields. Non-exhaustive: a build that can end another way gains a variant.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   #[non_exhaustive]
   pub enum OutcomeClass {
       /// It did not run: what its last build recorded still stands.
       Cached,
       /// It ran, exited with status zero, and every output it declares was stored and recorded.
       Built,
       /// It ran and exited with a status other than zero.
       ActionFailure {
           /// The exit status.
           status: i32,
       },
       /// It ran and exited with status zero, and left nothing where an output it declares should
       /// be.
       OutputMissing {
           /// The first such output, in declared order.
           name: OutputName,
       },
       /// The builder could not build it. Nothing is said about the target's own action.
       InfraFailure {
           /// What the builder could not do.
           kind: InfraKind,
       },
       /// It was never begun, because another target failed.
       Skipped {
           /// The target whose failure caused this one to be skipped.
           because: Label,
       },
   }

   impl OutcomeClass {
       /// Whether the target's outputs are in place: true for [`OutcomeClass::Cached`] and
       /// [`OutcomeClass::Built`], false for every other class.
       #[must_use]
       pub const fn succeeded(&self) -> bool {
           matches!(self, Self::Cached | Self::Built)
       }
   }

   /// How the build of one target ended, with what it left behind.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct ActionOutcome {
       /// The target.
       pub label: Label,
       /// How its build ended.
       pub class: OutcomeClass,
       /// A message for display: set for an [`OutcomeClass::InfraFailure`] other than a timeout,
       /// and `None` for every other outcome.
       pub diagnostic: Option<Diagnostic>,
       /// What the target produced, in declared order. Empty unless the target succeeded.
       pub outputs: Vec<Produced>,
       /// The digest of what the action wrote to standard output, when that was stored.
       pub stdout: Option<Digest>,
       /// The digest of what the action wrote to standard error, when that was stored.
       pub stderr: Option<Digest>,
   }

   /// How many targets of one build ended in each way.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct Summary {
       /// Targets whose action failed, whose output was missing, or that the builder could not
       /// build.
       pub failed: usize,
       /// Targets never begun because another target failed.
       pub skipped: usize,
       /// Targets that ran and were recorded.
       pub built: usize,
       /// Targets that did not need to run.
       pub cached: usize,
   }

   impl Summary {
       /// Counts `outcomes` by how each ended.
       #[must_use]
       pub fn of(outcomes: &[ActionOutcome]) -> Self {
           let mut summary = Self {
               failed: 0,
               skipped: 0,
               built: 0,
               cached: 0,
           };
           for outcome in outcomes {
               match outcome.class {
                   OutcomeClass::Cached => summary.cached += 1,
                   OutcomeClass::Built => summary.built += 1,
                   OutcomeClass::ActionFailure { .. }
                   | OutcomeClass::OutputMissing { .. }
                   | OutcomeClass::InfraFailure { .. } => summary.failed += 1,
                   OutcomeClass::Skipped { .. } => summary.skipped += 1,
               }
           }
           summary
       }
   }
   ```

6. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::outcome
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 328 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree with plans `01` to `04` landed; none of them changes a manifest.

8. Commit:

   ```
   $ git add crates/buildl-core/src/types/outcome.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the outcome of one target and the summary of a build"
   ```

### Task 2 — Add the build log's event

**Files:**
- Create `crates/buildl-core/src/types/outcome_event.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod outcome_event;` to the `pub mod` lines and `pub use outcome_event::OutcomeEvent;` to the `pub use` lines, and add one bullet to the module doc, directly below the bullet task 1 added:

   ```rust
   //! - [`OutcomeEvent`] — one line of the build log: an outcome, with when and under which key.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `OutcomeEvent` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/outcome_event.rs` with the test module only. Each expected text is the whole event as the canonical serializer writes it, keys in sorted order:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use super::OutcomeEvent;
       use crate::json::canonical;
       use crate::types::{
           ActionOutcome, Argument, Command, Diagnostic, Digest, Directory, InfraKind, Label,
           OutcomeClass, OutputName, Produced, Provenance, Timestamp,
       };

       fn command(arguments: &[&str]) -> Command {
           Command::new(
               arguments
                   .iter()
                   .map(|argument| Argument::parse(*argument).unwrap())
                   .collect(),
           )
           .unwrap()
       }

       fn lib_file() -> Provenance {
           Provenance::new(
               PathBuf::from("lib/build.lua"),
               Directory::parse("lib").unwrap(),
           )
       }

       fn root_file() -> Provenance {
           Provenance::new(PathBuf::from("build.lua"), Directory::root())
       }

       const fn instant(nanos: u64) -> Timestamp {
           Timestamp::from_unix_nanos(nanos)
       }

       #[test]
       fn a_built_target_is_logged_with_its_key_its_outputs_and_both_instants() {
           let key = Digest::of(b"key");
           let text = Digest::of(b"text");
           let printed = Digest::of(b"compiling\n");
           let nothing = Digest::of(b"");
           let event = OutcomeEvent {
               at: instant(30),
               provenance: lib_file(),
               command: command(&["cc", "-c", "lib/a.c", "-o", "out/lib/text"]),
               key: Some(key),
               started: Some(instant(10)),
               finished: Some(instant(20)),
               outcome: ActionOutcome {
                   label: Label::parse("//lib:text").unwrap(),
                   class: OutcomeClass::Built,
                   diagnostic: None,
                   outputs: vec![Produced {
                       name: OutputName::parse("text").unwrap(),
                       digest: text,
                   }],
                   stdout: Some(printed),
                   stderr: Some(nothing),
               },
           };
           assert_eq!(
               canonical::to_string(&event).unwrap(),
               format!(
                   concat!(
                       r#"{{"at":30,"class":"Built","#,
                       r#""command":["cc","-c","lib/a.c","-o","out/lib/text"],"#,
                       r#""diagnostic":null,"file":"lib/build.lua","finished":20,"#,
                       r#""key":"{key}","label":"//lib:text","#,
                       r#""outputs":[{{"digest":"{text}","name":"text"}}],"#,
                       r#""started":10,"stderr":"{nothing}","stdout":"{printed}"}}"#
                   ),
                   key = key,
                   text = text,
                   nothing = nothing,
                   printed = printed
               )
           );
       }

       #[test]
       fn a_failed_target_is_logged_with_its_class_and_the_message() {
           let key = Digest::of(b"key");
           let printed = Digest::of(b"compiling\n");
           let nothing = Digest::of(b"");
           let event = OutcomeEvent {
               at: instant(30),
               provenance: root_file(),
               command: command(&["cc", "out/lib/text", "-o", "out/app"]),
               key: Some(key),
               started: Some(instant(10)),
               finished: Some(instant(20)),
               outcome: ActionOutcome {
                   label: Label::parse("//:app").unwrap(),
                   class: OutcomeClass::InfraFailure {
                       kind: InfraKind::Store,
                   },
                   diagnostic: Some(Diagnostic::new("content store: disk full")),
                   outputs: Vec::new(),
                   stdout: Some(printed),
                   stderr: Some(nothing),
               },
           };
           assert_eq!(
               canonical::to_string(&event).unwrap(),
               format!(
                   concat!(
                       r#"{{"at":30,"class":{{"InfraFailure":{{"kind":"Store"}}}},"#,
                       r#""command":["cc","out/lib/text","-o","out/app"],"#,
                       r#""diagnostic":"content store: disk full","file":"build.lua","#,
                       r#""finished":20,"key":"{key}","label":"//:app","outputs":[],"#,
                       r#""started":10,"stderr":"{nothing}","stdout":"{printed}"}}"#
                   ),
                   key = key,
                   nothing = nothing,
                   printed = printed
               )
           );
       }

       #[test]
       fn a_skipped_target_is_logged_without_a_key_an_instant_or_a_digest() {
           let event = OutcomeEvent {
               at: instant(31),
               provenance: root_file(),
               command: command(&["cc", "out/lib/text", "-o", "out/app"]),
               key: None,
               started: None,
               finished: None,
               outcome: ActionOutcome {
                   label: Label::parse("//:app").unwrap(),
                   class: OutcomeClass::Skipped {
                       because: Label::parse("//lib:text").unwrap(),
                   },
                   diagnostic: None,
                   outputs: Vec::new(),
                   stdout: None,
                   stderr: None,
               },
           };
           assert_eq!(
               canonical::to_string(&event).unwrap(),
               concat!(
                   r#"{"at":31,"class":{"Skipped":{"because":"//lib:text"}},"#,
                   r#""command":["cc","out/lib/text","-o","out/app"],"#,
                   r#""diagnostic":null,"file":"build.lua","finished":null,"key":null,"#,
                   r#""label":"//:app","outputs":[],"started":null,"stderr":null,"stdout":null}"#
               )
           );
       }

       #[test]
       fn an_event_holds_exactly_the_twelve_keys_of_the_log_format() {
           let event = OutcomeEvent {
               at: instant(1),
               provenance: root_file(),
               command: command(&["cc"]),
               key: None,
               started: None,
               finished: None,
               outcome: ActionOutcome {
                   label: Label::parse("//:app").unwrap(),
                   class: OutcomeClass::Cached,
                   diagnostic: None,
                   outputs: Vec::new(),
                   stdout: None,
                   stderr: None,
               },
           };
           let tree = serde_json::to_value(&event).unwrap();
           let keys: Vec<&str> = tree
               .as_object()
               .unwrap()
               .keys()
               .map(String::as_str)
               .collect();
           assert_eq!(
               keys,
               [
                   "at",
                   "class",
                   "command",
                   "diagnostic",
                   "file",
                   "finished",
                   "key",
                   "label",
                   "outputs",
                   "started",
                   "stderr",
                   "stdout"
               ]
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::outcome_event
   error[E0432]: unresolved import `super::OutcomeEvent`
   error[E0432]: unresolved import `outcome_event::OutcomeEvent`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/outcome_event.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! One line of the build log: how the build of one target ended, when, and under which key.
   //!
   //! Its own file because the log is a second reader of an outcome, with needs of its own: the
   //! build that returns an outcome already holds the graph, but a log is read later and alone, so
   //! an event carries the build file, the command and the key beside the outcome, and names every
   //! target by its label.
   //!
   //! Responsibilities: [`OutcomeEvent`] and its serialized form.
   //!
   //! Non-responsibilities: appending the event anywhere, which is a port's; and reading a log
   //! back, which nothing in this crate does.

   use serde::{Serialize, Serializer};

   use crate::types::{
       ActionOutcome, Command, Diagnostic, Digest, Label, OutcomeClass, Produced, Provenance,
       Timestamp,
   };

   /// What a build log records for one target.
   ///
   /// One event is written for every target of a build, whatever its outcome.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct OutcomeEvent {
       /// When the outcome was recorded.
       pub at: Timestamp,
       /// The build file that declared the target.
       pub provenance: Provenance,
       /// The target's command, with every placeholder expanded.
       pub command: Command,
       /// The digest of the key the target was built or found cached under; `None` for a target
       /// that was skipped, whose key was never finished.
       pub key: Option<Digest>,
       /// When a worker began the action; `None` for a target no worker began.
       pub started: Option<Timestamp>,
       /// When the worker was done with the action; `None` for a target no worker began. The span
       /// from `started` is how long the action took.
       pub finished: Option<Timestamp>,
       /// How the build of the target ended.
       pub outcome: ActionOutcome,
   }

   /// An event as it is written to JSON: one flat object, with the outcome's fields beside the
   /// event's own.
   #[derive(Serialize)]
   struct EventView<'a> {
       at: Timestamp,
       class: &'a OutcomeClass,
       command: &'a Command,
       diagnostic: Option<&'a str>,
       file: String,
       finished: Option<Timestamp>,
       key: Option<Digest>,
       label: &'a Label,
       outputs: &'a [Produced],
       started: Option<Timestamp>,
       stderr: Option<Digest>,
       stdout: Option<Digest>,
   }

   /// Serializes as one flat object, so that a line of the log can be read on its own.
   ///
   /// The build file is written as its path, and every absent value as `null`, so every event
   /// holds the same twelve keys. There is no `Deserialize`: a log is appended to, never read back
   /// here.
   impl Serialize for OutcomeEvent {
       fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
           EventView {
               at: self.at,
               class: &self.outcome.class,
               command: &self.command,
               diagnostic: self.outcome.diagnostic.as_ref().map(Diagnostic::as_str),
               file: self.provenance.to_string(),
               finished: self.finished,
               key: self.key,
               label: &self.outcome.label,
               outputs: &self.outcome.outputs,
               started: self.started,
               stderr: self.outcome.stderr,
               stdout: self.outcome.stdout,
           }
           .serialize(serializer)
       }
   }
   ```

6. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::outcome_event
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 336 filtered out
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
   $ git add crates/buildl-core/src/types/outcome_event.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the build log's event"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `OutcomeClass` has the six classes of spec §3.4 and no class for a refusal; `succeeded` is
  true for `Cached` and `Built` only; `InfraKind` has `Prepare`, `Run`, `TimedOut` and `Store`.
- `Summary::of` counts `ActionFailure`, `OutputMissing` and `InfraFailure` as failed, `Skipped`
  as skipped, `Built` as built and `Cached` as cached.
- A class without a field serializes as its name, and every other class as an object of one key.
- An `OutcomeEvent` serializes to one object holding exactly the keys `at`, `class`, `command`,
  `diagnostic`, `file`, `finished`, `key`, `label`, `outputs`, `started`, `stderr` and `stdout`,
  with `null` for every absent value. The exact text is pinned for a built, a failed and a
  skipped target.
- `buildl_core::{ActionOutcome, InfraKind, OutcomeClass, Summary, OutcomeEvent}` resolve from
  the crate root.
- `crates/buildl-core/Cargo.toml` and `crates/expected-module-edges.txt` are unchanged: the new
  files import only `crate::types`, `crate::json` in their tests, and `serde`.

## Review findings

Reviewed once, 2026-10-12, over both tasks. The reviewer re-ran the gate: `cargo make dod`
exit 0 (`[cargo-make] INFO - Build Done in 5.31 seconds.`), `cargo deny check` exit 0
(`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan: both new
files are byte-identical to the plan's code blocks; no drift.

- risk — `ActionOutcome`'s field docs state rules across fields that the type cannot enforce
  and nothing in this plan produces — `types/outcome.rs:84`, `types/outcome_event.rs:22,31-38`.
  Left as it is: the wording is the spec's (§3.4, §3.5); plan 08's `record` functions are what
  build these values and keep the rules.
- nit — the count "twelve keys" sits in a doc comment and a test name, unpinned —
  `types/outcome_event.rs:63,243`. Left as it is: the test beside it compares the full key list.
- nit — `file` is written through a lossy `Display`, while `Provenance`'s doc says a non-UTF-8
  path fails — `types/outcome_event.rs:73`. Left as it is: no path reaches it that is not built
  from validated text, and spec §3.5 fixes this form.
- nit — the twelve-keys test builds a `Cached` event with `key: None`, a pairing the field doc
  rules out — `types/outcome_event.rs:248`. Left as it is: the test is about the keys only.

## Probe results

No separate probe was run. The facts the tasks assert about existing code are ones their own
compile and test runs depend on:

- Task 1 red: `cargo test -p buildl-core --lib types::outcome` exit 101, two `error[E0432]`
  lines naming `ActionOutcome`, `InfraKind`, `OutcomeClass`, `Summary` — as the plan quotes.
- Task 1 green: `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 328 filtered out`
  — as the plan quotes.
- Task 2 red: `cargo test -p buildl-core --lib types::outcome_event` exit 101,
  `error[E0432]: unresolved import `super::OutcomeEvent`` and
  `error[E0432]: unresolved import `outcome_event::OutcomeEvent`` — as the plan quotes.
- Task 2 green: `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 336 filtered out`
  — as the plan quotes.

## Deviations

- 2026-10-12 — the commit step of both tasks was skipped. The work is in the tree, uncommitted;
  committing is left to the author.
