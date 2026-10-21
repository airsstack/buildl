---
status: done
created: 2026-10-21
depends-on: [02]
built-on: 6c7f83a41771
---

# CLI Verify Implementation Plan

**Goal:** The `buildl` binary runs `buildl verify`, prints its report and exits by spec §2's Exit row.

**Architecture:** `Command::Verify(VerifyArgs)` in `cli.rs`; the report text in a new
`buildl/src/report/verify_text.rs` beside `message.rs` and `block.rs`, where every other command's
text lives; `run.rs` calls `Workspace::verify` and maps the `Verification` to an `Exit` in
`exit.rs`, reusing `Exit::of_class` and `Exit::severity`.

**Tech Stack:** Rust, `buildl`, `buildl-cli`, clap, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2 (Command, Report, Exit), §3.5, §4, §5 (CLI row).

---

## Files that change

```
crates/buildl/src/report/verify_text.rs   — [create] verify_text(&Verification, Paint) -> String
crates/buildl/src/report/mod.rs           — [modify] declare and re-export it
crates/buildl-cli/src/cli.rs              — [modify] Command::Verify, VerifyArgs (--sample)
crates/buildl-cli/src/run.rs              — [modify] the verify command
crates/buildl-cli/src/exit.rs             — [modify] Exit::of_verification
crates/buildl-cli/src/main.rs             — [modify] crate docs: the command table (:12-20) and the exits (:44-55) gain verify
crates/buildl-cli/tests/cli/common.rs     — [modify] a manifest helper for a sandboxed workspace
crates/buildl-cli/tests/cli/verify.rs     — [create] binary tests
crates/buildl-cli/tests/cli/main.rs       — [modify] `mod verify;`
```

The report text lives in `buildl/src/report/verify_text.rs`, beside `plan_text.rs`,
`query_text.rs` and `log_text.rs`, as spec §3.5 says.

## Order of work

### Task 1 — The report text

**Files:** `report/verify_text.rs`, `report/mod.rs`.

**Change:** `verify_text` writes one line per checked target in label order, then the summary
line `N verified, N mismatched, N failed, N skipped, N not cached, N not sampled` (spec §2
Report). Line shapes:
- verified: `verified <label>`
- mismatched: `MISMATCH <label>: output <name> differs` then `: bytes` / `: mode` for a file,
  `: <path>: bytes|mode|only in the cache|only in the re-run` for a tree entry, `: a file in one,
  a directory in the other` for `Kind`.
- failed: the header and failure text of `build`'s block (`report/block.rs:135` `failure_text`,
  `:189` `header`), the diagnostic, then what it printed.
- skipped: `skipped <label>: needs <because>, which could not be checked`.
- unplaced: `unplaced <label>: <diagnostic>`.
The summary line has no unplaced slot: an unplaced target is counted under `skipped` (both could
not be checked, both exit 3), so the counts add up to the report's lines.
Failure-class lines painted as `build`'s failures (`Paint::failure`).

**Proof:** written first, seen failing, then passing —
`each_check_has_its_line`, `lines_are_in_label_order`, `the_summary_counts_every_kind`,
`a_tree_mismatch_names_the_path_and_the_change`, `nothing_to_check_says_so` (`0 verified, …, 4 not cached`).

**Done when:** `cargo test -p buildl --lib report::` passes and `cargo make dod` is green. Commit
`feat(buildl): write verify's report`.

### Task 2 — The command and its exit

**Files:** `cli.rs`, `run.rs`, `exit.rs`, `main.rs`, `tests/cli/common.rs`, `tests/cli/verify.rs`,
`tests/cli/main.rs`.

**Change:**
- `cli.rs`: a `Verify(VerifyArgs)` command, help "Re-run cached actions and name those whose
  outputs differ". `VerifyArgs` holds its own targets and `--set` fields, with help text about
  verify (`Selection`'s help, `cli.rs:109`, speaks of plan, build and test only), and `--sample`,
  an optional `NonZeroUsize`. clap rejects `0` and non-numbers (exit 2, usage). No sample →
  `Sample::All`, `n` → `Sample::Lowest(n)`.
- `run.rs`: the `Verify` arm beside `build` (`run.rs:106`) calls a `verify` function: scope and
  overrides parsed by the functions `build` uses, `Workspace::verify`, `verify_text` to stderr,
  `Exit::of_verification` returned.
- `main.rs` crate docs: `verify` in the command table and in the exits text.
- `tests/cli/common.rs`: a helper for a manifest without the `strategy = "bare"` line that
  `common::manifest` always writes (`common.rs:44-50`), so the workspace runs `sandbox`.
- `exit.rs`: `Exit::of_verification(&Verification)`: `Mismatched` → `ActionFailure`; `Failed`
  → `of_class(class)`; `Skipped` and `Unplaced` → `Infrastructure`; `Verified` → `Success`;
  the most severe by `severity` (`exit.rs:123`); none checked → `Success`. Errors map through the
  existing `of_error`.

**Proof:** written first, seen failing, then passing —
- `exit.rs` unit: `a_mismatch_exits_as_an_action_failure`, `a_skip_or_an_unplaced_exits_as_infrastructure`,
  `a_failed_rerun_exits_as_its_class`, `the_most_severe_check_decides`, `nothing_checked_is_success`.
- `run.rs` colocated tests (its `run(…)` harness, `run.rs:417-468`): `verify_writes_its_report_to_stderr`.
- `tests/cli/verify.rs` (through the binary, with `common::workspace`/`program` and the sandboxed
  manifest helper; the bare and container cases write their own `[execution]` line):
  `verify_of_a_reproducible_workspace_exits_zero`, `a_mismatch_exits_one_and_names_target_and_output`,
  `verify_under_bare_exits_five_naming_the_line_to_write`, `verify_under_container_exits_five`,
  `sample_zero_is_a_usage_error`, `sample_takes_a_number`.

**Done when:** `cargo test -p buildl-cli` passes and `cargo make dod` is green. Commit
`feat(buildl-cli): add the verify command`.

---

## Risks

- A `Verification` with a `Failed` check of class `Skipped` cannot occur (core never builds one);
  `of_class` handles it anyway.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `buildl verify --help` lists `--sample` and the targets.

## Review findings

Tasks 1–2 — one reviewer over both: code 0🔴 0🟡 7🔵, spec 0🔴 1🟡, no blocking finding; `cargo make dod` and `cargo deny check` green; `buildl verify --help` lists `--sample <N>` and `[TARGET]...`.
- doc-comment-discipline — the module doc said a failed re-run is worded exactly as `build` words a failed target; the order and lines differ — `report/verify_text.rs:3` — fixed
- consistency — printed bytes go through `from_utf8_lossy` where `build`'s block writes them raw — `report/verify_text.rs:50` — declined: the planned `-> String` signature requires it
- strong-types — stdout and stderr taken as a positional `(&[u8], &[u8])` — `report/verify_text.rs:66` — fixed: takes `buildl_core::Captured`
- correctness — a `Failed` check whose class has no failure text printed `FAILED <label>: ` — `report/verify_text.rs:74` — fixed: no trailing separator; test `a_failure_of_another_class_has_no_trailing_separator`
- strong-types — the summary's counts passed as a positional `[usize; 4]` — `report/verify_text.rs:157` — fixed: a private named `Counts`
- doc-comment-discipline — `--sample` help said "the same N every time" — `buildl-cli/src/cli.rs:178` — fixed: "the same ones while their keys stay the same"; help snapshot updated
- modularity — `SANDBOX_MANIFEST` copied `MANIFEST` by hand; a test rebuilt `common::manifest` — `buildl-cli/src/run.rs:387`, `tests/cli/verify.rs` — fixed: one shared prefix; the bare case uses `common::manifest`
- amendment hygiene — counting an unplaced target under `skipped` was recorded only in rustdoc — task 1 — fixed (plan amended)

After the fix round: `cargo test -p buildl --lib report::` → `100 passed`; `cargo test -p buildl-cli` → 71 unit + 87 binary passed. Plan-wide, with plan 04: `cargo make dod` exit 0, 1622 tests passed, 0 failed, 0 warnings; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

## Probe results

- Structural facts of tasks 1–2 — grep at `922bb89`: `failure_text` `report/block.rs:135`, `header` `:189`, `Paint::failure` `report/paint.rs:40`; `Selection` `buildl-cli/src/cli.rs:105-109`; the `Build` arm `run.rs:106`; `of_class` `exit.rs:47`, `of_error` `:76`, `severity` `:123`; `common::manifest` `tests/cli/common.rs:44` writes `strategy = "bare"`; `workspace`, `program`, `run`, `code`, `stdout`, `stderr` helpers present; the `Ran` harness `run.rs:415` — all present.
- Task 1 seen failing first: 10 tests against a stub, all failed; then green. Task 2's `cli.rs` tests failed first on the missing `Command::Verify`. None came out against the plan.

## Deviations

- 2026-10-21 — task 1: the summary counts an unplaced target under `skipped` (plan amended); five unit tests beyond the five named (bytes/mode, kind, failed block, two-line failure, painting).
- 2026-10-21 — task 2: `exit.rs`'s tests and `of_verification` were written together, with no separate red run; `tests/cli/arguments.rs` also changed (the command-list help snapshots gain the `verify` row).
- 2026-10-21 — tasks 1 and 2 were reviewed by one reviewer pass over both, not one per task.
