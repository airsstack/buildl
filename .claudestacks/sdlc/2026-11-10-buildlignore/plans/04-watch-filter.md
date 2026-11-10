---
status: done
created: 2026-11-10
depends-on: [03]
built-on: 6643cfeb82a2
---

# Watch Filter Implementation Plan

**Goal:** `watch` starts no cycle for a change under a path the workspace's ignore rules report ignored.

**Architecture:** The `Cycles` port (`crates/buildl-core/src/ports/cycles.rs:17`) gains an
associated type `Ignore: IgnoreRules` and a method returning the rule set of the last cycle that
opened the workspace, `None` before any did. `kept` (`crates/buildl-core/src/watch/react.rs:30`)
takes that `Option<&I>` beside the `WatchLayout` and, after the existing out / state / `.git`
checks, drops a path the rule set reports ignored when asked with `EntryKind::Unknown`; a port
error keeps the path. `react` (`react.rs:51-53`), which calls `kept` itself, gains the same
`Option<&I>` parameter and passes it on. The loop in `watch/run.rs:32-68` passes `cycles`' rule set
at its `kept` call (`:50`) and its `react` call (`:53`). In `buildl`, `Reopening` (`crates/buildl/src/workspace/watching.rs:30,60`) keeps the
`BuildlIgnore` of the workspace it last reopened (the instance plan 03 puts on the wiring) and
returns it.

**Tech Stack:** Rust 1.94, `buildl-core` (no I/O, no threads in tests), `buildl` watch composition;
the end-to-end test drives the watch with the scripted `Changes` the e2e suite already uses
(`crates/buildl/tests/e2e/watch.rs:47-70`).

**Content authority:** `spec.md` §3 (watch filter row, composition row's `Reopening`), §4 (watch
line), §6 (every `watch` row), §8 (`kept` flows, `watch` end to end).

---

## Files that change

```
crates/buildl-core/src/ports/cycles.rs          — modify: type Ignore and the rule-set method on Cycles
crates/buildl-core/src/watch/react.rs           — modify: kept drops ignored paths; react passes the rule set on; their unit tests
crates/buildl-core/src/watch/run.rs             — modify: pass the cycles' rule set to kept (:50); the test Fake (:142-148)
crates/buildl/src/workspace/watching.rs         — modify: Reopening keeps and returns the last BuildlIgnore
crates/buildl/tests/e2e/watch.rs                — modify: an ignored change starts no cycle
```

## Order of work

### Task 1 — Drop ignored paths in kept

**Files:**
- Modify `crates/buildl-core/src/ports/cycles.rs` — `type Ignore: IgnoreRules`;
  `fn rules(&self) -> Option<&Self::Ignore>`, documented as the rule set of the last cycle whose
  workspace opened
- Modify `crates/buildl-core/src/watch/react.rs` — `kept(batch, layout, rules)`; `react` gains
  `rules` and passes it to `kept`; existing `react` tests pass `None`
- Modify `crates/buildl-core/src/watch/run.rs` — the calls at `:50` and `:53`; the test `Fake`
  implements the new method with a test-local rule set
- Modify `crates/buildl/src/workspace/watching.rs` — `Reopening` gains a field holding the last
  reopened workspace's `BuildlIgnore`, set in `build` when the reopen succeeds and left as it was
  when it fails; `rules` returns it

**Change:** a path is kept unless it is under the output or state directory, under `.git`, or the
rule set answers `Ok(Some(_))` for `(path, Unknown)`. `None` rules (no cycle opened yet) drop
nothing by rules. `Err` keeps the path.

**Proof:**
- `an_ignored_path_is_dropped` (`react.rs` tests) — a batch of `notes/a.md` and `src/a.c` with a
  rule set ignoring `notes/a.md` keeps only `src/a.c`.
- `a_batch_of_only_ignored_paths_is_no_change` — a batch of `notes/a.md` alone gives `None`.
- `without_rules_nothing_is_dropped_by_rules` — the same batch with `None` rules keeps `notes/a.md`.
- `a_rule_error_keeps_the_path` — a rule set failing with `IgnoreFile` keeps the path.
- `paths_are_asked_as_unknown` — the test rule set records the kind it was asked with: `Unknown`.
- `the_loop_filters_with_the_last_opened_rules` (`run.rs` tests) — the `Fake` cycles return a rule
  set ignoring `notes/a.md`; a change batch of only that path starts no cycle; one of `src/a.c`
  starts one.

**Done when:** `cargo test -p buildl-core -p buildl --all-targets --all-features` passes and `cargo make dod`
and `cargo deny check -D unused-wrapper` are green. Commit `feat(buildl-core): watch drops changes to ignored paths`.

### Task 2 — Prove it on a real watch

**Files:**
- Modify `crates/buildl/tests/e2e/watch.rs` — the end-to-end case

**Change:** none in source.

**Proof:**
- `a_change_under_an_ignored_folder_starts_no_cycle` — a workspace with `.buildlignore` `notes/`
  and a target reading `src/a.c`; the script delivers a batch of `notes/a.md` after the first cycle,
  then one of `src/a.c`: the report shows exactly two cycles, the second for `src/a.c`.
- `a_changed_buildlignore_takes_effect_next_cycle` — the same workspace; the script rewrites
  `.buildlignore` to add `src/` and delivers a batch of `.buildlignore` (it starts a cycle, the file
  is not ignored), then a batch of `src/a.c`: that last batch starts no cycle.

**Done when:** `cargo test -p buildl --test e2e --all-features watch` passes and `cargo make dod` and `cargo deny check -D unused-wrapper` are green. Commit `test(buildl): watch ignores changes under ignored paths`.

---

## Risks

- The scripted `Changes` delivers batches in order and ends the watch when the script ends, so the
  "no cycle" assertions count cycles in the report and depend on no timing.
- `kept` now does I/O through the port (`symlink_metadata` for `Unknown`, plan 02); it runs between
  cycles, on paths of one event batch, so its cost is one stat per changed path.

## Proof

- `cargo make dod` green after each task; `cargo make guard-module-edges` passes (`watch -> ports`
  and `watch -> types` exist, `crates/expected-module-edges.txt:57,59`).

## Review findings

One reviewer pass over 9c1bb64..08afa55: 4 marked blocking (R1, R2, P1, P2).

- 🟡 R1 `graph()` reopened the workspace but kept the old rules, so a `.buildlignore` edit whose Check found an equal graph left stale rules — fixed in 230f632: `graph()` stores the reopened rules; `a_graph_check_hands_back_the_rules_it_read` failed before (`None` vs `Some(true)`), passes after. `cargo make dod` → green.
- 🟡 R2 keep `layout.config` paths and `.buildlignore` files before asking the rules — declined: spec §6 settles that `watch` asks the rules for every path, a root `buildl.toml` and a `.buildlignore` included ("a change like any other unless a pattern ignores that file").
- 🟡 P1, P2 plan premises disproved by the build — amended below in Deviations.
- 🔵 N1 docs said "last workspace it opened" while `graph()` did not keep its rules — fixed in 230f632 (module, `Reopening` and `Cycles::rules` docs).
- 🔵 N2 `a_cycle_hands_back_the_rules_it_read` passed with the original wiring's rules — fixed in 230f632: the original is asked before `.buildlignore` is written.
- 🔵 N3 each changed path asked twice (loop `kept`, then `react`'s `kept`) — fixed in 230f632: the loop passes `None` to `react`; `each_changed_path_is_asked_of_the_rules_once` failed before, passes after. The Risks note (one stat per changed path) holds.
- 🔵 N4 no loop test for a mixed batch — fixed in 230f632: `the_trigger_names_only_the_paths_the_rules_keep`.

## Deviations

- 2026-11-10 — Task 2 (P1): the plan's fixture cannot prove the filter. With a built graph, a change that is no input only triggers a graph Check; the Check finds an equal graph and starts no cycle, so the test passes with no filter. Built instead: the `chain` fixture with a broken `lib/build.lua`, so no graph loads and every kept change starts a build; `README.md` and `notes/a.md` replace `src/a.c` (the fixture has no `src/`). Both tests fail with the filter disabled.
- 2026-11-10 — Task 2 (P2): `a_changed_buildlignore_takes_effect_next_cycle` starts with no `.buildlignore`, writes `notes/` and delivers `.buildlignore` (a cycle), then `notes/a.md` (no cycle). The plan's "a `.buildlignore` batch starts a cycle" holds only with no loaded graph; with one it is a Check, which is why R1 (rules refreshed by `graph()` too) was needed.
- 2026-11-10 — Task 1: `Reopening` refreshes its rules after every successful reopen, in `build` and in `graph`; a failed reopen leaves them as they were.
- 2026-11-10 — Task 1: buildl-core test rule sets record through `std::sync::Mutex` (`IgnoreRules: Sync`); no threads.
- 2026-11-10 — `book/src/ch10-00-watch.md:152` shows `react(batch, reference)`; plan 06 owns that file and updates it.
- 2026-11-10 — Tasks 1 and 2 ran in one coder with one gate run, committed separately (3b9f39a, 08afa55).
