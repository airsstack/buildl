---
status: approved
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
crates/buildl/src/remote/capabilities.rs — [modify] fetch (:66) is retried before its answer is kept
crates/buildl/src/remote/error.rs        — [modify] RemoteError (:17) says whether it is transient (Status by code, TimedOut, a connection that failed); Retried { attempts } wording
crates/buildl/src/remote/test_server.rs  — [modify] Fault (:68) gains ResourceExhausted, Aborted, Cancelled; fail_times(method, fault, n)
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
- the existing `a_cache_that_is_down_fails_the_first_call_and_not_the_adapter` (`reapi.rs:880`) still passes, now after the retries

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
