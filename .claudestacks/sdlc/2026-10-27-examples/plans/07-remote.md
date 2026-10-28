---
status: done
created: 2026-10-27
depends-on: [03]
built-on: 5947af3e315f
---

# Remote cache and execution example — Implementation Plan

**Goal:** an example shows a Python project executed on, then fetched from, a NativeLink server it starts itself, proven by a Linux CI job.

**Architecture:** `examples/remote/` is the Python starter's sources (plan 03) plus a `[remote]`
table (`url = "grpc://127.0.0.1:50051"`) and a `docker/` directory: a Dockerfile putting
NativeLink's static binary on a Debian base with `python3` installed (the shape of
`ci/nativelink.Dockerfile`), a NativeLink config with a local worker (trimmed from
`ci/nativelink.json5`), and a `compose.yaml` starting it on `127.0.0.1` only. Its `assert.sh`
runs the four-copy sequence of spec §5; a CI job `examples-remote` on `ubuntu-latest` runs it
(not `check.sh`, which `cargo make examples` leaves out for `remote`). `ci/` stays as it is for
buildl's own `real_remote` tests.

**Tech Stack:** Docker, NativeLink v1.4.0 (as pinned in `ci/nativelink.Dockerfile`), Python 3.12,
the `buildl` binary, GitHub Actions.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §2, §5, §8. Gates for every task:
`cargo make dod` and `cargo deny check` green; Conventional Commit, one per task.

---

## Files that change

```
examples/remote/docker/Dockerfile        — [create] NativeLink + Debian + python3
examples/remote/docker/nativelink.json5  — [create] CAS, action cache, scheduler and one local worker
examples/remote/compose.yaml             — [create] one service from docker/, port 50051 on 127.0.0.1
examples/remote/{buildl.toml,build.lua,app/…,tests/…,assert.sh,README.md} — [create] the workspace and its four-copy check
examples/README.md                       — [modify] a row for the example
.github/workflows/ci.yml                 — [modify] `examples-remote` job
```

## Order of work

### Task 1 — The server image, proven on the CI runner

**Files:** Create `examples/remote/docker/*`, `examples/remote/compose.yaml`

**Change:** Dockerfile from `ci/nativelink.Dockerfile` (same pinned NativeLink image digest and
Debian base) plus `apt-get install python3`. Config from `ci/nativelink.json5`, keeping only what
buildl sends: the scheduler's and worker's `OSFamily` (`linux`) and `ISA` (`x86-64`) platform
properties, and the default instance. `compose.yaml` publishes `127.0.0.1:50051` only, since the
server asks for no login and runs what it is sent. First step, on a Linux x86_64 machine — the
CI runner, since the worker matches the client's OSFamily and ISA and no other machine was at
hand: start it, wait for `Worker registered with scheduler`, run copy A
(`buildl build --remote-execute`, expect `remote`) and a fresh copy B (`buildl build`, expect
`fetched`) — spec §8 risks 2 and 3. Their output goes in this plan's `## Probe results`. If B
misses, this plan amends the four-copy sequence on that evidence before Task 2 (spec §8).

**Proof:** `## Probe results` holds A's and B's summary lines.

**Done when:** A reports `remote` and B `fetched`. Commit `feat(repo): NativeLink server for the remote example`.

### Task 2 — The example workspace and its check

**Files:** Create `examples/remote/{buildl.toml,build.lua,app/…,tests/…,assert.sh,README.md}`;
modify `examples/README.md` (a row)

**Change:**
- Sources and `build.lua` as the Python starter's, in the packing form plan 03 Task 3 settled on;
  `buildl.toml` adds `[remote]` with the local url, `proc.run = ["python3"]`.
- `assert.sh`: in four fresh temporary copies, each carrying a line no earlier run wrote (so it
  holds against a server that served it before), every build with `--remote-strict` — A
  `buildl build --remote-execute`, asserts `1 remote`, then `buildl test --remote-execute`,
  asserts `tests: 1 passed (1 remote)` (the unittest action on the worker, spec §8 risk 2); B
  `buildl build`, asserts `1 fetched`; C appends a line to `app/greet.py`,
  `buildl build --remote-upload`, asserts it ran locally (`1 built`); D, with the same edit,
  `buildl build`, asserts `1 fetched`. Each built `out/app.pyz` prints its greeting.
- `README.md`: `docker compose up -d --build`, then `sh assert.sh`, or the commands of copies A
  and B by hand against a server just started; Linux x86_64 only, because NativeLink's image is
  x86_64 and its worker matches the client's OSFamily and ISA (the `remote-execution` job's
  comment in `.github/workflows/ci.yml`); nothing is claimed for other machines.

**Proof:** `assert.sh` passes against the Task 1 server.

**Done when:** it passes. Commit `feat(repo): remote cache and execution example`.

### Task 3 — Run it in CI

**Files:** Modify `.github/workflows/ci.yml`

**Change:** job `examples-remote` (`name: Remote example on NativeLink`), `ubuntu-latest`:
checkout; Rust toolchain and cache as the `remote-execution` job; `actions/setup-python@v5` with
`python-version: "3.12"`; `cargo build -p buildl-cli`; `docker compose -f
examples/remote/compose.yaml up -d --build`; wait for `Worker registered with scheduler` with the
60-second loop of the `remote-execution` job's "Wait for NativeLink's worker" step;
`sh examples/remote/assert.sh` with `BUILDL=target/debug/buildl`; on failure print the server's
log with `docker compose … logs nativelink`.

**Proof:** the job is green.

**Done when:** green. Commit `ci(repo): run the remote example against NativeLink`.

## Risks

- **Most risky: Task 1.** Whether the worker runs the Python commands with only `python3` added,
  and whether NativeLink keeps what it executed so B is `fetched`. Settled on the CI runner (see
  Probe results).
- **The tool fingerprint.** The local `python3`'s fingerprint is in the key while the worker runs
  its own `python3`; A, B, C and D run on the same runner, so their keys agree.
- **Chose not to:** run `remote` on macOS or in the `examples` matrix; share the image with `ci/`.

## Proof

- `examples-remote` green in CI; `cargo make dod` and `cargo deny check` green.

## Review findings

- security — `compose.yaml` published `50051` on every interface, so anyone on the network could
  send commands to a server that asks for no login; now `127.0.0.1:50051:50051`, and the README
  says why. Fixed.
- spec gap — `## Probe results` was not filled; filled below from the CI run.
- spec gap — no `## Deviations`; added below.
- spec gap — spec §8 risk 2 names the unittest action on the worker, which only `build` had
  exercised; `assert.sh` now runs `buildl test --remote-execute` in copy A. Fixed.
- nits — `examples/README.md` said `cargo make examples` checks every example and that `remote`
  is "not run by check.sh"; both now say `cargo make examples` leaves `remote` out and a CI job
  runs it. The README's by-hand `1 remote` holds only against a server just started; it says so.
  This plan cited `ci.yml` line numbers that have moved and four by-hand commands where the
  README gives the A and B commands; it now names jobs and steps, and the commands it gives.
  Fixed.
- Left as is — spec §2 and §5 cite `ci.yml` line numbers that moved with plan 03's `examples`
  job (`378ef1a`); the spec is the approved authority, the citations named the right lines when
  written, and this plan now cites job and step names.

## Probe results

- CI run 40632551313 (at `7431351`), job `Remote example on NativeLink`, success:
  `remote: a: the server runs the target` (asserted `1 remote`),
  `remote: b: a fresh copy fetches what the server ran` (asserted `1 fetched`),
  `remote: c: an edited copy builds here and uploads` (asserted `1 built`),
  `remote: d: a fresh copy with the same edit fetches what was uploaded` (asserted `1 fetched`),
  `remote: ok`. B fetched, so the four-copy sequence stands as written.
- CI run 40633614431 (at `b1cc0f0`, after the review fixes), same job, success, with
  `remote: a: the server runs the test` (asserted `tests: 1 passed (1 remote)`) between A and B.

## Deviations

- 2026-10-28 — Task 1's probe ran on the CI runner, together with Tasks 2 and 3: this machine is
  macOS arm64, for which the worker matches no client.
- 2026-10-28 — the NativeLink config keeps only the `OSFamily` and `ISA` platform properties and
  the default instance, which is what buildl sends (`crates/buildl/src/remote/encode.rs`).
- 2026-10-28 — each copy carries a per-run line, so `assert.sh` can run against a server that
  served it before.
- 2026-10-28 — the CI job starts the server with `docker compose`, the way the README does.
- 2026-10-28 — copy A also runs `buildl test --remote-execute` (review finding).
