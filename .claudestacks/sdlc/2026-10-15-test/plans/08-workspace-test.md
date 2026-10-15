---
status: approved
created: 2026-10-15
depends-on: [06, 07]
built-on: fefed82cb705
---

# `Workspace::test` Implementation Plan

**Goal:** The `buildl` facade runs a test run on a real workspace, with `build`'s lock and fold.

**Architecture:** `Workspace::test` is `Workspace::build` with `Pipeline::test` in place of
`Pipeline::build` and one more argument, `rerun`: the same `runs()` check, lock, adapters, fold
and `concluded`. A new fixture of real `cp` and `cmp` commands proves it end to end, including
the two behaviours the fakes hid: a test that writes no file passes, and a failed rerun drops the
pass it contradicts so the next run runs it.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `tempfile` (tests). Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §1 P2–P4, §3.8 (second bullet), §6 (rows for `crates/buildl/tests/e2e/test.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl`.

Facts the tasks rest on, each read at commit `1799d47` or left by an earlier plan:

| Fact | Where |
|---|---|
| `Workspace::build<R: Reporter>(&self, scope, overrides, on_failure, live, reporter)`: `self.wiring.runs()?`, `lock()`, `Pipeline::new(self.wiring.ports(&lock, reporter)?)`, `pipeline.build(…)`, `pipeline.ports_mut().cache.fold()`, `drop(pipeline)`, `concluded(built, folded)`; its rustdoc's `# Errors` | `crates/buildl/src/workspace.rs:141-172` |
| `workspace.rs`'s struct doc says the state directory is taken by `plan` and `build` only | `workspace.rs:24-30` |
| `workspace.rs`'s tests: helper `copying()` (`:234`), `a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal` (`:620`) | `workspace.rs:215-720` |
| `Pipeline::test(entry, scope, overrides, on_failure, live, rerun)` | `plans/06-pipeline-test.md` Task 1 |
| e2e harness: `Fixture::copy_of(name, executables)`, `root()`, `open()`, `build()`, `build_targets(targets)`, `rows()`, `log_lines()`; `ended(outcomes, class)` | `crates/buildl/tests/e2e/common.rs:56`, `:75`, `:179-222`, `:151`, `:137`, `:285` |
| e2e modules are declared in `tests/e2e/main.rs`: `common, facade, failure, identical, incremental, named, recovery` | `crates/buildl/tests/e2e/main.rs` |
| Fixtures live in `crates/buildl/tests/fixtures/<name>/`; `failing/buildl.toml` is the smallest manifest (`proc.run = ["cp", "false"]`, `fs.read = ["."]`, `fs.write = ["out", ".buildl"]`, `workers = 1`, `strategy = "bare"`) | `crates/buildl/tests/fixtures/failing/buildl.toml` |
| The bare strategy runs an action with the workspace root as its working directory, so a relative path argument names a workspace file | `crates/buildl/src/exec/bare.rs:102` |
| On the real binary (spec P2, P3): a `cmp` test of equal files with `outputs = {}` passes and is cached next run; a `cmp` of different files prints `… differ: char 1, line 1` and exits 1 | `.claudestacks/sdlc/2026-10-15-test/spec.md:23-24` |

## Files that change

```
crates/buildl/src/workspace.rs                    — [modify] Workspace::test; struct doc; a unit test
crates/buildl/tests/fixtures/tested/buildl.toml   — [create] cp and cmp, one worker
crates/buildl/tests/fixtures/tested/build.lua     — [create] a copy target and four tests
crates/buildl/tests/fixtures/tested/a.txt         — [create] "a\n"
crates/buildl/tests/fixtures/tested/expected.txt  — [create] "a\n"
crates/buildl/tests/fixtures/tested/other.txt     — [create] "b\n"
crates/buildl/tests/fixtures/tested/flag.txt      — [create] "a\n", read by a test that does not declare it
crates/buildl/tests/e2e/common.rs                 — [modify] Fixture::test
crates/buildl/tests/e2e/test.rs                   — [create] the e2e tests of a test run
crates/buildl/tests/e2e/main.rs                   — [modify] declare mod test
```

## Order of work

### Task 1 — `Workspace::test`

**Files:** `crates/buildl/src/workspace.rs`.

**Change:** `pub fn test<R: Reporter>(&self, scope: &Scope, overrides: &BTreeMap<SettingName,
SettingValue>, on_failure: OnFailure, live: LiveOutput, rerun: Rerun, reporter: &mut R) ->
Result<Vec<ActionOutcome>>`, the body of `build` with `pipeline.test(self.wiring.entry(), scope,
overrides, on_failure, live, rerun)`. Its rustdoc: runs the tests `scope` asks for, every test
when it asks for the workspace, telling `reporter` how it goes; the same lock and fold as
`build`; `# Errors` as `build`'s, with `Pipeline::test`'s in place of `Pipeline::build`'s. The
struct doc names `test` beside `plan` and `build` as taking the state directory.

**Proof:**
- `a_test_run_folds_its_rows_and_runs_no_build_target_of_its_own` in `workspace.rs`'s tests —
  a tempdir workspace (as `copying()` makes one) holding `//:copy` (`cp a.txt`), `//:spare`
  (another `cp` target) and `//:check` (`b.test`, no `outputs`, `cmp` of `//:copy`'s output
  against `a.txt`): a test run of the workspace gives `//:copy` and `//:check` `Built` and
  `//:spare` no outcome; `cache.json` holds the two rows and no `cache.journal` is left.

**Done when:** `cargo test -p buildl --lib workspace::` passes and the Definition of Done is green.
Commit `feat(buildl): a workspace runs its tests`.

### Task 2 — A test run on real commands

**Files:** the fixture files, `tests/e2e/common.rs`, `tests/e2e/test.rs`, `tests/e2e/main.rs`.

**Change:**
- The fixture `tested`: `buildl.toml` as `failing`'s with `name = "tested"` and
  `proc.run = ["cp", "cmp"]`. `build.lua`:
  - `b.target("copy", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })`
  - `b.test("same", { deps = { "copy" }, inputs = { "expected.txt" }, run = { "cmp", "$deps", "$in" } })` — passes, writes no file
  - `b.test("differs", { deps = { "copy" }, inputs = { "other.txt" }, run = { "cmp", "$deps", "$in" } })` — fails
  - `b.test("unreported", { deps = { "copy" }, inputs = { "expected.txt" }, outputs = { "report.txt" }, run = { "cmp", "$deps", "$in" } })` — exits 0 and leaves no `report.txt`
  - `b.test("flagged", { deps = { "copy" }, run = { "cmp", "$deps", "flag.txt" } })` — reads `flag.txt` without declaring it, so editing it does not change its key (tier 1, spec non-goals)
- `Fixture::test(&self, targets: &[&str], rerun: Rerun) -> Vec<ActionOutcome>`: a test run with
  `Silent`, `OnFailure::KeepGoing`, `LiveOutput::Off`, of the named targets (read with
  `workspace.label`) or the whole workspace when `targets` is empty; unwraps.
- `test.rs`'s module doc says what the fixture holds and why each test is there.

**Proof** (in `tests/e2e/test.rs`):
- `a_passing_test_with_no_outputs_passes_and_is_cached_next_run` — `test(&["same"], Off)`:
  `//:same` `Built`; again: `//:same` `Cached`.
- `a_failing_test_is_never_cached` — `test(&["differs"], Off)` twice: `//:differs` is
  `ActionFailure { status: 1 }` both times, and `rows()` has no `//:differs`.
- `a_test_that_declares_an_output_is_held_to_it` — `test(&["unreported"], Off)`: `//:unreported`
  is `OutputMissing` naming `report.txt`.
- `a_build_of_the_workspace_runs_no_test` — `build()`: the outcomes are `//:copy` alone, and the
  log holds one line.
- `a_failed_rerun_runs_again_next_time` — `test(&["flagged"], Off)`: `Built`; `flag.txt`
  rewritten to `b\n`; `test(&["flagged"], Off)`: `Cached` (its key did not change);
  `test(&["flagged"], Tests)`: `ActionFailure { status: 1 }` and `rows()` has no `//:flagged`;
  `test(&["flagged"], Off)`: `ActionFailure { status: 1 }` again — it ran, it was not served
  from a row.

**Done when:** `cargo test -p buildl --test e2e test::` passes and the Definition of Done is
green. Commit `test(buildl): a test run on real commands`.

## Risks

- **`flag.txt` is read by a relative path**, which holds because the bare strategy runs an
  action in the workspace root (`bare.rs:102`); the first run of
  `a_failed_rerun_runs_again_next_time` passing is the check.
- **Real `cmp` wording** is not asserted; only exit statuses and classes are.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
