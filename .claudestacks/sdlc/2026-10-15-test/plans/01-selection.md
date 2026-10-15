---
status: done
created: 2026-10-15
built-on: fefed82cb705
---

# Selection by purpose Implementation Plan

**Goal:** Narrowing takes the targets a run's purpose means: every target but tests for a build of
the workspace, and only tests for a test run.

**Architecture:** A new invocation value, `types::Purpose { Build, Test }`, is the third argument
of `resolve::narrow`. A whole workspace narrows to the closure of every node of the purpose's
role; named targets narrow as today for `Build`, and for `Test` each must resolve to a `Test`-role
node or the run is refused with the new `Error::NotATest`. `Pipeline::plan` and `Pipeline::build`
pass `Purpose::Build`. No module edge changes: `resolve` already reaches `error` and `types`.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `thiserror`. Definition of Done: `cargo make dod`
and `cargo deny check`.

**Content authority:** spec §2 (the `build`/`plan` paragraph), §3.2, §3.3, §5, §6 (rows for
`resolve/narrow.rs`, `error.rs`, `tests/flows/build.rs`'s `a_plan_of_the_workspace_leaves_tests_out`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl-core`. The ten plans of this chain run in number order on this one branch.

Facts the tasks rest on, each read at commit `1799d47`:

| Fact | Where |
|---|---|
| `narrow(graph, scope) -> Result<Cow<'g, TargetGraph>>`; returns `Cow::Borrowed(graph)` for `Scope::Workspace`; otherwise starts from `graph.id_of(label)` per label (`unknown_target` for none) and walks `closure(graph, starts, TargetGraph::deps)` | `crates/buildl-core/src/resolve/narrow.rs:31-43` |
| `closure` and `unknown_target` are `pub(super)` in `reach.rs` | `crates/buildl-core/src/resolve/reach.rs:63`, `:78` |
| `narrow`'s six tests and its fixture `graph()` (`//:a`→`//:b`→`//:c`, `//:z`, alias `//:all`→`//:a`, setting `mode`) | `narrow.rs:96-170` |
| `resolve/fixtures.rs`: `target(raw, deps)`, `target_with(raw, change)`, `alias(raw, target)`, `setting(name)`, `label(raw)`; a target's `role` field is `TargetRole::Build` by default | `crates/buildl-core/src/resolve/fixtures.rs:57-110` |
| `Node` has public `label`, `provenance`, `role`, `freshness` | `crates/buildl-core/src/types/target_graph.rs:27-48` |
| `TargetRole { Build, Test }`, `Copy` | `crates/buildl-core/src/types/declaration.rs:22-29` |
| `Provenance` displays as its file's path | `crates/buildl-core/src/types/provenance.rs:50-54` |
| `Error::UnknownTarget { label, suggestion }`, message `target {label} was asked for but is not declared{suffix}`, beside it `InvalidQuery` | `crates/buildl-core/src/error.rs:285-301` |
| `Pipeline::plan` and `Pipeline::build` call `narrow(&whole, scope)` | `crates/buildl-core/src/pipeline/driver.rs:124`, `:159` |
| Root re-exports `narrow` and the `types` list | `crates/buildl-core/src/lib.rs:120-135` |
| `types/mod.rs`: one `pub mod` per file and one `pub use` line each; the Responsibilities bullet for `Scope` | `crates/buildl-core/src/types/mod.rs:51`, `:70-180` |
| Flows: only `two_directories` (`build.rs:1023`) and `tested` (`build.rs:808`) in `build.rs`, and `graph::fixture` (`graph.rs:69`, used by `plan.rs:1061`) declare a `TargetRole::Test` target | `grep -n 'TargetRole::Test' crates/buildl-core/tests/flows/*.rs` → `build.rs:812`, `build.rs:1041`, `graph.rs:51` |
| Flow helpers that narrow `Scope::Workspace`: `build.rs` `build` (`:62`), `refused` (`:795`), `build_filtered` (`:1077`); `plan.rs` `planned` (`:60`), `build` (`:157`, which also records every node of the whole graph) | as cited |
| The pinned flows over a test target: `a_test_that_runs_what_its_dependency_built_…` and `a_program_first_named_by_expansion_…` (`build.rs:820`, `:850`), `a_two_directory_workspace_…_pinned_events` and `the_order_tasks_are_taken_in_…` (`build.rs:1120`, `:1182`), `a_built_workspace_with_one_edit_plans_to_the_pinned_json` (`plan.rs:1060`) | as cited |
| `app_test` depends on `default` → `app`, which depends on `main.o` and `//lib:text`: its closure is the whole of both fixtures, so naming `//:app_test` narrows to a graph equal to the whole one (`asking_for_every_target_gives_back_an_equal_graph`, `narrow.rs:139`) | `build.rs:1030-1050`; `graph.rs:30-61` |
| Emptiness is asserted with `assert_eq!` against an empty value in this repo's tests | `crates/buildl-core/src/resolve/reach.rs:159` |

## Files that change

```
crates/buildl-core/src/types/purpose.rs        — [create] Purpose, a type definition only
crates/buildl-core/src/types/mod.rs            — [modify] declare and re-export purpose; Responsibilities bullet
crates/buildl-core/src/lib.rs                  — [modify] Purpose in the root re-exports
crates/buildl-core/src/error.rs                — [modify] Error::NotATest and its test
crates/buildl-core/src/resolve/narrow.rs       — [modify] the purpose argument, its rules, its tests
crates/buildl-core/src/pipeline/driver.rs      — [modify] plan and build pass Purpose::Build; their docs
crates/buildl-core/tests/flows/build.rs        — [modify] the pinned test-target flows name //:app_test
crates/buildl-core/tests/flows/plan.rs         — [modify] the pinned flow names //:app_test; a new flow
```

## Order of work

### Task 1 — `Purpose` and `Error::NotATest`

**Files:**
- Create `crates/buildl-core/src/types/purpose.rs` — the enum.
- Modify `crates/buildl-core/src/types/mod.rs` — `pub mod purpose;` after `provenance`;
  `pub use purpose::Purpose;`; a Responsibilities bullet after the `Scope` one: "`Purpose` — what
  a run is for, which decides the targets a whole workspace means."
- Modify `crates/buildl-core/src/lib.rs` — `Purpose` in the root `pub use types::{…}` list
  (`cargo fmt` orders it).
- Modify `crates/buildl-core/src/error.rs` — the new variant after `UnknownTarget`.

**Change:**
- `Purpose` is `Build` or `Test`, derives `Debug, Clone, Copy, PartialEq, Eq`, and holds no
  logic. Its doc: what a run is for, which decides the targets a whole workspace means — every
  target but tests for `Build`, every test for `Test` — and which targets a run may name. Its
  module doc follows the house shape (why its own file, Responsibilities, Non-responsibilities:
  choosing the targets, which `narrow` does). Being a plain enum with no logic, `purpose.rs` is
  exempt from the unit-test mandate, and its module doc ends with the line the sibling data
  files use: "This file holds only a type definition, so it carries no logic to unit-test." (`scope.rs:12`).
- `Error::NotATest { asked: Label, target: Label, provenance: Box<Provenance> }`, doc "A test run was
  asked for a target that is not a test.", fields documented as "The label that was asked for",
  "The target it names: the same label unless an alias was followed", "The build file that
  declares the target". Message exactly
  `target {asked} was asked for as a test, but {target} is declared with b.target in {provenance}`.
- `buildl-cli`'s `Exit::of_error` keeps compiling (`Error` is `#[non_exhaustive]`, `error.rs:41`,
  and its last arm is a wildcard); plan `09` gives the variant its exit code.

**Proof:**
- `not_a_test_names_the_target_and_its_file` in `error.rs`'s tests — `NotATest` with asked
  `//:default`, target `//:app` and provenance `build.lua` renders as
  `target //:default was asked for as a test, but //:app is declared with b.target in build.lua`.

**Done when:** `cargo test -p buildl-core --lib error::tests` passes and the Definition of Done
is green. Commit `feat(buildl-core): a purpose for a run, and an error for a test that is not one`.

### Task 2 — `narrow` takes the targets of a purpose

**Files:**
- Modify `crates/buildl-core/src/resolve/narrow.rs` — signature, rules, rustdoc, tests.
- Modify `crates/buildl-core/src/pipeline/driver.rs` — `Pipeline::plan` and `Pipeline::build`
  pass `Purpose::Build`; their rustdoc says a whole workspace means every target but tests.
- Modify `crates/buildl-core/tests/flows/build.rs` — the four pinned flows over a test target.
- Modify `crates/buildl-core/tests/flows/plan.rs` — the pinned flow, and one new flow.

**Change:**
- `pub fn narrow<'g>(graph: &'g TargetGraph, scope: &Scope, purpose: Purpose) ->
  Result<Cow<'g, TargetGraph>>`, by spec §3.3:
  - `Scope::Workspace`: the starts are every node whose `role` is the purpose's role (`Build`
    for `Purpose::Build`, `Test` for `Purpose::Test`), in id order, and the result is their
    closure over `TargetGraph::deps`, so a test a build target depends on is still taken. The
    `Cow::Borrowed` shortcut goes; a graph whose every node is taken comes back equal to `graph`.
  - `Scope::Targets(labels)`, `Purpose::Build`: unchanged.
  - `Scope::Targets(labels)`, `Purpose::Test`: labels are taken in label order; a label naming
    nothing is `unknown_target` as today; a label resolving (directly or through an alias) to a
    node whose role is not `Test` is `Error::NotATest { asked: label, target: node.label,
    provenance: Box::new(node.provenance) }`; the first failing label decides the error.
  - An empty start set gives an empty graph, with every setting kept.
  The rustdoc's first paragraph and `# Errors` section say all of this; `Error::NotATest` is
  linked by its explicit path, as `UnknownTarget` is (`narrow.rs:29`).
- The two `Pipeline` methods narrow with `Purpose::Build`; their argument lists do not change.
- `build.rs`: add a helper `build_named(pipeline, labels: &[&str], on_failure) ->
  Result<Vec<ActionOutcome>>` that builds `Scope::Targets` of those labels with no override and
  `LiveOutput::Off`. The two `tested()` flows build `//:app_test` through it instead of the
  workspace (the refusal flow keeps its exact message); `build_filtered` asks for `//:app_test`
  by name instead of `Scope::Workspace`. Its closure is the whole fixture, so `GOLDEN`
  (`golden/log.jsonl`) and every assertion of the two `two_directories` flows stay byte for
  byte as they are.
- `plan.rs`: add `planned_in(pipeline, scope)` and `build_in(pipeline, scope, overrides,
  version)`; `planned` and `build` delegate to them with `Scope::Workspace`, so their 39 callers do
  not change. The pinned flow at `plan.rs:1060` uses both with `//:app_test`, so `golden/plan.json`
  stays as it is.

**Proof** (written first, seen failing, then passing):
- In `narrow.rs`'s tests, a second fixture `tested()`: `//:app` (build) ← `//:app_test` (test),
  `//:lib` (build) ← `//:app` and ← `//:lib_test` (test), `//:tool` (test) ← `//:gen` (build,
  depends on `//:tool`), `//:solo` (build), alias `//:check` → `//:app_test`, alias `//:default`
  → `//:app`, a setting `mode`. The existing six tests pass `Purpose::Build` and assert as before,
  `the_whole_workspace_is_the_graph_itself` included (a graph with no test is taken whole).
  - `a_build_of_the_workspace_leaves_tests_out` — `Workspace`, `Build` over `tested()` holds
    `//:app`, `//:gen`, `//:lib`, `//:solo`, `//:tool` and neither `//:app_test` nor
    `//:lib_test`.
  - `a_build_of_the_workspace_keeps_a_test_a_target_depends_on` — the same narrowing holds
    `//:tool`, the test `//:gen` depends on.
  - `a_build_of_a_named_test_includes_it` — `Targets({//:app_test})`, `Build` holds `//:app`,
    `//:app_test`, `//:lib`.
  - `a_test_run_of_the_workspace_takes_every_test_and_its_deps` — `Workspace`, `Test` holds
    `//:app`, `//:app_test`, `//:lib`, `//:lib_test`, `//:tool`, and not `//:gen` or `//:solo`.
  - `a_test_run_of_named_tests_takes_them_and_their_deps` — `Targets({//:lib_test})`, `Test`
    holds `//:lib`, `//:lib_test`.
  - `a_test_run_follows_an_alias_to_a_test` — `Targets({//:check})`, `Test` holds `//:app`,
    `//:app_test`, `//:lib`, and keeps the alias `//:check`.
  - `a_test_run_naming_a_build_target_is_not_a_test` — `Targets({//:default})`, `Test` is
    `NotATest` with asked `//:default`, target `//:app`, and the fixture's provenance.
  - `a_test_run_of_a_workspace_without_tests_is_empty` — `Workspace`, `Test` over the existing
    `graph()` (no test) is empty (`assert_eq!` against an empty label list), and keeps the
    setting `mode`.
- `a_plan_of_the_workspace_leaves_tests_out` in `tests/flows/plan.rs` (spec §6 lists it under
  `build.rs`; it is a plan flow, and `plan.rs` holds the plan flows and their helpers) — `Pipeline::plan` of
  `graph::fixture()` with `Scope::Workspace` has verdicts for `//:app`, `//:main.o`,
  `//lib:text` and none for `//:app_test`.
- The pinned flows of `build.rs` and `plan.rs` named above pass with their goldens unchanged
  (`git diff --exit-code crates/buildl-core/tests/flows/golden` is clean).

**Done when:** `cargo test -p buildl-core --lib resolve::` and
`cargo test -p buildl-core --test flows` pass and the Definition of Done is green,
`guard-module-edges` included. Commit
`feat(buildl-core): narrow a workspace by what the run is for`.

## Risks

- **Existing pinned flows lean on "workspace means every target".** Five flows over a test
  target would silently change meaning; Task 2 moves each to a named `//:app_test` whose closure
  is the whole fixture, which keeps all three goldens byte-identical. A golden diff after Task 2
  is the signal that a flow was missed.
- **`narrow` no longer borrows the whole graph.** A build of a workspace without tests now copies
  it; the graphs are small (one per run) and the copy is what a named build already does.
- Not done here: the CLI exit code for `NotATest` (plan `09`); `Pipeline::test` (plan `06`).

## Proof

- `cargo make dod` and `cargo deny check` green after Task 2.
- `git diff --exit-code crates/buildl-core/tests/flows/golden` clean after Task 2.

## Review findings

- doc-comment-discipline — `Scope::Workspace` still read "Every target." though a build leaves tests out — `types/scope.rs:21` — fixed; `cargo make dod` → `Build Done in 28.69 seconds.`
- doc-comment-discipline — one `Pipeline::plan` rustdoc line ran to 127 columns — `pipeline/driver.rs:108` — fixed (rewrapped)
- correctness hygiene — `named`'s `_ => Ok(id)` also takes a missing node — `resolve/narrow.rs` — fixed: a comment says `id_of` gave the id
- unit-test-mandate — "the first failing label decides the error" was unpinned — `resolve/narrow.rs` — fixed: `the_first_failing_label_in_label_order_decides_the_error`; `cargo test -p buildl-core --lib resolve::narrow` → `15 passed; 0 failed`
- redundancy — `a_build_of_the_workspace_keeps_a_test_a_target_depends_on` repeats a check of `a_build_of_the_workspace_leaves_tests_out` — kept: the plan names it
- amendment hygiene — spec §6 lists `a_plan_of_the_workspace_leaves_tests_out` under `tests/flows/build.rs`; it is in `tests/flows/plan.rs` — not amended: this plan records the move, and editing the approved spec would mark all ten plans stale

## Probe results

- No throwaway probe: every premise of both tasks sits under a test the tasks name. The four flows that reach a test through the whole workspace failed before they were moved to `//:app_test`, as Risks predicted, and pass after.

## Deviations

- 2026-10-15 — `Error::NotATest.provenance` is `Box<Provenance>`, not `Provenance`: three inline fields made the variant 144 bytes and `clippy::result_large_err` (limit 128) failed the gate; every other large variant of `Error` boxes its site. Task 1 and Task 2 text amended.
- 2026-10-15 — `narrow`'s doc links to `Error::*` are plain intra-doc links (`rustdoc::redundant_explicit_links` once `Error` is imported); its first doc paragraph is one sentence (`clippy::too_long_first_doc_paragraph`).
