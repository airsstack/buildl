---
status: approved
created: 2026-10-15
depends-on: [02]
built-on: fefed82cb705
---

# Outcomes that know a test Implementation Plan

**Goal:** A build's outcomes and its summary tell the test targets apart from the others.

**Architecture:** `ActionOutcome` gains `role: TargetRole`, copied from the node by every outcome
helper of `execute/record.rs`; the log's twelve-key view does not read it, so `log.jsonl` stays
byte-identical. `Summary` becomes two counts of one canonical type: `Summary { all: Tally,
tests: Tally }` with `Tally { failed, skipped, built, cached }`. Every reader of the old four
fields reads `summary.all`, and `summary_line` takes a `Tally`.

**Tech Stack:** Rust 2024 (`rust-version` 1.94). Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §3.6, §5, §7 (second risk), §6 (rows for `types/outcome.rs` and
`types/outcome_event.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits.
Both tasks change a `buildl-core` type every crate reads, so each also edits `buildl` and
`buildl-cli`, and commits with scope `repo`.

Facts the tasks rest on, each read at commit `1799d47`:

| Fact | Where |
|---|---|
| `ActionOutcome { label, class, diagnostic, outputs, stdout, stderr }`, all public, `Debug, Clone, PartialEq, Eq`, no `Serialize` | `crates/buildl-core/src/types/outcome.rs:109-127` |
| `Summary { failed, skipped, built, cached }` (`Debug, Clone, Copy, PartialEq, Eq`) and `Summary::of(&[ActionOutcome])` with the class-to-count match | `outcome.rs:129-166` |
| `outcome.rs`'s summary tests: `a_summary_counts_each_class_once`, `a_summary_of_no_outcome_counts_nothing`, `a_summary_adds_outcomes_of_one_class` | `outcome.rs:237`, `:251`, `:264` |
| The log writes an explicit twelve-key view; its test `an_event_holds_exactly_the_twelve_keys_of_the_log_format` | `crates/buildl-core/src/types/outcome_event.rs:46-59`, `:243` |
| Outcome helpers: `built` `:55`, `cached` `:61`, `failed` `:68` (the struct literal), `unstored` `:86`, `unplaced` `:97`, `mismatched` `:109`, `skipped` `:119`, `store` `:157`, `restore` `:186`, `succeeded` `:289` (the other literal) | `crates/buildl-core/src/execute/record.rs` |
| Their callers in the build: `store` and `failed` in `Build::record`, `restore` in `Build::drain`, `skipped` in `Build::skip`; each has `target.node` (a `Node`, whose `role` is public) | `crates/buildl-core/src/execute/assembly.rs:398-442`, `:442-470`, `:511-521`; `types/target_graph.rs:33` |
| `ActionOutcome {` literals besides the two helpers (the grep also matches `-> ActionOutcome {` signatures, e.g. `tests/flows/build.rs:262`): `types/outcome.rs` tests, `types/outcome_event.rs` (4), `ports/reporter.rs:153` (doc example), `tests/flows/common.rs:1226`, `execute/record.rs`'s tests (`:447-584`); `buildl`: `log/local_log.rs:169`, `report/silent.rs:72`, `report/terminal.rs:288`, `report/block.rs:290`; `buildl-cli`: `src/exit.rs:363-364` | `grep -rn 'ActionOutcome {' --include='*.rs' crates` |
| `Summary` readers outside `outcome.rs`: `buildl-core` `lib.rs`, `types/mod.rs`, `execute/assembly.rs` (`Summary::of` at `:598`), `ports/{reporter,bundle,port_set}.rs`, `tests/flows/{build,common,report}.rs`; `buildl` `report/{block,silent,terminal}.rs`, `workspace.rs`, `tests/e2e/failure.rs` | `grep -rn 'Summary\b' --include='*.rs' crates`, 14 files, spec §3.6 |
| `summary_line(summary: Summary) -> String`, order failed, skipped, built, cached; `nothing to build` when all zero; called by the terminal reporter's `summary` | `crates/buildl/src/report/block.rs:58-75`; `report/terminal.rs:216` |
| Root re-exports `Summary` from `types`; `types/mod.rs:155` `pub use outcome::{ActionOutcome, InfraKind, OutcomeClass, OutputRefusal, Summary};` | `crates/buildl-core/src/lib.rs:122-135` |

## Files that change

```
crates/buildl-core/src/types/outcome.rs          — [modify] ActionOutcome.role; Tally; Summary { all, tests }; tests
crates/buildl-core/src/types/mod.rs              — [modify] re-export Tally
crates/buildl-core/src/lib.rs                    — [modify] Tally in the root re-exports
crates/buildl-core/src/types/outcome_event.rs    — [modify] role in its test literals
crates/buildl-core/src/execute/record.rs         — [modify] every helper takes and sets role; tests
crates/buildl-core/src/execute/assembly.rs       — [modify] pass target.node.role to the helpers
crates/buildl-core/src/ports/reporter.rs         — [modify] doc example literal; summary's doc
crates/buildl-core/src/ports/bundle.rs           — [modify] doc example reads the new Summary
crates/buildl-core/src/ports/port_set.rs         — [modify] its test reporter reads the new Summary
crates/buildl-core/tests/flows/build.rs          — [modify] literal; summaries read .all
crates/buildl-core/tests/flows/common.rs         — [modify] literal; FakeReporter keeps the new Summary
crates/buildl-core/tests/flows/report.rs         — [modify] summaries read .all
crates/buildl/src/log/local_log.rs               — [modify] role in its test literal
crates/buildl/src/report/block.rs                — [modify] summary_line takes a Tally; test literal
crates/buildl/src/report/silent.rs               — [modify] literal; Summary in its test
crates/buildl/src/report/terminal.rs             — [modify] summary_line(summary.all); test literal and counts()
crates/buildl/src/workspace.rs                   — [modify] tests read .all
crates/buildl/tests/e2e/failure.rs               — [modify] summaries read .all
crates/buildl-cli/src/exit.rs                    — [modify] role in its test literals
```

## Order of work

### Task 1 — An outcome carries its target's role

**Files:** `types/outcome.rs`, `types/outcome_event.rs`, `execute/record.rs`,
`execute/assembly.rs`, `ports/reporter.rs`, `tests/flows/build.rs`, `tests/flows/common.rs`
(all `crates/buildl-core`); `crates/buildl/src/log/local_log.rs`,
`crates/buildl/src/report/{silent,terminal,block}.rs`; `crates/buildl-cli/src/exit.rs`.

**Change:**
- `ActionOutcome` gains `pub role: TargetRole` after `label`, documented "Whether the target was
  declared with `b.target` or `b.test`." `outcome_event.rs`'s view is not changed: `role` is not
  one of the twelve keys.
- Every helper of `record.rs` that makes an outcome takes `role: TargetRole` right after `label`
  and sets it: `built`, `cached`, `failed`, `unstored`, `unplaced`, `mismatched`, `skipped`,
  `succeeded`, and the two that call them, `store` and `restore`. `skipped`'s role is the
  skipped target's own, not the cause's.
- `assembly.rs` passes `target.node.role` at each call (`record`, `drain`, `skip`).
- Every other literal adds `role: TargetRole::Build` (or `Test` where a test names a test). The
  compiler lists any literal the grep above missed.

**Proof:**
- `every_outcome_carries_the_role_it_was_given` in `record.rs`'s tests — `built`, `cached`,
  `failed`, `unplaced`, `mismatched` and `skipped`, each given `TargetRole::Test`, return an
  outcome whose `role` is `Test`; `built` given `Build` returns `Build`.
- `an_event_holds_exactly_the_twelve_keys_of_the_log_format` (`outcome_event.rs:243`) — existing,
  unchanged, passes: the log has no `role`.
- `a_two_directory_workspace_…_logs_the_pinned_events` (`tests/flows/build.rs:1120`) — existing,
  passes on the unchanged `golden/log.jsonl`.

**Done when:** `cargo test --workspace --all-targets` passes and the Definition of Done is
green. Commit `feat(repo): an outcome carries its target's role`.

### Task 2 — A summary counts the tests apart

**Files:** `types/outcome.rs`, `types/mod.rs`, `lib.rs`, `execute/assembly.rs`,
`ports/{reporter,bundle,port_set}.rs`, `tests/flows/{build,common,report}.rs` (all
`crates/buildl-core`); `crates/buildl/src/report/{block,silent,terminal}.rs`,
`crates/buildl/src/workspace.rs`, `crates/buildl/tests/e2e/failure.rs`.

**Change:**
- `pub struct Tally { pub failed, pub skipped, pub built, pub cached }` (all `usize`), deriving
  `Debug, Clone, Copy, PartialEq, Eq, Default`, documented "How many targets ended in each way."
  with the four field docs moved from `Summary`. `Tally::of<'a>(outcomes: impl
  IntoIterator<Item = &'a ActionOutcome>) -> Self` holds the class-to-count match that
  `Summary::of` holds today, unchanged.
- `pub struct Summary { pub all: Tally, pub tests: Tally }`, same derives as today, documented
  "How the targets of one build ended: all of them, and the tests among them." `Summary::of`
  keeps its signature: `all` is `Tally::of(outcomes)`, `tests` is `Tally::of` over the outcomes
  whose `role` is `TargetRole::Test`.
- `types/mod.rs` re-exports `Tally` from `outcome`; `lib.rs` adds it to the root list.
- `buildl`'s `summary_line(tally: Tally) -> String` — the same text as today; the terminal
  reporter calls `summary_line(summary.all)`. Its doc says it is the line closing a build.
- Every other reader reads `summary.all.<count>` or builds `Summary { all: Tally { … }, tests:
  Tally::default() }`; test helpers named `counts(…)` (`terminal.rs:326`, `block.rs:241`) return
  the type their callers now need (`Tally` for `summary_line`, `Summary` for the reporter).

**Proof:**
- `a_summary_tallies_tests_apart_from_every_target` in `outcome.rs`'s tests — outcomes: a built
  build target, a cached test, a failed test, a skipped test, a cached build target. `all` is
  `{ failed 1, skipped 1, built 1, cached 2 }`; `tests` is `{ failed 1, skipped 1, built 0,
  cached 1 }`.
- The three existing summary tests of `outcome.rs` assert on `.all` and pass; each also asserts
  `tests` is `Tally::default()`, their outcomes being build targets.
- Every existing summary assertion in `buildl` and the flows passes reading `.all`; the summary
  line text is unchanged (`block.rs`'s `the_summary_leaves_out_zero_counts`,
  `the_summary_names_every_count_in_a_fixed_order`, `a_build_of_nothing_says_so`).

**Done when:** `cargo test --workspace --all-targets` passes and the Definition of Done is
green. Commit `feat(repo): a summary counts the tests apart`.

## Risks

- **Wide, mechanical edits.** Task 1 touches every outcome literal in three crates and Task 2
  every summary reader (spec §7). The compiler finds each; the risk is a reader that silently
  starts counting the wrong tally, which is why every existing summary assertion moves to `.all`
  rather than being rewritten.
- **Most risky step:** Task 1's `skipped` — its role must be the skipped target's, which the
  named proof covers.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `git diff --exit-code crates/buildl-core/tests/flows/golden` clean after each task.
