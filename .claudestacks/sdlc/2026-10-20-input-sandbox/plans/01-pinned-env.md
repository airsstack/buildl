---
status: done
created: 2026-10-20
built-on: 5fb250597c90
---

# Pinned Environment Implementation Plan

**Goal:** Plan refuses, with exit 5, a target that declares a variable the sandbox pins.

**Architecture:** The pinned names and values live once, in `Isolation::pinned` in `buildl-core`;
the sandbox (plan 03) reads them from there. A new pure step `plan/pinned.rs` runs in `plan`
right after `authority::check` and reports the first pinned name in name order that `sites.env`
holds, with that entry's node, as `Error::PinnedEnv`. The CLI maps the new variant to exit 5.

**Tech Stack:** Rust (`buildl-core`, `buildl-cli`), `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2 rows Environment and Pinned names, §3.1, §3.6, §4 row 1, §5 rows
`buildl-core` unit, `buildl-core` flows, `buildl-cli` (exit 5).

---

## Files that change

```
crates/buildl-core/src/types/isolation.rs     — [modify] `Isolation::pinned` and its unit tests
crates/buildl-core/src/types/env_name.rs      — [modify] crate-private constructor for a literal
crates/buildl-core/src/types/env_value.rs     — [modify] crate-private constructor for a literal
crates/buildl-core/src/error.rs               — [modify] `Error::PinnedEnv`, its rendering, a display test
crates/buildl-core/src/plan/pinned.rs         — [create] the pinned-name check and its unit tests
crates/buildl-core/src/plan/mod.rs            — [modify] declare `pinned` (`mod` line only: `plan/mod.rs:5` lists only the public `plan`)
crates/buildl-core/src/plan/assembly.rs       — [modify] call the check after `check`; error order gains step 4
crates/buildl-core/src/types/workspace.rs     — [modify] `Execution` doc: Plan now reads `strategy`
crates/buildl-core/tests/flows/plan.rs        — [modify] the flow test of the order
crates/buildl-cli/src/exit.rs                 — [modify] `PinnedEnv` → `Definition`
crates/buildl-cli/tests/cli/exit_codes.rs     — [modify] `plan` exits 5 for a pinned name
```

## Order of work

### Task 1 — Add `Isolation::pinned`

**Files:**
- Modify `crates/buildl-core/src/types/isolation.rs` — `pub fn pinned(self) -> Vec<(EnvName, EnvValue)>`
  (`Isolation` at `isolation.rs:34-50`); the module doc's Responsibilities names it
- Test `crates/buildl-core/src/types/isolation.rs` — its `mod tests` (`:94`)

**Change:** `Bare` pins nothing; `Sandbox`, `OsSandbox` and `Container` pin, in name order,
`LC_ALL=C`, `SOURCE_DATE_EPOCH=315532800`, `TZ=UTC` (spec §3.1; 315532800 is 1980-01-01T00:00:00Z,
spec P22). The doc comment says why that epoch (the earliest time ZIP records) and that this is
the one place the names and values are written; spec and premise IDs in this plan are
citations for the reader of the plan, never text for rustdoc (doc-comment-discipline). `EnvName` and `EnvValue` have only the fallible
`parse` (`env_name.rs:52`, `env_value.rs:33`) and `unwrap` is denied, so each gains a
crate-private constructor from a `&'static str` that skips the check, documented as for literals
a test proves valid; `pinned` uses it, and `the_pinned_literals_parse` is that test (the three
names fit the grammar, spec P12, `env_name.rs:21-35`).

Also modify `crates/buildl-core/src/types/env_name.rs` and `crates/buildl-core/src/types/env_value.rs`
for those two constructors.

**Proof:**
- `bare_pins_nothing` — `Isolation::Bare.pinned()` is empty.
- `every_other_strategy_pins_the_three_in_name_order` — for `Sandbox`, `OsSandbox`,
  `Container`: the names are `["LC_ALL", "SOURCE_DATE_EPOCH", "TZ"]` and the values
  `["C", "315532800", "UTC"]`, in that order.
- `the_pinned_literals_parse` — each literal name and value passes `EnvName::parse` /
  `EnvValue::parse`.

**Done when:** `cargo test -p buildl-core --lib types::isolation` passes and `cargo make dod` is
green. Commit `feat(buildl-core): name the variables the sandbox pins`.

### Task 2 — Refuse a pinned name at Plan

**Files:**
- Modify `crates/buildl-core/src/error.rs` — `Error::PinnedEnv { name: EnvName, value: EnvValue, site: Box<DeclarationSite> }`
  beside `MissingTool` (`error.rs:242-252`), rendered
  `"{provenance}: {label}: env {name} is set to {value} by the sandbox and cannot be declared"`
  in the same shape as `MissingTool`'s `#[error]`
- Create `crates/buildl-core/src/plan/pinned.rs` — `pub(super) fn pinned(sites: &Sites<'_>, workspace: &Workspace) -> Result<()>`
- Modify `crates/buildl-core/src/plan/mod.rs` — `mod pinned;` only
- Modify `crates/buildl-core/src/plan/assembly.rs` — call `pinned(&sites, &workspace)?` right after
  `let wanted = check(graph, &sites, &workspace)?;` (`assembly.rs:75`); the `# Errors` list
  (`:51-64`) gains step 4, "`Error::PinnedEnv` for a target that declares a variable the
  workspace's strategy pins", and the old steps 4–6 become 5–7; the module doc's step list
  (`:3-6`) names it
- Modify `crates/buildl-core/tests/flows/plan.rs` — one flow test

**Change:** for each `(name, value)` of `workspace.execution.strategy.pinned()`, in order, the
first `name` that `sites.env` (`authority.rs:30`, `BTreeMap<&EnvName, &Node>`, the
lowest-labelled declaring node) holds fails Plan with `Error::PinnedEnv`, its site
`DeclarationSite::from(node)` as `authority.rs:78` builds one. Under `bare` the list is empty and
nothing is refused. No new module edge: `plan -> error`, `plan -> types` exist
(`crates/expected-module-edges.txt:20-22`). `Error` is `#[non_exhaustive]`; any exhaustive match
on it inside `buildl-core` that the compiler names gains the arm.

**Proof:** unit tests in `plan/pinned.rs`, built with `plan/fixtures.rs` helpers:
- `a_pinned_name_is_refused_with_the_lowest_labelled_site` — `//b:x` and `//a:y` both declare
  `TZ` under `sandbox`: `PinnedEnv { name: TZ, value: UTC, site }` with `site.label == //a:y`.
- `the_first_pinned_name_in_name_order_is_reported` — one target declares `TZ` and `LC_ALL`:
  the error names `LC_ALL`.
- `under_bare_a_pinned_name_is_planned` — same declarations under `bare`: `Ok(())`.
- `no_pinned_name_is_planned` — `HOME` only, under `sandbox`: `Ok(())`.
- `error.rs` test `pinned_env_renders_its_site_name_and_value` — the rendered text is
  `"<provenance>: //a:y: env TZ is set to UTC by the sandbox and cannot be declared"`.

Flow test in `tests/flows/plan.rs`:
- `a_pinned_variable_is_refused_after_the_ceiling_and_before_any_tool` — a target declaring
  `TZ`, with no tool fingerprint and a missing input: with `TZ` outside `env.read` the error is
  `Refused`; once the ceiling allows it, `PinnedEnv` (not `MissingTool`); with
  `execution.strategy` set to `bare`, the error moves on to `MissingTool`. The fake manifest's
  default strategy is `sandbox` (`Execution::default()`, used at `flows/plan.rs:834-846`).

**Done when:** `cargo test -p buildl-core` passes, `crates/expected-module-edges.txt` is unchanged,
and `cargo make dod` is green. Commit `feat(buildl-core): refuse a target that declares a pinned variable`.

### Task 3 — Exit 5 for `PinnedEnv`

**Files:**
- Modify `crates/buildl-cli/src/exit.rs` — `Error::PinnedEnv { .. }` joins the `Definition` arm
  (`exit.rs:91-106`); without it the variant falls to `_ => Internal` (`:117`)
- Test `crates/buildl-cli/tests/cli/exit_codes.rs`

**Change:** a pinned name is fixed by editing a build file, so it exits 5 (spec P13).

**Proof:**
- `a_pinned_variable_exits_5` — a workspace with no `strategy` line, `env.read = ["TZ"]`, and a
  target declaring `env = { "TZ" }`: `buildl plan` exits 5 and stderr is
  `error: build.lua: //:<target>: env TZ is set to UTC by the sandbox and cannot be declared\n`
  (the provenance as `MissingTool`'s existing CLI test renders it); with `strategy = "bare"`
  added, `buildl plan` exits 0.

**Done when:** `cargo test -p buildl-cli` passes and `cargo make dod` is green. Commit
`feat(buildl-cli): exit 5 for a pinned variable`.

---

## Risks

- Task 2 changes `plan`'s documented error order; the flow test is the one check that pins it.
- The fake manifest in the flows defaults to `sandbox`; a flow fixture declaring `TZ`, `LC_ALL`
  or `SOURCE_DATE_EPOCH` would start failing. None does today
  (`grep -rnE '"(TZ|LC_ALL|SOURCE_DATE_EPOCH)"' crates` → no match); re-run it before Task 2.
- Not done here: the sandbox setting the values (plan 03), documents (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-module-edges` green (run by `dod`'s clippy step).

## Review findings

- unit-test-mandate — `the_pinned_literals_parse` checked its own copy of the literals, not what `pinned()` returns — `types/isolation.rs:199`; fixed: the test now loops `Isolation::ALL` and re-parses each `pinned()` name and value; `cargo test -p buildl-core` → 658 unit tests passed.
- doc-comment-discipline — the module-doc link to `Isolation::pinned` did not resolve — `plan/pinned.rs:9`; fixed: link definition moved into the `//!` block.
- style — test helper spelled `BTreeSet` and `Result` in full — `plan/pinned.rs:58`; fixed: imported.
- style — one module-doc line at 141 columns — `plan/assembly.rs:5`; fixed: re-flowed to 100.
- comment-matches-code — `Execution` doc kept "No phase of the pipeline reads them" beside a sentence contradicting it — `types/workspace.rs:34`; fixed: the false sentence rewritten.
- plan amendment — `types/workspace.rs` changed but was not in the file list; fixed: added.
- Verified after the fix round, on the combined tree with plan 02: `cargo make dod` → exit 0, 1501 tests passed, 0 failed, 0 warnings; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

## Probe results

- No fixture declares a pinned name (Risks) — `grep -rnE '"(TZ|LC_ALL|SOURCE_DATE_EPOCH)"' crates` → no match, exit 1; control `grep -rnE '"HOME"' crates` → `crates/buildl-core/src/error.rs:1142`, `crates/buildl-core/src/types/cache_row.rs:274`. Holds.
- 315532800 is 1980-01-01T00:00:00Z — reviewer: 3652 days × 86400. Holds.

## Deviations

- 2026-10-21 — one coder ran all three tasks in sequence (each builds on the one before) instead of one coder per task; the per-task brief extraction was skipped because the plan text was already in the orchestrator's context.
- 2026-10-21 — `crates/buildl-core/src/types/workspace.rs` changed off the file list: the `Execution` doc said no phase reads `strategy`, which Task 2 makes false. Belongs to Task 2's commit; file list amended.
