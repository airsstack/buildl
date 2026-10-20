---
status: executing
created: 2026-10-19
built-on: 62323287dd81
---

# Load Through the Dispatcher Implementation Plan

**Goal:** Every loading command evaluates build files through the `Dispatcher` port, returning serial load's declarations or error.

**Architecture:** `load` gains a `dispatcher: &mut D` and runs its directory queue through
`Dispatcher::dispatch`: the work function (`load/evaluation.rs`) evaluates one file on any thread;
the step (`load/loading.rs`, struct `Loading`) runs on the calling thread, hands out each new
subdirectory as soon as a result names it, and replays the serial loop over the results to decide
the declarations and the error. A shared `AtomicBool` stops new evaluations once the replay meets
its error. Every caller in the workspace passes a dispatcher: `FakeDispatcher` in core tests,
`ThreadPool::new(execution.workers)` through a new `Wiring::dispatcher()` in `buildl`, a one-thread
test dispatcher in `buildl-lua`'s tests.

**Tech Stack:** Rust (`buildl-core`, `buildl`, `buildl-cli`, `buildl-lua` tests), std `AtomicBool`
and `Mutex`, `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2, §3.1–§3.4, §4 rows `Loading` unit, `evaluation` unit,
`buildl-core` flows.

**Order with the sibling chain:** this branch executes the clean-gc chain first; Task 5 changes
`pipeline::gc` and `Workspace::gc`, which that chain's plans 01 and 02 create. Start this plan only
after those are `done`.

---

## Files that change

```
crates/buildl-core/src/ports/declaration_source.rs  — [modify] `Sync` supertrait and its reason
crates/buildl-core/tests/flows/common.rs            — [modify] `FakeSource` on `Mutex`; `common.rs:1411` passes a dispatcher
crates/buildl-core/src/error.rs                     — [modify] `Error::LostBuildFiles`, its message, unit test
crates/buildl-core/src/ports/dispatcher.rs          — [modify] contract doc names `LostBuildFiles` beside `DispatcherFault`
crates/buildl-cli/src/exit.rs                       — [modify] `LostBuildFiles` in the `DispatcherFault` arm; unit test
crates/buildl-core/src/load/evaluation.rs           — [create] `FileResult` and the work function, unit tests
crates/buildl-core/src/load/loading.rs              — [create] `Loading` (hand-out and replay), unit tests
crates/buildl-core/src/load/traversal.rs            — [modify] `load` over the dispatcher; serial loop body moves to the two files above
crates/buildl-core/src/load/mod.rs                  — [modify] declare `evaluation`, `loading`
crates/buildl-core/src/pipeline/driver.rs           — [modify] callers of `load`; `check/graph/query` take a dispatcher; docs
crates/buildl-core/src/pipeline/gc.rs               — [modify] `gc` takes a dispatcher for its load
crates/buildl-core/tests/flows/load.rs              — [modify] callers; order × pace matrix; stop; lost task
crates/buildl-core/tests/flows/{check,graph,query,gc}.rs — [modify] free-function callers pass a `FakeDispatcher`
crates/buildl/src/local_ports.rs                    — [modify] `Wiring::dispatcher()`; `ports` uses it
crates/buildl/src/workspace.rs                      — [modify] `check/graph/query/gc` pass `&mut self.wiring.dispatcher()`
crates/buildl-lua/tests/flows/common.rs             — [modify] `InlineDispatcher`, test-only
crates/buildl-lua/tests/flows/load.rs               — [modify] `:59` passes it
```

## Order of work

### Task 1 — Require `Sync` of a declaration source

**Files:**
- Modify `crates/buildl-core/src/ports/declaration_source.rs` — `pub trait DeclarationSource: Sync`
  (`:58`), with a doc paragraph in the form `ContentStore` gives its own (`content_store.rs:22-24`):
  several build files are evaluated at once, each on the thread that took it
- Modify `crates/buildl-core/tests/flows/common.rs` — `FakeSource` (`:43-48`): `root_evaluations`
  and `evaluated` move from `Cell`/`RefCell` to `Mutex` (or `AtomicU32` for the counter);
  `evaluated()` keeps its signature

- Modify `crates/buildl-core/src/pipeline/gc.rs` (created by the clean-gc chain) — its in-file
  test source must be `Sync`: a `Cell`/`RefCell` in it moves to `Mutex`

**Change:** the trait bound only. The other impls are unit structs (spec P14: `bundle.rs:49`,
`port_set.rs:116`, `declaration_source.rs:36`, `plan_text.rs:230`) and `LuaSource` is asserted
`Send + Sync` (`buildl-lua/src/source.rs:249-250`), so they compile unchanged.

**Proof:**
- Every existing test passes unchanged: `cargo test --workspace` — `FakeSource`'s behaviour
  (`with_second_run`, `evaluated`) is exercised by `tests/flows/check.rs` and `load.rs`.
- `declaration_source.rs` stays a trait-only file; its unit-test status is unchanged.

**Done when:** `cargo make dod` is green. Commit `refactor(buildl-core): require a Sync declaration source`.

### Task 2 — Add `Error::LostBuildFiles`

**Files:**
- Modify `crates/buildl-core/src/error.rs` — variant after `DispatcherFault` (`:373-376`):
  `LostBuildFiles { directories: Vec<Directory> }`, doc "never empty, in directory order", message
  `dispatcher: returned without evaluating the build files of {dirs}` (spec §3.3), formatted by a
  helper beside `lost_by_the_dispatcher`
- Modify `crates/buildl-core/src/ports/dispatcher.rs` — the contract doc (`:38-40`): a build's
  lost task fails with `DispatcherFault`, a load's with `LostBuildFiles`
- Modify `crates/buildl-cli/src/exit.rs` — `| Error::LostBuildFiles { .. }` beside
  `Error::DispatcherFault { .. }` (`:114`), so it is listed rather than caught by `_`

**Change:** the error exists and exits as `DispatcherFault` does (`Internal`).

**Proof:**
- `error.rs` unit test `lost_build_files_names_each_directory` — two directories (root and `lib`)
  render in the spec's message, in order, root shown the way `MissingBuildFile` shows it.
- `exit.rs` unit test `a_lost_build_file_is_an_internal_fault` — `Exit::of_error` gives the same
  exit as for `DispatcherFault`.

**Done when:** `cargo test -p buildl-core -p buildl-cli` passes and `cargo make dod` is green.
Commit `feat(buildl-core): name build files a dispatcher lost`.

### Task 3 — Add the work function in `load/evaluation.rs`

**Files:**
- Create `crates/buildl-core/src/load/evaluation.rs` — `pub(crate) enum FileResult { Stopped,
  Absent, Failed(Error), Loaded { subdirs: Vec<Directory>, declarations: Vec<Declaration> } }` and
  `pub(crate) fn evaluate_file<S: DeclarationSource>(source, entry, directory, stop: &AtomicBool) -> FileResult`
- Modify `crates/buildl-core/src/load/traversal.rs` — the per-file part of the serial loop
  (`:44-73`: evaluate, `resolve_subdirs` `:120-134`, convert) is called from here, not duplicated:
  the private helpers `build_file` (`:76`), `resolve_subdirs` (`:120`) and `convert` (`:139`) become
  `pub(super)` and stay in `traversal.rs` with their unit tests (`mod tests`, `:302`)
- Modify `crates/buildl-core/src/load/mod.rs` — `mod evaluation;`

**Change:** one file's evaluation in serial's order within the file (evaluate → resolve subdirs →
convert each declaration), as a value. `Stopped` returns before calling the source when `stop` is
set. `traversal::load` still runs serially in this task, now through `evaluate_file`.

**Proof:** unit tests in `evaluation.rs`, with an in-file counting source:
- `a_set_stop_flag_evaluates_nothing` — `Stopped`, and the source was called 0 times.
- `a_missing_file_is_absent`.
- `an_evaluation_error_is_failed` / `a_bad_subdir_is_failed` / `a_bad_declaration_is_failed` —
  each `Failed` with the same error serial load gives for it.
- `a_file_with_both_a_bad_subdir_and_a_bad_declaration_fails_on_the_subdir` — serial's order.
- `a_good_file_is_loaded_with_sorted_subdirs_and_its_declarations`.
- Every existing `tests/flows/load.rs` test passes unchanged.

**Done when:** `cargo test -p buildl-core` passes and `cargo make dod` is green. Commit
`refactor(buildl-core): evaluate one build file as a value`.

### Task 4 — Add `Loading`: hand-out and serial replay

**Files:**
- Create `crates/buildl-core/src/load/loading.rs` — `pub(crate) struct Loading` with
  `begin() -> Vec<Directory>`, `done(directory, FileResult) -> Vec<Directory>`, and
  `finish(self) -> Result<Vec<Declaration>>`; holds the results map (`BTreeMap`), the dispatched
  set, the replay's queue, seen set and requesters, and a reference to the stop `AtomicBool`
- Modify `crates/buildl-core/src/load/mod.rs` — `mod loading;`

**Change:** spec §3.1's step and replay, exactly: `begin` → `[root]`; `done` stores the result;
after the replay has met its error it returns `[]`; otherwise collects a `Loaded` result's subdirs
not yet dispatched (in order), advances the replay, and on a new error sets `stop` and returns
`[]`. `finish`: replay done → declarations sorted with `declaration_order` (`traversal.rs:288-299`)
→ `Ok`; replay error → `Err`; replay waiting on a directory with no result → `LostBuildFiles` of
every dispatched directory without a result, in order. `Absent` maps to `MissingBuildFile` with
the replay's recorded requester.

**Proof:** unit tests in `loading.rs`, feeding `FileResult`s by hand (no dispatcher):
- `a_loaded_root_hands_out_every_subdir_at_once` — `done(root, Loaded{[a, b, c]})` returns
  `[a, b, c]` before any is done.
- `a_subdir_already_handed_out_is_not_handed_out_again` — `a` and `b` both name `c`: `c` once.
- `every_arrival_order_gives_serial_declarations` — a four-directory tree, results delivered in
  every permutation that respects hand-out: `finish` equals the serial result.
- `every_arrival_order_gives_serial_error` — two failing files: `finish` is the one first in
  serial queue order, in every permutation.
- `a_directory_named_by_two_parents_reports_serials_requester` — both name missing `m`; the
  later-in-serial parent's result arrives first: `MissingBuildFile.requested_by` is the earlier.
- `the_error_sets_stop_and_later_results_hand_out_nothing` — after the error, `stop` is set and
  each further `done`, even a `Loaded` with new subdirs, returns `[]`.
- `a_directory_without_a_result_is_lost` — `finish` with one handed-out directory never done →
  `LostBuildFiles { directories: [it] }`.

**Done when:** `cargo test -p buildl-core --lib load::loading` passes and `cargo make dod` is
green. Commit `feat(buildl-core): replay serial load over results in any order`.

### Task 5 — Run `load` through the dispatcher, and pass one from every caller

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs` — `load<S, D>(source, dispatcher, entry)`
  (spec §3.1 signature): `dispatcher.dispatch(work, step)` where the task is a `Directory`, the
  message type is `()` (load sends none), and `work` returns `(Directory, FileResult)` — `Turn`
  hands `step` only the result, and `Loading::done` needs the directory; `step` forwards
  `Turn::Begin` / a result to `Loading`. An `Err` from `dispatch` itself (an adapter failure,
  `ports/dispatcher.rs:42-47`) is returned before `Loading::finish` is called; the stop `AtomicBool` lives on `load`'s
  stack (precedent: `execute/perform.rs:16`, `:46`); then `Loading::finish`. The serial loop is gone
- Modify `crates/buildl-core/src/pipeline/driver.rs` — `:124`, `:159`, `:209` pass
  `&mut self.ports.dispatcher`; free `check` (`:235`, both loads `:236-237`), `graph` (`:247`),
  `query` (`:260`) gain `dispatcher: &mut D`; `Pipeline::check/graph/query` (`:74`, `:88`, `:99`)
  take `&mut self`; doc comments at `:225-229` (and the matching ones on `graph`/`query`) say "a
  source and a dispatcher"
- Modify `crates/buildl-core/src/pipeline/gc.rs` — `gc` gains `dispatcher: &mut D` (after `source`)
  for its load; its `mod tests` gains an in-file `Inline` test dispatcher that runs each task on the
  calling thread, oldest first, and every unit test passes `&mut Inline` (the crate's only other
  in-crate dispatcher, `Stub` in `ports/port_set.rs:248-259`, runs nothing and is private to that
  file's tests)
- Modify `crates/buildl-core/tests/flows/load.rs`, `common.rs:1411`, `check.rs`, `graph.rs`,
  `query.rs`, `gc.rs` — pass `&mut FakeDispatcher::default()` (spec P9; re-run
  `grep -rn "load(\|check(\|graph(\|query(\|gc(" crates/buildl-core/tests` to catch every call)
- Modify `crates/buildl/src/local_ports.rs` — `pub fn dispatcher(&self) -> ThreadPool`:
  `ThreadPool::new(self.workspace.execution.workers)`; `ports` (`:241`) calls it
- Modify `crates/buildl/src/workspace.rs` — `check`, `graph`, `query` (`:95-120`) and `gc` pass
  `&mut self.wiring.dispatcher()`
- Modify `crates/buildl-lua/tests/flows/common.rs` — `InlineDispatcher`: implements `Dispatcher`,
  runs each task on the calling thread, oldest first; doc says it is a test double
- Modify `crates/buildl-lua/tests/flows/load.rs` — `:59` passes `&mut InlineDispatcher`

**Change:** Load is parallel wherever the dispatcher is. With `FakeDispatcher::default()`
(`OldestFirst`, `OneAtATime`, `common.rs:789-833`) the evaluation order is serial's.

**Proof:**
- `a_directory_requested_twice_is_evaluated_once` (`tests/flows/load.rs:90-100`) keeps its exact
  order `[dir(""), dir("a"), dir("a/x")]` under the default dispatcher.
- Every existing test in `buildl-core`, `buildl`, `buildl-lua`, `buildl-cli` passes.

**Done when:** `cargo test --workspace` passes, `crates/expected-module-edges.txt` is unchanged
(`load -> ports`, `load -> types` exist), and `cargo make dod` is green. Commit
`feat(buildl-core): load build files through the dispatcher`.

### Task 6 — Prove the result and error do not depend on the dispatcher's order

**Files:**
- Modify `crates/buildl-core/tests/flows/load.rs` — the matrix tests below

**Change:** none in library code.

**Proof:** each over a workspace of at least five directories at two depths, comparing values with
the default dispatcher's result:
- `every_order_and_pace_loads_the_same_declarations` — `Order::{OldestFirst, NewestFirst}` ×
  `Pace::{OneAtATime, Together}`.
- `every_order_and_pace_reports_the_same_error` — two failing files and one missing directory.
- `a_task_taken_after_the_error_evaluates_nothing` — `OldestFirst`, `OneAtATime` (serial order):
  root names `a`, `b`, `c`, and `a` fails. Root's result hands out `[a, b, c]`; `a`'s failure is
  replayed and sets the stop flag before `b` and `c` are taken, so both come back `Stopped`:
  `FakeSource::evaluated()` is exactly `[root, a]`, and the error is `a`'s. (Under `NewestFirst` +
  `OneAtATime` the fake works as a stack, `common.rs:848-850`, `:873-875`, so no task would still be
  waiting when the error is met and the assertion would hold vacuously.)
- `a_lost_load_task_is_lost_build_files` — `lose: {1: Loss::Unreported}` and
  `{1: Loss::Unrun}` (`common.rs:799-806`): `Error::LostBuildFiles` naming that directory.
- `check_graph_query_and_gc_load_in_any_order` — `pipeline::check`, `graph`, `query` and `gc` with
  `NewestFirst` + `Together` return what they return with the default.

**Done when:** `cargo test -p buildl-core --test flows load` passes and `cargo make dod` is green.
Commit `test(buildl-core): prove load is the same in any dispatch order`.

---

## Risks

- **Task 5 is the widest change**: one signature reaches four crates and every test that loads;
  it has to land as one commit to compile. The grep in its Files line catches stray callers.
- **The stop is observable only where tasks are still waiting when the error is replayed.**
  `Pace::Together` runs every waiting task before `step` sees a result (`common.rs:813-816`), and
  `NewestFirst` + `OneAtATime` drains like a stack; `OldestFirst` + `OneAtATime` with siblings after
  the failing file is the setting that leaves tasks waiting (Task 6's third test).
- **`with_second_run`** counts root evaluations to switch file sets (`FakeSource::evaluate`, `common.rs:86-95`); under the
  dispatcher, `check`'s second load starts after the first `dispatch` returns, so the count still
  switches between loads. If a `check` flow test regresses, this is the place to look.
- **Order dependence elsewhere:** any test asserting `FakeSource::evaluated()` order beyond P16 would
  fail under a non-default dispatcher; Task 6 compares values only.

## Proof

- `cargo make dod` (with every `guard-*`, including `guard-core-purity`: core still starts no
  thread) and `cargo deny check` green after each task.
- `grep -rn "thread::" crates/buildl-core/src` still matches nothing.
