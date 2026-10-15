---
status: done
created: 2026-10-15
depends-on: [08]
built-on: fefed82cb705
---

# `buildl test` Implementation Plan

**Goal:** The `buildl` binary has a `test` command that runs the test targets and ends with the
status of design §8.7.

**Architecture:** `Command::Test(TestArgs)` sits after `Build` in `buildl-cli/src/cli.rs`;
`TestArgs` shares `Selection` with `plan` and `build` and maps its flags to the library's values
(`OnFailure`, `LiveOutput`, `Rerun`, `TestBlocks`). `run.rs`'s `test` is `build`'s flow with
`Workspace::test` and `Report::Test`. `Error::NotATest` is a usage error. The help of `Selection`
and the binary's command table say what a selection with no targets means for each command.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `clap` (derive). Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §2 (the command, its flags, stdout, exit, `-k` refused), §3.8 (third
bullet), §6 (CLI rows and `exit.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl-cli`.

Facts the tasks rest on, each read at commit `1799d47` or left by an earlier plan:

| Fact | Where |
|---|---|
| `Command { Check, Graph(GraphArgs), Query(QueryArgs), Plan(PlanArgs), Build(BuildArgs), Log(LogArgs) }` (`Log` added by the log slice, which ran first on this branch), each variant's doc line is its one-line help | `crates/buildl-cli/src/cli.rs:51-63` |
| `Selection { targets: Vec<String>, overrides: Vec<Override> }`, struct doc "What `plan` and `build` share: which targets, under which settings."; `targets`' help "Targets to build, as //dir:name, :name or name; every target when none is given" | `cli.rs:96-105` |
| `BuildArgs { selection, keep_going (-k, --keep-going), stream }` with `on_failure()` and `live()` | `cli.rs:118-147` |
| `cli.rs` tests: `each_command_parses` (`:248`), `keep_going_and_stream_are_flags` (`:330`), helpers `build(args)`, `refused(args)` | `cli.rs:199-370` |
| `run.rs` dispatch `Command::Build(args) => build(&start, args, terminal.mode(), paint, stderr)`; `build` reads overrides, opens the workspace, `scope(&workspace, &targets)`, makes a `TerminalReporter`, calls `workspace.build`, closes the reporter on error, returns `Exit::of_outcomes` | `crates/buildl-cli/src/run.rs:98-103`, `:229-252` |
| `run.rs` unit test `a_build_of_nothing_says_so_on_stderr` | `run.rs:590` |
| `Exit::of_error` maps `UnknownOverride`, `UnknownTarget`, `InvalidQuery`, `NoRun` to `Usage`, and `NotATest` falls to the `_ => Internal` arm; tests list `usage_errors()` and check `every_error_variant_has_its_code` | `crates/buildl-cli/src/exit.rs:76-80`, `:189-204`, `:430` |
| The binary's module doc has a command table and a paragraph saying "naming none means every target" | `crates/buildl-cli/src/main.rs:8-24` |
| CLI integration tests pin `--help` byte for byte: the command list (`help_names_every_command_and_flag`, `arguments.rs:79`), each command's help (`each_command_has_its_help`, `:104`, with the shared `SELECTION` constant at `:38-43`), and the short help (`no_command_prints_the_short_help_on_stderr`, `:194`) | `crates/buildl-cli/tests/cli/arguments.rs` |
| CLI test helpers: `workspace(files)`, `manifest(programs)`, `run(start, args, variables)`, `code`, `stdout`, `stderr`; exit tests render an error as `error: <message>\n` on stderr (`an_unknown_target_is_a_usage_error`) | `crates/buildl-cli/tests/cli/common.rs:12-88`; `exit_codes.rs:214-233` |
| `TerminalReporter::new(out, mode, paint, report)`, `Report::{Build, Test { blocks, rerun }}`, `TestBlocks::{Failed, Every}`; `Workspace::test(scope, overrides, on_failure, live, rerun, reporter)`; `buildl_core::Rerun::{Off, Tests}` | `plans/07-test-reporter.md` Task 2; `plans/08-workspace-test.md` Task 1; `plans/04-rerun.md` Task 1 |
| `Error::NotATest`'s message: `target {asked} was asked for as a test, but {target} is declared with b.target in {provenance}` | `plans/01-selection.md` Task 1 |

## Files that change

```
crates/buildl-cli/src/cli.rs              — [modify] Command::Test, TestArgs and its mappings; Selection's help; tests
crates/buildl-cli/src/run.rs              — [modify] dispatch and test(); a unit test
crates/buildl-cli/src/exit.rs             — [modify] NotATest is a usage error; tests
crates/buildl-cli/src/main.rs             — [modify] the command table row and the selection paragraph
crates/buildl-cli/tests/cli/arguments.rs  — [modify] the pinned helps; two tests
crates/buildl-cli/tests/cli/output.rs     — [modify] three tests of what test prints
crates/buildl-cli/tests/cli/exit_codes.rs — [modify] three tests of test's status
```

## Order of work

### Task 1 — `buildl test` on the command line

**Files:** `src/cli.rs`, `src/run.rs`, `src/exit.rs`, `src/main.rs`,
`tests/cli/arguments.rs` (all `crates/buildl-cli`).

**Change:**
- `Command::Test(TestArgs)` after `Build` and before `Log`, doc line `Run test targets and what they need`; every list the binary prints keeps that order.
- `TestArgs { selection: Selection, stop: bool, stream: bool, rerun: bool, verbose: bool }`,
  flags long only, their help lines:
  - `--stop` — `Start nothing new after the first failure`
  - `--stream` — `Show one running action's output as it is printed` (as `build`'s)
  - `--rerun` — `Run the selected tests even when nothing they depend on changed`
  - `--verbose` — `Show what every test printed, not only what a failing one did`
  No `-k`/`--keep-going`. Methods: `on_failure()` (`KeepGoing` unless `--stop`, then `Stop`),
  `live()`, `rerun()` (`Rerun::Tests` with `--rerun`), `blocks()` (`TestBlocks::Every` with
  `--verbose`, else `Failed`), each a `const fn` like `BuildArgs`'s.
- `Selection`'s doc becomes "What `plan`, `build` and `test` share: which targets, under which
  settings."; `targets`' help becomes exactly
  `Targets, as //dir:name, :name or name; with none, plan and build take every target but tests, and test every test`.
- `run.rs`: `Command::Test(args) => test(&start, args, terminal.mode(), paint, stderr)`; `fn
  test` as `build`, with `TerminalReporter::new(…, Report::Test { blocks: args.blocks(), rerun:
  args.rerun() })` and `workspace.test(&scope, &settings, on_failure, live, rerun, &mut
  reporter)`. It writes nothing to stdout.
- `exit.rs`: `Error::NotATest { .. }` joins the `Usage` arm; `usage_errors()` gains a
  `NotATest` value.
- `main.rs`: a table row
  `` | `buildl test [TARGET]... [--set NAME=VALUE]... [--stop] [--stream] [--rerun] [--verbose]` | nothing | a block for each failing test (and each test that printed, with `--verbose`) and for each other target that printed or failed, and a line counting the tests | ``, after `build`'s row and before `log`'s;
  the paragraph's "naming none means every target" becomes "naming none means every target but
  tests for `plan` and `build`, and every test for `test`", and it says `--stop`, `--rerun` and
  `--verbose` in one sentence each.
- `arguments.rs`'s pinned helps take the new command line and `SELECTION` text, and a `test`
  case joins `each_command_has_its_help`, pinned from the binary's output.

**Proof:**
- `test_flags_map_to_their_values` in `cli.rs`'s tests — `test` alone maps to `KeepGoing`,
  `LiveOutput::Off`, `Rerun::Off`, `TestBlocks::Failed`; `test --stop --stream --rerun
  --verbose` to `Stop`, `OneAction`, `Rerun::Tests`, `TestBlocks::Every`; `test` keeps targets as
  written.
- `test_takes_no_keep_going` in `cli.rs`'s tests — `test -k` and `test --keep-going` are refused
  as `ErrorKind::UnknownArgument`.
- `each_command_parses` gains `test`.
- `not_a_test_is_a_usage_error` in `exit.rs`'s tests — `Exit::of_error(&NotATest{…})` is
  `Exit::Usage`; `every_error_variant_has_its_code` passes with the new value in
  `usage_errors()`, its two literals moved from `[4, 3, 1, 16, 3, 6]` and `33` to
  `[5, 3, 1, 16, 3, 6]` and `34`.
- `a_test_run_of_a_workspace_without_tests_says_so_on_stderr` in `run.rs`'s tests — `test` on
  the `copying()` workspace exits `Success`, writes `no tests\n` to stderr and nothing to stdout.
- `help_names_every_command_and_flag`, `each_command_has_its_help` and
  `no_command_prints_the_short_help_on_stderr` pass on the updated pins.

**Done when:** `cargo test -p buildl-cli` passes and the Definition of Done is green. Commit
`feat(buildl-cli): the test command`.

### Task 2 — What `buildl test` prints and how it ends, on the binary

**Files:** `tests/cli/output.rs`, `tests/cli/exit_codes.rs`, `tests/cli/arguments.rs`.

**Change:** tests only. Their workspaces: a manifest allowing `sh` and `cp`; `pass`
(`buildl.test`, `run = { "sh", "-c", "echo ok" }`) and `fail` (`buildl.test`, `run = { "sh", "-c",
"echo bad; exit 1" }`), written with the `buildl` global as the file's `COPY` is (`exit_codes.rs:11-12`) for the output tests; the `COPY` target alone for the no-test and
not-a-test cases. `sh -c` is a system tool used in tests only (architecture-building-blocks §8).

**Proof:**
- `test_prints_only_failing_tests_and_the_test_line` (`output.rs`) — `buildl test`: stdout is
  empty; stderr is `//:fail`'s block exactly as `buildl build fail` prints it, then
  `tests: 1 passed, 1 failed\n`; `ok` appears nowhere in it.
- `test_verbose_prints_every_test` — `buildl test --verbose`: stderr holds `//:pass`'s block
  with `ok` as well, after `//:fail`'s (blocks come in label order), then the same test line.
- `test_stop_starts_nothing_after_the_first_failure` — `buildl test --stop`: exits 1 and its
  stderr ends `tests: 1 failed, 1 skipped\n`.
- `test_stream_shows_a_passing_test_as_it_runs` — `buildl test --stream pass`: stderr is
  exactly `--- //:pass\nok\ntests: 1 passed\n`.
- `test_rerun_line` — `buildl test pass`, then `buildl test --rerun pass`: the second exits 0 and its
  stderr is exactly `tests (rerun): 1 passed\n` — `//:pass` ran again and, passing, has no
  block.
- `test_with_a_failing_test_exits_1` (`exit_codes.rs`) — `buildl test` over `pass` and `fail`
  exits 1.
- `test_naming_a_build_target_exits_2` — `buildl test copy` exits 2 with stderr exactly
  `error: target //:copy was asked for as a test, but //:copy is declared with b.target in build.lua\n`.
- `test_with_no_tests_exits_0` — `buildl test` over `COPY` alone exits 0, stderr `no tests\n`,
  and `out/copy` was not made.
- `test_rejects_keep_going` (`arguments.rs`) — `buildl test -k` exits 2 with nothing on stdout.
- `test_accepts_stop_stream_rerun_verbose` — `buildl test --stop --stream --rerun --verbose` over
  `pass` alone exits 0.

**Done when:** `cargo test -p buildl-cli --test cli` passes and the Definition of Done is green.
Commit `test(buildl-cli): what the test command prints and how it ends`.

## Risks

- **Pinned help is byte-exact**, so every help string here is written once and pinned from the
  binary; a wording change elsewhere shows up as a pin diff, which is intended.
- `--stream` with `--verbose` and a hidden pass is exercised only by the reporter's unit tests
  (plan `07`); the binary test here checks that the flags are accepted together.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.

## Review findings

- risk — `main.rs` said `--stop` starts nothing new after the first failed *test*; it stops on any failure, a dependency's included — `src/main.rs` — fixed: "after the first failure"
- risk — nothing failed if `test()` stopped passing `--stop` or `--stream` on to the workspace — `tests/cli/output.rs` — fixed: `test_stop_starts_nothing_after_the_first_failure`, `test_stream_shows_a_passing_test_as_it_runs`, added to Task 2's Proof
- amendment — the plan still read the command list without `log` and the exit literals of a branch without it — this plan — fixed: facts table, Task 1's Change and Proof amended
- nit — the `test` row left out `--verbose` — `src/main.rs` and Task 1's row — fixed. The finding also asked for skipped tests; the documentation review that followed showed a skipped test gets no block (`block` writes nothing for a skipped target that printed nothing), so the row names failing tests only
- nit — a paragraph of `main.rs`'s module doc broke short — fixed
- nit — `test_with_no_tests_exits_0` uses the helper `failed` for a run that succeeds — kept: the file's `success_exits_zero` already uses it so

## Probe results

- None needed beyond the named tests: every claim the plan makes about the binary is pinned by one.

## Deviations

- The log slice ran first on this branch: `Test` sits between `Build` and `Log` in `Command` and in every list the binary prints, and `every_error_variant_has_its_code` reads `[5, 3, 1, 16, 3, 6]` and `34` (amended above).
- `test_verbose_prints_every_test` pins `//:fail`'s block before `//:pass`'s: blocks come in label order.
- `TestArgs` carries `#[expect(clippy::struct_excessive_bools, reason = …)]`: clap maps each long flag to a `bool`, and the methods turn them into the library's enums.
- `each_command_has_its_help`'s `plan`, `build` and `test` cases moved into a helper, `selecting_helps()`, under `too_many_lines`.
- `main.rs`'s sentence on `--keep-going`'s severity now also says `test` keeps going unless `--stop` is given.
- Both tasks are one commit, `feat(buildl-cli): the test command`.
