---
status: approved
created: 2026-10-22
depends-on: [04]
built-on: 0f868841fc48
---

# Discovery Declaration Implementation Plan

**Goal:** The declaration API gains discovery targets, whose results the declaration port hands to the build file that asks for them.

**Architecture:** A discovery target is a target with a third `TargetRole`, `Discovery`, one
output `<name>.json`, cached, never `always` (spec §6). `buildl.discover` stages it;
`buildl.discovered(label)` returns the bytes of that target's result from a `Discovered` map the
port now receives (`DeclarationSource::evaluate(&self, file, discovered)`, spec §6), or `nil`.
`load` passes the map through. Where the map's contents come from, and the two-pass flow, are
plan 07; here `load` callers pass an empty map, so behaviour is unchanged for every command.
Execute refuses a discovery output that is a directory as an action failure.

**Tech Stack:** Rust, `buildl-core`, `buildl-lua`, `buildl`, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §6 (declaration, reading, key/cache/sandbox, `build`/`test` default
scope rows), §7 (discovery output as a directory), §9 (`b.discover`/`b.discovered` rows).

---

## Files that change

```
crates/buildl-core/src/types/declaration.rs       — [modify] TargetRole::Discovery
crates/buildl-core/src/types/discovered.rs        — [create] Discovered: BTreeMap<Label, Vec<u8>>, `get`, `empty`
crates/buildl-core/src/types/mod.rs               — [modify] export Discovered
crates/buildl-core/src/ports/declaration_source.rs — [modify] evaluate(&self, file, discovered: &Discovered); doctest (:38)
crates/buildl-core/src/load/evaluation.rs         — [modify] evaluate_file (:46) passes it (call :56)
crates/buildl-core/src/load/traversal.rs          — [modify] `load` (:54) takes `&Discovered`; role match (:223): Discovery → `<name>.json`
crates/buildl-core/src/types/forcing.rs           — [modify] (:33) Discovery forces like Build (no arm change needed if `_` covers it; asserted by a test)
crates/buildl-core/src/types/outcome.rs           — [modify] OutputRefusal::NotAFile, its text
crates/buildl-core/src/execute/perform.rs         — [modify] a Discovery target whose output is a tree → refused NotAFile
crates/buildl-core/src/pipeline/{driver,doctor,gc}.rs — [modify] every `load` call passes `&Discovered::empty()` (plan 07 replaces this)
every DeclarationSource impl — [modify] new signature: buildl-lua/src/source.rs:96; buildl-core/tests/flows/common.rs:88;
  pipeline/gc.rs:188; load/evaluation.rs:121; ports/port_set.rs:121; buildl/tests/e2e/parallel_load.rs:69;
  buildl/src/report/plan_text.rs:236; doctests ports/bundle.rs:49, ports/declaration_source.rs:38
crates/buildl-lua/src/primitives.rs               — [modify] `discover` fields: target's closed table minus outputs, always
crates/buildl-lua/src/module.rs                   — [modify] install `discover` and `discovered` on every primitives table (plan 04's builder)
crates/buildl-lua/src/source.rs                   — [modify] hands `discovered` and the file's directory to the module
crates/buildl-lua/tests/flows/discovery.rs        — [create] flows
crates/buildl-lua/tests/flows/main.rs             — [modify] `mod discovery;`
crates/buildl-core/tests/flows/{load,build}.rs    — [modify] discovery role flows
```

## Order of work

### Task 1 — The `Discovery` role and the port's new argument

**Files:** `types/declaration.rs`, `types/discovered.rs`, `types/mod.rs`,
`ports/declaration_source.rs`, `load/evaluation.rs`, `load/traversal.rs`, `types/forcing.rs`,
`pipeline/{driver,doctor,gc}.rs`, every `DeclarationSource` impl listed above.

**Change:** `TargetRole::Discovery`. `convert_target`'s default outputs: Discovery → one output
`<name>.json`; a staged Discovery target carrying `outputs` or `always` is refused as an invalid
declaration (defence: `buildl-lua` never stages them). `Discovered` (ordered map, label → bytes).
`DeclarationSource::evaluate` and `load` take `&Discovered`; every current caller passes
`Discovered::empty()`. `resolve::narrow::role_of` is unchanged, so a whole-workspace build or test
excludes discovery targets and a build may still name one (spec §6).

**Proof:** written first, seen failing, then passing —
- `load` flow `a_discovery_target_outputs_its_name_dot_json`
- `load` flow `a_staged_discovery_with_outputs_is_refused`
- `build` flow `a_workspace_build_skips_discovery_targets` and `a_named_discovery_target_builds`
- `test` flow `a_test_run_naming_a_discovery_target_is_not_a_test`
- `forcing` unit `discovery_is_forced_like_a_build_target`
- `evaluation` unit `the_discovered_map_reaches_the_source`

**Done when:** `cargo test --workspace` passes; `cargo make dod` green. Commit
`feat(buildl-core): discovery targets and the discovered map on the declaration port`.

### Task 2 — `buildl.discover` and `buildl.discovered`

**Files:** `buildl-lua/src/primitives.rs`, `module.rs`, `source.rs`, `tests/flows/discovery.rs`,
`tests/flows/main.rs`.

**Change:** `discover(name, table)` reads `run` or `rule`, `inputs`, `deps`, `env`, `network`
(closed table: `outputs` and `always` are unknown fields and refused like any other); stages a
target with role Discovery. `discovered(raw)` resolves `raw` with `Label::resolve(raw, file
directory)` (`types/label.rs:76`); an unparseable label raises a refusal naming the line;
returns the bytes as a Lua string, or `nil` when the map holds none. Both are on every primitives
table, plugin instances included (a plugin's discovery target carries its origin).

**Proof:** written first, seen failing, then passing (`tests/flows/discovery.rs`) —
- `discover_stages_a_discovery_target`
- `discover_refuses_outputs_and_always`
- `discovered_returns_nil_without_a_result`
- `discovered_returns_the_bytes_for_a_relative_label` — map holds `//svc:pkgs`; `svc/build.lua`
  asks `:pkgs`
- `a_plugin_s_discover_carries_its_origin`

**Done when:** `cargo test -p buildl-lua` passes; `cargo make dod` green. Commit
`feat(buildl-lua): buildl.discover and buildl.discovered`.

### Task 3 — A discovery output must be a file

**Files:** `types/outcome.rs`, `execute/perform.rs`, `tests/flows/build.rs`.

**Change:** `OutputRefusal::NotAFile` ("is a directory, and a discovery output must be a file");
in `perform`, a Discovery target whose output was found as a directory tree is refused with it,
as the other refusals are (`perform.rs:236-266`): an action failure, exit 1, nothing cached.

**Proof:** written first, seen failing, then passing —
- `build` flow `a_discovery_that_leaves_a_directory_fails_and_is_not_cached`
- `build` flow `a_build_target_may_still_leave_a_directory`

**Done when:** `cargo test -p buildl-core` passes; `cargo make dod` green. Commit
`feat(buildl-core): a discovery output must be a file`.

## Risks

- The port signature change touches every `DeclarationSource` (nine sites, listed); compile errors
  are the guard.
- `forcing.rs:33` falls through `_` for non-test roles; Task 1's test pins that Discovery behaves
  as Build.
- Chosen not to do here: reading results and the two passes (plan 07).

## Proof

`cargo make dod` and `cargo deny check` green; a discovery target builds when named, is skipped
by a whole-workspace build, fails when it leaves a directory, and `buildl.discovered` returns what
the map holds.
