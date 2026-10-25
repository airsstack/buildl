---
status: approved
created: 2026-10-25
depends-on: [03]
built-on: 3d9113128fe5
---

# Remote Upload Implementation Plan

**Goal:** A target whose action ran and succeeded is uploaded to the remote cache when uploading is on.

**Architecture:** Upload happens on the worker in `perform`, after the outputs and logs are in
the local cas and before the result returns to the scheduler (spec §3), through a new
`execute/upload.rs` that builds the `RemoteUpload` and hands the adapter a `BlobSource` over the
local content store. The protocol order of the upload is the adapter's (plan 07); what the core
owns is when an upload happens, what it names, and what a failure does. `execute` refuses a
`ReadWrite` policy under `strategy = "bare"` before anything runs (spec §6).

**Tech Stack:** Rust, `buildl-core`, `buildl-cli`, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §3 (what is uploaded and when; failures and restored targets never;
input changed; upload before `store`), §6 (lenient and strict during upload; uploading refused
under `bare`), §10 (core rows on upload).

---

## Files that change

```
crates/buildl-core/src/execute/upload.rs     — [create] upload_of(): the RemoteUpload of a run; upload(): call the port, map the answer; unit tests
crates/buildl-core/src/execute/perform.rs    — [modify] after Ended::Produced under ReadWrite: upload; notices as plan 03 defines them
crates/buildl-core/src/execute/assembly.rs   — [modify] refuse ReadWrite under Isolation::Bare before any target starts
crates/buildl-core/src/execute/mod.rs        — [modify] declare upload
crates/buildl-core/src/error.rs              — [modify] Error::UploadUnsandboxed
crates/buildl-cli/src/exit.rs                — [modify] UploadUnsandboxed → the usage exit (spec §7)
crates/buildl-core/tests/flows/remote.rs     — [modify] the upload flow tests below
crates/buildl-core/tests/flows/common.rs     — [modify] FakeRemote records uploads and can answer InputChanged or fail
```

## Order of work

### Task 1 — What an upload names

**Files:** `execute/upload.rs`, `execute/mod.rs`.

**Change:** `upload_of(task, kept, printed, cas)` builds the `RemoteUpload`: per declared
output a `RemoteOutput` — its `OutputPath` and `RemoteContent` (`File { blob, mode }`, or
`Tree(RemoteTree)` of the manifest's entries), each file's size read from the content store —
and stdout and stderr as `Blob`s. `upload(remote, action, upload, cas)` calls `remote.upload`
with a `BlobSource` reading the content store, and maps its answer: `Done` → nothing to say;
`InputChanged(path)` → a `RemoteNotice::InputChanged { label, path }` that does not turn the
remote off; `Err` → a remote failure of call `Upload`.

**Proof:** written first, seen failing, then passing —
- `an_upload_names_every_output_with_sizes` — a file and a tree output, each file's size equal to its bytes' length
- `an_upload_names_stdout_and_stderr` — their digests and sizes
- `the_blob_source_reads_the_local_store` — a digest the store holds comes back byte-identical; one it lacks is `None`
- `an_input_changed_answer_is_a_notice_not_a_failure` — `InputChanged` notice; `down` stays false

**Done when:** `cargo test -p buildl-core execute::upload` passes; `cargo make dod` green.
Commit `feat(buildl-core): describe a run for the remote cache`.

### Task 2 — The worker uploads

**Files:** `execute/perform.rs`, `execute/assembly.rs`, `error.rs`, `buildl-cli/src/exit.rs`,
`tests/flows/common.rs`, `tests/flows/remote.rs`.

**Change:** under `RemoteUse::ReadWrite`, when the remote run's `down` flag (plan 03) is not
set and the action ended `Produced`, the worker uploads before it returns; a lenient upload
failure flips `down` by the same compare-exchange as a lookup failure, so one warning is sent. A failed action, a target restored from the
local cache (it never reaches a worker) and a fetched target are never uploaded. An upload
failure under `Lenient` is one notice and `down`; the target's outcome stays `Built`. Under
`Strict` it is the build's fatal error, like a lookup failure (plan 03). Before any target
starts, `execute` returns `Error::UploadUnsandboxed` when the policy is `ReadWrite` and
`workspace.execution.strategy` is `Isolation::Bare` (`isolation.rs:36-38`).

**Proof:** written first, seen failing, then passing (`tests/flows/remote.rs`) —
- `a_target_that_ran_is_uploaded_under_read_write` — the fake records one upload naming its outputs
- `nothing_is_uploaded_under_read` — the fake records no upload
- `a_failed_action_is_never_uploaded`
- `a_target_restored_from_the_local_cache_is_not_uploaded` — second build of an unchanged graph: no upload
- `a_fetched_target_is_not_uploaded_again`
- `the_upload_happens_before_the_row_is_recorded` — the fake's upload call precedes the cache's `record` for the label
- `an_upload_failure_warns_and_keeps_the_target_built` — one notice; outcome `Built`; a later independent target neither looks up nor uploads (one worker)
- `a_strict_upload_failure_fails_the_build` — fatal `Error::Adapter` naming the remote cache and `Upload`
- `an_input_changed_during_the_build_is_not_uploaded_and_the_remote_stays_on` — an `InputChanged` notice; the next target still looks up
- `uploading_under_the_bare_strategy_is_refused_before_anything_runs` — `Error::UploadUnsandboxed`; strategy never called

**Done when:** `cargo test -p buildl-core --test flows remote` passes; `cargo make dod` green.
Commit `feat(buildl-core): upload what ran to the remote cache`.

## Risks

- An upload holds its worker for the transfer, delaying that target's dependents by the upload
  time; that is spec §3's design (upload before `store`), chosen over a background uploader.
- Reading each output blob for its size reads it a second time; the content was written moments
  before.

## Proof

`cargo make dod` and `cargo deny check` green; the flow tests show exactly the targets that ran
and succeeded uploaded, in the order of spec §3, and every failure behaving as spec §6 says.
