---
status: done
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
tools/reapi-codegen/src/main.rs                     — [modify] ROOT_PROTOS (:19-22) gains google/rpc/error_details.proto; the array length becomes 3; the prost config turns on type names (enable_type_names, type_name_domain "type.googleapis.com")
crates/buildl/generated/reapi/*.rs                  — [modify] regenerated: google.rpc.rs gains ErrorInfo, RetryInfo, PreconditionFailure, …; every message in all six files gains `impl prost::Name`
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
vendored). Add it to `ROOT_PROTOS`. prost emits no message name by default, so the generator also
turns on type names with the domain `type.googleapis.com`: every generated message gains
`impl prost::Name`, whose `full_name()` is the name a status detail's `type_url` ends with and
whose `type_url()` is the `type.googleapis.com/<full name>` form servers send; plan 08's
`prost_types::Any::to_msg` needs it too. Run `cargo make reapi-codegen`
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

## Review findings

- risk — generated `type_url()` had no domain (`/google.rpc.PreconditionFailure`), unlike real servers' `type.googleapis.com/…` — `tools/reapi-codegen/src/main.rs` — fixed: `type_name_domain(["."], "type.googleapis.com")`; verified by `cargo test -p buildl remote::proto` → `test result: ok. 3 passed; 0 failed` (the new `type_url()` assertion failed first with left `"/google.rpc.PreconditionFailure"`)
- docs — generator module doc named only REAPI and ByteStream — `tools/reapi-codegen/src/main.rs:1` — fixed
- docs — README line not wrapped to the file's width — `crates/buildl/proto/README.md:14` — fixed
- docs — comment said the full name is the `type_url`; it is the segment after the last `/` — `crates/buildl/src/remote/proto.rs` — fixed
- spec — plan did not record the generator config change — this plan — fixed: Files and Task 1 amended

## Probe results

- `error_details.proto` at `dff1cd0` is 363 lines and imports only `google/protobuf/duration.proto` — `wc -l`, `grep -n '^import'` → `363`, `19:import "google/protobuf/duration.proto";` — as planned
- vendored file equals upstream — reviewer `cmp` against a fresh download → identical — as planned
- prost emits a message name without config — red run of `a_precondition_failure_round_trips` → no `Name` impl — **against the plan**: needs `enable_type_names()`
- codegen is stable — `cargo make reapi-codegen` twice, `git diff --exit-code crates/buildl/generated` → no output, exit 0

## Deviations

- 2026-10-26 — the generator turns on prost type names (`enable_type_names`, domain `type.googleapis.com`), so every message in all six generated files gains `impl prost::Name`, not only `google.rpc.rs`'s new messages. Needed for the message name the test checks and for `Any::to_msg` later. Plan amended.
