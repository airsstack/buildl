---
status: done
created: 2026-10-25
depends-on: [02, 06]
built-on: 5ec007f25407
---

# REAPI Adapter Implementation Plan

**Goal:** `Reapi`, the `remote` adapter's `RemoteCache`, turns buildl's remote values into REAPI messages and back over the plan 06 client.

**Architecture:** Three files on top of plan 06's `Client`: `input_tree.rs` builds the
canonical `Directory` tree of a `RemoteAction`'s files, reading sizes and executable bits from
disk (spec §2); `encode.rs` maps `RemoteAction` to `Command`, `Platform` and `Action`, a
`RemoteUpload` to an `ActionResult` with a `Tree` per directory output, and an `ActionResult` to
a `RemoteHit`; `reapi.rs` implements the four port methods. Every check of a hit stays in the
core (plan 03); this plan only transports and encodes. The adapter reads the content store
only through the core's `BlobSource` and the workspace with `std::fs`, never through another
adapter module (P26).

**Tech Stack:** Rust, `buildl`, `prost` 0.14, `cargo make dod`, `cargo deny check`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2 (the table of fields, sizes and modes from disk), §3 (upload
order, input changed), §4 (the port), §5 (Upload, Download, Input tree, Encoding), §10 (adapter
row), P2-P8, P10, P14.

---

## Files that change

```
crates/buildl/src/remote/input_tree.rs  — [create] the Directory messages of an input root, from RemoteFiles and the disk; unit tests
crates/buildl/src/remote/encode.rs      — [create] RemoteAction → Command, Platform, Action; RemoteUpload → ActionResult and Trees; ActionResult → RemoteHit; Tree → RemoteTreeFiles; unit tests
crates/buildl/src/remote/reapi.rs       — [create] Reapi: the RemoteCache implementation over Client; tests against TestServer
crates/buildl/src/remote/mod.rs         — [modify] declare them; export Reapi
crates/buildl/src/remote/client.rs      — [modify] connect on the first call; a failed connect kept, later calls `Unusable`
crates/buildl/src/remote/channel.rs     — [modify] `prepare` (read the CA, check the address) and `Pending::open` (connect, bounded)
crates/buildl/src/remote/transfer.rs    — [modify] `FindMissingBlobs` chunked under the message cap
crates/buildl/src/remote/error.rs       — [modify] `TooLarge`; `Blob` names its call
crates/buildl/src/remote/vocabulary.rs  — [modify] no `unreachable_pub` with the feature off
```

## Order of work

### Task 1 — The input tree

**Files:** `remote/input_tree.rs`, `remote/mod.rs`.

**Change:** `input_tree(root, files)` builds, from the sorted `RemoteFile`s, one `Directory`
per directory of the input root: `FileNode`s with name, `Digest { hash, size_bytes }` and
`is_executable`, `DirectoryNode`s with the digest of the child's encoding, each list sorted by
name bytes (P6); it returns the root's digest and every `Directory` with its encoded bytes for
upload. Each file's size is read from disk at `root/<path>`; an input's executable bit from its
mode (any execute bit set), a dependency file's from its `RemoteFile`. A file missing from disk
is `Error::Adapter` naming the remote cache and the path.

**Proof:** written first, seen failing, then passing —
- `files_in_one_directory_are_sorted_by_name`
- `nested_paths_become_nested_directories_whose_digests_chain`
- `an_input_executable_on_disk_is_marked_executable`
- `a_dependency_file_keeps_its_manifest_mode`
- `the_same_files_give_the_same_root_digest` — two calls, byte-identical encodings
- `a_missing_file_names_its_path`

**Done when:** `cargo test -p buildl remote::input_tree` passes; `cargo make dod` green. Commit
`feat(buildl): build the input root of a remote action`.

### Task 2 — Encoding

**Files:** `remote/encode.rs`, `remote/mod.rs`.

**Change:** `command(action)` — `arguments` the command's words, `environment_variables`
sorted by name, `output_paths` as given (already sorted, plan 02), `working_directory` empty
(spec §2). `platform(action)` — the `RemotePlatform` plan 02 mapped, as the properties
`ISA` and `OSFamily`, sorted by name then value (P6). `action(...)` — `command_digest`, `input_root_digest`, `timeout`, `salt` the
32 key bytes, `platform`. Digests are SHA-256 over `encode_to_vec()` (P7, P8).
`action_result(upload)` — `output_files` for file outputs (path, digest, `is_executable`),
`output_directories` with each `Tree`'s digest (a `Tree` of `root` and `children` built from the
manifest's entries, children in a stable order), `stdout_digest`, `stderr_digest`,
`exit_code` 0. `hit(result)` — the `RemoteHit` of an `ActionResult`: every output file and
directory as written, `symlinks` true when `output_symlinks`, `output_file_symlinks` or
`output_directory_symlinks` is non-empty (P10). The last two are deprecated in the proto and
`#[deprecated]` in the generated code, so the one function that reads them carries
`#[expect(deprecated, reason = "a v2.0 server reports symlinks only in these fields")]`. `tree_files(bytes)` — decode a `Tree` and
flatten it to `RemoteTreeFile`s; `None` when it holds a symlink, an empty directory or a name
`TreePath` refuses.

**Proof:** written first, seen failing, then passing —
- `the_command_matches_spec_section_2` — fields and order for a fixed action
- `the_platform_is_two_sorted_properties` — `ISA` then `OSFamily`, with the values plan 02 mapped
- `the_salt_changes_the_action_digest` — two actions differing only in salt differ
- `an_action_digest_is_stable` — the same `RemoteAction` encodes to the same digest twice
- `an_action_result_round_trips_to_a_hit` — `action_result` then `hit` names the same outputs, digests and modes
- `a_tree_round_trips_to_its_files`
- `a_tree_with_a_symlink_or_an_empty_directory_decodes_to_none`
- `output_symlinks_are_reported`

**Done when:** `cargo test -p buildl remote::encode` passes; `cargo make dod` green. Commit
`feat(buildl): encode remote actions and results as REAPI messages`.

### Task 3 — `Reapi`

**Files:** `remote/reapi.rs`, `remote/mod.rs`.

**Change:** `Reapi::new(&RemoteSettings, root)` (the client is crate-private, so a public
constructor takes the settings) implements `RemoteCache`; its client reads the token at
construction and connects on its first call, so a server that is down is that call's failure:
- `lookup` — encode the action, `get_action_result`, `hit`; `NOT_FOUND` → `Ok(None)`; an input
  missing from disk → `Ok(None)` with no call.
- `download` — the blobs through the client's batch or ByteStream reads, in the order asked.
- `decode_tree` — `tree_files`.
- `upload` — read each input file from disk and hash it as it is read, keeping the size and mode
  measured on that handle for the input tree; a file whose digest no
  longer matches → `Ok(Uploaded::InputChanged(path))` with nothing written (spec §3). Otherwise
  `FindMissingBlobs` (chunked) over the input files, the output files (bytes from the `BlobSource`),
  each `Tree`, stdout, stderr, every input-root `Directory`, the `Command` and the `Action`; upload
  the missing ones in groups of at most 32 MiB, re-hashing each input as it is sent; then
  `update_action_result` (P2-P4).
Every failure is `Error::Adapter` naming the remote cache, the server's url and the call.

**Proof:** written first, seen failing, then passing (against `TestServer`, `--all-features`) —
- `a_lookup_of_an_unknown_action_misses` — `Ok(None)`
- `an_uploaded_result_is_found_by_the_same_action` — upload then lookup: the hit names the uploaded outputs
- `an_upload_sends_the_action_and_command_before_the_result` — the server's recorded order: blobs and messages before `UpdateActionResult`
- `an_upload_sends_the_input_files_and_directories` — every input blob and `Directory` digest is held by the server afterwards
- `an_upload_sends_only_what_the_server_lacks` — a second upload of the same result writes no blob
- `an_input_changed_on_disk_is_reported_and_nothing_is_written` — `Uploaded::InputChanged(path)`; the server records no write
- `a_download_returns_blobs_in_the_order_asked` — three blobs asked in reverse digest order come back in that order
- `a_server_error_names_the_url_and_the_call` — the error text holds the url and `GetActionResult`

**Done when:** `cargo test -p buildl --all-features remote` passes; `cargo make dod`,
`cargo make guard-adapter-isolation` and `cargo deny check` green. Commit
`feat(buildl): the REAPI remote cache adapter`.

## Risks

- **Most risky: Task 2's digests.** A field encoded in the wrong order or a list not sorted
  gives a digest no other client computes; the stability and spec-table tests pin the shape,
  and plan 08's real-server run is the outside check.
- `Tree.children` order is the client's choice (`re.proto:1459-1465`); a stable order
  (depth-first by name) keeps a target's `Tree` digest the same from run to run.
- Reading input files for upload re-reads the workspace; the hash-as-read check makes a file
  edited mid-build harmless.

## Proof

`cargo make dod`, `cargo deny check` and `cargo make guard-adapter-isolation` green; against
the test server, an upload followed by a lookup of the same action finds the uploaded result,
and the server holds every blob the `Action` names.

## Review findings

- strict-quality — `vocabulary::{Method, Updates}` warned `unreachable_pub` with `test-server` off (the shipped binary's build) — fixed: `cfg_attr(not(feature), expect(unreachable_pub))`; `cargo check -p buildl` clean (blocking)
- records — the constructor, the lazy connect and the extra tests were not in the plan; plan 06's "connect is eager" and plan 08 Task 2's "connects lazily" out of date — fixed: plans 06, 07 and 08 amended (blocking)
- security — `tree_files` expanded a Tree as a graph, so a small hostile Tree hangs the client — fixed: capped at 2^18 entries, `None` past it; 2^40-path test
- strong types — `wire_digest` clamped a too-large size — fixed: fallible, `RemoteError::TooLarge`
- M-PUBLIC-DEBUG — `Reapi` had no `Debug` — fixed: url and root only; token never shown (tested)
- doc — write-group doc overstated the memory bound — fixed: flush before a blob that would pass 32 MiB; doc says what stays held
- error text — url twice in a connect failure; ByteStream errors labelled with batch calls — fixed: url once, `ByteStream.Write`/`Read` named
- correctness — input-tree, store and io failures named no url or call; an input deleted before lookup turned the remote off — fixed: every failure names url and call; a missing input at lookup is a miss with no call (tested)
- test coverage — no test for a file changed between hash and send, nor for more than one write group — fixed: both, each seen failing with its guard removed
- correctness — the upload's input tree re-stat'ed sizes after hashing — fixed: `Input { file, size, mode }` from the hashed handle; `input_tree` does no I/O
- spec §3 — blob order put stdout/stderr before Trees — fixed
- robustness — one `FindMissingBlobs` for every digest could pass 4 MiB — fixed: chunked at `BATCH_BUDGET / 80`; 60,000-digest test (failed before: `decoded message length too large`)
- dead code — `Link::Failed` unreachable — fixed: removed, doc corrected
- test robustness — tight wall-clock bounds (`channel` connect test measured 2.4 s against a 2 s bound under load) — fixed: generous bounds or server-recorded counts
- Fix round gates (run by the fix coders): `cargo make guard-adapter-isolation` → `OK (37 edges)`; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`; `cargo +1.94 check -p buildl --all-targets --all-features` and `cargo check -p buildl` → `Finished`, no warning; `cargo make dod` → `Build Done in 48.83 seconds` (exit 0)

## Probe results

- Risk "Task 2's digests": reviewed against spec §2 and the vendored proto — names sorted by bytes, environment by name, platform by name then value, fields in tag order (P7, P8), salt the 32 key bytes, `Tree.children` depth-first pre-order by name; `an_action_digest_is_stable` and `the_command_matches_spec_section_2` pin it. The outside check is plan 08 Task 4.
- `Action.timeout` is always set: the execution timeout has a default (plan 02).

## Deviations

- 2026-10-26 — Tasks 1 and 2 built in parallel (disjoint files, both modules declared first); committed separately.
- 2026-10-26 — Task 3: `Reapi::new(&RemoteSettings, root)`; the client's lazy connect moved here from plan 08 Task 2.
- 2026-10-26 — one reviewer pass reading the diff only, then one fix round and a test-hardening pass, committed together.
