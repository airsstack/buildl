---
status: approved
created: 2026-10-25
depends-on: [02, 06]
built-on: 867ed49b84d7
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

**Change:** `Reapi::new(client, root)` implements `RemoteCache`:
- `lookup` — encode the action, `get_action_result`, `hit`; `NOT_FOUND` → `Ok(None)`.
- `download` — the blobs through the client's batch or ByteStream reads, in the order asked.
- `decode_tree` — `tree_files`.
- `upload` — read each input file from disk and hash it as it is read; a file whose digest no
  longer matches → `Ok(Uploaded::InputChanged(path))` with nothing written (spec §3). Otherwise
  `FindMissingBlobs` over the input files, the output files (bytes from the `BlobSource`), each
  `Tree`, stdout, stderr, every input-root `Directory`, the `Command` and the `Action`; upload
  the missing ones; then `update_action_result` (P2-P4).
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
