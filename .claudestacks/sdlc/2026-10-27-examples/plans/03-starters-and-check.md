---
status: done
created: 2026-10-27
built-on: 5947af3e315f
---

# Starter examples and their CI check — Implementation Plan

**Goal:** the Go, Rust and Python starter examples are proven on Linux and macOS by one shared check that CI runs.

**Architecture:** a top-level `examples/` directory, outside every crate (publish rules,
architecture-building-blocks §9). `examples/check.sh <dir>` runs the common steps (spec §5) then
the example's own `assert.sh`. A `cargo make examples` task runs it for every example directory
holding an `assert.sh`, except `remote`. A new `examples` CI job runs that task on the `dod`
matrix with Go and Python installed.

**Tech Stack:** POSIX sh, the `buildl` binary, Go 1.22, cargo, Python 3.12 standard library,
cargo-make, GitHub Actions (`actions/setup-go@v5`, `actions/setup-python@v5`).

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §1, §2, §5, §7. Gates for every task:
`cargo make dod` and `cargo deny check` green, plus `cargo make examples`; Conventional Commit,
one per task.

---

## Files that change

```
examples/check.sh                 — [create] the common check: check, build+test, plan --json all Clean, verify, then assert.sh
examples/README.md                — [create] what each example shows and how to run one
examples/go/{buildl.toml,build.lua,go.mod,main.go,main_test.go,assert.sh,README.md}       — [create] Go starter
examples/rust/{buildl.toml,build.lua,Cargo.toml,Cargo.lock,src/main.rs,assert.sh,README.md} — [create] Rust starter
examples/python/{buildl.toml,build.lua,app/…,tests/…,assert.sh,README.md}                  — [create] Python starter
Makefile.toml                     — [modify] `examples` task (shell-script task shaped like guard-crate-edges, :126-152)
.gitignore                        — [modify] ignore examples/*/out and examples/*/.buildl if not already covered
.github/workflows/ci.yml          — [modify] `examples` job
docs/design.md                    — [modify] §9.3 Rust example
```

## Order of work

### Task 1 — The Go starter and the shared check

**Files:** Create `examples/check.sh`, `examples/README.md`, `examples/go/*`; modify
`Makefile.toml`, `.gitignore`

**Change:**
- `examples/go`: a `main` package printing one line, a passing `go test`, `buildl.toml` with
  `[ceiling]` `proc.run = ["go"]`, `env.read = ["HOME"]`, `fs.read = ["."]`,
  `fs.write = ["out", ".buildl"]`; `build.lua` declaring the binary
  (`go build -o $out .`, `env = { "HOME" }`) and a `buildl.test` running `go test ./...`
  (spec §1: without `HOME`, `GOCACHE is not defined and $HOME is not defined`).
- `examples/check.sh <dir>`: runs from `<dir>` with the `buildl` on `PATH` (or `$BUILDL`):
  `buildl check`; `buildl build`; `buildl test`; `buildl plan --json` and fails unless every
  target's `verdict` is `Clean` (python3 reads the JSON — the shape spec §1 probed); `buildl verify`
  and fails unless it exits 0 and prints `0 mismatched`; then `sh assert.sh`. Each step prints
  its name; the first failure stops with the step and output. It works on a fresh copy of the
  example made in a temporary directory, so the checkout stays clean.
- `examples/go/assert.sh`: `out/<binary>` prints the expected line; append a comment to
  `main.go` and `buildl plan` names `input main.go changed`; `buildl build`; then `buildl test`
  twice — the first runs the test (its inputs include `main.go`), the second reports `cached`;
  `buildl log` of the test target exits 0 and prints `ok`.
- `examples/README.md` says `check.sh` itself needs `python3` (it reads `plan --json` with it),
  beside each example's own toolchain.
- `cargo make examples` (`Makefile.toml`, shell-script task like `guard-crate-edges`):
  `cargo build -p buildl-cli`, exports `BUILDL` as the absolute path of the built binary, runs `check.sh` for every
  `examples/*/` that holds an `assert.sh`, except `examples/remote`.
- First step of the task, before writing the example around them: run the Go commands through
  `buildl` in a scratch copy (spec §8 risk 1).

**Proof:** `cargo make examples` passes on this machine; it fails when `assert.sh`'s expected line
is changed (seen once, then restored).

**Done when:** `cargo make examples` and `cargo make dod` pass. Commit
`feat(repo): Go starter example and the shared example check`.

### Task 2 — The Rust starter

**Files:** Create `examples/rust/*`

**Change:**
- A binary crate printing one line with a passing `#[test]`. Its `Cargo.toml` holds an empty
  `[workspace]` table so cargo does not treat it as part of the repository's root workspace
  (the root `Cargo.toml` lists its members and would otherwise claim the package when cargo runs
  in the checkout). `Cargo.lock` is committed and declared as an input.
- `build.lua`: the binary via `cargo install --path . --root $out --target-dir tt --no-track`
  with `env = { "HOME", "PATH" }`, giving `out/<target>/bin/<name>`; a `buildl.test` running
  `cargo test --target-dir tt`. `README.md` says why this form, citing spec §1: a directory output
  holds regular files only, and `outputs = { "target/release/…" }` is not produced under `out/`.
- `buildl.toml`: `proc.run = ["cargo"]`, `env.read = ["HOME", "PATH"]`, `fs.read`, `fs.write` as
  in Task 1.
- `assert.sh`: `out/<target>/bin/<name>` prints the expected line; the edit/`plan`/`test`-twice
  checks of Task 1 on `src/main.rs`; `buildl log` of the test target prints `test result: ok`.

**Proof:** `cargo make examples` passes with the Rust starter included; `cargo make dod` still
passes (the root workspace does not pick the example up).

**Done when:** both pass. Commit `feat(repo): Rust starter example`.

### Task 3 — The Python starter

**Files:** Create `examples/python/*`

**Change:**
- `app/__main__.py` and `app/greet.py`; `tests/test_greet.py` with `unittest`, standard library
  only. `build.lua`: `python3 -m zipapp app -o $out` (the form spec §1 probed) with
  `inputs = { buildl.sources("app/*.py") }`; a `buildl.test` running
  `python3 -m unittest discover -s tests` with app and test sources as inputs. `buildl.toml`:
  `proc.run = ["python3"]`.
- `assert.sh`: `python3 out/<name>.pyz` prints the expected line; the edit/`plan`/`test`-twice
  checks on `app/greet.py`; `buildl log` of the test target prints `OK`.
- `zipapp` writes each file's modification time into the archive, and the sandbox's input copy
  (`fs::copy`, `crates/buildl/src/exec/sandbox.rs:241`) is not known to keep that time on Linux
  (spec §1's `verify` ran on macOS only). If Task 4's first Linux run shows `verify` mismatched
  for this target, the example packs with a short standard-library script instead, writing every
  entry with the fixed time 1980-01-01 00:00 (the sandbox's `SOURCE_DATE_EPOCH`, design §12.3),
  and its `README.md` says why; the fix lands in this plan, and plans 05 and 07 use the same form.

**Proof:** `cargo make examples` passes with all three starters.

**Done when:** it passes. Commit `feat(repo): Python starter example`.

### Task 4 — Run the examples in CI

**Files:** Modify `.github/workflows/ci.yml`

**Change:** a job `examples` (`name: Examples`) on the `dod` matrix
(`os: [ubuntu-latest, macos-latest]`, `ci.yml:35-36`): checkout; the Rust toolchain step and
cargo cache as the `dod` job does (`ci.yml:46-58`); cargo-make install (`:59-65`);
`actions/setup-go@v5` with `go-version-file: examples/go/go.mod`; `actions/setup-python@v5` with
`python-version: "3.12"`; then `cargo make examples`.

**Proof:** the job is green on a pushed branch on both operating systems (checked on the PR).

**Done when:** CI `examples` is green on both. Commit `ci(repo): run the examples on Linux and macOS`.

### Task 5 — Correct design §9.3

**Files:** Modify `docs/design.md`

**Change:** §9.3 (`:583-594`) shows the Rust target in the form the starter uses,
`cargo install --path . --root $out --no-track`, and says why: `outputs = { "target/release/mytool" }`
is never produced, because an output is placed under `out/` while cargo writes under `target/`
(spec §1). The paragraph about cargo's own cache is corrected too: in the input sandbox cargo's
`--target-dir` lies in the exec dir, which is removed after each run (design §12.2), so cargo's
cache lasts across runs only under `strategy = "bare"`; §12's `rust` plugin line says the same.

**Proof:** `cargo make dod` green.

**Done when:** committed `docs(repo): correct the Rust example in design §9.3`.

## Risks

- **Most risky: Task 4.** Toolchain differences between the runners and this machine (Go and
  Python versions, the macOS linker). `setup-go`/`setup-python` pin the versions; the first CI
  run is the probe, and a failure is fixed in the example or the job inside this plan.
- **Cargo and the root workspace.** Without the example's own `[workspace]` table, cargo run in
  `examples/rust` reports that the package believes it is in a workspace it is not a member of.
  Task 2 adds the table; `cargo make dod` proves the root workspace is unaffected.
- **`zipapp` and file times on Linux** — see Task 3; the first `ubuntu-latest` run of Task 4 is
  the probe.
- **Parallel cargo in the sandbox** waits on `~/.cargo`'s package lock (`Blocking waiting for file
  lock on package cache`, seen in the probes); harmless, only slower.
- **Chose not to:** use pytest or a venv; pin Go or Python inside `buildl.toml`.

## Proof

- `cargo make examples` passes locally and in CI on `ubuntu-latest` and `macos-latest`.
- `cargo make dod` and `cargo deny check` stay green.

## Review findings

- doc truth — design §9.3 said cargo's `--target-dir` stays cargo's cache; in the input sandbox `target` lies in the exec dir, removed after each run (design §12.2). §9.3 and §12's `rust` plugin line now say the cache lasts across runs only under `strategy = "bare"`; Task 5's Change amended. Fixed at the Task 5 commit.
- comment-vs-code — `check.sh` copied an example's `out/` and `.buildl/` left by building in place, so the copy was not fresh; the copy now drops both. Fixed; `check.sh` passed for go, rust and python with a relative `BUILDL`.
- `check.sh` nits — the copy's temp directory now goes on INT/TERM too; a relative `BUILDL` is made absolute before the `cd`. Fixed.
- README nits — the Go error text named as macOS's wording; the linker error quoted exactly (``error: linker `cc` not found``); Go version stated as 1.22 throughout; "CI runs it" moved to the Task 4 commit. Fixed.
- `Makefile.toml` nits — the comment no longer claims a `remote` CI job; the binary path follows `CARGO_TARGET_DIR`. Fixed.
- plan drift — Task 1 said the task puts `target/debug` on `PATH`; it exports `BUILDL` as the binary's absolute path. Plan amended.
- Left as is — the empty `[workspace]` table in `examples/rust/Cargo.toml` has no test that fails without it: `check.sh` copies outside the repository. The table stays for a reader running cargo in the checkout.

## Probe results

- Go, Rust and Python commands through `buildl` in scratch copies before each example was written: each built, tested, reported `Clean` in `plan --json` and `1 verified, 0 mismatched` in `verify`; after an edit `plan` printed `input <file> changed`, the next `test` ran, the one after printed `tests: 1 passed (1 cached)`, and `log //:unit` printed `ok` / `test result: ok` / `OK`. Not against the plan.
- `cargo make examples` failed with `assert: out/hello prints its greeting` when the Go expected line was changed, then passed once restored.
- Task 4: CI run 40630488346 (workflow_dispatch on `worktree-prep-publish`, the branch pushed with no PR, by the author's choice): `Examples (ubuntu-latest)` and `Examples (macos-latest)` success, every other job success. Task 3's `zipapp` file-time risk did not occur on Linux: `verify` reported no mismatch, so the fixed-time packer was not needed.

## Deviations

- 2026-10-28 — Task 4 proved by a hand-started CI run on the pushed branch, not on a PR, as the author chose.
- 2026-10-28 — `cargo make examples` exports `BUILDL` instead of putting `target/debug` on `PATH`; Task 1 amended.
- 2026-10-28 — Task 5 also corrects design §9.3's cargo-cache sentence and §12's `rust` plugin line (review finding); Task 5 amended.
- 2026-10-28 — `examples/README.md` gained each starter's row in that starter's commit, and the "CI runs it" line in Task 4's.
