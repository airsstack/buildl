---
status: done
created: 2026-10-15
depends-on: [01]
built-on: fefed82cb705
---

# A test's outputs Implementation Plan

**Goal:** A test target declares no outputs unless its declaration names some.

**Architecture:** Load's `convert_target` is the one place a missing `outputs` field gets its
default; it now reads the staged role it already carries: `[name]` for a build target, `[]` for a
test. A declared list is kept for either role, so a test that names an output is held to it by
the unchanged output checks of Execute.

**Tech Stack:** Rust 2024 (`rust-version` 1.94). Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §1 P1–P3, §3.1, §6 (rows for `load/traversal.rs` and
`a_test_with_no_outputs_that_exits_zero_is_built`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit, Conventional Commits, scope
`buildl-core`.

Facts the task rests on, each read at commit `1799d47`:

| Fact | Where |
|---|---|
| `convert_target(staged: StagedTarget, site)` maps `staged.outputs`: `Some(written)` parses each, `None` gives `vec![OutputName::parse(label.name().as_str())…]`; the role is copied as `role: staged.role` | `crates/buildl-core/src/load/traversal.rs:183`, `:205-221`, `:228` |
| Test helpers `staged_target(name)` (role `Build`, `outputs: None`), `convert_target_in(directory, target)`, `target_of(declaration)`; the existing `outputs_default_to_the_target_name` | `traversal.rs:323`, `:345`, `:362`, `:429` |
| The flows' `target_with(name, change)` stages a `StagedTarget` with `outputs: None`; `Script.outputs` maps an `OutputPath` to `Content`, and `Content::Missing` leaves nothing at it | `crates/buildl-core/tests/flows/common.rs:1068-1084`, `:543-555`, `:504-508` |
| Three goldens hold `//:app_test`, a test declared without `outputs`: `golden/graph.json` (`"outputs":["app_test"]`), `golden/log.jsonl` line 4 (its `outputs` and `key`), `golden/plan.json` (its components) | `crates/buildl-core/tests/flows/golden/` |
| `a_two_directory_workspace_…` asserts `handed(&pipeline, "//:app_test")`'s fourth list is `texts(&["out/app_test"])` | `crates/buildl-core/tests/flows/build.rs:1162-1169` |
| Plan `01` leaves `build_named(pipeline, labels, on_failure)` in `build.rs` | `plans/01-selection.md`, Task 2 |

## Files that change

```
crates/buildl-core/src/load/traversal.rs         — [modify] the default reads the role; two unit tests
crates/buildl-core/tests/flows/build.rs          — [modify] a flow of a test with no outputs; app_test handed no output
crates/buildl-core/tests/flows/golden/graph.json — [modify] //:app_test's outputs are []
crates/buildl-core/tests/flows/golden/log.jsonl  — [modify] //:app_test's line: outputs [] and its new key
crates/buildl-core/tests/flows/golden/plan.json  — [modify] //:app_test's components: outputs []
```

## Order of work

### Task 1 — A test declares no outputs by default

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs` — the `None` arm; tests.
- Modify `crates/buildl-core/tests/flows/build.rs` — one new flow; one assertion.
- Modify the three goldens above.

**Change:** In `convert_target`, `outputs: None` gives `[name]` when `staged.role` is
`TargetRole::Build` and `[]` when it is `TargetRole::Test`; `Some(list)` is unchanged for both
roles. The `match` on the role is exhaustive (no wildcard), so a future role must choose. The
rustdoc or comment at the default says a test is a record of its exit status and what it
printed, so it produces no file unless it names one.

The three goldens are updated to the bytes the flows now produce. The only differences allowed
are those of `//:app_test`: its `outputs` become `[]` in `graph.json`, in its key's components in
`log.jsonl` (so its key digest there changes too), and in its unfinished components in
`plan.json` (its key there is `null`, as a conditional target's is). Every other target's bytes
stay.
The `handed` assertion for `//:app_test` in the two-directory flow expects no output path.

**Proof** (written first, seen failing, then passing):
- `a_test_declares_no_outputs_by_default` in `traversal.rs` — `staged_target("app_test")` with
  role `Test` converts to a target with no outputs (`assert_eq!` against an empty list).
- `a_test_keeps_the_outputs_it_declares` — the same with `outputs: Some(["junit.xml"])`
  converts to outputs `["junit.xml"]`.
- `outputs_default_to_the_target_name` — existing, unchanged, still passes.
- `a_test_with_no_outputs_that_exits_zero_is_built` in `tests/flows/build.rs` — a workspace of
  `//:check`, a test with no `outputs` field whose script exits 0 and whose `Script.outputs`
  maps `out/check` to `Content::Missing`, built by `build_named(&["//:check"])`, ends `Built`
  with no outputs, and the cache holds a row for it. Before the change this flow fails with
  `OutputMissing`, which is the defect the spec's P2 shows on the real binary.
- `golden/graph.json`'s, `golden/log.jsonl`'s and `golden/plan.json`'s flows pass on the new
  bytes, and `git diff -U0 crates/buildl-core/tests/flows/golden` touches only `//:app_test`'s
  entries.

**Done when:** `cargo test -p buildl-core --lib load::traversal` and
`cargo test -p buildl-core --test flows` pass and the Definition of Done is green. Commit
`fix(buildl-core): a test declares no outputs by default`.

## Risks

- **Pinned bytes move.** The goldens are the contract a reviewer reads; the `git diff -U0` check
  above keeps the change to the one target that should move.
- **Real build files with `b.test` and no `outputs` change key.** Their first build after this
  runs them again, once; nothing else is affected (P3: the log line already allows `[]`).

## Proof

- `cargo make dod` and `cargo deny check` green.

## Review findings

- naming — `outputs_default_to_the_target_name` now covers build targets only, beside `a_test_declares_no_outputs_by_default` — `load/traversal.rs:434` — kept: the task and spec §6 leave it unchanged

## Probe results

- No throwaway probe: the named tests cover the premise. Seen failing first: the default test got `[app_test]`; the new flow ended `OutputMissing { name: "check" }`; the pinned-events flow failed on its `handed` assertion. Goldens: only `//:app_test` changed (graph.json and plan.json outputs `["app_test"]` → `[]`; log.jsonl line 4 key and outputs).

## Deviations

- None.
