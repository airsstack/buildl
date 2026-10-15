---
status: done
created: 2026-10-15
depends-on: [05]
built-on: fefed82cb705
---

# `Pipeline::test` Implementation Plan

**Goal:** `buildl-core`'s pipeline runs a test run: the selected tests and what they need, through
every phase, against any ports.

**Architecture:** `Pipeline::test` sits beside `Pipeline::build` in `pipeline/driver.rs` and is
`build`'s sequence with two differences: it narrows with `Purpose::Test` (plan `01`) and hands
`rerun` to Plan and Execute (plan `04`). No free `pipeline::test` is added: unlike `check`,
`graph` and `query`, it needs every port. The new flows run it end to end over the fakes.

**Tech Stack:** Rust 2024 (`rust-version` 1.94). Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §2 (Selection, Pipeline, Failure policy, Caching), §3.8 (first
bullet), §6 (row for `tests/flows/test.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl-core`.

Facts the tasks rest on, each read at commit `1799d47` or left by an earlier plan:

| Fact | Where |
|---|---|
| `Pipeline::build(entry, scope, overrides, on_failure, live)`: `resolve(load(…))`, `self.ports.graph.record(&whole)`, `narrow`, `plan`, `execute(&whole, &graph, &plan, …)`; its rustdoc's `# Errors` order | `crates/buildl-core/src/pipeline/driver.rs:130-162` |
| The driver's module doc lists its Responsibilities by method | `driver.rs:1-12` |
| After plans `01`–`05`: `narrow(graph, scope, purpose)`, `plan(graph, overrides, rerun, ports)`, `execute(whole, graph, plan, on_failure, live, rerun, ports)` | `plans/01-selection.md` Task 2; `plans/04-rerun.md` Task 2 |
| Flows: `tests/flows/main.rs` declares one module per flow file; `build.rs` has private helpers `root_only(items)` (`:287`, the fakes with `cc` fingerprinted), `script(pipeline, raw, script)` (`:476`), `ending(ends)` (`:468`), `skipped(raw)` (`:456`), `classes(outcomes)` (`:271`) and `expected(pairs)` (`:279`), which compare outcomes by label | `crates/buildl-core/tests/flows/main.rs`; `build.rs` as cited |
| The stop and keep-going flows of `build.rs` use `failing()` (`:491`), where `//:a` exits 1 and two targets stand apart; under `Stop` the two are skipped naming `//:a` and only `//:a` ran | `build.rs:491-570` |
| Fakes: `FakeStrategy::ran()`, `runs(label)`, `scripts`; `FakeCache::rows`, `recorded()` and (plan `05`) `forgotten()`; `FakeGraphRecord::recorded` holds each recorded graph's canonical bytes; `FakeLog::events` | `crates/buildl-core/tests/flows/common.rs:567-600`, `:268-284`, `:854-859`, `:435-446` |
| `target_with(name, change)` stages a target; setting `role = TargetRole::Test` makes it a test | `common.rs:1068`; `build.rs:808-818` |

## Files that change

```
crates/buildl-core/src/pipeline/driver.rs     — [modify] Pipeline::test; module doc
crates/buildl-core/tests/flows/common.rs      — [modify] receives the helpers build.rs and test.rs share
crates/buildl-core/tests/flows/build.rs       — [modify] imports those helpers from common
crates/buildl-core/tests/flows/test.rs        — [create] the test-run flows
crates/buildl-core/tests/flows/main.rs        — [modify] declare mod test
```

## Order of work

### Task 1 — `Pipeline::test`, proven end to end over the fakes

**Files:** as above.

**Change:**
- `pub fn test(&mut self, entry: &EntryName, scope: &Scope, overrides: &BTreeMap<SettingName,
  SettingValue>, on_failure: OnFailure, live: LiveOutput, rerun: Rerun) ->
  Result<Vec<ActionOutcome>>`: load once, resolve, record the whole graph through the
  graph-record port, `narrow(&whole, scope, Purpose::Test)`, `plan(&graph, overrides, rerun,
  …)`, `execute(&whole, &graph, &plan, on_failure, live, rerun, …)`. Its rustdoc follows
  `build`'s: what a whole workspace means here (every test, with what each needs), what `rerun`
  does (the selected tests run whatever their keys; their dependencies do not), and an `# Errors`
  section equal to `build`'s with `Error::NotATest` beside `Error::UnknownTarget`.
- The module doc's Responsibilities line names `Pipeline::test`.
- `root_only`, `script`, `ending`, `skipped`, `classes` and `expected` move from `build.rs` to
  `common.rs` as `pub` items, bodies unchanged, together with the `label(raw)` helper
  (`build.rs:35`) that `script` and `skipped` call; `build.rs` imports all seven.
- `test.rs` holds a fixture `suite()`, built with `root_only`: `//:app` and `//:docs` (build
  targets, `cc`), `//:app_test` (a test depending on `//:app`, `cc`), `//:unit` (a test with no
  dependency, `cc`); and a helper `tested(pipeline, scope, on_failure, rerun)` calling
  `Pipeline::test` with no override and `LiveOutput::Off`.

**Proof** (in `tests/flows/test.rs`; each written first and seen failing to compile, then passing):
- `a_test_run_records_the_whole_graph_and_runs_only_tests` — a test run of `Scope::Workspace`
  has outcomes for `//:app`, `//:app_test`, `//:unit` only; `strategy.ran()` holds no `//:docs`;
  the one recorded graph holds `//:docs`.
- `a_test_run_returns_each_outcome_with_its_role` — the same run's outcomes have roles `Build`,
  `Test`, `Test`, in label order.
- `a_test_run_keeps_going_past_a_failing_test` — `//:unit` scripted to exit 1, `KeepGoing`:
  `//:unit` is `ActionFailure { status: 1 }`, `//:app_test` is `Built`.
- `a_stopped_test_run_starts_nothing_new_after_a_failure` — `//:app` scripted to exit 1, `Stop`:
  `//:app_test` and `//:unit` are skipped naming `//:app`, and `strategy.ran()` is `[//:app]`.
- `a_failed_dependency_skips_the_tests_above_it` — `//:app` scripted to exit 1, `KeepGoing`:
  `//:app_test` is skipped naming `//:app`, `//:unit` is `Built`.
- `a_rerun_runs_a_cached_test_again_and_records_it` — three runs of the workspace: `Off`, `Off`
  (every outcome `Cached`), `Rerun::Tests`: `//:app_test` and `//:unit` are `Built`, each ran
  twice, each was recorded twice, and the third run's log events for them carry the key digest
  of the first run's (a rerun is not a key component).
- `a_rerun_does_not_rebuild_a_cached_dependency` — in the same three runs `//:app` is `Cached`
  in the third and ran once.
- `a_failed_rerun_forgets_the_row_so_the_next_run_runs_it` — one passing run; `//:unit` then
  scripted to exit 1; a `Rerun::Tests` run: it fails, `cache.forgotten()` is `[//:unit]`, it has
  no row; its script back to exit 0, a run with `Off`: it is `Built`, not `Cached`.
- Every existing flow of `build.rs` passes with the helpers moved.

**Done when:** `cargo test -p buildl-core --test flows` passes and the Definition of Done is
green. Commit `feat(buildl-core): a pipeline that runs tests`.

## Risks

- **Moving shared helpers** can change nothing but their visibility; the full `build.rs` run is
  the check.
- **The stop flow depends on the fake dispatcher's order**, as `build.rs`'s
  `under_stop_nothing_begins_after_the_first_failure` does: the fixture puts the failing
  `//:app` first in label order, the same arrangement.

## Proof

- `cargo make dod` and `cargo deny check` green.

## Review findings

- nit — the driver's module doc ran past 100 columns — `pipeline/driver.rs` — fixed: rewrapped
- nit — `test`'s rustdoc said "that part" with no antecedent, and dropped two of `build`'s clauses (a left-alone target keeps its row and outputs; the record never depends on the selection) — `pipeline/driver.rs` — fixed
- nit — "a rerun is not a key component" compared key digests that would pass if both were `None` — `tests/flows/test.rs` — fixed: `three_runs` asserts the first run's digests are present
- nit — the helper move changed more than visibility (see Deviations) — kept: recorded here

## Probe results

- No throwaway probe: every claim is under a named flow. Reversion check: `Rerun::Off` passed to `execute` inside `Pipeline::test` turned `a_rerun_runs_a_cached_test_again_and_records_it` and `a_failed_rerun_forgets_the_row_so_the_next_run_runs_it` red; restored. This pins plan `04`'s execute-side wiring.

## Deviations

- The seven helpers moved to `common.rs` with two changes besides visibility, both forced by the lints on non-test code there: `label` reads with `unwrap_or_else(|e| unreachable!(…))` and a `# Panics` doc, as the file's other fixtures do, and the moved functions that return a value carry `#[must_use]`. They sit before `common.rs`'s test module (`items_after_test_module`).
