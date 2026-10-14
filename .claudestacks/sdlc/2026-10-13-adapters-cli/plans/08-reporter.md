---
status: done
created: 2026-10-14
depends-on: [04]
---

# Reporter Implementation Plan

**Goal:** Everything `buildl` shows a person is rendered by its `report/` module.

**Architecture:** Seven files under `crates/buildl/src/report/` beside an export-only `mod.rs`. The module names `buildl_core` and `std` and nothing else of the crate. Five files hold pure renderings, which return text and write nowhere: `paint.rs` (`Paint`, the only place a colour sequence is written: bold for a header, bold red for the words `FAILED` and `error`), `block.rs` (`command_text`, `summary_line`, `trailer`, `block`), `plan_text.rs` (`plan_text`), `dot.rs` (`graph_dot`) and `message.rs` (`check_line`, `error_text`). Two hold the adapters of the `Reporter` port: `terminal.rs` (`TerminalReporter<W: Write>` and `Mode`) and `silent.rs` (`Silent`, every method empty). `TerminalReporter` is given its stream, its mode and its paint as values and reads nothing from the process, so every test captures what it writes in a `Vec<u8>` and none needs a terminal; whether the stream is a terminal and whether `NO_COLOR` is set are the caller's to read. Each call gathers what it has to say into one buffer and hands it to the stream in one `write_all`, then flushes. A write or a flush that fails sets a flag, after which nothing more is written; no call fails and a build is not told. In `Mode::Live` a status line, `[2/4] 1 running: compile main.c`, stands on the last row: `\r\x1b[2K` erases it before anything is printed, and `\x1b[?7l` + text + `\x1b[?7h` draws it again after, so auto-wrap is off only inside that one write and no width is probed. `Mode::Plain` writes blocks and the summary alone. The reporter has no queue: the core guarantees that between a target's first `printed` and its `finished` nothing about another target is reported, so `printed` writes the target's header once and then the bytes as they come, and `finished` writes either the live target's closing newline and trailer, or another target's whole block. What an action printed is written as bytes, valid UTF-8 or not, with a newline added where the last byte is not one. Every rendering that is a whole line or more ends with a newline, so a caller writes it as it is. The twelve public items are re-exported from `report/mod.rs`.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `std` and `buildl-core` (already a dependency of `buildl`), `cargo-make`, `cargo-deny`. No new dependency and no manifest change.

**Content authority:** spec §7 (the whole section); §9.2 (what `check`, `graph --dot`, `plan` and `build` print: the block, trailer, class-text, argv and summary rules, the plan-text table, the `--stream` rule); §9.4 (the `error_text` paragraph and its two special cases); §2.2 (the `Reporter` contract these adapters implement); §3 and §13.2 (what a file of `report/` may import, and in which spelling); §12.2 (the `report/` tests, each under its name); §17 (the one fact with no test on either platform: how a terminal treats `ESC[?7l`).

---

## Context an implementer needs

What the plans before this one leave, and this plan builds on. Every `buildl-core` item below is re-exported at the crate root and is written `buildl_core::<Item>`.

| Item | Where | Used as |
|---|---|---|
| `Reporter`: `begun(&mut self, usize)`, `started(&mut self, &ReadyAction)`, `printed(&mut self, &Label, Channel, &[u8])`, `finished(&mut self, &OutcomeEvent, Captured<'_>)`, `summary(&mut self, Summary)`; none can fail | `crates/buildl-core/src/ports/reporter.rs` | the trait `TerminalReporter` and `Silent` implement |
| `OutcomeEvent { at, provenance, command, key, started, finished, outcome }` and `ActionOutcome { label, class, diagnostic, outputs, stdout, stderr }`, all fields public | `crates/buildl-core/src/types/outcome_event.rs`, `outcome.rs` | what `finished`, `trailer` and `block` are handed |
| `OutcomeClass` (`Cached`, `Built`, `ActionFailure { status }`, `Killed { signal }`, `OutputMissing { name }`, `OutputRefused { name, refusal }`, `InfraFailure { kind }`, `Skipped { because }`), `InfraKind` (`Prepare`, `Run`, `TimedOut`, `Store`), `OutputRefusal` (`Symlink`, `Special`, `EmptyDirectory`, `NotText`); each `#[non_exhaustive]` | `crates/buildl-core/src/types/outcome.rs` | what the first line of a trailer is chosen from |
| `Captured<'a> { stdout: &'a [u8], stderr: &'a [u8] }` (`Copy`, `Default`); `Channel::{Stdout, Stderr}` | `crates/buildl-core/src/types/captured.rs`, `channel.rs` | what an action printed, as `finished` and `printed` hand it over |
| `Summary { failed, skipped, built, cached }` (`Copy`) | `crates/buildl-core/src/types/outcome.rs` | what `summary_line` counts |
| `ReadyAction`, all fields public; `description: Option<ExpandedDescription>` | `crates/buildl-core/src/types/ready_action.rs` | what `started` is handed; the status text |
| `Command::arguments() -> &[Argument]`, `Argument::as_str()`; `Label`, `Provenance` and `Diagnostic` each implement `Display` | `crates/buildl-core/src/types/` | what `command_text` and `trailer` read |
| `Plan::verdicts()`, `Plan::label(NodeId)`; `Verdict`, `DirtyReason`, `PlannedKey`, `Unfinished`; `Difference` (`#[non_exhaustive]`); `OwnComponents` with `tool: Option<ToolIdentity>` | `crates/buildl-core/src/types/plan.rs`, `action_key.rs` | what `plan_text` reads |
| `TargetGraph::nodes()`, `id_of(&Label)`, `deps(NodeId)`, `node(NodeId)`, `aliases()` | `crates/buildl-core/src/types/target_graph.rs` | what `graph_dot` reads |
| `Declaration::provenance()`, `Provenance::file()` | `crates/buildl-core/src/types/declaration.rs`, `provenance.rs` | what `check_line` counts |
| `Error` (`#[non_exhaustive]`) with `Nondeterministic { provenance, first_run, second_run }` and `CorruptState { file, source }`; `json::canonical::to_string` | `crates/buildl-core/src/error.rs`, `json/canonical.rs` | what `error_text` reads |
| `resolve(Vec<Declaration>) -> Result<TargetGraph>`; `plan(&TargetGraph, &BTreeMap<SettingName, SettingValue>, &mut PortSet<P>) -> Result<Plan>`; `Ports`, `PortSet` and the fourteen port traits | `crates/buildl-core/src/resolve/`, `plan/`, `ports/` | how the tests of `dot.rs` and `plan_text.rs` get a graph and a plan: neither value has a public constructor |
| `crates/buildl/src/report/mod.rs` holding only a module doc comment, and the line `pub mod report;` in `crates/buildl/src/lib.rs` | the crate skeleton, plan `04` | what task 1 starts from; `lib.rs` is not touched here |

Facts that bite here:

- The workspace's clippy settings refuse `text.push_str(&format!(…))` (`clippy::format_push_string`). Every rendering of several lines therefore collects a `Vec<String>` and joins it, as the error texts of `buildl-core` do.
- `cargo make clippy` runs `guard-adapter-isolation` (plan `04`), which reads test code too. A file of `report/` names a sibling's item as `crate::report::<file>::<item>` and, in its test module, its own as `super::<item>`. `mod.rs` never writes `super::`; nothing writes `crate::{…}`, `super::super` or another module of the crate. The guard's own script was run over this module after every task while the code was prototyped and printed `guard-adapter-isolation: OK (0 edges)` each time.
- `OutcomeClass`, `InfraKind`, `OutputRefusal`, `Difference` and `Error` are `#[non_exhaustive]`, so a `match` on one needs a wildcard arm. No test reaches such an arm: there is no variant to build for it.
- `Plan` and `TargetGraph` have no public constructor. `dot.rs` gets a graph from `resolve` over hand-built declarations. `plan_text.rs` gets a plan from `plan` over a `PortSet` of small stand-in ports written in its test module, with the rows of earlier builds built by hand from `ActionKey::new` and `CacheRow::new`.
- The four renderings of `block.rs` and the two methods of `Paint` are `pub`: a public item is never reported as unused, so each task compiles clean before the task that calls its item. Every private helper is called in the task that adds it, and so is `header`, which is `pub(crate)` and is called by `block`.
- Task 6 leaves two things for the two tasks after it, both inside `terminal.rs`: `printed` is an empty method until task 8, and a failed write is ignored until task 7. Nothing constructs a `TerminalReporter` outside its own tests before this plan is done.
- The new tests of tasks 7 and 8 compile against what the task before left and fail by assertion. Every other failing-test step fails to build.
- In a test module, a helper stands directly above the first tests that need it, in the order the tasks add them.
- A code block under a numbered step is indented three spaces for the list. The text of the file starts at the block's own first column.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the lines that name the cause, not in full. After every task `cargo fmt -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` and `cargo test --all-targets` were run for the crate and exited `0`; on the tree this plan leaves, `cargo make dod` ended `Build Done` and `cargo deny check` printed `advisories ok, bans ok, licenses ok, sources ok`. The `filtered out` figure in a quoted test result counts the other tests of the `buildl` crate, so it depends on which of plans `05` to `07` have landed; the `passed` figure does not.

## File structure

```
crates/buildl/src/report/mod.rs         — [modify] module doc, the seven `mod` lines, the twelve re-exports
crates/buildl/src/report/paint.rs       — [create] Paint
crates/buildl/src/report/block.rs       — [create] command_text, summary_line, trailer, block
crates/buildl/src/report/terminal.rs    — [create] Mode, TerminalReporter
crates/buildl/src/report/silent.rs      — [create] Silent
crates/buildl/src/report/plan_text.rs   — [create] plan_text
crates/buildl/src/report/dot.rs         — [create] graph_dot
crates/buildl/src/report/message.rs     — [create] check_line, error_text
```

Every task ends the same way: the task's own tests, then `cargo fmt --all -- --check`, `cargo make dod` and `cargo deny check`, then one commit.

### Task 1 — Add `Paint`

Colour is two styles, bold and bold red, applied by one value, so that no rendering writes an escape sequence of its own and `Paint::Plain` can promise that none is written.

**Files:**
- Create `crates/buildl/src/report/paint.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl/src/report/mod.rs` holds only a module doc comment so far. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod paint;

   pub use paint::Paint;
   ```

2. Write the failing tests. Create `crates/buildl/src/report/paint.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use super::Paint;

       #[test]
       fn plain_leaves_text_as_it_is() {
           assert_eq!(Paint::Plain.header("--- //:app"), "--- //:app");
           assert_eq!(Paint::Plain.failure("FAILED"), "FAILED");
       }

       #[test]
       fn a_header_is_bold() {
           assert_eq!(Paint::Ansi.header("--- //:app"), "\x1b[1m--- //:app\x1b[0m");
       }

       #[test]
       fn a_failure_is_bold_red() {
           assert_eq!(Paint::Ansi.failure("FAILED"), "\x1b[1;31mFAILED\x1b[0m");
       }

       #[test]
       fn styled_text_ends_its_style_even_when_empty() {
           assert_eq!(Paint::Ansi.header(""), "\x1b[1m\x1b[0m");
           assert_eq!(Paint::Plain.header(""), "");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::paint
   error[E0432]: unresolved import `super::Paint`
   error[E0432]: unresolved import `paint::Paint`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/paint.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Whether what a build shows is coloured.
   //!
   //! Its own file because colour is decided once, by whoever knows where the text goes, and then
   //! applied by several renderings: the header of a block, the word that marks a failed target,
   //! the word that opens an error. One value carries the decision to all of them, so none of them
   //! writes an escape sequence of its own.
   //!
   //! Responsibilities: [`Paint`] and the two styles it applies.
   //!
   //! Non-responsibilities: deciding whether to colour. That takes knowing whether the text goes
   //! to a terminal and what the person asked for, which only the caller knows.

   /// The sequence that makes what follows bold.
   const BOLD: &str = "\x1b[1m";
   /// The sequence that makes what follows bold and red.
   const BOLD_RED: &str = "\x1b[1;31m";
   /// The sequence that ends either style.
   const RESET: &str = "\x1b[0m";

   /// Whether text is styled with terminal colour sequences.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Paint {
       /// Text is written as it is: no escape sequence ever.
       Plain,
       /// A header is bold, and a word that marks a failure is bold red.
       Ansi,
   }

   impl Paint {
       /// `text` as the header of a block: bold under [`Paint::Ansi`], unchanged under
       /// [`Paint::Plain`].
       #[must_use]
       pub fn header(self, text: &str) -> String {
           self.styled(BOLD, text)
       }

       /// `text` as a word that marks a failure: bold red under [`Paint::Ansi`], unchanged under
       /// [`Paint::Plain`].
       #[must_use]
       pub fn failure(self, text: &str) -> String {
           self.styled(BOLD_RED, text)
       }

       /// `text` between `style` and the sequence that ends it, or alone when nothing is styled.
       fn styled(self, style: &str, text: &str) -> String {
           match self {
               Self::Plain => text.to_owned(),
               Self::Ansi => format!("{style}{text}{RESET}"),
           }
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::paint
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/paint.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): add Paint, the colour of what a build shows"
   ```

### Task 2 — Render a command on one line

A failure report shows the command that failed. Its arguments are joined by a space, and one that would not read as a single word is written as a Rust string literal.

**Files:**
- Create `crates/buildl/src/report/block.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;

   pub use block::command_text;
   pub use paint::Paint;
   ```

2. Write the failing tests. Create `crates/buildl/src/report/block.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use buildl_core::{Argument, Command};

       use super::command_text;

       fn command(arguments: &[&str]) -> Command {
           Command::new(
               arguments
                   .iter()
                   .map(|argument| Argument::parse(*argument).unwrap())
                   .collect(),
           )
           .unwrap()
       }

       #[test]
       fn an_argument_with_a_space_is_quoted() {
           assert_eq!(
               command_text(&command(&["cc", "--doc", "README", "a b"])),
               r#"cc --doc README "a b""#
           );
       }

       #[test]
       fn an_argument_that_would_not_read_as_one_word_is_a_string_literal() {
           assert_eq!(
               command_text(&command(&["echo", "", "a\tb", r#"say "hi""#, r"a\b"])),
               r#"echo "" "a\tb" "say \"hi\"" "a\\b""#
           );
       }

       #[test]
       fn plain_arguments_are_joined_by_one_space() {
           assert_eq!(
               command_text(&command(&["cc", "-c", "lib/text.c", "-o", "out/lib/text"])),
               "cc -c lib/text.c -o out/lib/text"
           );
           assert_eq!(command_text(&command(&["true"])), "true");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::block
   error[E0432]: unresolved import `super::command_text`
   error[E0432]: unresolved import `block::command_text`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/block.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The pieces of text a build's report is made of.
   //!
   //! Its own file because these are pure renderings: each turns a value a build hands over into
   //! the text a person reads, and none of them writes anywhere. The reporter that owns the
   //! terminal decides when something is written; what it reads is decided here, so a caller with
   //! a reporter of its own can show the same text.
   //!
   //! Responsibilities:
   //!
   //! - [`command_text`] — a command on one line.
   //!
   //! Non-responsibilities: writing, and anything a terminal needs beyond colour. The status line
   //! and the order blocks appear in belong to the reporter.

   use buildl_core::Command;

   /// A command on one line: its arguments joined by a space.
   ///
   /// An argument that is empty, or that holds whitespace, a control character, `"` or `\`, is
   /// written as a Rust string literal, so that the line shows where each argument begins and
   /// ends. Every other argument is written as it is.
   #[must_use]
   pub fn command_text(command: &Command) -> String {
       let shown: Vec<String> = command
           .arguments()
           .iter()
           .map(|argument| {
               let text = argument.as_str();
               if reads_as_one_word(text) {
                   text.to_owned()
               } else {
                   format!("{text:?}")
               }
           })
           .collect();
       shown.join(" ")
   }

   /// Whether `text` can be written unquoted and still be read as one argument.
   fn reads_as_one_word(text: &str) -> bool {
       !text.is_empty()
           && !text.chars().any(|letter| {
               letter.is_whitespace() || letter.is_control() || letter == '"' || letter == '\\'
           })
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::block
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/block.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render a command on one line"
   ```

### Task 3 — Render the summary line

The last line of a build names each count that is not zero, in a fixed order, and says `nothing to build` when all are zero.

**Files:**
- Modify `crates/buildl/src/report/block.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;

   pub use block::{command_text, summary_line};
   pub use paint::Paint;
   ```

2. Write the failing tests. In the test module of `crates/buildl/src/report/block.rs`, replace everything from the line `mod tests {` down to, but not including, the line `fn command(arguments: &[&str]) -> Command {` with this, leaving one blank line before that line:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use buildl_core::{Argument, Command, Summary};

       use super::{command_text, summary_line};
   ```

   Then add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       const fn counts(failed: usize, skipped: usize, built: usize, cached: usize) -> Summary {
           Summary {
               failed,
               skipped,
               built,
               cached,
           }
       }

       #[test]
       fn the_summary_leaves_out_zero_counts() {
           assert_eq!(
               summary_line(counts(2, 1, 0, 1)),
               "2 failed, 1 skipped, 1 cached\n"
           );
           assert_eq!(
               summary_line(counts(1, 2, 0, 1)),
               "1 failed, 2 skipped, 1 cached\n"
           );
           assert_eq!(summary_line(counts(0, 0, 4, 0)), "4 built\n");
           assert_eq!(summary_line(counts(0, 0, 0, 4)), "4 cached\n");
           assert_eq!(summary_line(counts(0, 0, 2, 1)), "2 built, 1 cached\n");
       }

       #[test]
       fn the_summary_names_every_count_in_a_fixed_order() {
           assert_eq!(
               summary_line(counts(1, 2, 3, 4)),
               "1 failed, 2 skipped, 3 built, 4 cached\n"
           );
       }

       #[test]
       fn a_build_of_nothing_says_so() {
           assert_eq!(summary_line(counts(0, 0, 0, 0)), "nothing to build\n");
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::block
   error[E0432]: unresolved import `super::summary_line`
   error[E0432]: unresolved import `block::summary_line`
   ```

4. Implement it. Replace the top of `crates/buildl/src/report/block.rs`, everything above the line `/// A command on one line: its arguments joined by a space.`, with this, leaving one blank line before that line:

   ```rust
   //! The pieces of text a build's report is made of.
   //!
   //! Its own file because these are pure renderings: each turns a value a build hands over into
   //! the text a person reads, and none of them writes anywhere. The reporter that owns the
   //! terminal decides when something is written; what it reads is decided here, so a caller with
   //! a reporter of its own can show the same text.
   //!
   //! Responsibilities:
   //!
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //!
   //! Non-responsibilities: writing, and anything a terminal needs beyond colour. The status line
   //! and the order blocks appear in belong to the reporter.

   use buildl_core::{Command, Summary};
   ```

   Then add directly above the line `#[cfg(test)]`, followed by one blank line:

   ```rust
   /// The line that closes a build: each count that is not zero, as `2 failed, 1 skipped, 3 built,
   /// 4 cached`, in that order, and `nothing to build` when every count is zero. Ends with a
   /// newline.
   #[must_use]
   pub fn summary_line(summary: Summary) -> String {
       let counts: Vec<String> = [
           (summary.failed, "failed"),
           (summary.skipped, "skipped"),
           (summary.built, "built"),
           (summary.cached, "cached"),
       ]
       .into_iter()
       .filter(|(count, _)| *count > 0)
       .map(|(count, word)| format!("{count} {word}"))
       .collect();
       if counts.is_empty() {
           "nothing to build\n".to_owned()
       } else {
           format!("{}\n", counts.join(", "))
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::block
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/block.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render the summary line of a build"
   ```

### Task 4 — Render the trailer of a failed target

A failed target is closed by a trailer: `FAILED`, the label and the class in words, then the diagnostic when there is one, the build file and the command. A target that was built, cached or skipped has none.

**Files:**
- Modify `crates/buildl/src/report/block.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;

   pub use block::{command_text, summary_line, trailer};
   pub use paint::Paint;
   ```

2. Write the failing tests. In the test module of `crates/buildl/src/report/block.rs`, replace everything from the line `mod tests {` down to, but not including, the line `fn command(arguments: &[&str]) -> Command {` with this, leaving one blank line before that line:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use buildl_core::{
           ActionOutcome, Argument, Command, Diagnostic, Directory, InfraKind, Label, OutcomeClass,
           OutcomeEvent, OutputName, OutputRefusal, Provenance, Summary, Timestamp,
       };

       use super::{command_text, summary_line, trailer};
       use crate::report::paint::Paint;
   ```

   Then add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       /// The event of `//lib:text`, declared in `lib/build.lua`, ending in `class`.
       fn event(class: OutcomeClass, diagnostic: Option<&str>) -> OutcomeEvent {
           OutcomeEvent {
               at: Timestamp::UNIX_EPOCH,
               provenance: Provenance::new(
                   PathBuf::from("lib/build.lua"),
                   Directory::parse("lib").unwrap(),
               ),
               command: command(&["cc", "-c", "lib/text.c", "-o", "out/lib/text"]),
               key: None,
               started: None,
               finished: None,
               outcome: ActionOutcome {
                   label: Label::parse("//lib:text").unwrap(),
                   class,
                   diagnostic: diagnostic.map(Diagnostic::new),
                   outputs: Vec::new(),
                   stdout: None,
                   stderr: None,
               },
           }
       }

       /// The first line of the trailer of a target that ended in `class`.
       fn first_line(class: OutcomeClass) -> String {
           trailer(&event(class, None), Paint::Plain)
               .lines()
               .next()
               .unwrap()
               .to_owned()
       }

       #[test]
       fn a_target_that_did_not_fail_has_no_trailer() {
           for class in [
               OutcomeClass::Built,
               OutcomeClass::Cached,
               OutcomeClass::Skipped {
                   because: Label::parse("//:docs").unwrap(),
               },
           ] {
               assert_eq!(trailer(&event(class, None), Paint::Plain), "");
           }
       }

       #[test]
       fn a_trailer_names_the_class_the_build_file_and_the_command() {
           assert_eq!(
               trailer(
                   &event(OutcomeClass::ActionFailure { status: 1 }, None),
                   Paint::Plain
               ),
               concat!(
                   "FAILED //lib:text: action failure (exit status 1)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
           );
       }

       #[test]
       fn a_diagnostic_comes_between_the_class_and_the_build_file() {
           let failed = event(
               OutcomeClass::InfraFailure {
                   kind: InfraKind::Run,
               },
               Some("exec strategy: //lib:text: the command cannot be started"),
           );
           assert_eq!(
               trailer(&failed, Paint::Plain),
               concat!(
                   "FAILED //lib:text: infrastructure failure (the command could not be started)\n",
                   "    exec strategy: //lib:text: the command cannot be started\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
           );
       }

       #[test]
       fn every_line_of_a_diagnostic_is_indented() {
           let failed = event(
               OutcomeClass::InfraFailure {
                   kind: InfraKind::Store,
               },
               Some("content store: disk full\nwhile storing out/lib/text"),
           );
           assert_eq!(
               trailer(&failed, Paint::Plain),
               concat!(
                   "FAILED //lib:text: infrastructure failure (what it produced could not be stored)\n",
                   "    content store: disk full\n",
                   "    while storing out/lib/text\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
           );
       }

       #[test]
       fn an_action_a_signal_ended_says_which() {
           assert_eq!(
               first_line(OutcomeClass::Killed { signal: 11 }),
               "FAILED //lib:text: action failure (ended by signal 11)"
           );
       }

       #[test]
       fn every_failure_class_has_its_words() {
           let name = OutputName::parse("text").unwrap();
           assert_eq!(
               first_line(OutcomeClass::ActionFailure { status: 3 }),
               "FAILED //lib:text: action failure (exit status 3)"
           );
           assert_eq!(
               first_line(OutcomeClass::OutputMissing { name: name.clone() }),
               "FAILED //lib:text: action failure (declared output text was not produced)"
           );
           for (kind, words) in [
               (
                   InfraKind::Prepare,
                   "the directory to run it in could not be made",
               ),
               (InfraKind::Run, "the command could not be started"),
               (
                   InfraKind::TimedOut,
                   "the command ran longer than the timeout",
               ),
               (InfraKind::Store, "what it produced could not be stored"),
           ] {
               assert_eq!(
                   first_line(OutcomeClass::InfraFailure { kind }),
                   format!("FAILED //lib:text: infrastructure failure ({words})")
               );
           }
           for (refusal, words) in [
               (OutputRefusal::Symlink, "it is or holds a symbolic link"),
               (
                   OutputRefusal::Special,
                   "it is or holds a device, a socket or a named pipe",
               ),
               (
                   OutputRefusal::EmptyDirectory,
                   "it is or holds an empty directory",
               ),
               (
                   OutputRefusal::NotText,
                   "it holds a name that is not valid Unicode",
               ),
           ] {
               assert_eq!(
                   first_line(OutcomeClass::OutputRefused {
                       name: name.clone(),
                       refusal,
                   }),
                   format!("FAILED //lib:text: action failure (output text cannot be kept: {words})")
               );
           }
       }

       #[test]
       fn the_word_failed_is_bold_red_when_painted() {
           assert_eq!(
               trailer(
                   &event(OutcomeClass::ActionFailure { status: 1 }, None),
                   Paint::Ansi
               ),
               concat!(
                   "\x1b[1;31mFAILED\x1b[0m //lib:text: action failure (exit status 1)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
           );
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::block
   error[E0432]: unresolved import `super::trailer`
   error[E0432]: unresolved import `block::trailer`
   ```

4. Implement it. Replace the top of `crates/buildl/src/report/block.rs`, everything above the line `/// A command on one line: its arguments joined by a space.`, with this, leaving one blank line before that line:

   ```rust
   //! The pieces of text a build's report is made of.
   //!
   //! Its own file because these are pure renderings: each turns a value a build hands over into
   //! the text a person reads, and none of them writes anywhere. The reporter that owns the
   //! terminal decides when something is written; what it reads is decided here, so a caller with
   //! a reporter of its own can show the same text.
   //!
   //! Responsibilities:
   //!
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //!
   //! Non-responsibilities: writing, and anything a terminal needs beyond colour. The status line
   //! and the order blocks appear in belong to the reporter.

   use buildl_core::{Command, InfraKind, OutcomeClass, OutcomeEvent, OutputRefusal, Summary};

   use crate::report::paint::Paint;
   ```

   Then add directly above the line `#[cfg(test)]`, followed by one blank line:

   ```rust
   /// What the builder could not do, in words.
   const fn infra_text(kind: InfraKind) -> &'static str {
       match kind {
           InfraKind::Prepare => "the directory to run it in could not be made",
           InfraKind::Run => "the command could not be started",
           InfraKind::TimedOut => "the command ran longer than the timeout",
           InfraKind::Store => "what it produced could not be stored",
           _ => "the builder could not build it",
       }
   }

   /// Why an output cannot be kept, in words.
   const fn refusal_text(refusal: OutputRefusal) -> &'static str {
       match refusal {
           OutputRefusal::Symlink => "it is or holds a symbolic link",
           OutputRefusal::Special => "it is or holds a device, a socket or a named pipe",
           OutputRefusal::EmptyDirectory => "it is or holds an empty directory",
           OutputRefusal::NotText => "it holds a name that is not valid Unicode",
           _ => "it cannot be put back exactly as it was",
       }
   }

   /// How a failed target ended, in words; `None` for a target that did not fail: one that was
   /// built, cached or skipped.
   fn failure_text(class: &OutcomeClass) -> Option<String> {
       match class {
           OutcomeClass::Cached | OutcomeClass::Built | OutcomeClass::Skipped { .. } => None,
           OutcomeClass::ActionFailure { status } => {
               Some(format!("action failure (exit status {status})"))
           }
           OutcomeClass::Killed { signal } => {
               Some(format!("action failure (ended by signal {signal})"))
           }
           OutcomeClass::OutputMissing { name } => Some(format!(
               "action failure (declared output {name} was not produced)"
           )),
           OutcomeClass::OutputRefused { name, refusal } => Some(format!(
               "action failure (output {name} cannot be kept: {})",
               refusal_text(*refusal)
           )),
           OutcomeClass::InfraFailure { kind } => {
               Some(format!("infrastructure failure ({})", infra_text(*kind)))
           }
           _ => Some("a failure this buildl cannot name".to_owned()),
       }
   }

   /// What closes the report of a failed target; empty for a target that did not fail.
   ///
   /// The first line is `FAILED`, the label and how the target ended. Then, each indented by four
   /// spaces: the outcome's diagnostic when it has one, the build file that declared the target,
   /// and its command. Every line ends with a newline. Under [`Paint::Ansi`] the word `FAILED` is
   /// bold red.
   #[must_use]
   pub fn trailer(event: &OutcomeEvent, paint: Paint) -> String {
       let Some(ended) = failure_text(&event.outcome.class) else {
           return String::new();
       };
       let mut lines = vec![format!(
           "{} {}: {ended}\n",
           paint.failure("FAILED"),
           event.outcome.label
       )];
       if let Some(diagnostic) = &event.outcome.diagnostic {
           lines.extend(
               diagnostic
                   .as_str()
                   .lines()
                   .map(|line| format!("    {line}\n")),
           );
       }
       lines.push(format!("    declared in {}\n", event.provenance));
       lines.push(format!("    command: {}\n", command_text(&event.command)));
       lines.concat()
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::block
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/block.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render the trailer of a failed target"
   ```

### Task 5 — Render the block of a finished target

A block is a header and what the action printed, standard output then standard error, only when it printed something; a failure adds its trailer whether or not it printed. The bytes pass as they are.

**Files:**
- Modify `crates/buildl/src/report/block.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;

   pub use block::{block, command_text, summary_line, trailer};
   pub use paint::Paint;
   ```

2. Write the failing tests. In the test module of `crates/buildl/src/report/block.rs`, replace everything from the line `mod tests {` down to, but not including, the line `fn command(arguments: &[&str]) -> Command {` with this, leaving one blank line before that line:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use buildl_core::{
           ActionOutcome, Argument, Captured, Command, Diagnostic, Directory, InfraKind, Label,
           OutcomeClass, OutcomeEvent, OutputName, OutputRefusal, Provenance, Summary, Timestamp,
       };

       use super::{block, command_text, summary_line, trailer};
       use crate::report::paint::Paint;
   ```

   Then add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       const fn printed<'a>(stdout: &'a [u8], stderr: &'a [u8]) -> Captured<'a> {
           Captured { stdout, stderr }
       }

       #[test]
       fn a_silent_success_prints_nothing() {
           for class in [
               OutcomeClass::Built,
               OutcomeClass::Cached,
               OutcomeClass::Skipped {
                   because: Label::parse("//:docs").unwrap(),
               },
           ] {
               assert_eq!(
                   block(&event(class, None), Captured::default(), Paint::Plain),
                   b""
               );
           }
       }

       #[test]
       fn a_block_is_a_header_then_standard_output_then_standard_error() {
           assert_eq!(
               block(
                   &event(OutcomeClass::Built, None),
                   printed(
                       b"compiled 1 file\n",
                       b"lib/text.c:4:9: warning: unused 'n'\n"
                   ),
                   Paint::Plain
               ),
               concat!(
                   "--- //lib:text\n",
                   "compiled 1 file\n",
                   "lib/text.c:4:9: warning: unused 'n'\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn a_cached_target_is_marked() {
           assert_eq!(
               block(
                   &event(OutcomeClass::Cached, None),
                   printed(b"", b"lib/text.c:4:9: warning: unused 'n'\n"),
                   Paint::Plain
               ),
               concat!(
                   "--- //lib:text (cached)\n",
                   "lib/text.c:4:9: warning: unused 'n'\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn a_failure_with_output_has_its_trailer_after_what_it_printed() {
           assert_eq!(
               block(
                   &event(OutcomeClass::ActionFailure { status: 1 }, None),
                   printed(b"", b"lib/text.c:3:1: error: expected ';'\n"),
                   Paint::Plain
               ),
               concat!(
                   "--- //lib:text\n",
                   "lib/text.c:3:1: error: expected ';'\n",
                   "FAILED //lib:text: action failure (exit status 1)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn a_failure_without_output_still_has_a_trailer() {
           assert_eq!(
               block(
                   &event(
                       OutcomeClass::InfraFailure {
                           kind: InfraKind::TimedOut
                       },
                       None
                   ),
                   Captured::default(),
                   Paint::Plain
               ),
               concat!(
                   "FAILED //lib:text: infrastructure failure (the command ran longer than the timeout)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn bytes_that_are_not_text_pass_unchanged() {
           let shown = block(
               &event(OutcomeClass::Cached, None),
               printed(b"wrote 3 pages\ncaf\xe9\n", b""),
               Paint::Plain,
           );
           assert_eq!(shown, b"--- //lib:text (cached)\nwrote 3 pages\ncaf\xe9\n");
           let last_line = shown
               .strip_suffix(b"\n")
               .unwrap()
               .rsplit(|byte| *byte == b'\n')
               .next()
               .unwrap();
           assert_eq!(last_line, [99, 97, 102, 233]);
       }

       #[test]
       fn a_last_line_without_a_newline_gets_one() {
           assert_eq!(
               block(
                   &event(OutcomeClass::Built, None),
                   printed(b"wrote 3 pages\ncaf\xe9", b"1 warning"),
                   Paint::Plain
               ),
               b"--- //lib:text\nwrote 3 pages\ncaf\xe9\n1 warning\n"
           );
       }

       #[test]
       fn a_painted_block_has_a_bold_header_and_a_plain_mark() {
           assert_eq!(
               block(
                   &event(OutcomeClass::Cached, None),
                   printed(b"wrote 3 pages\n", b""),
                   Paint::Ansi
               ),
               b"\x1b[1m--- //lib:text\x1b[0m (cached)\nwrote 3 pages\n"
           );
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::block
   error[E0432]: unresolved import `super::block`
   error[E0432]: unresolved import `block::block`
   ```

4. Implement it. Replace the top of `crates/buildl/src/report/block.rs`, everything above the line `/// A command on one line: its arguments joined by a space.`, with this, leaving one blank line before that line:

   ```rust
   //! The pieces of text a build's report is made of.
   //!
   //! Its own file because these are pure renderings: each turns a value a build hands over into
   //! the text a person reads, and none of them writes anywhere. The reporter that owns the
   //! terminal decides when something is written; what it reads is decided here, so a caller with
   //! a reporter of its own can show the same text.
   //!
   //! Responsibilities:
   //!
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target, and the header it opens with.
   //!
   //! Non-responsibilities: writing, and anything a terminal needs beyond colour. The status line
   //! and the order blocks appear in belong to the reporter.

   use buildl_core::{
       Captured, Command, InfraKind, Label, OutcomeClass, OutcomeEvent, OutputRefusal, Summary,
   };

   use crate::report::paint::Paint;
   ```

   Then add directly above the line `#[cfg(test)]`, followed by one blank line:

   ```rust
   /// The first line of a target's block, without its newline: `--- ` and the label, bold under
   /// [`Paint::Ansi`].
   pub(crate) fn header(label: &Label, paint: Paint) -> String {
       paint.header(&format!("--- {label}"))
   }

   /// Adds `bytes` to `block` as they are, then a newline when they do not end with one.
   fn push_lines(block: &mut Vec<u8>, bytes: &[u8]) {
       block.extend_from_slice(bytes);
       if bytes.last().is_some_and(|last| *last != b'\n') {
           block.push(b'\n');
       }
   }

   /// Everything shown for one target once it has its outcome.
   ///
   /// A target that printed something gets a header, marked ` (cached)` when it did not run, then
   /// what it wrote to standard output, then what it wrote to standard error. The bytes are
   /// written as they are, valid text or not, and each of the two is given a newline when it does
   /// not end with one. A target that failed then gets its [`trailer`], whether or not it printed.
   /// A target that printed nothing and did not fail gets nothing: the result is empty.
   #[must_use]
   pub fn block(event: &OutcomeEvent, printed: Captured<'_>, paint: Paint) -> Vec<u8> {
       let mut block = Vec::new();
       if !(printed.stdout.is_empty() && printed.stderr.is_empty()) {
           block.extend_from_slice(header(&event.outcome.label, paint).as_bytes());
           if matches!(event.outcome.class, OutcomeClass::Cached) {
               block.extend_from_slice(b" (cached)");
           }
           block.push(b'\n');
           push_lines(&mut block, printed.stdout);
           push_lines(&mut block, printed.stderr);
       }
       block.extend_from_slice(trailer(event, paint).as_bytes());
       block
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::block
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/block.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render the block of a finished target"
   ```

### Task 6 — Report blocks, the status line and the summary

`TerminalReporter` writes a block for each finished target that has one and the summary last, and in `Mode::Live` keeps a status line on the last row: erased before anything is printed, drawn again after. Each call is one write. In this task `printed` is an empty method and a failed write is ignored; tasks 7 and 8 change those two.

**Files:**
- Create `crates/buildl/src/report/terminal.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the two types. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use paint::Paint;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. Create `crates/buildl/src/report/terminal.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::io::{self, Write};
       use std::path::PathBuf;

       use buildl_core::{
           ActionOutcome, Argument, Captured, Command, Diagnostic, ExpandedDescription, InfraKind,
           Label, NetworkAccess, OutcomeClass, OutcomeEvent, Provenance, ReadyAction, Reporter,
           Summary, Timestamp,
       };

       use super::{Mode, TerminalReporter};
       use crate::report::paint::Paint;

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       fn command(arguments: &[&str]) -> Command {
           Command::new(
               arguments
                   .iter()
                   .map(|argument| Argument::parse(*argument).unwrap())
                   .collect(),
           )
           .unwrap()
       }

       /// The action of `raw`, shown as `description` while it runs.
       fn action(raw: &str, description: Option<&str>) -> ReadyAction {
           ReadyAction {
               label: label(raw),
               command: command(&["cc"]),
               description: description.map(ExpandedDescription::new),
               env: BTreeMap::new(),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: Vec::new(),
               network: NetworkAccess::Sealed,
           }
       }

       /// The event of `raw`, declared in the build file of its directory, running `arguments`.
       fn event(raw: &str, class: OutcomeClass, arguments: &[&str]) -> OutcomeEvent {
           let label = label(raw);
           let directory = label.directory().clone();
           let file = if directory.as_str().is_empty() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(directory.as_str()).join("build.lua")
           };
           OutcomeEvent {
               at: Timestamp::UNIX_EPOCH,
               provenance: Provenance::new(file, directory),
               command: command(arguments),
               key: None,
               started: None,
               finished: None,
               outcome: ActionOutcome {
                   label,
                   class,
                   diagnostic: None,
                   outputs: Vec::new(),
                   stdout: None,
                   stderr: None,
               },
           }
       }

       /// `event` with the message a builder gave for it.
       fn with_diagnostic(mut event: OutcomeEvent, text: &str) -> OutcomeEvent {
           event.outcome.diagnostic = Some(Diagnostic::new(text));
           event
       }

       const fn nothing() -> Captured<'static> {
           Captured {
               stdout: b"",
               stderr: b"",
           }
       }

       const fn on_stdout(bytes: &[u8]) -> Captured<'_> {
           Captured {
               stdout: bytes,
               stderr: b"",
           }
       }

       const fn on_stderr(bytes: &[u8]) -> Captured<'_> {
           Captured {
               stdout: b"",
               stderr: bytes,
           }
       }

       const fn counts(failed: usize, skipped: usize, built: usize, cached: usize) -> Summary {
           Summary {
               failed,
               skipped,
               built,
               cached,
           }
       }

       fn reporter(mode: Mode, paint: Paint) -> TerminalReporter<Vec<u8>> {
           TerminalReporter::new(Vec::new(), mode, paint)
       }

       /// What `reporter` wrote since this was last asked.
       fn written(reporter: &mut TerminalReporter<Vec<u8>>) -> Vec<u8> {
           std::mem::take(&mut reporter.out)
       }

       /// A stream that keeps each write apart and counts its flushes.
       #[derive(Debug, Default)]
       struct Writes {
           writes: Vec<Vec<u8>>,
           flushes: usize,
       }

       impl Write for Writes {
           fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
               self.writes.push(bytes.to_vec());
               Ok(bytes.len())
           }

           fn flush(&mut self) -> io::Result<()> {
               self.flushes += 1;
               Ok(())
           }
       }

       /// Makes the calls of a build in which `//:main.o` is found cached with a warning,
       /// `//:docs` cannot be started, `//lib:text` exits 1 and `//:app` is skipped.
       fn report_a_build_with_two_failures<W: Write>(reporter: &mut TerminalReporter<W>) {
           reporter.begun(4);
           reporter.finished(
               &event("//:main.o", OutcomeClass::Cached, &["cc", "-c", "main.c"]),
               on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
           );
           reporter.started(&action("//:docs", None));
           reporter.finished(
               &with_diagnostic(
                   event(
                       "//:docs",
                       OutcomeClass::InfraFailure {
                           kind: InfraKind::Run,
                       },
                       &["cc", "--doc", "README", "a b"],
                   ),
                   "exec strategy: //:docs: the command cannot be started",
               ),
               nothing(),
           );
           reporter.started(&action("//lib:text", Some("compile lib/text.c")));
           reporter.finished(
               &event(
                   "//lib:text",
                   OutcomeClass::ActionFailure { status: 1 },
                   &["cc", "-c", "lib/text.c", "-o", "out/lib/text"],
               ),
               on_stderr(b"lib/text.c:3:1: error: expected ';'\n"),
           );
           reporter.finished(
               &event(
                   "//:app",
                   OutcomeClass::Skipped {
                       because: label("//:docs"),
                   },
                   &["cc", "-o", "out/app"],
               ),
               nothing(),
           );
           reporter.summary(counts(2, 1, 0, 1));
       }

       #[test]
       fn a_build_with_failures_is_blocks_then_the_summary() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           report_a_build_with_two_failures(&mut reporter);
           assert_eq!(
               String::from_utf8(written(&mut reporter)).unwrap(),
               concat!(
                   "--- //:main.o (cached)\n",
                   "main.c:4:9: warning: unused variable 'n'\n",
                   "FAILED //:docs: infrastructure failure (the command could not be started)\n",
                   "    exec strategy: //:docs: the command cannot be started\n",
                   "    declared in build.lua\n",
                   "    command: cc --doc README \"a b\"\n",
                   "--- //lib:text\n",
                   "lib/text.c:3:1: error: expected ';'\n",
                   "FAILED //lib:text: action failure (exit status 1)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc -c lib/text.c -o out/lib/text\n",
                   "2 failed, 1 skipped, 1 cached\n",
               )
           );
       }

       #[test]
       fn plain_never_writes_an_escape() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           report_a_build_with_two_failures(&mut reporter);
           let bytes = written(&mut reporter);
           assert!(bytes.ends_with(b"2 failed, 1 skipped, 1 cached\n"));
           assert!(!bytes.contains(&0x1b));
           assert!(!bytes.contains(&b'\r'));
       }

       #[test]
       fn a_plain_stream_asked_for_colour_gets_colour_and_no_status_line() {
           let mut reporter = reporter(Mode::Plain, Paint::Ansi);
           reporter.begun(1);
           reporter.started(&action("//lib:text", None));
           reporter.finished(
               &event(
                   "//lib:text",
                   OutcomeClass::ActionFailure { status: 1 },
                   &["cc"],
               ),
               on_stderr(b"lib/text.c:3:1: error: expected ';'\n"),
           );
           reporter.summary(counts(1, 0, 0, 0));
           assert_eq!(
               written(&mut reporter),
               concat!(
                   "\x1b[1m--- //lib:text\x1b[0m\n",
                   "lib/text.c:3:1: error: expected ';'\n",
                   "\x1b[1;31mFAILED\x1b[0m //lib:text: action failure (exit status 1)\n",
                   "    declared in lib/build.lua\n",
                   "    command: cc\n",
                   "1 failed\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn a_cached_build_replays_what_was_printed_byte_for_byte() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           reporter.begun(4);
           reporter.finished(&event("//:app", OutcomeClass::Cached, &["cc"]), nothing());
           reporter.finished(
               &event("//:docs", OutcomeClass::Cached, &["cc"]),
               on_stdout(b"wrote 3 pages\ncaf\xe9"),
           );
           reporter.finished(
               &event("//:main.o", OutcomeClass::Cached, &["cc"]),
               on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
           );
           reporter.finished(
               &event("//lib:text", OutcomeClass::Cached, &["cc"]),
               nothing(),
           );
           reporter.summary(counts(0, 0, 0, 4));
           assert_eq!(
               written(&mut reporter),
               b"--- //:docs (cached)\n\
                 wrote 3 pages\n\
                 caf\xe9\n\
                 --- //:main.o (cached)\n\
                 main.c:4:9: warning: unused variable 'n'\n\
                 4 cached\n"
           );
       }

       #[test]
       fn a_build_of_nothing_writes_only_its_summary() {
           let mut reporter = reporter(Mode::Live, Paint::Ansi);
           reporter.begun(0);
           assert_eq!(written(&mut reporter), b"");
           reporter.summary(counts(0, 0, 0, 0));
           assert_eq!(written(&mut reporter), b"nothing to build\n");
       }

       #[test]
       fn the_status_line_counts_what_is_done_and_names_the_action_begun_first() {
           let mut reporter = reporter(Mode::Live, Paint::Plain);
           reporter.begun(4);
           assert_eq!(written(&mut reporter), b"");
           reporter.started(&action("//:main.o", Some("compile main.c")));
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/4] 1 running: compile main.c\x1b[?7h"
           );
           reporter.started(&action("//:docs", None));
           assert_eq!(
               written(&mut reporter),
               b"\r\x1b[2K\x1b[?7l[0/4] 2 running: compile main.c\x1b[?7h"
           );
           reporter.finished(&event("//:main.o", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(
               written(&mut reporter),
               b"\r\x1b[2K\x1b[?7l[1/4] 1 running: //:docs\x1b[?7h"
           );
           reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(written(&mut reporter), b"\r\x1b[2K\x1b[?7l[2/4]\x1b[?7h");
       }

       #[test]
       fn live_erases_before_a_block_and_redraws_after() {
           let mut reporter = reporter(Mode::Live, Paint::Ansi);
           reporter.begun(4);
           reporter.finished(&event("//:docs", OutcomeClass::Cached, &["cc"]), nothing());
           reporter.finished(
               &event("//lib:text", OutcomeClass::Cached, &["cc"]),
               nothing(),
           );
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[1/4]\x1b[?7h\r\x1b[2K\x1b[?7l[2/4]\x1b[?7h"
           );
           reporter.started(&action("//:main.o", Some("compile main.c")));
           reporter.finished(
               &event("//:main.o", OutcomeClass::Built, &["cc"]),
               on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
           );
           assert_eq!(
               written(&mut reporter),
               concat!(
                   "\r\x1b[2K\x1b[?7l[2/4] 1 running: compile main.c\x1b[?7h",
                   "\r\x1b[2K\x1b[1m--- //:main.o\x1b[0m\n",
                   "main.c:4:9: warning: unused variable 'n'\n",
                   "\x1b[?7l[3/4]\x1b[?7h",
               )
               .as_bytes()
           );
       }

       #[test]
       fn nothing_is_drawn_after_the_summary() {
           let mut reporter = reporter(Mode::Live, Paint::Plain);
           reporter.begun(1);
           reporter.started(&action("//:app", None));
           reporter.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/1] 1 running: //:app\x1b[?7h\r\x1b[2K\x1b[?7l[1/1]\x1b[?7h"
           );
           reporter.summary(counts(0, 0, 1, 0));
           assert_eq!(written(&mut reporter), b"\r\x1b[2K1 built\n");
           assert!(!reporter.drawn);
       }

       #[test]
       fn each_call_is_one_write_and_one_flush() {
           let mut reporter = TerminalReporter::new(Writes::default(), Mode::Live, Paint::Ansi);
           report_a_build_with_two_failures(&mut reporter);
           // One per call that had something to say: four `finished`, two `started`, the summary.
           // `begun` says nothing.
           assert_eq!(reporter.out.writes.len(), 7);
           assert_eq!(reporter.out.flushes, 7);
           assert_eq!(
               reporter.out.writes.first().unwrap(),
               concat!(
                   "\x1b[1m--- //:main.o\x1b[0m (cached)\n",
                   "main.c:4:9: warning: unused variable 'n'\n",
                   "\x1b[?7l[1/4]\x1b[?7h",
               )
               .as_bytes()
           );
           assert_eq!(
               reporter.out.writes.last().unwrap(),
               b"\r\x1b[2K2 failed, 1 skipped, 1 cached\n"
           );
       }

       #[test]
       fn a_call_with_nothing_to_say_writes_nothing() {
           let mut reporter = TerminalReporter::new(Writes::default(), Mode::Plain, Paint::Plain);
           reporter.begun(2);
           reporter.started(&action("//:app", None));
           reporter.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
           reporter.finished(
               &event(
                   "//:docs",
                   OutcomeClass::Skipped {
                       because: label("//:app"),
                   },
                   &["cc"],
               ),
               nothing(),
           );
           assert_eq!(reporter.out.writes.len(), 0);
           assert_eq!(reporter.out.flushes, 0);
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::terminal
   error[E0432]: unresolved imports `super::Mode`, `super::TerminalReporter`
   error[E0432]: unresolved imports `terminal::Mode`, `terminal::TerminalReporter`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/terminal.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The reporter that shows a build on a terminal, or in a stream nobody watches.
   //!
   //! Its own file because it is the one place of this crate that decides when something is
   //! written: a block when a target ends, a status line between blocks, the summary last. It is
   //! given the stream it writes to as a value, so what it writes can be captured in memory.
   //!
   //! Responsibilities: [`TerminalReporter`] and its [`Reporter`] implementation; and [`Mode`],
   //! whether a status line is kept.
   //!
   //! Non-responsibilities: what a block or the summary holds, which the block renderings decide;
   //! whether to colour and whether the stream is a terminal, which the caller decides; and the
   //! order of the calls, which the build that makes them guarantees.

   use std::io::Write;

   use buildl_core::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Reporter, Summary};

   use crate::report::block::{block, summary_line};
   use crate::report::paint::Paint;

   /// Moves to the start of the row and clears it.
   const ERASE: &[u8] = b"\r\x1b[2K";
   /// Stops the terminal from wrapping a long row onto the next one.
   const WRAP_OFF: &[u8] = b"\x1b[?7l";
   /// Lets the terminal wrap again.
   const WRAP_ON: &[u8] = b"\x1b[?7h";

   /// Whether a status line is kept below what a build prints.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Mode {
       /// Blocks and the summary only: for a pipe, a file, a log.
       Plain,
       /// A status line on the last row, erased before anything is printed and drawn again after:
       /// for a terminal.
       Live,
   }

   /// Writes a build's blocks, status line and summary to `out`.
   ///
   /// Each call writes at most once: everything it has to say is gathered and handed to `out` in
   /// one write, then `out` is flushed.
   ///
   /// The status line reads `[2/4] 1 running: compile main.c`: the targets that have their
   /// outcome of all the targets, how many actions run, and the one that has run longest, by its
   /// description or else its label. It is drawn again only when a call arrives, with the
   /// terminal's wrapping switched off for the length of that one write, so a long line is cut at
   /// the edge instead of taking a second row.
   #[derive(Debug)]
   pub struct TerminalReporter<W: Write> {
       /// Where everything is written.
       out: W,
       /// Whether a status line is kept.
       mode: Mode,
       /// Whether what is written is coloured.
       paint: Paint,
       /// How many targets the build has.
       total: usize,
       /// How many of them have their outcome.
       done: usize,
       /// The actions that run, the one begun first at the front, each with its status text.
       running: Vec<(Label, String)>,
       /// Whether a status line stands on the last row.
       drawn: bool,
   }

   impl<W: Write> TerminalReporter<W> {
       /// A reporter that writes to `out`, with or without a status line, coloured or not.
       pub const fn new(out: W, mode: Mode, paint: Paint) -> Self {
           Self {
               out,
               mode,
               paint,
               total: 0,
               done: 0,
               running: Vec::new(),
               drawn: false,
           }
       }

       /// Adds to `frame` what erases the status line, when one stands.
       fn erase(&mut self, frame: &mut Vec<u8>) {
           if self.drawn {
               frame.extend_from_slice(ERASE);
               self.drawn = false;
           }
       }

       /// Adds the status line to `frame`, when one is kept.
       fn draw(&mut self, frame: &mut Vec<u8>) {
           if self.mode == Mode::Live {
               frame.extend_from_slice(WRAP_OFF);
               frame.extend_from_slice(self.status().as_bytes());
               frame.extend_from_slice(WRAP_ON);
               self.drawn = true;
           }
       }

       /// The text of the status line.
       fn status(&self) -> String {
           let progress = format!("[{}/{}]", self.done, self.total);
           match self.running.first() {
               Some((_, text)) => format!("{progress} {} running: {text}", self.running.len()),
               None => progress,
           }
       }

       /// Hands `frame` to the stream in one write and flushes it. Nothing is written for an empty
       /// frame.
       fn emit(&mut self, frame: &[u8]) {
           if frame.is_empty() {
               return;
           }
           let _ = self.out.write_all(frame).and_then(|()| self.out.flush());
       }
   }

   impl<W: Write> Reporter for TerminalReporter<W> {
       /// Writes nothing: keeps the number of targets, and forgets what an earlier build left.
       fn begun(&mut self, targets: usize) {
           self.total = targets;
           self.done = 0;
           self.running.clear();
       }

       /// Counts the action as running, and draws the status line again.
       fn started(&mut self, action: &ReadyAction) {
           let text = action
               .description
               .as_ref()
               .map_or_else(|| action.label.to_string(), ToString::to_string);
           self.running.push((action.label.clone(), text));
           let mut frame = Vec::new();
           self.erase(&mut frame);
           self.draw(&mut frame);
           self.emit(&frame);
       }

       /// Writes nothing.
       fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

       /// Writes the target's block. The status line is erased before and drawn again after.
       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
           let label = &event.outcome.label;
           self.done += 1;
           self.running.retain(|(running, _)| running != label);
           let mut frame = Vec::new();
           self.erase(&mut frame);
           frame.extend_from_slice(&block(event, printed, self.paint));
           self.draw(&mut frame);
           self.emit(&frame);
       }

       /// Erases the status line and writes the summary line.
       fn summary(&mut self, summary: Summary) {
           let mut frame = Vec::new();
           self.erase(&mut frame);
           frame.extend_from_slice(summary_line(summary).as_bytes());
           self.emit(&frame);
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::terminal
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/terminal.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): report a build's blocks, status line and summary"
   ```

### Task 7 — Stop writing once a write fails

A reporter never fails a build. When the stream refuses a write or a flush, the reporter remembers it and writes nothing more; every later call returns as before.

**Files:**
- Modify `crates/buildl/src/report/terminal.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/report/terminal.rs`, add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       /// A stream that takes `good` writes, then refuses every one after, counting what it was
       /// asked.
       #[derive(Debug, Default)]
       struct Failing {
           good: usize,
           writes: usize,
           flushes: usize,
           fail_flush: bool,
       }

       impl Write for Failing {
           fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
               self.writes += 1;
               if self.writes > self.good {
                   return Err(io::Error::other("the stream is closed"));
               }
               Ok(bytes.len())
           }

           fn flush(&mut self) -> io::Result<()> {
               self.flushes += 1;
               if self.fail_flush {
                   return Err(io::Error::other("the stream cannot be flushed"));
               }
               Ok(())
           }
       }

       #[test]
       fn a_failed_write_stops_writing_and_does_not_panic() {
           let failing = Failing {
               good: 1,
               ..Failing::default()
           };
           let mut reporter = TerminalReporter::new(failing, Mode::Live, Paint::Ansi);
           report_a_build_with_two_failures(&mut reporter);
           // The first write is taken, the second is refused, and no third is tried.
           assert_eq!(reporter.out.writes, 2);
           assert_eq!(reporter.out.flushes, 1);
       }

       #[test]
       fn a_failed_flush_stops_writing_too() {
           let failing = Failing {
               good: usize::MAX,
               fail_flush: true,
               ..Failing::default()
           };
           let mut reporter = TerminalReporter::new(failing, Mode::Plain, Paint::Plain);
           report_a_build_with_two_failures(&mut reporter);
           assert_eq!(reporter.out.writes, 1);
           assert_eq!(reporter.out.flushes, 1);
       }
   ```

2. Run them and confirm the new ones fail:

   ```
   $ cargo test -p buildl --lib report::terminal
   test report::terminal::tests::a_failed_flush_stops_writing_too ... FAILED
   test report::terminal::tests::a_failed_write_stops_writing_and_does_not_panic ... FAILED
   test result: FAILED. 10 passed; 2 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.00s
   ```

   As task 6 leaves it, the reporter goes on writing: the first test finds 7 writes where it expects 2, the second 4 where it expects 1.

3. Implement it. In `crates/buildl/src/report/terminal.rs`, replace

   ```rust
   /// Each call writes at most once: everything it has to say is gathered and handed to `out` in
   /// one write, then `out` is flushed.
   ```

   with

   ```rust
   /// Each call writes at most once: everything it has to say is gathered and handed to `out` in
   /// one write, then `out` is flushed. A write or a flush that fails ends the writing: nothing
   /// more is written, and nothing else changes, since a build does not depend on being shown.
   ```

   Then replace

   ```rust
       /// Whether a status line stands on the last row.
       drawn: bool,
   }
   ```

   with

   ```rust
       /// Whether a status line stands on the last row.
       drawn: bool,
       /// Whether a write failed.
       broken: bool,
   }
   ```

   Then replace

   ```rust
               drawn: false,
           }
   ```

   with

   ```rust
               drawn: false,
               broken: false,
           }
   ```

   Then replace

   ```rust
       /// Hands `frame` to the stream in one write and flushes it. Nothing is written for an empty
       /// frame.
       fn emit(&mut self, frame: &[u8]) {
           if frame.is_empty() {
               return;
           }
           let _ = self.out.write_all(frame).and_then(|()| self.out.flush());
       }
   ```

   with

   ```rust
       /// Hands `frame` to the stream in one write and flushes it. Nothing is written for an empty
       /// frame, nor once a write has failed.
       fn emit(&mut self, frame: &[u8]) {
           if frame.is_empty() || self.broken {
               return;
           }
           if self
               .out
               .write_all(frame)
               .and_then(|()| self.out.flush())
               .is_err()
           {
               self.broken = true;
           }
       }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::terminal
   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.01s
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl/src/report/terminal.rs
   $ git commit -m "feat(buildl): stop writing a report once a write fails"
   ```

### Task 8 — Show one action's output as it arrives

Under `--stream` the core hands the reporter the bytes of one action as they are printed. The first bytes of a target erase the status line and get the target's header; the bytes follow as they are. When that target ends, a newline closes a line left open and a failure gets its trailer. No status line is drawn in between.

**Files:**
- Modify `crates/buildl/src/report/terminal.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl/src/report/terminal.rs`, replace everything from the line `mod tests {` down to, but not including, the line `fn label(raw: &str) -> Label {` with this, leaving one blank line before that line:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::io::{self, Write};
       use std::path::PathBuf;

       use buildl_core::{
           ActionOutcome, Argument, Captured, Channel, Command, Diagnostic, ExpandedDescription,
           InfraKind, Label, NetworkAccess, OutcomeClass, OutcomeEvent, Provenance, ReadyAction,
           Reporter, Summary, Timestamp,
       };

       use super::{Mode, TerminalReporter};
       use crate::report::paint::Paint;
   ```

   Then add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       #[test]
       fn a_live_action_gets_its_header_once_and_its_bytes_as_they_come() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           reporter.begun(1);
           reporter.started(&action("//:docs", None));
           assert_eq!(written(&mut reporter), b"");
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
           assert_eq!(written(&mut reporter), b"--- //:docs\npage 1\n");
           reporter.printed(&label("//:docs"), Channel::Stderr, b"1 warn");
           assert_eq!(written(&mut reporter), b"1 warn");
           reporter.printed(&label("//:docs"), Channel::Stderr, b"ing\ncaf\xe9\n");
           assert_eq!(written(&mut reporter), b"ing\ncaf\xe9\n");
           reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(written(&mut reporter), b"");
       }

       #[test]
       fn a_live_action_that_ends_mid_line_gets_a_newline_before_its_trailer() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           reporter.begun(1);
           reporter.started(&action("//:docs", None));
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\npage 2");
           assert_eq!(written(&mut reporter), b"--- //:docs\npage 1\npage 2");
           reporter.finished(
               &event(
                   "//:docs",
                   OutcomeClass::ActionFailure { status: 2 },
                   &["cc", "--doc", "README"],
               ),
               nothing(),
           );
           assert_eq!(
               written(&mut reporter),
               concat!(
                   "\n",
                   "FAILED //:docs: action failure (exit status 2)\n",
                   "    declared in build.lua\n",
                   "    command: cc --doc README\n",
               )
               .as_bytes()
           );
       }

       #[test]
       fn a_live_action_that_ends_mid_line_and_succeeds_gets_only_the_newline() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           reporter.begun(1);
           reporter.started(&action("//:docs", None));
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
           reporter.printed(&label("//:docs"), Channel::Stdout, b"");
           assert_eq!(written(&mut reporter), b"--- //:docs\npage 1");
           reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(written(&mut reporter), b"\n");
       }

       #[test]
       fn no_status_line_is_drawn_while_an_action_is_shown_live() {
           let mut reporter = reporter(Mode::Live, Paint::Ansi);
           reporter.begun(3);
           reporter.started(&action("//:docs", Some("write the docs")));
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/3] 1 running: write the docs\x1b[?7h"
           );
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
           assert_eq!(
               written(&mut reporter),
               b"\r\x1b[2K\x1b[1m--- //:docs\x1b[0m\npage 1\n"
           );
           reporter.started(&action("//:main.o", Some("compile main.c")));
           assert_eq!(written(&mut reporter), b"");
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 2\n");
           assert_eq!(written(&mut reporter), b"page 2\n");
           reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[1/3] 1 running: compile main.c\x1b[?7h"
           );
       }

       #[test]
       fn blocks_of_other_actions_follow_the_live_one_and_the_next_is_shown_live() {
           let mut reporter = reporter(Mode::Plain, Paint::Plain);
           reporter.begun(3);
           reporter.started(&action("//:docs", None));
           reporter.started(&action("//:main.o", None));
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 2");
           // `//:main.o` ended meanwhile; the build reports it once the live action has ended.
           reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
           reporter.finished(
               &event("//:main.o", OutcomeClass::Built, &["cc"]),
               on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
           );
           reporter.started(&action("//lib:text", None));
           reporter.printed(&label("//lib:text"), Channel::Stderr, b"text: done\n");
           reporter.finished(
               &event("//lib:text", OutcomeClass::Built, &["cc"]),
               nothing(),
           );
           reporter.summary(counts(0, 0, 3, 0));
           assert_eq!(
               String::from_utf8(written(&mut reporter)).unwrap(),
               concat!(
                   "--- //:docs\n",
                   "page 1\n",
                   "page 2\n",
                   "--- //:main.o\n",
                   "main.c:4:9: warning: unused variable 'n'\n",
                   "--- //lib:text\n",
                   "text: done\n",
                   "3 built\n",
               )
           );
       }
   ```

2. Run them and confirm the new ones fail:

   ```
   $ cargo test -p buildl --lib report::terminal
   test report::terminal::tests::a_live_action_gets_its_header_once_and_its_bytes_as_they_come ... FAILED
   test report::terminal::tests::a_live_action_that_ends_mid_line_and_succeeds_gets_only_the_newline ... FAILED
   test report::terminal::tests::a_live_action_that_ends_mid_line_gets_a_newline_before_its_trailer ... FAILED
   test report::terminal::tests::blocks_of_other_actions_follow_the_live_one_and_the_next_is_shown_live ... FAILED
   test report::terminal::tests::no_status_line_is_drawn_while_an_action_is_shown_live ... FAILED
   test result: FAILED. 12 passed; 5 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.00s
   ```

   As task 7 leaves it, `printed` writes nothing: each test finds no bytes, or a block missing, where it expects the header and the output.

3. Implement it. Replace everything above the line `#[cfg(test)]` of `crates/buildl/src/report/terminal.rs` with this, leaving one blank line before that line:

   ```rust
   //! The reporter that shows a build on a terminal, or in a stream nobody watches.
   //!
   //! Its own file because it is the one place of this crate that decides when something is
   //! written: a block when a target ends, a status line between blocks, one action's output as it
   //! arrives, the summary last. It is given the stream it writes to as a value, so what it writes
   //! can be captured in memory.
   //!
   //! Responsibilities: [`TerminalReporter`] and its [`Reporter`] implementation; and [`Mode`],
   //! whether a status line is kept.
   //!
   //! Non-responsibilities: what a block or the summary holds, which the block renderings decide;
   //! whether to colour and whether the stream is a terminal, which the caller decides; and the
   //! order of the calls, which the build that makes them guarantees. Between an action's first
   //! live bytes and its end the build reports nothing about another target, so nothing is held
   //! back here.

   use std::io::Write;

   use buildl_core::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Reporter, Summary};

   use crate::report::block::{block, header, summary_line, trailer};
   use crate::report::paint::Paint;

   /// Moves to the start of the row and clears it.
   const ERASE: &[u8] = b"\r\x1b[2K";
   /// Stops the terminal from wrapping a long row onto the next one.
   const WRAP_OFF: &[u8] = b"\x1b[?7l";
   /// Lets the terminal wrap again.
   const WRAP_ON: &[u8] = b"\x1b[?7h";

   /// Whether a status line is kept below what a build prints.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Mode {
       /// Blocks and the summary only: for a pipe, a file, a log.
       Plain,
       /// A status line on the last row, erased before anything is printed and drawn again after:
       /// for a terminal.
       Live,
   }

   /// Writes a build's blocks, status line and summary to `out`.
   ///
   /// Each call writes at most once: everything it has to say is gathered and handed to `out` in
   /// one write, then `out` is flushed. A write or a flush that fails ends the writing: nothing
   /// more is written, and nothing else changes, since a build does not depend on being shown.
   ///
   /// The status line reads `[2/4] 1 running: compile main.c`: the targets that have their
   /// outcome of all the targets, how many actions run, and the one that has run longest, by its
   /// description or else its label. It is drawn again only when a call arrives, with the
   /// terminal's wrapping switched off for the length of that one write, so a long line is cut at
   /// the edge instead of taking a second row. It is never drawn while one action's output is
   /// shown as it arrives.
   #[derive(Debug)]
   pub struct TerminalReporter<W: Write> {
       /// Where everything is written.
       out: W,
       /// Whether a status line is kept.
       mode: Mode,
       /// Whether what is written is coloured.
       paint: Paint,
       /// How many targets the build has.
       total: usize,
       /// How many of them have their outcome.
       done: usize,
       /// The actions that run, the one begun first at the front, each with its status text.
       running: Vec<(Label, String)>,
       /// The target whose output is shown as it arrives.
       live: Option<Label>,
       /// Whether the last byte shown of the live target's output was not a newline.
       mid_line: bool,
       /// Whether a status line stands on the last row.
       drawn: bool,
       /// Whether a write failed.
       broken: bool,
   }

   impl<W: Write> TerminalReporter<W> {
       /// A reporter that writes to `out`, with or without a status line, coloured or not.
       pub const fn new(out: W, mode: Mode, paint: Paint) -> Self {
           Self {
               out,
               mode,
               paint,
               total: 0,
               done: 0,
               running: Vec::new(),
               live: None,
               mid_line: false,
               drawn: false,
               broken: false,
           }
       }

       /// Adds to `frame` what erases the status line, when one stands.
       fn erase(&mut self, frame: &mut Vec<u8>) {
           if self.drawn {
               frame.extend_from_slice(ERASE);
               self.drawn = false;
           }
       }

       /// Adds the status line to `frame`, when one is kept and no action is shown live.
       fn draw(&mut self, frame: &mut Vec<u8>) {
           if self.mode == Mode::Live && self.live.is_none() {
               frame.extend_from_slice(WRAP_OFF);
               frame.extend_from_slice(self.status().as_bytes());
               frame.extend_from_slice(WRAP_ON);
               self.drawn = true;
           }
       }

       /// The text of the status line.
       fn status(&self) -> String {
           let progress = format!("[{}/{}]", self.done, self.total);
           match self.running.first() {
               Some((_, text)) => format!("{progress} {} running: {text}", self.running.len()),
               None => progress,
           }
       }

       /// Hands `frame` to the stream in one write and flushes it. Nothing is written for an empty
       /// frame, nor once a write has failed.
       fn emit(&mut self, frame: &[u8]) {
           if frame.is_empty() || self.broken {
               return;
           }
           if self
               .out
               .write_all(frame)
               .and_then(|()| self.out.flush())
               .is_err()
           {
               self.broken = true;
           }
       }
   }

   impl<W: Write> Reporter for TerminalReporter<W> {
       /// Writes nothing: keeps the number of targets, and forgets what an earlier build left.
       fn begun(&mut self, targets: usize) {
           self.total = targets;
           self.done = 0;
           self.running.clear();
           self.live = None;
           self.mid_line = false;
       }

       /// Counts the action as running, and draws the status line again.
       fn started(&mut self, action: &ReadyAction) {
           let text = action
               .description
               .as_ref()
               .map_or_else(|| action.label.to_string(), ToString::to_string);
           self.running.push((action.label.clone(), text));
           let mut frame = Vec::new();
           self.erase(&mut frame);
           self.draw(&mut frame);
           self.emit(&frame);
       }

       /// Writes the bytes as they are, under the target's header the first time.
       fn printed(&mut self, label: &Label, _channel: Channel, bytes: &[u8]) {
           let mut frame = Vec::new();
           if self.live.as_ref() != Some(label) {
               self.erase(&mut frame);
               frame.extend_from_slice(header(label, self.paint).as_bytes());
               frame.push(b'\n');
               self.live = Some(label.clone());
               self.mid_line = false;
           }
           frame.extend_from_slice(bytes);
           if let Some(last) = bytes.last() {
               self.mid_line = *last != b'\n';
           }
           self.emit(&frame);
       }

       /// Writes the target's block; or, for the target shown live, ends its last line and writes
       /// its trailer. The status line is erased before and drawn again after.
       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
           let label = &event.outcome.label;
           self.done += 1;
           self.running.retain(|(running, _)| running != label);
           let mut frame = Vec::new();
           if self.live.as_ref() == Some(label) {
               if self.mid_line {
                   frame.push(b'\n');
               }
               frame.extend_from_slice(trailer(event, self.paint).as_bytes());
               self.live = None;
               self.mid_line = false;
           } else {
               self.erase(&mut frame);
               frame.extend_from_slice(&block(event, printed, self.paint));
           }
           self.draw(&mut frame);
           self.emit(&frame);
       }

       /// Erases the status line and writes the summary line.
       fn summary(&mut self, summary: Summary) {
           let mut frame = Vec::new();
           self.erase(&mut frame);
           frame.extend_from_slice(summary_line(summary).as_bytes());
           self.emit(&frame);
       }
   }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::terminal
   test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.00s
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl/src/report/terminal.rs
   $ git commit -m "feat(buildl): show one action's output as it arrives"
   ```

### Task 9 — Close a report a build left open

A build that ends in an error makes no `summary` call, so a status line or a half-written line may still stand. `close` erases the one and ends the other, so that the error is printed on an empty row.

**Files:**
- Modify `crates/buildl/src/report/terminal.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/report/terminal.rs`, add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       #[test]
       fn close_erases_the_status_line() {
           let mut reporter = reporter(Mode::Live, Paint::Plain);
           reporter.begun(2);
           reporter.started(&action("//:app", None));
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/2] 1 running: //:app\x1b[?7h"
           );
           reporter.close();
           assert_eq!(written(&mut reporter), b"\r\x1b[2K");
           reporter.close();
           assert_eq!(written(&mut reporter), b"");
       }

       #[test]
       fn close_ends_a_line_a_live_action_left_open() {
           let mut reporter = reporter(Mode::Live, Paint::Plain);
           reporter.begun(1);
           reporter.started(&action("//:docs", None));
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/1] 1 running: //:docs\x1b[?7h\r\x1b[2K--- //:docs\npage 1"
           );
           reporter.close();
           assert_eq!(written(&mut reporter), b"\n");
           reporter.close();
           assert_eq!(written(&mut reporter), b"");
       }

       #[test]
       fn a_second_build_starts_from_nothing() {
           let mut reporter = reporter(Mode::Live, Paint::Plain);
           reporter.begun(5);
           reporter.started(&action("//:docs", None));
           reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
           reporter.close();
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/5] 1 running: //:docs\x1b[?7h\r\x1b[2K--- //:docs\npage 1\n"
           );
           reporter.begun(2);
           reporter.started(&action("//:app", None));
           assert_eq!(
               written(&mut reporter),
               b"\x1b[?7l[0/2] 1 running: //:app\x1b[?7h"
           );
       }

       #[test]
       fn close_writes_nothing_when_nothing_is_left_open() {
           let mut plain = reporter(Mode::Plain, Paint::Plain);
           plain.begun(1);
           plain.started(&action("//:app", None));
           plain.close();
           assert_eq!(written(&mut plain), b"");

           let mut live = reporter(Mode::Live, Paint::Plain);
           live.begun(1);
           live.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
           live.summary(counts(0, 0, 1, 0));
           assert_eq!(
               written(&mut live),
               b"\x1b[?7l[1/1]\x1b[?7h\r\x1b[2K1 built\n"
           );
           live.close();
           assert_eq!(written(&mut live), b"");
       }
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::terminal
   error[E0599]: no method named `close` found for struct `TerminalReporter<W>` in the current scope
   ```

3. Implement it. Replace the top of `crates/buildl/src/report/terminal.rs`, everything above the line `/// Moves to the start of the row and clears it.`, with this, leaving one blank line before that line:

   ```rust
   //! The reporter that shows a build on a terminal, or in a stream nobody watches.
   //!
   //! Its own file because it is the one place of this crate that decides when something is
   //! written: a block when a target ends, a status line between blocks, one action's output as it
   //! arrives, the summary last. It is given the stream it writes to as a value, so what it writes
   //! can be captured in memory.
   //!
   //! Responsibilities: [`TerminalReporter`], its [`Reporter`] implementation and
   //! [`TerminalReporter::close`]; and [`Mode`], whether a status line is kept.
   //!
   //! Non-responsibilities: what a block or the summary holds, which the block renderings decide;
   //! whether to colour and whether the stream is a terminal, which the caller decides; and the
   //! order of the calls, which the build that makes them guarantees. Between an action's first
   //! live bytes and its end the build reports nothing about another target, so nothing is held
   //! back here.

   use std::io::Write;

   use buildl_core::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Reporter, Summary};

   use crate::report::block::{block, header, summary_line, trailer};
   use crate::report::paint::Paint;
   ```

   Then add this method to `impl<W: Write> TerminalReporter<W>`, directly above the line `` /// Adds to `frame` what erases the status line, when one stands. ``, followed by one blank line:

   ```rust
       /// Erases the status line and ends a line left open.
       ///
       /// For a build that ended in an error before its summary: whatever is written next then
       /// starts on an empty row.
       pub fn close(&mut self) {
           let mut frame = Vec::new();
           self.erase(&mut frame);
           if self.mid_line {
               frame.push(b'\n');
           }
           self.live = None;
           self.mid_line = false;
           self.emit(&frame);
       }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::terminal
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.00s
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl/src/report/terminal.rs
   $ git commit -m "feat(buildl): close a report a build left open"
   ```

### Task 10 — Add `Silent`

A build always needs a reporter. `Silent` is the one for a caller with nobody to show anything to: every method is empty and the type holds nothing.

**Files:**
- Create `crates/buildl/src/report/silent.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the type. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //! - [`Silent`] — the reporter that shows nothing.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;
   mod silent;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use paint::Paint;
   pub use silent::Silent;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. Create `crates/buildl/src/report/silent.rs` with the test module only:

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

       use buildl_core::{
           ActionOutcome, Argument, Captured, Channel, Command, Directory, Label, NetworkAccess,
           OutcomeClass, OutcomeEvent, Provenance, ReadyAction, Reporter, Summary, Timestamp,
       };

       use super::Silent;

       #[test]
       fn every_call_is_taken_and_nothing_happens() {
           let label = Label::parse("//:app").unwrap();
           let command = Command::new(vec![Argument::parse("cc").unwrap()]).unwrap();
           let mut silent = Silent;
           silent.begun(1);
           silent.started(&ReadyAction {
               label: label.clone(),
               command: command.clone(),
               description: None,
               env: BTreeMap::new(),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: Vec::new(),
               network: NetworkAccess::Sealed,
           });
           silent.printed(&label, Channel::Stdout, b"hi");
           silent.finished(
               &OutcomeEvent {
                   at: Timestamp::UNIX_EPOCH,
                   provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                   command,
                   key: None,
                   started: None,
                   finished: None,
                   outcome: ActionOutcome {
                       label,
                       class: OutcomeClass::Built,
                       diagnostic: None,
                       outputs: Vec::new(),
                       stdout: None,
                       stderr: None,
                   },
               },
               Captured {
                   stdout: b"hi",
                   stderr: b"",
               },
           );
           silent.summary(Summary {
               failed: 0,
               skipped: 0,
               built: 1,
               cached: 0,
           });
           // It has nowhere to keep anything it was told.
           assert_eq!(size_of_val(&silent), 0);
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::silent
   error[E0432]: unresolved import `super::Silent`
   error[E0432]: unresolved import `silent::Silent`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/silent.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The reporter that shows nothing.
   //!
   //! Its own file because a build always needs a reporter, and some builds have nobody to show
   //! anything to: a caller that only wants the outcomes, a test, a dry run. A type with no state
   //! says that at the call site, where a writer that discards everything would not.
   //!
   //! Responsibilities: [`Silent`] and its [`Reporter`] implementation.
   //!
   //! Non-responsibilities: recording what a build did. The outcomes a build returns and the log
   //! it writes hold that; this type keeps nothing.

   use buildl_core::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Reporter, Summary};

   /// A reporter that shows nothing.
   #[derive(Debug, Clone, Copy, Default)]
   pub struct Silent;

   impl Reporter for Silent {
       fn begun(&mut self, _targets: usize) {}

       fn started(&mut self, _action: &ReadyAction) {}

       fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

       fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}

       fn summary(&mut self, _summary: Summary) {}
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::silent
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 46 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/silent.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): add Silent, the reporter that shows nothing"
   ```

### Task 11 — Render a plan as text

`plan_text` gives one line per target in label order, `clean`, `run` with the reason or `maybe` with the dependencies waited on, then one line of counts. The test builds one real plan that holds every verdict and every difference a key can show, through `buildl_core::plan` over stand-in ports.

**Files:**
- Create `crates/buildl/src/report/plan_text.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //! - [`Silent`] — the reporter that shows nothing.
   //! - [`plan_text`] — a plan as lines for a person.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod paint;
   mod plan_text;
   mod silent;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use paint::Paint;
   pub use plan_text::plan_text;
   pub use silent::Silent;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. Create `crates/buildl/src/report/plan_text.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::{BTreeMap, BTreeSet};
       use std::path::PathBuf;

       use buildl_core::{
           Action, ActionCache, ActionKey, Argument, BuildFile, CacheRow, Ceiling, Clock, Command,
           ContentStore, Declaration, DeclarationSource, Declared, DepProducts, Diagnostic, Digest,
           Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated, EventLog,
           ExecStrategy, Execution, FileMode, FileStat, Found, Freshness, GraphRecord, Host,
           InputDigest, InputStat, KeyComponents, Label, LiveSink, Manifest, NetworkAccess,
           OutcomeEvent, OutputName, OutputPath, OwnComponents, Plan, PlannedKey, Platform, PortName,
           PortSet, Ports, Program, Provenance, RawOutcome, ReadyAction, Result, Setting, SettingName,
           SettingValue, SourcePath, StatCache, StatEntry, Target, TargetGraph, TargetRole, Timestamp,
           ToolIdentity, ToolResolver, Turn, Verdict, Workspace, plan, resolve,
       };

       use super::plan_text;
       use crate::report::silent::Silent;

       /// The machine a plan asks about: every file exists and holds its own path, every program
       /// is found, and every environment variable has the value `-Wall`.
       #[derive(Debug)]
       struct Machine;

       impl Manifest for Machine {
           fn workspace(&self) -> Result<Workspace> {
               Ok(Workspace {
                   out: Directory::parse("out")?,
                   state: Directory::parse(".buildl")?,
                   ceiling: Ceiling {
                       run: BTreeSet::from([Program::parse("cc")?]),
                       env: BTreeSet::from([EnvName::parse("CC_FLAGS")?]),
                       read: BTreeSet::from([Directory::root()]),
                       write: BTreeSet::from([Directory::parse("out")?, Directory::parse(".buildl")?]),
                   },
                   execution: Execution::default(),
               })
           }
       }

       impl Host for Machine {
           fn platform(&self) -> Platform {
               Platform::parse("linux/x86_64").unwrap()
           }

           fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
               Ok(Some(EnvValue::parse("-Wall")?))
           }
       }

       impl Digester for Machine {
           fn stat(&self, _path: &SourcePath) -> Result<InputStat> {
               Ok(InputStat::File(FileStat {
                   modified: Timestamp::UNIX_EPOCH,
                   size: 1,
               }))
           }

           fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
               Ok(paths
                   .iter()
                   .map(|path| Digest::of(path.as_str().as_bytes()))
                   .collect())
           }
       }

       impl StatCache for Machine {
           fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
               None
           }

           fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}

           fn commit(&mut self) -> Result<()> {
               Ok(())
           }
       }

       impl ToolResolver for Machine {
           fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
               Ok(Some(Digest::of(program.as_str().as_bytes())))
           }
       }

       /// What earlier builds recorded.
       #[derive(Debug, Default)]
       struct Rows(BTreeMap<Label, CacheRow>);

       impl ActionCache for Rows {
           fn row(&self, label: &Label) -> Option<&CacheRow> {
               self.0.get(label)
           }

           fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
               self.0.insert(label, row);
               Ok(())
           }
       }

       /// Stands in for every port a plan never asks.
       #[derive(Debug)]
       struct Unused;

       /// The failure of a port a plan was not expected to ask.
       fn unused(port: PortName) -> Error {
           Error::Adapter {
               port,
               diagnostic: Diagnostic::new("a plan does not ask this port"),
           }
       }

       impl DeclarationSource for Unused {
           fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
               Ok(Evaluated::Absent)
           }
       }

       impl ContentStore for Unused {
           fn put(&self, _bytes: &[u8]) -> Result<Digest> {
               Err(unused(PortName::ContentStore))
           }

           fn contains(&self, _digest: &Digest) -> bool {
               false
           }

           fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
               Err(unused(PortName::ContentStore))
           }

           fn clear(&self, _at: &OutputPath) -> Result<()> {
               Err(unused(PortName::ContentStore))
           }

           fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
               Err(unused(PortName::ContentStore))
           }
       }

       impl EventLog for Unused {
           fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
               Err(unused(PortName::EventLog))
           }
       }

       impl ExecStrategy for Unused {
           type Dir = ();

           fn prepare(&self, _action: &ReadyAction) -> Result<()> {
               Err(unused(PortName::ExecStrategy))
           }

           fn run(
               &self,
               _dir: &(),
               _action: &ReadyAction,
               _live: Option<LiveSink<'_>>,
           ) -> Result<RawOutcome> {
               Err(unused(PortName::ExecStrategy))
           }

           fn collect<C: ContentStore>(
               &self,
               _dir: &(),
               _action: &ReadyAction,
               _cas: &C,
           ) -> Result<Vec<Found>> {
               Err(unused(PortName::ExecStrategy))
           }
       }

       impl Dispatcher for Unused {
           fn dispatch<T, M, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
           where
               T: Send,
               M: Send,
               R: Send,
               W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
               S: FnMut(Turn<M, R>) -> Vec<T>,
           {
               Err(unused(PortName::Dispatcher))
           }
       }

       impl Clock for Unused {
           fn now(&self) -> Timestamp {
               Timestamp::UNIX_EPOCH
           }
       }

       impl GraphRecord for Unused {
           fn record(&mut self, _graph: &TargetGraph) -> Result<()> {
               Err(unused(PortName::GraphRecord))
           }
       }

       /// The bundle a plan is made over here.
       struct Planning;

       impl Ports for Planning {
           type Source = Unused;
           type Manifest = Machine;
           type Host = Machine;
           type Digester = Machine;
           type StatCache = Machine;
           type Tools = Machine;
           type Cache = Rows;
           type Cas = Unused;
           type Log = Unused;
           type Strategy = Unused;
           type Dispatcher = Unused;
           type Clock = Unused;
           type Graph = Unused;
           type Reporter = Silent;
       }

       /// The plan of `graph` on the machine, against what `rows` recorded.
       fn planned(graph: &TargetGraph, rows: Rows) -> Plan {
           let mut ports = PortSet::<Planning> {
               source: Unused,
               manifest: Machine,
               host: Machine,
               digester: Machine,
               stat_cache: Machine,
               tools: Machine,
               cache: rows,
               cas: Unused,
               log: Unused,
               strategy: Unused,
               dispatcher: Unused,
               clock: Unused,
               graph: Unused,
               reporter: Silent,
           };
           plan(graph, &BTreeMap::new(), &mut ports).unwrap()
       }

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       fn command(arguments: &[&str]) -> Command {
           Command::new(
               arguments
                   .iter()
                   .map(|argument| Argument::parse(*argument).unwrap())
                   .collect(),
           )
           .unwrap()
       }

       fn root_file() -> Provenance {
           Provenance::new(PathBuf::from("build.lua"), Directory::root())
       }

       /// A target of the root build file running `run`, with nothing else declared.
       fn target(raw: &str, run: &[&str]) -> Target {
           Target {
               label: label(raw),
               role: TargetRole::Build,
               action: Action::Run(command(run)),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: Vec::new(),
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           }
       }

       fn declared(target: Target) -> Declaration {
           Declaration::new(root_file(), Declared::Target(target))
       }

       fn paths(raw: &[&str]) -> Vec<SourcePath> {
           raw.iter()
               .map(|path| SourcePath::parse(*path).unwrap())
               .collect()
       }

       fn deps(raw: &[&str]) -> Vec<Label> {
           raw.iter().map(|dep| label(dep)).collect()
       }

       /// A graph with a target for every verdict a plan can reach.
       fn graph() -> TargetGraph {
           resolve(vec![
               Declaration::new(
                   root_file(),
                   Declared::Setting(Setting {
                       name: SettingName::parse("mode").unwrap(),
                       default: SettingValue::parse("debug").unwrap(),
                   }),
               ),
               declared(Target {
                   freshness: Freshness::Always,
                   ..target("//:always", &["cc", "--version"])
               }),
               declared(Target {
                   deps: deps(&["//:main.o"]),
                   ..target("//:app", &["cc", "$deps", "-o", "$out"])
               }),
               declared(Target {
                   inputs: paths(&["changed.c"]),
                   deps: deps(&["//:docs"]),
                   outputs: vec![OutputName::parse("changed").unwrap()],
                   env: vec![EnvName::parse("CC_FLAGS").unwrap()],
                   ..target("//:changed", &["cc", "-O$opt:mode", "$in", "-o", "$out"])
               }),
               declared(target("//:docs", &["cc", "--doc"])),
               declared(target("//:leaf", &["cc", "--leaf"])),
               declared(Target {
                   inputs: paths(&["main.c"]),
                   ..target("//:main.o", &["cc", "-c", "$in"])
               }),
               declared(target("//:new", &["cc", "--new"])),
               declared(Target {
                   inputs: paths(&["b.txt", "a.txt"]),
                   deps: deps(&["//:leaf", "//:docs"]),
                   ..target("//:reordered", &["cc", "$in"])
               }),
               declared(Target {
                   deps: deps(&["//:leaf"]),
                   ..target("//:script", &["$deps", "--run"])
               }),
           ])
           .unwrap()
       }

       /// What a plan made with nothing recorded knows of the key of `raw`, without its
       /// dependencies.
       fn own(first: &Plan, graph: &TargetGraph, raw: &str) -> OwnComponents {
           let id = graph.id_of(&label(raw)).unwrap();
           match first.verdict(id).unwrap() {
               Verdict::Dirty {
                   key: PlannedKey::Finished(key),
                   ..
               } => key.components().own.clone(),
               Verdict::Dirty {
                   key: PlannedKey::Unfinished(unfinished),
                   ..
               } => unfinished.own.clone(),
               other => unreachable!("with nothing recorded every target is dirty, not {other:?}"),
           }
       }

       /// The row of a build under the key made of `own` and `deps`, which printed nothing.
       fn row(own: OwnComponents, deps: &[&str]) -> CacheRow {
           let outputs = own
               .outputs
               .iter()
               .map(|name| Digest::of(name.as_str().as_bytes()))
               .collect();
           let deps = deps
               .iter()
               .map(|dep| DepProducts {
                   label: label(dep),
                   outputs: Vec::new(),
               })
               .collect();
           let key = ActionKey::new(KeyComponents { own, deps }).unwrap();
           CacheRow::new(key, outputs, Digest::of(b""), Digest::of(b"")).unwrap()
       }

       #[test]
       fn every_reason_has_its_words() {
           let graph = graph();
           let first = planned(&graph, Rows::default());
           let own = |raw: &str| own(&first, &graph, raw);
           let mut rows = Rows::default();
           let mut record = |raw: &str, row: CacheRow| {
               rows.0.insert(label(raw), row);
           };

           // Built before, and nothing changed since.
           record("//:always", row(own("//:always"), &[]));
           record("//:docs", row(own("//:docs"), &[]));
           record("//:leaf", row(own("//:leaf"), &[]));
           // Built before from the same declaration; `//:main.o` is about to run again.
           record("//:app", row(own("//:app"), &["//:main.o"]));
           // Built before from another `main.c`.
           let mut main = own("//:main.o");
           main.inputs = vec![InputDigest {
               path: SourcePath::parse("main.c").unwrap(),
               digest: Digest::of(b"an older main.c"),
           }];
           record("//:main.o", row(main, &[]));
           // Built before with the same inputs and dependencies, each list in the other order.
           let mut reordered = own("//:reordered");
           reordered.inputs.reverse();
           record("//:reordered", row(reordered, &["//:docs", "//:leaf"]));
           // Built before by a program that was found on the machine.
           let mut script = own("//:script");
           script.tool = Some(ToolIdentity {
               program: Program::parse("cc").unwrap(),
               fingerprint: Digest::of(b"cc"),
           });
           record("//:script", row(script, &["//:leaf"]));
           // Built before with every part of its key different, and no dependency.
           let before = OwnComponents {
               command: command(&["cc", "$in"]),
               inputs: vec![InputDigest {
                   path: SourcePath::parse("changed.c").unwrap(),
                   digest: Digest::of(b"an older changed.c"),
               }],
               outputs: vec![OutputName::parse("older").unwrap()],
               env: BTreeMap::new(),
               tool: Some(ToolIdentity {
                   program: Program::parse("cc").unwrap(),
                   fingerprint: Digest::of(b"an older cc"),
               }),
               settings: BTreeMap::from([(
                   SettingName::parse("mode").unwrap(),
                   SettingValue::parse("release").unwrap(),
               )]),
               network: NetworkAccess::Declared,
               platform: Platform::parse("macos/aarch64").unwrap(),
           };
           record("//:changed", row(before, &[]));

           assert_eq!(
               plan_text(&planned(&graph, rows)),
               concat!(
                   "run    //:always: declared to run on every build\n",
                   "maybe  //:app: runs only if //:main.o produces something else\n",
                   "run    //:changed: command changed; input changed.c changed; ",
                   "declared outputs changed; environment variable CC_FLAGS changed; ",
                   "tool cc changed; setting mode changed; network access changed; ",
                   "platform changed; dependency //:docs changed\n",
                   "clean  //:docs\n",
                   "clean  //:leaf\n",
                   "run    //:main.o: input main.c changed\n",
                   "run    //:new: never built\n",
                   "run    //:reordered: inputs reordered; dependencies reordered\n",
                   "run    //:script: tool changed\n",
                   "6 to run, 1 conditional, 2 clean\n",
               )
           );
       }

       #[test]
       fn a_plan_of_nothing_is_only_the_counts() {
           let graph = resolve(Vec::new()).unwrap();
           assert_eq!(
               plan_text(&planned(&graph, Rows::default())),
               "0 to run, 0 conditional, 0 clean\n"
           );
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::plan_text
   error[E0432]: unresolved import `super::plan_text`
   error[E0432]: unresolved import `plan_text::plan_text`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/plan_text.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! A plan as lines for a person: what a build would do with each target, and why.
   //!
   //! Its own file because it is the second form of a plan. The canonical JSON of a plan holds
   //! both keys of every target, for a program to compare; this holds one line a target, saying in
   //! words what the comparison found.
   //!
   //! Responsibilities: [`plan_text`], and the words for each verdict, each reason and each
   //! difference between two keys.
   //!
   //! Non-responsibilities: reaching a verdict, which the phase that plans does; and writing the
   //! text anywhere.

   use buildl_core::{Difference, DirtyReason, NodeId, OwnComponents, Plan, PlannedKey, Verdict};

   /// One way a key differs from the recorded one, in words. `own` is the key as planned: it
   /// names the program a changed tool is.
   fn difference_text(difference: &Difference, own: &OwnComponents) -> String {
       match difference {
           Difference::Command => "command changed".to_owned(),
           Difference::Input(path) => format!("input {path} changed"),
           Difference::InputOrder => "inputs reordered".to_owned(),
           Difference::Outputs => "declared outputs changed".to_owned(),
           Difference::Env(name) => format!("environment variable {name} changed"),
           Difference::Tool => own.tool.as_ref().map_or_else(
               || "tool changed".to_owned(),
               |tool| format!("tool {} changed", tool.program),
           ),
           Difference::Setting(name) => format!("setting {name} changed"),
           Difference::Network => "network access changed".to_owned(),
           Difference::Platform => "platform changed".to_owned(),
           Difference::Dep(label) => format!("dependency {label} changed"),
           Difference::DepOrder => "dependencies reordered".to_owned(),
           _ => "something this buildl cannot name changed".to_owned(),
       }
   }

   /// Why a target will run, in words. `key` is the target's key as planned.
   fn reason_text(reason: &DirtyReason, key: &PlannedKey) -> String {
       match reason {
           DirtyReason::Always => "declared to run on every build".to_owned(),
           DirtyReason::NeverBuilt => "never built".to_owned(),
           DirtyReason::Changed(differences) => {
               let own = match key {
                   PlannedKey::Finished(key) => &key.components().own,
                   PlannedKey::Unfinished(unfinished) => &unfinished.own,
               };
               let changes: Vec<String> = differences
                   .iter()
                   .map(|difference| difference_text(difference, own))
                   .collect();
               changes.join("; ")
           }
       }
   }

   /// The labels of `ids` in `plan`, joined by a comma and a space.
   fn labels(plan: &Plan, ids: &[NodeId]) -> String {
       let names: Vec<String> = ids
           .iter()
           .filter_map(|id| plan.label(*id))
           .map(ToString::to_string)
           .collect();
       names.join(", ")
   }

   /// `plan` as text: one line per target in label order, then one line of counts, each ending
   /// with a newline.
   ///
   /// A target that needs nothing reads `clean  <label>`. One that will run reads
   /// `run    <label>: ` and the reason: `never built`, `declared to run on every build`, or
   /// every way its key differs from the one its last build recorded, joined by `; `. One that
   /// runs only if a dependency it waits on produces something else reads
   /// `maybe  <label>: runs only if <those dependencies> produces something else`. The last line
   /// counts the three: `1 to run, 1 conditional, 2 clean`.
   #[must_use]
   pub fn plan_text(plan: &Plan) -> String {
       let mut lines = Vec::new();
       let (mut run, mut maybe, mut clean) = (0_usize, 0_usize, 0_usize);
       for (verdict, id) in plan.verdicts().iter().zip((0_u32..).map(NodeId::new)) {
           let Some(label) = plan.label(id) else {
               continue;
           };
           match verdict {
               Verdict::Clean { .. } => {
                   clean += 1;
                   lines.push(format!("clean  {label}\n"));
               }
               Verdict::Dirty { reason, key } => {
                   run += 1;
                   lines.push(format!("run    {label}: {}\n", reason_text(reason, key)));
               }
               Verdict::Conditional(unfinished) => {
                   maybe += 1;
                   lines.push(format!(
                       "maybe  {label}: runs only if {} produces something else\n",
                       labels(plan, &unfinished.awaiting)
                   ));
               }
           }
       }
       lines.push(format!(
           "{run} to run, {maybe} conditional, {clean} clean\n"
       ));
       lines.concat()
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::plan_text
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/plan_text.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render a plan as text"
   ```

### Task 12 — Render a target graph as Graphviz DOT

`graph_dot` writes every target in label order, then every dependency edge by dependent in label order and dependency in written order, then every alias in label order. A label needs no escaping inside the quotes.

**Files:**
- Create `crates/buildl/src/report/dot.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //! - [`Silent`] — the reporter that shows nothing.
   //! - [`plan_text`] — a plan as lines for a person.
   //! - [`graph_dot`] — a target graph as Graphviz DOT.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod dot;
   mod paint;
   mod plan_text;
   mod silent;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use dot::graph_dot;
   pub use paint::Paint;
   pub use plan_text::plan_text;
   pub use silent::Silent;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. Create `crates/buildl/src/report/dot.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use buildl_core::{
           Action, Alias, Argument, Command, Declaration, Declared, Freshness, Label, NetworkAccess,
           Provenance, Target, TargetRole, resolve,
       };

       use super::graph_dot;

       /// Where the declaration of `label` is: the build file of its directory.
       fn declared(label: &Label, item: Declared) -> Declaration {
           let directory = label.directory().clone();
           let file = if directory.as_str().is_empty() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(directory.as_str()).join("build.lua")
           };
           Declaration::new(Provenance::new(file, directory), item)
       }

       /// The target `raw`, running `cc`, depending on `deps` in that order.
       fn target(raw: &str, deps: &[&str]) -> Declaration {
           let label = Label::parse(raw).unwrap();
           declared(
               &label,
               Declared::Target(Target {
                   label: label.clone(),
                   role: TargetRole::Build,
                   action: Action::Run(Command::new(vec![Argument::parse("cc").unwrap()]).unwrap()),
                   inputs: Vec::new(),
                   deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
                   outputs: Vec::new(),
                   env: Vec::new(),
                   network: NetworkAccess::Sealed,
                   freshness: Freshness::Cached,
               }),
           )
       }

       /// The alias `raw` for the target `of`.
       fn alias(raw: &str, of: &str) -> Declaration {
           let label = Label::parse(raw).unwrap();
           declared(
               &label,
               Declared::Alias(Alias {
                   label: label.clone(),
                   target: Label::parse(of).unwrap(),
               }),
           )
       }

       #[test]
       fn nodes_edges_and_aliases_come_in_a_fixed_order() {
           // Declared in an order that is neither label order nor the order of the text.
           let graph = resolve(vec![
               alias("//:default", "//:app"),
               target("//lib:text", &[]),
               target("//:main.o", &[]),
               target("//:docs", &[]),
               target("//:app", &["//:main.o", "//lib:text"]),
           ])
           .unwrap();
           assert_eq!(
               graph_dot(&graph),
               concat!(
                   "digraph buildl {\n",
                   "  \"//:app\";\n",
                   "  \"//:docs\";\n",
                   "  \"//:main.o\";\n",
                   "  \"//lib:text\";\n",
                   "  \"//:app\" -> \"//:main.o\";\n",
                   "  \"//:app\" -> \"//lib:text\";\n",
                   "  \"//:default\" [shape=plaintext];\n",
                   "  \"//:default\" -> \"//:app\" [style=dashed];\n",
                   "}\n",
               )
           );
       }

       #[test]
       fn a_dependency_keeps_the_order_it_was_written_in() {
           let graph = resolve(vec![
               target("//:a", &[]),
               target("//:b", &[]),
               target("//:z", &["//:b", "//:a"]),
               alias("//:all", "//:z"),
               alias("//:also", "//:a"),
           ])
           .unwrap();
           assert_eq!(
               graph_dot(&graph),
               concat!(
                   "digraph buildl {\n",
                   "  \"//:a\";\n",
                   "  \"//:b\";\n",
                   "  \"//:z\";\n",
                   "  \"//:z\" -> \"//:b\";\n",
                   "  \"//:z\" -> \"//:a\";\n",
                   "  \"//:all\" [shape=plaintext];\n",
                   "  \"//:all\" -> \"//:z\" [style=dashed];\n",
                   "  \"//:also\" [shape=plaintext];\n",
                   "  \"//:also\" -> \"//:a\" [style=dashed];\n",
                   "}\n",
               )
           );
       }

       #[test]
       fn a_graph_with_no_target_is_an_empty_digraph() {
           let graph = resolve(Vec::new()).unwrap();
           assert_eq!(graph_dot(&graph), "digraph buildl {\n}\n");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::dot
   error[E0432]: unresolved import `super::graph_dot`
   error[E0432]: unresolved import `dot::graph_dot`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/dot.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! A target graph as Graphviz DOT.
   //!
   //! Its own file because it is a second form of the graph, for a drawing tool instead of a
   //! program: the canonical JSON of a graph is the graph's own serialized form, and this is a
   //! rendering of it, with an order of its own to keep fixed.
   //!
   //! Responsibilities: [`graph_dot`].
   //!
   //! Non-responsibilities: building or narrowing a graph, and writing the text anywhere.

   use buildl_core::TargetGraph;

   /// `graph` as a Graphviz `digraph`, one statement a line, ending with a newline.
   ///
   /// Every target comes first, in label order; then every dependency as an edge from the
   /// dependent to what it depends on, by dependent in label order and by dependency in the order
   /// it was written; then every alias in label order, as a node without a border and a dashed
   /// edge to the target it names. The same graph always gives the same text.
   ///
   /// Every name is quoted and none is escaped: a label holds only ASCII letters, digits, `.`,
   /// `-`, `_`, `/` and `:`.
   #[must_use]
   pub fn graph_dot(graph: &TargetGraph) -> String {
       let mut lines = vec!["digraph buildl {\n".to_owned()];
       lines.extend(
           graph
               .nodes()
               .iter()
               .map(|node| format!("  \"{}\";\n", node.label)),
       );
       for node in graph.nodes() {
           let deps = graph
               .id_of(&node.label)
               .and_then(|id| graph.deps(id))
               .unwrap_or_default();
           lines.extend(
               deps.iter()
                   .filter_map(|id| graph.node(*id))
                   .map(|dep| format!("  \"{}\" -> \"{}\";\n", node.label, dep.label)),
           );
       }
       for (alias, id) in graph.aliases() {
           if let Some(target) = graph.node(*id) {
               lines.push(format!("  \"{alias}\" [shape=plaintext];\n"));
               lines.push(format!(
                   "  \"{alias}\" -> \"{}\" [style=dashed];\n",
                   target.label
               ));
           }
       }
       lines.push("}\n".to_owned());
       lines.concat()
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::dot
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 49 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/dot.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render a target graph as Graphviz DOT"
   ```

### Task 13 — Render the line a check ends with

`check_line` says how many declarations were found and in how many build files, which shows that subdirectories were reached.

**Files:**
- Create `crates/buildl/src/report/message.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Declare the module and export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //! - [`Silent`] — the reporter that shows nothing.
   //! - [`plan_text`] — a plan as lines for a person.
   //! - [`graph_dot`] — a target graph as Graphviz DOT.
   //! - [`check_line`] — what a check of the build files says when they are sound.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod dot;
   mod message;
   mod paint;
   mod plan_text;
   mod silent;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use dot::graph_dot;
   pub use message::check_line;
   pub use paint::Paint;
   pub use plan_text::plan_text;
   pub use silent::Silent;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. Create `crates/buildl/src/report/message.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use buildl_core::{
           Declaration, Declared, Directory, Provenance, Setting, SettingName, SettingValue,
       };

       use super::check_line;

       /// The build file of `directory`.
       fn build_file(directory: &str) -> Provenance {
           let file = if directory.is_empty() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(directory).join("build.lua")
           };
           Provenance::new(file, Directory::parse(directory).unwrap())
       }

       /// A setting named `name`, declared in the build file of `directory`.
       fn setting(directory: &str, name: &str) -> Declaration {
           Declaration::new(
               build_file(directory),
               Declared::Setting(Setting {
                   name: SettingName::parse(name).unwrap(),
                   default: SettingValue::parse("debug").unwrap(),
               }),
           )
       }

       #[test]
       fn a_check_counts_declarations_and_the_build_files_they_came_from() {
           let declarations = [
               setting("", "mode"),
               setting("", "opt"),
               setting("lib", "a"),
               setting("lib", "b"),
               setting("lib", "c"),
               setting("", "d"),
               setting("", "e"),
           ];
           assert_eq!(
               check_line(&declarations),
               "ok: 7 declarations in 2 build files\n"
           );
       }

       #[test]
       fn one_declaration_in_one_build_file_is_counted_in_the_singular() {
           assert_eq!(
               check_line(&[setting("", "mode")]),
               "ok: 1 declaration in 1 build file\n"
           );
       }

       #[test]
       fn a_workspace_that_declares_nothing_counts_nothing() {
           assert_eq!(check_line(&[]), "ok: 0 declarations in 0 build files\n");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::message
   error[E0432]: unresolved import `super::check_line`
   error[E0432]: unresolved import `message::check_line`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/report/message.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The lines a command says about itself.
   //!
   //! Its own file because none of these lines belongs to a build: each is what a command says
   //! about how it went, for a person, written beside whatever the command was asked to produce
   //! and never in it.
   //!
   //! Responsibilities:
   //!
   //! - [`check_line`] — that the build files are sound.
   //!
   //! Non-responsibilities: writing the text anywhere.

   use std::collections::BTreeSet;
   use std::path::Path;

   use buildl_core::Declaration;

   /// `count` of a thing, named by `one` for exactly one and by `many` otherwise.
   fn counted(count: usize, one: &str, many: &str) -> String {
       if count == 1 {
           format!("{count} {one}")
       } else {
           format!("{count} {many}")
       }
   }

   /// What a check of the build files says when every one of them evaluated: how many
   /// declarations were found, and in how many build files, as `ok: 7 declarations in 2 build
   /// files`. Ends with a newline.
   ///
   /// The count of build files says that subdirectories were reached. A build file that declares
   /// nothing is not counted, since only a declaration names its file.
   #[must_use]
   pub fn check_line(declarations: &[Declaration]) -> String {
       let files: BTreeSet<&Path> = declarations
           .iter()
           .map(|declaration| declaration.provenance().file())
           .collect();
       format!(
           "ok: {} in {}\n",
           counted(declarations.len(), "declaration", "declarations"),
           counted(files.len(), "build file", "build files")
       )
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::message
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/message.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render the line a check ends with"
   ```

### Task 14 — Render an error for a person

`error_text` is `error: ` and the error's own `Display`. Two errors get more lines: build files that declared differently on two evaluations are followed by both declarations as canonical JSON, and a state file that is not what buildl writes is followed by what to remove.

**Files:**
- Modify `crates/buildl/src/report/message.rs`
- Modify `crates/buildl/src/report/mod.rs`

**Steps:**

1. Export the function. Replace the whole of `crates/buildl/src/report/mod.rs` with:

   ```rust
   //! What a build shows a person, and every rendering of a value into text.
   //!
   //! Its own directory because showing is the one thing here that is for a person and not for a
   //! later build: nothing it writes is read back, and nothing a build stores or returns depends on
   //! it. It holds the adapter of the reporting port and every piece of text a command prints, so
   //! that a caller formats nothing itself, and a caller with a reporter of its own can print what
   //! this crate's does.
   //!
   //! Responsibilities:
   //!
   //! - [`Paint`] — whether what is shown is coloured.
   //! - [`command_text`] — a command on one line.
   //! - [`summary_line`] — the line that closes a build.
   //! - [`trailer`] — what closes the report of a failed target.
   //! - [`block`] — everything shown for one target once it has its outcome.
   //! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
   //!   stream it is given, with [`Mode`] saying whether a status line is kept.
   //! - [`Silent`] — the reporter that shows nothing.
   //! - [`plan_text`] — a plan as lines for a person.
   //! - [`graph_dot`] — a target graph as Graphviz DOT.
   //! - [`check_line`] — what a check of the build files says when they are sound.
   //! - [`error_text`] — an error as a command shows it.
   //!
   //! Non-responsibilities: deciding what is reported and when, which the build does; choosing
   //! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
   //! graph or a plan, which is the value's own serialized form.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod block;
   mod dot;
   mod message;
   mod paint;
   mod plan_text;
   mod silent;
   mod terminal;

   pub use block::{block, command_text, summary_line, trailer};
   pub use dot::graph_dot;
   pub use message::{check_line, error_text};
   pub use paint::Paint;
   pub use plan_text::plan_text;
   pub use silent::Silent;
   pub use terminal::{Mode, TerminalReporter};
   ```

2. Write the failing tests. In the test module of `crates/buildl/src/report/message.rs`, replace everything from the line `mod tests {` down to, but not including, the line `` /// The build file of `directory`. `` with this, leaving one blank line before that line:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use buildl_core::{
           Action, Argument, Command, Declaration, Declared, Directory, Error, Freshness, Label,
           NetworkAccess, Provenance, Setting, SettingName, SettingValue, Target, TargetRole,
       };

       use super::{check_line, error_text};
       use crate::report::paint::Paint;
   ```

   Then add at the end of the test module, above the `}` that is the last line of the file, after one blank line:

   ```rust
       /// The target `raw`, running `cc`, depending on `deps`.
       fn target(raw: &str, deps: &[&str]) -> Declaration {
           let label = Label::parse(raw).unwrap();
           Declaration::new(
               build_file(label.directory().as_str()),
               Declared::Target(Target {
                   label,
                   role: TargetRole::Build,
                   action: Action::Run(Command::new(vec![Argument::parse("cc").unwrap()]).unwrap()),
                   inputs: Vec::new(),
                   deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
                   outputs: Vec::new(),
                   env: Vec::new(),
                   network: NetworkAccess::Sealed,
                   freshness: Freshness::Cached,
               }),
           )
       }

       #[test]
       fn an_error_is_its_own_words_after_the_word_error() {
           let error = Error::UnknownOverride {
               name: SettingName::parse("optimise").unwrap(),
           };
           assert_eq!(
               error_text(&error, Paint::Plain),
               "error: setting optimise is overridden but not declared\n"
           );
       }

       #[test]
       fn the_word_error_is_bold_red_when_painted() {
           let error = Error::UnknownOverride {
               name: SettingName::parse("optimise").unwrap(),
           };
           assert_eq!(
               error_text(&error, Paint::Ansi),
               "\x1b[1;31merror\x1b[0m: setting optimise is overridden but not declared\n"
           );
       }

       #[test]
       fn a_corrupt_state_error_says_what_to_remove() {
           let error = Error::CorruptState {
               file: PathBuf::from(".buildl/cache.json"),
               source: Box::new(Error::NonCanonicalJson { offset: 586 }),
           };
           assert_eq!(
               error_text(&error, Paint::Plain),
               concat!(
                   "error: .buildl/cache.json is not a file buildl wrote: ",
                   "JSON is not in canonical form: it differs from it at byte 586\n",
                   "    remove .buildl/cache.json and run the command again: ",
                   "every target is then built anew\n",
               )
           );
       }

       #[test]
       fn a_nondeterministic_error_shows_both_declarations() {
           let error = Error::Nondeterministic {
               provenance: build_file("lib"),
               first_run: Some(Box::new(target("//lib:text", &[]))),
               second_run: Some(Box::new(target("//lib:text", &["//:main.o"]))),
           };
           assert_eq!(
               error_text(&error, Paint::Plain),
               concat!(
                   "error: lib/build.lua: declarations differ between two evaluations\n",
                   "    first evaluation:  {\"item\":{\"Target\":{\"action\":{\"Run\":[\"cc\"]},",
                   "\"deps\":[],\"env\":[],\"freshness\":\"Cached\",\"inputs\":[],",
                   "\"label\":\"//lib:text\",\"network\":\"Sealed\",\"outputs\":[],",
                   "\"role\":\"Build\"}},",
                   "\"provenance\":{\"directory\":\"lib\",\"file\":\"lib/build.lua\"}}\n",
                   "    second evaluation: {\"item\":{\"Target\":{\"action\":{\"Run\":[\"cc\"]},",
                   "\"deps\":[\"//:main.o\"],\"env\":[],\"freshness\":\"Cached\",\"inputs\":[],",
                   "\"label\":\"//lib:text\",\"network\":\"Sealed\",\"outputs\":[],",
                   "\"role\":\"Build\"}},",
                   "\"provenance\":{\"directory\":\"lib\",\"file\":\"lib/build.lua\"}}\n",
               )
           );
       }

       #[test]
       fn an_evaluation_that_declared_nothing_there_says_so() {
           let error = Error::Nondeterministic {
               provenance: build_file(""),
               first_run: Some(Box::new(setting("", "mode"))),
               second_run: None,
           };
           assert_eq!(
               error_text(&error, Paint::Plain),
               concat!(
                   "error: build.lua: declarations differ between two evaluations\n",
                   "    first evaluation:  {\"item\":{\"Setting\":{\"default\":\"debug\",",
                   "\"name\":\"mode\"}},\"provenance\":{\"directory\":\"\",\"file\":\"build.lua\"}}\n",
                   "    second evaluation: nothing\n",
               )
           );
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib report::message
   error[E0432]: unresolved import `super::error_text`
   error[E0432]: unresolved import `message::error_text`
   ```

4. Implement it. Replace the top of `crates/buildl/src/report/message.rs`, everything above the line `` /// `count` of a thing, named by `one` for exactly one and by `many` otherwise. ``, with this, leaving one blank line before that line:

   ```rust
   //! The lines a command says about itself.
   //!
   //! Its own file because none of these lines belongs to a build: each is what a command says
   //! about how it went, for a person, written beside whatever the command was asked to produce
   //! and never in it.
   //!
   //! Responsibilities:
   //!
   //! - [`check_line`] — that the build files are sound.
   //! - [`error_text`] — why the command failed.
   //!
   //! Non-responsibilities: writing the text anywhere; the words of an error, which are the
   //! error's own `Display`; and choosing an exit status, which reads the error's variant, never
   //! this text.

   use std::collections::BTreeSet;
   use std::path::Path;

   use buildl_core::json::canonical;
   use buildl_core::{Declaration, Error};

   use crate::report::paint::Paint;
   ```

   Then add directly above the line `#[cfg(test)]`, followed by one blank line:

   ```rust
   /// One evaluation's declaration on one line, or `nothing` when that evaluation had none there.
   fn evaluated(declaration: Option<&Declaration>) -> String {
       declaration.map_or_else(
           || "nothing".to_owned(),
           |declaration| {
               canonical::to_string(declaration).unwrap_or_else(|_| format!("{declaration:?}"))
           },
       )
   }

   /// An error as a command shows it: `error: ` and the error's own words, ending with a newline.
   ///
   /// Two errors get more lines, each indented by four spaces. Build files that declared
   /// differently on two evaluations are followed by the declaration each evaluation had at the
   /// first difference, as canonical JSON. A state file that is not what buildl writes is followed
   /// by what to do about it: remove the file and run the command again.
   ///
   /// Under [`Paint::Ansi`] the word `error` is bold red.
   #[must_use]
   pub fn error_text(error: &Error, paint: Paint) -> String {
       let mut lines = vec![format!("{}: {error}\n", paint.failure("error"))];
       match error {
           Error::Nondeterministic {
               first_run,
               second_run,
               ..
           } => {
               lines.push(format!(
                   "    first evaluation:  {}\n",
                   evaluated(first_run.as_deref())
               ));
               lines.push(format!(
                   "    second evaluation: {}\n",
                   evaluated(second_run.as_deref())
               ));
           }
           Error::CorruptState { file, .. } => {
               lines.push(format!(
                   "    remove {} and run the command again: every target is then built anew\n",
                   file.display()
               ));
           }
           _ => {}
       }
       lines.concat()
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib report::message
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out; finished in 0.00s
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
   $ git add crates/buildl/src/report/message.rs crates/buildl/src/report/mod.rs
   $ git commit -m "feat(buildl): render an error as a command shows it"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`. `Cargo.toml`, `Cargo.lock`, `deny.toml`, `Makefile.toml`, the two golden files and `crates/buildl/src/lib.rs` are unchanged by this plan.
- `cargo test -p buildl --lib report::` runs 60 tests: 4 in `paint`, 21 in `block`, 21 in `terminal`, 1 in `silent`, 2 in `plan_text`, 3 in `dot`, 8 in `message`.
- Every test the design names for `report/` exists under its name:
  - `block.rs`: `a_silent_success_prints_nothing`, `a_cached_target_is_marked`, `a_failure_without_output_still_has_a_trailer`, `an_action_a_signal_ended_says_which`, `bytes_that_are_not_text_pass_unchanged`, `a_last_line_without_a_newline_gets_one`, `an_argument_with_a_space_is_quoted`, `the_summary_leaves_out_zero_counts`, `a_build_of_nothing_says_so`;
  - `plan_text.rs`: `every_reason_has_its_words`;
  - `dot.rs`: `nodes_edges_and_aliases_come_in_a_fixed_order`;
  - `message.rs`: `a_corrupt_state_error_says_what_to_remove`, `a_nondeterministic_error_shows_both_declarations`;
  - `terminal.rs`: `plain_never_writes_an_escape`, `live_erases_before_a_block_and_redraws_after`, `nothing_is_drawn_after_the_summary`, `a_failed_write_stops_writing_and_does_not_panic`, `a_live_action_gets_its_header_once_and_its_bytes_as_they_come`, `a_live_action_that_ends_mid_line_gets_a_newline_before_its_trailer`, `no_status_line_is_drawn_while_an_action_is_shown_live`, `close_erases_the_status_line`;
  - `silent.rs`: `every_call_is_taken_and_nothing_happens`.
- The examples of spec §7 and §9.2 are asserted byte for byte: the keep-going build with two failures (`a_build_with_failures_is_blocks_then_the_summary`), the cached build that replays `caf` and the byte `0xE9` (`a_cached_build_replays_what_was_printed_byte_for_byte`, `bytes_that_are_not_text_pass_unchanged`), the status line and the erase in front of a bold header (`live_erases_before_a_block_and_redraws_after`), the DOT listing (`nodes_edges_and_aliases_come_in_a_fixed_order`), the summary lines `2 failed, 1 skipped, 1 cached`, `1 failed, 2 skipped, 1 cached`, `4 built`, `4 cached` and `2 built, 1 cached` (`the_summary_leaves_out_zero_counts`), and the plan lines for a clean, a changed and a conditional target (`every_reason_has_its_words`).
- Every write of `TerminalReporter` goes through one function, once per call, and a failed write or flush ends the writing without a panic and without an error (`each_call_is_one_write_and_one_flush`, `a_failed_write_stops_writing_and_does_not_panic`, `a_failed_flush_stops_writing_too`).
- `report/` names `buildl_core` and `std` and no other module of the crate; the body of `guard-adapter-isolation` prints `guard-adapter-isolation: OK` with no edge from `report`.
- What plans `09` and `11` are written against, each reached as `buildl::report::<item>`:

  | Item | Signature | Its text |
  |---|---|---|
  | `Paint` | `enum Paint { Plain, Ansi }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`); `header(self, &str) -> String`, `failure(self, &str) -> String` | bold `\x1b[1m…\x1b[0m`, bold red `\x1b[1;31m…\x1b[0m`; unchanged under `Plain` |
  | `Mode` | `enum Mode { Plain, Live }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`) | — |
  | `TerminalReporter<W: Write>` | `pub const fn new(out: W, mode: Mode, paint: Paint) -> Self`; `pub fn close(&mut self)`; `impl<W: Write> Reporter`; `Debug` when `W: Debug` | blocks, status line, summary |
  | `Silent` | unit struct (`Debug`, `Clone`, `Copy`, `Default`); `impl Reporter` | nothing |
  | `command_text` | `fn(&Command) -> String` | one line, no newline |
  | `summary_line` | `fn(Summary) -> String` | one line, ends with `\n` |
  | `trailer` | `fn(&OutcomeEvent, Paint) -> String` | empty, or lines each ending with `\n` |
  | `block` | `fn(&OutcomeEvent, Captured<'_>, Paint) -> Vec<u8>` | empty, or lines each ending with `\n` |
  | `plan_text` | `fn(&Plan) -> String` | lines each ending with `\n` |
  | `graph_dot` | `fn(&TargetGraph) -> String` | lines each ending with `\n` |
  | `check_line` | `fn(&[Declaration]) -> String` | one line, ends with `\n` |
  | `error_text` | `fn(&Error, Paint) -> String` | `error: ` and the error's own words, then up to two more lines; ends with `\n` |
