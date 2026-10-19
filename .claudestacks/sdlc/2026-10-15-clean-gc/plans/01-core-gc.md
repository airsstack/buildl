---
status: executing
created: 2026-10-19
built-on: 137a1ada1042
---

# Core gc Implementation Plan

**Goal:** `buildl-core` decides, against fake ports, which rows `gc` drops and which blobs it removes.

**Architecture:** A new `Sweep` port lists and removes what a store holds, kept out of the `Ports`
bundle and handed only to `gc`, as `RunHistory` is handed only to `log`. `ActionCache` gains
`labels`. A pure `history::logged_outputs` names the blobs `buildl log` reads. `pipeline::gc` is a
free function in its own file, like `pipeline::log`: load and resolve once, forget undeclared rows,
mark, sweep — rows always before blobs.

**Tech Stack:** Rust (`buildl-core`, plus the two `ActionCache` impls in `buildl` the trait change
forces), `cargo make dod`, `cargo deny check`.

**Content authority:** spec §3.1–§3.4, §3.6, §4 rows `history` unit, `pipeline::gc` unit,
`buildl-core` flows, `LocalCache` adapter.

---

## Files that change

```
crates/buildl-core/src/ports/sweep.rs              — [create] the Sweep port and its unit tests
crates/buildl-core/src/ports/mod.rs                — [modify] declare and export `sweep`, list it in the module doc
crates/buildl-core/src/lib.rs                      — [modify] re-export `Sweep`, `Collected`, `gc`
crates/buildl-core/src/error.rs                    — [modify] `PortName::Sweep` and its rendered phrase
crates/buildl-core/src/ports/action_cache.rs       — [modify] `labels` on the trait
crates/buildl-core/src/ports/bundle.rs             — [modify] doc-example `Nothing` gains `labels`
crates/buildl-core/src/ports/port_set.rs           — [modify] test `Stub` gains `labels`
crates/buildl-core/src/execute/record.rs           — [modify] `Rows` gains `labels`
crates/buildl/src/cache/local_cache.rs             — [modify] `LocalCache::labels` and its unit test
crates/buildl/src/report/plan_text.rs              — [modify] `Rows` gains `labels`
crates/buildl-core/src/history/logged_outputs.rs   — [create] `logged_outputs` and its unit tests
crates/buildl-core/src/history/last_run.rs         — [modify] `has_output` visible to the sibling file
crates/buildl-core/src/history/mod.rs              — [modify] declare `logged_outputs`, re-export it pub(crate)
crates/buildl-core/src/pipeline/gc.rs              — [create] `Collected`, `gc`, and their unit tests
crates/buildl-core/src/pipeline/mod.rs             — [modify] declare `gc`, re-export `Collected` and `gc`
crates/buildl-core/tests/flows/common.rs           — [modify] `FakeCache::labels`; `impl Sweep for FakeCas`
crates/buildl-core/tests/flows/gc.rs               — [create] whole-`gc`-after-`build` flow tests
crates/buildl-core/tests/flows/main.rs             — [modify] `mod gc;`
```

## Order of work

### Task 1 — Add the `Sweep` port and `PortName::Sweep`

**Files:**
- Create `crates/buildl-core/src/ports/sweep.rs` — `trait Sweep` with `held` and `remove`, exactly
  as spec §3.1 shows, with the spec's doc comments
- Modify `crates/buildl-core/src/ports/mod.rs` — `pub mod sweep;`, `pub use sweep::Sweep;`, a
  bullet in the module doc beside `RunHistory` (`ports/mod.rs:20`)
- Modify `crates/buildl-core/src/lib.rs` — add `Sweep` to the `pub use ports::{…}` list (`lib.rs:123-127`)
- Modify `crates/buildl-core/src/error.rs` — `PortName::Sweep` after `GraphRecord`
  (`error.rs:558-583`), doc "What lists and removes what the content store holds.", rendered
  `"sweep"` in `Display` (`:585-600`)
- Test `crates/buildl-core/src/error.rs` — the existing case table at `:1423-1437`

**Change:** the port exists; `Ports` and `PortSet` do not gain it (spec §3.1). `PortName` is
`#[non_exhaustive]` and `buildl-cli/src/exit.rs:86-89` maps every port but `Manifest` to
`Infrastructure`, so `Sweep` exits 3 with no change there.

**Proof:**
- `every_port_name_renders_a_human_phrase` (existing, `error.rs:1423`) — gains the row
  `(PortName::Sweep, "sweep")`; fails until the variant renders.
- `ports/sweep.rs` carries no tests: it holds only a trait definition, and says so in its module
  doc as `ports/run_history.rs:12` does ("This file holds only a trait definition, so it carries no
  logic to unit-test."). Its contract is proven by plan 01 Task 4's fakes and plan 02 Task 1's
  `LocalCas` tests.

**Done when:** `cargo test -p buildl-core --lib` passes and `cargo make dod` is green. Commit
`feat(buildl-core): add the Sweep port`.

### Task 2 — Add `ActionCache::labels` to the trait and all six impls

**Files:**
- Modify `crates/buildl-core/src/ports/action_cache.rs` — `fn labels(&self) -> Vec<Label>;`, doc
  "Every label with a row, in label order." (spec §3.2)
- Modify `crates/buildl-core/src/ports/bundle.rs` — doc example `Nothing` returns `Vec::new()`
- Modify `crates/buildl-core/src/ports/port_set.rs` — test `Stub` returns its keys (or empty if it holds none)
- Modify `crates/buildl-core/src/execute/record.rs` — `Rows` returns its map's keys
- Modify `crates/buildl/src/cache/local_cache.rs` — `LocalCache::labels` returns the keys of its
  `BTreeMap<Label, CacheRow>` (`local_cache.rs:36-37`), journal replay included since `open`
  already applies it
- Modify `crates/buildl/src/report/plan_text.rs` — `Rows` returns its keys
- Modify `crates/buildl-core/tests/flows/common.rs` — `FakeCache::labels` (`:297`) returns its keys

The six impls are the ones `grep -rn "impl ActionCache for" crates` lists (spec P10); run it again
before starting and add any new one.

**Change:** every action cache lists its labels in label order. Adding a required trait method
breaks every impl, so all six change in this one commit to keep the workspace compiling.

**Proof:**
- `local_cache.rs` unit test `labels_lists_rows_after_open_record_and_forget` — open an empty
  cache: `labels` is empty; record `//b:x` then `//a:y`: `labels` is `[//a:y, //b:x]`; forget
  `//a:y`: `[//b:x]`; drop and reopen (journal replay): still `[//b:x]`.
- `record.rs` unit test `rows_lists_its_labels_in_order` — two recorded rows come back sorted.

**Done when:** `cargo test -p buildl-core -p buildl` passes, the bundle doctest passes, and
`cargo make dod` is green. Commit `feat(buildl-core): list an action cache's labels`.

### Task 3 — Add `history::logged_outputs`

**Files:**
- Create `crates/buildl-core/src/history/logged_outputs.rs` —
  `pub(crate) fn logged_outputs(lines: &[LogLine]) -> BTreeSet<Digest>`
- Modify `crates/buildl-core/src/history/last_run.rs` — `has_output` (`:80`) becomes `pub(super)`,
  so both files apply the one rule
- Modify `crates/buildl-core/src/history/mod.rs` — `mod logged_outputs;`,
  `pub(crate) use logged_outputs::logged_outputs;`, the Responsibilities line names it

**Change:** for each label with a logged run for which `has_output` holds, the stdout and stderr
digests of its newest such run — the run `last_run` (`last_run.rs:39`) would choose. Damaged lines
and runs without both digests add nothing (spec §3.3).

**Proof:**
- `takes_each_labels_newest_run_with_output` — two labels, each with two runs with output: only
  the newer run's two digests per label are in the set (4 digests).
- `an_older_runs_digests_are_not_taken` — one label, an older run with output then a newer one:
  the older digests are absent.
- `a_newer_run_without_output_leaves_the_older_one` — newest run has a `null` digest: the older
  run's digests are taken, as `last_run` would show.
- `skipped_null_and_damaged_lines_add_nothing` — only skipped runs, runs with one `null` digest,
  and damaged lines: the set is empty.

**Done when:** `cargo test -p buildl-core --lib history` passes and `cargo make dod` is green.
Commit `feat(buildl-core): name the blobs the build log still reads`.

### Task 4 — Add `pipeline::gc` and `Collected`

**Files:**
- Create `crates/buildl-core/src/pipeline/gc.rs` — `Collected` and `gc` with the signature in spec
  §3.4; doc comments state the four steps and the order rules of the spec's §3.4 table
- Modify `crates/buildl-core/src/pipeline/mod.rs` — `pub mod gc;`, `pub use gc::{Collected, gc};`,
  module doc's Responsibilities names them
- Modify `crates/buildl-core/src/lib.rs` — `pub use pipeline::{…}` (`lib.rs:121`) gains `Collected`, `gc`

**Change:** `gc` runs spec §3.4's steps 1–4 in order: `resolve(load(source, entry))` once; forget
every `cache.labels()` entry that is not a node label of the graph (`TargetGraph::nodes`,
`target_graph.rs:117`; aliases have no rows); mark each remaining row's output manifests, the
digests their `OutputManifest` names (`File` → its digest, `Tree` → every entry's digest), its
stdout and stderr, plus `logged_outputs(history.lines()?)`; then `remove` every digest in
`sweep.held()? − marked`, summing bytes. A manifest whose `get` is `None` or whose bytes are not a
manifest is marked by its own digest only. Any error before step 4 returns with nothing removed;
an error in step 4 returns at once. No new module edge (spec P14): `pipeline -> history`,
`-> load`, `-> resolve`, `-> ports`, `-> types` already exist in `crates/expected-module-edges.txt`.

**Proof:** unit tests in `gc.rs`, with small in-file fakes (a `BTreeMap` cache recording every call
in one shared `Vec<&'static str>` journal with the sweep, a `BTreeMap` store, a `Vec<LogLine>`
history, a one-file source):
- `undeclared_rows_are_forgotten_declared_kept` — rows for `//:a` (declared) and `//:gone`: only
  `//:gone` forgotten; `rows_dropped == 1`.
- `every_forget_comes_before_the_first_remove` — the shared call journal has no `forget` after a
  `remove`.
- `a_kept_rows_manifest_files_and_output_streams_are_kept` — a row with a `Tree` manifest of two
  files plus stdout/stderr: none of those five digests is removed; an unrelated held digest is.
- `log_only_digests_are_kept` — a digest named only by a log line's newest run with output survives.
- `a_missing_or_foreign_manifest_keeps_the_row_and_marks_only_itself` — one row whose manifest
  digest is not in the store, one whose bytes are not a manifest: both rows kept, both digests not
  removed, and a held file digest those would not name is removed.
- `a_load_error_forgets_and_removes_nothing` — the source fails: the error is returned, no
  `forget`, no `remove`.
- `a_store_error_while_marking_removes_nothing` — `get` errs: error returned, no `remove` call.
- `counts_and_bytes_are_summed` — three unmarked digests of 1, 2 and 4 bytes:
  `Collected { rows_dropped: 0, blobs_removed: 3, bytes_removed: 7 }`.

**Done when:** `cargo test -p buildl-core --lib pipeline::gc` passes,
`crates/expected-module-edges.txt` is unchanged, and `cargo make dod` is green (its `clippy`
step runs `guard-module-edges`, `Makefile.toml:60-65`). Commit
`feat(buildl-core): collect what no declared target or log run needs`.

### Task 5 — Prove a whole `gc` after a `build` over the fake ports

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs` — `impl Sweep for FakeCas`: `held` is the keys
  of `CasState.blobs` (a `BTreeMap<Digest, Vec<u8>>`, `common.rs:331`, behind `FakeCas`'s
  `Mutex`, `:344-346`); `remove` drops the entry and returns its byte length, `0` when absent. The
  flow passes the one `FakeCas` as both store and sweep, as `Workspace::gc` passes `&cas, &cas`
- Create `crates/buildl-core/tests/flows/gc.rs` — the flows below
- Modify `crates/buildl-core/tests/flows/main.rs` — `mod gc;`

**Change:** none in library code; this proves `gc` composes with what `Pipeline::build` writes.

**Proof:**
- `gc_after_build_keeps_everything_still_declared` — build a two-target workspace with
  `common::ports`, then `gc` with the same source: `Collected` is all zeros and every blob the
  build stored is still held.
- `gc_after_a_target_is_removed_drops_its_row_and_only_its_blobs` — build, then `gc` with a source
  that no longer declares one leaf target, and a `FakeHistory` (`common.rs:481`) holding one
  `LogLine` for that target whose stdout and stderr digests are the ones its cache row held,
  read from the row before `gc` (`FakeLog` stores `OutcomeEvent`s and is not a `RunHistory`, and
  core has no event-to-line conversion): its row is gone, its manifest and file blobs are gone,
  its stdout/stderr blobs remain, and every other target's blobs remain.
- `a_second_gc_removes_nothing` — `gc` twice: the second returns all zeros.

**Done when:** `cargo test -p buildl-core --test flows gc` passes and `cargo make dod` is green.
Commit `test(buildl-core): prove gc over a built workspace`.

---

## Risks

- **Task 2 is the widest change**: one trait method touches six impls across two crates and a
  doctest. A seventh impl added since spec P10 would break the build — Task 2 re-runs the grep.
- **Task 4's step order is the crash-safety guarantee** (design §8.5); the shared call journal in
  its unit tests is the one test that would catch a reordering.
- Not done here: `LocalCas: Sweep` and the facade (plan 02), CLI and documents (plan 03).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` green (Task 2 touches `buildl` adapters).
- `grep -rn "impl ActionCache for" crates | wc -l` equals `grep -rn "fn labels(&self)" crates | wc -l`
  minus the one declaration in `ports/action_cache.rs`.
