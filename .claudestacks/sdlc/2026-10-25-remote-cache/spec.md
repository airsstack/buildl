---
status: approved
created: 2026-10-25
built-on: 683db8171593
---

# Spec: remote cache via REAPI

Delivers milestone 6 (design §13 item 6). `buildl build` and `buildl test` share action
results through any server that speaks the Remote Execution API v2 over gRPC. A target the
local cache cannot serve is looked up on the remote under the digest of a real REAPI `Action`;
a hit is downloaded, checked against its digests and stored locally before its row is written;
a miss runs as today, and with uploading turned on its result, its `Action` and everything the
`Action` names are uploaded in the order REAPI requires. Reading is the default; uploading and
strict failure handling are turned on per machine or per run; the server is named in
`buildl.toml` and its secret never is. buildl is a client only. Every decision is rooted in this
chain's intent.

## 1. Premises (proven in this task)

External sources were fetched on 2026-10-25 and read locally: `remote_execution.proto` and
`platform.md` of bazelbuild/remote-apis at commit `c2c07e3d718d3d490a4958344d9c3e47de0bfaf3`
(cited `re.proto:<line>` and `platform.md:<line>`), and the googleapis protos the file imports.
Probes ran in a scratch copy of this workspace at `2095814` with the dependencies and
`deny.toml` of §8; their output is quoted. Every other claim cites a file opened in this task.

|#|Claim|Evidence|
|---|---|---|
|P1|The action cache is keyed by the digest of an `Action`; a missing entry is `NOT_FOUND`.|`re.proto:1783-1784` `action_digest`: "The digest of the [Action] whose result is requested"; `re.proto:169` "`NOT_FOUND`: The requested `ActionResult` is not in the cache"|
|P2|Before `UpdateActionResult`, "the client MUST first upload the [Action] that produced the result, along with its [Command], into the `ContentAddressableStorage`".|`re.proto:177-180`|
|P3|"In order to build an [Action] … the client will need to also upload the [Command] and input root [Directory] … which must also each be uploaded on their own."|`re.proto:204-211`|
|P4|Of `input_root_digest`: "The root directory, as well as every subdirectory and content blob referred to, MUST be in the [ContentAddressableStorage]".|`re.proto:680-687`|
|P5|`Action.salt` places an `Action` "into a separate cache namespace from other instances having the same field contents".|`re.proto:723-729`|
|P6|`Command.environment_variables`, `output_paths`, `Platform.properties` and every child list of a `Directory` must be sorted by code point; `output_paths` deduplicated.|`re.proto:783`, `:862`, `:966-967`, `:992-994`|
|P7|A digest of a message is taken over its encoding with "Fields are serialized in tag order".|`re.proto:1169`|
|P8|prost encodes in tag order even where a message declares fields out of tag order (`Command` declares 7 before 5 and 6).|probe, `Command { arguments, output_paths, platform, working_directory }.encode_to_vec()`: `tags=[1, 5, 6, 7]`|
|P9|`instance_name` may be omitted unless the server requires it.|`re.proto:1776-1781` "The server MAY require use of this field … otherwise it can be omitted"|
|P10|`ActionResult` lists outputs unsorted; files carry `path`, `digest`, `is_executable`; directories carry a `Tree` digest; plus `exit_code`, `stdout_digest`, `stderr_digest`.|`re.proto:1263`, `:1387`, `OutputFile` `:1423-1452`, `Tree` `:1453-1468`, `OutputDirectory` `:1469-1534`|
|P11|Small blobs go through `BatchReadBlobs`/`BatchUpdateBlobs`, large ones through ByteStream `Read` (`{instance}/blobs/{hash}/{size}`) and `Write` (`{instance}/uploads/{uuid}/blobs/{hash}/{size}`, uuid version 4); an empty instance drops the leading slash.|`re.proto:213-232`, `:291-295`|
|P12|The batch limit is `CacheCapabilities.max_batch_total_size_bytes`, 0 meaning none beyond the protocol's; SHA-256 support and AC updates are announced in `CacheCapabilities`.|`re.proto:2416-2431`, `ActionCacheUpdateCapabilities.update_enabled` `:2353-2354`|
|P13|tonic 0.14.6 refuses a received message above 4 MiB by default.|`tonic-0.14.6/src/codec/mod.rs:101` `const DEFAULT_MAX_RECV_MESSAGE_SIZE: usize = 4 * 1024 * 1024;`|
|P14|Platform lexicon: `OSFamily` `linux`, `macos`; `ISA` `arm-a64`, `x86-64`.|`platform.md:11-19`, `:35`, `:46`|
|P15|buildl's platform is `"{std::env::consts::OS}/{std::env::consts::ARCH}"`.|`crates/buildl/src/host/local_host.rs:47`|
|P16|tonic 0.14.6, tonic-prost, prost 0.14, prost-types 0.14, tokio and uuid compile the REAPI v2 and ByteStream protos, also on rust 1.94, and the whole workspace checks with `--all-features`.|probe: `cargo check -p buildl` → `Finished`; `cargo +1.94 check -p buildl` → `Finished`; `cargo check --workspace --all-targets --all-features` → `Finished`|
|P17|With TLS (`tls-ring`, `tls-native-roots`) the current `deny.toml` fails on exactly: ISC (`ring`, `rustls-webpki`, `untrusted`), BSD-3-Clause (`subtle`), duplicates `getrandom` 0.2/0.4 (via `ring`), `syn` 2/3 (`async-trait` 3, `pin-project-internal` 2, both under tonic), `windows-sys` 0.52/0.61 (via `ring`).|probe: `cargo deny --format json check` errors listed, crates traced with `cargo tree -i`|
|P18|With §8's `deny.toml` changes the check passes.|probe: `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`|
|P19|Generating the code at build time (`tonic-prost-build` + `protox`) adds Zlib (`foldhash`), a `tempfile` ban hit (`prost-build`) and a `hashbrown` duplicate; committing the generated code removes all three.|probe: deny output with and without `build.rs`|
|P20|Settling runs on the thread that schedules the build; actions run on workers, which store their outputs in the cas themselves.|`crates/buildl-core/src/execute/assembly.rs:421` (`settle` in `drain`); `perform.rs:326-369`; `Ended::Produced` "all of it is stored" `perform.rs:124-131`|
|P21|`store` places every output and only then records the row.|`crates/buildl-core/src/execute/record.rs:176-199`|
|P22|The local action cache is keyed by label; the content store is `Send + Sync` and holds whole blobs as byte slices.|`ports/action_cache.rs:19-49`; `ports/content_store.rs:24-35`|
|P23|An output is `OutputManifest::File(StoredFile { digest, mode })` or `Tree(entries of path, digest, mode)`, its digest that of the manifest's canonical JSON; a discovery output must be a file.|`crates/buildl-core/src/types/output_manifest.rs`; `types/tree.rs:24-64`; `perform.rs:203`, `:295`|
|P24|A ready action carries the expanded command, its env, input and dependency-output paths and output paths, all workspace-relative; each output path holds a file or a directory of files, its kind not declared.|`crates/buildl-core/src/types/ready_action.rs:18-47`|
|P25|Every strategy but `bare` pins `LC_ALL=C`, `SOURCE_DATE_EPOCH=315532800`, `TZ=UTC`, written in one place.|`crates/buildl-core/src/types/isolation.rs:71-90`|
|P26|`buildl-core` may use no I/O API; adapters in `buildl` may not import each other.|`docs/architecture-building-blocks.md:129`, `:489`; `Makefile.toml:209-211` (`guard-adapter-isolation`)|
|P27|The async-runtime decision record reads "None — threads … Revisit only if remote execution's network fan-out demands it."|`docs/architecture.md:473`|
|P28|The manifest refuses unknown tables (`Raw`) and each table refuses unknown keys.|`crates/buildl/src/manifest/schema.rs:62`, `:86`, `:100` (`deny_unknown_fields`)|
|P29|bazel-remote's current stable release is 2.6.4, available as a Homebrew bottle.|`brew info bazel-remote` → `bazel-remote: stable 2.6.4 (bottled), HEAD`|

## 2. The REAPI `Action` of a target

The core builds a `RemoteAction`, a plain value; the adapter encodes it (§5). Nothing protobuf
crosses into `buildl-core`.

|`Action` / `Command` field|Value|
|---|---|
|`Command.arguments`|the ready action's expanded command (P24)|
|`Command.environment_variables`|the ready action's env, then the strategy's pinned variables over it (P25), sorted by name (P6)|
|`Command.output_paths`|the target's output paths, sorted and deduplicated (P6)|
|`Command.working_directory`|empty: the input root is the workspace root, as every path is workspace-relative (P24)|
|`Action.input_root_digest`|a canonical `Directory` tree (P6) of every declared input at its path and every file of every dependency's outputs at its path|
|`Action.timeout`|`[execution] timeout`, which always has a value (its default when `buildl.toml` sets none)|
|`Action.platform`|`OSFamily` = the OS half of buildl's platform; `ISA` = `arm-a64` for `aarch64`, `x86-64` for `x86_64`, the arch half unchanged otherwise (P14, P15)|
|`Action.salt`|the 32 bytes of the target's buildl action key|

The salt makes everything buildl's key folds in — the tool fingerprint, build settings, network
access, unset variables — part of the `Action` digest (P5), so two targets with different buildl
keys never share a remote entry, whatever the other fields hold.

The `RemoteAction` lists each input-root file with its workspace path and digest, and a
dependency's file with its manifest's executable bit too. The adapter builds the `Directory`
tree: it reads each file's size, and an input's executable bit, from the file on disk — inputs
in the workspace, dependency outputs where they were placed. A file changed on disk since its
digest was taken gives a size that disagrees with its hash; the lookup misses, which is
harmless, because the salt still pins any result to the buildl key. Upload catches the same
case by content (§3).

## 3. One target on a worker

```text
 scheduler: settle ── local row stands ──► restore (as today)
               │ runs(key)
               ▼
 worker:  remote read on and not down?
            ├─ yes: lookup(action) ── hit ──► download, check each digest,
            │            │                     put into the local cas, store the
            │            │                     manifests ──► Ended::Fetched
            │            └─ miss ─┐
            └─ no ────────────────┤
                                  ▼
                        run (as today) ── succeeded and upload on ──► upload
                                  ▼
 scheduler: store (place outputs, then the row) ──► outcome built | fetched
```

- A remote hit returns `Ended::Fetched { outputs, printed }`, which the scheduler records
  exactly as `Ended::Produced` (P21): outputs placed, then the row written. A crash in between
  leaves no row, so the next build downloads again. Its outcome class is `fetched`.
- A hit is usable only when `exit_code` is 0 and every declared output path appears exactly
  once across `output_files` and `output_directories`, with no output symlinks and no other
  path; a directory's `Tree` must hold only regular files and no empty directory, and a
  discovery target's output must be a file (P23), as a local run would require. Anything else
  is a remote failure (§6).
- Every downloaded blob, `Tree` messages included, is checked by the core: its length is the
  digest's size and its SHA-256 is the digest's hash. A mismatch is a remote failure, and the
  blob is never stored.
- Upload happens on the worker, after the outputs and logs are in the local cas and before the
  result goes back to the scheduler, in this order (P2, P3, P4): the blobs the server is missing
  among the input files, the output files, the `Tree` of each directory output, stdout and
  stderr, the input-root `Directory` messages, the `Command` and the `Action`; then
  `UpdateActionResult`. A failed action is never uploaded, and neither is a target restored
  from the local cache: only an action that ran.
- An input file is read from disk for upload and hashed as it is read. When its bytes no
  longer match the digest the key holds, that target's upload is skipped with a warning naming
  the target and the file; the remote stays on, since the server did nothing wrong.
- The upload precedes the scheduler's `store`. If `store` then fails (`mismatched`,
  `unstored`, `record.rs:190-199`), the remote holds a sound result for a target whose local
  record failed. That is intended: the content is what the action produced, and no failure is
  cached anywhere.
- A target served from the remote is not reported as started; it is reported when it ends.

## 4. Core: the port and the policy

```text
 buildl-core/src/ports/remote_cache.rs
   trait RemoteCache: Send + Sync            (blocking; workers call it concurrently)
     lookup(&RemoteAction)          -> Result<Option<RemoteHit>>
     download(&[Blob])              -> Result<Vec<Vec<u8>>>          (same order)
     decode_tree(&[u8])             -> Result<Option<Vec<RemoteTreeFile>>>
                                       (None: holds a symlink or an empty directory)
     upload(&RemoteAction, &RemoteUpload, &dyn BlobSource) -> Result<Uploaded>
                                       (Uploaded::Done | InputChanged(path))

 buildl-core/src/types/
   RemoteAction   command, env, outputs, input-root files, platform, timeout, salt
   RemoteHit      exit code; output files (path, blob, mode); output directories
                  (path, tree blob); whether output symlinks were present; stdout, stderr
   RemoteUpload   what a run produced, as the ActionResult will name it
   Blob           a digest with its size
   RemoteUse      Off | Read | ReadWrite
   RemoteFailure  Lenient | Strict
```

- The adapter only transports and encodes; every check of §3 — exit code, the outputs
  matching the declared paths, digests, a discovery output's kind — is the core's, so each is
  tested against a fake port.
- `Ports` gains `type Remote: RemoteCache`. A workspace with no `[remote]` table, and every
  command but `build` and `test`, uses `RemoteUse::Off`, and the core never calls the port.
- The adapter answers a miss with `Ok(None)` and every other failure with
  `Error::Adapter { port: PortName::RemoteCache, … }`. The core decides what a failure does
  (§6).
- `BlobSource` hands the adapter the output and log blobs from the local content store, which
  the remote adapter may not import (P26). Input files the adapter reads from the workspace.
- Lookup, download and upload run on worker threads (P20), so the scheduler loop is unchanged
  but for the new `Ended::Fetched` arm. The flag that turns the remote off after a lenient
  failure lives beside `Shared::stop`.

## 5. The adapter: `crates/buildl/src/remote/`

|Concern|Design|
|---|---|
|Runtime|a multi-thread tokio runtime created and owned by the adapter; worker threads call it through `block_on`; nothing outside the module sees tokio. Amends the record of P27.|
|Channel|`grpc://host:port` is plain HTTP/2, `grpcs://host:port` is TLS with the system roots plus `[remote] ca` when set. One channel per build, shared by the workers.|
|Login|every call carries the header `[remote] header` (default `authorization`) holding `Bearer <token>` for `authorization` and the bare token for any other name. The token is read once, from `token.env` or `token.file`.|
|Capabilities|`GetCapabilities` once per build, before the first call that needs it: SHA-256 must be among the digest functions, and uploading requires `update_enabled` (P12); otherwise a remote failure.|
|Batching|a blob goes into a batch call while the batch's data stays at or under `max_batch_total_size_bytes` (0 = none) and its message — each blob's size plus 128 bytes of per-entry framing — stays under tonic's 4 MiB message cap less 64 KiB of headroom (P12, P13); a batch read that names one digest twice asks for it once and answers every position; a larger blob uses ByteStream, written in 1 MiB chunks under a fresh version 4 UUID (P11).|
|Upload|`FindMissingBlobs` over everything §3 lists, the missing ones uploaded, then `UpdateActionResult`.|
|Download|`GetActionResult`; then, through the core, each `Tree` blob, then the file blobs.|
|Timeout|`[remote] timeout` (default 30 s) bounds the connection (TCP, TLS and HTTP/2 handshakes together), each unary call whole, and each message of a ByteStream `Read` or `Write` (a `Write` by the time since tonic last pulled a chunk), so a large transfer that keeps moving is never cut off.|
|Input tree|built here from the `RemoteAction`'s files with the sizes and executable bits read from disk (§2); input bytes read and hashed here for upload (§3).|
|Encoding|`RemoteAction` and `RemoteUpload` become protobuf here; prost's tag-order encoding (P7, P8) makes the digests stable.|
|Generated code|the REAPI v2, ByteStream and imported googleapis protos are vendored under `crates/buildl/proto/` at the commit of §1, and the Rust prost and tonic generate is committed under `crates/buildl/generated/reapi/`, outside `src/` because its `super::super` paths are refused there by `guard-adapter-isolation`; `src/remote/proto.rs` pulls each file in with `include!` and carries the lint suppressions the generated code needs, each with its reason. A generator kept outside the workspace regenerates it, so its build tools never enter `cargo deny` (P19).|

## 6. Configuration, switches, failures

```toml
[remote]
url      = "grpcs://cache.example.com:443"
instance = "ci"                           # optional; omitted → empty (P9)
ca       = "certs/ca.pem"                 # optional; workspace-relative or absolute
token    = { env = "BUILDL_REMOTE_TOKEN" } # or { file = "/abs/or/workspace/relative" }
header   = "authorization"                # optional
timeout  = "30s"                          # optional
```

- The table is read strictly like every other (P28): an unknown key, a scheme other than
  `grpc` or `grpcs`, a `token` with both or neither of `env` and `file`, a `token` with a
  `grpc://` url (a secret is never sent unencrypted), or a `timeout` that is not a duration is
  an error naming the key, line and column.

|Switch|Per machine|Per run|
|---|---|---|
|upload|`BUILDL_REMOTE_UPLOAD=1`|`--remote-upload`|
|strict|`BUILDL_REMOTE_STRICT=1`|`--remote-strict`|
|remote off|—|`--no-remote`, which overrides both|

- Each variable is `1` or `0`, anything else an error naming it; a flag turns its switch on
  whatever the variable says. Either flag without a `[remote]` table, or `--no-remote` with
  either flag, is a usage error.
- Uploading is refused when `[execution] strategy = "bare"`: an action outside the sandbox can
  read undeclared inputs, and its result must not reach other machines (design §11.2). Reading
  works under every strategy.
- A token named but missing — the variable unset, the file unreadable — is an error before
  anything runs, unless `--no-remote`. The token never appears in output, the event log or
  errors.

|Remote failure (call error, timeout, unusable result, digest mismatch)|Lenient (default)|Strict|
|---|---|---|
|during lookup or download|one warning naming the server and the call; the target runs locally; the remote is off for the rest of the build|the build fails, naming the server and the call|
|during upload|one warning; the target's local outcome stands; the remote is off for the rest of the build|the build fails, naming the server and the call|

- The summary counts `fetched` beside `built` and `cached`; the event log records the class
  `fetched` with the key and the worker's span.
- `buildl doctor` prints the remote's `url`, `instance`, `ca`, `header`, `timeout`, whether the
  token is set, and upload and strict as this machine's variables set them, noting that the
  flags can turn either on for one run. It connects to nothing.
- `plan`, `verify`, `query`, `graph`, `check`, `log`, `clean` and `gc` never connect.

## 7. Errors and exit codes

A manifest, variable or flag error is the existing usage/manifest error. A strict remote
failure is a build error with `PortName::RemoteCache`, exiting as other port failures do, and so
is a remote cache that cannot be set up before the build starts (an unreadable `ca`, say),
strict or not. Nothing else in the exit code table changes.

## 8. Dependencies

|Crate|Where|Why|
|---|---|---|
|`tonic` 0.14 (`channel`, `codegen`, `tls-ring`, `tls-native-roots`)|`buildl`|the gRPC client|
|`tonic-prost`, `prost` 0.14, `prost-types` 0.14|`buildl`|the generated messages and their codec|
|`tokio` (`rt`, `rt-multi-thread`, `time`)|`buildl`|the runtime the client needs, owned by the remote adapter|
|`uuid` (`v4`)|`buildl`|ByteStream upload resource names (P11)|
|feature `test-server` of `buildl` (`tonic/server`, `tonic/router`, `tokio/net`)|`buildl` tests; `buildl-cli` dev-dependency|the in-test REAPI server, one source for both crates' tests, never in the shipped binary|

`deny.toml` (proven by P18): `BSD-3-Clause` and `ISC` join `licenses.allow`; `skip` gains
`getrandom@0.2.17`, `syn@2.0.119` and `windows-sys@0.52.0`, each with its reason; `bans.deny`
gains `tonic` (wrappers `buildl`, `tonic-prost`), `prost` (`buildl`, `prost-types`,
`tonic-prost`) and `uuid` (`buildl`). `tokio` is not wrapper-banned: its direct dependents are
tonic's own transport crates, so its confinement follows from tonic's.

## 9. Code shape (guideline-driven)

- Strong types (rust-guidelines `strong-types.md`): `RemoteUse` and `RemoteFailure` are enums,
  never `bool` flags; `RemoteUrl` parses its scheme at construction; `Blob` pairs a digest with
  its size so neither travels alone.
- `mod.rs` files are tables of contents only (`mod-rs-export-only.md`); `remote/` holds one file
  per concern: channel and login, capabilities, the input tree, encoding, transfer, the port
  implementation, the test server.
- Static dispatch (`static-dispatch.md`): the port is an associated type of `Ports`; the only
  new `dyn` is `BlobSource`, a callback handed across the port.
- Every logic file carries colocated unit tests (`unit-test-mandate.md`).

## 10. Testing

|Layer|What it proves|
|---|---|
|`buildl-core`, fake `RemoteCache`|hit → fetched, the row after the content; miss → run; upload order, and that failures and restored targets are not uploaded; `InputChanged` → warning, remote still on; every unusable result of §3; digest mismatch, `Tree` blobs included; lenient warning once and the remote off afterwards; strict failure; `Off` never calls the port; the `RemoteAction` of §2 (sorting, pinned env, salt, platform mapping, file list)|
|`buildl-core`, early cutoff|a dependency fetched from the remote gives its dependents the same finished keys as the same dependency built locally; a graph whose every target the fake holds finishes with no strategy call|
|`buildl-core`, keys|the same plan built with `Off` and with `Read` against an empty fake gives byte-identical keys and rows|
|`buildl` adapter, in-test server on loopback|every call of §5; the input tree's sizes and executable bits; batch versus ByteStream at the threshold; capabilities refusal; header and token; TLS against a committed test certificate; per-message timeout|
|`buildl-cli`, the binary against the same server|`fetched` in the summary; upload through `--remote-upload` and through `BUILDL_REMOTE_UPLOAD`; `--no-remote`; strict failure exit; uploading refused under `bare`; a token with `grpc://` refused; `doctor` output; the token absent from all output and from the event log; manifest and variable errors|
|Real server, once during execution|a build seeding a local `bazel-remote` 2.6.4 (P29) and a second checkout fetching every target from it, output kept as evidence; not part of CI|

No test reaches the internet.

## 11. Documents

Design §4 (`[remote]`), §7 (the switches and `fetched`), §11.2 and §13 item 6 (built);
`architecture.md` §3.4 (the worker's remote steps), §3.5, §6 (the async-runtime record
amended: a runtime confined to the remote adapter); `architecture-building-blocks.md` (the
remote cache as an external system, no longer "later"; the port catalog; the dependency
rules); `README.md`; `CLAUDE.md` Status; the crate docs.
`.claudestacks/sdlc/roadmaps/buildl/06-remote-cache.md` is unchanged: a milestone file
carries no progress.

## 12. Risks the plans carry

- **Calling `block_on` from several worker threads at once** is the design of §5; its
  behaviour under tonic is proven by the plan's first adapter task before anything is built
  on it.
- **Generated code and the lint gate:** which `#[expect]`s the generated code needs is known
  only once it is generated under the workspace lints; the plan settles it.
- **Servers other than bazel-remote** are covered by following the protocol's MUSTs (P2–P4),
  not by a run against each; a refusal found by the real-server run is closed in this chain.

## 13. Non-goals

- Remote execution, the `Execution` service, toolchain containers (milestone 08).
- The Bazel HTTP cache protocol, compressed blobs, and any second backend.
- Client certificates (mTLS) and any login beyond a header token.
- Matching keys across machines whose declared environment or platform differ.
- Evicting remote entries; the Remote Asset and Logstream APIs.
- Sharing entries with Bazel or another REAPI client.
- Shipping or operating a REAPI server.
