---
status: done
created: 2026-10-14
---

# Core Values Implementation Plan

**Goal:** `buildl-core`'s vocabulary — its value types with its error enum — covers everything a scoped, reported build hands across a port.

**Architecture:** Seven new files under `src/types/`, one type per file, in the shape every file there has: a module doc saying why the type has its own file, the type, and its tests when it holds logic. `Turn<M, R>` is what a dispatcher hands the thread that schedules a build: the beginning, a message from a running task, or a task's result. `Channel` names one of a command's two outputs; `LiveSink` is the borrowed callback a strategy hands a running command's bytes to; `Captured` is what an action printed, as bytes, for whoever shows it. `LiveOutput` and `Scope` are the two choices a caller makes about one run: whether one action is shown while it prints, and which targets the run works on. `Hasher` sits beside `Digest` and takes the same SHA-256 over bytes handed in pieces. `InputStat` and `InputKind` say what stands at the path of a declared input: nothing, a regular file, or something else and what. `OutcomeClass` gains `Killed { signal }` and `Exit` gains `Signaled(i32)`, because a command a signal ended has no exit status. `Error` gains `UnknownTarget`, `CorruptState` and `InputNotAFile`, and `PortName` gains `GraphRecord`. Nothing here calls anything outside the crate, no port changes, and every new name is re-exported from `types/mod.rs` and from `lib.rs`.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`, `cargo-deny`. No new dependency.

**Content authority:** spec §2.1 (the values), §2.8 (`Error`, `PortName`, the module doc's sentence), §2.10 (the files), §2.11 (the unit tests of `types/` and `error.rs`), §2.12 (`InputStat`, `InputKind`).

---

## Context an implementer needs

What the crate holds that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Digest` with `Digest::of(&[u8])`, a `Copy` newtype over 32 bytes whose field is private to its file | `crates/buildl-core/src/types/digest.rs` | what `Hasher::finish` returns |
| `FileStat { modified, size }` (`Copy`) | `crates/buildl-core/src/types/file_stat.rs` | what `InputStat::File` holds |
| `Label` (`Ord`, `Display` is `//dir:name`) | `crates/buildl-core/src/types/label.rs` | what `Scope::Targets` holds; the field of `Error::UnknownTarget` |
| `OutcomeClass` and `Summary::of` | `crates/buildl-core/src/types/outcome.rs` | the enum that gains `Killed`; the one exhaustive match over it |
| `Exit::{Exited(i32), TimedOut}` | `crates/buildl-core/src/types/raw_outcome.rs` | the enum that gains `Signaled` |
| `exited(exit, printed) -> Option<Ended>` | `crates/buildl-core/src/execute/perform.rs` | the one exhaustive match over `Exit` in the workspace |
| `Error`, `PortName`, `DeclarationSite`, `suggestion_suffix` | `crates/buildl-core/src/error.rs` | the enums that gain variants; the helper that renders ` (did you mean …?)` |
| the test helpers `label(raw)` and `root_site(raw)` | the `tests` module of `crates/buildl-core/src/error.rs` | what the new error tests are written with |

Facts of the gate that bite here:

- `crates/buildl-core/src/lib.rs` and every `mod.rs` hold module declarations and re-exports only, and keep both lists sorted. A step adds a name where it says and runs `cargo fmt --all`, which sorts and rewraps the list; the blocks below show the list as rustfmt leaves it.
- The workspace denies warnings, `clippy::unwrap_used` and `clippy::panic` included. Test modules carry `#![expect(clippy::unwrap_used, …)]`; the files this plan creates with tests do the same.
- `cargo make dod` runs three guards before clippy. `guard-module-edges` reads every `crate::<module>` on a line that is not a comment and compares the list with `crates/expected-module-edges.txt`: this plan adds `types -> …` and `error -> types` imports only, both already listed, so that file does not change. An import is always spelled `crate::<module>::<item>`.
- A file that holds only type definitions says so in the last line of its module doc, where a file with logic has its `#[cfg(test)] mod tests`.
- Six of the new types hold no logic, so tasks 1 to 3 have no failing-test step: the gate's build, lint and rustdoc steps check them, and plans `02` and `03` are what use them.
- `Exit` and `OutcomeClass` are `#[non_exhaustive]`, which lets another crate keep compiling when a variant is added; inside this crate a `match` must still name every variant, so each variant is added together with the arm it needs.
- Two things `crates/buildl-core/src/execute/perform.rs` holds belong to the next plans by subject and are in task 7 of this one by necessity: the arm of `exited` for `Exit::Signaled`, and its test. Without the arm the crate does not compile once the variant exists.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes, in a copy of the workspace walked through these tasks in order; a failing run is quoted by the error lines that name the cause, not in full. `cargo make dod` was run after every task and exited `0` with the counts each gate step gives. `cargo deny check` was run on the tree this plan starts from and on the tree plan `03` leaves, and printed `advisories ok, bans ok, licenses ok, sources ok` both times; no task of the three plans changes a manifest or the lockfile.

Before task 1 the three `buildl-core` results of `cargo make dod` read `535 passed` (unit tests), `108 passed` (flows) and `12 passed` (doctests).

## File structure

```
crates/buildl-core/src/types/turn.rs          — [create] Turn
crates/buildl-core/src/types/channel.rs       — [create] Channel
crates/buildl-core/src/types/live_sink.rs     — [create] LiveSink
crates/buildl-core/src/types/captured.rs      — [create] Captured
crates/buildl-core/src/types/live_output.rs   — [create] LiveOutput
crates/buildl-core/src/types/scope.rs         — [create] Scope
crates/buildl-core/src/types/input_stat.rs    — [create] InputStat, InputKind and the words a kind is shown with
crates/buildl-core/src/types/digest.rs        — [modify] Hasher and its tests
crates/buildl-core/src/types/outcome.rs       — [modify] OutcomeClass::Killed, counted as a failure
crates/buildl-core/src/types/raw_outcome.rs   — [modify] Exit::Signaled
crates/buildl-core/src/types/mod.rs           — [modify] declare, index and re-export the new types
crates/buildl-core/src/execute/perform.rs     — [modify] what an action a signal ended ends as, and its test
crates/buildl-core/src/error.rs               — [modify] UnknownTarget, CorruptState, InputNotAFile, PortName::GraphRecord, the module doc
crates/buildl-core/src/lib.rs                 — [modify] re-export the new types
```

### Task 1 — Add the turn a dispatcher hands the scheduling thread

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Create `crates/buildl-core/src/types/turn.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/turn.rs`:

   ```rust
   //! One turn of the thread that schedules a build: what a dispatcher hands it each time.
   //!
   //! Its own file because it is the value that crosses the dispatching port towards the caller:
   //! a build is begun once, is told things by the tasks that are running, and is handed the result
   //! of each task when it is done.
   //!
   //! Responsibilities: [`Turn`].
   //!
   //! Non-responsibilities: what a message or a result is. The port is generic over both.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   /// What a dispatcher hands the calling thread: the start, a message from a running task, or the
   /// result of a finished one.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Turn<M, R> {
       /// Nothing has run yet. Handed over once, first.
       Begin,
       /// A task that is running said this.
       Told(M),
       /// A task is done, with this result.
       Done(R),
   }
   ```

2. Declare the module, name the type in the module's index and re-export it. In `crates/buildl-core/src/types/mod.rs`:

   Replace:

   ```rust
   //!   one target ended; [`Summary`] — the count of a build's outcomes.
   //! - [`OutcomeEvent`] — one line of the build log: an outcome, with when and under which key.
   //!
   //! Non-responsibilities: decisions between different concepts. A type here validates and renders
   ```

   With:

   ```rust
   //!   one target ended; [`Summary`] — the count of a build's outcomes.
   //! - [`OutcomeEvent`] — one line of the build log: an outcome, with when and under which key.
   //! - [`Turn`] — what a dispatcher hands the thread that schedules a build, each time it hands
   //!   it something.
   //!
   //! Non-responsibilities: decisions between different concepts. A type here validates and renders
   ```

   Replace:

   ```rust
   pub mod tree;
   pub mod tree_path;
   pub mod workers;
   pub mod workspace;
   ```

   With:

   ```rust
   pub mod tree;
   pub mod tree_path;
   pub mod turn;
   pub mod workers;
   pub mod workspace;
   ```

   Replace:

   ```rust
   pub use tree::{Tree, TreeEntry};
   pub use tree_path::TreePath;
   pub use workers::Workers;
   pub use workspace::{Execution, Workspace};
   ```

   With:

   ```rust
   pub use tree::{Tree, TreeEntry};
   pub use tree_path::TreePath;
   pub use turn::Turn;
   pub use workers::Workers;
   pub use workspace::{Execution, Workspace};
   ```

3. Re-export it from the crate root. In `crates/buildl-core/src/lib.rs`, add `Turn` to the `pub use types::{…}` list and run `cargo fmt --all`, which places it:

   Replace:

   ```rust
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName, TargetRole,
       Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Unfinished, Verdict, Want,
       WantedSet, Workers, Workspace, Written,
   };
   ```

   With:

   ```rust
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName, TargetRole,
       Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished, Verdict, Want,
       WantedSet, Workers, Workspace, Written,
   };
   ```

4. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `535 passed` (unit tests), `108 passed` (flows) and `12 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

5. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/types/turn.rs
   $ git commit -m "feat(buildl-core): add the turn a dispatcher hands the scheduling thread"
   ```

### Task 2 — Add the values that carry what an action prints

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Create `crates/buildl-core/src/types/captured.rs`
- Create `crates/buildl-core/src/types/channel.rs`
- Create `crates/buildl-core/src/types/live_sink.rs`
- Modify `crates/buildl-core/src/types/mod.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/channel.rs`:

   ```rust
   //! Which of its two outputs a command wrote to.
   //!
   //! Its own file because the two are kept apart everywhere: captured apart, stored apart, and
   //! shown apart. A name for each keeps a call that hands over bytes from saying which by position.
   //!
   //! Responsibilities: [`Channel`].
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   /// One of the two outputs of a running command.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
   pub enum Channel {
       /// Standard output.
       Stdout,
       /// Standard error.
       Stderr,
   }
   ```

2. Create `crates/buildl-core/src/types/live_sink.rs`. The alias exists because clippy's `type_complexity` refuses the bare type in a signature:

   ```rust
   //! Where a strategy hands what a running command writes, as it writes it.
   //!
   //! Its own file because it is the one value that crosses the execution port while a command is
   //! still running. A name for it keeps the port's signature readable.
   //!
   //! Responsibilities: [`LiveSink`].
   //!
   //! This file holds only a type alias, so it carries no logic to unit-test.

   use crate::types::Channel;

   /// Takes a run of bytes a running command just wrote, with the output it wrote them to.
   ///
   /// Borrowed for one run of one command, and callable from any thread during it: a strategy
   /// that reads the two outputs on two threads may call it from both.
   pub type LiveSink<'a> = &'a (dyn Fn(Channel, &[u8]) + Sync);
   ```

3. Create `crates/buildl-core/src/types/captured.rs`:

   ```rust
   //! What an action printed, as bytes, for whoever shows it.
   //!
   //! Its own file because it is the value that crosses the reporting port beside an outcome: an
   //! outcome holds the digests of what was printed, and a reporter is handed the bytes themselves.
   //!
   //! Responsibilities: [`Captured`].
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   /// What one action wrote to its two outputs, still to be shown.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub struct Captured<'a> {
       /// What it wrote to standard output.
       pub stdout: &'a [u8],
       /// What it wrote to standard error.
       pub stderr: &'a [u8],
   }
   ```

4. Declare the three modules, name the types in the index and re-export them. In `crates/buildl-core/src/types/mod.rs`:

   Replace:

   ```rust
   //! - [`ReadyAction`] — one action as an execution strategy is handed it; [`RawOutcome`] and
   //!   [`Exit`] — what the strategy reports back.
   //! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
   //! - [`ActionOutcome`], [`OutcomeClass`], [`InfraKind`] and [`OutputRefusal`] — how the build of
   ```

   With:

   ```rust
   //! - [`ReadyAction`] — one action as an execution strategy is handed it; [`RawOutcome`] and
   //!   [`Exit`] — what the strategy reports back.
   //! - [`Channel`] — which of its two outputs a command wrote to; [`LiveSink`] — where a strategy
   //!   hands what a running command writes; [`Captured`] — what an action printed, as whoever
   //!   shows it is handed it.
   //! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
   //! - [`ActionOutcome`], [`OutcomeClass`], [`InfraKind`] and [`OutputRefusal`] — how the build of
   ```

   Replace:

   ```rust
   pub mod build_file;
   pub mod cache_row;
   pub mod declaration;
   pub mod description;
   ```

   With:

   ```rust
   pub mod build_file;
   pub mod cache_row;
   pub mod captured;
   pub mod channel;
   pub mod declaration;
   pub mod description;
   ```

   Replace:

   ```rust
   pub mod isolation;
   pub mod label;
   pub mod node_id;
   pub mod on_failure;
   ```

   With:

   ```rust
   pub mod isolation;
   pub mod label;
   pub mod live_sink;
   pub mod node_id;
   pub mod on_failure;
   ```

   Replace:

   ```rust
   };
   pub use cache_row::CacheRow;
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
   ```

   With:

   ```rust
   };
   pub use cache_row::CacheRow;
   pub use captured::Captured;
   pub use channel::Channel;
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
   ```

   Replace:

   ```rust
   pub use isolation::Isolation;
   pub use label::Label;
   pub use node_id::NodeId;
   pub use on_failure::OnFailure;
   ```

   With:

   ```rust
   pub use isolation::Isolation;
   pub use label::Label;
   pub use live_sink::LiveSink;
   pub use node_id::NodeId;
   pub use on_failure::OnFailure;
   ```

5. Re-export them from the crate root. In `crates/buildl-core/src/lib.rs`, add `Captured`, `Channel` and `LiveSink` to the `pub use types::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
   pub use resolve::resolve;
   pub use types::{
       Action, ActionKey, ActionOutcome, Alias, Argument, BuildFile, CacheRow, Ceiling, Command,
       Declaration, DeclarationOrder, Declared, DepProducts, Description, Diagnostic, Difference,
       Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue, Evaluated, Execution, Exit,
       ExpandedDescription, FieldName, FileMode, FileStat, Found, FoundEntry, FoundKind, Freshness,
       InfraKind, InputDigest, Isolation, KeyComponents, Label, NetworkAccess, Node, NodeId,
       OnFailure, OutcomeClass, OutcomeEvent, OutputManifest, OutputName, OutputPath, OutputRefusal,
       OwnComponents, Plan, PlannedKey, Platform, Produced, Program, Provenance, RawOutcome,
       ReadyAction, Rule, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName, TargetRole,
       Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished, Verdict, Want,
       WantedSet, Workers, Workspace, Written,
   };
   ```

   With:

   ```rust
   pub use resolve::resolve;
   pub use types::{
       Action, ActionKey, ActionOutcome, Alias, Argument, BuildFile, CacheRow, Captured, Ceiling,
       Channel, Command, Declaration, DeclarationOrder, Declared, DepProducts, Description,
       Diagnostic, Difference, Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue,
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, InfraKind, InputDigest, Isolation, KeyComponents, Label,
       LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass, OutcomeEvent, OutputManifest,
       OutputName, OutputPath, OutputRefusal, OwnComponents, Plan, PlannedKey, Platform, Produced,
       Program, Provenance, RawOutcome, ReadyAction, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName,
       TargetRole, Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished,
       Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `535 passed` (unit tests), `108 passed` (flows) and `12 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/types/captured.rs crates/buildl-core/src/types/channel.rs crates/buildl-core/src/types/live_sink.rs crates/buildl-core/src/types/mod.rs
   $ git commit -m "feat(buildl-core): add the values that carry what an action prints"
   ```

### Task 3 — Add the two choices a caller makes about one run

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Create `crates/buildl-core/src/types/live_output.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Create `crates/buildl-core/src/types/scope.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/live_output.rs`:

   ```rust
   //! Whether a build shows what an action prints while the action is still running.
   //!
   //! Its own file because it is a choice a caller makes about one build that is not in the
   //! workspace: the same actions run and the same bytes are stored either way, and only when they
   //! are shown differs. An enum rather than a flag, so a call site reads as the choice it made.
   //!
   //! Responsibilities: [`LiveOutput`].
   //!
   //! Non-responsibilities: applying the choice, which the phase that executes does.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   /// When a build shows what its actions print.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum LiveOutput {
       /// Only once an action has ended, all of it together.
       #[default]
       Off,
       /// As it is printed, for one running action at a time; every other action as with
       /// [`LiveOutput::Off`].
       OneAction,
   }
   ```

2. Create `crates/buildl-core/src/types/scope.rs`:

   ```rust
   //! Which targets of a workspace a command works on.
   //!
   //! Its own file because it is a choice a caller makes about one run that is not in the
   //! workspace: the same build files are loaded either way, and only how much of their graph is
   //! planned and built differs.
   //!
   //! Responsibilities: [`Scope`].
   //!
   //! Non-responsibilities: finding what the named targets need, which the phase that builds the
   //! graph does.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   use std::collections::BTreeSet;

   use crate::types::Label;

   /// The part of a workspace's graph one run works on.
   #[derive(Debug, Clone, PartialEq, Eq, Default)]
   pub enum Scope {
       /// Every target.
       #[default]
       Workspace,
       /// The targets these labels name, each directly or as an alias, and everything they depend
       /// on. No label is no target.
       Targets(BTreeSet<Label>),
   }
   ```

3. Declare the two modules, name the types in the index and re-export them. In `crates/buildl-core/src/types/mod.rs`:

   Replace:

   ```rust
   //!   shows it is handed it.
   //! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
   //! - [`ActionOutcome`], [`OutcomeClass`], [`InfraKind`] and [`OutputRefusal`] — how the build of
   //!   one target ended; [`Summary`] — the count of a build's outcomes.
   ```

   With:

   ```rust
   //!   shows it is handed it.
   //! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
   //! - [`Scope`] — which targets of a workspace one run works on; [`LiveOutput`] — whether a build
   //!   shows what an action prints while the action is still running.
   //! - [`ActionOutcome`], [`OutcomeClass`], [`InfraKind`] and [`OutputRefusal`] — how the build of
   //!   one target ended; [`Summary`] — the count of a build's outcomes.
   ```

   Replace:

   ```rust
   pub mod isolation;
   pub mod label;
   pub mod live_sink;
   pub mod node_id;
   ```

   With:

   ```rust
   pub mod isolation;
   pub mod label;
   pub mod live_output;
   pub mod live_sink;
   pub mod node_id;
   ```

   Replace:

   ```rust
   pub mod raw_outcome;
   pub mod ready_action;
   pub mod setting;
   pub mod source_path;
   ```

   With:

   ```rust
   pub mod raw_outcome;
   pub mod ready_action;
   pub mod scope;
   pub mod setting;
   pub mod source_path;
   ```

   Replace:

   ```rust
   pub use isolation::Isolation;
   pub use label::Label;
   pub use live_sink::LiveSink;
   pub use node_id::NodeId;
   ```

   With:

   ```rust
   pub use isolation::Isolation;
   pub use label::Label;
   pub use live_output::LiveOutput;
   pub use live_sink::LiveSink;
   pub use node_id::NodeId;
   ```

   Replace:

   ```rust
   pub use raw_outcome::{Exit, RawOutcome};
   pub use ready_action::ReadyAction;
   pub use setting::{SettingName, SettingValue};
   pub use source_path::SourcePath;
   ```

   With:

   ```rust
   pub use raw_outcome::{Exit, RawOutcome};
   pub use ready_action::ReadyAction;
   pub use scope::Scope;
   pub use setting::{SettingName, SettingValue};
   pub use source_path::SourcePath;
   ```

4. Re-export them from the crate root. In `crates/buildl-core/src/lib.rs`, add `LiveOutput` and `Scope` to the `pub use types::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, InfraKind, InputDigest, Isolation, KeyComponents, Label,
       LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass, OutcomeEvent, OutputManifest,
       OutputName, OutputPath, OutputRefusal, OwnComponents, Plan, PlannedKey, Platform, Produced,
       Program, Provenance, RawOutcome, ReadyAction, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName,
       TargetRole, Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished,
       Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

   With:

   ```rust
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, InfraKind, InputDigest, Isolation, KeyComponents, Label,
       LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass, OutcomeEvent,
       OutputManifest, OutputName, OutputPath, OutputRefusal, OwnComponents, Plan, PlannedKey,
       Platform, Produced, Program, Provenance, RawOutcome, ReadyAction, Rule, Scope, Setting,
       SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatEntry, StoredFile, Summary, Target,
       TargetGraph, TargetName, TargetRole, Timeout, Timestamp, ToolIdentity, Tree, TreeEntry,
       TreePath, Turn, Unfinished, Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `535 passed` (unit tests), `108 passed` (flows) and `12 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/types/live_output.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/types/scope.rs
   $ git commit -m "feat(buildl-core): add the scope and the live-output choice of a run"
   ```

### Task 4 — Hash bytes handed in pieces

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/types/digest.rs`
- Modify `crates/buildl-core/src/types/mod.rs`

**Steps:**

1. Write the failing tests. In the `tests` module of `crates/buildl-core/src/types/digest.rs`:

   Replace:

   ```rust
       use super::Digest;

       #[test]
       fn matches_the_known_vectors() {
           assert_eq!(
   ```

   With:

   ```rust
       use super::Digest;

       #[test]
       fn bytes_handed_in_pieces_hash_like_the_bytes_joined() {
           let bytes: Vec<u8> = (0..200_000_u32).map(|n| n.to_le_bytes()[0]).collect();
           for piece in [1_usize, 7, 64, 65_536, 200_000, 300_000] {
               let mut hasher = super::Hasher::new();
               for chunk in bytes.chunks(piece) {
                   hasher.update(chunk);
               }
               assert_eq!(hasher.finish(), Digest::of(&bytes), "pieces of {piece}");
           }
       }

       #[test]
       fn a_hasher_handed_nothing_gives_the_digest_of_nothing() {
           assert_eq!(super::Hasher::new().finish(), Digest::of(&[]));
           let mut handed_empty = super::Hasher::new();
           handed_empty.update(&[]);
           assert_eq!(handed_empty.finish(), Digest::of(&[]));
       }

       #[test]
       fn matches_the_known_vectors() {
           assert_eq!(
   ```

2. Run them and confirm they fail to build, because there is no `Hasher`:

   ```
   $ cargo test -p buildl-core --lib types::digest
   error[E0433]: cannot find `Hasher` in `super`
   error: could not compile `buildl-core` (lib test) due to 3 previous errors
   ```

3. Write the type. It wraps the `Sha256` state `Digest::of` uses, so the two cannot disagree about one content. In `crates/buildl-core/src/types/digest.rs`:

   Replace:

   ```rust
   //! transport problem rather than a re-keying.
   //!
   //! Responsibilities: [`Digest`], hashing in-memory bytes, and the hexadecimal form.
   //!
   //! Non-responsibilities: reading anything. Hashing a *file* means touching the filesystem,
   ```

   With:

   ```rust
   //! transport problem rather than a re-keying.
   //!
   //! Responsibilities: [`Digest`], hashing in-memory bytes — whole with [`Digest::of`], or in
   //! pieces with [`Hasher`] — and the hexadecimal form.
   //!
   //! Non-responsibilities: reading anything. Hashing a *file* means touching the filesystem,
   ```

   Replace:

   ```rust
       pub const fn as_bytes(&self) -> &[u8; 32] {
           &self.0
       }
   }
   ```

   With:

   ````rust
       pub const fn as_bytes(&self) -> &[u8; 32] {
           &self.0
       }
   }

   /// A SHA-256 digest taken over bytes handed in pieces.
   ///
   /// For content too large to hold whole: whoever reads it hands each piece over as it is read.
   /// The digest is the one [`Digest::of`] gives for the pieces joined, so the two never disagree
   /// about one content.
   ///
   /// # Examples
   ///
   /// ```
   /// use buildl_core::{Digest, Hasher};
   ///
   /// let mut hasher = Hasher::new();
   /// hasher.update(b"hello, ");
   /// hasher.update(b"world");
   /// assert_eq!(hasher.finish(), Digest::of(b"hello, world"));
   /// ```
   #[derive(Debug, Clone, Default)]
   pub struct Hasher(Sha256);

   impl Hasher {
       /// A hasher that has been handed nothing.
       #[must_use]
       pub fn new() -> Self {
           Self::default()
       }

       /// Adds `bytes` to what is hashed.
       pub fn update(&mut self, bytes: &[u8]) {
           self.0.update(bytes);
       }

       /// The digest of every byte added, in order: equal to [`Digest::of`] over them joined.
       #[must_use]
       pub fn finish(self) -> Digest {
           let out = self.0.finalize();
           let mut raw = [0_u8; 32];
           raw.copy_from_slice(&out);
           Digest(raw)
       }
   }
   ````

4. Name it in the module's index and re-export it. In `crates/buildl-core/src/types/mod.rs`:

   Replace:

   ```rust
   //!
   //! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
   //! - [`Directory`] — a workspace-relative directory path.
   //! - [`TargetName`] — the name half of a label.
   ```

   With:

   ```rust
   //!
   //! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
   //! - [`Hasher`] — that digest, taken over bytes handed in pieces.
   //! - [`Directory`] — a workspace-relative directory path.
   //! - [`TargetName`] — the name half of a label.
   ```

   Replace:

   ```rust
   pub use description::{Description, ExpandedDescription};
   pub use diagnostic::Diagnostic;
   pub use digest::Digest;
   pub use directory::Directory;
   pub use entry_name::EntryName;
   ```

   With:

   ```rust
   pub use description::{Description, ExpandedDescription};
   pub use diagnostic::Diagnostic;
   pub use digest::{Digest, Hasher};
   pub use directory::Directory;
   pub use entry_name::EntryName;
   ```

5. Re-export it from the crate root. In `crates/buildl-core/src/lib.rs`, add `Hasher` to the `pub use types::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
       Diagnostic, Difference, Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue,
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, InfraKind, InputDigest, Isolation, KeyComponents, Label,
       LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass, OutcomeEvent,
       OutputManifest, OutputName, OutputPath, OutputRefusal, OwnComponents, Plan, PlannedKey,
       Platform, Produced, Program, Provenance, RawOutcome, ReadyAction, Rule, Scope, Setting,
       SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatEntry, StoredFile, Summary, Target,
       TargetGraph, TargetName, TargetRole, Timeout, Timestamp, ToolIdentity, Tree, TreeEntry,
       TreePath, Turn, Unfinished, Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

   With:

   ```rust
       Diagnostic, Difference, Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue,
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, Hasher, InfraKind, InputDigest, Isolation, KeyComponents,
       Label, LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass,
       OutcomeEvent, OutputManifest, OutputName, OutputPath, OutputRefusal, OwnComponents, Plan,
       PlannedKey, Platform, Produced, Program, Provenance, RawOutcome, ReadyAction, Rule, Scope,
       Setting, SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile,
       StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatEntry, StoredFile,
       Summary, Target, TargetGraph, TargetName, TargetRole, Timeout, Timestamp, ToolIdentity, Tree,
       TreeEntry, TreePath, Turn, Unfinished, Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

6. Run the tests and the type's doc example and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::digest
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 528 filtered out
   $ cargo test -p buildl-core --doc types::digest
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `537 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/types/digest.rs crates/buildl-core/src/types/mod.rs
   $ git commit -m "feat(buildl-core): hash bytes handed in pieces"
   ```

### Task 5 — Say what stands at the path of an input

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Create `crates/buildl-core/src/types/input_stat.rs`
- Modify `crates/buildl-core/src/types/mod.rs`

**Steps:**

1. Write the two types with the failing test: the words a kind is shown with are the one piece of logic, and they are not written yet. Create `crates/buildl-core/src/types/input_stat.rs`:

   ```rust
   //! What stands at the path of a declared input.
   //!
   //! Its own file because an input is a regular file, and whether a path holds one, nothing, or
   //! something else decides which error a plan ends in: nothing there and something else there
   //! are both mistakes of the build file that declared the input, and are told apart.
   //!
   //! Responsibilities: [`InputStat`], [`InputKind`] and the words [`InputKind`] is shown with.
   //!
   //! Non-responsibilities: looking at the path, which arrives through the
   //! [`Digester`](crate::ports::Digester) port, and raising the error, which belongs to the phase
   //! that plans.

   use crate::types::FileStat;

   /// What a digester found at the path of one input, following no symbolic link.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum InputStat {
       /// Nothing is there.
       Missing,
       /// A regular file, with its metadata.
       File(FileStat),
       /// Something that is not a regular file.
       Other(InputKind),
   }

   /// What stands at a path that holds something other than a regular file.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
   pub enum InputKind {
       /// A symbolic link, whatever it leads to.
       Symlink,
       /// A directory.
       Directory,
       /// Anything else: a pipe, a socket or a device.
       Special,
   }

   #[cfg(test)]
   mod tests {
       use super::InputKind;

       #[test]
       fn each_kind_is_shown_as_what_a_person_would_call_it() {
           assert_eq!(InputKind::Symlink.to_string(), "a symbolic link");
           assert_eq!(InputKind::Directory.to_string(), "a directory");
           assert_eq!(
               InputKind::Special.to_string(),
               "a pipe, a socket or a device"
           );
       }
   }
   ```

2. Declare the module, name the types in the index and re-export them. In `crates/buildl-core/src/types/mod.rs`:

   Replace:

   ```rust
   //!   variable's value; [`Platform`] — the operating system and architecture actions run on.
   //! - [`FileStat`] and [`StatEntry`] — a file's metadata, and a digest recorded beside it.
   //! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
   //! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
   ```

   With:

   ```rust
   //!   variable's value; [`Platform`] — the operating system and architecture actions run on.
   //! - [`FileStat`] and [`StatEntry`] — a file's metadata, and a digest recorded beside it.
   //! - [`InputStat`] and [`InputKind`] — what stands at the path of a declared input.
   //! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
   //! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
   ```

   Replace:

   ```rust
   pub mod file_stat;
   pub mod found;
   pub mod isolation;
   pub mod label;
   ```

   With:

   ```rust
   pub mod file_stat;
   pub mod found;
   pub mod input_stat;
   pub mod isolation;
   pub mod label;
   ```

   Replace:

   ```rust
   pub use file_stat::{FileStat, StatEntry};
   pub use found::{Found, FoundEntry, FoundKind};
   pub use isolation::Isolation;
   pub use label::Label;
   ```

   With:

   ```rust
   pub use file_stat::{FileStat, StatEntry};
   pub use found::{Found, FoundEntry, FoundKind};
   pub use input_stat::{InputKind, InputStat};
   pub use isolation::Isolation;
   pub use label::Label;
   ```

3. Re-export them from the crate root. In `crates/buildl-core/src/lib.rs`, add `InputKind` and `InputStat` to the `pub use types::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
       Diagnostic, Difference, Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue,
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, Hasher, InfraKind, InputDigest, Isolation, KeyComponents,
       Label, LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure, OutcomeClass,
       OutcomeEvent, OutputManifest, OutputName, OutputPath, OutputRefusal, OwnComponents, Plan,
       PlannedKey, Platform, Produced, Program, Provenance, RawOutcome, ReadyAction, Rule, Scope,
       Setting, SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile,
       StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatEntry, StoredFile,
       Summary, Target, TargetGraph, TargetName, TargetRole, Timeout, Timestamp, ToolIdentity, Tree,
       TreeEntry, TreePath, Turn, Unfinished, Verdict, Want, WantedSet, Workers, Workspace, Written,
   };
   ```

   With:

   ```rust
       Diagnostic, Difference, Digest, Directory, DirtyReason, EntryName, EnvName, EnvValue,
       Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat, Found,
       FoundEntry, FoundKind, Freshness, Hasher, InfraKind, InputDigest, InputKind, InputStat,
       Isolation, KeyComponents, Label, LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure,
       OutcomeClass, OutcomeEvent, OutputManifest, OutputName, OutputPath, OutputRefusal,
       OwnComponents, Plan, PlannedKey, Platform, Produced, Program, Provenance, RawOutcome,
       ReadyAction, Rule, Scope, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName, TargetRole,
       Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished, Verdict, Want,
       WantedSet, Workers, Workspace, Written,
   };
   ```

4. Run the test and confirm it fails to build, because a kind cannot be shown:

   ```
   $ cargo test -p buildl-core --lib types::input_stat
   error[E0599]: `InputKind` doesn't implement `std::fmt::Display`
   error: could not compile `buildl-core` (lib test) due to 3 previous errors
   ```

5. Write the words. In `crates/buildl-core/src/types/input_stat.rs`:

   Replace:

   ```rust
   //! [`Digester`](crate::ports::Digester) port, and raising the error, which belongs to the phase
   //! that plans.

   use crate::types::FileStat;
   ```

   With:

   ```rust
   //! [`Digester`](crate::ports::Digester) port, and raising the error, which belongs to the phase
   //! that plans.

   use std::fmt;

   use crate::types::FileStat;
   ```

   Replace:

   ```rust
   }

   #[cfg(test)]
   mod tests {
   ```

   With:

   ```rust
   }

   impl fmt::Display for InputKind {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Symlink => "a symbolic link",
               Self::Directory => "a directory",
               Self::Special => "a pipe, a socket or a device",
           })
       }
   }

   #[cfg(test)]
   mod tests {
   ```

6. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib types::input_stat
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 537 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `538 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/types/input_stat.rs crates/buildl-core/src/types/mod.rs
   $ git commit -m "feat(buildl-core): say what stands at the path of an input"
   ```

### Task 6 — Give an action a signal ended its own outcome class

**Files:**
- Modify `crates/buildl-core/src/types/outcome.rs`

**Steps:**

1. Write the failing tests. The tests of `crates/buildl-core/src/types/outcome.rs` go through one class of every kind; give them the new one: it is not a success, it is counted as a failure, and it serializes as an object of one key. In the `tests` module:

   Replace:

   ```rust
       use crate::types::{Label, OutputName};

       /// One class of every kind, in the order the enum declares them.
       fn every_class() -> [OutcomeClass; 7] {
           [
               OutcomeClass::Cached,
               OutcomeClass::Built,
               OutcomeClass::ActionFailure { status: 2 },
               OutcomeClass::OutputMissing {
                   name: OutputName::parse("bin/app").unwrap(),
   ```

   With:

   ```rust
       use crate::types::{Label, OutputName};

       /// One class of every kind, in the order the enum declares them.
       fn every_class() -> [OutcomeClass; 8] {
           [
               OutcomeClass::Cached,
               OutcomeClass::Built,
               OutcomeClass::ActionFailure { status: 2 },
               OutcomeClass::Killed { signal: 11 },
               OutcomeClass::OutputMissing {
                   name: OutputName::parse("bin/app").unwrap(),
   ```

   Replace:

   ```rust
       fn only_a_cached_and_a_built_target_succeeded() {
           let succeeded = every_class().map(|class| class.succeeded());
           assert_eq!(succeeded, [true, true, false, false, false, false, false]);
       }

       #[test]
   ```

   With:

   ```rust
       fn only_a_cached_and_a_built_target_succeeded() {
           let succeeded = every_class().map(|class| class.succeeded());
           assert_eq!(
               succeeded,
               [true, true, false, false, false, false, false, false]
           );
       }

       #[test]
   ```

   Replace:

   ```rust
               Summary::of(&outcomes),
               Summary {
                   failed: 4,
                   skipped: 1,
                   built: 1,
   ```

   With:

   ```rust
               Summary::of(&outcomes),
               Summary {
                   failed: 5,
                   skipped: 1,
                   built: 1,
   ```

   Replace:

   ```rust
               [
                   r#"{"ActionFailure":{"status":2}}"#,
                   r#"{"OutputMissing":{"name":"bin/app"}}"#,
                   r#"{"OutputRefused":{"name":"dist","refusal":"Symlink"}}"#,
   ```

   With:

   ```rust
               [
                   r#"{"ActionFailure":{"status":2}}"#,
                   r#"{"Killed":{"signal":11}}"#,
                   r#"{"OutputMissing":{"name":"bin/app"}}"#,
                   r#"{"OutputRefused":{"name":"dist","refusal":"Symlink"}}"#,
   ```

2. Run them and confirm they fail to build, because there is no such class:

   ```
   $ cargo test -p buildl-core --lib types::outcome
   error[E0599]: no variant named `Killed` found for enum `OutcomeClass`
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

3. Add the class and count it. `Summary::of` holds the one `match` that names every class, so the variant and its arm go in together. In `crates/buildl-core/src/types/outcome.rs`:

   Replace:

   ```rust
           status: i32,
       },
       /// It ran and exited with status zero, and left nothing where an output it declares should
       /// be.
   ```

   With:

   ```rust
           status: i32,
       },
       /// It ran and a signal ended it before it exited.
       Killed {
           /// The signal's number.
           signal: i32,
       },
       /// It ran and exited with status zero, and left nothing where an output it declares should
       /// be.
   ```

   Replace:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct Summary {
       /// Targets whose action failed, whose output was missing or refused, or that the builder
       /// could not build.
       pub failed: usize,
       /// Targets never begun because another target failed.
   ```

   With:

   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct Summary {
       /// Targets whose action failed or was ended by a signal, whose output was missing or
       /// refused, or that the builder could not build.
       pub failed: usize,
       /// Targets never begun because another target failed.
   ```

   Replace:

   ```rust
                   OutcomeClass::Built => summary.built += 1,
                   OutcomeClass::ActionFailure { .. }
                   | OutcomeClass::OutputMissing { .. }
                   | OutcomeClass::OutputRefused { .. }
   ```

   With:

   ```rust
                   OutcomeClass::Built => summary.built += 1,
                   OutcomeClass::ActionFailure { .. }
                   | OutcomeClass::Killed { .. }
                   | OutcomeClass::OutputMissing { .. }
                   | OutcomeClass::OutputRefused { .. }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::outcome
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 525 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `538 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/types/outcome.rs
   $ git commit -m "feat(buildl-core): give an action a signal ended its own outcome class"
   ```

### Task 7 — Let a command end by a signal

**Files:**
- Modify `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/src/types/raw_outcome.rs`

**Steps:**

1. Write the failing test. In the `tests` module of `crates/buildl-core/src/execute/perform.rs`:

   Replace:

   ```rust
       }

       #[test]
       fn a_timeout_is_the_builders_failure_and_carries_no_message() {
           assert_eq!(
   ```

   With:

   ```rust
       }

       #[test]
       fn a_signal_is_the_actions_failure_and_carries_its_number() {
           for signal in [9, 11, 15] {
               assert_eq!(
                   exited(Exit::Signaled(signal), printed()),
                   Some(Ended::Failed {
                       class: OutcomeClass::Killed { signal },
                       diagnostic: None,
                       printed: Some(printed()),
                   })
               );
           }
       }

       #[test]
       fn a_timeout_is_the_builders_failure_and_carries_no_message() {
           assert_eq!(
   ```

2. Run it and confirm it fails to build, because a command cannot end that way yet:

   ```
   $ cargo test -p buildl-core --lib execute::perform
   error[E0599]: no variant, associated function, or constant named `Signaled` found for enum `Exit` in the current scope
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

3. Add the variant. In `crates/buildl-core/src/types/raw_outcome.rs`:

   Replace:

   ```rust
       /// It exited on its own, with this status. Zero is success.
       Exited(i32),
       /// It ran longer than it was allowed to and was stopped.
       TimedOut,
   ```

   With:

   ```rust
       /// It exited on its own, with this status. Zero is success.
       Exited(i32),
       /// A signal ended it before it exited, with this number. It has no status.
       Signaled(i32),
       /// It ran longer than it was allowed to and was stopped.
       TimedOut,
   ```

4. Say what such an ending is. `exited` holds the one `match` that names every way a command ends, so the crate does not build until the arm is there. In `crates/buildl-core/src/execute/perform.rs`:

   Replace:

   ```rust
   ///
   /// `None` for an exit with status zero: what the action left then decides. An action that ran
   /// out of time is the builder's failure; any other status is the action's own.
   pub(crate) const fn exited(exit: Exit, printed: Printed) -> Option<Ended> {
       let class = match exit {
           Exit::Exited(0) => return None,
           Exit::Exited(status) => OutcomeClass::ActionFailure { status },
           Exit::TimedOut => OutcomeClass::InfraFailure {
               kind: InfraKind::TimedOut,
   ```

   With:

   ```rust
   ///
   /// `None` for an exit with status zero: what the action left then decides. An action that ran
   /// out of time is the builder's failure; any other status, and a signal that ended the action,
   /// is the action's own.
   pub(crate) const fn exited(exit: Exit, printed: Printed) -> Option<Ended> {
       let class = match exit {
           Exit::Exited(0) => return None,
           Exit::Exited(status) => OutcomeClass::ActionFailure { status },
           Exit::Signaled(signal) => OutcomeClass::Killed { signal },
           Exit::TimedOut => OutcomeClass::InfraFailure {
               kind: InfraKind::TimedOut,
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::perform
   test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 523 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `539 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute/perform.rs crates/buildl-core/src/types/raw_outcome.rs
   $ git commit -m "feat(buildl-core): let a command end by a signal"
   ```

### Task 8 — Name the graph record among the ports

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. The test that renders every port name gains a row. In the `tests` module of `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
               (PortName::ExecStrategy, "exec strategy"),
               (PortName::Dispatcher, "dispatcher"),
           ];
           for (port, rendered) in cases {
   ```

   With:

   ```rust
               (PortName::ExecStrategy, "exec strategy"),
               (PortName::Dispatcher, "dispatcher"),
               (PortName::GraphRecord, "graph record"),
           ];
           for (port, rendered) in cases {
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::every_port_name
   error[E0599]: no variant, associated function, or constant named `GraphRecord` found for enum `PortName` in the current scope
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

3. Add the name and its phrase. In `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
       /// What hands actions to workers.
       Dispatcher,
   }

   impl fmt::Display for PortName {
   ```

   With:

   ```rust
       /// What hands actions to workers.
       Dispatcher,
       /// The record of the graph a build resolved.
       GraphRecord,
   }

   impl fmt::Display for PortName {
   ```

   Replace:

   ```rust
               Self::ExecStrategy => "exec strategy",
               Self::Dispatcher => "dispatcher",
           })
       }
   ```

   With:

   ```rust
               Self::ExecStrategy => "exec strategy",
               Self::Dispatcher => "dispatcher",
               Self::GraphRecord => "graph record",
           })
       }
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib error::tests::every_port_name
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 538 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `539 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): name the graph record among the ports"
   ```

### Task 9 — Refuse a target that was asked for and is not declared

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing tests. In the `tests` module of `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
       }

       #[test]
       fn a_foreign_plan_says_the_plan_does_not_belong_to_the_graph() {
           assert_eq!(
   ```

   With:

   ```rust
       }

       #[test]
       fn unknown_target_names_the_label_and_the_nearest_one() {
           let err = Error::UnknownTarget {
               label: label("//:libs"),
               suggestion: Some(label("//:lib")),
           };
           assert_eq!(
               err.to_string(),
               "target //:libs was asked for but is not declared (did you mean //:lib?)"
           );
       }

       #[test]
       fn unknown_target_without_a_near_label_offers_nothing() {
           let err = Error::UnknownTarget {
               label: label("//:nope"),
               suggestion: None,
           };
           assert_eq!(
               err.to_string(),
               "target //:nope was asked for but is not declared"
           );
       }

       #[test]
       fn a_foreign_plan_says_the_plan_does_not_belong_to_the_graph() {
           assert_eq!(
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::unknown_target
   error[E0599]: no variant named `UnknownTarget` found for enum `error::Error`
   error: could not compile `buildl-core` (lib test) due to 2 previous errors
   ```

3. Add the variant, after `UnknownOverride`: like it, it has no declaring site, because what named the target was a caller and not a build file. The same step corrects the module doc, which still hands the showing of an error to a port: an error is the return value of a command, and whoever called the command shows it. In `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
   //!
   //! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic, on one line
   //! unless an adapter's own message spans several; how a
   //! command surfaces it belongs to the `Reporter` port.

   use core::fmt;
   ```

   With:

   ```rust
   //!
   //! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic, on one line
   //! unless an adapter's own message spans several. An error is returned, and its caller shows it.

   use core::fmt;
   ```

   Replace:

   ```rust
           /// The setting that was named.
           name: SettingName,
       },
       /// A plan was handed over with a graph it was not made from.
   ```

   With:

   ```rust
           /// The setting that was named.
           name: SettingName,
       },
       /// A run was asked for a target, and no target or alias has that label.
       #[error(
           "target {label} was asked for but is not declared{}",
           suggestion_suffix(.suggestion.as_ref())
       )]
       UnknownTarget {
           /// The label that was asked for.
           label: Label,
           /// The nearest declared target or alias, if one is near enough.
           suggestion: Option<Label>,
       },
       /// A plan was handed over with a graph it was not made from.
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests::unknown_target
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 539 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `541 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): refuse a target that was asked for and is not declared"
   ```

### Task 10 — Tell a state file with foreign content from one that cannot be read

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. In the `tests` module of `crates/buildl-core/src/error.rs`, which already imports `PathBuf`:

   Replace:

   ```rust
       }

       #[test]
       fn evaluation_names_the_file_the_failure_and_the_message() {
           let err = Error::Evaluation {
   ```

   With:

   ```rust
       }

       #[test]
       fn corrupt_state_names_the_file_and_keeps_the_reading_failure() {
           let err = Error::CorruptState {
               file: PathBuf::from("/ws/.buildl/cache.json"),
               source: Box::new(Error::NonCanonicalJson { offset: 36 }),
           };
           assert_eq!(
               err.to_string(),
               "/ws/.buildl/cache.json is not a file buildl wrote: JSON is not in canonical form: \
                it differs from it at byte 36"
           );
           assert_eq!(
               std::error::Error::source(&err).unwrap().to_string(),
               "JSON is not in canonical form: it differs from it at byte 36"
           );
           let Error::CorruptState { source, .. } = err else {
               unreachable!("built as CorruptState above")
           };
           assert!(matches!(*source, Error::NonCanonicalJson { offset: 36 }));
       }

       #[test]
       fn evaluation_names_the_file_the_failure_and_the_message() {
           let err = Error::Evaluation {
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::corrupt_state
   error[E0599]: no variant named `CorruptState` found for enum `error::Error`
   error: could not compile `buildl-core` (lib test) due to 2 previous errors
   ```

3. Add the variant, after `NonCanonicalJson`. Its `source` is the reading failure, boxed because the enum holds itself. `PathBuf` is a value, not an I/O API, so the crate's purity bans do not touch it. In `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
   //! unless an adapter's own message spans several. An error is returned, and its caller shows it.

   use core::fmt;

   use crate::types::{
   ```

   With:

   ```rust
   //! unless an adapter's own message spans several. An error is returned, and its caller shows it.

   use core::fmt;
   use std::path::PathBuf;

   use crate::types::{
   ```

   Replace:

   ```rust
           /// The index of the first byte that differs from the canonical form of the value.
           offset: usize,
       },
       /// An adapter could not evaluate a build file.
   ```

   With:

   ```rust
           /// The index of the first byte that differs from the canonical form of the value.
           offset: usize,
       },
       /// A file an adapter keeps a workspace's state in does not hold what the adapter writes
       /// there.
       ///
       /// Distinct from [`Error::Adapter`], which is a file that could not be read at all: this
       /// one was read, and its content is refused. `source` is the reading failure,
       /// [`Error::UnreadableJson`] or [`Error::NonCanonicalJson`].
       #[error("{} is not a file buildl wrote: {source}", .file.display())]
       CorruptState {
           /// The file, as the adapter opened it.
           file: PathBuf,
           /// Why its content was refused.
           #[source]
           source: Box<Self>,
       },
       /// An adapter could not evaluate a build file.
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib error::tests::corrupt_state
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 541 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `542 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): tell a state file with foreign content from an unreadable one"
   ```

### Task 11 — Refuse an input that is not a regular file

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test, one text per kind. In the `tests` module of `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
       use crate::types::{
           Argument, Ceiling, Command, DeclarationOrder, Diagnostic, Directory, EnvName, FieldName,
           Freshness, Label, NetworkAccess, Node, OutputName, OutputPath, Program, Provenance,
           SettingName, SourcePath, TargetName, TargetRole, TreePath, Want, Written,
       };

       fn lib_file() -> Provenance {
   ```

   With:

   ```rust
       use crate::types::{
           Argument, Ceiling, Command, DeclarationOrder, Diagnostic, Directory, EnvName, FieldName,
           Freshness, InputKind, Label, NetworkAccess, Node, OutputName, OutputPath, Program,
           Provenance, SettingName, SourcePath, TargetName, TargetRole, TreePath, Want, Written,
       };

       fn lib_file() -> Provenance {
   ```

   Replace:

   ```rust
               "build.lua: //:app: input src/main.c does not exist"
           );
       }

       /// A ceiling allowing `cc` and `ld`, `HOME` and `PATH`, reads under `src`, and no write.
   ```

   With:

   ```rust
               "build.lua: //:app: input src/main.c does not exist"
           );
       }

       #[test]
       fn an_input_that_is_not_a_file_names_the_site_the_path_and_what_was_found() {
           for (found, text) in [
               (
                   InputKind::Directory,
                   "build.lua: //:app: input src is a directory; an input must be a regular file",
               ),
               (
                   InputKind::Symlink,
                   "build.lua: //:app: input src is a symbolic link; an input must be a regular file",
               ),
               (
                   InputKind::Special,
                   "build.lua: //:app: input src is a pipe, a socket or a device; an input must be a regular file",
               ),
           ] {
               let err = Error::InputNotAFile {
                   site: Box::new(root_site("//:app")),
                   path: SourcePath::parse("src").unwrap(),
                   found,
               };
               assert_eq!(err.to_string(), text);
           }
       }

       /// A ceiling allowing `cc` and `ld`, `HOME` and `PATH`, reads under `src`, and no write.
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::an_input_that_is_not_a_file
   error[E0599]: no variant named `InputNotAFile` found for enum `error::Error`
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

3. Add the variant, after `MissingInput`, whose shape it has: the lowest-labelled target declaring the input, and the path. In `crates/buildl-core/src/error.rs`:

   Replace:

   ```rust
   use std::path::PathBuf;

   use crate::types::{
       Ceiling, Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Node,
       OutputPath, Program, Provenance, SettingName, SourcePath, TreePath, Want, Written,
   };

   /// The result of any fallible call in this crate.
   ```

   With:

   ```rust
   use std::path::PathBuf;

   use crate::types::{
       Ceiling, Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, InputKind, Label,
       Node, OutputPath, Program, Provenance, SettingName, SourcePath, TreePath, Want, Written,
   };

   /// The result of any fallible call in this crate.
   ```

   Replace:

   ```rust
           /// The file.
           path: SourcePath,
       },
       /// A setting was given a value for this run, but nothing declares it.
   ```

   With:

   ```rust
           /// The file.
           path: SourcePath,
       },
       /// A target reads a workspace path that holds something other than a regular file.
       #[error(
           "{}: {}: input {path} is {found}; an input must be a regular file",
           .site.provenance,
           .site.label
       )]
       InputNotAFile {
           /// The lowest-labelled target declaring the input, and its build file.
           site: Box<DeclarationSite>,
           /// The path.
           path: SourcePath,
           /// What stands there.
           found: InputKind,
       },
       /// A setting was given a value for this run, but nothing declares it.
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib error::tests::an_input_that_is_not_a_file
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 542 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): refuse an input that is not a regular file"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0` after every task; `crates/buildl-core/Cargo.toml`, `Cargo.lock`, `crates/expected-edges.txt` and `crates/expected-module-edges.txt` are unchanged.
- `Turn`, `Channel`, `LiveSink`, `Captured`, `LiveOutput`, `Scope`, `Hasher`, `InputStat` and `InputKind` exist with the definitions of spec §2.1 and §2.12, one file each (`Hasher` beside `Digest`), and each is re-exported from `buildl_core`.
- `Hasher::finish` equals `Digest::of` over the pieces joined, for 200,000 bytes in pieces of 1, 7, 64, 65,536, 200,000 and 300,000 bytes and for a hasher handed nothing.
- `OutcomeClass::Killed { signal: 11 }` is not `succeeded()`, is counted in `Summary::failed`, and serializes as `{"Killed":{"signal":11}}`. `exited(Exit::Signaled(n), …)` is `Killed { signal: n }`.
- `Error` has 30 variants: the 27 it had and `UnknownTarget`, `CorruptState`, `InputNotAFile`. Their texts are the ones the tests pin: `target //:libs was asked for but is not declared (did you mean //:lib?)`, `/ws/.buildl/cache.json is not a file buildl wrote: JSON is not in canonical form: it differs from it at byte 36`, `build.lua: //:app: input src is a directory; an input must be a regular file`. `PortName::GraphRecord` is shown as `graph record`.
- Unit tests of `buildl-core`: 543 (the 535 it had and 8 new); flows: 108, unchanged; doctests: 13 (the 12 it had and `Hasher`'s example).
- Nothing in this plan is used by a phase yet except `Exit::Signaled` and `OutcomeClass::Killed` in `exited`: plan `02` hands the other values across the ports, and plan `03` has the pipeline produce and consume them.
