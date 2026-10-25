---
status: executing
created: 2026-10-25
depends-on: [01, 02, 05]
built-on: 867ed49b84d7
---

# Remote Transport Implementation Plan

**Goal:** The `remote` adapter makes every REAPI call spec §5 lists against a server, over the channel, login and limits spec §5 sets.

**Architecture:** A `Client` in `crates/buildl/src/remote/` owns a multi-thread tokio runtime,
one tonic channel and the login header; it exposes blocking methods over the generated
protobuf types (plan 05) — capabilities, `FindMissingBlobs`, blob reads and writes choosing
batch or ByteStream, `GetActionResult`, `UpdateActionResult` — each called through
`Runtime::block_on` from whichever thread asks (spec §5). Mapping buildl's values to and from
these messages is plan 07's. An in-memory REAPI server behind feature `test-server`
(`test_server.rs`) serves this plan's tests and, through the feature, plan 08's.

**Tech Stack:** Rust, `buildl`, `buildl-core` (`Error::RemoteToken`), `tonic` 0.14 (`channel`,
`tls-ring`, `tls-native-roots`; `server` and `router` under `test-server`, plan 05), `tokio`,
`uuid`, `openssl` (once, to make the committed test
certificate), `cargo make dod`, `cargo deny check`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §5 (Runtime, Channel, Login, Capabilities, Batching, Timeout), §10
(adapter row), §12 (`block_on` risk), P9-P13, P26.

---

## Files that change

```
crates/buildl/src/remote/runtime.rs        — [create] the adapter's runtime; block_on for callers on any thread
crates/buildl/src/remote/channel.rs        — [create] RemoteUrl → endpoint; TLS with system roots plus [remote] ca; per-call timeout
crates/buildl/src/remote/login.rs          — [create] read the token (env or file) once; the header added to every call
crates/buildl/src/remote/capabilities.rs   — [create] GetCapabilities once; SHA-256, update_enabled, batch limit
crates/buildl/src/remote/transfer.rs       — [create] FindMissingBlobs; read and write blobs by batch or ByteStream; per-message stream timeout
crates/buildl/src/remote/client.rs         — [create] Client: connect(settings, root) and the blocking calls
crates/buildl/src/remote/test_server.rs    — [create, feature test-server] in-memory ActionCache, CAS, ByteStream, Capabilities on loopback; records calls; can be told to fail or stall
crates/buildl/src/remote/mod.rs            — [modify] table of contents for the files above
crates/buildl/tests/fixtures/tls/ca.pem, server.pem, server.key — [create] a test CA and a loopback server certificate, made once, long-lived
crates/buildl-core/src/error.rs            — [modify] Error::RemoteToken { source } — a token named but not readable
```

## Order of work

### Task 1 — Runtime and test server, with the `block_on` probe

**Files:** `remote/runtime.rs`, `remote/test_server.rs`, `remote/mod.rs`.

**Change:** `Runtime` — a multi-thread tokio runtime with a small fixed thread count, owned by
the adapter. `TestServer` — starts on `127.0.0.1:0` on its own runtime, serves the four
services from memory, returns its address, records each call, and can be scripted to fail a
named call or to stall one.

**Proof:** written first, seen failing, then passing —
- `many_threads_block_on_one_runtime_at_once` — eight std threads each make a unary call to the test server through one `Runtime` and one channel at the same moment; all complete (settles spec §12's first risk; recorded in the execution record)
- `the_test_server_round_trips_a_blob` — a written blob reads back byte-identical

**Done when:** `cargo test -p buildl --all-features -- remote::runtime remote::test_server`
passes; `cargo make dod` green. Commit `feat(buildl): the remote adapter's runtime and test server`.

### Task 2 — Channel, TLS and login

**Files:** `remote/channel.rs`, `remote/login.rs`, `tests/fixtures/tls/*`.

**Change:** `grpc://` connects in plain HTTP/2 and `grpcs://` in TLS with the system roots,
plus the PEM at `ca` when set (a relative `HostPath` resolved against the workspace root).
`Login` reads the token once — from the named variable, or the named file with one trailing
newline dropped — and adds `header: Bearer <token>` for `authorization` and `header: <token>`
for any other name to every call (spec §6). A variable unset or a file unreadable is a new
`Error::RemoteToken { source }` naming the variable or file, never the token; plan 08 maps it
to the usage exit (spec §7: a variable error is the existing usage error). The test
certificate is made once with `openssl` for `127.0.0.1` and `localhost`.

**Proof:** written first, seen failing, then passing (against `TestServer`) —
- `a_plain_url_connects_without_tls` — a call to a plain test server succeeds
- `a_tls_url_trusts_the_configured_ca` — server with the fixture certificate; client with `ca`: the call succeeds
- `a_tls_url_without_the_ca_is_refused` — same server, no `ca` → an error
- `the_authorization_header_carries_a_bearer_token` — server records `authorization: Bearer s3cret`
- `another_header_carries_the_bare_token` — `x-api-key: s3cret`
- `a_token_from_a_file_drops_one_trailing_newline` — file `s3cret\n` → `s3cret`
- `an_unset_token_variable_is_an_error_that_does_not_name_the_token` — `Error::RemoteToken` naming the variable; its text holds no token

**Done when:** `cargo test -p buildl --all-features -- remote::channel remote::login` passes;
`cargo make dod` green. Commit `feat(buildl): connect and log in to a remote cache`.

### Task 3 — Capabilities and blob transfer

**Files:** `remote/capabilities.rs`, `remote/transfer.rs`, `remote/client.rs`, `remote/mod.rs`.

**Change:** `Capabilities` — `GetCapabilities` once per `Client`, kept: SHA-256 must be among
`cache_capabilities.digest_functions`; uploads need `update_enabled` (P12). Transfer: a blob
joins a batch call while the batch's total stays at or under the threshold
min(`max_batch_total_size_bytes`, 4 MiB − 64 KiB), the server's term dropped when it is 0
(P12, P13). The 64 KiB of headroom comes off tonic's 4 MiB message cap only: spec §5 writes it
after the min, which for a server limit of 64 KiB or less would leave a threshold of zero and
send every blob by ByteStream, so this plan reads it as headroom on the message cap. A larger
blob is read through ByteStream `Read` of
`{instance}/blobs/{hash}/{size}` or written through `Write` of
`{instance}/uploads/{uuid}/blobs/{hash}/{size}` in 1 MiB chunks, the uuid version 4 and fresh
per write, the leading slash dropped when no instance is set (P11). `FindMissingBlobs` returns
the subset to upload. `Client` adds `get_action_result` (`NOT_FOUND` → `None`, P1) and
`update_action_result`. Every unary call is bounded whole by `[remote] timeout`; each message
of a ByteStream stream by the same timeout (spec §5). A per-blob status other than OK in a
batch answer is an error naming the digest.

**Proof:** written first, seen failing, then passing (against `TestServer`) —
- `a_server_without_sha256_is_refused` — capabilities lacking SHA-256 → error naming it
- `uploading_to_a_server_with_updates_disabled_is_refused`
- `small_blobs_travel_in_one_batch` — three small blobs: the server records one `BatchReadBlobs`
- `a_blob_at_the_threshold_batches_and_one_byte_more_streams` — server limit 1024: a 1024-byte blob → `BatchReadBlobs`; a 1025-byte blob → `ByteStream.Read`
- `no_server_limit_batches_up_to_the_message_cap` — limit 0: a 1 MiB blob batches
- `a_large_write_is_chunked_under_an_upload_resource_name` — resource name `uploads/<uuid v4>/blobs/<hash>/<size>`; no chunk over 1 MiB
- `the_instance_name_prefixes_resource_names` / `no_instance_drops_the_leading_slash`
- `find_missing_returns_only_absent_blobs` — two held, one not → the one
- `an_absent_action_result_is_none` — `NOT_FOUND` → `None`
- `a_stalled_unary_call_times_out` — timeout 200 ms, server stalls → error within a second
- `a_slow_stream_that_keeps_moving_completes` — each message within the timeout, the whole stream longer than it: completes
- `a_failed_blob_in_a_batch_is_an_error_naming_it` — the error names that digest

**Done when:** `cargo test -p buildl --all-features remote` passes; `cargo make dod`,
`cargo make guard-adapter-isolation` and `cargo deny check` green. Commit
`feat(buildl): REAPI calls and blob transfer`.

## Risks

- **Most risky: Task 1's premise** that worker threads may share one runtime through
  `block_on`. If the probe test fails, the runtime design is replaced inside `runtime.rs` (a
  handle per caller, or one dedicated I/O thread with a channel), the spec's §5 Runtime row is
  amended, and nothing outside the module changes.
- Tests behind `test-server` run under the gate's `--all-features`; a plain `cargo test -p
  buildl` skips them.
- The committed test certificate is a fixture with a long life; it secures nothing.

## Proof

`cargo make dod`, `cargo deny check` and `cargo make guard-adapter-isolation` green; every REAPI
call of spec §5 succeeds against the test server over plain and TLS channels, and every limit
and timeout behaves as spec §5 says.
