---
status: approved
created: 2026-10-15
depends-on: [04, 05]
built-on: 280eda6f4573
---

# `buildl log` Implementation Plan

**Goal:** The `buildl` binary has a `log` command that prints what a target printed when it last
ran, each stream where it was asked for.

**Architecture:** `Command::Log(LogArgs)` follows `Test` in `buildl-cli/src/cli.rs`. `--stdout`
and `--stderr` are a clap group of at most one, as `GraphFormat` is, mapped to the enum
`Streams { Both, Stdout, Stderr }`. `run.rs`'s `log` reads the label as `scope` does (a bad one is
a clap usage error), calls `Workspace::log`, writes the header and any damage warning
(plan `04`) to stderr, then the stored bytes as asked. `NoRun` is a usage error and
`MissingOutput` an infrastructure failure.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `clap` (derive). Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §1 P14, P16, P17, §2, §3.6, §6 (CLI rows and `exit.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl-cli`.

**The argument struct.** Spec §3.6 writes `LogArgs { target: String, streams: Streams }`. A clap
group of two flags is a struct of two `bool`s (`GraphFormat`, `cli.rs:74-83`), so `LogArgs` holds
that struct, `StreamFlags`, and `LogArgs::streams()` turns it into the enum `Streams` the rest of
the binary reads; no `bool` leaves `cli.rs`.

Facts the tasks rest on, each read at commit `0cca515` or left by an earlier plan:

| Fact | Where |
|---|---|
| `GraphFormat` is `#[derive(Debug, Args)] #[group(multiple = false)]` with `--json` and `--dot`; `json_and_dot_exclude_each_other` tests it | `crates/buildl-cli/src/cli.rs:72-83`, `:304` |
| After the test slice: `Command { Check, Graph, Query, Plan, Build, Test }`, `TestArgs`, `run.rs` dispatches `Command::Test` | `.claudestacks/sdlc/2026-10-15-test/plans/09-cli-test.md`, Task 1 |
| `run.rs`'s dispatch passes `stdout` and `stderr` writers to each command fn; `Stop::{Usage, Failed, Unwritten}`; `product(stdout, bytes)` writes and flushes stdout, and a closed stdout ends in success | `crates/buildl-cli/src/run.rs:96-118`, `:141-144` |
| `query` wraps a bad target as `Cli::command().error(ErrorKind::ValueValidation, format!("invalid value '{}' for '<EXPR>': {error}", …))`; `scope` does the same for `<TARGET>` | `run.rs:190-199`, `:148-160` |
| `run.rs` tests: helpers `workspace(files)`, `copying()`, `ran(start, args) -> Ran` | `run.rs:283-350` |
| `Exit::of_error`: `UnknownOverride`, `UnknownTarget`, `InvalidQuery` (and, after the test slice, `NotATest`) → `Usage`; `Error::Adapter` for any port but the manifest → `Infrastructure`; tests `usage_errors()`, `infrastructure_errors()`, `every_error_variant_has_its_code` | `crates/buildl-cli/src/exit.rs:76-86`, `:189-215`, `:430` |
| The binary's module doc: the command table and the paragraph on targets | `crates/buildl-cli/src/main.rs:8-28` |
| Pinned help: `help_names_every_command_and_flag` (`arguments.rs:79`), `each_command_has_its_help` (`:104`), `no_command_prints_the_short_help_on_stderr` (`:194`); after the test slice they hold `test` | `crates/buildl-cli/tests/cli/arguments.rs` |
| CLI test helpers: `workspace(files)`, `manifest(programs)`, `run(start, args, variables)`, `code`, `stdout`, `stderr`; an error is printed as `error: <message>\n` | `crates/buildl-cli/tests/cli/common.rs:12-88`; `exit_codes.rs:214-233` |
| `Workspace::log(&Label) -> Result<Shown>`; `Shown { last, stdout, stderr }`; `buildl::report::{header, damage}` | `plans/05-workspace-log.md` Task 1; `plans/04-log-text.md` Task 2 |
| Messages: `target {label} has no run in the log…`; `target {label}: the output of its last logged run is no longer in the store` | `plans/02-pipeline-log.md` Task 2 |

## Files that change

```
crates/buildl-cli/src/cli.rs              — [modify] Command::Log, LogArgs, StreamFlags, Streams; tests
crates/buildl-cli/src/run.rs              — [modify] dispatch and log(); a test
crates/buildl-cli/src/exit.rs             — [modify] NoRun → Usage, MissingOutput → Infrastructure; tests
crates/buildl-cli/src/main.rs             — [modify] the command table row; a sentence on log
crates/buildl-cli/tests/cli/arguments.rs  — [modify] the pinned helps; three tests
crates/buildl-cli/tests/cli/output.rs     — [modify] four tests of what log prints
crates/buildl-cli/tests/cli/exit_codes.rs — [modify] three tests of log's status
```

## Order of work

### Task 1 — `buildl log` on the command line

**Files:** `src/cli.rs`, `src/run.rs`, `src/exit.rs`, `src/main.rs`, `tests/cli/arguments.rs`.

**Change:**
- `Command::Log(LogArgs)` after `Test`, doc line `Show what a target printed when it last ran`.
- `LogArgs { target: String, streams: StreamFlags }`: `target` with `value_name = "TARGET"` and
  help `The target, as //dir:name, :name or name`; `StreamFlags` a `#[group(multiple = false)]`
  of `--stdout` (`Print only what the run wrote to standard output`) and `--stderr` (`Print only
  what the run wrote to standard error, on standard output`).
- `pub(crate) enum Streams { Both, Stdout, Stderr }` (`Debug, Clone, Copy, PartialEq, Eq`) and
  `LogArgs::streams(&self) -> Streams`.
- `run.rs`: `Command::Log(args) => log(&start, &args, stdout, stderr)`; `fn log` opens the
  workspace, reads `args.target` with `Workspace::label`, its error wrapped as `scope` wraps one
  (`invalid value '<raw>' for '<TARGET>': <error>`), calls `workspace.log`, writes
  `report::header(&shown.last)` and then `report::damage(&shown.last.damaged)` when it is `Some`
  to stderr, then: `Both` — `shown.stdout` through `product` to stdout, then `shown.stderr` to
  stderr; `Stdout` — `shown.stdout` to stdout; `Stderr` — `shown.stderr` to stdout. Bytes are
  written as stored. Returns `Exit::Success`.
- `exit.rs`: `Error::NoRun { .. }` joins the `Usage` arm, `Error::MissingOutput { .. }` maps to
  `Infrastructure`; `usage_errors()` and `infrastructure_errors()` gain one value each.
- `main.rs`: a row `` | `buildl log <TARGET> [--stdout \| --stderr]` | the stored stdout, or the
  one stream asked for | the header, a warning of damaged lines, and the stored stderr when no
  flag is given | `` after `test`'s, and a sentence: `log`, like `query`, takes no lock, loads no
  build file and writes nothing; it does not follow an alias.
- `arguments.rs`'s three pinned helps gain `log` (command list and short help: `  log    Show
  what a target printed when it last ran`), and `each_command_has_its_help` gains a `log` case,
  pinned from the binary.

**Proof:**
- `log_streams_map_to_their_values` in `cli.rs`'s tests — `log app` is `Streams::Both`,
  `log app --stdout` `Stdout`, `log app --stderr` `Stderr`; the target is kept as written.
- `log_takes_one_stream_flag_and_one_target` — `log app --stdout --stderr` is
  `ErrorKind::ArgumentConflict`; `log` alone is `ErrorKind::MissingRequiredArgument`.
- `each_command_parses` gains `log app`.
- `no_run_is_a_usage_error` and `missing_output_is_an_infrastructure_failure` in `exit.rs`'s
  tests — `Exit::of_error` gives `Usage` and `Infrastructure`; `every_error_variant_has_its_code`
  passes with the new values, its two literals moved from what the test slice leaves (`[4, 2, 1,
  16, 3, 6]`, `32`) to `[5, 3, 1, 16, 3, 6]` and `33` (`exit.rs:446`, `:451`).
- `only_the_manifests_adapter_is_a_definition_error` (`exit.rs:460`) gains `PortName::RunHistory`,
  so a log that cannot be read is shown to exit 3.
- `log_writes_the_stored_streams_to_their_writers` in `run.rs`'s tests — `copying()` built, then
  `log copy`: exit `Success`, stdout empty, stderr one line starting `//:copy: built at ` and
  ending ` UTC\n`.
- The three pinned-help tests pass.

**Done when:** `cargo test -p buildl-cli` passes and the Definition of Done is green. Commit
`feat(buildl-cli): the log command`.

### Task 2 — What `buildl log` prints and how it ends, on the binary

**Files:** `tests/cli/output.rs`, `tests/cli/exit_codes.rs`, `tests/cli/arguments.rs`.

**Change:** tests only. Their workspace for the output tests: a manifest allowing `sh`, and
`buildl.target("both", { run = { "sh", "-c", "echo out; echo err >&2" }, outputs = {} })`,
built once with `buildl build` before `log` runs; `sh -c` is a system tool in tests only
(architecture-building-blocks §8). The header's time is the wall clock's, so it is checked by
shape: `//:both: built at `, ten characters `YYYY-MM-DD` of digits and `-`, a space, eight
`HH:MM:SS`, ` UTC`, a newline.

**Proof:**
- `log_prints_each_stream_on_its_own_channel` (`output.rs`) — `buildl log both`: stdout `out\n`;
  stderr the header line, then `err\n`; exit 0.
- `log_stdout_prints_only_stdout` — `buildl log both --stdout`: stdout `out\n`; stderr the header
  line alone.
- `log_stderr_prints_stored_stderr_on_stdout` — `buildl log both --stderr`: stdout `err\n`.
- `log_header_is_on_stderr` — the header line of `buildl log both` has the shape above.
- `log_of_a_target_that_never_ran_exits_2` (`exit_codes.rs`) — the `COPY` workspace never built:
  `buildl log copy` exits 2 with stderr exactly `error: target //:copy has no run in the log\n`.
- `log_with_a_blob_removed_exits_3` — `COPY` built, `.buildl/cas` removed: `buildl log copy`
  exits 3 with stderr exactly
  `error: target //:copy: the output of its last logged run is no longer in the store\n`.
- `log_with_a_broken_build_file_exits_0` — `COPY` built, `build.lua` overwritten with `this is
  not lua`: `buildl log copy` exits 0.
- `log_rejects_both_stream_flags` (`arguments.rs`) — `buildl log copy --stdout --stderr` exits 2
  with nothing on stdout.
- `log_reads_a_relative_target` — a workspace whose `lib/build.lua` declares `text` (a `cp`), built
  at the root; `buildl log text` run from `lib/` exits 0 and its header names `//lib:text`.
- `log_rejects_a_bad_label_with_exit_2` — `buildl log //lib` exits 2, stderr starting
  `error: invalid value '//lib' for '<TARGET>'`.

**Done when:** `cargo test -p buildl-cli --test cli` passes and the Definition of Done is green.
Commit `test(buildl-cli): what the log command prints and how it ends`.

## Risks

- **Pinned help is byte-exact**; the help lines here are written once and pinned from the binary.
- **Two writers, one order.** With no flag, stdout and stderr are separate streams, so a terminal
  may interleave them; the tests read each stream on its own, as a pipe would.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
