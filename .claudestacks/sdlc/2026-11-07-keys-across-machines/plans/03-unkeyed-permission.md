---
status: approved
created: 2026-11-07
depends-on: [01]
built-on: 55ab2847b726
---

# Unkeyed Permission Implementation Plan

**Goal:** Every level of permission must allow a variable before a target may mark it `unkeyed`.

**Architecture:** `env.unkeyed` beside each `env.read`: the ceiling and the plugin table in
`buildl.toml` (`crates/buildl/src/manifest/schema.rs:111,237`, `interpret.rs:311-313,118-120`) and
a plugin's `extension.toml` (`crates/buildl-lua/src/plugin/manifest.rs:98-107`), each refused when
it names a variable its own `env.read` does not (E4). The wanted set gains `unkeyed` and
`Want::Unkeyed`, checked against the ceiling and, for a plugin's target, the plugin table and the
manifest, as `env.read` is today (E3). `plan`, the approvers and `doctor` list `unkeyed variables:`
apart (spec §3, §4).

**Tech Stack:** Rust 1.94, `buildl-core`, `buildl` (TOML via the existing manifest reader),
`buildl-lua` (manifest), `buildl-cli` (doctor text).

**Content authority:** `spec.md` §3, §4 (listings), §7 (E3, E4).

---

## Files that change

```
crates/buildl-core/src/types/authority.rs      — modify: WantedSet.unkeyed, Ceiling.unkeyed; listing line (:109-110)
crates/buildl-core/src/types/plugin.rs         — modify: Want::Unkeyed (:311), PluginCap.unkeyed (:173), PluginRef.unkeyed (:190)
crates/buildl-core/src/grants/wanted.rs        — modify: collect unkeyed names (:45-58, :102)
crates/buildl-core/src/grants/outside.rs       — modify: unkeyed minus the ceiling's (:29-34)
crates/buildl-core/src/grants/plugins.rs       — modify: unkeyed ∩ manifest ∩ cap (:64-68)
crates/buildl-core/src/grants/attribution.rs   — modify: attribute unkeyed wants (:42)
crates/buildl-core/src/plan/authority.rs       — modify: refuse unkeyed outside the ceiling (:82)
crates/buildl-core/src/error.rs                — modify: E3 text names env.unkeyed (:1755-1783); E4 (UnkeyedNotRead), exit 5
crates/buildl/src/manifest/schema.rs           — modify: EnvTable gains `unkeyed`
crates/buildl/src/manifest/interpret.rs        — modify: read it for ceiling and plugin table; E4
crates/buildl/src/manifest/toml_manifest.rs    — test: the new key
crates/buildl-lua/src/plugin/manifest.rs       — modify: [capabilities] env.unkeyed; E4
crates/buildl/src/approver/interactive.rs      — modify: name unkeyed variables in the prompt (:276)
crates/buildl-cli/src/doctor_text.rs           — modify: env.unkeyed rows for ceiling and plugins (:94)
crates/buildl/tests/e2e/approver.rs, doctor.rs, crates/buildl-cli/tests/cli/output.rs — modify: expected texts
crates/buildl-cli/tests/cli/exit_codes.rs      — test: E3 exits 4, E4 exits 5
```

## Order of work

### Task 1 — Read `env.unkeyed` at the three levels

**Files:** Modify `crates/buildl/src/manifest/schema.rs`, `interpret.rs`, `toml_manifest.rs` (tests),
`crates/buildl-lua/src/plugin/manifest.rs`, `crates/buildl-core/src/types/authority.rs` (`Ceiling`),
`types/plugin.rs` (`PluginCap`, `PluginRef`), `error.rs` (E4), `crates/buildl-cli/tests/cli/exit_codes.rs`.

**Change:** `EnvTable` gains `unkeyed`, a list left out being empty. Reading `[ceiling]`,
`[plugins.<name>]` and `[capabilities]` fills `Ceiling.unkeyed`, `PluginCap.unkeyed`,
`PluginRef.unkeyed`. A name in a table's `env.unkeyed` that is not in the same table's `env.read`
is E4, exit 5: `buildl.toml: ceiling env.unkeyed lists HOME, which env.read does not` (the plugin
table and `extension.toml` name themselves the same way).

**Proof:** written first, seen failing, then passing —
- `ceiling_reads_env_unkeyed` — `["HOME", "PATH"]` reaches `Ceiling.unkeyed`.
- `env_unkeyed_left_out_is_empty` — no key gives an empty set.
- `ceiling_unkeyed_outside_read_is_refused` — E4 with its message.
- `plugin_table_unkeyed_outside_read_is_refused` — E4 naming `[plugins.c]`.
- `extension_unkeyed_outside_read_is_refused` — E4 naming the manifest.
- `unkeyed_outside_read_exits_5` (in `crates/buildl-cli/tests/cli/exit_codes.rs`) — the binary exits 5.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl): read env.unkeyed beside env.read`.

### Task 2 — Want, check and attribute `unkeyed` variables

**Files:** Modify `types/authority.rs` (`WantedSet`), `types/plugin.rs` (`Want::Unkeyed`),
`grants/wanted.rs`, `grants/outside.rs`, `grants/plugins.rs`, `grants/attribution.rs`,
`plan/authority.rs`, `error.rs` (E3 text), `crates/buildl-cli/tests/cli/exit_codes.rs`.

**Change:** `WantedSet.env` holds the names some target marks `key`; `WantedSet.unkeyed` those some
target marks `unkeyed`; a name marked both ways in two targets is in both. Each `env` name must be
in the ceiling's `env.read` (today); each `unkeyed` name in the ceiling's `env.unkeyed`. A plugin's
target is held to the plugin table's and the manifest's `env.unkeyed` the same way. `Want::Unkeyed`
renders `unkeyed variable X` with cap key `env.unkeyed`. A refusal exits 4 (E3).

**Proof:** written first, seen failing, then passing —
- `unkeyed_names_are_wanted_apart` — a target marking `HOME` unkeyed puts it in `unkeyed`, not `env`.
- `a_name_marked_both_ways_is_in_both_sets` — two targets, one each way.
- `unkeyed_outside_the_ceiling_is_refused` — E3 naming `env.unkeyed`.
- `unkeyed_outside_the_ceiling_exits_4` (in `crates/buildl-cli/tests/cli/exit_codes.rs`) — the binary exits 4.
- `a_key_mark_needs_only_env_read` — a name in both lists marked `key` is allowed by `env.read`.
- `plugin_unkeyed_needs_table_and_manifest` — refused when either list leaves it out.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): allow an unkeyed variable only where every level does`.

### Task 3 — List `unkeyed variables` in `plan`, the approvers and `doctor`

**Files:** Modify `types/authority.rs:109-110` (listing), `crates/buildl/src/approver/interactive.rs`
(prompt, `:276`), `crates/buildl-cli/src/doctor_text.rs` (`:94` and the plugin rows), and the
expected texts in `crates/buildl/tests/e2e/approver.rs`, `doctor.rs`,
`crates/buildl-cli/tests/cli/output.rs`.

**Change:** the listing prints `read variables:` for `key` names and, when any, `unkeyed
variables:` for the rest. The interactive approver names them in its prompt; since it asks only for
wants it has not remembered (`interactive.rs:71-72`), a remembered yes for reading `HOME` does not
cover `HOME` unkeyed. The manifest approver is unchanged. `doctor` prints `env.unkeyed` for the
ceiling and each plugin.

**Proof:** written first, seen failing, then passing —
- `listing_prints_unkeyed_variables_apart` — `read variables: RUSTFLAGS` and `unkeyed variables: HOME, PATH`.
- `remembered_read_does_not_cover_unkeyed` — the interactive approver asks again for `HOME` unkeyed,
  and after a yes the remembered set holds it, so a third run asks nothing.
- `doctor_prints_env_unkeyed` — the ceiling row and a plugin row.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): list unkeyed variables in plan, approvers and doctor`.

---

## Risks

- Task 2 touches the permission core; a missed intersection would let a plugin leave a variable
  unkeyed that its manifest does not allow. `plugin_unkeyed_needs_table_and_manifest` guards it.
- Not done here: the examples' and plugin's `buildl.toml`/`extension.toml` (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` pass.
