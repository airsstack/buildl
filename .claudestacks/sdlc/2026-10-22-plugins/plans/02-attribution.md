---
status: done
created: 2026-10-22
depends-on: [01]
built-on: 5fa60e343706
---

# Attribution Implementation Plan

**Goal:** `plan`, `doctor` and the approver prompt name the plugin chain and target behind each want.

**Architecture:** `buildl-core` computes an `Attribution` beside the `WantedSet` — it never
changes the set's shape, so remembered approvals and `WantedSet` JSON stay as they are (spec §5).
`Plan` and `Checkup` carry it; the `Approver` port's `approve` gains it as a second argument so
the interactive prompt can show it; `doctor` also lists every plugin violation, refusing
nothing. Rendering lives where it lives today: `buildl/src/report/plan_text.rs`,
`buildl/src/approver/interactive.rs`, `buildl-cli/src/doctor_text.rs`.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `buildl-cli`, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §5 ("Attribution of a want", the "Where / What" table), §6 is not
touched, §9 (core attribution rows, CLI prompt and doctor text).

---

## Files that change

```
crates/buildl-core/src/types/attribution.rs      — [create] Attribution (keyed by the canonical `Want`, plan 01) and its Display
crates/buildl-core/tests/flows/golden/plan.json  — [modify] the plan JSON gains `attribution` (included by tests/flows/plan.rs:26)
crates/buildl-core/src/types/mod.rs              — [modify] export them
crates/buildl-core/src/grants/attribution.rs     — [create] `attribution(graph, sites…)`: the map for a graph
crates/buildl-core/src/grants/mod.rs             — [modify] declare it
crates/buildl-core/src/types/plan.rs             — [modify] Plan holds `attribution`; accessor; JSON
crates/buildl-core/src/plan/assembly.rs          — [modify] compute and store it
crates/buildl-core/src/types/checkup.rs          — [modify] Checkup holds attribution and plugin violations
crates/buildl-core/src/pipeline/doctor.rs        — [modify] fill them (`plugin_violations` from plan 01)
crates/buildl-core/src/ports/approver.rs         — [modify] `approve(&mut self, wanted, attribution)`
crates/buildl-core/src/grants/negotiate.rs       — [modify] pass the plan's attribution
crates/buildl-core/src/pipeline/driver.rs        — [modify] build/test/verify hand `plan.attribution()` to negotiate
crates/buildl/src/approver/{manifest,deny,local,interactive}.rs — [modify] new signature; interactive prints attribution
crates/buildl/src/report/plan_text.rs            — [modify] attribution lines under the wanted set
crates/buildl-cli/src/doctor_text.rs             — [modify] attribution and plugin violations
test Approver impls: buildl-core/tests/flows/common.rs:1068, grants/negotiate.rs:62, ports/approver.rs:63,
  ports/port_set.rs:289, buildl/src/report/plan_text.rs:321 — [modify] new signature
crates/buildl-core/tests/flows/{plan,doctor}.rs  — [modify] attribution flows
```

## Order of work

### Task 1 — The attribution value and its computation

**Files:** create `types/attribution.rs`, `grants/attribution.rs`; modify `types/mod.rs`,
`grants/mod.rs`, `types/plan.rs`, `plan/assembly.rs`, `tests/flows/golden/plan.json`.

**Change:** `Attribution` maps a `Want` — only its `Program`, `Env` and `Network(Label)` kinds
occur; there is no second want type (plan 01 made `Want` canonical) — to the chains whose targets
want it, each chain (a node's `origin`, never its `narrowing`) with the lowest-labelled target of
that origin wanting it; ordered maps throughout (`architecture.md` §5 rule 1). Write directories
are never attributed; a want only targets of `Origin::BuildFile` want has no entry — including a
hand-written target that names a plugin's rule (spec §5). A program a dependency built is no want and has no entry.
`Display` renders, for each attributed want in the wanted set's display order (programs,
variables, network), one line per chain:
`  go: declared by plugin go@1.2.0 for //services/api:server`, a chain of two as
`ourgo@0.1.0 > go@1.2.0`. `Plan` stores it (accessor `attribution()`) and serializes it as `attribution`; the golden
`tests/flows/golden/plan.json` is updated.

**Proof:** written first, seen failing, then passing —
- `grants::attribution` unit `a_plugin_program_names_its_chain_and_lowest_target`
- `… two_chains_wanting_one_program_both_appear`
- `… build_file_only_wants_have_no_entry`
- `… network_targets_are_attributed_by_label`
- `… a_hand_written_target_using_a_plugin_rule_is_not_attributed`
- `types::attribution` unit `display_follows_the_wanted_set_order`
- `plan` flow `plan_carries_the_attribution`

**Done when:** `cargo test -p buildl-core` passes and `cargo make dod` green. Commit
`feat(buildl-core): attribute each want to the plugins behind it`.

### Task 2 — The approver sees the attribution

**Files:** modify `ports/approver.rs`, `grants/negotiate.rs`, `pipeline/driver.rs`,
`buildl/src/approver/{manifest,deny,local,interactive}.rs`, every test `Approver` impl listed above.

**Change:** `Approver::approve(&mut self, wanted: &WantedSet, attribution: &Attribution)`;
`negotiate(wanted, attribution, approver)`; `Pipeline::build`, `test` and `verify` pass the
plan's. `InteractiveApprover` prints the attribution lines for the **new** wants only (those it
is asking about) after the existing wanted-set lines and before `allow? [y/N]`. Remembered
approvals compare `WantedSet` only, unchanged.

**Proof:** written first, seen failing, then passing —
- `buildl` unit in `approver/interactive.rs` `the_prompt_names_the_plugin_of_a_new_want` — a
  console fake records the prompt; it contains `go: declared by plugin go@1.2.0 for //:server`
- `… remembered_wants_are_not_attributed_again` — only new wants' lines appear
- core `negotiate` unit `the_attribution_reaches_the_approver`

**Done when:** `cargo test --workspace` passes and `cargo make dod` green. Commit
`feat(buildl): show plugin attribution in the approval prompt`.

### Task 3 — `plan` and `doctor` show it

**Files:** modify `buildl/src/report/plan_text.rs`, `types/checkup.rs`, `pipeline/doctor.rs`,
`buildl-cli/src/doctor_text.rs`, `tests/flows/doctor.rs`.

**Change:** `plan_text` prints the attribution lines below the wanted set. `Checkup` gains
`attribution()` and `plugin_violations()` (all of them, from plan 01's `plugin_violations`);
`doctor_text` lists each want followed by its attribution, and a section of plugin violations
worded as the refusal message is, while `doctor` still exits 0.

**Proof:** written first, seen failing, then passing —
- `plan_text` unit `plan_text_lists_attribution_under_the_wanted_set`
- `doctor` flow `doctor_reports_every_plugin_violation_and_refuses_nothing`
- `doctor_text` unit `doctor_text_names_plugins_and_violations`

The same output through the binary, with a target a real plugin declared, is proven in plan 04
Task 4, the first point at which Lua can declare one.

**Done when:** `cargo test --workspace` passes and `cargo make dod` green. Commit
`feat(buildl-cli): show plugin attribution in plan and doctor`.

## Risks

- The `Approver` signature change touches nine impls across three crates (listed); a missed test
  impl fails to compile, which is the guard.
- No plugin target can be declared from Lua until plan 04, so this plan proves rendering with
  core fakes and unit tests; the binary-level check is plan 04 Task 4.
- Chosen not to do: attribution in the remembered approvals file (spec §5 keeps its shape).

## Proof

`cargo make dod` and `cargo deny check` green; a plan of a workspace with a plugin target prints
the attribution line; `doctor` lists a plugin violation and exits 0.

## Review findings

- spec-drift — `plan` printed bare attribution lines with no wants (spec §5: each want, then its attribution) — `buildl/src/report/plan_text.rs` — fixed: `plan_text` prints `plan.wanted()` with its existing `Display`, then the attribution, before the counts line; verified `cargo make dod` → `Build Done in 17.12 seconds`, exit 0
- test-coverage — `FakeApprover` dropped the attribution, so build/test/verify passing it was unguarded — `tests/flows/common.rs` — fixed: recorded and asserted by `a_build_hands_the_approver_the_attribution_of_the_plan` (seen failing with `Attribution::default()` in `build`); verified same run
- strong-types — `Attribution::record` kept the first label, not the lowest, and accepted Read/Write — `types/attribution.rs` — fixed: lowest label kept whatever the order, Read/Write ignored; two tests; verified same run
- doc — `AttributionView` doc claimed wanted-set field order — fixed: alphabetical, as canonical JSON writes
- modularity — `Attribution::iter` had no caller — removed
- format — doctest line over 100 columns in `ports/bundle.rs` — re-wrapped
- simplification — `doctor_text` built an `Error::PluginRefused` to print it — fixed: `PluginViolation: Display` holds the one wording, used by the error and by `doctor_text`; texts byte-identical (existing pinned tests pass)
- question — plan JSON carries the whole `PluginRef` per chain member — no change: same shape as `graph --json`'s `origin`
- process — some red steps were not seen failing in isolation — noted; every named proof test exists and passes

## Probe results

- Structural: `git grep -n -e "pub trait Approver" -e "fn approve" -e "pub fn negotiate" -e "pub struct Checkup" -- crates` → found (`ports/approver.rs:28`, `grants/negotiate.rs:22`, `types/checkup.rs:16`, nine `fn approve` impls); one impl the plan did not list, the doctest at `ports/bundle.rs:228`, updated too.

## Deviations

- 2026-10-23 — `plan` text prints the wanted set (it printed none before), so every plan shows its wants, plugins or not; attribution lines sit under it, before the counts line. CLI tests pinning plan text updated.
- 2026-10-23 — `Attribution` gained `within(&WantedSet)` (prompt shows only new wants' lines) and `lines(&Want)` (one wording for plan, prompt and doctor); `record` is public so `buildl` tests can build one.
- 2026-10-23 — `origin_does_not_change_the_key` compares verdicts (which hold the keys), since plan JSON now differs by attribution.
- 2026-10-23 — a network want is shown by its label: `  //:fetch: declared by plugin go@1.2.0 for //:fetch`, under the wanted set's network line.
