---
status: done
created: 2026-10-19
depends-on: [01]
built-on: 62323287dd81
---

# Parallel Load Proof Implementation Plan

**Goal:** Tests prove the real `ThreadPool` overlaps build-file evaluations while results stay identical across worker counts.

**Architecture:** Tests only, in `buildl`'s e2e test binary, which can reach `ThreadPool`
(`buildl::dispatch::ThreadPool`, re-exported in `dispatch/mod.rs`), `Wiring` (re-exported from
`buildl/src/lib.rs`, defined in `local_ports.rs`) and
`buildl_core::load`. A new fixture `fanned` gives a workspace of several directories. `buildl-core`
starts no thread, so overlap can only be shown here (spec §4).

**Tech Stack:** Rust tests (`std::sync::{Condvar, Mutex}`), `cargo make dod`.

**Content authority:** spec §4 rows `buildl` adapter and e2e via `Workspace`.

---

## Files that change

```
crates/buildl/tests/fixtures/fanned/buildl.toml          — [create] manifest, `workers  = 8`
crates/buildl/tests/fixtures/fanned/build.lua            — [create] root: `subdir`s a, b, c; one target
crates/buildl/tests/fixtures/fanned/in.txt               — [create] the root target's input
crates/buildl/tests/fixtures/fanned/{a,b,c,a/x,b/y}/build.lua — [create] one `cp` target each
crates/buildl/tests/fixtures/fanned/{a,b,c,a/x,b/y}/in.txt    — [create] each target's input
crates/buildl/tests/e2e/parallel_load.rs                 — [create] the tests below
crates/buildl/tests/e2e/main.rs                          — [modify] `mod parallel_load;`
```

## Order of work

### Task 1 — Prove two evaluations overlap on `ThreadPool`

**Files:**
- Create `crates/buildl/tests/e2e/parallel_load.rs`
- Modify `crates/buildl/tests/e2e/main.rs` — `mod parallel_load;`

**Change:** none in library code.

**Proof:**
- `two_sibling_build_files_are_evaluated_at_once` — a test `DeclarationSource` whose root names
  subdirs `a` and `b`; evaluating `a` or `b` increments a `Mutex<usize>` and waits on a `Condvar`
  until it reads 2, with a 10 s `wait_timeout`; a timeout returns an evaluation error. `load` over
  `ThreadPool::new(Workers::of(2))` (`types/workers.rs:48`) returns `Ok` — possible only if both
  evaluations were under way together.
- `one_worker_never_overlaps` — the same source with a 0.2 s timeout under `Workers::of(1)`:
  `load` fails with the timeout's evaluation error (the control that shows the test can fail).

**Done when:** `cargo test -p buildl --test e2e parallel_load` passes and `cargo make dod` is
green. Commit `test(buildl): prove build files evaluate at once on the thread pool`.

### Task 2 — Prove identical results at 1, 2 and 8 workers

**Files:**
- Create the `fanned` fixture (files listed above): `buildl.toml` copied from
  `crates/buildl/tests/fixtures/logged/buildl.toml` with `name = "fanned"` and `workers  = 8` (the `replace_line` key
  form `identical.rs:20` uses); root `build.lua` with `buildl.subdir` for `a`, `b`, `c`; `a`
  names `x`, `b` names `y`; each file, root included, one target `run = { "cp", "$in", "$out" }` over its own `in.txt`
- Modify `crates/buildl/tests/e2e/parallel_load.rs`

**Change:** none in library code.

**Proof:**
- `real_build_files_load_the_same_at_1_2_and_8_workers` — `Wiring::read(fanned).source()` loaded
  through `ThreadPool` at `Workers::of(1)`, `of(2)`, `of(8)`: the three `Vec<Declaration>` are equal.
- `graph_json_is_byte_identical_at_1_and_8_workers` — two copies (`Fixture::copy_of("fanned", &[])`, `tests/e2e/common.rs:56`),
  one with `replace_line("buildl.toml", "workers  = 8", "workers  = 1")`; `build` each; the two
  `.buildl/graph.json` files are equal byte for byte.
- `two_broken_build_files_report_the_same_error_at_1_and_8_workers` — in both copies, break
  `a/x/build.lua` and `c/build.lua` (`append` a Lua syntax error); `Workspace::check` at each count
  returns errors whose `to_string()` is equal, and names `c/build.lua` (first in serial queue
  order: root, a, b, c, x, y).

**Done when:** `cargo test -p buildl --test e2e parallel_load` passes and `cargo make dod` is
green. Commit `test(buildl): prove parallel load matches serial on a real workspace`.

---

## Risks

- **Serial queue order** decides which broken file is reported: root → a, b, c → a/x, b/y
  (breadth-first, `load` in `traversal.rs` and its `Loading` replay). If execute finds the order differs, the expected name in
  the third test follows the code, and the plan's Deviations say so.
- **Timing-based tests:** the overlap test fails only on a real failure to overlap (10 s); the
  control uses a short timeout and cannot pass by luck under one worker.
- Not done here: documents (plan 03).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo test -p buildl --test e2e parallel_load` run three times passes each time.

## Review findings

One reviewer pass over `855ef5b..fa65497`: spec compliant, DoD green, `parallel_load` ×3 green;
no blocking finding. Each test shown to prove its claim (one worker cannot overlap: `serve` hands
work only to a free worker; `Workers::of(2)` is not capped to the core count).

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | 🔵 | module doc claimed worker count "changes how long a load takes" — nothing measures time | fixed: "changes nothing a load returns" |
| 2 | 🔵 | `loaded_by` doc named `fanned`; `Declaration` spelled by full path | fixed |
| 3 | 🔵 | 1/2/8-worker equality on six small files rarely sees out-of-order arrival | declined — what spec §4 asks; the core order×pace matrix is the guard |
| 4–6 | 🔵 | plan citations `dispatch/mod.rs:17`, `lib.rs:167`, `traversal.rs:44-73` off by a line or imprecise | fixed in this plan: cite by symbol |

## Probe results

- Serial queue order (Risks): with `a/x/build.lua` and `c/build.lua` broken, 1 and 8 workers both
  report `c/build.lua` — order root, a, b, c, a/x, b/y holds. The test is the probe.
- The 1-worker control fails with the timeout's evaluation error on directory `a`, as planned.

## Deviations

- Briefs pointed each coder at its task's text in this plan rather than through a task-briefer
  ledger; tasks ran beside the clean-gc chain's plan 03 (disjoint files).
