---
status: executing
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

## Review findings

One reviewer pass over Tasks 1-2 (`08-reviewer-p06`, blocking: none), one fix round (`12-coder-p06-fix`: `cargo test -p buildl-cli --test cli machines` 2 passed, `sh -n examples/remote/assert.sh` ok, `cargo make dod` green):

- 🔵 `machines.rs` asserted `first.len() == TARGETS.len()`, which cannot fail (`keys()` panics on a missing target) — fixed: removed; the per-target guard stays.
- 🔵 `machines.rs` reads each digest by a substring scan of `plan --json` — accepted: buildl prints JSON through one canonical sorted-key serializer, so the layout is fixed; the comment now says so.
- 🔵 `assert.sh` row E's expected-summary column was off by 3; a comment read "keeps the changes from" — fixed.
- 🔵 `build.lua` and the README said HOME and PATH reach the action; under remote execution unkeyed variables are not sent (`remote_action.rs:50-56`) — fixed: both say the worker uses its own environment.
- 🔵 the README diagram's E lines had no arrow to the server — fixed.
- Task 1 red/green: with `PATH` marked `key`, `two_machines_plan_the_same_keys` failed with differing keys; with `unkeyed` both tests pass. `cargo make dod` and `cargo deny check` green.

## Deviations

- 2026-11-08 — `.github/workflows/ci.yml` changed (not listed): the job's step "The four copies" is now "The copies A to E".
- 2026-11-08 — the test writes the workspace into each folder rather than copying it; the content is the same.
- 2026-11-08 — Task 2's red CI run was not made (author's choice): the red side is shown by Task 1's red run (keys differ with `PATH` marked `key`); one green CI run on the pushed branch proves step `e`.
