---
status: done
created: 2026-10-26
depends-on: [02, 03]
built-on: 0e431e5caec9
---

# Test Server Execution Implementation Plan

**Goal:** The in-memory test server runs the actions it is asked to execute, answering through the Execution service as the Remote Execution API describes.

**Architecture:** `remote/test_server.rs` (behind the `test-server` feature) serves
Capabilities, ActionCache, CAS and ByteStream today (`:203-206`). It gains the Execution
service from the generated `execution_server` (`generated/reapi/build.bazel.remote.execution.v2.rs:2589`,
compiled for the feature by `tools/reapi-codegen/src/main.rs:33-35`). `Execute` reads the
`Action`, `Command` and input-root `Directory` tree from the server's own CAS, writes the files
into a fresh temporary directory, runs `Command.arguments` there with
`Command.environment_variables` and nothing of the host's environment, reads each of
`Command.output_paths`, stores what it finds, keeps the `ActionResult` in its action cache
(as a REAPI server "MAY" do, `REAPI:663-667`) and streams `Operation`s: `QUEUED`, `EXECUTING`,
then `done` with the `ExecuteResponse`. Switches let a test script each failure the client must
handle (spec §10). Nothing here runs outside tests.

**Tech Stack:** Rust, tonic server, tokio, `tempfile` (already a `buildl` dependency), `std::process`.

**Content authority:** spec §10 (the test server's Execution service and its switches), §4 (what the client must handle).

---

## Files that change

```
crates/buildl/src/remote/vocabulary.rs   — [modify] Method (:18-35) gains Execute, WaitExecution
crates/buildl/src/remote/test_server.rs  — [modify] ServerOptions (:115) gains exec: Exec (Enabled/Disabled); Execution service impl; switches; tests (~:738)
crates/buildl/src/remote/test_execution.rs — [create] running one action from the CAS in a temporary directory, used by the test server (a sibling file, not a child module, so test_server.rs need not become export-only; behind the test-server feature)
crates/buildl/src/remote/mod.rs           — [modify] declare test_execution under the test-server feature
```

## Order of work

### Task 1 — `Execute` runs an action

**Files:** `vocabulary.rs`, `test_server.rs`, `test_execution.rs`, `mod.rs`.

**Change:** the capabilities answer (`:428-445`) gains `execution_capabilities` with
`exec_enabled` from `ServerOptions.exec` (default enabled) and `digest_functions = [SHA256]`.
`Execute`: with `skip_cache_lookup` false and a result held for the action, answer `done` at
once with `cached_result = true`. Otherwise run the action as described above; a missing
`Action`, `Command`, `Directory` or file blob is `FAILED_PRECONDITION` with a
`google.rpc.PreconditionFailure` detail (plan 02) listing each as a `MISSING` violation with
subject `blobs/{hash}/{size}` (`REAPI:103-112`). An action that runs past `Action.timeout` is
killed and answered with `status` `DEADLINE_EXCEEDED` and its logs (`REAPI:1672-1673`). Each
operation gets a name; `WaitExecution(name)` streams the rest of a held operation, or answers
`NOT_FOUND` for a name it does not hold.

**Proof:** written first, seen failing, then passing (`test_server.rs` tests) —
- `an_action_is_run_and_its_outputs_stored` — a `Command` `sh -c 'printf hi > out'` with `out` in `output_paths`: the response's `ActionResult` names `out` with the digest of `hi`, and the blob is in the CAS
- `the_action_sees_only_its_environment` — with `FOO=1` in the command, `sh -c 'env > out'` lists `FOO=1` and no host `HOME` or `PATH`; it does not assert an exact list, since `sh` adds `PWD`, `SHLVL` and `_` itself (probe: `env -i FOO=1 /bin/sh -c env` printed `FOO=1`, `PWD=…`, `SHLVL=1`, `_=/usr/bin/env`)
- `an_action_with_a_non_zero_exit_reports_it`
- `a_held_result_is_answered_from_the_cache` — the same action twice: the second answer has `cached_result = true`; with `skip_cache_lookup` it runs again
- `a_missing_input_is_a_precondition_failure_naming_it`
- `an_action_past_its_timeout_is_deadline_exceeded`
- `the_stream_reports_queued_executing_then_done`
- `wait_execution_finishes_a_held_operation` and `wait_execution_of_an_unknown_name_is_not_found`
- `capabilities_say_whether_execution_is_enabled`

**Done when:** `cargo test -p buildl --features test-server remote::test_server` passes;
`cargo make dod` green. Commit `test(buildl): the test server executes actions`.

### Task 2 — Switches for the failures the client handles

**Files:** `test_server.rs`.

**Change:** switches, each a method on `TestServer` beside `fail` (`:244`):
- `break_streams(n)` — the next `n` streams, `Execute` or `WaitExecution`, each end after their
  first message, before `done`
- `forget_operations()` — `WaitExecution` answers `NOT_FOUND` for every name
- `report_missing_once(digest)` — the next `Execute` answers `FAILED_PRECONDITION`/`MISSING` for that blob, as a call error, and forgets the blob so it must be uploaded again
- `report_missing_in_status_once(digest)` — the same, but as `done` with an `ExecuteResponse`
  whose `status` is `FAILED_PRECONDITION` carrying the `PreconditionFailure` detail
  (`REAPI:81-84`: errors while running are reported in that field)
- `status_in_response(fault, n)` — the next `n` `ExecuteResponse`s carry `status` with the code of `fault` (buildl's `Fault`, so no tonic type reaches a caller) and no result
- `report_violation_once(kind, digest)` — the next `Execute` answers `FAILED_PRECONDITION` with one violation of type `kind` for that blob, as a call error; the blob is kept
- `hold_execution(Duration)` — each action waits this long before it runs, the stream sending nothing meanwhile
- `fail_times(Method::Execute, fault, n)` and `fail(Method::Execute, fault)` from plan 03 apply to `Execute` and `WaitExecution` as to any call
- `calls_to(Method::Execute)` (`:230`) counts executions

**Proof:** one test per switch, each asserting the client-visible answer —
`a_broken_stream_ends_before_done`, `a_wait_execution_stream_can_be_broken_too`,
`forgotten_operations_are_not_found`, `a_blob_reported_missing_once_is_found_after_upload`,
`a_blob_reported_missing_in_the_status_carries_the_detail`, `a_status_in_the_response_is_sent_n_times`,
`a_held_execution_sends_nothing_until_it_runs`, `an_execute_can_be_failed_like_any_call`,
`a_violation_of_another_kind_is_reported_as_given`.

**Done when:** `cargo test -p buildl --features test-server remote::test_server` passes;
`cargo make dod` green. Commit `test(buildl): scripted execution failures in the test server`.

## Risks

- **Most risky step:** running a real process from the test server. Tests use `sh` and `printf`,
  which the macOS and Ubuntu CI runners both have; a Windows runner is not in the matrix
  (`ci.yml:35-36`).
- A held stream needs the server to keep the operation while the client reconnects; keep held
  operations in the server's `State` (`:279`) under its mutex.
- Writing a `Directory` tree needs the same path rules the client's `input_tree.rs` follows;
  a mismatch shows up as a missing file in Task 1's first test.

## Proof

`cargo make dod` green; every test named above runs under `--features test-server` in the `dod`
task (`Makefile.toml:91` runs `--all-features`).

## Review findings

One review over both tasks (`02bdfb5..9551ada`); spec compliant, `cargo make dod` green; blocking: none. All closed in one fix round:
- api — `status_in_response` took `tonic::Code`, a tonic type in the public test API — fixed: it takes `Fault`, which gains `InvalidArgument`, `FailedPrecondition`, `DeadlineExceeded`
- risk — both timeout tests gave the action 800 ms before checking its stdout — fixed: 3 s; the headers-before-body test holds 2 s and allows 1 s for headers
- plan 08 — `a_precondition_failure_with_another_violation_is_a_failure` had no switch to set it up — fixed: `report_violation_once(kind, digest)` with test `a_violation_of_another_kind_is_reported_as_given`; plan 08 amended to name it
- test-coverage — output parent directories, `do_not_cache`, `Execute` on a disabled server had no test — fixed: `the_directories_leading_to_an_output_are_created`, `a_do_not_cache_action_is_run_again_instead_of_answered_from_the_cache`, `execute_on_a_server_with_execution_disabled_is_unimplemented`
- docs — only the direct child is killed on timeout, so a command meant to time out `exec`s its last program — documented on `Job::run`; plan 08's deadline test amended to say so
- style — a private `wire_digest`; `is_relative(path, bool)` split into two named functions; two `impl State` blocks merged; an `Any` encoding failure is an `INTERNAL` error, never a `done` with no result; module doc names `do_not_cache`
- kept — the server runs any command a loopback client sends; it exists only under the `test-server` feature, which only dev-dependencies enable
- kept — no helper stores a non-zero `ActionResult`; plan 08 can store one with `UpdateActionResult` inside the crate

Gates after the fix: `cargo test -p buildl --features test-server remote::test_` → 50 passed; `cargo make dod` → Build Done; `cargo make guard-adapter-isolation` → OK. Named tests: `conformance.py … 07` → 9/9 bullet-listed, and every Task 2 name present by `grep -n "fn <name>"`.

## Probe results

- generated server halves — `grep -n "pub mod execution_server" generated/reapi/build.bazel.remote.execution.v2.rs` → `:3233`, under the `test-server` feature; `PreconditionFailure` at `google.rpc.rs:288` — as planned
- `tempfile` is a normal `[dependencies]` entry of `crates/buildl/Cargo.toml` (`:36`) — as planned
- tokio has only `rt`, `rt-multi-thread`, `time` (`Cargo.toml:90`) — so the action runs with `std::process` in `spawn_blocking`, the timeout enforced by `try_wait` and kill; no feature added

## Deviations

- stream shape fixed for the client plan: every stream sends a non-done stage message first (Execute: `QUEUED`, `EXECUTING`, then `done`; a cache hit: `CACHE_CHECK`, then `done`; `WaitExecution`: the current stage, then `done`), so `break_streams` never delivers `done`
- missing inputs at `Execute` are a call error, not a `status` in the response: "Errors discovered during creation of the `Operation` will be reported as gRPC Status errors" (`remote_execution.proto:80-81`)
- `Exec::Disabled` advertises `exec_enabled = false` and answers `Execute` with `UNIMPLEMENTED`
- added `report_violation_once` (review, above)
