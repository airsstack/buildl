---
status: done
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
owns is when an upload happens, what it names, and what a failure does. A `ReadWrite` policy
under `strategy = "bare"` is refused (spec §6) by `ensure_sandboxed_upload` (`assembly.rs`),
which `Pipeline::run` calls after planning and before the approver is asked, and `execute`
calls again for a direct caller.

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
crates/buildl-core/src/execute/assembly.rs   — [modify] ensure_sandboxed_upload: refuse ReadWrite under Isolation::Bare before any target starts
crates/buildl-core/src/pipeline/driver.rs    — [modify] run: the refusal before the approver is asked
crates/buildl-core/src/ports/reporter.rs     — [modify] Reporter::remote doc: a changed-input notice, once per target
crates/buildl-core/src/types/remote_notice.rs — [modify] doc: a failure once per command, a changed input once per target
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
and stdout and stderr as `Blob`s. `upload(remote, label, action, upload, cas)` (the label because
`RemoteAction` carries none and the notice names it) calls `remote.upload` with `StoredBlobs`, a
`BlobSource` reading the content store, and answers a crate-private `Sent`: `Done` → `Stored`;
`InputChanged(path)` → `Declined(RemoteNotice::InputChanged { label, path })`, which does not turn
the remote off; `Err` → `Failed(notice)` of call `Upload`, to which the worker applies the policy.

**Proof:** written first, seen failing, then passing —
- `an_upload_names_every_output_with_sizes` — a file and a tree output, each file's size equal to its bytes' length
- `an_upload_names_stdout_and_stderr` — their digests and sizes
- `the_blob_source_reads_the_local_store` — a digest the store holds comes back byte-identical; one it lacks is `None`
- `an_input_changed_answer_is_a_notice_not_a_failure` — `Declined` with the `InputChanged` notice (that the remote stays on is asserted by the Task 2 flow test, where the switch lives)

**Done when:** `cargo test -p buildl-core execute::upload` passes; `cargo make dod` green.
Commit `feat(buildl-core): describe a run for the remote cache`.

### Task 2 — The worker uploads

**Files:** `execute/perform.rs`, `execute/assembly.rs`, `error.rs`, `buildl-cli/src/exit.rs`,
`tests/flows/common.rs`, `tests/flows/remote.rs`.

**Change:** under `RemoteUse::ReadWrite`, when the remote run's `down` flag (plan 03) is not
set and the action ended `Produced`, the worker uploads before it returns, describing the task
again with `remote_action` (deterministic: the same action the lookup used); a lenient upload
failure turns the remote off through the same `RemoteSwitch::turn_off` as a lookup failure, so
one warning is sent. A failed action, a target restored from the
local cache (it never reaches a worker) and a fetched target are never uploaded. An upload
failure under `Lenient` is one notice and `down`; the target's outcome stays `Built`. Under
`Strict` it is the build's fatal error, like a lookup failure (plan 03). Before the approver is
asked and before any target starts, the build returns `Error::UploadUnsandboxed` when the policy
is `ReadWrite` and `workspace.execution.strategy` is `Isolation::Bare` (`isolation.rs:36-38`);
its message says how to sandbox or stop uploading (`BUILDL_REMOTE_UPLOAD`, `--remote-upload`).

**Proof:** written first, seen failing, then passing (`tests/flows/remote.rs`) —
- `a_target_that_ran_is_uploaded_under_read_write` — the fake records one upload naming its outputs; the uploaded action equals the looked-up one
- `nothing_is_uploaded_under_read` — the fake records no upload
- `a_failed_action_is_never_uploaded`
- `a_target_restored_from_the_local_cache_is_not_uploaded` — second build of an unchanged graph: no upload
- `a_fetched_target_is_not_uploaded_again`
- `the_upload_happens_before_the_row_is_recorded` — the fake's upload call precedes the cache's `record` for the label
- `an_upload_failure_warns_and_keeps_the_target_built` — one notice; outcome `Built`; a later independent target neither looks up nor uploads (one worker)
- `a_strict_upload_failure_fails_the_build` — fatal `Error::Adapter` naming the remote cache; its diagnostic starts `upload: `
- `a_remote_that_declines_an_upload_for_a_changed_input_is_only_a_notice_and_stays_on` — an `InputChanged` notice; two upload calls; the next target still looks up
- `uploading_under_the_bare_strategy_is_refused_before_anything_runs` — `Error::UploadUnsandboxed`; strategy never called; the approver never asked

**Done when:** `cargo test -p buildl-core --test flows remote` passes; `cargo make dod` green.
Commit `feat(buildl-core): upload to the remote cache on the workers`.

## Risks

- An upload holds its worker for the transfer, delaying that target's dependents by the upload
  time; that is spec §3's design (upload before `store`), chosen over a background uploader.
- Reading each output blob for its size reads it a second time; the content was written moments
  before.

## Proof

`cargo make dod` and `cargo deny check` green; the flow tests show exactly the targets that ran
and succeeded uploaded, in the order of spec §3, and every failure behaving as spec §6 says.

## Review findings

- spec — deviations not recorded in the plan — plan 04 — fixed: plan amended (signature, `Sent`, recompute, refusal placement, test names, commit subject)
- doc-comment-discipline — `UploadUnsandboxed` named only the strategy as the fix — `error.rs` — fixed: names `BUILDL_REMOTE_UPLOAD` and `--remote-upload` too; verified by the fix coder's `cargo make dod` (green on its third run after a fmt and a clippy fix)
- correctness — the refusal ran after the approver was asked, so an interactive approver could prompt for a refused build — `assembly.rs` / `driver.rs` — fixed: `ensure_sandboxed_upload` before `negotiate`; flow test asserts the approver was not asked; same run
- test-coverage — nothing asserted the uploaded action equals the looked-up one — `tests/flows/remote.rs` — fixed; same run
- strong-types — `Result<Option<RemoteNotice>, RemoteNotice>` — `upload.rs` — fixed: `Sent { Stored, Declined, Failed }`; same run
- efficiency — `blob_of` reads a whole blob to learn its size — `upload.rs` — kept: a size query on `ContentStore` would touch 2 adapters and 13 fakes; the content was written moments before (plan Risks)
- doc-comment-discipline — `Reporter::remote` and `RemoteNotice` docs said "once per build" though a changed input is said per target — fixed
- test-coverage — strict upload test matched any text containing "upload" — fixed: `starts_with("upload: ")`
- naming — the changed-input test name said "not uploaded" though the core calls upload — fixed: renamed, asserts two upload calls
- Fix round gates (run by the fix coder): `cargo make dod` → `Build Done in 55.20 seconds`; `cargo make guard-module-edges` exit 0; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`

## Probe results

- `ContentStore` size query — fix coder's search: 2 adapters (`cas/local_cas.rs`, `exec/memory_store.rs`) and 13 fakes implement the port — the change was not taken (see Review findings)

## Deviations

- 2026-10-25 — Task 1: `upload` takes the target's label and answers `Sent`; the off-switch half of `an_input_changed_answer_is_a_notice_not_a_failure` is asserted by the Task 2 flow test.
- 2026-10-25 — Task 2: the worker re-describes the task at upload time; the bare-strategy refusal moved before the approver (`d9dd333`).
- 2026-10-25 — one reviewer pass over the two tasks, reading the diff only (gates run once, by the coder), then one fix commit.
