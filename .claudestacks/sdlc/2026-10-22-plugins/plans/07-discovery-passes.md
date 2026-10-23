---
status: approved
created: 2026-10-22
depends-on: [02, 06]
built-on: 5fa60e343706
---

# Discovery Passes Implementation Plan

**Goal:** Every command that loads build files evaluates them a second time with the discovery results its row of spec §6 names.

**Architecture:** `buildl-core` gains a crate-private `discovery` module (a helper several
pipeline functions use, like `suggest`, not a phase): reading the last result of each discovery
target from the log and the cas (`RunHistory`, `ContentStore`, as `pipeline::log` does,
`pipeline/log.rs:36`), the second load, and the pass-1/pass-2 consistency check. With no discovery
target declared, every command takes one pass exactly as today. `build` and `test` first run the
discovery targets through the existing build path, narrowed to them, then load again. `plan`,
`verify` and `gc` use last results; `check`, `graph`, `query` and `doctor` use last results read
without the state lock. The `buildl` facade opens the log and cas read-only for those four, as
`Workspace::log` does (`workspace.rs:252`); the CLI prints the notes.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `buildl-cli`, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §6 ("Where the result comes from", the per-command table, "Decision
recorded", pass-2 consistency), §7 (`DiscoveryChanged`), §9 (core two-pass rows, `gc` keeping
discovered rows, e2e discovery building then read by `check`).

---

## Files that change

```
crates/buildl-core/src/history/last_success.rs  — [create] newest logged run of a label that succeeded with one output → its manifest digest
crates/buildl-core/src/history/mod.rs           — [modify] declare it
crates/buildl-core/src/discovery/mod.rs         — [create] export-only
crates/buildl-core/src/discovery/results.rs     — [create] `results(history, store, labels) -> (Discovered, Vec<Label> missing)`
crates/buildl-core/src/discovery/passes.rs      — [create] `second_pass(source, dispatcher, entry, pass_one, discovered)` (Load #2 + consistency) and `load_twice` (Load #1 then `second_pass`)
crates/buildl-core/tests/flows/golden/plan.json — [modify] the plan JSON gains `stale_discovery`
crates/buildl-core/src/lib.rs                   — [modify] `mod discovery`
crates/buildl-core/src/error.rs                 — [modify] DiscoveryChanged { label, provenance }
crates/buildl-core/src/types/loaded.rs          — [create] Loaded { declarations, missing: Vec<Label> } returned to the facade
crates/buildl-core/src/types/plan.rs            — [modify] Plan gains `stale_discovery: Vec<Label>`
crates/buildl-core/src/pipeline/driver.rs       — [modify] check/graph/query free fns and Pipeline methods use the passes; build/test run discovery first
crates/buildl-core/src/pipeline/doctor.rs       — [modify] passes with last results
crates/buildl-core/src/pipeline/gc.rs           — [modify] passes with last results (rows of discovered targets kept)
crates/buildl/src/workspace.rs                  — [modify] check/graph/query/doctor open log + cas read-only, no lock; return `missing`
crates/buildl/src/local_ports.rs                — [modify] the read-only log/cas wiring for those four
crates/buildl/src/report/plan_text.rs           — [modify] note for each stale discovery target
crates/buildl-cli/src/run.rs                    — [modify] stderr note per missing result for check/graph/query/doctor
crates/buildl-cli/src/exit.rs                   — [modify] DiscoveryChanged → 5
crates/buildl-core/tests/flows/discovery.rs     — [create] two-pass flows per command group
crates/buildl-core/tests/flows/main.rs          — [modify] `mod discovery;`
crates/buildl/tests/e2e/discovery.rs            — [create] end to end
crates/buildl/tests/fixtures/discovery/…        — [create] workspace: a discovery target `cp`s a checked-in list to `$out`; build.lua declares one target per entry
crates/buildl/tests/e2e/main.rs                 — [modify] `mod discovery;`
crates/buildl-cli/tests/cli/output.rs           — [modify] the stderr note
```

## Order of work

### Task 1 — Reading a discovery target's last result

**Files:** `history/last_success.rs`, `history/mod.rs`, `discovery/mod.rs`,
`discovery/results.rs`, `lib.rs`.

**Change:** `last_success(lines, label) -> Option<Digest>`: the newest event for `label` whose
outcome succeeded and holds exactly one produced output (`types/outcome.rs:126-127`), that
output's manifest digest; damaged lines skipped as `last_run` skips them
(`history/last_run.rs:39,73`). `results(history, store, labels)`: for each label, the manifest
blob read from the store must be `OutputManifest::File`; its file blob's bytes go into
`Discovered`. A log that cannot be read, a damaged line, a missing blob, or a `Tree` manifest each
count as no result for that label, never an error; labels without a result are returned as
`missing`, in label order.

**Proof:** written first, seen failing, then passing (unit, fake history/store) —
- `last_success_skips_a_newer_failure`, `last_success_ignores_other_labels`
- `results_reads_the_file_bytes`
- `an_unreadable_log_is_no_result`, `a_missing_blob_is_no_result`, `a_tree_manifest_is_no_result`
- `missing_lists_labels_without_a_result_in_label_order`

**Done when:** `cargo test -p buildl-core` passes; `cargo make dod` green. Commit
`feat(buildl-core): read the last result of a discovery target`.

### Task 2 — Two passes for commands that run nothing

**Files:** `discovery/passes.rs`, `types/loaded.rs`, `types/mod.rs`, `error.rs`,
`pipeline/driver.rs` (check/graph/query free fns and the `plan`/`verify` methods),
`pipeline/doctor.rs`, `pipeline/gc.rs`, `types/plan.rs`, `buildl-cli/src/exit.rs`,
`tests/flows/discovery.rs`, `tests/flows/main.rs`.

**Change:** `second_pass(…, pass_one, discovered)` runs Load #2 with `discovered` and compares
the two passes' discovery declarations (label, fields and origin): any added, removed or changed
→ `DiscoveryChanged` naming the first label in order (exit 5). `load_twice` runs Load #1 with
`Discovered::empty()`; when it declares no discovery target it returns those declarations (one
pass); otherwise it asks `results_for(labels)` for the map and calls `second_pass`. A caller that
has already run Load #1 (Task 3) calls `second_pass` directly, so pass 1 is never evaluated twice. `check` evaluates pass 1 once and keeps its existing
double load and comparison (`driver.rs:290-298`) for pass 2. `graph`, `query`, `doctor`, `gc`,
`plan` and `verify` use `load_twice` with last results; their signatures gain a `RunHistory` and a
`ContentStore` (the free fns) or use the `Pipeline`'s. `plan` also plans the discovery targets
(scope `Scope::Targets(labels)`, `Purpose::Build`) and records the ones not clean in
`Plan::stale_discovery`, serialized as `stale_discovery` (golden `plan.json` updated).

**Proof:** written first, seen failing, then passing (core flows, fakes) —
- `no_discovery_means_one_load` — the fake source counts evaluations: unchanged from today
- `graph_includes_targets_declared_from_last_results`
- `check_loads_pass_two_twice_and_pass_one_once`
- `a_discovery_added_in_pass_two_is_refused`, `a_discovery_changed_in_pass_two_is_refused`
- `missing_results_are_reported_and_declare_nothing_more`
- `gc_keeps_rows_of_targets_declared_from_discovery`
- `plan_lists_a_dirty_discovery_target_as_stale`
- `doctor_wants_include_discovered_targets`
- `verify_checks_targets_declared_from_last_results`
- `exit.rs` unit `discovery_changed_exits_5`

**Done when:** `cargo test -p buildl-core -p buildl-cli` pass; `cargo make dod` green. Commit
`feat(buildl-core): declare from the last discovery results`.

### Task 3 — `build` and `test` run discovery first

**Files:** `pipeline/driver.rs` (`build` :162, `test` :218), `tests/flows/discovery.rs`.

**Change:** when pass 1 declares discovery targets, `build` and `test` resolve, narrow to
`Scope::Targets(discovery labels)` with `Purpose::Build`, plan, negotiate (with the plan's
attribution, plan 02), execute and record, as a build of those targets does today. If any of
those fails, the command returns that run's outcomes and goes no further. Otherwise results are
read (Task 1) and the command calls `second_pass` with the pass-1 declarations it already has,
then runs its own resolve, narrow, plan, negotiate, execute and record. Three loads never happen:
the flow is Load #1, the discovery run, Load #2. The approver is asked twice; an approver that remembers a yes
asks only about new wants (existing behaviour of `InteractiveApprover`).

**Proof:** written first, seen failing, then passing (core flows) —
- `build_runs_discovery_then_builds_discovered_targets`
- `a_second_build_is_all_cached_including_discovery`
- `a_failed_discovery_stops_the_build_with_its_failure`
- `the_approver_is_asked_for_discovery_then_for_the_build`
- `test_runs_discovery_then_the_discovered_tests`
- `build_evaluates_each_build_file_twice_not_three_times` — the fake source counts evaluations

**Done when:** `cargo test -p buildl-core` passes; `cargo make dod` green. Commit
`feat(buildl-core): build and test run discovery targets first`.

### Task 4 — The facade, the notes, end to end

**Files:** `buildl/src/workspace.rs`, `buildl/src/local_ports.rs`, `buildl/src/report/plan_text.rs`,
`buildl-cli/src/run.rs`, `buildl/tests/e2e/discovery.rs`, `buildl/tests/fixtures/discovery/…`,
`buildl/tests/e2e/main.rs`, `buildl-cli/tests/cli/output.rs`.

**Change:** `Workspace::check`, `graph`, `query` and `doctor` open the log and cas read-only
without the state lock (as `Workspace::log`, `workspace.rs:252`) and return the `missing` labels
with their result; a state directory that does not exist yet means no results. The CLI prints
`//svc:pkgs has not run; run buildl build` on stderr per missing label. `plan_text` prints, per
stale discovery target, `//svc:pkgs is not up to date; targets declared from it may change`.

**Proof:** written first, seen failing, then passing —
- e2e `a_fresh_workspace_graph_has_no_discovered_targets_and_notes_it`
- e2e `build_then_graph_shows_discovered_targets`
- e2e `editing_the_list_then_build_adds_a_target`
- e2e `gc_after_build_keeps_discovered_targets_cached` — a build after `gc` is all cached
- e2e `check_works_with_a_damaged_log`
- CLI `check_prints_the_not_run_note_on_stderr`
- `plan_text` unit `plan_text_notes_a_stale_discovery`

**Done when:** `cargo test --workspace` passes; `cargo make dod` and `cargo deny check` green.
Commit `feat(buildl): discovery results for every command`.

## Risks

- Most risky: Task 3, two full plan/execute cycles in one `build`; summary lines and exit codes
  must account for both runs. The proof's second-build and failure cases are the guard.
- `check`, `graph`, `query` and `doctor` now read the log and the cas: a concurrent build may be
  appending; the newest complete line wins, a cut last line is skipped as `log` skips it.
- `test`'s summary counts tests only (`Summary.tests`); discovery outcomes count in `Summary.all`.
- Chosen not to do: running discovery in commands other than `build` and `test` (spec §6).

## Proof

`cargo make dod` and `cargo deny check` green; on the discovery fixture, `graph` on a fresh
workspace notes the missing result, `build` builds discovery then the discovered targets, `graph`
then shows them, `gc` keeps them cached, and `check` survives a damaged log.
