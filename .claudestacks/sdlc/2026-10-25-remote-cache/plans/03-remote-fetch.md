---
status: done
created: 2026-10-25
depends-on: [02]
built-on: 3d9113128fe5
---

# Remote Fetch Implementation Plan

**Goal:** A target the local cache cannot serve is fetched from the remote cache when the remote holds a usable result for it.

**Architecture:** The remote is consulted on the worker, inside `perform` (`execute/perform.rs:326`),
before the action runs (spec §3, P20). `perform` gains one argument, `remote:
Option<RemoteRun<'_, R>>` — the port, the `RemotePolicy`, the workspace's `Execution` and a
`down: &RemoteSwitch` (`types/remote_switch.rs`: `turn_off()` is one compare-exchange, true only
for the caller that flipped it) — so `verify`, which calls `perform` too (`verify/assembly.rs:330`), passes `None`
and never reaches the remote (spec §6). A hit is downloaded and checked by a new
`execute/fetch.rs` (`fetch` → `Checked`), then kept in the store by `Checked::keep` — a store
failure there is the run's infrastructure `Store` failure, never a remote failure — and comes back as `Ended::Fetched`, which the scheduler records through the
same `store` as `Ended::Produced` (`record.rs:176-199`, P21), through
`Finished { made: Made::Fetched, … }` (`Made` is `Built` or `Fetched`, so no other class reaches it). A
remote failure becomes a `Told::Remote` notice the scheduler reports (only the scheduling
thread calls the reporter); under `Strict` it is `Ended::RemoteFailed`, the build's fatal
error. The policy reaches `execute` through `RunChoices.remote` (`types/run_choices.rs`), set by
`Pipeline::set_remote(policy)` (default `RemotePolicy::OFF`, kept for every later build and
test) — a new parameter on `build`/`test`/`run`/`execute` would pass clippy's
`too_many_arguments` limit. `build` and `test` each create one `RemoteSwitch` and hand it to both
`execute` passes (discovery, then main), so a remote turned off in the first pass stays off for
the rest of the command (spec §6) and gives one warning per command.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `buildl-cli`, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §3 (hit, usable result, digest checks, not reported as started),
§4, §6 (lenient and strict for lookup and download, one warning, summary, event log, `verify`
never connects), §10 (core rows: fetch, early cutoff, keys), P10, P21, P23.

---

## Files that change

```
crates/buildl-core/src/execute/fetch.rs        — [create] usable(): check a RemoteHit; fetch(): download, check, store; unit tests
crates/buildl-core/src/execute/perform.rs      — [modify] RemoteRun; perform's `remote` argument; Ended::Fetched, Ended::RemoteFailed; Told::Remote
crates/buildl-core/src/execute/assembly.rs     — [modify] execute takes RemotePolicy; builds RemoteRun; Ended::Fetched arm; Told::Remote → reporter; RemoteFailed → fatal
crates/buildl-core/src/execute/record.rs       — [modify] fetched() outcome; store takes the class it records
crates/buildl-core/src/execute/mod.rs          — [modify] declare fetch
crates/buildl-core/src/verify/assembly.rs      — [modify] perform(…, None) (:330); the Ended match (:166) gains the two variants, unreachable with no remote, mapped to an infrastructure check failure
crates/buildl-core/src/types/outcome.rs        — [modify] OutcomeClass::Fetched; Tally.fetched; succeeded() true for it; its test literals
crates/buildl-core/src/ports/reporter.rs       — [modify] Reporter::remote(&RemoteNotice); the &mut R blanket (:67), the test impl (:109), the Tally literal (:168)
crates/buildl-core/src/ports/bundle.rs         — [modify] the doc example's `impl Reporter for Nothing` (:222) gains `remote`
crates/buildl-core/src/ports/port_set.rs       — [modify] the Stub reporter (:284)
crates/buildl-core/src/pipeline/driver.rs      — [modify] build, test and run (:306) take RemotePolicy and pass it to execute
crates/buildl-core/tests/flows/common.rs       — [modify] FakeRemote (scripted, records calls); FakeReporter (:1058) records notices; FakePorts uses FakeRemote; Tally literal (:1825)
crates/buildl-core/tests/flows/{build,discovery,report}.rs — [modify] Tally literals gain `fetched: 0`
crates/buildl-core/tests/flows/remote.rs       — [create] the flow tests below
crates/buildl-core/tests/flows/main.rs         — [modify] mod remote
crates/buildl/src/report/block.rs              — [modify] failure_text (:136-156): Fetched is no failure; the header marks a fetched target ` (fetched)` as it marks ` (cached)` (:214-216); Tally literal (:279)
crates/buildl/src/report/log_text.rs           — [modify] ended (:65-72): Fetched → "fetched"
crates/buildl/src/report/terminal.rs           — [modify] Reporter::remote prints one warning line; the summary counts fetched; `fetched` handled like `cached` (:154); Tally literals
crates/buildl/src/report/silent.rs             — [modify] Reporter::remote does nothing; Tally literal (:89)
crates/buildl/src/workspace.rs                 — [modify] build (:353) and test (:545) pass RemotePolicy::OFF; the Told test reporter (:692); Tally literals (:1029, :1038)
crates/buildl/tests/e2e/failure.rs             — [modify] Tally literals (:32, :63)
crates/buildl-cli/src/exit.rs                  — [modify] of_class (:47-56): Fetched is a success
```

The reporter impls are every hit of `grep -rn "impl.*Reporter for" crates/` plus the doc example
`ports/bundle.rs:222` (control: the grep finds `TerminalReporter`, `terminal.rs:202`). The
`Tally` literals are every hit of `grep -rn "Tally {" crates/` that is not the definition. The
`_` arms over `OutcomeClass` that a new variant falls into silently (it is `#[non_exhaustive]`,
`outcome.rs:64`) are `block.rs:154` and `log_text.rs:70`; both are listed.

## Order of work

### Task 1 — `OutcomeClass::Fetched`

**Files:** `types/outcome.rs`, `buildl-cli/src/exit.rs`, `report/block.rs`, `report/log_text.rs`,
`report/terminal.rs` (class display and summary), `execute/record.rs` (`fetched()` and the
class parameter of `store`), every `Tally` literal listed above.

**Change:** a new variant `Fetched` ("its result was fetched from the remote cache; its outputs
are in place"), serialized as its name like `Cached`; `succeeded()` true; `Tally` gains
`fetched`, counted in `Tally::of` (`outcome.rs:154-160`); the terminal summary prints the
fetched count beside built and cached; `failure_text` returns `None` for it and the block
header marks it ` (fetched)`; `buildl log` words it `fetched`; `of_class` treats it as a
success; `store` records what `Finished.made` says, `Made::Built` for a run and
`Made::Fetched` for a hit.

**Proof:** written first, seen failing, then passing —
- `fetched_is_a_success_whose_outputs_are_in_place` — `succeeded()` true (whose doc defines success as outputs in place; there is no separate `outputs_in_place`)
- `fetched_serializes_as_its_name` — canonical JSON is `"Fetched"` and reads back equal
- `a_tally_counts_fetched_targets` — two fetched, one built → `fetched: 2, built: 1`
- `the_summary_line_counts_fetched` (terminal.rs) — the line names the fetched count
- `a_fetched_target_has_no_failure_trailer` (block.rs) — `trailer` is empty; the header ends ` (fetched)`
- `the_log_words_a_fetched_run_fetched` (log_text.rs)
- `store_records_the_class_it_is_given` — `Fetched` in, `Fetched` out, row recorded

**Done when:** `cargo make dod` green. Commit `feat(buildl-core): a fetched outcome class`.

### Task 2 — Checking a hit

**Files:** `execute/fetch.rs`, `execute/mod.rs`.

**Change:** `usable(hit, outputs, role)` decides whether a `RemoteHit` may stand for a task:
`exit_code` 0; every declared output path exactly once across `files` and `directories`, no
other path; `symlinks` false; a discovery target's output a file (P23). `fetch(hit, task,
remote, cas)` then downloads in two rounds through `remote.download` (spec §5): first stdout,
stderr and each directory's `Tree` blob; then, once each `Tree` is checked and decoded through
`remote.decode_tree` (`None` → unusable), every file blob. Each blob is checked (length = size,
SHA-256 = hash) before anything is put. Then every blob goes into `cas`, each output's
`OutputManifest` is built (`File` or `Tree` from the decoded entries, with modes) and put, and
the `Kept` outputs and `Printed` are returned. A hit with no stdout or stderr digest stands for
empty content. Any check failing is a remote failure naming what failed; nothing unchecked
reaches the store.

**Proof:** written first, seen failing, then passing (unit, with a scripted fake) —
- `a_hit_with_every_output_once_is_usable` — `usable` is true
- `a_non_zero_exit_is_unusable`
- `a_missing_or_extra_output_is_unusable` — one missing, one extra: both false
- `an_output_symlink_is_unusable`
- `a_directory_for_a_discovery_output_is_unusable`
- `a_blob_whose_hash_differs_is_refused_before_anything_is_stored` — the store holds none of the hit's blobs afterwards
- `a_blob_whose_length_differs_is_refused`
- `a_tree_blob_is_checked_like_any_other` — a tampered `Tree` blob is refused before `decode_tree` is called
- `a_tree_holding_a_symlink_is_unusable` — `decode_tree` → `None`
- `file_blobs_are_asked_for_only_after_their_tree` — the fake's download calls come in two rounds
- `a_fetched_tree_has_the_manifest_a_local_run_would_store` — the same digest as the manifest `collected` (`perform.rs:210`) builds for the same files

**Done when:** `cargo test -p buildl-core execute::fetch` passes; `cargo make dod` green.
Commit `feat(buildl-core): check and fetch a remote hit`.

### Task 3 — The worker consults the remote

**Files:** `execute/perform.rs`, `execute/assembly.rs`, `verify/assembly.rs`,
`ports/reporter.rs`, `ports/bundle.rs`, `ports/port_set.rs`, `pipeline/driver.rs`,
`report/terminal.rs`, `report/silent.rs`, `workspace.rs`, `tests/flows/common.rs`,
`tests/flows/remote.rs`, `tests/flows/main.rs`.

**Change:** in `perform`, with `Some(remote)` whose policy reads and whose `down` is not set:
build the task's `RemoteAction` (plan 02), `lookup`; on a hit, `fetch`; a usable hit ends
`Ended::Fetched` with the worker's span and without `Told::Began`, so the target is not shown
as started. A miss, `None`, or `RemoteUse::Off` runs the action as today. A task that cannot
be described for the remote (a dependency manifest missing from the store) is told `Began` and
ends as an infrastructure `Store` failure, as any run's store failure does. A failure (an `Err`
from the port, a failed download, a digest mismatch, or an unusable hit): under `Lenient`, the
worker whose `RemoteSwitch::turn_off()` returns true sends the one `Told::Remote(notice)` — any other worker failing at
the same moment sends nothing — and the action runs; under `Strict`, `Ended::RemoteFailed(notice)`,
which the scheduler turns into the build's fatal `Error::Adapter` naming `PortName::RemoteCache`
and the call. The scheduler records `Ended::Fetched` through `store` with class `Fetched` and
reports each notice through `Reporter::remote` (terminal: one warning line naming the call and
the adapter's diagnostic; silent: nothing). `verify` passes `None`; its `Ended` match maps the
two new variants, which `None` never produces, to its infrastructure check failure.

**Proof:** written first, seen failing, then passing (`tests/flows/remote.rs` with `FakeRemote`) —
- `a_target_the_remote_holds_is_fetched_not_run` — strategy never called; outcome `Fetched`; outputs in place; row written
- `the_row_is_written_only_after_the_content` — when the cache's `record` runs, every blob of the hit is held
- `a_miss_runs_the_action` — outcome `Built`, strategy called once
- `the_off_policy_never_calls_the_remote` — the fake records no call
- `a_fetched_target_is_not_reported_as_started` — the reporter sees `finished`, never `started`
- `a_lookup_failure_warns_once_and_turns_the_remote_off` — four independent targets, every lookup failing: exactly one notice; one lookup; all four built (buildl-core's `clippy.toml` bans `std::thread`, so the flow runs on one thread)
- `RemoteSwitch` races — 16 threads behind a `Barrier`, 200 rounds, exactly one told it turned the remote off (`crates/buildl/tests/e2e/remote_switch.rs`, where threads are allowed)
- a store failure while keeping a hit is a `Store` failure, not a remote notice (lenient and strict)
- a failed download, and a tampered blob, each warn once and build locally; nothing tampered is stored
- a workspace with a discovery target and a failing remote: one notice and one lookup across both passes; the next command asks again
- `a_strict_lookup_failure_fails_the_build` — fatal `Error::Adapter` naming the remote cache and `Lookup`
- `an_unusable_hit_is_a_remote_failure` — one notice; the target built
- `a_fetched_dependency_gives_dependents_the_keys_a_local_build_would` — early cutoff: the dependent's finished key equals the one a fully local build gives
- `a_graph_the_remote_holds_entirely_runs_nothing` — every outcome `Fetched`, strategy never called
- `remote_on_or_off_gives_identical_keys_and_rows` — `Off` against `Read` with an empty fake: byte-identical keys and canonical rows
- `the_log_records_fetched_with_its_key_and_span`
- `verify_never_calls_the_remote` (in `tests/flows/verify.rs` style, here) — a verify run leaves the fake with no call

**Done when:** `cargo test -p buildl-core --test flows remote` passes; `cargo make dod` and
`cargo make guard-module-edges` green. Commit `feat(buildl-core): fetch from the remote cache on the workers`.

## Risks

- **Most risky: Task 3's change to `perform`**, which every build and every verify runs. With
  `None` or `RemotePolicy::OFF` the path must be exactly today's; the existing flow and e2e
  suites passing unchanged is the check.
- Downloads are held in memory blob by blob, as the content store's `put(&[u8])` already
  requires (P22); a very large output costs its size in memory, as a local run's does.
- Discovery targets run through the same `execute` (`discovery/passes.rs`), so they are fetched
  too; Task 2's discovery check keeps a remote directory from standing for one.

## Proof

`cargo make dod`, `cargo make guard-module-edges` and `cargo deny check` green; the flow tests
show a fully remote-held graph building with no action run, the same graph with the remote off
producing identical keys and rows, and `verify` never touching the remote.

## Review findings

- correctness — a local store failure while keeping a hit was reported as a remote `Download` failure (turned the remote off; strict named the remote cache) — `execute/fetch.rs:190`, `perform.rs:457` — fixed: `fetch` → `Checked`, `Checked::keep` stores; store errors are the run's `Store` failure; flow tests for lenient and strict; verified by the fix coder's single `cargo make dod` run, exit 0
- correctness — a task that could not be described for the remote ended `InfraFailure{Store}` without `Told::Began` — `perform.rs:448` — fixed, unit test; same run
- unguarded fix — the warn-once race had no concurrent test — `perform.rs:698` — fixed: `RemoteSwitch::turn_off` (one compare-exchange) and `crates/buildl/tests/e2e/remote_switch.rs` (16 threads, 200 rounds); same run
- strong-types — `Finished.class` accepted any of nine classes, `_ =>` recorded them as `Built` — `record.rs:183,220` — fixed: `Made { Built, Fetched }`; same run
- doc-comment-discipline — edited doc lines past 100 columns — `assembly.rs`, `record.rs`, `reporter.rs`, `block.rs`, `log_text.rs` — fixed
- style — split import — `assembly.rs:35` — fixed
- test-coverage — `FakeRemote::download_fails` and `tampered` unused by any flow test — `tests/flows/common.rs` — fixed: two flow tests; same run
- question → correctness — `down` lived per `execute` pass, so a discovery workspace re-enabled a dead remote and warned twice (spec §6: off for the rest of the build) — `assembly.rs:151` — fixed: one `RemoteSwitch` per build/test command, flow tests for one notice across passes and a fresh switch per command; same run
- temporal coupling — `Pipeline::set_remote` persists across calls — `pipeline/driver.rs:78` — kept; rustdoc now says it applies to every later build and test until changed
- spec — five deviations not written back; plan 08 Task 2 said "plan 03's parameter" — fixed: plans 03 and 08 amended
- spec — `outputs_in_place` named a method that does not exist — fixed in the plan text
- spec — Task 2 commit subject differs and landed before Task 1 — plan text amended to the subject used
- Fix round gates (fix coder, run once): `cargo make dod` exit 0; `cargo make guard-module-edges` exit 0

## Probe results

- "two workers" warn-once flow test — `crates/buildl-core/clippy.toml` bans `std::thread` and `guard-core-purity` forbids silencing it — AGAINST the plan (Task 3 Proof); the concurrent check moved to the `buildl` crate's e2e tests
- "`execute`/`build`/`test` take a `RemotePolicy`" — clippy `too_many_arguments` (8 > 7) on `test`, `run`, `execute` — AGAINST the plan (Architecture); replaced by `RunChoices` and `Pipeline::set_remote`
- "`store` takes the class it records" — clippy `too_many_arguments` — AGAINST the plan (Task 1); replaced by `Finished { made }`

## Deviations

- 2026-10-25 — Architecture: the policy travels in `RunChoices.remote` set by `Pipeline::set_remote`; plan 08 Task 2 amended to call it.
- 2026-10-25 — Task 1: `store` takes `Finished { key, made, outputs, printed }`; the test summary reads `N passed (X cached, Y fetched)`.
- 2026-10-25 — Task 3: `down` is a `RemoteSwitch` shared by both passes of one command; fetch split into `fetch` (remote) and `Checked::keep` (store).
- 2026-10-25 — Tasks 1 and 2 ran in parallel; Task 2's commit (`a6caf3b`) landed before Task 1's (`90aa7ed`).
- 2026-10-25 — one reviewer pass over the three tasks, reading the diff only (the user asked that gates run once, by the coder), then one fix commit (`42b9c19`).
