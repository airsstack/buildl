---
status: done
created: 2026-10-22
built-on: 5fa60e343706
---

# Plugin Origin Implementation Plan

**Goal:** A target a plugin declared carries its origin into the graph and is held to each plugin's manifest and workspace cap before the ceiling.

**Architecture:** All in `buildl-core`, testable against fake ports (spec §5, §8). New domain
types name a plugin, its manifest caps and the workspace's cap; an `Origin` rides on each staged
declaration, each `Declaration`, each `Node` and each `DeclarationSite`, so every site-bearing
error names the chain beside the file and label. A node also holds `narrowing`: its origin chain
followed by its rule's chain (spec §5), used only by the check; display and attribution read
`origin`. A new check in `grants`, run by Plan before the ceiling check, refuses the first want
outside any narrowing member's caps with `Error::PluginRefused` (exit 4). The existing `Want`
gains `Network(Label)` so one canonical want type serves the ceiling, the plugin check and
attribution (plan 02). `Workspace` gains `plugins`, empty until plan 03 reads `[plugins]`.
Origin is not an action-key component.

**Tech Stack:** Rust, `buildl-core`, `buildl-cli` (exit mapping), `serde` derives,
`cargo make dod`, `cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`
(strong types parsed at construction, no `bool` for semantic flags, one canonical type per
concept, `mod.rs` export-only, colocated unit tests, doc discipline).

**Content authority:** spec §5, §7 (`PluginRefused` row), §8 (`buildl-core` row), §9 (core rows
for origin and narrowing).

---

## Files that change

```
crates/buildl-core/src/types/plugin.rs        — [create] PluginName, PluginVersion, PluginRef, PluginCap, PluginNetwork, Origin, PluginViolation
crates/buildl-core/src/types/mod.rs           — [modify] declare and re-export the new types
crates/buildl-core/src/types/authority.rs     — [modify] Want gains Network(Label)
crates/buildl-core/src/types/workspace.rs     — [modify] Workspace gains `plugins: BTreeMap<PluginName, PluginCap>`
crates/buildl-core/src/types/build_file.rs    — [modify] StagedDeclaration gains `origin: Origin`
crates/buildl-core/src/types/declaration.rs   — [modify] Declaration gains `origin`; new/into_parts/accessor
crates/buildl-core/src/types/target_graph.rs  — [modify] Node gains `origin: Origin` (serialized) and `narrowing: Vec<PluginRef>` (not serialized)
crates/buildl-core/src/load/traversal.rs      — [modify] `convert` (:142) carries origin into Declaration (:153)
crates/buildl-core/src/resolve/index.rs       — [modify] Entry keeps the origin (:78-83 drop it today via into_parts)
crates/buildl-core/src/resolve/references.rs  — [modify] `rule` (:87-99) returns the rule with its origin
crates/buildl-core/src/resolve/assembly.rs    — [modify] Node (:150) gets origin and narrowing
crates/buildl-core/src/error.rs               — [modify] DeclarationSite (:589) gains origin, rendered beside file and label; From<&Node> (:596); PluginRefused; Want::Network arm in `refusal` (:503)
crates/buildl-core/src/grants/plugins.rs      — [create] plugin_violations
crates/buildl-core/src/grants/mod.rs          — [modify] declare `plugins`
crates/buildl-core/src/plan/authority.rs      — [modify] plugin_check beside `check` (refusing stays in plan, listing in grants)
crates/buildl-core/src/plan/assembly.rs       — [modify] call `plugin_check` before `authority::check`
crates/buildl-cli/src/exit.rs                 — [modify] `of_error` (:96) maps PluginRefused to 4
crates/buildl/src/manifest/interpret.rs       — [modify] `workspace` (:60) sets `plugins` empty
crates/buildl-lua/src/staging.rs              — [modify] stages Origin::BuildFile (plan 04 stamps plugins)
crates/buildl-core/tests/flows/{load,plan,graph}.rs — [modify] origin and narrowing flows
crates/buildl-core/tests/flows/golden/*.json, crates/buildl/tests/e2e/golden/chain.graph.json — [modify] graph JSON gains `origin`
```

Every `Workspace {`, `StagedDeclaration {` and `DeclarationSite {` construction gains its new
field. Known sites: `Workspace {` — `buildl/src/manifest/interpret.rs:63`, `types/checkup.rs:69`,
`plan/pinned.rs:56`, `plan/authority.rs:112`, `grants/outside.rs:61`, `ports/bundle.rs:57`,
`ports/port_set.rs:129`, `buildl-cli/src/doctor_text.rs:213`, `buildl/src/report/plan_text.rs:141`,
`buildl-core/tests/flows/common.rs:1111`; `StagedDeclaration {` — `load/evaluation.rs:149`,
`load/loading.rs:251`, `load/traversal.rs:342`, `pipeline/gc.rs:198`,
`buildl-core/tests/flows/common.rs:1188`, `buildl-lua/src/module.rs:339`,
`buildl-lua/src/staging.rs:66`, `buildl-lua/tests/flows/declarations.rs:16`. The coder re-runs
`git grep -n -e "Workspace {" -e "StagedDeclaration {" -e "DeclarationSite {" -- crates` and
updates every hit.

## Order of work

### Task 1 — The plugin types, `Want::Network` and `Workspace::plugins`

**Files:** `types/plugin.rs`, `types/mod.rs`, `types/authority.rs`, `types/workspace.rs`,
`error.rs` (the `Want::Network` arm only), `buildl/src/manifest/interpret.rs`, every `Workspace {` site.

**Change:** add
- `PluginName` — `[plugins.<name>]`'s name, parsed with `TargetName`'s grammar (spec §2);
- `PluginVersion` — the `[extension] version` text: non-empty, no control characters; airsl
  keeps it uninterpreted (`airsl-0.1.4/src/extension/manifest.rs:134-138`), so buildl does not
  parse it as semver;
- `PluginCap` — the workspace's cap: `run: BTreeSet<Program>`, `env: BTreeSet<EnvName>`,
  `network: PluginNetwork`;
- `PluginNetwork` — `Sealed` (default) or `Allowed`;
- `PluginRef` — a chain member: `name`, `version`, manifest `run` and `env`; displays
  `<name>@<version>`;
- `Origin` — `BuildFile` or `Plugins(Vec<PluginRef>)`, outermost first; the constructor refuses
  an empty chain; displays a chain as `ourgo@0.1.0 > go@1.2.0`;
- `PluginViolation` — `{ want: Want, site: DeclarationSite, plugin: PluginRef, allowed: BTreeSet<String> }`;
- `Want::Network(Label)` — a target declared to reach the network. The ceiling never produces it
  (`grants/outside.rs`: "The network is never outside"); `refusal` (`error.rs:503`) renders it
  as `network` for completeness;
- `Workspace.plugins`, empty from `interpret::workspace`.

`Origin` and `PluginRef` derive `Ord`, `Hash`, `Serialize`, `Deserialize` because `Declaration`
does and `resolve/index.rs:78` sorts declarations.

**Proof:** written first, seen failing, then passing (colocated) —
- `plugin_name_accepts_a_target_name_and_refuses_a_slash` — `go` parses; `a/b` and the empty
  string are refused
- `plugin_version_refuses_empty_and_control_characters`
- `plugin_ref_displays_name_at_version` — `go@1.2.0`
- `origin_refuses_an_empty_chain`, `origin_displays_a_chain_outermost_first`
- `plugin_network_defaults_to_sealed`

**Done when:** `cargo test -p buildl-core` passes and `cargo make dod` is green.
Commit `feat(buildl-core): plugin names, caps and origin types`.

### Task 2 — Origin from staged declaration to graph node

**Files:** `types/build_file.rs`, `types/declaration.rs`, `types/target_graph.rs`,
`load/traversal.rs`, `resolve/index.rs`, `resolve/references.rs`, `resolve/assembly.rs`,
`buildl-lua/src/staging.rs`, every `StagedDeclaration {` site, goldens, flows.

**Change:** `StagedDeclaration.origin`; `convert` copies it into `Declaration` (accessor
`origin()`, part of `into_parts`). `resolve/index.rs` keeps the origin in its `Entry` and
`references::rule` hands back the rule's origin with the rule. `Node.origin` is the target's
origin, serialized, so `buildl graph --json` shows it (spec §5). `Node.narrowing` is the
target's chain followed by each member of its rule's chain not already in it; it is not
serialized. `buildl-lua` stages `Origin::BuildFile` everywhere until plan 04. No `KeyComponents`
change.

**Proof:** written first, seen failing, then passing —
- `load` flow `origin_is_carried_to_the_declaration`
- `resolve` unit `a_hand_written_target_naming_a_plugin_rule_has_build_file_origin_and_that_plugin_in_narrowing`
- `resolve` unit `narrowing_joins_target_and_rule_chains_without_repeats` — `[ourgo, go]` target, `[go]` rule → `[ourgo, go]`
- `graph` flow `graph_json_shows_each_node_s_origin` — golden updated
- `plan` flow `origin_does_not_change_the_key`

**Done when:** `cargo test --workspace` passes (goldens `crates/buildl-core/tests/flows/golden/*`
and `crates/buildl/tests/e2e/golden/chain.graph.json` updated to add `origin` only), `cargo make
dod` green. Commit `feat(buildl-core): carry a declaration's plugin origin to the graph`.

### Task 3 — Errors name the plugin chain

**Files:** `error.rs`, every `DeclarationSite {` site, `tests/flows/plan.rs`.

**Change:** `DeclarationSite` gains `origin: Origin`, filled from the declaration or node it
names (`From<&Node>`, `:596`). Every message that renders a site prints
`<file>: <label> (plugin ourgo@0.1.0 > go@1.2.0): …` for a plugin origin and is unchanged for
`BuildFile`, so `Refused`, `MissingTool`, resolve errors and the rest name the chain (spec §5).

**Proof:** written first, seen failing, then passing —
- `error` unit `a_site_with_a_plugin_origin_names_the_chain`
- `error` unit `a_build_file_site_reads_as_today`
- `plan` flow `a_ceiling_refusal_of_a_plugin_target_names_its_chain`

**Done when:** `cargo test --workspace` passes; `cargo make dod` green. Commit
`feat(buildl-core): name the plugin chain in errors`.

### Task 4 — Plan holds a plugin's targets to its caps

**Files:** `grants/plugins.rs`, `grants/mod.rs`, `plan/assembly.rs`, `error.rs`,
`buildl-cli/src/exit.rs`, `tests/flows/plan.rs`.

**Change:** `plugin_violations(graph, settings, layout, workspace)` lists every
`PluginViolation`, in label order then chain order, so `doctor` (plan 02) can report them all.
For each node with a non-empty `narrowing`, each want — its program (a built one is no want, via
`grants::built`, as today), each env name, and `Want::Network(label)` when it declares the
network — is held to every member: program in the member's `run` and in
`workspace.plugins[name].run`; env in both `env` sets; network only when the cap's `network` is
`Allowed`. A member with no `workspace.plugins` entry has an empty cap. A violation's `allowed`
is the intersection of the member's manifest list and its cap's list for that kind (empty for
network). `plugin_check` runs in Plan before `authority::check` and refuses with the first as
`Error::PluginRefused { violation: Box<PluginViolation> }`, rendered
`<site>: program curl is outside plugin go@1.3.0 (proc.run allows: go)`; for the network,
`network is outside plugin go@1.3.0 (network not allowed)`. `exit.rs` maps it to 4.

**Proof:** written first, seen failing, then passing —
- `grants::plugins` unit `a_program_outside_the_manifest_is_refused`
- `… a_program_outside_the_workspace_cap_is_refused`
- `… allowed_is_manifest_intersect_cap`
- `… every_narrowing_member_is_checked` — `[ourgo, go]`, allowed by `go` only → names `ourgo`
- `… a_plugin_rule_used_by_a_hand_written_target_is_checked`
- `… network_needs_the_cap`
- `… a_built_program_is_not_held_to_a_plugin`
- `… build_file_targets_are_not_checked`
- `… violations_are_listed_in_label_order`
- `plan` flow `plugin_refusal_comes_before_the_ceiling`
- `plan` flow `wants_inside_the_plugin_still_meet_the_ceiling`
- `exit.rs` unit `plugin_refused_exits_4`

**Done when:** `cargo test --workspace` passes; `cargo make dod` green.
Commit `feat(buildl-core): hold plugin targets to their plugins' caps`.

## Risks

- New fields on `StagedDeclaration`, `Declaration`, `Node`, `DeclarationSite` and `Workspace`
  touch many sites in four crates; the sweep command is the guard. Most risky: Task 2's resolve
  changes (`index.rs`, `references.rs`).
- Goldens change for every workspace; reviewers confirm only `origin` was added.
- Adding `Want::Network` makes existing exhaustive matches on `Want` grow an arm
  (`error.rs:503`); the compiler lists them.
- Chosen not to do: attribution display (plan 02), reading `[plugins]` (plan 03).

## Proof

`cargo make dod` and `cargo deny check` green; `buildl graph` on the `chain` fixture shows
`"origin":"BuildFile"`-shaped values on every node; a plugin target outside its cap is refused
before the ceiling check with exit 4, naming its chain.

## Review findings

- correctness — `DuplicateLabel`, `NotATest`, `InvalidDeclaration`, `ActionConflict` dropped the plugin chain — `error.rs` — fixed: each carries its origin (`DuplicateLabel.sites: Vec<DeclarationSite>`, `NotATest.site`, `origin` on the two load errors); verified `cargo make dod` → `Build Done in 15.70 seconds`, exit 0
- strong-types — public `Origin::Plugins(Vec<PluginRef>)` plus derived `Deserialize` allowed an empty chain — `types/plugin.rs` — fixed: `PluginChain` newtype (private field, serde `try_from`), `{"Plugins":[]}` refused by test; verified same run
- test-coverage — no plugin-origin test for `OutputConflict` or `Holder::site` — `error.rs`, `resolve/references.rs` — fixed: tests added; verified same run
- correctness — ceiling refusal for `Want::Network` dropped the label — `error.rs` — fixed: `network of //:fetch is outside the ceiling …`; verified same run
- test-coverage — `plugin_check` had no unit test — `plan/authority.rs` — fixed: two unit tests; verified same run
- spec-drift — `every_narrowing_member_is_checked` tested the reverse case — `grants/plugins.rs` — fixed: test matches the plan wording, reverse kept as `the_innermost_member_is_checked_too`
- amendment — `plugin_check` placed in `plan/authority.rs`, not `grants/plugins.rs` — plan amended (Files that change)
- nit — `PluginViolation.allowed` is `BTreeSet<String>` — no change: the plan asks for it (one list type for programs and env names)

## Probe results

- Structural: `git grep -n -e "pub struct TargetName" -e "pub struct Program" -e "pub struct EnvName" -e "pub enum Want" -e "pub struct DeclarationSite" -e "pub struct Workspace" -e "pub struct StagedDeclaration" -e "fn of_error" -e "The network is never outside" -- crates` → all found (`types/target_name.rs:26`, `types/program.rs:39`, `types/env_name.rs:43`, `types/authority.rs:148`, `error.rs:589`, `types/workspace.rs:18`, `types/build_file.rs:107`, `buildl-cli/src/exit.rs:96`, `grants/outside.rs:19`). No behavioural premise outside the plan's own tests.

## Deviations

- 2026-10-23 — `plugin_check` lives in `plan/authority.rs` beside `check` (refusing in plan, listing in `grants`, the split `outside`/`check` already use). Plan amended.
- 2026-10-23 — `Declaration` serializes its origin, so the `Nondeterministic` error text shows `"origin":"BuildFile"` inside each declaration JSON; pinned strings in `buildl` tests updated.
- 2026-10-23 — `DependencyCycle` and `OutputConflict` put the chain inside their existing parentheses: `//a (build.lua, plugin ourgo@0.1.0 > go@1.2.0)`.
- 2026-10-23 — `Origin::Plugins` holds a `PluginChain` (non-empty by construction), not a bare `Vec<PluginRef>`; JSON shape unchanged.
- 2026-10-23 — two flow tests give their plugins `run = {cc}` plus a cap, since a plugin with an empty manifest is now refused before the ceiling.
