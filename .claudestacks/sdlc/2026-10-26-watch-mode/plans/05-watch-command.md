---
status: done
created: 2026-10-26
depends-on: [03, 04]
built-on: a08d695c7505
---

# Watch Command Implementation Plan

**Goal:** The `buildl` binary runs `buildl watch [target]...` with `build`'s flags.

**Architecture:** `TerminalReporter` (`crates/buildl/src/report/terminal.rs:84`) already
writes a build's blocks and summary to stderr; it gains plan 01's `WatchReport`, so one value
writes every line of a watch and `Workspace::watch` (plan 04) can take it as both reporter and
watch report. Its three lines are spec §7's, and a failed cycle is rendered with the
`error_text` every command uses (`crates/buildl/src/report/message.rs:110`). In
`buildl-cli`, `Command::Watch(BuildArgs)` reuses `build`'s flags (spec P12,
`crates/buildl-cli/src/cli.rs:152-180`). Spec §4 names a `WatchArgs` flattening `BuildArgs`;
with no flag of its own, that wrapper would hold one field and add nothing, so this plan uses
`BuildArgs` directly, a recorded departure with the same behaviour. `run.rs` gains `watch`,
shaped like `build` (`run.rs:358-386`): arguments, `Workspace::open`, remote policy, scope;
then `NotifyChanges::start(workspace.root())` (plan 03) before the first cycle (spec §2); then
`Workspace::watch`. The loop only ever returns the watcher's error: `watch` writes it as
spec §6's `watch: <error>` line — `watch: ` followed by `error_text` — and returns its exit
code from `Exit::of_error`, which is 3 for the file-watcher port (`exit.rs:113-116`).

**Tech Stack:** Rust, `buildl`, `buildl-cli`, `clap`, `cargo make dod`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2, §6, §7, §8 (CLI row), P12, P13.

---

## Files that change

```
crates/buildl/src/report/watch_text.rs      — [create] the watch lines as text; unit tests
crates/buildl/src/report/terminal.rs        — [modify] impl WatchReport for TerminalReporter; unit tests
crates/buildl/src/report/mod.rs             — [modify] `mod watch_text;` and `pub use watch_text::{…}`, as every sibling is declared and re-exported
crates/buildl-cli/src/cli.rs                — [modify] Command::Watch(BuildArgs) and its help line; parse tests
crates/buildl-cli/src/run.rs                — [modify] dispatch Command::Watch to `watch`; `fn watch`
crates/buildl-cli/tests/cli/watch.rs        — [create] the binary under `watch`, stopped by SIGINT
crates/buildl-cli/tests/cli/main.rs         — [modify] `mod watch;`
crates/buildl-cli/tests/cli/arguments.rs    — [modify] `help_names_every_command_and_flag` (:129-140) enumerates commands: add `watch`
```

---

### Task 1 — The watch lines

**Files:**
- Create `crates/buildl/src/report/watch_text.rs`
- Modify `crates/buildl/src/report/mod.rs`, `crates/buildl/src/report/terminal.rs`

**Change:** text functions for spec §7: `watch: building` for `Trigger::Start`;
`watch: <path> changed` for a batch of one; `watch: <first> and <n> more changed` for a
batch of more (first = `ChangeBatch::first()`, n = `len() - 1`); `watch: waiting for
changes`; and `watch: ` followed by `error_text(error, paint)` for the watcher's own error
(spec §6), used by plan 05 Task 2. `impl WatchReport for TerminalReporter`: `building` and `waiting` write their line
on a row of their own (closing the status line first, as `close` does at `terminal.rs:132`);
`failed` closes the status line and writes `error_text(error, paint)`. Painting follows the
reporter's `Paint` as its other lines do.

**Proof:** written first, seen failing, then passing —
- `start_is_building`, `one_path_is_named`, `more_paths_are_counted` (`a.c and 2 more`), `waiting_is_said`, `a_watcher_error_is_prefixed` — in `watch_text.rs`
- `a_failed_cycle_is_written_as_any_error_is` — `TerminalReporter` over a `Vec<u8>`: `failed` writes exactly `error_text` of the error
- `watch_lines_start_on_a_row_of_their_own` — after `begun`/`started` in `Mode` with a status line, `waiting` writes its line after the status line is closed

**Done when:** `cargo test -p buildl report::` passes and the stack's Definition of Done is
green. Commit `feat(buildl): say what a watch is doing`.

### Task 2 — `buildl watch`

**Files:**
- Modify `crates/buildl-cli/src/cli.rs`, `crates/buildl-cli/src/run.rs`
- Modify `crates/buildl-cli/tests/cli/arguments.rs` if its help test enumerates commands

**Change:** `Command::Watch(BuildArgs)`, help line `Build targets, then build again when a
file they read changes`. `run.rs` dispatches it (`run.rs:113-142`) to `fn watch`, with
`build`'s parameters: `overrides`, `Workspace::open(start)`, `remote.resolve(&args.remote,
remote_table(&workspace))`, `scope`, exactly as `build` (`run.rs:370-374`); then
`NotifyChanges::start(workspace.root())`; then a `TerminalReporter` with `Report::Build` and
`workspace.watch(&mut changes, &scope, &settings, RunOptions::new(on_failure,
live).with_remote(policy), &mut reporter, console)`; on the returned error, `reporter.close()`,
write the `watch: ` error line of Task 1 to stderr, and return `Ok(Exit::of_error(&error))`.
A `NotifyChanges::start` failure takes the same path. The remote policy is resolved once, from the
flags, the variables and the first manifest; each cycle's `Workspace::build` applies it only
when that cycle's manifest names a remote (`workspace.rs:164-170`).

**Proof:** written first, seen failing, then passing —
- `cli.rs` unit: `watch_parses_builds_flags` — `watch -k --stream --set a=b //x --remote-upload --remote-strict` fills `BuildArgs` as `build` does; `watch_with_no_target_is_the_whole_workspace`
- `arguments.rs`: `help_names_every_command_and_flag` names `watch`
(The remote-flag conflict, raised by `RemoteSwitches::resolve` after the workspace opens,
`remote_switches.rs:100-118`, is proven on the binary in Task 3.)

**Done when:** `cargo test -p buildl-cli` passes and the stack's Definition of Done is green.
Commit `feat(buildl-cli): add buildl watch`.

### Task 3 — The binary under watch

**Files:**
- Create `crates/buildl-cli/tests/cli/watch.rs`; modify `crates/buildl-cli/tests/cli/main.rs`

**Change:** tests only. Start the binary as a child (`Command::new(env!("CARGO_BIN_EXE_buildl"))`,
as `tests/cli/common.rs:72-73` does, with stderr piped) on a workspace from
`common::workspace` (`common.rs:12`) whose one target copies an input. A reader thread
forwards stderr lines over a channel; every wait is a `recv_timeout` with a generous bound,
so a failure ends the test instead of hanging it. The child is held by a small guard whose
`Drop` kills and reaps it, so a test that fails before its SIGINT leaves no `buildl watch`
running. SIGINT is sent with `kill -INT <pid>` through `std::process::Command`:
`crates/buildl-cli/Cargo.toml` has no `libc`, and this adds no dependency.

**Proof:** written first, seen failing, then passing —
- `watch_builds_then_waits` — `watch: building`, a summary line, `watch: waiting for changes`
- `an_edit_rebuilds` — after the first waiting line, append to the input; `watch: <input> changed`, a summary with one built, waiting again
- `sigint_ends_watch_without_a_summary` — `kill -INT` while waiting; the child exits by signal and prints no further summary line
- `a_usage_error_exits_2` — `watch --no-remote --remote-upload` exits 2 before any `watch:` line (the `RemoteFlags` conflict)

**Done when:** `cargo test -p buildl-cli --test cli watch` passes on macOS and in CI on Linux,
and the stack's Definition of Done is green. Commit `test(buildl-cli): run buildl watch end to end`.

---

## Risks

- SIGINT through `kill -INT` assumes a `kill` program on `PATH`, true on both CI images
  (`ubuntu-latest`, `macos-latest`, spec P15).
- The watcher error line is spec §6's `watch: <error>`; it is never reached in a test (the
  watcher does not fail on demand), so only its text is unit-tested (Task 1).
- The status line: on a terminal `TerminalReporter` keeps a live status line; Task 1's
  ordering test is what keeps `watch:` lines off it. Tests run without a terminal, so the CLI
  test sees the plain mode only.

## Proof

`cargo make dod` green; `cargo test -p buildl-cli` covers spec §8's CLI row; a manual
`cargo run -p buildl-cli -- watch` in `crates/buildl/tests/fixtures/chain` (copied out first)
shows spec §7's lines.

## Review findings

- correctness — the trigger line was built from the unfiltered batch, so the first build's own writes, delivered late by the OS and coalesced with an edit, made it read `watch: .buildl and 23 more changed` instead of `watch: a.txt changed` — `crates/buildl-core/src/watch/run.rs`, `react.rs`. Found by Task 3's `an_edit_rebuilds` (red: left `["watch: .buildl and 23 more changed", "1 built", "watch: waiting for changes"]`). Fixed and verified: new `kept` drops out/state/`.git` paths first; the loop runs nothing on an empty remainder and passes `Trigger::Changed(kept)`; `the_trigger_names_only_the_paths_kept` failed before the fix (named `.buildl` with all 3 paths), passes after; `kept_holds_the_paths_outside_the_ignored_directories`, `kept_is_none_when_every_path_is_dropped` added; `cargo test -p buildl-cli --test cli watch` 4 passed; `cargo make dod` green (fix coder, run once).
- unit-test-mandate — `watch_ended` had no test, so dropping the `watch: ` prefix or the error's exit code would go unseen — `crates/buildl-cli/src/run.rs`. Fixed and verified: `a_watch_that_ends_says_why_and_exits_with_the_error_status` (failed against a wrong expectation first); `cargo test -p buildl-cli` 108 unit, 122 cli passed; `cargo make dod` green.
- doc-comment-discipline — `watch`'s doc named only the watcher failure as an end; the comment above `reporter.close()` claimed the watch ended mid-line — `run.rs`. Fixed: the doc names the watch-layout error before the first build; the comment calls the close a safeguard.
- test strength — `sigint_ends_watch_without_a_summary` missed a `nothing to build` line; `a_usage_error_exits_2` passed on any usage error — `crates/buildl-cli/tests/cli/watch.rs`. Fixed: asserts nothing more was printed, and the `--no-remote cannot be combined with --remote-upload or --remote-strict` text.
- style — doc lines over 100 columns in `buildl-core/src/watch/{run.rs,react.rs}`, `buildl/src/report/mod.rs`. Fixed: reflowed.
- design note — the batch is filtered twice (`kept`, then `react`). No change: `react` is public and stays whole on its own (spec §3 places the drop there); the second pass costs nothing.

## Probe results

- No throwaway probe: SIGINT ending the process by signal (Risk 1) and the status-line ordering (Risk 3) are exercised by Task 3's `sigint_ends_watch_without_a_summary` and Task 1's `watch_lines_start_on_a_row_of_their_own`, both green. Painting: `summary_line` is unpainted too and `Paint` styles only headers and failure words, so unpainted watch lines match "as its other lines do" (reviewer read).
- `an_edit_rebuilds` came out against plan 01's loop (see Review findings, first line); with a 4 s pause before the edit (manual run) the line was `watch: a.txt changed`, so it is a race between the first build's file events and the edit.

## Deviations

- 2026-10-26 — `buildl-core`'s `watch/react.rs` and `watch/run.rs` changed (the `kept` fix), files outside this plan's list; recorded here, committed separately as `fix(buildl-core): name only the kept paths of a batch`.
- 2026-10-26 — `no_command_prints_the_short_help_on_stderr` (`tests/cli/arguments.rs`) also lists the commands and gained `watch`.
- 2026-10-26 — Task 1 added `a_watch_line_ends_a_line_a_live_action_left_open` beside the named tests; `close` was split into `closing` and `say`.
