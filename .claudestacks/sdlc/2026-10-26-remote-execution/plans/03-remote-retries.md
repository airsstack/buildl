---
status: done
created: 2026-10-26
depends-on: [01]
built-on: 4704ec60505e
---

# Remote Retries Implementation Plan

**Goal:** Every call the remote adapter makes is retried on a transient failure before it counts as failed.

**Architecture:** One new file in the adapter, `remote/retry.rs`, holds the rule: which codes
are transient, the delay before each retry, and the count. The `Client`
(`remote/client.rs:172`) reads `retries` from its `RemoteSettings` when it is made
(`Client::new`, `:194`) and runs every call through the rule, so no call has its own copy. The
delay is a `tokio::time::sleep` inside the adapter's runtime; the random part comes from
`std::collections::hash_map::RandomState`, so no crate is added and no clock reaches
`buildl-core` (architecture §5). The core is unchanged: a call that fails after its retries
reaches it as the same `Error::Adapter` it gets today.

**Tech Stack:** Rust, `buildl` (`remote` module), tokio, tonic, `cargo make dod`.

**Content authority:** spec §5 (calls, codes, count, delay, `RetryInfo` not read).

---

## Files that change

```
crates/buildl/src/remote/retry.rs        — [create] Retry: transient codes, delay schedule, count; unit tests
crates/buildl/src/remote/mod.rs          — [modify] declare retry (private)
crates/buildl/src/remote/client.rs       — [modify] Client holds a Retry; every method (:243-364) runs its call through it; tests
crates/buildl/src/remote/transfer.rs     — [modify] batch and ByteStream calls (:207-455) run through the Retry they are handed; a ByteStream write is retried whole under a fresh upload UUID
crates/buildl/src/remote/capabilities.rs — unchanged: the client retries the connect and the fetch as one unit, in client.rs
crates/buildl/src/remote/channel.rs      — [modify] Pending::open(runtime) becomes async Pending::connect, so a retried connect never nests block_on
crates/buildl/src/remote/login.rs        — [modify] test helper uses connect
crates/buildl/src/remote/reapi.rs        — [modify] failed() names a call once after retries; test settings use retries = 0
crates/buildl/tests/e2e/remote.rs        — [modify] the server-down test sets retries = 0
crates/buildl/src/remote/error.rs        — [modify] RemoteError (:17) says whether it is transient (Status by code, TimedOut, a connection that failed); Retried { attempts } wording
crates/buildl/src/remote/test_server.rs  — [modify] Fault (:68) gains ResourceExhausted, Aborted, Cancelled; fail_times(method, fault, n); a scripted transient status for one batch entry
```

## Order of work

### Task 1 — The retry rule

**Files:** `retry.rs`, `mod.rs`.

**Change:** introduce `Retry` — made from a `RetryCount` (plan 01); `transient(code)` is true for
exactly `Unavailable`, `ResourceExhausted`, `Aborted`, `Cancelled` (spec §5; Buck2
`re_grpc/src/client.rs:608-611`); `delay(n)` for the n-th retry is `100 ms × 2^(n-1)`, at most
5 s, plus a random 0–49 ms (spec §5; Buck2 `:671-679`); `run(op)` calls `op`, and while the
result is a transient failure and retries remain, sleeps `delay(n)` and calls again. Every
message a stream delivers resets the count (spec §5): `run` hands `op` a `Progress` handle whose
`made()` a streaming caller calls on each message, used by plan 08. `Retry` can be made with a
zero base delay for tests, since the workspace's tokio has no `test-util` feature to pause time
(`Cargo.toml:90`: `["rt", "rt-multi-thread", "time"]`).

**Proof:** written first, seen failing, then passing (`retry.rs` `mod tests`) —
- `exactly_four_codes_are_transient` — the four are; `Internal`, `Unknown`, `NotFound`,
  `PermissionDenied`, `Unauthenticated`, `InvalidArgument`, `FailedPrecondition`,
  `DeadlineExceeded` are not
- `delays_double_from_100ms_up_to_5s` — the random part removed, retries 1–7 wait 100, 200, 400,
  800, 1600, 3200, 5000 ms
- `the_random_part_is_under_50ms` — over 1000 draws, every extra lies in 0..50 ms
- `five_retries_make_at_most_six_calls` — an op that always fails transiently is called 6 times
- `zero_retries_make_one_call`
- `a_permanent_failure_is_not_retried` — an op failing `PermissionDenied` is called once
- `progress_resets_the_count` — an op that fails, reports progress, and fails again is allowed
  `retries` more calls after the progress

**Done when:** `cargo test -p buildl remote::retry` passes (tests use the zero-delay schedule,
so they take no real seconds); `cargo make dod` green. Commit
`feat(buildl): a retry rule for remote calls`.

### Task 2 — Every call through the rule

**Files:** `client.rs`, `transfer.rs`, `capabilities.rs`, `error.rs`, `test_server.rs`,
`crates/buildl-cli/tests/cli/remote.rs`.

**Change:** `Client` holds a `Retry` from `settings.retries()`. `get_action_result` (`:315`),
`update_action_result` (`:348`), `find_missing_blobs` (`:270`), `read_blobs` (`:285`),
`write_blobs` (`:298`) and the capabilities fetch run through it. Inside `transfer.rs`,
`batch_read` (`:243`), `batch_write` (`:384`), `stream_read` (`:313`), `stream_write` (`:424`)
and the `FindMissingBlobs` call (`:195`) are each one retried unit; a batch answer whose
entries carry a transient code (mapped in `check`, `:159`) is retried as a whole call. A
`TimedOut` (`error.rs:38`) and a connection that cannot be made count as `UNAVAILABLE`; a
channel that cannot be set up (a bad `ca`, `channel.rs`) is not retried. `GetActionResult`'s
`NOT_FOUND` is still a miss (`client.rs:334`), never a retry. The error after the last retry
says how many calls were made. The test server's `fail_times(method, fault, n)` fails the next
`n` calls to `method` and then answers normally; `fail` (`:244`) is unchanged. Every CLI test
whose workspace points at `closed_url` (`tests/cli/remote.rs:51`) or at a server scripted to fail
gains `retries = 0` in its `[remote]` table, so the suite does not wait through the retry
schedule (light machine load).

**Proof:** in `client.rs` tests and `reapi.rs` tests (`:460`) —
- `a_call_that_fails_unavailable_twice_then_succeeds_succeeds` — `fail_times(GetActionResult, Unavailable, 2)`; the lookup returns the result and the server saw 3 calls
- `each_transient_code_is_retried` — `ResourceExhausted`, `Aborted`, `Cancelled` likewise
- `a_permanent_code_fails_at_once` — `fail(GetActionResult, PermissionDenied)`; one call seen
- `retries_zero_makes_one_call` — settings with `retries = 0`, `fail_times(…, Unavailable, 1)` fails
- `a_streamed_write_is_retried_whole` — `fail_times(Write, Unavailable, 1)` on a blob over the batch threshold; the blob reaches the server
- `capabilities_are_retried_before_they_are_kept` — `fail_times(GetCapabilities, Unavailable, 1)`; the first lookup succeeds
- `a_lookup_miss_is_not_retried` — an unknown action: one `GetActionResult`, `None`
- the existing `a_cache_that_is_down_fails_the_first_call_and_not_the_adapter` (`reapi.rs:880`) still passes; its settings use `retries = 0` so it does not wait out the schedule, and `a_connection_that_cannot_be_made_is_retried` (`client.rs`) proves the connect is retried

**Done when:** `cargo test -p buildl remote` passes; `cargo make dod` and `cargo deny check`
green. Commit `feat(buildl): retry every remote call on a transient failure`.

## Risks

- **Most risky step:** a closed server now costs the full retry schedule (≈ 3.1 s plus jitter at
  `retries = 5`) before the warning; Task 2 sets `retries = 0` in the CLI tests that use one.
- Whether tonic reports a refused connection as `Unavailable` or as a transport error the
  `ChannelError` type holds is settled by a probe in Task 2 before the mapping is written.
- A ByteStream read retried whole re-reads from offset 0; resuming at an offset is not done.

## Proof

`cargo make dod`, `cargo deny check`; `cargo test -p buildl remote` includes every test named.

## Review findings

Task 1:
- modularity — a second `Progress` type beside `client::Progress` — `remote/retry.rs` — fixed: the rule hands each try a fresh `client::Progress`; verified by `cargo test -p buildl remote::retry` → `9 passed; 0 failed`
- test-coverage — removing the sleep left every test green — `remote/retry.rs` — fixed: `each_retry_waits_out_its_pause` (1 ms base, 3 retries, elapsed ≥ 7 ms)
- docs — `mod.rs:6` over-long line — fixed
- strong-types — test-only zeroing fields on `Retry` — kept: test-only constructor, no production setting

Task 2:
- bug — ByteStream tries recorded progress, so a cache that kept refusing a write or a read reset the count and was retried forever (spec §5 resets only on `Execute`/`WaitExecution` messages) — `remote/transfer.rs` — fixed: stream calls run as `run(|_| …)` with their own `Progress` for the timeout; verified by `a_streamed_write_that_keeps_failing_is_given_up_after_its_retries` and `a_streamed_read_that_keeps_failing_is_given_up_after_its_retries` (`GaveUp { calls: 4 }` at 3 retries); `cargo test -p buildl --all-features remote` → `177 passed` lib, `7 passed` e2e
- bug — after retries the call name was printed twice — `remote/reapi.rs` `failed` — fixed: `names_own_call` looks through `GaveUp.last`; test `a_call_given_up_after_retries_is_named_once`
- test-coverage — no test for the retry of `UpdateActionResult` and `BatchUpdateBlobs` — fixed: `an_action_result_update_is_retried`, `a_batch_write_is_retried`
- test-coverage — no test for a batch whose entries carry a transient code — fixed: `TestServer::fail_entries`; `a_batch_read_whose_entries_fail_transiently_is_retried_whole`, `a_batch_write_whose_entries_fail_transiently_is_retried_whole`
- question — a readable but wrong `ca` fails the TLS handshake, which is a connect failure and is retried — kept: the plan excludes only a channel that cannot be set up (an unreadable `ca`); a handshake failure is a connect failure
- spec — file list and the reapi down-cache proof did not match what was built — fixed: plan amended
- gates after the fix round: `cargo test --workspace --all-targets --all-features --no-fail-fast` → all passed (698, 145, 108, 9, 20, 2); `cargo make dod` passes fmt, clippy and doc, and its test step stopped twice on `watch::an_edit_rebuilds`, a file-watcher timing test with no remote code in it, which passes alone; `cargo deny check` → ok

## Probe results

- the four transient codes and the delay schedule match Buck2 — `b2src/.../re_grpc/src/client.rs:604-612, 668-680` → `Unavailable | ResourceExhausted | Aborted | Cancelled`; 100 ms initial, 5 s max, jitter `0..50` ms — as planned
- tokio has no `test-util` — `Cargo.toml:90` → `features = ["rt", "rt-multi-thread", "time"]` — as planned
- how tonic reports a refused connection — temporary test → `Channel(Connect { reason: "transport error: tcp connect error: tcp connect error: Connection refused (os error 61)" })` — not an `Unavailable` status, so `Channel(Connect)` maps to `Unavailable` explicitly
- `Pending::open(runtime)` calls `block_on` itself, so a retried connect inside `block_on` would nest — reviewer's reading of `channel.rs:132` — **against the plan's silence**: connect made async (`Pending::connect`)

## Deviations

- 2026-10-26 — the connect is async (`Pending::connect`) and is retried with `GetCapabilities` as one unit inside `capabilities()`; `capabilities.rs` is unchanged. Plan amended.
- 2026-10-26 — after more than one call the error is `RemoteError::GaveUp { calls, last }`, shown as `<last> (after N calls)`; a call made once keeps its own error.
- 2026-10-26 — `crates/buildl/tests/e2e/remote.rs` and the reapi test settings use `retries = 0`. Plan amended.
