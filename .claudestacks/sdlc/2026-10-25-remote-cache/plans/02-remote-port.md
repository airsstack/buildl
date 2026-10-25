---
status: done
created: 2026-10-25
built-on: 3d9113128fe5
---

# Remote Port Implementation Plan

**Goal:** `buildl-core` describes a target's REAPI action as a `RemoteAction` behind a `RemoteCache` port.

**Architecture:** The port (`ports/remote_cache.rs`) and its values (one type per file under
`types/`, as the crate keeps them) are plain Rust: nothing protobuf, no I/O (spec §4, P26).
`Ports` gains `type Remote: RemoteCache` and `PortSet` a `remote` field; every bundle gets
`NoRemote`, a pure port that holds nothing, until plan 08 wires the real adapter into
`LocalPorts`. The `RemoteAction` of a task is built on the worker by
`execute/remote_action.rs` from what the worker already holds: the task's finished key
(`KeyComponents`, `types/action_key.rs:80-107`), its `ReadyAction` (`types/ready_action.rs:26`),
the workspace's `Execution` (`types/workspace.rs:50-56`) and the content store, which holds
each dependency's output manifest. The platform mapping of spec §2 is decided here, in the
core, as spec §10 places it; the adapter only encodes it. Nothing calls the port yet.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `cargo make dod`, `cargo deny check`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2 (the `Action`'s fields), §4 (the port and its values), §9, §10
(core row: the `RemoteAction`, platform mapping), P5, P6, P14, P15, P23-P25.

---

## Files that change

```
crates/buildl-core/src/types/blob.rs             — [create] Blob: a digest with its size
crates/buildl-core/src/types/root_path.rs        — [create] RootPath: a workspace-relative file of the input root
crates/buildl-core/src/types/remote_action.rs    — [create] RemoteFile, RemotePlatform, RemoteAction
crates/buildl-core/src/types/remote_hit.rs       — [create] RemoteOutputFile, RemoteOutputDirectory, RemoteHit, RemoteTreeFile
crates/buildl-core/src/types/remote_upload.rs    — [create] RemoteUpload, Uploaded
crates/buildl-core/src/types/remote_policy.rs    — [create] RemoteUse, RemoteFailure, RemotePolicy
crates/buildl-core/src/types/remote_notice.rs    — [create] RemoteCall, RemoteNotice
crates/buildl-core/src/types/mod.rs              — [modify] declare and re-export
crates/buildl-core/src/ports/remote_cache.rs     — [create] trait RemoteCache, trait BlobSource
crates/buildl-core/src/ports/no_remote.rs        — [create] NoRemote: every lookup misses, nothing is uploaded; unit tests
crates/buildl-core/src/ports/mod.rs              — [modify] export both
crates/buildl-core/src/ports/bundle.rs           — [modify] `type Remote: RemoteCache` on Ports (:267) and the doc example (:249)
crates/buildl-core/src/ports/port_set.rs         — [modify] PortSet (:20) gains `remote: P::Remote`; test bundle (:312) gains `type Remote = NoRemote`
crates/buildl-core/src/error.rs                  — [modify] PortName (:732) gains RemoteCache; display table (:1931-1945)
crates/buildl-core/src/execute/remote_action.rs  — [create] remote_action(): a task's RemoteAction; unit tests
crates/buildl-core/src/execute/mod.rs            — [modify] declare it (crate-private)
crates/buildl-core/src/types/platform.rs         — [modify] `os()` and `arch()` accessors
crates/buildl-core/src/lib.rs                    — [modify] re-export the public types and ports
crates/buildl-core/tests/flows/common.rs         — [modify] FakePorts (:1156) `type Remote = NoRemote`; PortSet literal (:1202)
crates/buildl/src/local_ports.rs                 — [modify] LocalPorts (:50) `type Remote = NoRemote`; PortSet literal (:423)
crates/buildl/src/report/plan_text.rs            — [modify] Planning test bundle (:365) `type Remote = NoRemote`
crates/buildl-cli/src/exit.rs                    — [modify] the PortName list (:696-708) gains RemoteCache
crates/expected-module-edges.txt                 — [modify] only if `cargo make guard-module-edges` reports a new buildl-core edge
```

The five `Ports` impls are every hit of `grep -rn "impl.* Ports for" crates/` (control: the
same search finds `LocalPorts`, `local_ports.rs:50`); the four `PortSet` literals are every
hit of `grep -rn "PortSet\(::<[A-Za-z]*>\)\? {" crates/` that is not a destructuring
(`common.rs`, `local_ports.rs`, the `port_set.rs` test, and `plan_text.rs`'s `PortSet::<Planning> {`).

## Order of work

### Task 1 — The remote values

**Files:** the seven `types/` files above, `types/mod.rs`, `lib.rs`.

**Change:** introduce, each with one purpose (spec §4):
- `Blob` — a `Digest` with its size in bytes; the pair REAPI calls a digest (`re.proto:1177`).
- `RootPath` — a workspace-relative file of the input root, made from a `SourcePath` or from an
  `OutputPath` (a tree entry's path is made by the existing `OutputPath::entry`,
  `types/output_path.rs:46-50`, first); compares by UTF-8 bytes.
- `RemoteFile` — `path: RootPath`, `digest`, `mode: Option<FileMode>` (`None` for a declared
  input, whose bit the adapter reads from disk, spec §2).
- `RemotePlatform` — `os_family` and `isa`, the two REAPI properties (P14).
- `RemoteAction` — `salt: Digest` (the buildl key), `command: Command`,
  `env: BTreeMap<EnvName, EnvValue>`, `outputs: Vec<OutputPath>`, `files: Vec<RemoteFile>`,
  `platform: RemotePlatform`, `timeout: Timeout`.
- `RemoteOutputFile` (`path: String` as the server wrote it, `blob`, `mode`),
  `RemoteOutputDirectory` (`path: String`, `tree: Blob`), `RemoteHit` (`exit_code: i32`,
  `files`, `directories`, `symlinks: bool`, `stdout: Option<Blob>`, `stderr: Option<Blob>`):
  what `GetActionResult` answered, unchecked (P10). `RemoteTreeFile` — `path: TreePath`,
  `blob`, `mode`.
- `RemoteUpload` — what a run produced, as an `ActionResult` will name it: per declared output
  a `RemoteOutput { path: OutputPath, content: RemoteContent }`, where `RemoteContent` is
  `File { blob, mode }` or `Tree(RemoteTree)` (a `RemoteTree` holds `RemoteTreeFile`s sorted by
  path on construction) — each file's digest with its size, which an REAPI `OutputFile` needs and
  `OutputManifest` does not hold — plus stdout and stderr as `Blob`s. `Uploaded` — `Done` or `InputChanged(RootPath)`.
- `RemoteUse` (`Off`, `Read`, `ReadWrite`), `RemoteFailure` (`Lenient`, `Strict`),
  `RemotePolicy { use_, failure }` with `RemotePolicy::OFF`.
- `RemoteCall` (`Capabilities`, `Lookup`, `Download`, `Upload`) and `RemoteNotice` (a failure:
  `call`, `diagnostic`; or `InputChanged { label, path }`): what a warning says (spec §6).

**Proof:** written first, seen failing, then passing —
- `a_root_path_of_a_tree_entry_is_its_output_path_entry` — `out/gen` entry `a/b.txt` → `out/gen/a/b.txt`
- `root_paths_sort_by_bytes` — `B` before `a`, `a/b` before `a0`
- `the_off_policy_reads_and_writes_nothing` — `RemotePolicy::OFF.use_` is `Off`

**Done when:** `cargo test -p buildl-core types::` passes; `cargo make dod` green. Commit
`feat(buildl-core): values describing a remote action and its result`.

### Task 2 — The `RemoteCache` port and `NoRemote`

**Files:** `ports/remote_cache.rs`, `ports/no_remote.rs`, `ports/mod.rs`, `ports/bundle.rs`,
`ports/port_set.rs`, `error.rs`, `lib.rs`, every `Ports` impl and `PortSet` literal listed
above, `buildl-cli/src/exit.rs`.

**Change:** `RemoteCache: Send + Sync` with `lookup`, `download`, `decode_tree` and `upload`
exactly as spec §4 names them; `BlobSource: Sync` with one method reading a blob by digest
(`Result<Option<Vec<u8>>>`). `PortName::RemoteCache`, displayed `remote cache`; an
`Error::Adapter` naming it maps to the infrastructure exit like every other port but the
manifest (`exit.rs:108-110`). `NoRemote`: `lookup` → `Ok(None)`, `download` of anything → an
`Error::Adapter` (nothing is held), `decode_tree` → `Ok(None)`, `upload` → `Ok(Uploaded::Done)`.
Every bundle names `type Remote = NoRemote` and every `PortSet` literal holds `NoRemote`.

**Proof:** written first, seen failing, then passing —
- `no_remote_never_holds_a_result` — `lookup` of any action is `Ok(None)`
- `no_remote_holds_no_blob` — `download` of one blob is an `Error::Adapter` naming the remote cache
- `the_remote_cache_port_is_named_in_errors` — `PortName::RemoteCache` displays `remote cache`
- the existing suites stay green.

**Done when:** `cargo make dod` green. Commit `feat(buildl-core): the remote cache port`.

### Task 3 — A task's `RemoteAction`

**Files:** `execute/remote_action.rs`, `execute/mod.rs`, `expected-module-edges.txt` if needed.

**Change:** the crate-private `remote_action(task: &Task, execution, cas)` returns the `RemoteAction` of spec §2:
`salt` the key's digest; `command` the ready action's command; `env` the ready action's env
with `execution.strategy.pinned()` (`isolation.rs:71-90`) laid over it; `outputs` the ready
action's outputs sorted and deduplicated by bytes (P6); `files` every declared input (from the
key's own inputs: path and digest, mode `None`) and every file of every dependency output (the
key's `deps` paired in order with the ready action's `deps` paths; each output's manifest read
from `cas` by its digest through `OutputManifest::from_bytes`; a file output at its path, a tree
entry at `OutputPath::entry`, with the manifest's mode), sorted by `RootPath`; `platform` from
the key's platform through `Platform::os()`/`arch()` into `RemotePlatform::of` (private fields,
`&str` accessors): `os_family` the OS half, `isa` `arm-a64` for `aarch64`, `x86-64`
for `x86_64`, the arch half unchanged otherwise (P14, P15); `timeout` `execution.timeout`. A
dependency manifest the store does not hold, or cannot read, and a ready action whose `deps`
count differs from the key's dependency outputs, are each `Error::Adapter` naming the content store.

`execution.timeout` always has a value — the manifest fills the default
(`interpret.rs:304-307`; `Timeout::default()` is 10 minutes, `timeout.rs`) — so spec §2's
"`[execution] timeout` when set" is always met and the `Action` always carries it.

**Proof:** written first, seen failing, then passing (unit, with a content store fake local to
the test module — `execute/fixtures.rs` holds none) —
- `the_salt_is_the_buildl_key` — `salt` equals the key's digest
- `pinned_variables_override_declared_ones_under_the_sandbox` — declared `TZ=X`, sandbox → `TZ=UTC`; `bare` keeps `TZ=X`
- `outputs_are_sorted_and_deduplicated`
- `inputs_and_dependency_files_form_one_sorted_list` — two dependencies of two outputs each, one a tree: entries under the output path with their modes; inputs carry no mode; the list sorted
- `the_platform_maps_os_and_arch` — `macos/aarch64` → `macos`, `arm-a64`; `linux/x86_64` → `linux`, `x86-64`; `linux/riscv64` → `linux`, `riscv64`
- `a_dependency_manifest_not_held_is_a_content_store_error`, and the same for bytes that are not a manifest
- a ready action and key whose dependency counts differ is a content store error
- `the_action_carries_the_execution_timeout`

**Done when:** `cargo test -p buildl-core execute::remote_action` passes; `cargo make dod` and
`cargo make guard-module-edges` green. Commit `feat(buildl-core): describe a task as a REAPI action`.

## Risks

- `NoRemote` in `LocalPorts` is temporary: plan 08 replaces it with the REAPI adapter. Until
  then the remote is never consulted, so nothing a user sees changes.
- Pairing the key's `deps` with the ready action's `deps` paths relies on both being in the
  order the dependencies were written (`action_key.rs:105`, `ready_action.rs:37-39`); Task 3's
  two-dependency test pins it.

## Proof

`cargo make dod`, `cargo make guard-module-edges` and `cargo deny check` green; the
`RemoteAction` of a target with inputs, a file dependency and a tree dependency matches spec
§2 field for field.

## Review findings

- visibility — `execute::remote_action` shipped public; the plan says crate-private — `execute/mod.rs:26` — fixed: `pub(crate)`, takes `&Task`; verified `cargo test -p buildl-core` → `868 passed; 0 failed` / `244 passed; 0 failed` / `16 passed; 0 failed`
- correctness — `ready.deps` zipped with the key's dependency outputs dropped a length mismatch silently — `execute/remote_action.rs:58` — fixed: `Error::Adapter` naming the content store, tested; same run
- unit-test-mandate — "is not a manifest" branch untested — `execute/remote_action.rs:88` — fixed; same run
- strong-types — `Platform` string re-split with an impossible fallback — `execute/remote_action.rs:110` — fixed: `Platform::os()`/`arch()`; same run
- strong-types — `RemotePlatform` bare public `String` fields — `types/remote_action.rs:31` — fixed: private, `RemotePlatform::of`; same run
- doc-comment-discipline — `RemoteContent::Tree` order stated, not enforced — `types/remote_upload.rs:24` — fixed: `RemoteTree` sorts on construction, tested; same run
- style — over-wide doc-example import — `ports/bundle.rs:36` — fixed
- style — split import — `ports/port_set.rs:109` — fixed
- spec — four deviations not written back to plans 02 and 04 — fixed: both plans amended
- Fix round gates: `cargo make dod` exit 0, `cargo make guard-module-edges` exit 0, `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`

## Probe results

- "`execute/fixtures.rs` holds a content store fake" — reviewer search: no `ContentStore` fake there; each fake is local to its test module — AGAINST the plan (Task 3)
- "two `PortSet {` literals" — reviewer search: four constructing literals (one spelt `PortSet::<Planning> {`) — AGAINST the plan (Task 2)

## Deviations

- 2026-10-25 — Task 1: `RemoteUpload` carries `RemoteOutput { path, content: RemoteContent }` with `RemoteTree`, not `OutputManifest`, because an REAPI `OutputFile` needs each file's size. Plans 02 and 04 amended.
- 2026-10-25 — Task 3: `remote_action` takes `&Task`, is crate-private, and carries `#[cfg_attr(not(test), expect(dead_code))]` until the worker calls it (plan 03 Task 3 removes it).
- 2026-10-25 — Fix round: `Platform::os()`/`arch()`, `RemotePlatform::of`, `RemoteTree` added (`1cad25d`).
- 2026-10-25 — one reviewer pass over the plan's three tasks together, then one fix commit.
