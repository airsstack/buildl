---
status: done
created: 2026-10-15
depends-on: [03]
built-on: 280eda6f4573
---

# `Workspace::log` Implementation Plan

**Goal:** The `buildl` facade reads a target's last run on a real workspace without a lock, a
build file or a write.

**Architecture:** `Wiring::layout()` makes the state directory's layout without taking the lock,
and `Wiring::lock` uses it. `Workspace::log(label)` runs `pipeline::log` (plan `02`) over
`LocalLog` (plan `03`) and `LocalCas`, both made from that layout — neither touches the disk when
made — and over nothing else: no `wiring.ports()`, no source. It sits beside `Workspace::query`,
the other command that takes no lock.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `tempfile` (tests). Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §1 P7, P9, P11, P12, P18, §2 (Target, Reads), §3.5 (second and
third bullets), §6 (row for `crates/buildl/tests/e2e/log.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl`.

**Fixture.** Spec §6 names the `chain` and `failing` fixtures for the e2e tests; neither has an
action that prints (`chain`'s `greet.sh` writes to its output file, `cp` and `sort` print nothing;
`failing`'s `false` prints nothing), so they cannot show a stream. Task 2 adds a small fixture,
`logged`, whose one printing target writes to both streams.

Facts the tasks rest on, each read at commit `0cca515`:

| Fact | Where |
|---|---|
| `Wiring::lock` checks the ceiling, then `StateLock::acquire(StateLayout::new(self.root.as_path(), state))`; `Wiring::root() -> &WorkspaceRoot`; `ports` takes `lock.layout()` | `crates/buildl/src/local_ports.rs:185-200`, `:103`, `:214-230` |
| `local_ports.rs` tests: `a_state_directory_inside_an_allowed_directory_is_locked_and_made` (`:441`), `reading_finds_the_root_above_the_start_and_makes_nothing` (`:295`) | `local_ports.rs:245-600` |
| `StateLayout::new(root, state)`, `dir()`, `log()`, `blobs()` | `crates/buildl/src/state/layout.rs:39`, `:47`, `:77`, `:89` |
| `LocalLog::new(layout)` and `LocalCas::new(root, layout)` touch nothing on disk | `crates/buildl/src/log/local_log.rs:81-89`; `crates/buildl/src/cas/local_cas.rs:72-83` |
| `LocalCas::get` gives `None` for a missing blob and removes one whose bytes fail its digest; a blob is kept at `cas/<2 hex>/<62 hex>` under the state directory | spec P9 (`local_cas.rs:242-257`), P3 |
| `Workspace::query(&self, query)` calls `pipeline::query(&self.wiring.source(), …)`, documented "Takes no lock and opens no state file"; `Workspace::label` reads a relative target against the directory the workspace was opened from | `crates/buildl/src/workspace.rs:107-116`, `:55-70` |
| `workspace.rs` is a composition module: `guard-adapter-isolation` lets it name adapter modules (`local_ports workspace`) | `Makefile.toml`, `[tasks.guard-adapter-isolation]` |
| `workspace.rs` tests: helper `copying()` (`:234`); `source_only_commands_mind_neither_a_held_lock_nor_a_cache_that_is_not_canonical` (`:399`) | `workspace.rs:215-720` |
| e2e harness: `Fixture::copy_of(name, executables)`, `root()`, `state()`, `open()`, `build()`, `append(relative, bytes)`, `replace_line(relative, from, to)`; `StateLock::acquire(fixture.state())` holds a workspace (`facade.rs:45`) | `crates/buildl/tests/e2e/common.rs:56-110`, `:179-200`; `tests/e2e/facade.rs:40-60` |
| e2e modules are declared in `tests/e2e/main.rs`; after the test slice it also declares `test` | `crates/buildl/tests/e2e/main.rs`; `.claudestacks/sdlc/2026-10-15-test/plans/08-workspace-test.md` |
| `pipeline::log(history, cas, label) -> Result<Shown>`; `Shown { last: LastRun, stdout, stderr }`; `Error::NoRun`, `Error::MissingOutput` | `plans/02-pipeline-log.md` |

## Files that change

```
crates/buildl/src/local_ports.rs                 — [modify] Wiring::layout; lock uses it; a test
crates/buildl/src/workspace.rs                   — [modify] Workspace::log; struct doc; a test
crates/buildl/tests/fixtures/logged/buildl.toml  — [create] cp, sh; one worker
crates/buildl/tests/fixtures/logged/build.lua    — [create] hello, quiet, an alias
crates/buildl/tests/fixtures/logged/say.sh       — [create] prints to both streams, copies
crates/buildl/tests/fixtures/logged/hello.txt    — [create] "hello\n"
crates/buildl/tests/e2e/log.rs                   — [create] the e2e tests of reading the log
crates/buildl/tests/e2e/main.rs                  — [modify] declare mod log
```

## Order of work

### Task 1 — `Wiring::layout` and `Workspace::log`

**Files:** `crates/buildl/src/local_ports.rs`, `crates/buildl/src/workspace.rs`.

**Change:**
- `pub fn layout(&self) -> StateLayout` on `Wiring`: `StateLayout::new(self.root.as_path(),
  &self.workspace.state)`, documented "Where the state directory's files are, read without taking
  the directory: for a command that only reads it." `lock` builds its layout with it.
- `pub fn log(&self, label: &Label) -> Result<Shown>` on `Workspace`, beside `query`: with
  `let layout = self.wiring.layout();`, `pipeline::log(&LocalLog::new(&layout),
  &LocalCas::new(self.wiring.root().as_path(), &layout), label)`. Rustdoc: the newest logged run
  of the target with stored output, what it printed read back from the store; "Takes no lock
  and reads no build file, so it runs while a build holds the workspace and whatever the build
  files now say", and it writes nothing but the store's own removal of a blob whose bytes fail
  its digest (spec §2, Reads); an alias is not followed; `# Errors` as `pipeline::log`'s.
  The struct doc names `log` beside `check`, `graph` and `query` as holding nothing.

**Proof:**
- `the_layout_is_made_without_the_lock` in `local_ports.rs`'s tests — on a workspace read with
  `Wiring::read`, `layout().dir()` is the state directory's path and the directory does not
  exist afterwards.
- `log_minds_neither_a_held_lock_nor_build_files` in `workspace.rs`'s tests — `copying()`
  built once; with a `StateLock` held and the build file overwritten with text that is not Lua,
  `log(//:copy)` returns a `Built` run with empty stdout and stderr.

**Done when:** `cargo test -p buildl --lib local_ports::` and
`cargo test -p buildl --lib workspace::` pass and the Definition of Done is green,
`guard-adapter-isolation` included. Commit `feat(buildl): a workspace reads a target's last run`.

### Task 2 — Reading the log of a real workspace

**Files:** the `logged` fixture's files, `tests/e2e/log.rs`, `tests/e2e/main.rs`.

**Change:**
- The fixture `logged`: `buildl.toml` as `failing`'s with `name = "logged"` and
  `proc.run = ["cp", "sh"]`; `say.sh`, run by `sh`, prints `said <its first argument>` and a
  newline on stdout and `warned` and a newline on stderr, and copies its first argument to its
  second with shell built-ins only (as `chain/greet.sh` does); `hello.txt` holds `hello\n`;
  `build.lua`, opening with `local b = buildl` as `chain/build.lua:1` does:
  - `b.target("hello", { inputs = { "say.sh", "hello.txt" }, run = { "sh", "$in", "$out" } })`
  - `b.target("quiet", { deps = { "hello" }, run = { "cp", "$deps", "$out" } })`
  - `b.alias("greeting", "hello")`
- `log.rs`'s module doc says what the fixture holds and why.

**Proof** (in `tests/e2e/log.rs`):
- `log_after_a_build_shows_what_each_target_printed` — after `build()`, `log(//:hello)` is a
  `Built` run with stdout `said hello.txt\n` and stderr `warned\n`; `log(//:quiet)` has both empty.
- `log_after_a_cached_build_shows_the_real_run` — after two builds, `log(//:hello)`'s run is
  `Cached` and its stdout and stderr are the first build's bytes.
- `log_while_a_build_holds_the_lock_still_reads` — built, then `StateLock::acquire(state())`
  held: `log(//:hello)` succeeds.
- `log_with_a_blob_removed_is_missing_output` — built; the file of `log(//:hello)`'s stdout
  digest removed from `.buildl/cas/<2>/<62>`: `log(//:hello)` is `MissingOutput { //:hello }`.
- `log_creates_no_state_directory` — on a fresh copy, `log(//:hello)` is `NoRun`, and `.buildl`
  does not exist afterwards.
- `log_reads_with_a_build_file_broken_since` — built; `build.lua` appended with a line that is
  not Lua: `log(//:hello)` is still the `Built` run.
- `log_shows_a_target_removed_from_the_build_files_since_it_ran` — built; `quiet`'s line
  replaced by a comment: `log(//:quiet)` is still its `Built` run.
- `log_of_an_alias_is_no_run` — built: `log(//:greeting)` is `NoRun` with label `//:greeting`.

**Done when:** `cargo test -p buildl --test e2e log::` passes and the Definition of Done is
green. Commit `test(buildl): reading the log of a real workspace`.

## Risks

- **`$in` expands in written order** (design §5, `docs/design.md:174`), so the command is
  `sh say.sh hello.txt out/hello` and `say.sh`'s first argument is `hello.txt`: the first test's
  stdout is `said hello.txt\n`. `execute` confirms it on the first run and pins the bytes.
- **A digest-mismatched blob is removed by the read that finds it** (spec P9, the intent's one
  exception): no test here corrupts a blob; the removal test deletes one.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.

## Review findings

- risk — `Workspace::log`'s rustdoc said it "writes nothing", where the store removes a blob whose bytes fail its digest (spec §2, Reads; P9) — `workspace.rs` — fixed, and Task 1's wording amended to match
- nit — the two tests that overwrite or break the build file never showed the file fails to load — `workspace.rs`, `tests/e2e/log.rs` — fixed: each asserts `check()` is an error before reading the log
- nit — spec §6's e2e row still names the `chain` and `failing` fixtures — kept: this plan's "Fixture" note records why `logged` replaces them
- nit — an untracked `.DS_Store` at the worktree root — kept out of every commit

## Probe results

- The `$in` risk is settled on the first run: `say.sh`'s first argument is `hello.txt`, stdout `said hello.txt\n`, stderr `warned\n`, as the plan predicted.

## Deviations

- `tests/e2e/main.rs` declares `log` only: the test slice's e2e plan, which adds `test`, has not run on this branch yet.
- Task 2's tests were never red on behaviour: they exercise Task 1's code, which existed when they were written; their red run was `log::` matching no test before `mod log` was declared.
- The rustdoc link reads `` [`pipeline::log()`] ``, since `pipeline::log` names both a module and a function.
