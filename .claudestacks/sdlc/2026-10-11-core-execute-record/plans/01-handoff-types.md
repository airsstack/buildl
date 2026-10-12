---
status: executing
created: 2026-10-11
---

# Handoff Types Implementation Plan

**Goal:** The handoff values of a build — an output's path, a ready action, a raw outcome, the failure policy — exist as types.

**Architecture:** Four new files under `types/`. `types/output_path.rs` holds `OutputPath`, the only one with logic: one constructor, `OutputPath::of`, joins the workspace's output directory, the directory of a target's label and an output's name with `/`, a root directory contributing nothing. It has no `parse` and no `Deserialize`, because nothing reads a path back; it derives `Ord` and `Hash` so a later check can sort and compare paths, and it serializes as its text. The other three files are definitions only. `types/ready_action.rs` holds `ReadyAction`, everything an execution strategy is handed for one target; `types/raw_outcome.rs` holds `RawOutcome` and `Exit`, what the strategy reports back; `types/on_failure.rs` holds `OnFailure`, an enum rather than a `bool` so a call site reads as the policy it chose. Their fields are public, because none has an invariant beyond its fields' own. Nothing reads these types yet: they are public items of a library, so the build does not report them as unused.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D1, D4, D11, D13), §3.1 (output paths), §3.2 (the action handed to a strategy), §3.3 (what a strategy returns), §3.6 (the failure policy), §9 (the row for `types/output_path.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What exists that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Directory` (`as_str`; the workspace root is the empty text) | `crates/buildl-core/src/types/directory.rs` | the output directory `OutputPath::of` starts from |
| `Label` (`directory() -> &Directory`) | `crates/buildl-core/src/types/label.rs` | the middle of a path: the directory of the target's label |
| `OutputName` (`as_str`; never empty) | `crates/buildl-core/src/types/output_name.rs` | the end of a path |
| `Command`, `EnvName`, `EnvValue`, `SourcePath`, `NetworkAccess` | `crates/buildl-core/src/types/` | the fields of `ReadyAction` |
| The Responsibilities list of the module doc, whose last bullet is the `Plan` bullet | `crates/buildl-core/src/types/mod.rs` | where each task adds its bullet |
| The `pub use types::{…}` list | `crates/buildl-core/src/lib.rs` | where each task adds its names |

Gate facts that bite here:

- `crates/buildl-core/src/types/mod.rs` and `crates/buildl-core/src/lib.rs` keep their `mod`
  lines and `pub use` lists sorted. `cargo fmt --all` sorts and rewraps a `pub use a::{…}` list
  and a run of `pub mod` or `pub use` lines, so each task adds its line or name anywhere in the
  right list and lets `cargo fmt --all` place it. A task creates the file a `mod` line names
  before it runs `cargo fmt --all`.
- A doc bullet in `types/mod.rs` that links a type that does not exist yet fails `cargo make dod`
  at its rustdoc step. Each task therefore adds only the links for the types it creates.
- `ReadyAction`, `RawOutcome`, `Exit` and `OnFailure` have no behaviour, so task 2 has no
  failing-test step: each file carries the note the crate's other definition-only files carry,
  and the gate's build and rustdoc steps are what check them.
- clippy runs with `-D warnings` at pedantic and nursery level, on test code too. The code below
  passed it as written.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/output_path.rs    — [create] OutputPath, its constructor, unit tests
crates/buildl-core/src/types/ready_action.rs   — [create] ReadyAction
crates/buildl-core/src/types/raw_outcome.rs    — [create] RawOutcome, Exit
crates/buildl-core/src/types/on_failure.rs     — [create] OnFailure
crates/buildl-core/src/types/mod.rs            — [modify] declare and re-export the four modules
crates/buildl-core/src/lib.rs                  — [modify] re-export the five types
```

### Task 1 — Add `OutputPath`

**Files:**
- Create `crates/buildl-core/src/types/output_path.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod output_path;` to the `pub mod` lines and `pub use output_path::OutputPath;` to the `pub use` lines, and add one bullet at the end of the module doc's Responsibilities list, directly below the `Plan` bullet:

   ```rust
   //! - [`OutputPath`] — where one output of one target lives in the workspace.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `OutputPath` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/output_path.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::OutputPath;
       use crate::types::{Directory, Label, OutputName};

       fn path(out: &str, label: &str, name: &str) -> OutputPath {
           OutputPath::of(
               &Directory::parse(out).unwrap(),
               &Label::parse(label).unwrap(),
               &OutputName::parse(name).unwrap(),
           )
       }

       #[test]
       fn joins_the_output_directory_the_labels_directory_and_the_name() {
           assert_eq!(path("out", "//lib:text", "text").as_str(), "out/lib/text");
           assert_eq!(path("out", "//:z", "bin/z").as_str(), "out/bin/z");
           assert_eq!(
               path("build/out", "//lib/text:fold", "fold.o").as_str(),
               "build/out/lib/text/fold.o"
           );
       }

       #[test]
       fn a_root_directory_contributes_nothing() {
           assert_eq!(path("out", "//:app", "app").as_str(), "out/app");
           assert_eq!(path("", "//lib:text", "text").as_str(), "lib/text");
           assert_eq!(path("", "//:app", "app").as_str(), "app");
       }

       #[test]
       fn displays_as_its_text() {
           assert_eq!(
               path("out", "//lib:text", "text").to_string(),
               "out/lib/text"
           );
       }

       #[test]
       fn orders_as_its_text_does() {
           let mut paths = [
               path("out", "//lib:text", "text"),
               path("out", "//:lib2", "lib2"),
               path("out", "//:app", "app"),
               path("out", "//:lib", "lib"),
           ];
           paths.sort();
           let sorted: Vec<&str> = paths.iter().map(OutputPath::as_str).collect();
           assert_eq!(sorted, ["out/app", "out/lib", "out/lib/text", "out/lib2"]);
       }

       #[test]
       fn two_targets_can_be_given_the_same_path() {
           assert_eq!(
               path("out", "//:lib", "text"),
               path("out", "//:other", "text")
           );
           assert_ne!(
               path("out", "//:lib", "text"),
               path("out", "//lib:text", "text")
           );
       }

       #[test]
       fn serializes_as_its_text() {
           let json = serde_json::to_string(&path("out", "//lib:text", "text")).unwrap();
           assert_eq!(json, r#""out/lib/text""#);
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::output_path
   error[E0432]: unresolved import `super::OutputPath`
   error[E0432]: unresolved import `output_path::OutputPath`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/output_path.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The workspace-relative path one output of one target lives at.
   //!
   //! Its own file because an output's name says only what a target calls it. Where it lives is a
   //! second value, decided from three others: the directory the workspace puts outputs in, the
   //! directory of the target's label, and the name. Giving that its own type keeps a path nobody
   //! computed from being handed to an executor.
   //!
   //! Responsibilities: [`OutputPath`], its one constructor, and its rendering.
   //!
   //! Non-responsibilities: whether two targets were given the same path, which needs the whole
   //! graph; and the filesystem. Nothing here asks whether the file exists.

   use core::fmt;

   use serde::Serialize;

   use crate::types::{Directory, Label, OutputName};

   /// The workspace-relative path one output of one target lives at, such as `out/lib/text`.
   ///
   /// Built only by [`OutputPath::of`], so every path held is one the layout rule produced. There
   /// is no `parse` and no `Deserialize`: nothing reads one back.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
   #[serde(transparent)]
   pub struct OutputPath(String);

   impl OutputPath {
       /// The path of the output `name` of the target `label`, in a workspace whose outputs go
       /// under `out`.
       ///
       /// The three texts are joined by `/`: the output directory, the directory of the label, and
       /// the name. A root directory contributes nothing, so a target declared at the workspace
       /// root puts its outputs directly under `out`.
       #[must_use]
       pub fn of(out: &Directory, label: &Label, name: &OutputName) -> Self {
           let segments = [out.as_str(), label.directory().as_str(), name.as_str()];
           let present: Vec<&str> = segments
               .into_iter()
               .filter(|segment| !segment.is_empty())
               .collect();
           Self(present.join("/"))
       }

       /// The path as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for OutputPath {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }
   ```

6. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::output_path
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 322 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree with this plan and the three after it landed; none of them changes a manifest.

8. Commit:

   ```
   $ git add crates/buildl-core/src/types/output_path.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the OutputPath type"
   ```

### Task 2 — Add `ReadyAction`, `RawOutcome` and `OnFailure`

**Files:**
- Create `crates/buildl-core/src/types/ready_action.rs`
- Create `crates/buildl-core/src/types/raw_outcome.rs`
- Create `crates/buildl-core/src/types/on_failure.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/ready_action.rs`:

   ```rust
   //! One action, ready to run: everything an execution strategy is handed for one target.
   //!
   //! Its own file because it is the value that crosses the execution port. A strategy is told the
   //! command to run, the variables to run it with, and which workspace-relative paths the command
   //! reads and writes; it learns nothing else about the build.
   //!
   //! Responsibilities: [`ReadyAction`].
   //!
   //! Non-responsibilities: expanding the command's placeholders and laying out the output paths,
   //! which the phase that executes does before it builds this value; and running anything, which
   //! is a port's.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   use std::collections::BTreeMap;

   use crate::types::{Command, EnvName, EnvValue, Label, NetworkAccess, OutputPath, SourcePath};

   /// One target's action as an execution strategy receives it.
   ///
   /// Every path is relative to the workspace root, and the command already names those same
   /// paths: no placeholder is left in it.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct ReadyAction {
       /// The target, for the strategy's own diagnostics.
       pub label: Label,
       /// The command to run, as an argument vector with every placeholder expanded.
       pub command: Command,
       /// The environment the command runs with: each variable the target reads that has a value.
       pub env: BTreeMap<EnvName, EnvValue>,
       /// The workspace files the command reads, in the order the target wrote them.
       pub inputs: Vec<SourcePath>,
       /// Every output of every dependency, in the order the dependencies were written, each
       /// dependency's outputs in the order it declares them.
       pub deps: Vec<OutputPath>,
       /// Where the command must leave each output the target declares, in declared order.
       pub outputs: Vec<OutputPath>,
       /// Whether the command may reach the network.
       pub network: NetworkAccess,
   }
   ```

2. Create `crates/buildl-core/src/types/raw_outcome.rs`:

   ```rust
   //! What an execution strategy reports for one action it ran.
   //!
   //! Its own file because it is the value that comes back across the execution port: how the
   //! command ended, and what it printed. It is raw because nothing in it is judged yet; whether a
   //! build succeeded is decided from it, not by the strategy.
   //!
   //! Responsibilities: [`RawOutcome`] and [`Exit`].
   //!
   //! Non-responsibilities: classifying the outcome, and the files the command produced, which a
   //! strategy stores separately. A command that could not be started at all is not an outcome:
   //! the strategy returns an error for it.
   //!
   //! This file holds only type definitions, so it carries no logic to unit-test.

   /// How a command that was started came to an end.
   ///
   /// Non-exhaustive: a strategy that can end a command another way gains a variant.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum Exit {
       /// It exited on its own, with this status. Zero is success.
       Exited(i32),
       /// It ran longer than it was allowed to and was stopped.
       TimedOut,
   }

   /// What running one command produced, apart from its output files.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct RawOutcome {
       /// How the command ended.
       pub exit: Exit,
       /// Everything it wrote to standard output.
       pub stdout: Vec<u8>,
       /// Everything it wrote to standard error.
       pub stderr: Vec<u8>,
   }
   ```

3. Create `crates/buildl-core/src/types/on_failure.rs`:

   ```rust
   //! What a build does with the rest of its targets once one has failed.
   //!
   //! Its own file because it is the one choice a caller makes about a build that is not in the
   //! workspace: the same graph is built either way, and only what happens after a failure differs.
   //! An enum rather than a flag, so a call site reads as the policy it chose.
   //!
   //! Responsibilities: [`OnFailure`].
   //!
   //! Non-responsibilities: applying the policy, which the phase that executes does.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   /// What a build does once a target has failed.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum OnFailure {
       /// Begin nothing more: every target not yet begun is skipped, and an action already running
       /// finishes.
       Stop,
       /// Skip only the targets that depend on the failed one, directly or through others, and
       /// build the rest.
       KeepGoing,
   }
   ```

4. Declare the modules. In `crates/buildl-core/src/types/mod.rs`, add `pub mod on_failure;`, `pub mod raw_outcome;` and `pub mod ready_action;` to the `pub mod` lines, add `pub use on_failure::OnFailure;`, `pub use raw_outcome::{Exit, RawOutcome};` and `pub use ready_action::ReadyAction;` to the `pub use` lines, and add two bullets to the module doc, directly below the `OutputPath` bullet task 1 added:

   ```rust
   //! - [`ReadyAction`] — one action as an execution strategy is handed it; [`RawOutcome`] and
   //!   [`Exit`] — what the strategy reports back.
   //! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
   ```

5. Re-export them. In `crates/buildl-core/src/lib.rs`, add `Exit`, `OnFailure`, `RawOutcome` and `ReadyAction` to the `pub use types::{…}` list, then run `cargo fmt --all`.

6. Confirm the four `pub use` lines of this plan are in place:

   ```
   $ grep -c -e 'pub use on_failure::OnFailure;' -e 'pub use output_path::OutputPath;' -e 'pub use raw_outcome::{Exit, RawOutcome};' -e 'pub use ready_action::ReadyAction;' crates/buildl-core/src/types/mod.rs
   4
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
   $ git add crates/buildl-core/src/types/ready_action.rs crates/buildl-core/src/types/raw_outcome.rs crates/buildl-core/src/types/on_failure.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add ReadyAction, RawOutcome and OnFailure"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `OutputPath::of` yields the four paths of the spec's table — `out/app`, `out/lib/text`,
  `out/bin/z`, and `lib/text` for a root output directory — orders as its text does, and
  serializes as its text. It has no `parse` and no `Deserialize`.
- `ReadyAction` holds the seven public fields of spec §3.2; `RawOutcome` holds `exit`, `stdout`
  and `stderr`; `Exit` is `Copy` and non-exhaustive with `Exited(i32)` and `TimedOut`;
  `OnFailure` is `Copy` with `Stop` and `KeepGoing`.
- `buildl_core::{OutputPath, ReadyAction, RawOutcome, Exit, OnFailure}` resolve from the crate
  root.
- `crates/buildl-core/Cargo.toml` and `crates/expected-module-edges.txt` are unchanged: the new
  files import only `crate::types` and `serde`.

## Review findings

Reviewed once, 2026-10-12, over both tasks. The reviewer re-ran the gate: `cargo make dod`
exit 0 (`[cargo-make] INFO - Build Done in 5.13 seconds.`), `cargo deny check` exit 0
(`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan: no drift.

- risk — the new newtype `OutputPath` ships no doctest — `types/output_path.rs:25`. Left as it
  is: the crate's other newtypes carry none and the gate does not ask for one.
- nit — nothing fails if one of the five new names is dropped from the crate-root re-export
  list — `lib.rs:90`. Left as it is.
- nit — the docs of the three definition-only files describe a phase and a port that are not in
  the tree yet, so they cannot be checked against code today — `types/on_failure.rs:3`. Left as
  it is: plans 05 and 09 of this chain add them.
- nit — "no placeholder is left in it" is a guarantee `ReadyAction` does not enforce —
  `types/ready_action.rs:21`. Left as it is: the type is a plain handoff value; plan 06's
  `expand` is what makes it true.

## Probe results

No separate probe was run. Every fact the tasks assert about existing code (`Directory`,
`Label`, `OutputName` and their methods, the `types/mod.rs` and `lib.rs` lists) is one the
tasks' own compile and test runs depend on:

- Task 1 red: `cargo test -p buildl-core --lib types::output_path` exit 101,
  `error[E0432]: unresolved import `super::OutputPath`` and
  `error[E0432]: unresolved import `output_path::OutputPath`` — as the plan quotes.
- Task 1 green: `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 322 filtered out`
  — as the plan quotes.
- Task 2: the `grep -c` over `types/mod.rs` printed `4` — as the plan quotes.

## Deviations

- 2026-10-12 — the commit step of both tasks was skipped. The work is in the tree, uncommitted;
  committing is left to the author.
