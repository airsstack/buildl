---
status: approved
created: 2026-10-27
depends-on: [03]
built-on: 5947af3e315f
---

# Remote cache and execution example — Implementation Plan

**Goal:** an example shows a Python project executed on, then fetched from, a NativeLink server it starts itself, proven by a Linux CI job.

**Architecture:** `examples/remote/` is the Python starter's sources (plan 03) plus a `[remote]`
table (`url = "grpc://127.0.0.1:50051"`) and a `docker/` directory: a Dockerfile putting
NativeLink's static binary on a Debian base with `python3` installed (the shape of
`ci/nativelink.Dockerfile`), a NativeLink config with a local worker (from `ci/nativelink.json5`),
and a `compose.yaml` starting it. Its `assert.sh` runs the four-copy sequence of spec §5; a CI job
`examples-remote` on `ubuntu-latest` runs it (not `check.sh`, which leaves `remote` out). `ci/`
stays as it is for buildl's own `real_remote` tests.

**Tech Stack:** Docker, NativeLink v1.4.0 (as pinned in `ci/nativelink.Dockerfile`), Python 3.12,
the `buildl` binary, GitHub Actions.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §2, §5, §8. Gates for every task:
`cargo make dod` and `cargo deny check` green; Conventional Commit, one per task.

---

## Files that change

```
examples/remote/docker/Dockerfile        — [create] NativeLink + Debian + python3
examples/remote/docker/nativelink.json5  — [create] CAS, action cache, scheduler and one local worker
examples/remote/compose.yaml             — [create] one service from docker/, port 50051
examples/remote/{buildl.toml,build.lua,app/…,tests/…,assert.sh,README.md} — [create] the workspace and its four-copy check
.github/workflows/ci.yml                 — [modify] `examples-remote` job
```

## Order of work

### Task 1 — The server image, proven by hand

**Files:** Create `examples/remote/docker/*`, `examples/remote/compose.yaml`

**Change:** Dockerfile from `ci/nativelink.Dockerfile` (same pinned NativeLink image digest and
Debian base) plus `apt-get install python3`; config from `ci/nativelink.json5`. First step, on a
Linux x86_64 machine or the CI runner: start it, wait for `Worker registered with scheduler`,
then, in a scratch copy of `examples/python` (plan 03) with the `[remote]` table added, run copy A
(`buildl build --remote-execute`, expect `remote`) and a fresh copy B (`buildl build`, expect
`fetched`) — spec §8 risks 2 and 3. Their output goes in this plan's `## Probe results`. If B
misses, this plan amends the four-copy sequence on that evidence before Task 2 (spec §8).

**Proof:** `## Probe results` holds A's and B's summary lines.

**Done when:** A reports `remote` and B `fetched`. Commit `feat(repo): NativeLink server for the remote example`.

### Task 2 — The example workspace and its check

**Files:** Create `examples/remote/{buildl.toml,build.lua,app/…,tests/…,assert.sh,README.md}`

**Change:**
- Sources and `build.lua` as the Python starter's, in the packing form plan 03 Task 3 settled on;
  `buildl.toml` adds `[remote]` with the local url, `proc.run = ["python3"]`.
- `assert.sh`: in four fresh temporary copies — A `buildl build --remote-execute`, asserts the
  summary counts `remote`; B `buildl build`, asserts `fetched`; C appends a line to
  `app/greet.py`, `buildl build --remote-upload`, asserts it ran locally (`built`); D, with the
  same edit, `buildl build`, asserts `fetched`.
- `README.md`: `docker compose up`, then `sh assert.sh` or the four commands by hand; Linux
  x86_64 only, because NativeLink's image is x86_64 and its worker matches the client's OSFamily
  and ISA (`.github/workflows/ci.yml:178-182`); nothing is claimed for other machines.

**Proof:** `assert.sh` passes against the Task 1 server.

**Done when:** it passes. Commit `feat(repo): remote cache and execution example`.

### Task 3 — Run it in CI

**Files:** Modify `.github/workflows/ci.yml`

**Change:** job `examples-remote` (`name: Remote example on NativeLink`), `ubuntu-latest`:
checkout; Rust toolchain and cache as the `remote-execution` job (`ci.yml:186-191`);
`actions/setup-python@v5` with `python-version: "3.12"`; build `buildl`; start the example's
server; wait for `Worker registered with scheduler` with the 60-second loop of `ci.yml:202-210`;
`sh examples/remote/assert.sh`; on failure print the server's log (as `ci.yml:223-225`).

**Proof:** the job is green on the PR.

**Done when:** green. Commit `ci(repo): run the remote example against NativeLink`.

## Probe results

(filled by Task 1)

## Risks

- **Most risky: Task 1.** Whether the worker runs the Python commands with only `python3` added,
  and whether NativeLink keeps what it executed so B is `fetched`. Both are settled by hand
  before anything is built around them.
- **The tool fingerprint.** The local `python3`'s fingerprint is in the key while the worker runs
  its own `python3`; A, B, C and D run on the same runner, so their keys agree.
- **Chose not to:** run `remote` on macOS or in the `examples` matrix; share the image with `ci/`.

## Proof

- `examples-remote` green in CI; `cargo make dod` and `cargo deny check` green.
