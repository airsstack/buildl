---
status: done
created: 2026-10-15
depends-on: [04]
built-on: fefed82cb705
---

# The terminal reporter of a test run Implementation Plan

**Goal:** The terminal reporter shows a test run in its own words: blocks only for what failed,
unless asked for every test, and a summary line about tests.

**Architecture:** `TerminalReporter::new` gains a fourth argument, `Report`, chosen by the caller:
`Report::Build` keeps today's behaviour byte for byte; `Report::Test { blocks, rerun }` hides the
block of a test that passed (built or cached) unless `blocks` is `TestBlocks::Every`, and closes
with `test_line(summary.tests, rerun)`. A dependency (a `Build`-role target) is shown as a build
shows it. The reporter port's methods keep their signatures; the decision reads
`event.outcome.role` (plan `03`). Both enums and `test_line` are pure `buildl` values, beside the
renderings they choose between.

**Tech Stack:** Rust 2024 (`rust-version` 1.94). Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §2 (Printing, Summary), §3.7, §6 (rows for `report/block.rs`,
`report/terminal.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits.

Facts the tasks rest on, each read at commit `1799d47` or left by an earlier plan:

| Fact | Where |
|---|---|
| `TerminalReporter::new(out, mode, paint)` (`const fn`) and its fields; `finished` writes the trailer for the target shown live and otherwise `block(event, printed, paint)`, then redraws; `summary` erases and writes `summary_line(…)` | `crates/buildl/src/report/terminal.rs:47-96`, `:195-220` |
| `block` writes nothing for a target that printed nothing and did not fail; a cached target's header is marked ` (cached)` | `crates/buildl/src/report/block.rs:166-187` |
| `summary_line` (after plan `03`: takes a `Tally`): non-zero counts joined by `, `, `nothing to build` when all are zero, a newline at the end | `block.rs:54-75`; `plans/03-test-outcomes.md` Task 2 |
| `report/mod.rs` re-exports `block, command_text, summary_line, trailer` and `Mode, TerminalReporter` | `crates/buildl/src/report/mod.rs` |
| `TerminalReporter::new` is called at `crates/buildl-cli/src/run.rs:243`, `crates/buildl/tests/e2e/common.rs:227`, and in `terminal.rs`'s tests (`:336` helper `reporter(mode, paint)`, `:577`, `:600`, `:652`, `:666`) | `grep -rn 'TerminalReporter::new' --include='*.rs' crates` |
| `terminal.rs`'s test helpers: `event(raw, class, arguments)` (`:273`, builds an `OutcomeEvent` around an `ActionOutcome` literal), `on_stdout`, `on_stderr`, `nothing`, `counts`, `written`; the existing `a_cached_build_replays_what_was_printed_byte_for_byte` (`:469`) and the live tests (`:673-788`) | `terminal.rs:240-345` |
| `block.rs`'s summary tests: `the_summary_leaves_out_zero_counts`, `the_summary_names_every_count_in_a_fixed_order`, `a_build_of_nothing_says_so` | `block.rs:251`, `:266`, `:274` |
| `Rerun { Off, Tests }` and `Tally` are `buildl-core` types re-exported at its root (plans `03`, `04`); `ActionOutcome.role` (plan `03`) | as cited |
| Spec §2's summary wording: `tests: 3 passed (2 cached), 1 failed, 1 skipped`; zero counts left out; `no tests` when nothing was selected; `tests (rerun): …` under `--rerun` | `.claudestacks/sdlc/2026-10-15-test/spec.md:53` |

## Files that change

```
crates/buildl/src/report/block.rs       — [modify] test_line and its tests
crates/buildl/src/report/terminal.rs    — [modify] Report, TestBlocks, the fourth argument, hiding, the test line; tests
crates/buildl/src/report/mod.rs         — [modify] re-export Report, TestBlocks, test_line
crates/buildl/tests/e2e/common.rs       — [modify] build_shown passes Report::Build
crates/buildl-cli/src/run.rs            — [modify] build passes Report::Build
```

## Order of work

### Task 1 — The line that closes a test run

**Files:** `crates/buildl/src/report/block.rs`, `crates/buildl/src/report/mod.rs`.

**Change:** `pub fn test_line(tally: Tally, rerun: Rerun) -> String`, `#[must_use]`, in
`block.rs`, listed in its module doc's Responsibilities ("the line that closes a test run"):
- `passed` is `tally.built + tally.cached`. The parts, each only when its count is not zero, in
  this order: `{passed} passed`, followed by ` ({cached} cached)` when `tally.cached` is not zero;
  `{failed} failed`; `{skipped} skipped`. They are joined by `, `.
- Prefix `tests: `, or `tests (rerun): ` when `rerun` is `Rerun::Tests`.
- When every count is zero the line is `no tests`, with no prefix, whatever `rerun` is.
- A newline ends it.
`report/mod.rs` re-exports `test_line`.

**Proof** (in `block.rs`'s tests):
- `a_test_line_counts_passes_with_their_cached_share` — `{ built 1, cached 2, failed 1,
  skipped 1 }`, `Off` → `tests: 3 passed (2 cached), 1 failed, 1 skipped\n`.
- `a_test_line_leaves_zero_counts_out` — `{ built 2 }` → `tests: 2 passed\n`; `{ failed 1 }` →
  `tests: 1 failed\n`.
- `a_run_without_tests_says_no_tests` — all zero, `Off` and `Tests` → `no tests\n`.
- `a_rerun_line_says_rerun` — `{ built 2 }`, `Tests` → `tests (rerun): 2 passed\n`.

**Done when:** `cargo test -p buildl --lib report::block` passes and the Definition of Done is
green. Commit `feat(buildl): the line that closes a test run`.

### Task 2 — A reporter told what kind of run it shows

**Files:** `crates/buildl/src/report/terminal.rs`, `crates/buildl/src/report/mod.rs`,
`crates/buildl/tests/e2e/common.rs`, `crates/buildl-cli/src/run.rs`.

**Change:**
- In `terminal.rs`: `pub enum Report { Build, Test { blocks: TestBlocks, rerun: Rerun } }` and
  `pub enum TestBlocks { Failed, Every }`, both `Debug, Clone, Copy, PartialEq, Eq`, documented
  ("What kind of run a reporter shows, which decides what it prints of a passing test and how it
  sums up"; "Which tests of a test run get a block: those that failed, or every one"). The
  module doc's Responsibilities name both.
- `TerminalReporter::new(out, mode, paint, report: Report)`, stored as a field.
- `finished`: a target is *hidden* when `report` is `Test { blocks: Failed, .. }`, the outcome's
  `role` is `TargetRole::Test`, and its class succeeded (`Built` or `Cached`). A hidden target
  not shown live gets no block: the frame only erases and redraws the status line, and `done`
  still advances. A target shown live (`--stream`) ends as today — its bytes were already shown —
  hidden or not. Every other target, and every target under `Report::Build` or `TestBlocks::Every`,
  is shown as today.
- `summary`: `summary_line(summary.all)` for `Build`; `test_line(summary.tests, rerun)` for
  `Test`.
- `report/mod.rs` re-exports `Report` and `TestBlocks`. Every existing caller passes
  `Report::Build`: `run.rs:243`, `e2e/common.rs:227`, and `terminal.rs`'s tests.

**Proof** (in `terminal.rs`'s tests, over a helper `test_event(raw, class)` that is `event` with
role `Test`, and `Mode::Plain` unless said):
- `a_test_run_hides_a_passing_test_that_printed` — `Test { Failed, Off }`: a `Built` test with
  stdout `ok\n` writes nothing.
- `a_test_run_hides_the_replay_of_a_cached_pass` — the same with a `Cached` test.
- `a_test_run_shows_the_block_of_a_failing_test` — an `ActionFailure` test with stdout writes the
  bytes `block` writes for it.
- `a_test_run_shows_a_dependency_as_a_build_does` — a `Built` build-role target with stdout
  writes what `Report::Build` writes for it.
- `a_verbose_test_run_shows_every_test` — `Test { Every, Off }`: the `Built` and the `Cached`
  test each write their block, the cached one marked ` (cached)`.
- `a_test_run_ends_with_the_test_line` — `summary` of a `Summary` whose `tests` is `{ built 1,
  failed 1 }` writes `tests: 1 passed, 1 failed\n`.
- `a_rerun_test_run_ends_with_the_rerun_line` — `Test { Failed, Tests }` and `tests { built 1 }`
  writes `tests (rerun): 1 passed\n`.
- `a_live_test_run_still_advances_the_status_line_past_a_hidden_pass` — `Mode::Live`, `begun(2)`,
  a test started, then finished `Built` and hidden: the frame erases and draws `[1/2]`.
- The existing tests pass under `Report::Build`, byte for byte.

**Done when:** `cargo test -p buildl --lib report::` passes and the Definition of Done is green.
Commit `feat(repo): a terminal reporter that shows a test run`.

## Risks

- **A hidden pass must not leave the screen out of step.** The live test above pins that the
  status line is still erased and redrawn; the live path (`--stream`) is left as it is.
- `test_line`'s exact words are the spec's §2 example; plan `09`'s CLI tests pin them again on the
  real binary.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.

## Review findings

- nit — `test_reporter`'s doc named a `report` parameter it does not take — `terminal.rs` — fixed
- risk — no test showed that a failed test other than an exit status keeps its block under `TestBlocks::Failed` — `terminal.rs` — fixed: `a_test_run_shows_the_block_of_a_test_whose_output_is_missing` (a skipped test was tried first, but `block` writes nothing for a skipped target that printed nothing, so it could not tell hiding from showing)
- question — the two tasks land as one commit — kept: the branch commits one commit per plan

## Probe results

- None needed: the plan's one risk, a hidden pass leaving the screen out of step, is pinned by `a_live_test_run_still_advances_the_status_line_past_a_hidden_pass`.

## Deviations

- One test beyond the plan: `a_test_shown_live_ends_as_a_live_action_does_even_when_it_passes` pins Task 2's rule that a target shown live ends as today, hidden or not.
- Clippy's `assert_is_empty` refused `assert!(!bytes.is_empty())`; the tests assert `assert_ne!(bytes, b"")`.
- Both tasks are one commit, `feat(repo): a terminal reporter that shows a test run`.
