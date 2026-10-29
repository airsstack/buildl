---
status: done
created: 2026-10-29
built-on: 66bd139caa00
---

# Skills Checks Implementation Plan

**Goal:** `cargo make skills`, run locally and by a new CI job, fails whenever a skill under
`skills/` breaks the Agent Skills format or holds a `lua` block that `buildl check` rejects.

**Architecture:** The check is a `cargo make` task, `skills`, in a new *Skills* section of
`Makefile.toml`, written as an `@shell` script the way the `examples` task is (Makefile.toml:363-381).
It builds the `buildl` binary, runs the pinned `skills-ref` over every skill folder, stages the
Lua tests beside the libraries they load and runs them with `airsl test`, and runs `buildl check`
on every `lua` block the skills hold, extracted by a small Lua library. A CI job `skills` runs it
(spec §6.1–6.3). The later plans add their own steps to the same task (plan 05: the installer).

**Tech Stack:** cargo-make `@shell` tasks, Lua 5.4 on `airsl` (`airsl run`, `airsl test`), `skills-ref`
(Python ≥ 3.11), GitHub Actions.

---

## Files that change

```
skills/tests/snippets_lib.lua          — [create] pure: find the checkable lua blocks in a Markdown text
skills/tests/snippets.lua              — [create] entry: write one workspace per block under a folder W
skills/tests/snippets_test.lua         — [create] tests of snippets_lib, run by airsl test
skills/tests/fixtures/snippets/sample.md — [create] a Markdown fixture holding each kind of block
Makefile.toml                          — [modify] new "Skills" section with the `skills` task
.github/workflows/ci.yml               — [modify] new `skills` job
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| `buildl check` validates declarations only: a missing input and a program outside the ceiling both pass | spec §1 P13 (probe: `ok: 1 declaration in 1 build file`, exit 0) |
| `buildl check` rejects an unknown option key with exit 5 | spec §1 P12 |
| `airsl test` runs every `*_test.lua` under a folder; a test file returns a table of functions, a test fails by raising | spec §1 P22; airsl book ch05-05-testing.md:47-50 |
| under `confined`, `require` finds only the script's own folder and below; a sibling `require` needs no extra grant | spec §1 P27 |
| `airsl run` passes the arguments after the script as `arg[1]`, `arg[2]`, … | probe in the design (`arg1=first`); airsl book appendix-01-cli.md:16 |
| `skills-ref` installs with `pip install "git+https://github.com/agentskills/agentskills@69ef37e9424c0a7ea9dd2293b559e43ec8176379#subdirectory=skills-ref"`; `skills-ref validate <folder>` exits 0 for a valid skill, 1 otherwise | spec §1 P6 |
| the `examples` task builds `buildl` with `cargo build -p buildl-cli` and uses `${CARGO_TARGET_DIR:-$(pwd)/target}/debug/buildl` | Makefile.toml:370-372 |
| the `examples` CI job's steps are checkout, Rust toolchain, `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2` with `cargo-make`, then `cargo make examples` | .github/workflows/ci.yml:69-109 |

## Where tests find their fixtures

Every test reads its fixtures by a path relative to the working directory, `fixtures/…`. Both runs
of the tests start `airsl test` from the folder that holds the test files: by hand, `cd
skills/tests && airsl test --allow-read . .`; in the task, `cd "$T" && airsl test --allow-read . .`
over the staging folder, whose root holds the *contents* of `skills/tests/` (so `T/fixtures/…`,
`T/snippets_test.lua`) and the libraries beside them.

## Order of work

### Task 1 — Find the checkable `lua` blocks in Markdown

**Files:**
- Create `skills/tests/snippets_lib.lua` — one function, `blocks(text, name)`, returning the list of
  checkable blocks of a Markdown text, each with its body, its 1-based index among checkable
  blocks, and the line its fence opens on
- Create `skills/tests/fixtures/snippets/sample.md` — a fixture holding a `lua` block, a
  `lua no-check` block, a `toml` block, a `text` block, and a second `lua` block; both `lua` blocks
  are valid build files, so the fixture passes `buildl check` if it is ever checked
- Test `skills/tests/snippets_test.lua`

**Change:** a block is checkable when its opening fence's info string is exactly `lua` (spec §6.2).
A fence whose info string is `lua no-check`, or any other language, is not returned. A fence that
is never closed raises an error naming `name` and the line it opened on.

**Proof:** written first, seen failing, then passing —
- `a_lua_block_is_returned` — over the fixture, the first returned block's body is the fixture's first
  `lua` block, with index 1 and its opening line
- `a_no_check_block_is_skipped` — the `lua no-check` body is in no returned block
- `other_languages_are_skipped` — the `toml` and `text` bodies are in no returned block
- `blocks_keep_document_order` — exactly two blocks are returned, the second with index 2
- `an_unclosed_fence_is_an_error` — a text whose `lua` fence never closes raises, and the message holds
  the name and the line number

**Done when:** `cd skills/tests && airsl test --allow-read . .` passes (the tests and the library
are siblings, so no staging is needed for this file), and `cargo make dod` and `cargo deny check`
are green. Commit `test(repo): find the checkable lua blocks in skill Markdown`.

### Task 2 — Write one workspace per block

**Files:**
- Create `skills/tests/snippets.lua` — entry script: `airsl run --allow-read skills --allow-write W
  skills/tests/snippets.lua W`; walks `skills/` for `*.md`, leaving out `skills/tests/fixtures/`
  (test inputs, not skill content), and for every block `snippets_lib.blocks`
  returns writes `W/<md path with / as ->-<index>/build.lua` holding the body and a `buildl.toml`
  beside it, then prints one line per folder naming the source file, the index and the opening line

**Change:** the `buildl.toml` is the same for every block: a `[workspace]` with `name`, `entry =
"build.lua"`, `out = "out"`, `config = 1`, and a `[ceiling]` with `proc.run`, `env.read`, `fs.read
= ["."]` and `fs.write = ["out", ".buildl"]` — the shape `examples/python/buildl.toml` has. Since
`check` does not apply the ceiling (P13), the ceiling's program list may stay empty.

**Proof:**
- by hand, once: running the command over a temporary `skills/` copy holding a copy of
  `sample.md` at `probe/sample.md` writes exactly two folders, and `buildl check` in the first one
  exits 0; the same copy left at `tests/fixtures/snippets/sample.md` writes none. The run is a
  probe; it leaves no file in the repository.

**Done when:** that probe holds, and `cargo make dod` and `cargo deny check` are green. Commit
`feat(repo): write a buildl workspace for each lua block in the skills`.

### Task 3 — The `skills` task

**Files:**
- Modify `Makefile.toml` — add a section headed `# Skills`, after the `Examples` section and before
  `Book`, holding the task `skills` (category `Skills`, `script_runner = "@shell"`), with a comment
  saying why it is not a step of `dod`: it needs `airsl`, Python's `skills-ref`, and later
  `shellcheck`, beside the Rust toolchain (spec §6.1)

**Change:** the task's script, with `set -e`, in this order:
1. `cargo build -p buildl-cli`, and `BUILDL` set as the `examples` task sets it.
2. Each of `airsl` and `skills-ref` missing from `PATH` stops the task with a line naming it and how
   to install it (`cargo install --locked airsl-cli`; the pinned `pip install` line above).
3. For every `skills/*/` holding a `SKILL.md`: `skills-ref validate <folder>`.
4. Stage: a fresh temporary folder `T` receives the contents of `skills/tests/` at its root (so
   `T/fixtures/`, `T/*_test.lua`) and every `skills/*/scripts/*.lua` beside them; then
   `cd "$T" && airsl test --allow-read . .` (spec §6.4).
5. A fresh temporary folder `W`; `airsl run --allow-read skills --allow-write "$W"
   skills/tests/snippets.lua "$W"`; then in each folder under `W`, `"$BUILDL" check`; a non-zero
   exit names the folder and fails the task.
6. The temporary folders are removed on exit.

**Proof:**
- by hand, once: `cargo make skills` passes on the tree as it stands (no skill yet: step 3 finds
  nothing, and step 5 finds nothing because the fixture folder is left out; step 4 runs
  `snippets_test.lua`)
- by hand, once: a temporary `skills/probe/SKILL.md` with valid frontmatter and a `lua` block using
  `input = { "a" }` makes the task fail at step 5, naming the folder; the same file with `name`
  unequal to its folder makes it fail at step 3. The probe file is removed afterwards.

**Done when:** both probes hold, and `cargo make dod` and `cargo deny check` are green. Commit
`build(repo): add the cargo make skills task`.

### Task 4 — The `skills` CI job

**Files:**
- Modify `.github/workflows/ci.yml` — add a job `skills`, after `examples`

**Change:** `name: Skills`, `runs-on: ubuntu-latest`, `timeout-minutes: 30`, a comment saying what
it checks. Steps: `actions/checkout@v5`; the Rust toolchain step and `Swatinem/rust-cache@v2`
(`shared-key: skills`) and `taiki-e/install-action@v2` with `cargo-make`, as the `examples` job has
them; `actions/setup-python@v5` with `python-version: "3.12"`; `pip install` of `skills-ref` at the
pinned commit; `cargo install --locked airsl-cli`; then `cargo make skills`.

**Proof:** the job's YAML parses (`python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))'
.github/workflows/ci.yml` with PyYAML, or the CI run of the pull request); the job runs green on the
branch's pull request.

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`ci(repo): run cargo make skills in CI`.

## Risks

- `skills-ref validate` given no argument fails; step 3 loops over folders, so zero skills means
  zero calls. Until plan 02 lands, the task checks only the snippet library's own tests.
- That airsl resolves a test's relative `fixtures/…` path against the working directory is not
  verified; `execute` probes it first, before Task 1's tests are written. If it resolves against
  the test file's folder instead, both runs still work, since the fixtures sit beside the tests in
  each.
- How a `require`d module and an entry script share `arg` under airsl is not verified; splitting
  `snippets_lib.lua` (pure) from `snippets.lua` (entry) avoids depending on it.
- The `skills` job installs `airsl-cli` from crates.io unpinned; a new airsl release could change
  behaviour under the job. Pin it with `@0.1.4` if the first CI run shows drift.
- `cargo build -p buildl-cli` in a fresh worktree compiles the whole dependency tree once (several
  minutes); the CI cache key keeps later runs short.

## Proof

`cargo make skills` passes on the branch, fails on each probe in Task 3, and the `skills` CI job is
green on the pull request.

## Review findings

- correctness — two Markdown files could map to one workspace folder (`probe/b-c.md`, `probe-b/c.md` → `probe-b-c.md-1`), the second silently replacing the first — skills/tests/snippets.lua. Fixed: an existing folder raises. Verified: `airsl run --allow-read skills --allow-read W3 --allow-write W3 skills/tests/snippets.lua W3` over a probe tree holding both files → `skills/probe-b/c.md: block 1 maps to W3/probe-b-c.md-1, which another file already took`, exit 1.
- reversion guard — dropping the `tests/fixtures/` exclusion would pass unnoticed — Makefile.toml `skills`. Fixed: a `tests-fixtures-*` folder in W fails the task. Verified: `cargo make skills` green on HEAD (`Build Done in 1.61 seconds`).
- test strength — `err:find("3")` matched any 3 — skills/tests/snippets_test.lua. Fixed: matches `line 3:`. Verified: `airsl test --allow-read . snippets_test.lua` → `5 passed, 0 failed`.
- correctness — two scripts of one name copied flat into T would replace each other — Makefile.toml `skills`. Fixed: a name already staged fails the task. Verified with the green `cargo make skills` above.
- correctness (nit, declined) — `~~~` fences not recognised; a stray backtick fence inside one fails loudly with "code fence is never closed". No skill uses `~~~`.
- spec §6.3 vs plan (nit, declined) — a skill folder without `SKILL.md` is skipped; the plan's loop was approved as written.

## Probe results

- `airsl test` resolves `fixtures/…` against the working directory — `cd probe1 && airsl test --allow-read . .` with `fs.read("fixtures/a.txt")` → `ok rel`, `3 passed, 0 failed`.
- sibling `require` needs no extra grant; `arg[1]` holds the first argument — `airsl run --allow-read . entry.lua first` → `arg1=first	lib	entry.lua`.
- `fs.exists` on W needs a read grant on W (`--allow-write` grants no reading) — without it, `fs.exists denied: …/buildl-SKILL.md-1 is outside the granted read roots`; the task and the script's usage line now pass `--allow-read "${W}"`.
- Task 2 probe — a `skills/` copy with `probe/sample.md` wrote `W/probe-sample.md-1` and `-2`; the copy under `tests/fixtures/snippets/` wrote none; `buildl check` in the first → `ok: 1 declaration in 1 build file`, exit 0.
- Task 3 probes — a `skills/probe/SKILL.md` with `input = { "a" }` → `error: build.lua: unknown field "input"` and `skills: buildl check failed for probe-SKILL.md-1`, exit 1; `name: probe-x` → `Directory name 'probe' must match skill name 'probe-x'`, exit 1.
- Task 4 — `yaml.safe_load(ci.yml)` lists jobs `dod, examples, skills, deny, book, msrv, reapi-codegen, remote-execution, examples-remote`. The job's green run on the pull request is still to come.

## Deviations

- 2026-10-29 — `snippets.lua` takes `--allow-read W` beside `--allow-write W` (plan text: write grant only), for the collision check.
- 2026-10-29 — `skills-ref` installed locally with `uv tool install` at the pinned commit (CI uses `pip install`, as planned).
- 2026-10-29 — tasks 1–4 done inline by the orchestrator (small Lua and config), one reviewer spawn; review fixes committed as `fix(repo): refuse snippet folder and staged file collisions in cargo make skills`.
