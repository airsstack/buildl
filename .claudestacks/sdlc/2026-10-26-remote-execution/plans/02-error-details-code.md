---
status: approved
created: 2026-10-26
built-on: 4704ec60505e
---

# Error Details Code Implementation Plan

**Goal:** `buildl` carries generated code for `google.rpc.PreconditionFailure`, from a vendored `error_details.proto`.

**Architecture:** The proto is copied byte for byte from the googleapis commit every other
googleapis file is pinned to (`dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e`,
`crates/buildl/proto/README.md`), added to the generator's root list, and the generated
`google.rpc.rs` is regenerated and committed. No hand-written code uses it yet; plan 08 decodes
`PreconditionFailure` from a status's details.

**Tech Stack:** `tools/reapi-codegen` (protox, tonic-prost-build), `cargo make reapi-codegen`.

**Content authority:** spec §4 step 3 (the `PreconditionFailure` detail, vendored from `dff1cd0`).

---

## Files that change

```
crates/buildl/proto/google/rpc/error_details.proto  — [create] copied from googleapis dff1cd0
tools/reapi-codegen/src/main.rs                     — [modify] ROOT_PROTOS (:19-22) gains google/rpc/error_details.proto; the array length becomes 3
crates/buildl/generated/reapi/google.rpc.rs         — [modify] regenerated: gains ErrorInfo, RetryInfo, PreconditionFailure, …
crates/buildl/proto/README.md                       — [modify] the googleapis row of the table names error_details.proto
crates/buildl/src/remote/proto.rs                   — [modify] mod tests (:94) gains one test
```

## Order of work

### Task 1 — Vendor and regenerate

**Files:** all five above.

**Change:** fetch
`https://raw.githubusercontent.com/googleapis/googleapis/dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e/google/rpc/error_details.proto`
into `crates/buildl/proto/google/rpc/error_details.proto` unchanged (363 lines; it imports only
`google/protobuf/duration.proto`, line 19, a well-known type protox ships, so no other file is
vendored). Add it to `ROOT_PROTOS`. Run `cargo make reapi-codegen`
(`Makefile.toml:406`). `proto.rs` already `include!`s `google.rpc.rs` (`proto.rs:86`), so the
new messages appear under `proto::google::rpc`. The README's sentence "The set is the import
closure of `remote_execution.proto` and `bytestream.proto`" gains `error_details.proto`.

**Proof:** written first, seen failing, then passing —
- `a_precondition_failure_round_trips` (in `proto.rs` `mod tests`) — a `PreconditionFailure`
  with one `MISSING` violation and subject `blobs/<hash>/<size>` encodes and decodes to itself,
  and its message name is `google.rpc.PreconditionFailure` (the `type_url` suffix plan 08 matches on)
- running `cargo make reapi-codegen` a second time leaves `git diff --exit-code crates/buildl/generated` clean (the CI job at `ci.yml:132-171` checks the same)

**Done when:** both pass, `cargo make dod` and `cargo deny check` are green. Commit
`build(buildl): vendor google.rpc error details`.

## Risks

- The generated file may need a lint suppression `proto.rs` (`:15`) does not yet carry; add it
  with a reason, as the existing ones have.
- `prost_types::Duration` is used by `RetryInfo`; `prost-types` is already a dependency
  (`crates/buildl/Cargo.toml`), so no new crate enters `cargo deny`.

## Proof

`cargo make dod`, `cargo deny check`, and `cargo make reapi-codegen` twice with no diff.
