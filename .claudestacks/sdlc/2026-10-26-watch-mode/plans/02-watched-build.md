---
status: done
created: 2026-10-26
depends-on: [01]
built-on: a08d695c7505
---

# Watched Build Implementation Plan

**Goal:** `Pipeline` runs a build that also hands back the graph and input set it ran on.

**Architecture:** `Pipeline::build` (`crates/buildl-core/src/pipeline/driver.rs:403-427`)
already holds the whole graph as `Prepared::whole` (`driver.rs:39-44`) and throws it away.
A sibling, `Pipeline::build_watched`, runs the same steps and returns a `Watched` (plan 01,
Task 2): the outcomes or error, plus a `Built` when a graph existed. `Pipeline::build` becomes
`build_watched(…).ended`, so its signature, outcomes and errors stay as they are (spec §4,
"build with graph"). The input set is computed by a crate-private function in `watch/`:
the declared inputs of the scoped graph and of every discovery target of the whole graph,
each with what it needs, since every build runs discovery whatever the scope (spec §3, P17;
`driver.rs:253-313`).

**Tech Stack:** Rust, `buildl-core`, `cargo make dod`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §3 (`built`, `inputs`), §4 ("build with graph"), §8 (core
`pipeline` row), P9, P17.

---

## Files that change

```
crates/buildl-core/src/watch/inputs.rs        — [create] watched_inputs(): the input set of spec §3; unit tests
crates/buildl-core/src/watch/mod.rs           — [modify] declare `inputs` (crate-private)
crates/buildl-core/src/pipeline/driver.rs     — [modify] Pipeline::build_watched; Pipeline::build delegates to it
crates/buildl-core/tests/flows/watch.rs       — [create] flow tests of build_watched against FakePorts
crates/buildl-core/tests/flows/main.rs        — [modify] `mod watch;`
crates/expected-module-edges.txt              — [modify] `pipeline -> watch`, if `cargo make guard-module-edges` reports it
```

---

### Task 1 — The input set

**Files:**
- Create `crates/buildl-core/src/watch/inputs.rs`
- Modify `crates/buildl-core/src/watch/mod.rs`

**Change:** `watched_inputs(whole: &TargetGraph, scoped: &TargetGraph) -> Result<BTreeSet<SourcePath>>`:
every `Node::inputs` (`types/target_graph.rs:46`) of `scoped`, unioned with every input of
`narrow(whole, Scope::Targets(<labels of nodes whose role is TargetRole::Discovery>),
Purpose::Build)` when there is at least one such node. `narrow` is the existing
`resolve::narrow` used at `driver.rs:413`; its closure includes what each target needs.

**Proof:** written first, seen failing, then passing — graphs built through the crate's
existing test helpers for `TargetGraph` (as `plan` and `resolve` unit tests build them):
- `the_scoped_targets_and_what_they_need_give_their_inputs` — `//app` depending on `//lib`: inputs of both
- `a_target_outside_the_scope_gives_nothing` — `//other`'s inputs absent
- `every_discovery_target_gives_its_inputs_whatever_the_scope` — a discovery target outside the scope, its inputs and those of its dependency present
- `no_discovery_target_adds_nothing`

**Done when:** `cargo test -p buildl-core inputs` passes and the stack's Definition of Done is
green. Commit `feat(buildl-core): gather the inputs a watched build reads`.

### Task 2 — `Pipeline::build_watched`

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Create `crates/buildl-core/tests/flows/watch.rs`; modify `crates/buildl-core/tests/flows/main.rs`
- Modify `crates/expected-module-edges.txt` if the guard reports a new edge

**Change:** `Pipeline::build_watched(&mut self, entry, scope, overrides, on_failure, live) ->
Watched`, with `Pipeline::build`'s parameters (`driver.rs:403-410`). Steps as `build` today:
in this order —
1. `prepare`: an `Err` is `Watched { built: None, ended: Err }`; a `ControlFlow::Break` (a
   discovery target failed) is `Watched { built: None, ended: Ok(outcomes) }`.
2. `narrow(&whole, scope, Purpose::Build)`: an `Err` is `Watched { built: None, ended: Err }`.
3. `watched_inputs(&whole, &graph)`: an `Err` is `Watched { built: None, ended: Err }`.
4. `self.run(&whole, &graph, …)` as `build` calls it today; its `Err` or outcomes, after the
   discovery outcomes as `build` extends them, become `ended`.
5. Only then is `whole` moved into `Built { graph: whole, inputs }`, so it is never cloned;
   `built` is `Some` whatever step 4 returned (a refusal or approver no after the graph
   existed keeps the graph, spec §6).

`Pipeline::build` returns `self.build_watched(…).ended` and keeps its docs.

**Proof:** written first, seen failing, then passing — in `tests/flows/watch.rs` over
`FakePorts` (`tests/flows/common.rs:1156`):
- `build_watched_returns_builds_outcomes` — on the fixture `build.rs` flows use, `ended` equals `Pipeline::build`'s outcomes for the same ports state
- `build_watched_returns_the_whole_graph` — `built.graph` equals what `GraphRecord` recorded last
- `a_load_failure_has_no_graph` — an evaluation error gives `built: None` and `ended: Err(Evaluation)`
- `a_failed_discovery_target_has_no_graph` — mirrors the discovery flow's failing case: `built: None`, `ended: Ok` with the discovery outcomes
- `a_refusal_keeps_the_graph` — a want outside the ceiling: `ended: Err(Refused)`, `built: Some`
- `an_unknown_scope_label_has_no_graph` — `ended: Err(UnknownTarget)`, `built: None`
- the existing `tests/flows/build.rs` and `discovery.rs` pass unchanged (the delegation changes nothing)

**Done when:** `cargo test -p buildl-core --test flows` passes and the stack's Definition of
Done is green. Commit `feat(buildl-core): hand back the graph a build ran on`.

---

## Risks

- Most risky step: Task 2's reshaping of `build`. Every existing flow test of `build` is the
  guard; none may change.
- The plan chose not to touch `Pipeline::test` or `verify`: watch builds only (intent).
- Whether `FakePorts` can stage a refusal and a failing discovery target in one file is for
  the coder to read in `tests/flows/build.rs` and `discovery.rs`, which already do each.

## Proof

`cargo make dod` green; `cargo test -p buildl-core` covers spec §8's core `pipeline` row.

## Review findings

- test-coverage — the call `watched_inputs(&whole, &graph)` in `Pipeline::build_watched` was unguarded: swapping or doubling its two `&TargetGraph` arguments would drop an out-of-scope discovery target's inputs, and no test failed (spec §8 core `pipeline` row) — `crates/buildl-core/tests/flows/watch.rs`. Fixed and verified: `a_discovery_target_outside_the_scope_still_has_its_inputs_watched` added; with the call changed to `watched_inputs(&graph, &graph)` it failed (left `{main.c}`, right `{main.c, pkgs.in}`), restored it passes; `cargo test -p buildl-core --test flows` 281 passed, clippy `-p buildl-core -D warnings` clean (fix coder, run once).

## Probe results

- Every asserted fact was exercised by the task's own named tests, red then green (`inputs` unit tests against an empty-set stub; flow tests before `build_watched` existed). None came out against the plan. The existing `tests/flows/build.rs` and `discovery.rs` pass unchanged (280 flows before the fix round).

## Deviations

- 2026-10-26 — Task 1: `watched_inputs` carried `#[cfg_attr(not(test), expect(dead_code, …))]` until Task 2 called it; Task 2 removed it.
- 2026-10-26 — Task 1: the `inputs` unit tests build graphs with `crate::grants::fixtures` (as `plan/` tests do), since `resolve::fixtures` is private; this adds the module edge `watch -> grants` beside the real `watch -> resolve` (`narrow`) in `crates/expected-module-edges.txt`.
- 2026-10-26 — Task 2: `watch/mod.rs` gained `pub(crate) use inputs::watched_inputs;` (not in the task's file list) so the driver can import it; module edge `pipeline -> watch` added.
- 2026-10-26 — Task 2 added a seventh flow test, `the_inputs_are_those_of_the_scope_and_what_it_needs`, beyond the six named.
- 2026-10-26 — Task 2's gate ran scoped to `buildl-core` while another coder edited `crates/buildl`; the workspace-wide gate runs once at the end of the run.
