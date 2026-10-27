---
status: executing
created: 2026-10-26
depends-on: [02, 03, 04, 05, 07]
built-on: 75312efc6398
disproves: 0e431e5caec9
---

# REAPI Execute Implementation Plan

**Goal:** `Reapi::execute` runs a `RemoteAction` on the server through the Execution service and answers with an `Executed`.

**Architecture:** On top of the milestone 6 client: the capabilities answer gains the execution
half; the client gains `execute`, which drives one `Execute` stream to its `done` message,
resuming with `WaitExecution` and re-uploading missing blobs; `Reapi::execute`
(`remote/reapi.rs:327`, the stub of plan 05) describes the action, uploads what the server lacks
by reusing the upload's input reading (`read_inputs`, `:266`, and `write`, `:291`) without
outputs, logs or `UpdateActionResult`, calls `execute`, and turns the `ExecuteResponse` into an
`Executed`. Every call goes through the plan 03 `Retry`, and each stream message counts as
progress. The whole of one execution is bounded by `queue_timeout + [execution] timeout`
(spec §4); milestone 6's `[remote] timeout` keeps bounding the connection, unary calls and
ByteStream messages, and not the silence between `Execute` stream messages.

**Tech Stack:** Rust, tonic client, prost, prost-types (`Any`), tokio, `cargo make dod`.

**Content authority:** spec §4 (steps 1–4, wait limit, no upload afterwards), §5 (stream progress resets the count), §6.

---

## Files that change

```
crates/buildl/src/remote/capabilities.rs  — [modify] Capabilities (:22) gains execution: Option<…>; from_response (:34) reads exec_enabled and SHA-256 among execution digest functions; require_execution(); tests (:97)
crates/buildl/src/remote/execution.rs     — [create] the Execute/WaitExecution loop: operation stream, resume, missing blobs, response decoding; unit tests
crates/buildl/src/remote/mod.rs           — [modify] declare execution (private)
crates/buildl/src/remote/client.rs        — [modify] Wire (:75) gains the execution client; Client::execute; the wait-limit deadline
crates/buildl/src/remote/encode.rs        — [modify] executed(): ExecuteResponse → Executed via the existing hit() (:315)
crates/buildl/src/remote/reapi.rs         — [modify] execute replaces the stub: describe, upload inputs, execute, decode; tests (:460)
crates/buildl/src/remote/error.rs         — [modify] RemoteError gains NoExecution (exec_enabled false or no SHA-256) and WaitPassed
```

## Order of work

### Task 1 — The server must be able to execute

**Files:** `capabilities.rs`.

**Change:** `from_response` keeps `execution_capabilities` when present: `exec_enabled`
(`REAPI:2577-2578`) and whether SHA-256 is in `digest_functions`, or in the legacy
`digest_function` when the list is empty (`REAPI:2571-2575`, `:2586-2587`).
`require_execution()` is an error, `RemoteError::NoExecution`, when either is missing; the cache
half is checked as today.

**Proof:** written first, seen failing, then passing (`capabilities.rs` tests) —
- `execution_needs_exec_enabled`
- `execution_needs_sha256_in_its_digest_functions`
- `the_legacy_digest_function_counts_when_the_list_is_empty`
- `a_server_without_execution_capabilities_cannot_execute`
- `the_cache_half_is_checked_as_before` — the existing tests from `:131` pass unchanged

**Done when:** `cargo test -p buildl remote::capabilities` passes; `cargo make dod` green.
Commit `feat(buildl): check that the server can execute`.

### Task 2 — One execution to its end

**Files:** `execution.rs`, `mod.rs`, `client.rs`, `error.rs`.

**Change:** `Client::execute(action_digest, upload_missing)` sends `ExecuteRequest` with the
instance, the digest, `skip_cache_lookup = false` (spec §1) and the SHA-256 digest function, and
reads `Operation`s (`ops.proto:121-156`) until `done`. It owns the one `Retry` budget of the
execution (plan 03), so every retry below counts against `retries` in one place;
`upload_missing` is the caller's callback that sends the blobs a `MISSING` answer names:
- every message is progress for the `Retry` (spec §5), through the progress handle plan 03's
  `Retry::run` hands the operation;
- a stream that ends or breaks before `done` calls `WaitExecution(name)` (`REAPI:122-136`); its
  `NOT_FOUND` calls `Execute` again (`REAPI:132-135`); each uses one retry;
- `FAILED_PRECONDITION` whose `google.rpc.Status` details hold a `PreconditionFailure`
  (`type_url` ending `google.rpc.PreconditionFailure`, plan 02) with only `MISSING` violations
  calls `upload_missing` with the digests and calls `Execute` again (one retry); any other detail
  is a failure (spec §4 step 3; Pants `remote.rs:660-668`). The same holds when the code arrives
  as the `status` of a `done` `ExecuteResponse` (spec §4 step 4);
- `UNAVAILABLE`, `RESOURCE_EXHAUSTED`, `ABORTED` and `CANCELLED` in a `done` response's `status`
  are retried as the same codes from the call are (spec §4 step 4, §5);
- `done` with `response` decodes the `Any` as `ExecuteResponse`; `done` with `error` is a failure
  (`REAPI:84`: the server "MUST NOT set the error field", so it is the server's fault);
- the whole call is bounded by `queue_timeout + timeout` as one `tokio::time::timeout`; passing it
  is `RemoteError::WaitPassed`, named as the `Execute` call.

**Proof:** in `execution.rs` / `client.rs` tests over the plan 07 test server —
- `an_action_runs_to_done` — the response holds the result
- `a_broken_stream_is_resumed_with_wait_execution` — `break_streams(1)`; one `Execute`, one `WaitExecution`, result received
- `a_lost_operation_is_executed_again` — `break_streams(1)` plus `forget_operations()`; two `Execute` calls
- `missing_blobs_are_handed_to_the_upload_callback` — `report_missing_once(d)`; the callback receives `d`, then the execution completes
- `missing_blobs_in_the_status_are_handed_to_the_upload_callback` — `report_missing_in_status_once(d)`; likewise
- `a_precondition_failure_with_another_violation_is_a_failure` — `report_violation_once("UNSUPPORTED", d)` (plan 07's switch for a violation other than `MISSING`): a failure, no re-upload
- `progress_resets_the_retries` — `retries = 1`, `break_streams(2)`: the `Execute` stream breaks, the `WaitExecution` stream sends a message and breaks, the second `WaitExecution` completes; without the reset the second break would exhaust the one retry
- `each_transient_status_in_the_response_is_retried_retries_times` — for each of the four codes, `status_in_response(code, retries)` then success: completes after `retries + 1` calls; with `status_in_response(code, retries + 1)` it fails
- `the_wait_limit_ends_a_silent_execution` — `hold_execution(2 s)`, `queue_timeout` and timeout giving a 1 s limit: `WaitPassed`
- `the_per_call_timeout_does_not_cut_a_silent_stream` — `[remote] timeout = "100ms"`, `hold_execution(500 ms)`, a 10 s limit: completes

**Done when:** `cargo test -p buildl remote` passes; `cargo make dod` green. Commit
`feat(buildl): drive an Execute stream to its end`.

### Task 3 — `Reapi::execute`

**Files:** `reapi.rs`, `encode.rs`.

**Change:** `execute(&RemoteAction)` checks `require_execution` once per build (with the
capabilities, `client.rs:243`), describes the action as `lookup` does (`describe`, `:249`),
reads the inputs as an upload does (`read_inputs`, `:266`) — an input whose bytes no longer
match is `Executed::InputChanged(path)` — sends the input files, the input-root `Directory`
messages, the `Command` and the `Action` that `FindMissingBlobs` reports missing (`REAPI:47-52`),
calls `Client::execute` with a callback that uploads the blobs a `MISSING` answer names (from
the same inputs, `Command` and `Action` it described). `encode::executed` maps the final
`ExecuteResponse` (`REAPI:1664-1695`): `status` OK and exit code 0 → `Cached(hit)` when
`cached_result`, else `Ran(hit)`; `status` OK and a non-zero exit → `Ran(hit)` whatever
`cached_result` says (spec §4 step 4); both through `hit()` (`encode.rs:315`);
`DEADLINE_EXCEEDED` → `TimedOut` with the result's stdout/stderr blobs when present; any other
code that reaches it (Task 2 has already retried the transient ones and re-uploaded for
`MISSING`) → a failure. No `UpdateActionResult` is sent.

**Proof:** in `reapi.rs` tests over the test server —
- `an_action_is_executed_and_its_result_returned` — `Ran` with the outputs' blobs fetchable through `download`
- `an_executed_result_is_then_found_by_lookup` — the server kept it (test server stores it, plan 07)
- `only_what_the_server_lacks_is_uploaded_before_execute` — mirrors `an_upload_sends_only_what_the_server_lacks` (`:684`)
- `an_input_changed_on_disk_is_input_changed_and_nothing_is_executed` — mirrors `:702`
- `a_cached_answer_is_cached`
- `a_non_zero_exit_is_ran_with_its_code` — also when `cached_result` is true
- `a_blob_missing_at_execute_is_uploaded_and_the_action_completes` — `report_missing_once(d)` for an input: `d` reaches the server's CAS and `execute` returns `Ran`
- `a_blob_missing_in_the_status_is_uploaded_and_the_action_completes` — `report_missing_in_status_once(d)`: likewise
- `deadline_exceeded_is_timed_out_with_its_logs` — the command prints, then `exec sleep` past a 3 s limit (the test server kills only the process it started)
- `a_transient_status_in_the_response_is_retried` — `status_in_response(Unavailable, 1)`; one more `Execute`, then `Ran`
- `a_permanent_status_in_the_response_is_a_failure` — `status_in_response(InvalidArgument, 1)`
- `a_server_that_cannot_execute_fails_the_execute_call` — `ServerOptions { exec: Disabled }`
- `no_result_is_uploaded_after_an_execution` — `calls_to(UpdateActionResult)` is 0

**Done when:** `cargo test -p buildl remote` passes; `cargo make dod` and `cargo deny check`
green. Commit `feat(buildl): execute actions on the remote`.

## Risks

- **Most risky step:** Task 2's deadline. The `Retry` sleeps and the stream reads must all sit
  inside the one `tokio::time::timeout`, or a long backoff could outlast the wait limit.
- Decoding `Any` relies on the `type_url` being `type.googleapis.com/<full name>`; NativeLink's
  exact form is settled by the real-server run (plan 11).
- `cached_result = true` with a non-zero exit is judged in the core (plan 05), not here.

## Proof

`cargo make dod`, `cargo deny check`; every test named runs under `--all-features`.
