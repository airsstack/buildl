---
status: done
created: 2026-10-26
depends-on: [01, 04]
built-on: 0e431e5caec9
disproves: 4704ec60505e
---

# Execute In Core Implementation Plan

**Goal:** A target neither cache serves is executed through the `RemoteCache` port when the run's policy says to execute.

**Architecture:** The port (`buildl-core/src/ports/remote_cache.rs:23`) gains `execute`, and its
answer is a new value, `Executed`, judged entirely in the core, as a fetch is. The worker's
`perform` (`execute/perform.rs:391`) takes one more branch after a lookup miss in `consult`
(`:464`): when the policy executes and the remote is not down, it passes a remote gate and
calls `execute`; otherwise, and on every fallback, it passes a local gate and calls `run`
(`:563`). Both gates are one new type, a counting gate (`Mutex` + `Condvar`, no I/O), so `jobs`
bounds remote jobs and `workers` bounds local runs (spec §7). The branch sits in `consult`,
before `perform` claims the live place (`:425-429`), so a remote job never takes it (spec §9,
`--stream`). A worker that waited at a gate checks the build's stop flag again before it sends
or runs anything (spec §4, "Stopping"). A remote result becomes `Ended::Executed`, which the
calling thread records as `Fetched` is recorded (`assembly.rs:554`), with a new `Made::Remote`
and `OutcomeClass::Remote`. The policy gains the execution choice, and the `bare` refusal covers
it (spec §2). Plan 08 implements the port in the adapter; until then `Reapi::execute` answers
with an adapter error, which no run reaches because nothing turns execution on before plan 09.

**Tech Stack:** Rust, `buildl-core` (plus the literal updates in `buildl` and `buildl-cli`), `cargo make dod`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §1, §4 step 4 (the answer table) and "Stopping", §6, §7, §8, §9 (`--stream`).

---

## Files that change

```
crates/buildl-core/src/types/executed.rs          — [create] Executed: Ran(RemoteHit), Cached(RemoteHit), TimedOut { stdout: Option<Blob>, stderr: Option<Blob> }, InputChanged(RootPath)
crates/buildl-core/src/types/remote_policy.rs     — [modify] RemoteExecution { Off, On { jobs: RemoteJobs, local: NonZeroUsize } }; RemotePolicy gains `execution`; OFF keeps it Off
crates/buildl-core/src/types/remote_hit.rs        — [modify] symlinks: bool (:46) becomes symlinks: Vec<String>, the output paths the server reported as links
crates/buildl-core/src/types/remote_notice.rs     — [modify] RemoteCall (:16) gains Execute, shown "execute"
crates/buildl-core/src/types/outcome.rs           — [modify] OutcomeClass (:66) gains Remote; succeeded (:114) counts it; Tally (:142-154) gains remote; its tests' literals
crates/buildl-core/src/types/mod.rs, lib.rs       — [modify] export Executed, RemoteExecution
crates/buildl-core/src/ports/remote_cache.rs      — [modify] trait RemoteCache gains execute(&RemoteAction) -> Result<Executed>
crates/buildl-core/src/ports/no_remote.rs         — [modify] execute answers an adapter error naming the remote cache
crates/buildl-core/src/ports/reporter.rs          — [modify] Tally literal (:194)
crates/buildl-core/src/execute/gate.rs            — [create] Gate: admits n holders at once; single-threaded unit tests (buildl-core starts no thread, crates/buildl-core/clippy.toml)
crates/buildl-core/src/execute/mod.rs             — [modify] declare gate (crate-private)
crates/buildl-core/src/execute/perform.rs         — [modify] Ended::Executed; RemoteRun gains the gates and executing(); consult takes the execute branch; executed() judges an Executed; RemotePolicy literals in tests (:724-776); tests (:639)
crates/buildl-core/src/execute/fetch.rs           — [modify] usable (:38) reads the symlink list; unusable() says why a hit cannot be placed; stdout/stderr download for a failed run; RemoteHit literals in tests (:408, :477)
crates/buildl-core/src/execute/record.rs          — [modify] Made (:177) gains Remote; store (:222) gives OutcomeClass::Remote
crates/buildl-core/src/execute/assembly.rs        — [modify] record (:541-570) arm for Ended::Executed; RemoteRun literal (:190) builds the gates; ensure_sandboxed_upload (:52) becomes ensure_sandboxed_remote (:173)
crates/buildl-core/src/verify/assembly.rs         — [modify] rechecked (:198-215) gains the Executed arm (unreachable there: verify passes no remote, :367-372)
crates/buildl-core/src/pipeline/driver.rs         — [modify] the sandbox check call (:342) is the renamed one
crates/buildl-core/src/error.rs                   — [modify] UploadUnsandboxed (:414) gains which: Upload or Execute; its Display names the switch
crates/buildl-core/src/execute/upload.rs          — [modify] test fake (:235) implements execute
crates/buildl-core/tests/flows/common.rs          — [modify] FakeRemote (:1171, impl :1244) scripts execute answers and records calls; Tally literal
crates/buildl/tests/e2e/remote_gates.rs           — [create] the gates' limits over the real ThreadPool: the ports of Wiring::ports (lib.rs:211) moved into a PortSet whose remote is a fake that holds each execute until released
crates/buildl/tests/e2e/main.rs                   — [modify] declare remote_gates
crates/buildl-core/tests/flows/remote.rs          — [modify] the four RemotePolicy consts (:33-51) gain execution: Off; new flow tests
crates/buildl-core/tests/flows/build.rs, report.rs, discovery.rs — [modify] Tally literals
crates/buildl-core/src/types/remote_hit.rs (bis) — [modify] DecodedTree: Files, Symlink(path), EmptyDirectory(path), Unreadable — what a decoded output tree holds
crates/buildl-core/src/ports/remote_cache.rs (bis)— [modify] decode_tree returns DecodedTree, so a refused tree is told apart from an unreadable one
docs/architecture.md, docs/architecture-building-blocks.md — [modify] the decode_tree signature; the UploadUnsandboxed { which } error
crates/buildl/src/remote/encode.rs                — [modify] names_symlinks (:293) becomes symlink_paths, returning the paths of all three wire lists, sorted and deduplicated
crates/buildl/src/remote/reapi.rs                 — [modify] impl RemoteCache (:327) gains execute answering an adapter error, replaced in plan 08
crates/buildl/src/remote/configured.rs            — [modify] Remote::Off answers execute with an adapter error; Remote::Reapi delegates; RemotePolicy literal (:119)
crates/buildl/src/report/block.rs, silent.rs, terminal.rs — [modify] Tally literals only (plan 06 changes what they print)
crates/buildl/src/workspace.rs                    — [modify] Tally literals; RemotePolicy literal (:1550)
crates/buildl/tests/e2e/failure.rs                — [modify] Tally literals
crates/buildl/tests/e2e/remote.rs                 — [modify] RemotePolicy literals (:27, :33)
crates/buildl-cli/src/remote_switches.rs          — [modify] RemotePolicy literals (:125, :181) gain execution: Off (plan 09 sets it)
crates/buildl-cli/src/exit.rs                     — [modify] the UploadUnsandboxed mapping (:104) and its tests (:253-262)
```

The literal lists are exhaustive: `grep -rln "Tally {" crates/` gives the eleven files above that
hold a `Tally` literal (`ports/reporter.rs`, `types/outcome.rs`, `tests/flows/build.rs`,
`common.rs`, `discovery.rs`, `report.rs`, `buildl/src/report/block.rs`, `silent.rs`,
`terminal.rs`, `buildl/src/workspace.rs`, `buildl/tests/e2e/failure.rs`); `grep -rn
"RemotePolicy {" crates/` gives the `RemotePolicy` literals at `tests/flows/remote.rs:33-51`,
`execute/perform.rs:724-776`, `buildl-cli/src/remote_switches.rs:125`, `:181`,
`buildl/tests/e2e/remote.rs:27`, `:33`, `buildl/src/workspace.rs:1550`,
`buildl/src/remote/configured.rs:119` (control: it finds the definition at
`types/remote_policy.rs:33`). Every `impl RemoteCache for` (`grep -rn "impl RemoteCache for"
crates/`: `no_remote.rs:23`, `reapi.rs:327`, `configured.rs:56`, `fetch.rs:290`, `upload.rs:235`,
`common.rs:1244`) gains the method. A `RemoteHit` literal is a hit of `grep -rn "symlinks:" crates/`.

## Order of work

### Task 1 — The values and the port

**Files:** `types/executed.rs`, `types/remote_policy.rs`, `types/remote_hit.rs`,
`types/remote_notice.rs`, `types/outcome.rs`, `types/mod.rs`, `lib.rs`, `ports/remote_cache.rs`,
`ports/no_remote.rs`, `ports/reporter.rs`, every `Tally`, `RemotePolicy` and `RemoteHit` literal
listed above, the six `RemoteCache` impls, `encode.rs`.

**Change:** introduce `Executed` (spec §8; `TimedOut`'s logs are `Option<Blob>` because "the
result field is optional" for an error status, `REAPI:1678-1680`) and `RemoteExecution`;
`RemotePolicy::OFF` has `execution: Off`, and every other literal gains `execution: Off`.
`RemoteHit.symlinks` holds the output paths the server reported as links (`encode.rs:293-297`
collects them), so the core can name the output. `RemoteCall::Execute` displays as `execute`.
`OutcomeClass::Remote` is a success (`succeeded`) and is tallied as `remote`; every `Tally`
literal gains `remote: 0`; it serializes as `"Remote"` like the other field-less variants
(`outcome.rs:60-65`). `RemoteCache::execute` is added; `NoRemote`, `Remote::Off` and the stub in
`Reapi` answer an `Error::Adapter` for `PortName::RemoteCache` saying execution is not available;
`Remote::Reapi` delegates.

**Proof:** written first, seen failing, then passing —
- `remote_is_a_success_and_is_tallied` (`outcome.rs` tests, beside `:365`) — `succeeded` is true; a tally of one `Remote` outcome has `remote == 1`
- `remote_serializes_as_its_name` (beside `:370`) — `"Remote"` round-trips
- `the_off_policy_executes_nothing` (`remote_policy.rs` tests, beside `:53`)
- `no_remote_refuses_to_execute` (`no_remote.rs` tests) — an adapter error naming the remote cache
- `execute_is_shown_as_execute` (`remote_notice.rs` tests)
- `a_hit_names_its_symlinked_outputs` (`encode.rs` tests) — an `ActionResult` with one `output_symlinks` entry gives `symlinks == ["out/link"]`
- the existing `an_output_symlink_is_unusable` (`fetch.rs:474`) passes with the list

**Done when:** `cargo make dod` green (the whole workspace compiles with the new fields). Commit
`feat(buildl-core): an execute call on the remote cache port`.

### Task 2 — The counting gate

**Files:** `execute/gate.rs`, `execute/mod.rs`.

**Change:** `Gate::new(n)` admits at most `n` holders; `enter()` blocks until a place is free and
returns a guard that frees it on drop.

**Proof:** in `gate.rs` tests, on one thread (`crates/buildl-core/clippy.toml` bans `std::thread`, and
`guard-core-purity` refuses any exception; the blocking wait is proven over real threads by Task 4's
`buildl` tests) —
- `a_gate_admits_its_count_at_once` — a gate of three gives three places and has none left
- `a_place_is_freed_when_its_holder_leaves` — a gate of one; dropping its place frees it for the next holder
- `a_gate_of_one_serializes`
- `a_gate_of_no_places_admits_one`

**Done when:** `cargo test -p buildl-core gate` passes; `cargo make dod` green. Commit
`feat(buildl-core): a counting gate for runs in flight`.

### Task 3 — Recording a remote result

**Files:** `execute/perform.rs` (`Ended::Executed` only), `execute/record.rs`,
`execute/assembly.rs` (record arm), `verify/assembly.rs`.

**Change:** `Ended::Executed { outputs, printed }`; `Made::Remote`; `store` (`record.rs:222`)
gives `OutcomeClass::Remote` for it; the `Ended` match of `Build::record` (`assembly.rs:541-570`)
moves into `record.rs` as `recorded(target, key, ended, paths, cas, cache)`, so it can be tested
without the flow fakes, and its `Executed` arm mirrors the `Fetched` arm with `Made::Remote`;
`Build::record` calls it. `verify`'s `rechecked` (`verify/assembly.rs:198-215`) gains the arm.
The log event carries class `Remote`, the key, and the span the worker measured. Nothing yet
produces `Ended::Executed`; the tests hand one to `recorded` directly.

**Proof:** written first, seen failing, then passing (`record.rs` tests) —
- `an_executed_result_is_placed_and_recorded_as_remote` — the outcome class is `Remote`, the row is written after the content, as `the_row_is_written_only_after_the_content` (`tests/flows/remote.rs:127`) checks for a fetch
- `an_executed_result_logs_remote_with_its_key_and_span` — mirrors `:500`

**Done when:** `cargo test -p buildl-core` passes; `cargo make dod` green. Commit
`feat(buildl-core): record a target the remote ran`.

### Task 4 — The worker sends a missed target to be executed

**Files:** `execute/perform.rs`, `execute/fetch.rs`, `execute/assembly.rs` (RemoteRun literal),
`tests/flows/common.rs`, `tests/flows/remote.rs`, `crates/buildl/tests/e2e/remote_gates.rs`,
`crates/buildl/tests/e2e/main.rs`; `gate.rs` loses its `dead_code` expectation.

**Change:** `RemoteRun` (`perform.rs:68`) gains `executing()` — the policy's execution is `On`
and the remote is not down — and the two gates, built in `assembly.rs:190` from
`RemoteExecution::On { jobs, local }`; with execution off there is no gate and nothing changes.
After a lookup miss in `consult` (before the live place is claimed, `:425-429`), an executing run
enters the remote gate, checks `shared.stop` again and returns `Worked::NotBegun` if it is set,
says the task began (`Told::Began`, once), and calls `execute(&described)`. The answer is judged
by `sent_to_be_run` and `judged`, row by row of spec §4 step 4 (a path not asked for is a failed call
whatever the exit status; an output directory holding a symlink or an empty directory is refused
with the class a local run gets, never a failed call):

| `Executed` | Ends as |
|---|---|
| `Cached(hit)`, exit 0, usable | as a fetch today: `fetch`, `Checked::keep` → `Ended::Fetched` |
| `Ran(hit)`, exit 0, usable | `fetch`, `keep` → `Ended::Executed { outputs, printed }` |
| `Ran`/`Cached`, exit 0, an output missing, a symlink, a discovery directory | `Ended::Failed` with `OutputMissing { name }` or `OutputRefused { name, refusal }` for the first declared output that fails, the classes `collected` (`:260`) gives; stdout and stderr stored; the remote stays on |
| `Ran`/`Cached`, exit 0, a path the command did not ask for | a failed call (`RemoteCall::Execute`) |
| `Ran`/`Cached`, exit ≠ 0 | stdout and stderr downloaded, checked, stored; `exited(Exit::Exited(status), printed)` (`:210`) |
| `TimedOut { stdout, stderr }` | the logs present stored; `exited(Exit::TimedOut, printed)`, which is `InfraFailure { kind: TimedOut }` as for a local timeout; not run locally |
| `InputChanged(path)` | `Told::Remote(RemoteNotice::InputChanged { … })`, then run locally; the remote stays on |
| `Err(_)` or a blob that fails its check | `failed_call(remote, RemoteCall::Execute, …)` (`:498`): lenient runs locally and turns the remote off; strict ends `RemoteFailed` |

Every local `run` of an executing build enters the local gate first and then checks
`shared.stop` again, ending `Worked::NotBegun` if it is set; a task that already said it began
(a fallback after a failed `execute`) does not say it again. An `Ended::Executed` never reaches
`sent` (`:533`), so it is never uploaded (spec §4). `FakeRemote` scripts an `Executed` per call
and records the calls. After waiting at the remote gate a worker checks again that the remote is
still on, and runs locally without sending when another worker turned it off meanwhile (spec §6).
Each `remote_gates` test builds its own `ThreadPool` of the width it needs; taking the width from
`[execution] workers` and `jobs` is plan 09's. The gates' limits need real threads, which buildl-core cannot start, so
they are proven in `buildl`'s `tests/e2e/remote_gates.rs`: it takes the ports `Wiring::ports`
makes, moves them into a `PortSet` whose `remote` is a fake that holds each `execute` until the
test releases it, and builds through `Pipeline` with the policy's `RemoteExecution::On`.

**Proof:** in `perform.rs` tests (`:639`, beside `:746-911`), `tests/flows/remote.rs`, and, for
the four marked `(threads)`, `crates/buildl/tests/e2e/remote_gates.rs` —
- `a_missed_target_is_executed_when_the_policy_executes` — lookup miss, `Ran` exit 0: `Ended::Executed`, the strategy never called
- `a_missed_target_runs_locally_when_execution_is_off`
- `a_cached_answer_ends_fetched`
- `a_remote_non_zero_exit_is_an_action_failure_with_its_logs`
- `a_remote_timeout_is_a_timeout_and_not_run_locally`
- `a_remote_missing_output_is_output_missing_and_the_remote_stays_on`
- `a_remote_symlink_output_is_refused_naming_it`
- `a_path_not_asked_for_is_a_failed_call`
- `an_input_changed_runs_locally_with_a_notice`
- `a_failed_execute_runs_locally_and_turns_the_remote_off` — lenient; the target is reported as begun once
- `a_strict_failed_execute_fails_the_build`
- `an_executed_target_is_not_uploaded` — under `ReadWrite`, no `upload` call for it
- `a_remote_job_takes_no_live_place` — with live output on, the remote job's `Performed.live` is false and a later local action can take the place
- `remote_jobs_never_exceed_jobs_below_workers` (threads) — `jobs = 2`, `local = 4`, eight targets, each execute held: peak 2
- `remote_jobs_reach_jobs_above_workers` (threads) — `jobs = 6`, `local = 2`, a dispatcher of 6 (`[execution] workers = 6`): peak 6
- `local_runs_never_exceed_workers_while_executing` (threads) — every execute fails leniently, `jobs = 6`, `local = 2`: peak local runs 2
- `a_task_waiting_at_a_gate_when_the_build_stops_is_not_begun` (threads) — `jobs = 1`, the first execute held then answered with an action failure, fail-fast: the waiting targets end not begun and `execute` was called once. A worker already waiting at the remote gate is admitted as the failing job's place is freed, before the build sets stop, so it is sent (spec §4 "Stopping": sent before the build stopped); the test therefore holds the other targets at their lookup until the first result is taken, so they reach the gate only after stop is set
- `a_target_sent_to_be_executed_is_reported_as_started` (flow)
- `an_executed_dependency_gives_dependents_the_keys_a_local_build_would` (flow) — mirrors `:427`

**Done when:** `cargo test -p buildl-core` and `cargo test -p buildl --all-features --test e2e remote_gates` pass; `cargo make dod` green. Commit
`feat(buildl-core): execute a missed target on the remote`.

### Task 5 — Refusing execution under `bare`

**Files:** `execute/assembly.rs` (`:52`, `:173`), `pipeline/driver.rs` (`:342`), `error.rs`,
`buildl-cli/src/exit.rs`.

**Change:** `ensure_sandboxed_upload` becomes `ensure_sandboxed_remote`: a policy that uploads
*or executes* under `Isolation::Bare` is the usage error, before the approver is asked. The error
says which (`--remote-upload` / remote execution) and the line to write, `strategy = "sandbox"`.
It still maps to exit 2 (`exit.rs:104`).

**Proof:** in `tests/flows/remote.rs` beside `:691` —
- `executing_under_the_bare_strategy_is_refused_before_anything_runs`
- `the_refusal_names_execution_and_not_upload`
- `uploading_under_the_bare_strategy_is_refused_before_anything_runs` (`:691`) still passes with its wording
- `exit.rs` tests (`:253-262`) map both to exit 2

**Done when:** `cargo make dod` green. Commit `feat(buildl-core): refuse remote execution under bare`.

## Risks

- **Most risky step:** Task 4's fallback path, where one task can say it began, fail its
  `execute`, wait at the local gate and then run. `a_failed_execute_runs_locally_and_turns_the_remote_off`
  asserts one `Began`, and the stop test asserts nothing starts after a stop.
- A worker blocked on a gate holds a dispatcher thread; with `jobs` far above `workers` and every
  job falling back, threads wait. That is intended (spec §7) and bounded by the dispatcher's width.
- Adding `OutcomeClass::Remote` changes what a log line can hold; an older buildl reading a newer
  log fails on the new name. The log belongs to one checkout's `.buildl/`, so no migration.
- `OutcomeClass` is `#[non_exhaustive]`, so matches outside `buildl-core` use `_` and the compiler
  does not point at them; plan 06 handles each such site.

## Proof

`cargo make dod` and `cargo deny check` green; the named tests pass; the 29 existing tests of
`tests/flows/remote.rs` (`grep -c "#\[test\]"`) pass with their `RemotePolicy` consts gaining
`execution: Off` and no other change.

## Review findings

Task 1:
- docs — `lib.rs` line over the paragraph width; `remote_cache.rs` docs named only lookup and store; an encode comment overstated v2.1 behaviour — fixed
- test-coverage — `Execute` missing from the call-name list test — kept as its own plan-named test `execute_is_shown_as_execute`
- style — one `Tally` literal written `..Tally::default()` — kept: it keeps that test under clippy's line limit
- test-coverage — no test fails if `Remote::Off` or the `Reapi` stub stop refusing `execute` — kept: the stub is replaced by plan 08

Task 2:
- docs — `expect(dead_code)` reason and the poisoned-lock comment were inaccurate — fixed
- test-coverage — the blocking wait has no test in core (core starts no thread) — proven by Task 4's `remote_gates` tests
- strong-types — `Gate::new(usize)` clamps 0 to 1 — kept: the plan names `a_gate_of_no_places_admits_one`

Task 3:
- spec — the `Ended` match moved from `Build::record` into `record.rs::recorded` — plan amended
- test-coverage — the log test echoed its inputs; a hand-built `Kept`; an alias clash — fixed; `cargo test -p buildl-core --lib record` → `38 passed; 0 failed`

Task 4:
- bug — a directory output holding a symlink or an empty directory was a failed call, against spec §4 step 4 row 3 — fixed: `decode_tree` returns `DecodedTree`, and such a tree is `OutputRefused` with the remote on; flow tests `a_remote_directory_holding_a_symlink_is_refused_naming_it`, `a_remote_directory_holding_an_empty_directory_is_refused`
- test-coverage — no test of a blob failing its check on the execute path — fixed: `a_blob_that_fails_its_check_in_an_execute_is_a_failed_call_to_execute`, `a_strict_blob_that_fails_its_check_in_an_execute_fails_the_build`
- test-coverage — no test of the stop check after the local gate, nor of the remote re-check after the remote gate; one test would hang rather than fail if the gates were swapped — fixed: `a_build_that_stops_while_a_task_waits_at_the_local_gate_does_not_begin_it`, three `a_worker_admitted_to_…` tests, `Gate::free()` in the gates test
- flakiness — `local_runs_never_exceed_workers_while_executing` depended on an 80 ms overlap — fixed: local runs held on a latch
- strong-types — `began: bool` — fixed: `Began::{NotYet, Told}`
- docs — `fetch.rs` module and `found_at` docs — fixed
- spec — the stop test's premise (workers waiting at the gate are admitted before stop is set), the remote re-check, the per-test pool width, the judge function's name — plan amended
- kept — `Gate::enter` has no timeout, so a place leak would hang a test

Task 5:
- docs — the two refusal messages worded the fix differently; a stray rustdoc line; an inaccurate sentence in `architecture.md` — fixed
- naming — `UploadUnsandboxed` now also covers execution — kept, as the plan says
- after all fixes: `cargo make dod` → `dod exit 0`, `Build Done in 65.39 seconds`

Named tests: `conformance.py … 05` → `plan 05: 34/34 named tests present`.

## Probe results

- buildl-core may start threads in tests — `crates/buildl-core/clippy.toml` bans `std::thread::{spawn, scope, sleep}` and `guard-core-purity` refuses exceptions — **against the spec and plan**: the four limit tests moved to `crates/buildl/tests/e2e/remote_gates.rs`; spec §10 amended (4a698d2)
- a `buildl` test can build the core with its own remote — `lib.rs:211` exports `Wiring`, `state/mod.rs:35` exports `StateLock`, `port_set.rs:20-52` fields are pub, `driver.rs:71,85` `Pipeline::new`/`set_remote` are pub — as needed
- a worker waiting at the remote gate is admitted before the build sets stop — coder's run of the held-gate test → the waiting task was sent — **against the plan's wording**; the test holds lookups instead

## Deviations

- 2026-10-26 — the gates' limit tests live in `buildl` (spec §10 and plan amended).
- 2026-10-26 — `RemoteCache::decode_tree` returns `DecodedTree`, a port change, so a refused output tree ends as a local run's refusal.
- 2026-10-26 — `recorded()` in `record.rs` holds the `Ended` match.
- 2026-10-26 — `Error::UploadUnsandboxed { which: UnsandboxedRemote }`; a run that both uploads and executes is told about execution.
- 2026-10-26 — `the_refusal_names_execution_and_not_upload` checks `strategy to "sandbox"` after the two refusal messages were worded alike.
