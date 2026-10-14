---
status: done
created: 2026-10-14
depends-on: [09]
---

# buildl End-to-End Tests Implementation Plan

**Goal:** Real builds of checked-in fixture workspaces, run through the `Workspace` facade alone, prove every end-to-end scenario the design names for the `buildl` crate.

**Architecture:** One integration test binary, `e2e` (`crates/buildl/tests/e2e/main.rs`, found by Cargo with no manifest change). `common.rs` holds `Fixture`, a temporary copy of a fixture under `crates/buildl/tests/fixtures/<name>/` that sets its own executable bits and runs builds through `buildl::Workspace`, and `Snapshot`, which holds what two builds must agree on byte for byte: `out/**` with the executable bit, `cache.json`, `graph.json` and every file of `cas/`. The log and the stat cache are never compared as bytes. There are three fixtures (`chain`, `twins`, `failing`) and one golden file. No test sets an environment variable, changes the current directory, waits on a clock or depends on the order a directory is listed in. Every tool is a system tool, run by argument vector.

**Tech Stack:** Rust 2024; the public API of `buildl` and `buildl::buildl_core`; `tempfile` (already a normal dependency of `buildl`); the system tools `cp`, `sort`, `false` and `/bin/sh`.

**Content authority:** spec §12.1, §12.3, §12.4, §10, §17; the intent's Desired outcome.

**Shape of this record:** this plan is a short record. The code is landed from the proven scratch tree, file for file, as listed below. Each task adds the listed files and tests, then runs the gate (`cargo fmt --all -- --check`, `cargo make dod`, `cargo make deny`) and commits.

**Precondition from another plan:** `ThreadPool` (plan 07, `crates/buildl/src/dispatch/thread_pool.rs`) must hand a task to a worker only when a worker is free. Without that, `a_failure_stops_the_build_by_default` fails on every run with `Summary { failed: 1, skipped: 3, built: 1, cached: 0 }`. The fix is recorded in the hand-off report of this plan.

## File structure

```
crates/buildl/tests/e2e/main.rs                  — [create] test binary root: modules, lint expectation
crates/buildl/tests/e2e/common.rs                — [create] Fixture, Snapshot, ended
crates/buildl/tests/e2e/facade.rs                — [create]
crates/buildl/tests/e2e/identical.rs             — [create]
crates/buildl/tests/e2e/incremental.rs           — [create]
crates/buildl/tests/e2e/named.rs                 — [create]
crates/buildl/tests/e2e/failure.rs               — [create]
crates/buildl/tests/e2e/recovery.rs              — [create]
crates/buildl/tests/e2e/golden/chain.graph.json  — [create] 1,615 bytes + newline; sha256 of content ff953b79…78c10dac
crates/buildl/tests/fixtures/chain/{buildl.toml, build.lua, greet.sh, footer.txt, lib/build.lua, lib/words/a.txt, lib/words/b.txt, lib/assets/one.txt, lib/assets/sub/two.txt}   — [create]
crates/buildl/tests/fixtures/twins/{buildl.toml, build.lua, echo.sh, echo.txt}   — [create]
crates/buildl/tests/fixtures/failing/{buildl.toml, build.lua, x.txt}             — [create]
```

## Tasks

| # | Title | Files | Tests added | Commit |
|---|---|---|---|---|
| 1 | The `chain` fixture, and a harness that copies it | `e2e/main.rs`, `e2e/common.rs`, `e2e/facade.rs`, `fixtures/chain/*` | `opening_a_workspace_writes_nothing` | `test(buildl): copy the chain fixture and open it through the facade` |
| 2 | The graph a build records, held to its golden file | `e2e/main.rs`, `e2e/common.rs`, `e2e/identical.rs`, `e2e/golden/chain.graph.json` | `the_graph_file_matches_its_golden` | `test(buildl): hold the graph a build records to its golden file` |
| 3 | The `twins` fixture: one set of bytes, placed plain and executable | `e2e/common.rs`, `e2e/identical.rs`, `fixtures/twins/*` | `equal_bytes_placed_plain_and_executable_keep_one_blob_and_its_twin` | `test(buildl): keep one blob and its twin for bytes placed in two modes` |
| 4 | Two clean builds in two directories are byte-identical | `e2e/common.rs`, `e2e/identical.rs` | `two_clean_builds_in_two_directories_are_byte_identical` | `test(buildl): build each fixture twice in two directories and compare the bytes` |
| 5 | Two logs agree once their times are dropped | `e2e/common.rs`, `e2e/identical.rs` | `two_logs_agree_once_times_are_dropped` | `test(buildl): compare the logs of two builds once their times are dropped` |
| 6 | A second build runs nothing, and puts deleted outputs back | `e2e/main.rs`, `e2e/common.rs`, `e2e/incremental.rs` | `a_second_build_runs_nothing`, `deleted_outputs_are_put_back_without_running` | `test(buildl): a second build runs nothing and puts deleted outputs back` |
| 7 | An edit or a setting rebuilds only what it reaches | `e2e/common.rs`, `e2e/incremental.rs` | `an_edited_input_rebuilds_its_dependents_only`, `an_edit_that_leaves_the_output_unchanged_stops_there`, `a_setting_override_rebuilds_what_names_it` | `test(buildl): rebuild only what an edit or a setting reaches` |
| 8 | A named target builds what it needs and nothing else | `e2e/main.rs`, `e2e/common.rs`, `e2e/named.rs` | `a_named_target_builds_what_it_needs_and_nothing_else`, `a_named_alias_builds_the_target_behind_it`, `a_full_build_after_a_named_one_matches_a_clean_full_build` | `test(buildl): build a named target and what it needs, and nothing else` |
| 9 | The `failing` fixture: one failure, with and without keep-going | `e2e/main.rs`, `e2e/failure.rs`, `fixtures/failing/*` | `a_failure_stops_the_build_by_default`, `keep_going_builds_what_does_not_depend_on_the_failure`, `a_failed_target_runs_again_and_leaves_no_row` | `test(buildl): stop at a failure, or keep going, and never cache one` |
| 10 | Content with no row, and an output left half-written | `e2e/main.rs`, `e2e/recovery.rs` | `blobs_without_rows_are_not_trusted_and_the_build_finishes`, `a_partial_output_no_row_names_is_cleared_and_built_again` | `test(buildl): trust no blob without its row, and clear a half-written output` |
| 11 | Rows left in the journal, and staging files left behind | `e2e/recovery.rs` | `rows_left_in_the_journal_are_folded_by_the_next_run`, `leftover_staging_files_are_swept_and_change_nothing` | `test(buildl): fold rows left in the journal and sweep staging files` |
| 12 | A corrupt `cache.json` is refused | `e2e/main.rs`, `e2e/common.rs`, `e2e/recovery.rs` | `a_corrupt_cache_is_refused` (three cases) | `test(buildl): refuse a cache.json that is not what buildl wrote` |
| 13 | What the facade refuses before a build begins | `e2e/facade.rs` | `a_relative_target_resolves_from_where_it_was_opened`, `a_workspace_another_buildl_holds_is_refused`, `build_refuses_a_strategy_it_cannot_run_and_plan_does_not`, `a_state_directory_outside_the_ceiling_is_refused_before_it_is_made` | `test(buildl): refuse a busy workspace, a strategy it cannot run and a state directory outside the ceiling` |

All paths in the table are under `crates/buildl/tests/`. The tree as each task leaves it was built and checked in task order (fmt, clippy `-D warnings`, `e2e` green after every task). `main.rs` gains `clippy::panic` in its `#![expect]` only in task 12, with the first `panic!`. Written earlier, clippy stops with `error: this lint expectation is unfulfilled`.

## Verification summary (plan-level)

- **Gate (scratch tree, final state):** `cargo fmt --all -- --check` exit 0. `cargo make dod` exit 0, with `guard-adapter-isolation: OK (25 edges)`, `buildl` unit `248 passed` (247 plus the pool test) and `e2e` `25 passed`. `cargo make deny` exit 0.
- **Steadiness:** `e2e` ran with `--test-threads 1` (25 passed) and with `--test-threads 64` (25 passed). Earlier, 25 consecutive default runs all passed.
- **All 25 tests of spec §12.4 are present under the spec's names** (the table above, rows 1–25 in the spec's order map one to one).
- **Each test was seen to fail:** 19 one-line breaks of the crate, each run against the full binary; every test fails under at least one break (e.g. `put` removing the twin, `graph.json` with a newline, the root's path in a tool fingerprint → `.buildl/cache.json differs`, the dependency's key folded instead of its output, the pool sending tasks eagerly).
- **No absolute path in a compared file:** two copies built in two directories compare equal. A search of built workspaces for the root path, `/private/`, `/var/folders`, `/tmp/`, `/Users/` and a JSON string starting with a single `/` found nothing.
- **Left to the CLI's tests (spec §12.5):** `an_env_value_change_rebuilds_the_target_that_reads_it` and `a_value_that_is_not_unicode_fails_the_plan`, which need a changed environment value.
- **Linux:** not run here. The `ubuntu-latest` leg is the first run there (spec §17).
