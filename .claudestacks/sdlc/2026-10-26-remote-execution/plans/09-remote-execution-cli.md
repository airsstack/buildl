---
status: approved
created: 2026-10-26
depends-on: [01, 05, 06, 08]
built-on: 0e431e5caec9
---

# Remote Execution CLI Implementation Plan

**Goal:** `buildl build`, `test` and `watch` execute on the server when the switches of spec §2 turn remote execution on.

**Architecture:** The switches resolve in `buildl-cli/src/remote_switches.rs`, where milestone
6's upload and strict switches already resolve into a `RemotePolicy` (`resolve`, `:100`). The
resolution gains a third switch with four sources and the precedence of spec §2, the jobs
count (`[remote] jobs` or `--remote-jobs`), and the local count, which `buildl`'s `Workspace`
reports as its resolved `[execution] workers` (the number `ThreadPool::new` resolves today,
`dispatch/thread_pool.rs:32-35`). The policy reaches the ports through `RunOptions::with_remote`
(`buildl/src/workspace.rs:64`) as today. `Wiring::ports` (`local_ports.rs:440`) builds the
dispatcher wide enough for the remote jobs: the larger of `workers` and `jobs` when execution is
on (spec §7). `watch` takes `build`'s flags (`cli.rs:68`), so it gains the switches with no code
of its own.

**Tech Stack:** Rust, clap, `buildl-cli`, `buildl`, `cargo make dod`.

**Content authority:** spec §2 (switches, precedence, usage errors, the variable without a table), §3 (`--remote-jobs`), §7 (dispatcher width).

---

## Files that change

```
crates/buildl-cli/src/cli.rs             — [modify] RemoteArgs (:172) gains remote_execute, no_remote_execute, remote_jobs; parse tests (:624-670)
crates/buildl-cli/src/remote_switches.rs — [modify] RemoteSwitches (:70) gains execute; resolve (:100) gains execution, jobs, local; tests (:155)
crates/buildl-cli/src/main.rs            — [modify] reads BUILDL_REMOTE_EXECUTE beside :116-117
crates/buildl-cli/src/run.rs             — [modify] the three resolve calls (:382, :414, :461) pass the workspace's remote settings and local count
crates/buildl-core/src/error.rs          — [modify] RemoteVariable (:993) gains Execute (BUILDL_REMOTE_EXECUTE); RemoteFlagConflict (~:1009) gains ExecuteBothWays, NoRemoteWithExecute, JobsWithoutExecution, ExecutionWithoutRemote; Display; tests (:1834-1850)
crates/buildl/src/workspace.rs           — [modify] Workspace gains remote_settings() -> Option<&RemoteSettings> and local_workers(): the resolved [execution] workers; remote_policy (:167) unchanged
crates/buildl/src/dispatch/thread_pool.rs — [modify] ThreadPool gains width_for(Workers) (the resolution new() does) and wide(workers, jobs); tests (:179)
crates/buildl/src/local_ports.rs         — [modify] dispatcher (:416) takes the policy: wide when execution is on; ports (:440, :513) passes it
crates/buildl-cli/tests/cli/remote.rs    — [modify] end-to-end tests over the test server (helpers :32-85)
```

## Order of work

### Task 1 — The switches

**Files:** `cli.rs`, `remote_switches.rs`, `main.rs`, `error.rs`.

**Change:** `--remote-execute`, `--no-remote-execute`, `--remote-jobs N` (parsed by plan 01's
`RemoteJobs::parse`) on `RemoteArgs`; `BUILDL_REMOTE_EXECUTE` read in `main.rs`. `resolve`
decides execution with the precedence
`--no-remote > --no-remote-execute / --remote-execute > BUILDL_REMOTE_EXECUTE > [remote] execute`,
and refuses, as `Error::RemoteFlags` (exit 2, `exit.rs:104-107`): `--remote-execute` with
`--no-remote` (`NoRemoteWithExecute`) or with `--no-remote-execute` (`ExecuteBothWays`);
`--remote-execute`, `--no-remote-execute` or `--remote-jobs` with no `[remote]` table
(`ExecutionWithoutRemote`, whose text names those flags; the milestone 6 `SwitchWithoutRemote`,
`error.rs:1028-1031`, names `--remote-upload` and `--remote-strict` and is left as it is);
`--remote-jobs` when execution ends off (`JobsWithoutExecution`). `resolve` reads `[remote]
execute` and `jobs` from `Workspace::remote_settings()`, passed by `run.rs`. The variable
is `1`/`0` or `Error::RemoteSwitch` (`error.rs:430`), and is not read under `--no-remote`, when a
flag decides, or when the workspace has no `[remote]` table (spec §2). With execution on, the
policy carries `RemoteExecution::On { jobs, local }`: `jobs` from the flag, else from
`[remote] jobs`; `local` from the workspace.

**Proof:** written first, seen failing, then passing (`remote_switches.rs` tests, beside `:185-337`) —
- `the_manifest_turns_execution_on` and `the_manifest_default_is_off`
- `the_variable_overrides_the_manifest` — `execute = true`, `BUILDL_REMOTE_EXECUTE=0`: off
- `the_flag_overrides_the_variable` — variable `0`, `--remote-execute`: on
- `no_remote_execute_keeps_the_cache` — `execute = true`, `--no-remote-execute`: reading on, execution off
- `no_remote_turns_execution_off`
- `remote_execute_with_no_remote_is_refused`, `remote_execute_with_no_remote_execute_is_refused`
- `an_execution_flag_without_a_remote_table_is_refused`
- `the_variable_without_a_remote_table_is_not_read` — `BUILDL_REMOTE_EXECUTE=yes`, no table: no error, off
- `remote_jobs_without_execution_is_refused`
- `remote_jobs_overrides_the_manifest` and `jobs_default_to_the_manifest`
- `a_bad_execute_variable_is_refused_naming_it`
- `cli.rs`: `build_test_and_watch_take_the_execution_flags`

**Done when:** `cargo test -p buildl-cli` passes; `cargo make dod` green. Commit
`feat(buildl-cli): switches for remote execution`.

### Task 2 — Wide enough for the remote jobs

**Files:** `thread_pool.rs`, `local_ports.rs`, `workspace.rs`, `run.rs`.

**Change:** `ThreadPool::width_for(workers)` returns what `new` resolves (`:32-35`);
`Workspace::local_workers()` returns it for the manifest's `[execution] workers`, and `run.rs`
passes it to `resolve`. `Wiring::dispatcher` takes the run's policy: `ThreadPool::wide(workers,
jobs)` with execution on, whose limit is the larger of the two; `ThreadPool::new(workers)`
otherwise, as today. The commands that build ports with `RemotePolicy::OFF` (`local_ports.rs:942`,
`:964`, `:1005`) are unchanged.

**Proof:** in `thread_pool.rs` tests (`:179`) and `local_ports.rs` tests —
- `a_wide_pool_runs_as_many_tasks_at_once_as_its_larger_count` — `wide(2, 6)`: peak 6, beside `no_more_tasks_run_at_once_than_there_are_workers` (`:262`)
- `a_wide_pool_with_fewer_jobs_than_workers_keeps_the_workers` — `wide(4, 2)`: peak 4
- `width_for_matches_new`
- `the_dispatcher_is_wide_only_when_executing`

**Done when:** `cargo test -p buildl dispatch` passes; `cargo make dod` green. Commit
`feat(buildl): a dispatcher wide enough for remote jobs`.

### Task 3 — End to end over the test server

**Files:** `crates/buildl-cli/tests/cli/remote.rs`.

**Change:** none to production code; binary tests using the test-server helpers (`:41-59`).

**Proof:** beside `build_reports_fetched_targets_in_the_summary` (`:102`) —
- `remote_execute_runs_every_target_on_the_server` — a fresh workspace, `--remote-execute`: the summary counts every target `remote`, nothing ran locally, `out/` holds the outputs
- `a_second_checkout_fetches_what_the_server_ran` — then a fresh checkout with no flags: every target `fetched`, outputs byte-identical
- `the_manifest_switch_runs_remotely` — `execute = true` in `buildl.toml`, no flag
- `remote_execution_under_bare_exits_as_usage_from_each_source` — beside `uploading_under_bare_is_refused_with_the_usage_exit` (`:188`): `strategy = "bare"` with `[remote] execute = true`, with `BUILDL_REMOTE_EXECUTE=1`, and with `--remote-execute`, each exit 2
- `a_bare_manifest_that_executes_still_checks_plans_and_doctors` — `strategy = "bare"`, `execute = true`: `check`, `plan` and `doctor` exit 0 (spec §2)
- `every_execution_usage_error_exits_as_usage` — the five refusals of Task 1, run through the binary, each exit 2 with its message
- `a_server_that_cannot_execute_warns_and_builds_locally` — `exec: Disabled`: one warning naming `execute`, every target `built`
- `a_strict_execute_failure_exits_as_infrastructure` — beside `:161`: exit 3
- `a_remote_action_failure_fails_the_build_as_an_action_failure` — a command that exits 1: exit as for a local action failure, `buildl log` shows what it printed
- `test_runs_tests_remotely` — `buildl test --remote-execute`: `tests: N passed (N remote)`
- `watch_takes_remote_execute` — `buildl watch --remote-execute` parses and runs its first cycle remotely (as `tests/cli/watch.rs` drives watch)

**Done when:** `cargo test -p buildl-cli --test cli remote` passes; `cargo make dod` and
`cargo deny check` green. Commit `test(buildl-cli): remote execution end to end`.

## Risks

- **Most risky step:** Task 3's `watch` test: watch runs until Ctrl-C; reuse the way
  `tests/cli/watch.rs` stops it rather than adding a new mechanism.
- New tests against a closed or failing server set `retries = 0` in their `buildl.toml`, as
  plan 03 Task 2 does for the milestone 6 ones, so no test waits through the retry schedule.
- `resolve`'s signature grows; keep its inputs as one struct if clippy's argument-count lint fires.

## Proof

`cargo make dod`, `cargo deny check`; every `tests/cli/remote.rs` test, old and new, passes.
