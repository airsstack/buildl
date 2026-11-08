---
status: approved
created: 2026-11-07
depends-on: [05]
built-on: f3bc0d9fa2e7
---

# Cross Machine Proof Implementation Plan

**Goal:** A test of the binary and a CI step prove that two machines share keys and the remote cache.

**Architecture:** Two end-to-end checks, beside the core and remote tests of plan 01. A
`buildl-cli` test runs one workspace from two folders with two `TMPDIR`, `HOME` and `PATH` values
and compares `buildl plan --json` keys. `examples/remote` marks `HOME` and `PATH` unkeyed and its
`assert.sh` gains a step, run by the CI job "Remote example on NativeLink", in which a copy built
from another folder, `TMPDIR`, `HOME` and `PATH` fetches what the first uploaded (spec §9).

**Tech Stack:** Rust 1.94 (`crates/buildl-cli/tests`), POSIX `sh`, GitHub Actions, NativeLink in
docker compose (`examples/remote/compose.yaml`).

**Content authority:** `spec.md` §9 (binary and CI rows).

---

## Files that change

```
crates/buildl-cli/tests/cli/machines.rs    — create: the two-folder, two-environment key test
crates/buildl-cli/tests/cli/main.rs        — modify: declare the module
examples/remote/build.lua                  — modify: env = { HOME = "unkeyed", PATH = "unkeyed" }
examples/remote/buildl.toml                — modify: env.read and env.unkeyed = ["HOME", "PATH"]
examples/remote/assert.sh                  — modify: step e, another machine fetches
examples/remote/README.md                  — modify: the copies it lists (:45, "the four copies A, B, C, D") gain E
```

## Order of work

### Task 1 — Compare keys from two folders and two environments

**Files:** Create `crates/buildl-cli/tests/cli/machines.rs`; modify `crates/buildl-cli/tests/cli/main.rs`.

**Change:** the test writes one workspace of independent targets (no target depends on another, so
`plan` gives each a finished key, not `PlannedKey::Unfinished`,
`crates/buildl-core/src/types/plan.rs:102`), each marking `HOME` and `PATH` `unkeyed`; copies it to
a second folder; runs `buildl plan --json` in each with its own `TMPDIR`, `HOME`, and a `PATH` with
one more folder in front; and compares each target's key.

**Proof:** written first, seen failing, then passing —
- `two_machines_plan_the_same_keys` — every target's key is the same in both (red while the
  targets mark `PATH` `key`: the keys differ or the run is refused; green with `unkeyed`).
- `a_key_variable_still_differs` — with `RUSTFLAGS` marked `key` and two values, the keys differ.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `test(buildl-cli): plan the same keys on two machines`.

### Task 2 — Fetch from another machine in the remote example

**Files:** Modify `examples/remote/build.lua`, `buildl.toml`, `assert.sh`, `README.md:45`.

**Change:** the example's target marks `HOME` and `PATH` `unkeyed`, and its `buildl.toml` allows
both in `env.read` and `env.unkeyed`. `assert.sh` gains step `e` after `d`: build with
`--remote-upload` in one fresh copy, then build another fresh copy in another folder with another
`TMPDIR`, another `HOME` (a new empty folder) and `PATH` with one more folder in front; it must print
`1 fetched`. The README's list of copies gains E. The job "Remote example on NativeLink"
(`.github/workflows/ci.yml`, job `examples-remote`) runs `assert.sh` unchanged.

**Proof:** written first, seen failing, then passing — with the target's `PATH` marked `key`, step
`e` fails (the copy is refused for holding this machine's folder, or builds instead of fetching);
with `unkeyed` it prints `1 fetched`. Both are seen in the CI job, since it needs NativeLink in
docker.

**Done when:** commit `test(repo): fetch from another machine in the remote example`, push the
branch, and the CI job "Remote example on NativeLink" passes on that push; `cargo make dod` and
`cargo deny check` pass before the commit.

---

## Risks

- Task 2 runs only in CI (OrbStack is not used for it); its red and green are seen on pushes of the
  branch, the red one on a push before the marks change to `unkeyed`.
- Not done here: the book (plan 07).

## Proof

- `cargo test -p buildl-cli --test cli machines` passes.
- CI job "Remote example on NativeLink" passes with step `e`.
