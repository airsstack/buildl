---
status: done
created: 2026-10-20
depends-on: [01, 02]
built-on: 5fb250597c90
---

# Input Sandbox Implementation Plan

**Goal:** An `InputSandbox` execution strategy runs one action in its own exec dir holding copies of exactly its declared inputs and dependency outputs, with the pinned environment.

**Architecture:** `exec/sandbox.rs` holds `InputSandbox` and `ExecDir`; the exec dir is
`Scratch::action(label)` (plan 02), claimed before use and removed when `ExecDir` drops. Inputs,
dependency outputs and a written-out relative program are copied (`fs::copy`, mode kept) from the
workspace; the run is located against the exec dir, with `env_clear`, the declared variables and
`Isolation::Sandbox.pinned()` (plan 01). The helpers `Bare` already has for ending a run and
building an adapter error move to `exec/outcome.rs` so neither strategy names the other.

**Tech Stack:** Rust (`buildl`), `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2 rows Where an action runs, What is in it, Program, Environment,
`out/`, Clean up, Diagnostics; §3.3; §4 rows 3–7; §5 row `buildl` unit `exec/sandbox.rs`.

---

## Files that change

```
crates/buildl/src/exec/outcome.rs   — [create] `ended` and `fault`, moved from bare.rs, with their unit tests
crates/buildl/src/exec/bare.rs      — [modify] use the moved helpers
crates/buildl/src/exec/sandbox.rs   — [create] `InputSandbox`, `ExecDir`, and their unit tests
crates/buildl/src/exec/run.rs       — [create] `start` and `collect_in`, the run and collect steps both strategies share (review fix round)
crates/buildl/src/exec/mod.rs       — [modify] declare `outcome`, `sandbox`; export `InputSandbox`, `ExecDir`; module doc
```

## Order of work

### Task 1 — Move the shared run helpers out of `Bare`

**Files:**
- Create `crates/buildl/src/exec/outcome.rs` — `pub(super) fn ended(captured: &Capture) -> Exit`
  and `pub(super) fn fault(label: &Label, doing: &str, why: impl fmt::Display) -> Error`, moved
  unchanged from `bare.rs:147-169`
- Modify `crates/buildl/src/exec/bare.rs` — import them from `crate::exec::outcome`
- Modify `crates/buildl/src/exec/mod.rs` — `mod outcome;`

**Change:** none in behaviour; both strategies share the two helpers without naming each other.

**Proof:**
- `outcome.rs` `a_timed_out_capture_ends_timed_out_whatever_its_status`,
  `an_exit_status_ends_exited`, `a_signal_ends_signaled` — `ended` over hand-made `Capture`s
  (`exec/capture.rs`).
- `outcome.rs` `a_fault_names_the_port_label_doing_and_why` — renders
  `"exec strategy: //lib:probe: doing: why"`.
- Every existing `bare.rs` test passes unchanged.

**Done when:** `cargo test -p buildl --lib exec` passes and `cargo make dod` is green. Commit
`refactor(buildl): share how a run ends between strategies`.

### Task 2 — Prepare the exec dir

**Files:**
- Create `crates/buildl/src/exec/sandbox.rs` — `InputSandbox`, `ExecDir`, `prepare`
- Modify `crates/buildl/src/exec/mod.rs` — `mod sandbox;`, `pub use sandbox::{ExecDir, InputSandbox};`,
  Responsibilities and the first paragraph cover tier 2

**Change:**
- `InputSandbox::new(root: PathBuf, scratch: Scratch, owner: Owner, timeout: Timeout, search: SearchPath)`,
  `Debug`. `ExecDir` holds its path, exposes none of it publicly, and removes it with `clear_path`
  on `Drop`, ignoring a failure.
- `impl ExecStrategy for InputSandbox` with `type Dir = ExecDir`. `prepare`, in spec §3.3's order:
  1. for each output, `clear_path(root/<output>)` and make its parent under the real `out/`,
     exactly as `Bare::prepare` (`bare.rs:60-77`);
  2. `scratch.claim(owner)`;
  3. a relative program (the first argument, through `Program::of`, `program.rs:72`) with a `..`
     component is refused: diagnostic `"{program}: a program outside the action's directory
     cannot be copied in; name it by an absolute path or declare it"`;
  4. `clear_path(exec dir)`, then create it;
  5. copy `root/<input>` → `exec/<input>` for each `action.inputs`, making parents;
  6. for each `action.deps` path, copy every regular file below `root/<dep>` (or the file itself)
     to the same relative path under the exec dir;
  7. a relative program containing `/` that is not yet present in the exec dir is copied from
     `root/<program>`;
  8. make the parent of each output under the exec dir.
  Copies are `fs::copy` (mode kept, spec P18–P20); no link is made anywhere. Every failure is
  `fault(label, "<doing> <workspace-relative path>", io_error)` — the text names workspace
  paths and the program as written, never the exec dir or the scratch path (spec §2
  Diagnostics). `ExecDir` is built as soon as the dir exists, so a failure at step 5–8 drops it
  and removes the dir.

**Proof:** unit tests in `sandbox.rs` on a real file system, with a `workspace()` helper as in
`bare.rs:197-202` and a `Scratch` whose `temp` is the test's own temp directory made canonical
(`pwd` prints the physical path; on macOS `/var` is `/private/var`), owner = `Owner::of` a file
the test made:
- `prepare_clears_the_real_output_paths` — what an earlier build left at `out/<path>` is gone
  after `prepare`, and its parent exists (as `bare.rs:263`).
- `only_declared_inputs_and_dependency_outputs_are_present` — workspace holds `a.txt`,
  `secret.txt` and `out/lib/tree/{x,sub/y}`; the action declares input `a.txt` and dep
  `out/lib/tree`: the exec dir holds exactly `a.txt`, `out/lib/tree/x`, `out/lib/tree/sub/y` and
  the output's parent directory, and no `secret.txt`.
- `copies_keep_the_mode_and_are_not_links` — an executable input stays `0755`; each copy's inode
  differs from its source and none is a symlink.
- `the_exec_dir_is_outside_the_workspace_and_stable` — the dir does not start with the root, and
  two `prepare`s of one label give one path.
- `a_written_out_relative_program_is_copied_in` — command `tools/run.sh …` with `tools/run.sh`
  not declared: the exec dir holds it.
- `a_program_climbing_out_is_refused` — command `../outside/tool`: `prepare` fails, the text
  names `../outside/tool` and not the scratch path.
- `a_failed_copy_names_the_workspace_path_only` — a declared input that is missing: the error
  names `missing.txt` and does not contain `scratch.dir()`.
- `a_scratch_that_cannot_be_claimed_fails_the_prepare` — the scratch dir exists at `0755`: error,
  and the text has no path.
- `the_dir_is_gone_after_drop` — drop the `ExecDir`: the exec dir no longer exists.

**Done when:** `cargo test -p buildl --lib exec::sandbox` passes and `cargo make dod` is green.
Commit `feat(buildl): prepare an action's exec dir with its declared inputs only`.

### Task 3 — Run in the exec dir and collect from it

**Files:**
- Modify `crates/buildl/src/exec/sandbox.rs` — `run`, `collect`, tests

**Change:**
- `run` locates the program with `self.search.locate(&program, exec dir)` (`search_path.rs:67`,
  spec P9), runs with `current_dir` = exec dir, `env_clear`, the declared variables, then
  `Isolation::Sandbox.pinned()`, captured by `capture` with the timeout (`exec/capture.rs`); exit
  through `outcome::ended`. A program not located: `"starting the command: the program {named}
  was not found"`; a run that cannot start: `"running {named}"` — the program **as written**,
  never `program.display()` (spec §2 Diagnostics, `bare.rs:110-115`).
- `collect` maps each output to `outputs::found(exec/<output>, cas)` (`exec/outputs.rs:36`), with
  read errors as `fault(label, "reading the output {output}", …)` and the store's own error
  passed through, as `bare.rs:124-144`.

**Proof:**
- `an_undeclared_workspace_file_is_absent_to_the_command` — `sh -c 'cat secret.txt'` with
  `secret.txt` in the workspace and not declared: `Exit::Exited` non-zero.
- `the_command_runs_in_the_exec_dir` — `pwd` prints a path under `scratch.dir()`.
- `a_dependency_program_runs_from_the_exec_dir` — a dep output `out/greet` (an executable script
  printing `$0`) run as `out/greet`: exit 0 and `$0` is under the exec dir.
- `the_child_sees_declared_and_pinned_variables_only` — `env` with `FOO=bar` declared prints
  exactly `FOO=bar`, `LC_ALL=C`, `SOURCE_DATE_EPOCH=315532800`, `TZ=UTC` (sorted lines).
- `outputs_are_collected_from_the_exec_dir` — an action writing `out/lib/file` and a tree:
  `collect` gives `Found::File` and `Found::Tree` as `bare.rs:390` does, and the real `out/`
  path stays empty.
- `a_missing_program_names_it_as_written` — `SearchPath::new([])` and `sh`: the error is
  `"exec strategy: //lib:probe: starting the command: the program sh was not found"`, and no
  error text in this test module contains `scratch.dir()`.
- `an_action_past_its_timeout_is_stopped` — as `bare.rs:496`, under the sandbox.

**Done when:** `cargo test -p buildl --lib exec::sandbox` passes and `cargo make dod` is green.
Commit `feat(buildl): run an action in its exec dir with the pinned environment`.

---

## Risks

- **Task 2 step order.** The real `out/` paths are cleared before anything else, so a failed
  sandbox action leaves nothing there, as at tier 1 (spec §2 `out/`); the first test pins it.
- **Copy cost** on a Linux file system without reflinks (spec §7); correctness does not depend
  on it, and CI on `ubuntu-latest` runs these tests.
- **An action that leaves a read-only directory** in its exec dir defeats `ExecDir`'s removal and
  then the lock's (plan 02 Risks). `execute` checks, with one probe on the e2e fixtures of plan 04,
  that none does, using the fixtures that exist now: `chain` (`cp -R` of a directory dependency)
  and `tested`, each built once through `InputSandbox` in a unit test; if one does, the fix is
  in this plan's `ExecDir` drop.
- Not done here: choosing this strategy from the manifest (plan 04).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` green: `exec` names only `state` and `search_path`.
- `grep -rn "display()" crates/buildl/src/exec/sandbox.rs` finds no diagnostic built from a path
  under the exec dir.

## Review findings

- unit-test-mandate 🟡 — `prepare_clears_the_real_output_paths` pre-created the parent it then checked, so it proved nothing about prepare — `exec/sandbox.rs`; fixed: the pre-created file removed; with prepare's parent-making step removed the test fails (`20 passed; 1 failed`), restored.
- comment-matches-code 🔵 — `bare.rs` module doc called `Bare` "the one place" a run's steps meet — `exec/bare.rs:3`; fixed: reworded for two tiers.
- modularity 🔵 — `run` and `collect` repeated `Bare`'s bodies — `exec/sandbox.rs`; fixed: new `exec/run.rs` (`start`, `collect_in`) shared by both; each strategy keeps its own error wording and the sandbox adds the pinned variables.
- strong-types 🔵 — copy helpers took three `&Path`s each — `exec/sandbox.rs`; fixed: a private `Copier { root, exec }` with `file`, `tree`, `program`.
- correctness 🔵 — every error looking for a written-out program read as "absent" — `exec/sandbox.rs`; fixed: only `NotFound` is absent; test `a_program_that_cannot_be_looked_at_fails_the_prepare`.
- unit-test-mandate 🔵 — the two choices below had no test; fixed: `a_relative_program_absent_from_the_workspace_is_not_found_at_run`, `an_input_below_a_dependency_directory_is_copied_once` (each fails with its choice undone).
- Verified after the fix round: `cargo make dod` → exit 0, 0 warnings (buildl lib 350 passed); `cargo make guard-adapter-isolation` → `OK (30 edges)`; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (coder, before the fix round, which changed no dependency).

## Probe results

- **Risks: does a fixture action leave a read-only directory that defeats `ExecDir`'s removal?** Throwaway unit test built each action of `chain` (`greet`, `assets` `cp -R`, `site` `cp -R` of a dependency holding read-only files) and `tested` (`copy`, `same`, `flagged`) through `InputSandbox` (prepare → run → collect → drop): no exec dir survived. No fix needed; probe deleted.
- Every other claim is under a named test, whose red-green run was its probe (Task 3: `9 passed; 9 failed` against a stub, then `18 passed`).

## Deviations

- 2026-10-21 — step 7: a written-out relative program absent from the workspace is not copied and `run` reports "the program … was not found", as tier 1 does, rather than failing the copy at prepare (Plan does not refuse a program it cannot find).
- 2026-10-21 — a file already copied in this prepare (an input that also lies under a dependency directory, whose copies are read-only) is left alone; the dependency walk copies regular files only, since outputs hold nothing else.
- 2026-10-21 — `exec/run.rs` added off the file list in the review fix round (shared run and collect); file list amended.
- 2026-10-21 — two extra tests in Task 3 (`a_store_that_fails_fails_the_collect_with_its_own_error`, `a_program_that_cannot_start_is_named_as_written`).
- 2026-10-21 — one coder ran the three tasks in sequence; Tasks 2 and 3 both write `sandbox.rs`, so they cannot be split into separate commits by file.
