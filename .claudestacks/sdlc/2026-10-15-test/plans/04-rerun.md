---
status: approved
created: 2026-10-15
depends-on: [03]
built-on: fefed82cb705
---

# Rerun Implementation Plan

**Goal:** Plan and Execute run a test target again, whatever its key, when the run asks for a
rerun of tests.

**Architecture:** An invocation value, `types::Rerun { Off, Tests }`, reaches `plan::plan` and
`execute::execute`. Each of the two decision sites — `plan/verdict.rs` `decide` and
`execute/settle.rs` `stands` — takes one crate-private value, `types::Forcing { None, Always,
Rerun }`, made per target from its freshness, its role and the run's `Rerun`, in place of the
freshness they read today. `Always` outranks `Rerun`, which outranks the existing rules, so a
rerun test is `Dirty { Rerun }`, never clean or conditional, and its row never stands.
`DirtyReason` gains `Rerun`. `Pipeline::plan` and `Pipeline::build` pass `Rerun::Off`, so no
existing command changes. The types live in `types` because `plan` and `execute` may reach only
`error`, `ports` and `types` (`crates/expected-module-edges.txt`).

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `serde`. Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §3.4, §5, §6 (rows for `plan/verdict.rs`, `execute/settle.rs`,
`types/plan.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits.

Facts the tasks rest on, each read at commit `1799d47`:

| Fact | Where |
|---|---|
| `decide(freshness: Freshness, own, deps, row) -> Result<Verdict>`; computes `key` (finished or unfinished) first, then `if freshness == Freshness::Always` → `Dirty { Always, key }`, no row → `Dirty { NeverBuilt, key }`, then clean / conditional / changed | `crates/buildl-core/src/plan/verdict.rs:52-107` |
| `verdict::all(graph, own, order, row)` calls `decide(node.freshness, components, &deps, row(&node.label))`; `node` is a `Node` with public `role` | `verdict.rs:117-151`; `types/target_graph.rs:33` |
| `verdict.rs` tests call `decide(Freshness::…, …)` directly; `all` is called once in tests through `verdicts_of_chain` | `verdict.rs:209-375`, `:422-430` |
| `plan::plan(graph, overrides, ports) -> Result<Plan>` calls `verdict::all(graph, components, &order, …)` | `crates/buildl-core/src/plan/assembly.rs:63-98` |
| `stands(freshness, key, row) -> bool` is `freshness != Always && digests equal`; `settle(label, freshness, own, deps, cache, cas)` calls it | `crates/buildl-core/src/execute/settle.rs:55-59`, `:122-147` |
| `settle.rs`'s `stands` tests: `a_row_stands_when_it_was_recorded_under_the_targets_key`, `no_row_stands_for_a_target_declared_to_always_run`, `a_row_recorded_under_another_key_does_not_stand` | `settle.rs:233`, `:239`, `:245` |
| `execute(whole, graph, plan, on_failure, live, ports)`; it builds `Build { … }` with no field for the invocation beyond `schedule` and `shared`; `Build::drain` calls `settle(&target.node.label, target.node.freshness, …)` | `crates/buildl-core/src/execute/assembly.rs:99-160`, `:300-334`, `:404-411` |
| `plan` and `execute` are called only by `Pipeline::plan` (`driver.rs:125`) and `Pipeline::build` (`:160-161`) | `grep -rn 'plan(&graph\|execute(&whole' --include='*.rs' crates` |
| `DirtyReason { Always, NeverBuilt, Changed(Vec<Difference>) }`, `Serialize` only; `every_reason_serializes_as_its_name_or_as_its_differences` | `crates/buildl-core/src/types/plan.rs:56-64`, `:450` |
| `buildl`'s `reason_text` matches `DirtyReason` with no wildcard arm | `crates/buildl/src/report/plan_text.rs:38-48` |
| `Freshness { Cached, Always }`, `Copy` | `crates/buildl-core/src/types/declaration.rs:41-47` |
| Module edges: `plan -> error, ports, types`; `execute -> error, ports, types` | `crates/expected-module-edges.txt` |
| A data-only type file ends its module doc with "This file holds only a type definition, so it carries no logic to unit-test." | `crates/buildl-core/src/types/scope.rs:12` |

## Files that change

```
crates/buildl-core/src/types/rerun.rs        — [create] Rerun, a type definition only
crates/buildl-core/src/types/forcing.rs      — [create] Forcing and Forcing::of, with unit tests (Task 2)
crates/buildl-core/src/types/plan.rs         — [modify] DirtyReason::Rerun; its serialization test
crates/buildl-core/src/types/mod.rs          — [modify] declare both; re-export Rerun, crate-re-export Forcing
crates/buildl-core/src/lib.rs                — [modify] Rerun in the root re-exports
crates/buildl/src/report/plan_text.rs        — [modify] the words for DirtyReason::Rerun; a test
crates/buildl-core/src/plan/verdict.rs       — [modify] decide takes Forcing; all takes Rerun; tests
crates/buildl-core/src/plan/assembly.rs      — [modify] plan takes rerun
crates/buildl-core/src/execute/settle.rs     — [modify] stands and settle take Forcing; tests
crates/buildl-core/src/execute/assembly.rs   — [modify] execute takes rerun; Build keeps it; drain forces
crates/buildl-core/src/pipeline/driver.rs    — [modify] plan and build pass Rerun::Off
```

## Order of work

### Task 1 — `Rerun`, and a reason to run again

**Files:**
- Create `crates/buildl-core/src/types/rerun.rs`.
- Modify `crates/buildl-core/src/types/mod.rs` — `pub mod rerun;`, `pub use rerun::Rerun;`; a
  Responsibilities bullet: "`Rerun` — whether a run runs its tests again whatever their keys".
- Modify `crates/buildl-core/src/lib.rs` — `Rerun` in the root list.
- Modify `crates/buildl-core/src/types/plan.rs` — the variant.
- Modify `crates/buildl/src/report/plan_text.rs` — its arm.

**Change:**
- `Rerun` is `Off` or `Tests`, `Debug, Clone, Copy, PartialEq, Eq`, documented as an invocation
  value that is never a key component. `rerun.rs` holds only the type.
- `DirtyReason::Rerun`, after `NeverBuilt`, documented "The run asked for its tests to run again,
  and this target is one." It serializes as `"Rerun"`.
- `reason_text` gives `DirtyReason::Rerun` the words `asked to run again`. `plan` never passes
  `Rerun::Tests`, so `buildl plan` never prints them; the arm exists because the match is
  exhaustive.

**Proof:**
- `a_rerun_reason_serializes_as_rerun` in `types/plan.rs`'s tests — `canonical::to_string(
  &DirtyReason::Rerun)` is `"\"Rerun\""`.
- `a_rerun_reads_as_asked_to_run_again` in `plan_text.rs`'s tests — `reason_text` of
  `DirtyReason::Rerun` with any key is `asked to run again`.

**Done when:** `cargo test -p buildl-core --lib types::plan` and `cargo test -p buildl --lib
report::plan_text` pass and the Definition of Done is green. Commit
`feat(repo): a rerun of tests, and the reason it gives`.

### Task 2 — Plan and Execute run a rerun test again

**Files:**
- Create `crates/buildl-core/src/types/forcing.rs` — `Forcing` and `Forcing::of`, with tests.
- Modify `crates/buildl-core/src/types/mod.rs` — `pub(crate) mod forcing;`,
  `pub(crate) use forcing::Forcing;`; a Responsibilities bullet: "`Forcing` — what makes one
  target run whatever its row says".
- Modify `plan/verdict.rs`, `plan/assembly.rs`, `execute/settle.rs`, `execute/assembly.rs`,
  `pipeline/driver.rs` (all `crates/buildl-core/src`).

`Forcing` is created in this task, not the first, because a crate-private item no code reads yet
fails the gate as dead code.

**Change:**
- `Forcing` is `None`, `Always` or `Rerun`, `pub(crate)`, `Debug, Clone, Copy, PartialEq, Eq`,
  with `pub(crate) const fn of(freshness: Freshness, role: TargetRole, rerun: Rerun) -> Self`:
  `Always` when `freshness` is `Freshness::Always`; else `Rerun` when `role` is `Test` and
  `rerun` is `Rerun::Tests`; else `None`.
- `decide(forcing: Forcing, own, deps, row)`: after the key is computed, `Forcing::Always` →
  `Dirty { Always, key }` as today; `Forcing::Rerun` → `Dirty { Rerun, key }` with the key as
  computed (finished or unfinished), so a rerun target is never `Clean` nor `Conditional`;
  `Forcing::None` → the existing rules. The rustdoc's numbered list gains the rerun rule as its
  second item.
- `verdict::all(graph, own, order, rerun: Rerun, row)` computes
  `Forcing::of(node.freshness, node.role, rerun)` per node.
- `plan::plan(graph, overrides, rerun: Rerun, ports)` passes it to `all`; its rustdoc names it.
- `stands(forcing: Forcing, key, row)`: true only for `Forcing::None` with equal digests.
  `settle(label, forcing, own, deps, cache, cas)` passes it on.
- `execute(whole, graph, plan, on_failure, live, rerun: Rerun, ports)`; `Build` gains a field
  `rerun: Rerun` ("Whether the run's tests run again whatever their rows say"); `Build::drain`
  settles with `Forcing::of(target.node.freshness, target.node.role, self.rerun)`.
- `Pipeline::plan` and `Pipeline::build` pass `Rerun::Off`; their signatures do not change.
- Existing tests that call `decide`, `stands`, `settle`, `all` pass `Forcing::None` for
  `Freshness::Cached`, `Forcing::Always` for `Freshness::Always`, and `Rerun::Off`.

**Proof** (in `forcing.rs`'s tests):
- `always_outranks_a_rerun` — `of(Always, Test, Tests)` is `Always`; `of(Always, Build, Off)` is
  `Always`.
- `a_rerun_forces_only_a_test` — `of(Cached, Test, Tests)` is `Rerun`; `of(Cached, Build, Tests)`
  is `None`.
- `without_a_rerun_nothing_cached_is_forced` — `of(Cached, Test, Off)` and `of(Cached, Build,
  Off)` are `None`.

(in `verdict.rs`'s tests):
- `a_rerun_test_with_an_unchanged_key_is_dirty_for_a_rerun` — `decide(Forcing::Rerun, app(),
  &[], Some(&row(key(app(), &[]), …)))` is `Dirty { Rerun, Finished(key(app(), &[])) }`.
- `a_rerun_test_waiting_on_a_dep_is_dirty_not_conditional` — with one `not_clean` dependency
  and a row, `Forcing::Rerun` is `Dirty { Rerun, Unfinished(…) }` awaiting that dependency.
- `a_rerun_leaves_a_clean_build_target_clean` — `verdicts_of_chain`-style: `all` over the chain
  fixture with every row recorded and `Rerun::Tests` gives every (build-role) target `Clean`.
- `a_test_declared_always_keeps_always_under_a_rerun` — `decide(Forcing::of(Always, Test,
  Tests), …)` is `Dirty { Always, … }`.

(in `settle.rs`'s tests):
- `a_rerun_test_row_does_not_stand` — `stands(Forcing::Rerun, &key, &row)` with equal digests is
  false.
- `a_rerun_build_target_row_stands` — `stands(Forcing::of(Cached, Build, Rerun::Tests), &key,
  &row)` with equal digests is true.

The existing `verdict.rs`, `settle.rs` and flow tests pass unchanged in what they assert.

**Done when:** `cargo test -p buildl-core --lib types::forcing`,
`cargo test -p buildl-core --lib plan::verdict`,
`cargo test -p buildl-core --lib execute::settle` and `cargo test -p buildl-core --test flows`
pass and the Definition of Done is green, `guard-module-edges` included. Commit
`feat(buildl-core): plan and execute run a rerun test again`.

## Risks

- **Two decision sites must agree.** A test the plan calls `Dirty { Rerun }` must not then be
  served from its row at Execute. Both read the same `Forcing::of`, and both sets of tests name
  the rerun case; plan `06`'s flow `a_rerun_runs_a_cached_test_again_and_records_it` shows them
  agreeing end to end.
- **No pipeline reaches `Rerun::Tests` yet.** Until plan `06` adds `Pipeline::test`, the new
  path is exercised by unit tests only.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
