---
status: approved
created: 2026-10-25
milestone: buildl/06
---

# Intent: remote cache via REAPI

## Problem

What one machine builds, no other machine can reuse.

- **The cache stops at the checkout.** The action cache and the cas live under `.buildl/`
  of one workspace. CI builds `main`, then every developer builds the same targets under
  the same keys again. Design §11.2 promises that the first build of the day is "mostly
  downloads", and nothing downloads.
- **The keys are already shareable, and nothing shares them.** Every digest is SHA-256,
  chosen over blake3 for REAPI compatibility (`architecture.md` §6), so that the remote
  step is "a transport problem, not a re-keying" (`architecture.md` §2). That transport
  does not exist.
- **There is no backend to point at.** Design §11.2 says "don't build the cluster": speak
  the Remote Execution API as a client, so that existing servers (bazel-remote, Buildbarn,
  Buildfarm, BuildBuddy, NativeLink) become buildl's backends. buildl speaks no remote
  protocol, and `buildl.toml` has no table to name a server (an unknown table is refused,
  design §4).
- **A shared cache trusts what others wrote.** A remote entry says "key K produced these
  bytes", and every machine computing K takes those bytes. A machine that leaks an
  undeclared input into an output, or a compromised one, would hand its result to the
  whole team. Nothing says who may write.

Agreed with the user, and overridable:

- **Protocol:** REAPI v2 over gRPC, the protocol design §11.2 and milestone 06 name and
  the one remote execution (milestone 08) extends. The Bazel HTTP cache protocol is not
  spoken.
- **Remote key:** the key a remote cache entry is stored under is the one REAPI defines,
  the digest of a REAPI `Action`, so that remote execution reuses the entries this
  milestone writes without changing their keys.
- **Who writes:** reading is the default; uploading is turned on explicitly, per machine
  or per run (typically on CI), without editing the `buildl.toml` that every machine
  shares.
- **When the remote fails:** a remote that is down, slow, or answers with content that
  does not match its digest counts as a miss: the target runs locally and the build warns.
  A strict setting makes a remote failure fail the build, for CI checks that rely on hits.
- **Logging in:** TLS to the server, with a custom CA allowed, and a secret token sent as
  a request header. The secret is read from an environment variable or a file, never
  written in `buildl.toml`.
- **Commands:** `build` and `test` read from the remote and, when uploading is on, write
  to it. `verify` stays local: it checks what this machine produces. `doctor` prints the
  remote settings. `plan`, `clean` and `gc` do not reach the remote.

## Affected systems

- `buildl-core` — settling a target consults the remote after the local cache and before
  running; recording uploads after the local row; the policy (read, upload, strict) and
  its failure handling, testable against a fake remote.
- `buildl` — the REAPI client adapter, the `[remote]` table of the manifest reader, and
  reading the secret.
- `buildl-cli` — turning upload and strict on per run; remote settings in `doctor`;
  remote hits and remote warnings in the build report.
- Workspace `Cargo.toml` and `deny.toml` — the gRPC and TLS dependencies.
- `docs/design.md` §4, §7, §11.2, §13 item 6; `docs/architecture.md` (§6 decision
  records); `docs/architecture-building-blocks.md` (the remote cache as an external
  system, no longer "later"); `.claudestacks/sdlc/roadmaps/buildl/06-remote-cache.md`;
  `README.md`; `CLAUDE.md` Status; the crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|Naming a server|`buildl.toml` names a REAPI server; a workspace that names none behaves exactly as today.|
|Remote hit|a target missing from the local cache whose key the remote holds is not run: its outputs and logs are downloaded into the local cas and placed, and the local row is written after the content, so a crash leaves no row.|
|Remote miss|the target runs locally as today.|
|Upload|with uploading on, a target that ran and succeeded leaves its content and then its result on the remote; a failed action is never uploaded. With uploading off, nothing is written to the remote.|
|Early cutoff|a dependency served from the remote finishes its dependents' keys exactly as a local build would; a whole graph CI has built is downloads only.|
|Trust|downloaded content is checked against its digest before it is stored; content that does not match is a miss, never a hit.|
|Failure|a remote that is down, slow or wrong is a miss plus a warning; under the strict setting it fails the build, naming the server and what failed.|
|Login|TLS with an optional custom CA, and a header token read from an environment variable or a file; a token never appears in `buildl.toml`, `doctor` output or logs.|
|Report|the build report says which targets came from the remote, and `doctor` prints the server, the upload and strict settings, and whether a token is set.|
|Keys|turning the remote on or off changes no action key and no local row format.|
|Tests|core tests against a fake remote, adapter tests against a REAPI server run inside the tests, and CLI tests of the binary cover each row above; no test reaches the internet.|
|Documents|the design, both architecture documents, README and `CLAUDE.md` describe the remote cache as built.|

## Constraints

- buildl is a REAPI client only; it ships no server and no worker.
- The remote cache key is the digest of a REAPI `Action`, as the REAPI spec defines it;
  everything buildl's own action key folds in is reflected in it, so two targets with
  different buildl keys never share a remote entry.
- Nothing reaches the network while build files are read, and Lua never learns that a
  remote exists: the remote is a host-side concern (design §11.2).
- The crash-safety invariant holds across the remote: content before the row it names,
  locally and remotely; failures are never cached anywhere.
- Determinism (`architecture.md` §5): no `HashMap` iteration reaches output, JSON through
  the one canonical serializer, and parallel downloads or uploads are never observable in
  what is built.
- `architecture.md` §6 settles plain threads and no async runtime. A gRPC client may need
  one; if so, the decision is amended by a recorded decision, and the runtime stays inside
  the remote adapter, the way `rayon` stays inside the `Digester`. `buildl-core` stays
  synchronous and free of any I/O API, `tokio` included (building blocks §7); its ports
  stay blocking.
- The structural rules hold: logic in `buildl-core`, testable against fake ports; network
  code only in `buildl`; adapter modules of `buildl` do not import each other
  (`cargo make guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` green before every commit; every new dependency
  carries its reason in the workspace `Cargo.toml`; work on this branch, never `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Remote execution and toolchain containers (milestone 08).
- The Bazel HTTP cache protocol, and any second backend protocol.
- Client certificates (mTLS) and login flows beyond a header token.
- Making keys match across machines whose declared environment or platform differ; a
  macOS developer and a Linux CI share no entries.
- Evicting or garbage-collecting remote entries, and the Remote Asset and Logstream APIs.
- Sharing cache entries with Bazel or any other REAPI client.
- Shipping or operating a REAPI server.

Candidate direction: a `RemoteCache` port in `buildl-core` consulted by settle and
record, with one gRPC REAPI adapter in `buildl`; design settles how a target becomes a
REAPI `Action` and whether the gRPC client needs an async runtime.
