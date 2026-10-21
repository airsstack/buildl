---
status: approved
created: 2026-10-21
built-on: 6c7f83a41771
---

# Core Verify Implementation Plan

**Goal:** `buildl-core` re-runs a workspace's cached actions against fake ports and reports, per target, whether each reproduced its cached outputs.

**Architecture:** New value types under `types/` (spec §3.1), a new crate-root module `verify/`
(spec §3.2) holding the flow, the sample choice and the first-difference walk, and
`Pipeline::verify` in `pipeline/driver.rs`. The module reuses `execute`'s `prepared`,
`restorable`, `restore`, `perform` and `shown` unchanged: their modules and items become
`pub(crate)` and nothing else about them changes. No port changes; no adapter is touched.
`verify` is not a phase: like `history`, it is a module that serves a command and may name a
phase's internals. `lib.rs`'s rule 1 ("No phase module names another") gains that sentence, so
`verify` naming `execute` is the documented exception, not a breach.

**Tech Stack:** Rust, `buildl-core`, `cargo make dod`, `cargo deny check`. Guideline:
`claudestacks-guideline-rust:rust-guidelines` (strong types, `mod.rs` export-only, colocated unit
tests, doc discipline).

**Content authority:** spec §2 (Plan, Restore, Sample, Re-run, Compare, Nothing recorded), §3.1–§3.3,
§4, §5 (core rows).

---

## Files that change

```
crates/buildl-core/src/types/sample.rs             — [create] Sample: All | Lowest(NonZeroUsize)
crates/buildl-core/src/types/check.rs              — [create] Check, OutputDifference, FileChange, EntryChange
crates/buildl-core/src/types/verification.rs       — [create] Verification: checked targets and the two counts
crates/buildl-core/src/types/mod.rs                — [modify] declare and re-export the three
crates/buildl-core/src/verify/mod.rs               — [create] module docs, `mod`/`pub use` only
crates/buildl-core/src/verify/sample.rs            — [create] choose the candidates for a Sample
crates/buildl-core/src/verify/difference.rs        — [create] the first difference of two manifests
crates/buildl-core/src/verify/assembly.rs          — [create] verify(): restore, choose, re-run, compare
crates/buildl-core/src/execute/mod.rs              — [modify] perform, record, settle, assembly items reachable crate-wide
crates/buildl-core/src/execute/{assembly,perform,record,settle}.rs — [modify] visibility of the reused items only
crates/buildl-core/src/pipeline/driver.rs          — [modify] Pipeline::verify
crates/buildl-core/src/lib.rs                      — [modify] `pub mod verify`, re-exports, module list and rule 1 in the crate docs
crates/buildl-core/tests/flows/verify.rs           — [create] verify flow tests through Pipeline::verify against the fakes of tests/flows/common.rs
crates/buildl-core/tests/flows/main.rs             — [modify] `mod verify;`
crates/buildl-core/tests/flows/common.rs           — [modify] only if a fake lacks a hook a test needs (e.g. removing a held blob)
```

## Order of work

### Task 1 — The verify value types

**Files:** `types/sample.rs`, `types/check.rs`, `types/verification.rs`, `types/mod.rs`.

**Change:** add, as spec §3.1 states them:
- `Sample` — `All`, `Lowest(NonZeroUsize)`.
- `Check` — `Verified`; `Mismatched { output: OutputName, difference: OutputDifference }`;
  `Failed { class: OutcomeClass, diagnostic: Option<Diagnostic>, stdout: Vec<u8>, stderr: Vec<u8> }`;
  `Skipped { because: Label }`; `Unplaced { diagnostic: Diagnostic }`.
- `OutputDifference` — `Kind`, `File(FileChange)`, `Entry { path: TreePath, change: EntryChange }`;
  `FileChange` — `Bytes`, `Mode`; `EntryChange` — `Bytes`, `Mode`, `OnlyCached`, `OnlyRerun`.
  The name `Difference` is taken (`types/plan.rs:30`).
- `Verification` — `checked: Vec<(Label, Check)>` (label order), `not_cached: usize`,
  `not_sampled: usize`, with accessors and a constructor that sorts `checked` by label.

**Proof:** written first, seen failing, then passing —
- `verification_lists_checks_in_label_order` — built from checks in reverse order, `checked()`
  returns them sorted by label.
- `a_check_and_a_verification_can_be_handed_to_another_thread` — `Send` holds (compile-time
  assertion in the test, as `perform.rs`'s `a_task_and_its_result_can_be_handed_to_another_thread`).

**Done when:** `cargo test -p buildl-core types::` passes and `cargo make dod` is green. Commit
`feat(buildl-core): add the values a verify reports`.

### Task 2 — Choosing the sample and finding the first difference

**Files:** `verify/mod.rs`, `verify/sample.rs`, `verify/difference.rs`, `lib.rs` (`pub mod verify`).

**Change:**
- `sample.rs`: `chosen(candidates, sample)` takes candidates each with its finished `ActionKey`
  and returns those to re-run: all for `Sample::All`; for `Lowest(n)`, the `n` with the lowest
  `ActionKey::digest()` (ties cannot happen between distinct keys; equal digests keep label
  order), all of them when `n` exceeds the count. It also returns how many were left out.
- `difference.rs`: `first_difference(cached: &OutputManifest, rerun: &OutputManifest)` returns
  `Option<OutputDifference>` by spec §3.3: file vs file → `File(Bytes)` when digests differ,
  else `File(Mode)` when modes differ; tree vs tree → walk both entry lists (sorted by path,
  `types/tree.rs:70`) merged by path, the first path present on one side only →
  `OnlyCached`/`OnlyRerun`, else digests → `Bytes`, else modes → `Mode`; file vs tree either way →
  `Kind`; equal manifests → `None`.

**Proof:** written first, seen failing, then passing —
- `sample.rs`: `all_takes_every_candidate`, `lowest_takes_the_lowest_key_digests`,
  `lowest_above_the_count_takes_all`, `the_choice_does_not_depend_on_the_order_given`,
  `the_left_out_count_is_the_rest`.
- `difference.rs`: `equal_manifests_have_no_difference`, `a_file_with_other_bytes_differs_in_bytes`,
  `a_file_with_only_another_mode_differs_in_mode`, `bytes_outrank_mode_for_a_file`,
  `a_tree_names_the_first_path_whose_bytes_differ`, `a_tree_names_a_path_only_the_cache_holds`,
  `a_tree_names_a_path_only_the_rerun_holds`, `the_earliest_path_wins_whatever_kind_of_change`,
  `a_file_against_a_tree_is_a_change_of_kind` (both directions).

Fixtures: the `file`/`tree` helpers of `execute/fixtures.rs:105-126` are `#[cfg(test)]` and
`pub(crate)`; reuse them rather than writing new ones (make the module reachable from `verify`
tests if it is not).

**Done when:** `cargo test -p buildl-core verify::` passes and `cargo make dod` is green. Commit
`feat(buildl-core): choose a verify sample and name the first differing file`.

### Task 3 — The verify flow and `Pipeline::verify`

**Files:** `verify/assembly.rs`, `verify/mod.rs`, `execute/mod.rs`, `execute/assembly.rs`,
`execute/perform.rs`, `execute/record.rs`, `execute/settle.rs`, `pipeline/driver.rs`, `lib.rs`,
`tests/flows/verify.rs`, `tests/flows/main.rs` (and `tests/flows/common.rs` only if needed).

**Change:**
- Visibility only: `execute/mod.rs` declares `layout`, `perform`, `record` and `settle`
  `pub(crate)`; `prepared`, `Prepared` and its fields (`execute/assembly.rs:227-283`), `own_of`
  and `made_from` become `pub(crate)`. No body changes.
- `verify/assembly.rs`: `pub fn verify<P: Ports>(whole, graph, plan, sample, ports: &mut PortSet<P>) -> Result<Verification>`:
  1. `made_from(plan, graph)` else `Error::ForeignPlan`; `Layout::of(whole, …)` and `Layout::of(graph, …)`
     and `prepared(...)` as `execute` does (`execute/assembly.rs:113-119`).
  2. In graph order, for each `Verdict::Clean { key }` with a row (`ActionCache::row`): `restorable`
     (`settle.rs:74`) with the cas's `contains`/`get`. `None` → not cached. `Some(manifests)` but a
     manifest whose `Digest::of(manifest.to_bytes()?)` is not the row's digest at its index → not
     cached (spec §3.3). Otherwise `restore` (`record.rs:240`); an outcome of class `InfraFailure`
     → `Check::Unplaced` with its diagnostic; otherwise a **restored** target, holding its key,
     `ReadyAction`, row and manifests. Every other verdict → not cached.
  3. A restored target with a direct dependency (from `Prepared::deps`) that is not restored
     (not cached or unplaced) → `Check::Skipped { because }`, naming the lowest such label. This
     is decided against step 2's restored set only: a skipped target's outputs are in place, so
     its own dependents are not skipped for it (chain A ← B ← C, A not cached: B skipped, C
     re-runs). The restored targets not skipped are the candidates.
  4. `sample::chosen` over the candidates.
  5. `Dispatcher::dispatch` with work `perform(task, strategy, cas, clock, shared, tell)`
     (`perform.rs:313`), where `shared` borrows a local `AtomicBool` that is never set and has
     `live: None` (`Shared`'s fields are references, `perform.rs:44-51`); `tell` ignored; the step
     returns every task on `Turn::Begin` and nothing after, collecting each `Worked::Performed` by
     node. A `Worked::NotBegun` cannot happen with the flag never set: treat it as
     `Error::DispatcherFault`.
  6. Compare: `Ended::Produced { outputs, .. }` → for each output in declared order,
     `first_difference(cached manifest, kept manifest)`; the first `Some` →
     `Mismatched { output, difference }`; none → `Verified` (spec §3.3: step 2 makes this the
     same as comparing digests, and no error case exists).
     `Ended::Failed { class, diagnostic, printed }` → `Failed`, `stdout`/`stderr` read back with
     `shown` (`record.rs:284`) when `printed` is `Some`, empty otherwise.
  7. No `ActionCache::record`/`forget`, no `EventLog::append`, no `Reporter` call, no
     `ContentStore::place`/`clear` beyond step 2's `restore`.
- `Pipeline::verify(entry, scope, overrides, sample)`: load → resolve → `narrow(…, Purpose::Build)`
  → `plan(…, Rerun::Off, …)` → `verify` (spec §3.2; no `graph.record`).
- `lib.rs`: re-export `verify` and the new types; the crate docs' module list names `verify`, and
  rule 1 says `verify`, like `history`, is not a phase (Architecture, above).

**Proof:** written first, seen failing, then passing. Flow tests go in `tests/flows/verify.rs`,
through `Pipeline::verify`, against the fakes of `tests/flows/common.rs` (`FakeCache::recorded`/
`forgotten` `:292-301`, `FakeCas::placements`/`fail_place`/`forget` `:383-396`, `FakeLog`, the
scripted `FakeStrategy` `:633-693`), as `lib.rs` rule 5 requires. The pure parts of
`verify/assembly.rs` (if any are split out) keep colocated unit tests.
- `a_target_that_reproduces_its_outputs_is_verified`
- `a_target_whose_rerun_writes_other_bytes_is_mismatched_naming_the_output`
- `a_tree_output_mismatch_names_the_first_differing_file`
- `a_failed_rerun_carries_its_class_and_what_it_printed` — and the other targets are still checked
  (spec §4: the remaining re-runs go on)
- `targets_that_are_not_clean_are_counted_not_cached` (dirty, never built, `always`, conditional)
- `a_clean_target_whose_content_is_gone_is_counted_not_cached`
- `a_cached_manifest_that_does_not_hash_to_its_digest_is_counted_not_cached`
- `a_target_whose_dependency_is_not_cached_is_skipped_naming_it` — including the chain
  A ← B ← C with A not cached: B skipped naming A, C re-run
- `a_failed_restore_is_unplaced_and_skips_its_dependents`
- `a_sample_reruns_only_the_lowest_keys_and_counts_the_rest`
- `every_clean_target_is_put_back_in_place_before_anything_reruns`
- `verify_records_no_row_forgets_no_row_and_logs_nothing` (fakes that fail on those calls)
- `a_reruns_outputs_are_never_placed` (the fake cas records `place` calls: only restores')
- `a_plan_of_another_graph_is_refused` (`Error::ForeignPlan`)
- `pipeline_verify_plans_like_build` — a named test target is checked; with nothing named, test
  targets are left out (spec §2 Command).

**Done when:** `cargo test -p buildl-core` (lib and `--test flows`) passes and `cargo make dod` is green. Commit
`feat(buildl-core): re-run cached actions and compare their outputs`.

---

## Risks

- **Reusing `execute` internals.** The items are private today; exposing them `pub(crate)` must
  not change a body. The existing `execute` tests are the guard: they pass unchanged.
- **`restore`'s outcome type.** `restore` returns an `ActionOutcome`; step 2 reads its `class`
  (`InfraFailure` for a failed placement, `record.rs:106` `unplaced`). If that class is not
  distinct, read the plain `place` result instead — `execute` probes nothing here; the named
  test `a_failed_restore_is_unplaced_and_skips_its_dependents` settles it.
- **`made_from` and `Layout::of` signatures** are taken from `execute/assembly.rs:104-119` as of
  `30b25a5`.
- Not done here: the adapter and CLI (plans 02, 03).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` OK (no adapter touched).
- The `execute` module's existing tests pass with no change to their bodies.
