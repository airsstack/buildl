---
status: done
created: 2026-10-22
built-on: 5d10ef9b2f2e
---

# Built Programs Implementation Plan

**Goal:** Plan holds every program a command runs to the ceiling, where a program a dependency built needs no `proc.run` entry.

**Architecture:** Plan now places the outputs and expands every command, so it needs `Layout`
and `expand`, which live in `execute` today. `lib.rs` rule 1 forbids one phase naming another,
so both move, unchanged, into a new crate-private shared module `placement` (the pattern of
`suggest`: a helper several modules use, not a phase). A new module `grants` (not a phase, spec
§2) holds the one "built" rule (spec §3) and the complete, unrefused wanted set; plan and
execute's admission both call it. `plan/authority.rs` keeps `check` (the refusal). No port
changes.

**Tech Stack:** Rust, `buildl-core`, `buildl` (one fixture), `cargo make dod`, `cargo deny
check`. Guideline: `claudestacks-guideline-rust:rust-guidelines` (`mod.rs` export-only,
colocated unit tests, doc discipline, strong types).

**Content authority:** spec §1 P2–P5, §2, §3, §5 (`Refused`, `OutputConflict` rows), §9 (core
unit, core flows rows; the `chain` row of CLI).

---

## Files that change

```
crates/buildl-core/src/placement/mod.rs        — [create] module docs, `mod`/`pub(crate) use` only
crates/buildl-core/src/placement/layout.rs     — [move from execute/layout.rs] Layout, unchanged body
crates/buildl-core/src/placement/expand.rs     — [move from execute/expand.rs] Lists, expand, expand_description, unchanged bodies
crates/buildl-core/src/placement/fixtures.rs   — [create] #[cfg(test)] builders the moved tests use, copied from execute/fixtures.rs
crates/buildl-core/src/execute/mod.rs          — [modify] drop `layout` and `expand`
crates/buildl-core/src/execute/assembly.rs     — [modify] import from placement; pass dep paths to admit
crates/buildl-core/src/execute/admission.rs    — [modify] built program admitted without a proc.run entry
crates/buildl-core/src/execute/fixtures.rs     — [modify] only import paths, if it names layout/expand
crates/buildl-core/src/verify/assembly.rs      — [modify] import Layout from placement
crates/buildl-core/src/grants/mod.rs           — [create] module docs, `mod`/`use` only
crates/buildl-core/src/grants/built.rs         — [create] `built`: is a program one of a target's dependency output paths or below one
crates/buildl-core/src/grants/wanted.rs        — [create] `Sites` and `sites` (moved from plan/authority.rs), now over expanded commands
crates/buildl-core/src/grants/fixtures.rs      — [create] #[cfg(test)] builders the grants tests use, copied from plan/ and execute/fixtures.rs
crates/buildl-core/src/pipeline/driver.rs      — [modify] doc comments of plan, build, test: plan's errors gain the output conflict
crates/buildl-core/src/plan/authority.rs       — [modify] keep `check` and `holds`; `Sites`/`sites` move out
crates/buildl-core/src/plan/tools.rs           — [modify] fingerprinted programs skip built ones
crates/buildl-core/src/plan/components.rs      — [modify] a target's tool identity skips a built program
crates/buildl-core/src/plan/pinned.rs          — [modify] import `Sites`/`sites` from grants
crates/buildl-core/src/plan/assembly.rs        — [modify] place outputs before sites; error-order doc
crates/buildl-core/src/lib.rs                  — [modify] `mod placement;`, `pub mod grants;`, module table, rule 1 sentence
crates/expected-module-edges.txt               — [modify] edges for placement and grants
crates/buildl-core/tests/flows/plan.rs         — [modify] plan-level flow tests
crates/buildl-core/tests/flows/build.rs        — [modify] build-level flow tests
crates/buildl/tests/fixtures/chain/buildl.toml — [modify] proc.run drops "out/greet"
```

## Order of work

### Task 1 — Move `Layout` and `expand` into a shared `placement` module

**Files:** `placement/mod.rs`, `placement/layout.rs`, `placement/expand.rs`, `placement/fixtures.rs`, `execute/mod.rs`,
`execute/assembly.rs`, `execute/fixtures.rs` (imports only, if needed), `verify/assembly.rs`,
`lib.rs`, `crates/expected-module-edges.txt`.

**Change:** a pure move. `execute/layout.rs` (`Layout`, `of` `:34`, `outputs`, `deps` `:81`)
and `execute/expand.rs` (`Lists`, `expand` `:65`, `expand_description`) move with their tests
to `placement/`; no body changes. Their test modules import `crate::execute::fixtures`
(`execute/layout.rs:117`, `execute/expand.rs:153`), and `guard-module-edges` reads test code
(`Makefile.toml:190-198`), so they import `crate::placement::fixtures` instead: a
`#[cfg(test)] mod fixtures;` in `placement/` holding copies of the builders they use (`dir`,
`graph`, `node`, `outputs`, `command`, `output_path`, `path`, `settings`), with the module doc
`execute/fixtures.rs:1-4` has. Each module keeping its own fixtures is the existing pattern
(`plan/fixtures.rs`, `execute/fixtures.rs`). `lib.rs` declares `mod placement;` (crate-private) and its
module table gains a row: "where every output lives and what a command's placeholders stand
for once they are placed; used by Plan, Execute and verify, and not a phase". Each moved
file's module doc drops "Its own file because" wording that names `execute` as its home.
`expected-module-edges.txt`: add `placement -> error`, `placement -> types`, `execute ->
placement`, `verify -> placement`; drop any edge left unused (the guard
`cargo make guard-module-edges`, `Makefile.toml:151`, fails on both missing and stale edges).

**Proof:** no behaviour changes. The moved unit tests pass unchanged in their new place; every
`execute` and `verify` test passes unchanged.

**Done when:** `cargo test -p buildl-core` passes, `cargo make guard-module-edges` is OK and
`cargo make dod` is green. Commit `refactor(buildl-core): move output layout and command
expansion into placement`.

### Task 2 — The "built" rule, and admission using it

**Files:** `grants/mod.rs`, `grants/built.rs`, `grants/fixtures.rs`, `execute/admission.rs`,
`execute/assembly.rs`, `lib.rs`, `crates/expected-module-edges.txt`, `tests/flows/build.rs`.

**Change:**
- `grants/built.rs`: `pub(crate) fn built(program: &Program, deps: &[OutputPath]) -> bool` —
  true when the program's text equals a dep path's text, or starts with it followed by `/`
  (spec §3 Rule). No other test: a program not written as a path (`go`) is never built.
- `lib.rs`: `pub mod grants;` (public: plan 02 adds `negotiate` here); table row "the build's
  authority: which programs a build made, everything a build wants, and the approver's answer;
  not a phase". Rule 1's sentence naming `history` and `verify` as non-phases names `grants`
  too.
- `admit` (`execute/admission.rs:32`) gains `deps: &[OutputPath]`; the ceiling check
  (`:42-48`) is skipped when `built(&program, deps)`. Its doc and the module doc say a built
  program needs no ceiling entry. `prepared` (`execute/assembly.rs:263,269`) passes the
  `layout.deps(graph, id)` list it already computes.
- `grants/fixtures.rs`: `#[cfg(test)]`, the `pub(crate)` builders grants' tests need (`program`,
  `output_path`; task 3 adds `node`, `graph`, `command`, `settings`, `label`, `path`, `dir`),
  copied from `plan/fixtures.rs` and `execute/fixtures.rs`, so no grants test names `plan` or
  `execute`.
- `tests/flows/build.rs`: `a_program_first_named_by_expansion_is_refused_before_anything_runs`
  (`:986`) asserts that the `$deps`-named `out/app` is refused; that program is now built. It is
  rewritten over a target whose `$in`-named program is outside the ceiling (still refused before
  anything runs). `a_test_that_runs_what_its_dependency_built_is_built_when_the_workspace_allows_that_program`
  (`:859`) drops its `ceiling.run.insert(program("out/app"))` (`:862`) and is renamed
  `a_test_that_runs_what_its_dependency_built_is_built_with_no_ceiling_entry`.
- Edges: `grants -> types`, `execute -> grants`.

**Proof:** written first, seen failing, then passing —
- `grants/built.rs`, one test per spec §3 row that the rule alone decides:
  `a_dependency_output_is_built`, `a_path_below_a_directory_output_is_built`,
  `a_path_sharing_only_a_prefix_is_not_built` (`out/gen2` vs `out/gen`),
  `a_path_with_no_dependency_is_not_built` (empty deps), `a_host_program_is_not_built` (`go`),
  `an_input_path_is_not_built` (`scripts/gen.sh` with deps `out/gen`),
  `the_targets_own_output_is_not_built` (`out/x` when deps hold only `out/gen`).
- `execute/admission.rs`: `a_built_program_outside_the_ceiling_is_admitted`,
  `a_program_outside_the_ceiling_that_is_not_built_is_refused` (the existing refusal test, with
  deps given).

**Done when:** `cargo test -p buildl-core` (lib and `--test flows`) passes and
`cargo make dod` is green. Commit `feat(buildl-core): run a program a dependency built without a ceiling entry`.

### Task 3 — Plan computes every program, places outputs first

**Files:** `grants/wanted.rs`, `grants/mod.rs`, `plan/authority.rs`, `plan/pinned.rs`,
`plan/tools.rs`, `plan/components.rs`, `plan/assembly.rs`, `pipeline/driver.rs`,
`grants/fixtures.rs`, `crates/expected-module-edges.txt`, `tests/flows/plan.rs`,
`tests/flows/build.rs`; amended 2026-10-22 (execution): `plan/fixtures.rs` and the test
imports of the other `plan/*.rs` files (one copy of the shared helpers, in `grants/fixtures.rs`),
`tests/flows/graph.rs` with `tests/flows/golden/graph.json` and `golden/plan.json` (the shared
fixture's test ran `$out/app` with no outputs, a program `/app` plan now holds to the ceiling;
it runs `$deps` now), and the test module of `crates/buildl/src/report/plan_text.rs` (a
fixture ran `$deps` over a dependency with no outputs).

**Change:**
- `grants/wanted.rs`: `Sites` and `sites` move here from `plan/authority.rs:26,37` (fields
  unchanged: `programs`, `env`, `inputs`). `sites` gains `layout: &Layout`; its programs are,
  for each node, the first text of `expand(&node.run, settings, &lists)` (lists from
  `layout.outputs(id)`, `layout.deps(graph, id)` and `node.inputs`, as `prepared` builds them,
  `execute/assembly.rs:262-268`), parsed with `Argument::parse` then `Program::of`, as `admit`
  does (`execute/admission.rs:33-37`), kept only when not
  `built(&program, &layout.deps(graph, id))`, each with its lowest-labelled node. A command
  that names no program is left out here; `admit` still refuses it at execute
  (`Error::NoProgram`). Module doc: this is the complete wanted set's sites, used by plan's
  check and (plan 04) by `doctor`.
- `plan/authority.rs`: `check` unchanged in behaviour, now over `grants::Sites`; `Sites`,
  `sites` and their tests leave; the moved tests import `crate::grants::fixtures`, not
  `crate::plan::fixtures` (`plan/authority.rs:151`). Module doc's Responsibilities line updated.
- `plan/tools.rs`: `programs` (`:35`) skips a node whose `program_of` result is built for that
  node (`built` over `layout.deps`), so a built program is never looked up or fingerprinted
  (spec §3, amended 2026-10-22). It gains `layout: &Layout`. `program_of` unchanged.
- `plan/components.rs`: `own` (`:62`) gives a target no `tool` when its program is built for
  it. `Gathered` carries what `own` needs to tell (the layout, or the per-node set of built
  programs) — the coder picks the smaller; the result is a target whose built program adds no
  `ToolIdentity` to its key, and whose key is otherwise unchanged.
- `plan/pinned.rs`: imports `Sites` from `grants`.
- `plan/assembly.rs` (`:76-82`): after `effective`, `Layout::of(graph, &workspace.out)?`, then
  `sites(graph, &settings, &layout)`, `check`, `pinned`, `fingerprints(...)` over the
  layout-aware `programs`. The error-order doc (`:55-68`) inserts
  "[`Error::OutputConflict`] for two outputs of the planned graph that would be written to one
  place" between item 2 and `Refused`, and item 3 says a program named through `$in`, `$out` or
  `$deps` is checked here too.
- `pipeline/driver.rs`: the `plan`, `build`, `test` doc comments that list plan's errors gain the
  output conflict (doc only).
- Edges: `plan -> grants`, `plan -> placement`, `grants -> placement`, `grants -> error` if used.

**Proof:** written first, seen failing, then passing —
- `grants/wanted.rs` unit tests: `a_program_named_through_in_is_wanted`,
  `a_program_named_through_deps_is_not_wanted`, `a_written_out_dependency_output_is_not_wanted`,
  `a_setting_naming_a_dependency_output_is_not_wanted` (`$opt:tool` = `out/gen`),
  `a_written_out_output_of_a_target_not_depended_on_is_wanted`,
  `a_program_named_through_out_is_wanted`, `a_command_naming_no_program_is_left_out`,
  `each_program_names_its_lowest_labelled_target`.
- `tests/flows/plan.rs`: `a_program_named_through_in_outside_the_ceiling_is_refused_at_plan`
  (no strategy call, `Error::Refused` naming it);
  `an_output_conflict_is_refused_at_plan`;
  `an_output_conflict_comes_before_a_refusal` (both present: `OutputConflict` returned);
  `a_built_program_is_never_looked_up` (the fake tool resolver records lookups: none for
  `out/gen`);
  `plan_json_lists_in_programs_and_not_built_ones` (the `wanted` of the plan).
- `tests/flows/build.rs`: `a_tool_a_dependency_built_runs_without_a_ceiling_entry` — spec P4's
  shape: `gen` from input `gen.go` via `go`, `banner.txt` with `deps = { "gen" }` and
  `run = { "$deps", "$out" }`, ceiling `proc.run = ["go"]`: builds, both targets built;
  `a_written_out_tool_a_dependency_built_runs_on_a_clean_build` (the same with
  `{ "out/gen", "$out" }`).
- Existing `plan/authority.rs` and `plan/tools.rs` tests keep passing after the move; task 2's
  rewrite of `build.rs:986` (a `$in` program) still passes, now refused at plan.

**Done when:** `cargo test -p buildl-core` (lib and `--test flows`) passes and `cargo make dod`
is green. Commit `feat(buildl-core): hold every command's program to the ceiling at plan`.

### Task 4 — The `chain` fixture lists host tools only

**Files:** `crates/buildl/tests/fixtures/chain/buildl.toml`.

**Change:** `proc.run = ["cp", "sort"]` (drops `"out/greet"`, line 8). `greeting` runs
`$deps` (`out/greet`), which it depends on, so it needs no entry.

**Proof:** the existing `buildl` e2e tests that build `chain` pass unchanged (amended
2026-10-22: no `buildl-cli` test builds `chain`; `cargo test -p buildl-cli` is a regression run).
On `main` the same fixture change is refused: the spec review re-ran a copy of `chain` without
`out/greet` and got `error: build.lua: //:greeting: program out/greet is outside the ceiling
(proc.run allows: cp, sort)`, exit 4 (spec review report, P4 row). So these tests prove the rule
end to end.

**Done when:** `cargo test -p buildl --test e2e` and `cargo test -p buildl-cli` pass and
`cargo make dod` is green. Commit `test(buildl): drop the built program from the chain
fixture's ceiling`.

---

## Risks

- **Most risky: task 3's change to `own`.** A target whose written-out program is built loses
  its `ToolIdentity` from its key components. Key format is unchanged; only such targets' key
  values change, once. Every other key must stay byte-identical: the golden `plan.json`
  (`tests/flows/golden/plan.json`) and the e2e `identical` tests are the guard and must pass
  unchanged. Amended 2026-10-22 (execution): the goldens' key digests are unchanged; their
  fixture command text changed by one token (`$out/app` → `$deps`), see Task 3's Files.
- `sites` now expands every command; `expand` is pure and cheap, but plan now refuses an output
  conflict for the planned graph, which only execute refused before. Execute keeps its own
  whole-workspace conflict check unchanged.
- A command naming no program passes plan and is still refused at execute (`NoProgram`), as
  today.
- `grants` must not name `plan` or `execute` (it is used by both); `placement` names only
  `types` and `error` — test code included, hence each keeps its own `fixtures.rs`. The edges
  guard enforces both; adding a reverse edge to `expected-module-edges.txt` to pass it is not a
  fix.
- Not done here: the approver (plans 02, 03), `doctor` (plan 04), documents (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-module-edges` and `cargo make guard-adapter-isolation` OK.
- Every key digest in the golden plan JSON is unchanged, and every existing key-related test
  passes unchanged.

## Review findings

Commits: `0ff0845` (task 1), `0840b18` (task 2), `ba37d17` (task 3), `938134e` (task 4).

- doc-comment — `placement/expand.rs` module doc had a 161-character line — task 1 — fixed
  (rewrapped); `cargo fmt --all -- --check` clean.
- doc-comment — `placement/mod.rs` said no user of `placement` may name another, but `verify`
  names `execute` — task 1 — fixed: "Plan and Execute may not name each other".
- correctness (security) — `grants/built.rs` matched a dependency's output path as a text
  prefix, so `$deps/../../../usr/bin/touch` counted as built and skipped `proc.run`; the reviewer
  ran `/usr/bin/touch` through the binary under `proc.run = ["cp"]` — task 2 — fixed and
  verified: the remainder after the output path must be empty or `/` plus a path
  `TreePath::parse` accepts; new tests `a_path_climbing_out_of_a_dependency_is_not_built`,
  `a_path_with_a_dot_segment_is_not_built`, `a_path_with_an_empty_segment_is_not_built`.
  `cargo test -p buildl-core --lib grants::built` → `test result: ok. 10 passed; 0 failed`.
- doc-comment — "a bare name is never built" relied on the manifest refusing a root `out` —
  task 2 — fixed: `built` is false for any program with no `/`.
- doc-comment — `grants/mod.rs` claimed re-exports it does not have — task 2 — fixed.
- doc-comment — `grants/mod.rs` and `lib.rs` said Plan names `grants` before it did — task 2 —
  true from task 3 (`plan -> grants` edge).
- test-fixture — the shared flows fixture's test target ran `$out/app` with no outputs (a
  program `/app`); the first cut added `/app` to the golden test's ceiling — task 3 — fixed: the
  fixture runs `$deps`; the goldens change one token each, `wanted.programs` stays `["cc"]`, and
  all four `"digest"` values in `golden/plan.json` are identical to the previous commit's
  (each appears once removed and once added in `git diff -U0`).
- plan — task 3's Files list and the "golden passes unchanged" premise were wrong — task 3 —
  amended above (Task 3 Files, Risks, Proof).
- modularity — `grants/fixtures.rs` copied nine helpers of `plan/fixtures.rs` — task 3 — fixed:
  one copy, in `grants/fixtures.rs`; plan tests import it.
- doc-comment — `Sites.programs` doc left out a `$deps`-named program that is not built; a
  grammar slip — task 3 — fixed.
- efficiency — the naming pass ran twice (programs, built runners) — task 3 — fixed: one
  `naming` pass returns both.
- test hygiene — `plan_text.rs`'s `row` helper hard-coded `//:leaf` — task 3 — fixed: derived
  from the graph.
- plan — task 4's Proof named `buildl-cli` tests that build `chain`; none does — task 4 —
  amended above.

## Probe results

- Claim: with `proc.run = ["cp", "sort"]` the `chain` workspace builds — a copy of the fixture
  (`greet.sh` made executable, as the e2e helper does), `BUILDL_FIXTURE_FLAVOR=x buildl build` →
  `6 built`, exit 0. A first run on a copy made with `cp -R` failed with `the program out/greet
  was not found`, exit 3, because `greet.sh` had lost its exec bit; not a buildl defect.
- Claim: `admit`, task 2's refusal site, sits at `execute/admission.rs:42`, called at
  `execute/assembly.rs:269` — `sed -n '28,50p'` and `grep -n` — held.

## Deviations

- 2026-10-22 — task 3 also changed `plan/fixtures.rs` and the test imports of the other
  `plan/*.rs` files, `tests/flows/graph.rs`, both goldens, and the test module of
  `crates/buildl/src/report/plan_text.rs`. Plan now holds `$out`- and `$deps`-named programs to
  the ceiling, which two test fixtures broke: one ran `$out/app` on a target with no outputs, the
  other `$deps` over a dependency with no outputs. Each fixture was corrected rather than given a
  ceiling entry.
