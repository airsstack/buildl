---
status: done
created: 2026-10-22
depends-on: [01, 03]
built-on: 5d10ef9b2f2e
---

# Doctor Implementation Plan

**Goal:** `buildl doctor` prints a workspace's ceiling, approver, run settings and wants, each want marked as inside the ceiling or not and approved or not.

**Architecture:** The core computes the wants without refusing: `pipeline::doctor`, a free
function over `DeclarationSource`, `Dispatcher` and `Manifest` (spec §6, §8), the shape of
`pipeline::check`/`graph`/`query` (`pipeline/driver.rs:269-306`). It reuses plan 01's
`grants::sites` and a `wanted` constructor split out of `plan/authority.rs`'s `check`, so
doctor and plan cannot disagree. `buildl`'s `Workspace::doctor` adds which wants are approved,
reading the approvals file under `interactive` (plan 03). The CLI renders the text (spec §8
`buildl-cli` row) and exits 0 for reported problems.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `buildl-cli` (clap), `cargo make dod`,
`cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §6, §8, §9 (CLI `doctor` row).

---

## Files that change

```
crates/buildl-core/src/grants/wanted.rs     — [modify] `wanted(graph, sites, workspace)`: the WantedSet, split out of check
crates/buildl-core/src/grants/outside.rs    — [create] `outside(wanted, workspace)`: the wants the ceiling does not allow
crates/buildl-core/src/grants/mod.rs        — [modify] declare and use
crates/buildl-core/src/plan/authority.rs    — [modify] check builds its result with `wanted`
crates/buildl-core/src/types/checkup.rs     — [create] Checkup { workspace, wanted, outside }
crates/buildl-core/src/types/mod.rs         — [modify] declare and re-export
crates/buildl-core/src/pipeline/doctor.rs   — [create] pipeline::doctor
crates/buildl-core/src/pipeline/mod.rs      — [modify] `pub mod doctor;`, `pub use`
crates/buildl-core/src/lib.rs               — [modify] re-exports; Status paragraph
crates/expected-module-edges.txt            — [modify] pipeline -> placement
crates/buildl-core/tests/flows/doctor.rs    — [create] doctor flow tests
crates/buildl-core/tests/flows/main.rs      — [modify] `mod doctor;`
crates/buildl/src/workspace.rs              — [modify] Workspace::doctor and the Doctor value
crates/buildl/src/local_ports.rs            — [modify] Wiring::manifest and Wiring::approvals (amended 2026-10-22)
crates/buildl/src/lib.rs                    — [modify] re-export Doctor
crates/buildl/tests/e2e/doctor.rs           — [create] doctor through Workspace
crates/buildl/tests/e2e/main.rs             — [modify] `mod doctor;`
crates/buildl-cli/src/cli.rs                — [modify] Command::Doctor
crates/buildl-cli/src/run.rs                — [modify] dispatch and the `doctor` handler
crates/buildl-cli/src/doctor_text.rs        — [create] the text of spec §6
crates/buildl-cli/src/main.rs               — [modify] `mod doctor_text;`; the command list in the crate docs
crates/buildl-cli/tests/cli/output.rs       — [modify] doctor's full output
crates/buildl-cli/tests/cli/exit_codes.rs   — [modify] doctor's exit codes
crates/buildl-cli/tests/cli/arguments.rs    — [modify] the help goldens list doctor (amended 2026-10-22)
```

## Order of work

### Task 1 — The core `doctor`

**Files:** `grants/wanted.rs`, `grants/outside.rs`, `grants/mod.rs`, `plan/authority.rs`,
`types/checkup.rs`, `types/mod.rs`, `pipeline/doctor.rs`, `pipeline/mod.rs`, `lib.rs`,
`crates/expected-module-edges.txt`, `tests/flows/doctor.rs`, `tests/flows/main.rs`.

**Change:**
- `grants::wanted(graph, sites, workspace) -> WantedSet` — the `WantedSet` literal now at the
  end of `check` (`plan/authority.rs:116-126`: programs and env from the sites, write =
  `{out, state}`, network = labels of targets declaring it). `check` returns
  `Ok(wanted(graph, sites, workspace))`; its refusals are unchanged.
- `grants::outside(wanted, workspace) -> WantedSet` — the programs not in `ceiling.run`, the
  variables not in `ceiling.env`, the directories no `ceiling.write` entry contains (the same
  tests `check` uses, `:82-114`); network is never outside (a ceiling has no network entry).
- `Checkup { workspace: Workspace, wanted: WantedSet, outside: WantedSet }` with a constructor
  and accessors; test `a_checkup_hands_back_what_it_was_built_from`.
- `pipeline::doctor<S, D, M>(source: &S, dispatcher: &mut D, manifest: &M, entry) ->
  Result<Checkup>`: loads once, resolves, reads the workspace, takes `graph.settings()` (the
  defaults; `plan::settings::effective` is private to Plan and with no override returns the
  same map — amended 2026-10-22), `Layout::of(&graph, &workspace.out)?`, `sites`, `wanted`,
  `outside`. No lock, no write, no hashing, no tool lookup. Errors: load, resolve, the
  manifest's, `OutputConflict`; never `Refused`. Its flow goes through ports, so it is tested in
  `tests/flows/doctor.rs` (`lib.rs` rule 5, `:53-56`); it carries a `#[cfg(test)]` module only
  for a pure helper it splits out.
- Edges: `pipeline -> placement`.

**Proof:** written first, seen failing, then passing —
- `grants/outside.rs`: `a_program_outside_proc_run_is_outside`,
  `a_variable_outside_env_read_is_outside`, `a_write_directory_outside_fs_write_is_outside`,
  `the_network_is_never_outside`, `a_set_inside_the_ceiling_has_nothing_outside`.
- `plan/authority.rs`: existing `check` tests pass unchanged.
- `tests/flows/doctor.rs`: `doctor_reports_wants_outside_the_ceiling_without_refusing`,
  `doctor_uses_the_whole_graph_and_default_settings`, `doctor_lists_a_program_named_through_in`,
  `doctor_leaves_out_a_built_program`, `doctor_reports_an_output_conflict_as_an_error`,
  `doctor_hashes_nothing_and_looks_up_no_tool` (built from source, dispatcher and manifest
  only — the signature proves no other port is reached).

**Done when:** `cargo test -p buildl-core` (lib and `--test flows`) passes and
`cargo make dod` is green. Commit `feat(buildl-core): report a workspace's wants without
refusing them`.

### Task 2 — `Workspace::doctor`

**Files:** `buildl/src/workspace.rs`, `buildl/src/local_ports.rs`, `buildl/src/lib.rs`,
`tests/e2e/doctor.rs`, `tests/e2e/main.rs`.

**Change:** `Doctor` — the `Checkup`, the workspace root (absolute) and `unapproved:
WantedSet`. `Workspace::doctor(&self) -> Result<Doctor>` takes no lock; it calls
`pipeline::doctor` with the wiring's source, dispatcher and manifest, then sets `unapproved`
from the mode: `manifest` → empty; `deny` → the whole wanted set; `interactive` →
`wanted.minus(&approvals.read()?)` (plan 03's `Approvals` and `DataDir`, honouring
`with_data_dir`). Errors are the approvals file's, as in plan 03 (6, or 3). `Wiring` gains
`manifest()` and `approvals()` for it (amended 2026-10-22).

**Proof:** written first, seen failing, then passing (`tests/e2e/doctor.rs`) —
- `doctor_writes_nothing` — the workspace tree, including `.buildl/`, is byte-identical before
  and after, and `.buildl/` is not made when absent
- `doctor_under_manifest_approves_everything`, `doctor_under_deny_approves_nothing`
- `doctor_under_interactive_reads_the_remembered_approvals` — after an approved build,
  approved wants are not in `unapproved`, a newly added want is
- `doctor_with_a_corrupt_approvals_file_is_corrupt_state`

**Done when:** `cargo test -p buildl --test e2e` passes and `cargo make dod` is green. Commit
`feat(buildl): add Workspace::doctor`.

### Task 3 — The `doctor` command

**Files:** `cli.rs`, `run.rs`, `doctor_text.rs`, `main.rs`, `tests/cli/output.rs`,
`tests/cli/exit_codes.rs`, `tests/cli/arguments.rs`; `buildl/src/workspace.rs` gains a public
`Doctor::new` so the text's unit tests can build a `Doctor` (amended 2026-10-22).

**Change:**
- `Command::Doctor` (`cli.rs:55`), with a help line: "Print the workspace's ceiling, approver,
  run settings and what its build wants". `run.rs` dispatches it to `doctor`, which writes
  `doctor_text` to stdout and returns `Exit::Success`.
- `doctor_text(&Doctor) -> String`, the layout of spec §6:
  - `workspace  <absolute root>`; `approver   <mode>`;
    `execution  workers <n>[ (one per core)], timeout <t>, strategy <s>` — the values'
    own `Display` (`Workers`, `types/workers.rs:90`, shows `0` for per-core; `Timeout`,
    `types/timeout.rs:116`), with ` (one per core)` after `0`.
  - `ceiling`, then `proc.run`, `env.read`, `fs.read`, `fs.write`, each list joined by `, `
    (`-` when empty).
  - `wants` with column heads `ceiling` and `approved`, then one row per want in
    `WantedSet` order — `run <program>`, `read <variable>`, `write <directory>`,
    `network <label>`; ceiling `allowed` or `REFUSED` (`-` for network); approved `-` when
    outside the ceiling, else `yes` or `no` by membership of `unapproved`. Each column is
    padded to its longest cell (the head included) plus two spaces, so the text is the same on
    every run (architecture.md §5). Spec §6's example pads the first column wider than this
    rule; the expected strings below are its rows and words re-padded by this rule.
  - Paths relative except the root.
- `main.rs` crate docs list `doctor` with the other commands and its exit statuses.

**Proof:** written first, seen failing, then passing —
- `doctor_text.rs` unit tests: `a_full_report_matches_the_documented_layout` (spec §6's example rows,
  re-padded by the column rule), `an_empty_ceiling_list_shows_a_dash`, `a_refused_want_has_no_approval`,
  `per_core_workers_say_so`.
- `tests/cli/output.rs`: `doctor_prints_the_workspace_authority` — a fixture with a program
  outside `proc.run`, a network target and `[approver] default = "interactive"` (child
  `HOME` set): the whole stdout equals the expected text, with the root substituted.
- `tests/cli/exit_codes.rs`: `doctor_exits_zero_with_refused_wants`,
  `doctor_exits_five_on_an_output_conflict`, `doctor_exits_six_on_a_corrupt_approvals_file`,
  `doctor_exits_five_on_a_build_file_error` (as `check`).

**Done when:** `cargo test -p buildl-cli` passes and `cargo make dod` is green. Commit
`feat(buildl-cli): add buildl doctor`.

---

## Risks

- **Most risky: doctor and plan disagreeing** about the wanted set. Both call
  `grants::sites` and `grants::wanted`; doctor differs only in using the whole graph and default
  settings (spec §6), which `doctor_uses_the_whole_graph_and_default_settings` pins.
- Doctor reads the build files, so a build file error is an error (exit 5), as `check`.
- Inputs outside `fs.read` are not rows: the wanted set does not list inputs (spec §6 prints
  "what the workspace's build wants" as `WantedSet`, `types/authority.rs:36-38`). `fs.read` is
  printed in the ceiling section.
- Not done here: documents (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-module-edges` and `cargo make guard-adapter-isolation` OK.
- `buildl doctor` on the `chain` fixture by hand prints the spec §6 layout.

## Review findings

Commits: `ca9754a` (task 1), `0054b03` (task 2), `b0efc47` (task 3), then the fix round.
One review over the three tasks: code 0🔴 0🟡 7🔵, spec 0🔴 1🟡.

- plan — the plan was not amended for its four deviations (`effective` is private to Plan;
  `Doctor::new`; `Wiring` accessors; help goldens) — amended in place.
- modularity — the ceiling key width was a literal `8 + GAP` — fixed: computed from the keys.
- doc-comment — a test and its doc named an internal document ("spec") — fixed: renamed
  `a_full_report_matches_the_documented_layout`.
- doc-comment — `main.rs` exit 3 left out "no data directory"; a 116-column line in
  `pipeline/mod.rs`; `Doctor::new` did not say it checks nothing — fixed.
- modularity — `grants::outside` repeats the three ceiling tests `check` makes — accepted: the
  plan names both; each is tested.
- modularity — `Wiring::approvals` and `InteractiveApprover` each locate the approvals file —
  accepted: adapter modules may not share code except through `state`; the e2e tests read the
  file the approver writes.

## Probe results

- Claim: `effective` can be called from `pipeline` — against the plan: `mod settings` is
  private to `plan` (`plan/mod.rs:19`); `graph.settings()` is the same map with no override.
- Claim: `Directory::parse(".")` gives the root — against the test fixture's first draft: `.`
  is refused by the path grammar; the root is `Directory::root()`, shown `.` by `doctor_text`.
- Mutation checks: `outside` without its `!`, `doctor` with `outside = wanted` and empty
  settings, `Workspace::doctor` with swapped approver arms, `doctor_text` with yes/no swapped —
  each failed its named tests.

## Deviations

- 2026-10-22 — task 1: `graph.settings()` in place of `effective(…, &BTreeMap::new())`.
- 2026-10-22 — task 2: `Wiring::manifest` and `Wiring::approvals`.
- 2026-10-22 — task 3: public `Doctor::new`; `arguments.rs` help goldens list `doctor`.
- 2026-10-22 — by hand, `buildl doctor` on a copy of `chain`: the spec §6 layout, exit 0,
  nothing written, `out/greet` (built) not listed.
