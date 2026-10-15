---
status: done
created: 2026-10-15
depends-on: [04]
built-on: fefed82cb705
---

# Forgetting a contradicted pass Implementation Plan

**Goal:** An action that fails under the key its target's row was recorded under removes that
row, durably, before its log line is written.

**Architecture:** The `ActionCache` port gains `forget(&mut self, label) -> Result<()>`.
`LocalCache` makes it durable with a journal line whose row is `null`, replayed as a removal; the
snapshot's format does not change. In `buildl-core`, one helper of `execute/record.rs` takes a
just-made outcome of a performed action and forgets the target's row when the outcome is a
failure and the row's key digest is the performed key's; a forget that fails turns the outcome
into the store's infrastructure failure. `Build::record` calls it before the outcome is finished,
so the forget precedes the log line (design §8.5's order).

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `serde`, the canonical serializer. Definition of
Done: `cargo make dod` and `cargo deny check`.

**Content authority:** spec §3.5, §5, §7 (first risk), §6 (rows for `execute/assembly.rs`,
`cache/journal.rs`, `cache/local_cache.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits.

**Where the decision's tests live.** Spec §6 lists the forget tests under `execute/assembly.rs`.
`Build` has no unit-test harness (its tests cover only the pure helpers, `assembly.rs:614-760`),
so the decision is the pure helper `forget_contradicted` in `execute/record.rs`, called from
`Build::record`, and four of the five named tests are its unit tests there. The fifth,
`an_unplaced_cached_target_forgets_nothing`, is about a path that never calls the helper
(`Build::drain` → `restore`), so it is a flow, where the whole build runs.

Facts the tasks rest on, each read at commit `1799d47`:

| Fact | Where |
|---|---|
| `ActionCache` has exactly `row(&self, &Label) -> Option<&CacheRow>` and `record(&mut self, Label, CacheRow) -> Result<()>`; its module doc says it holds only a trait definition | `crates/buildl-core/src/ports/action_cache.rs:12-35` |
| The five implementations: `LocalCache` (`crates/buildl/src/cache/local_cache.rs:121`), `FakeCache` (`crates/buildl-core/tests/flows/common.rs:286`), the doc example `Nothing` (`crates/buildl-core/src/ports/bundle.rs:109`), `Stub` (`crates/buildl-core/src/ports/port_set.rs:176`), `Rows` (`crates/buildl/src/report/plan_text.rs:201`) | `grep -rn 'impl ActionCache for' --include='*.rs' crates` |
| `Entry { label: Label, row: CacheRow }`, `#[serde(deny_unknown_fields)]`; `Journal::read` reads each whole line with `canonical::from_slice`, a refused line is `Error::CorruptState` naming the file; `append` writes one canonical line | `crates/buildl/src/cache/journal.rs:20-28`, `:76-93`, `:111-125` |
| `canonical::from_slice` refuses bytes that are not exactly what `to_vec` writes for the value read (`NonCanonicalJson`), so `{"label":"//:a"}` — which `serde` would read with `row: None` — is refused: `to_vec` writes `{"label":"//:a","row":null}` | `crates/buildl-core/src/json/canonical.rs:129-141` |
| `LocalCache::open` applies each journal entry with `rows.insert(entry.label, entry.row)`, then folds; `record` appends an `Entry`, inserts, sets `unfolded`; `fold` writes `cache.json` from `rows` and removes the journal | `local_cache.rs:63-93`, `:131-137`, `:105-118` |
| `journal.rs` tests: `LINE_A`, helpers `entry` (`:168`), `labels` (`:266`); `a_whole_line_that_is_not_canonical_is_refused_naming_the_file` (`:309`) | `journal.rs:150-343` |
| `local_cache.rs` tests: helpers `workspace()`, `label(name)`, `row(name)`, `written(…)`, `entries(layout)` (`:254`), `line(name, row)` (`:438`) | `local_cache.rs:158-260`, `:438` |
| `Build::record` makes the outcome — `store(…)` for `Ended::Produced`, `failed(…)` for `Ended::Failed` — then calls `self.finish(node, progress, outcome)`, which concludes it (log append, then report) | `crates/buildl-core/src/execute/assembly.rs:442-476`, `:481-496`, `:524-546` |
| `Build` holds `cache: &'a mut P::Cache`; `performed.key` is the `ActionKey` the action ran under | `assembly.rs:316-317`, `:450-452` |
| `NOT_STORED` is `OutcomeClass::InfraFailure { kind: InfraKind::Store }`; `OutcomeClass::succeeded` is true only for `Cached` and `Built` | `crates/buildl-core/src/execute/perform.rs:185-187`; `types/outcome.rs:100-107` |
| `store`'s port failure becomes `unstored(label, &error, printed)` — class `NOT_STORED`, diagnostic the error's text | `crates/buildl-core/src/execute/record.rs:150-178`, `:84-93` |
| `FakeCache { rows, failing, recorded }`; `failing` labels fail `record`; `FakeCas::fail_place(path)` makes placing at `path` fail | `crates/buildl-core/tests/flows/common.rs:268-305`, `:359` |
| A failed action leaves no row and runs again next time (design §8.7 "Failures are never cached") | `docs/design.md:368` |

## Files that change

```
crates/buildl-core/src/ports/action_cache.rs     — [modify] ActionCache::forget
crates/buildl-core/src/ports/bundle.rs           — [modify] the doc example implements forget
crates/buildl-core/src/ports/port_set.rs         — [modify] its Stub implements forget
crates/buildl-core/tests/flows/common.rs         — [modify] FakeCache implements forget, keeps what it forgot, can fail it
crates/buildl/src/cache/journal.rs               — [modify] Entry.row is Option<CacheRow>; tests
crates/buildl/src/cache/local_cache.rs           — [modify] forget; replay applies null as removal; tests
crates/buildl/src/report/plan_text.rs            — [modify] its Rows implements forget
crates/buildl-core/src/execute/record.rs         — [modify] forget_contradicted; tests with a test-only cache
crates/buildl-core/src/execute/assembly.rs       — [modify] Build::record calls it
crates/buildl-core/tests/flows/build.rs          — [modify] two flows
```

## Order of work

### Task 1 — The action cache can forget a row

**Files:** `ports/action_cache.rs`, `ports/bundle.rs`, `ports/port_set.rs`,
`tests/flows/common.rs` (`crates/buildl-core`); `cache/journal.rs`, `cache/local_cache.rs`,
`report/plan_text.rs` (`crates/buildl/src`).

**Change:**
- `ActionCache::forget(&mut self, label: &Label) -> Result<()>`, documented: removes the row of
  the target `label` names, durably; no row is no change. Once it returns `Ok`, `row` returns
  `None` and a build that dies afterwards does not bring the row back; after `Err`, `row` returns
  what it returned before. `# Errors`: `Error::Adapter` naming `PortName::ActionCache` when the
  removal cannot be made durable.
- `Entry.row` becomes `Option<CacheRow>`, documented "The row recorded for it; `None` when its row
  was forgotten." The journal's module doc says a line holds a row or the forgetting of one.
- `LocalCache::forget`: a label with no row appends nothing and returns `Ok`. Otherwise it appends
  `Entry { label, row: None }` — the line `{"label":"//:t","row":null}` — and only then removes
  the row from memory and sets `unfolded`. `record` appends `Some(row)`. `open` applies a `None`
  entry as a removal. `cache.json`'s format is unchanged. The struct and module docs mention the
  forgetting.
- `FakeCache` gains a public `unforgettable: BTreeSet<Label>` (labels whose `forget` fails with an
  `Adapter` error naming `ActionCache`) and a `forgotten()` accessor listing labels whose row was
  removed, in order; `forget` of a label with no row changes nothing and records nothing.
- `Nothing`, `Stub` and `Rows` implement `forget` as removal or `Ok(())`, matching what each does
  for `record`.

**Proof:**
- `a_forget_line_is_canonical` in `journal.rs` — appending `Entry { label: //:a, row: None }`
  writes exactly `{"label":"//:a","row":null}\n`, and `Journal::read` gives it back with `row`
  `None`.
- `a_forget_line_out_of_form_is_corrupt` in `journal.rs` — a journal holding
  `{"label":"//:a"}\n`, and one holding `{"row":null,"label":"//:a"}\n`, are each refused as
  `Error::CorruptState` naming the journal.
- `a_forgotten_row_is_gone_after_fold` in `local_cache.rs` — record `//:a` and `//:b`, forget
  `//:a`, fold: `row(//:a)` is `None`, and `cache.json` holds exactly the bytes of a snapshot of
  `//:b` alone.
- `a_forgotten_row_is_gone_after_reopen_from_the_journal` — record `//:a` and fold; record
  `//:b`, forget `//:a`, do not fold; `LocalCache::open` on the same layout has no row for
  `//:a` and the row of `//:b`.
- `after_a_failed_append_no_row_is_forgotten` — record `//:a`, make the journal unappendable;
  `forget(//:a)` is `Err` and `row(//:a)` still returns the row, here and after a reopen.
- `forgetting_a_label_with_no_row_writes_nothing` — on a fresh cache, `forget(//:a)` is `Ok`,
  no journal file exists afterwards (`entries(layout)` unchanged) and a `fold` leaves no
  `cache.json`.

**Done when:** `cargo test -p buildl --lib cache::` passes and the Definition of Done is green.
Commit `feat(repo): the action cache can forget a row`.

### Task 2 — A failure under the recorded key forgets the row

**Files:** `execute/record.rs`, `execute/assembly.rs`, `tests/flows/build.rs` (all
`crates/buildl-core`).

**Change:**
- `pub(crate) fn forget_contradicted<A: ActionCache>(outcome: ActionOutcome, key: &ActionKey,
  cache: &mut A) -> ActionOutcome`: when `outcome.class.succeeded()` is false and
  `cache.row(&outcome.label)` holds a row whose key digest equals `key.digest()`, calls
  `cache.forget(&outcome.label)`. A forget that returns `Err(error)` gives back the outcome with
  class `NOT_STORED` and diagnostic `error`'s text, everything else kept, as `store` does for a
  failed `record`; an outcome that is already `NOT_STORED` comes back unchanged when its forget
  fails, so the diagnostic of the first failure is kept. Otherwise the outcome is returned
  unchanged. Its rustdoc says why: a row is
  the claim that this key passes, and this run of the same key just failed — under `--rerun`,
  `always = true`, or a row that stood and could not be restored.
- `record.rs`'s module doc names it among the steps that reach a port.
- `Build::record` passes the outcome it made through `forget_contradicted(outcome, &key,
  self.cache)` before `self.finish(…)`, passing a clone of `performed.key` taken before `store`
  consumes it (`ActionKey` derives `Clone`, `types/action_key.rs:145`). `Build::drain` (cached targets, `restore`) and `Build::skip` do not
  call it.

**Proof** (in `record.rs`'s tests, over a test-only `Rows` cache holding a map and a set of labels
whose `forget` fails):
- `a_failure_under_the_recorded_key_forgets_the_row` — row of `//:a` under key `K`; a `failed`
  outcome (`ActionFailure`) with key `K` comes back unchanged and the row is gone.
- `a_failure_under_another_key_keeps_the_row` — row under `K`; the same failure under key `K2`
  keeps the row.
- `an_output_missing_under_the_recorded_key_forgets_the_row` — an `OutputMissing` outcome under
  `K` forgets the row.
- `a_forget_that_fails_is_a_store_failure` — row under `K`, `forget` failing: the outcome comes
  back as `NOT_STORED` with the cache's message as diagnostic and its `stdout`/`stderr` kept.
- `a_failed_forget_keeps_an_earlier_store_failure` — row under `K`, an outcome already
  `NOT_STORED` with a diagnostic, `forget` failing: the outcome comes back unchanged, diagnostic
  included, and the row stays.
- `a_success_forgets_nothing` — a `built` outcome under `K` keeps the row.

(in `tests/flows/build.rs`):
- `an_unplaced_cached_target_forgets_nothing` — build `//:a` once; `fail_place` on its output
  path; build again: `//:a` is the store's infrastructure failure, and `cache.forgotten()` is
  empty and its row stays.
- `a_failed_always_target_forgets_the_row_it_left` — a target with `freshness = Always` whose
  script exits 0, built once (row recorded); its script then exits 1; built again: it fails,
  `cache.forgotten()` is `[//:a]` and it has no row.
- `a_forget_that_fails_is_a_store_failure` — the same target, built once passing, then scripted
  to exit 1 with its label in `FakeCache.unforgettable`; built again: its outcome is the store's
  infrastructure failure, `cache.forgotten()` is empty and its row stays.

**Done when:** `cargo test -p buildl-core --lib execute::record` and
`cargo test -p buildl-core --test flows` pass and the Definition of Done is green. Commit
`feat(buildl-core): a failure under the recorded key forgets the row`.

## Risks

- **The journal format changes** (spec §7). A journal written now can hold a line an older
  buildl refuses with exit 6; only a downgrade across a crashed build meets it, and design §3's
  remedy (remove it, build again) applies. Plan `10` writes this into design §3.
- **Most risky step:** Task 2's comparison must use the key the action actually ran under
  (`performed.key`), not the row's or the plan's; the two "key" tests pin that.
- A failure after a key change keeps the old row on purpose, so reverting an edit is still a hit.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.

## Review findings

- risk — a forget that failed overwrote the diagnostic of an outcome that was already the store's failure (a forced run whose `cache.record` failed to append), losing the real I/O cause — `execute/record.rs` — fixed: such an outcome comes back unchanged; `a_failed_forget_keeps_an_earlier_store_failure`, seen failing first; Task 2 amended
- risk — nothing guarded `LocalCache::forget`'s append-before-remove order, or "after `Err`, `row` returns what it returned before" — `cache/local_cache.rs` — fixed: `after_a_failed_append_no_row_is_forgotten`, seen failing with the two steps swapped; Task 1 amended
- nit — `FakeCache.unforgettable` was used by no flow — `tests/flows/build.rs` — fixed: `a_forget_that_fails_is_a_store_failure`; Task 2 amended
- nit — stale or over-wide docs: `LocalCache::open`, `fold`, `record`, `Journal`; `record` read as if it might not insert — `cache/local_cache.rs`, `cache/journal.rs` — fixed
- question — an infrastructure failure (`Prepare`, `Run`, `TimedOut`, `Killed`) also forgets a row it did not contradict — kept: spec §3.5 forgets on any failure class, and the cost is one more run, never a wrong pass
- nit — `Build::record` clones the whole `ActionKey` to compare a digest — kept: the signature is this plan's, and the clone is one per performed action
- nit — spec §6 still lists the forget tests under `execute/assembly.rs` — kept: this plan's "Where the decision's tests live" records the move

## Probe results

- No throwaway probe: every claim is under a named test. The swap of append and removal in `LocalCache::forget` served as the reversion probe for the order.

## Deviations

- `an_unplaced_cached_target_forgets_nothing` and `a_forget_that_fails_is_a_store_failure` passed on their first run: each guards behaviour no change of this plan could break before it was written.
