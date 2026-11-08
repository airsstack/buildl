---
status: done
created: 2026-11-07
depends-on: [01]
built-on: 55ab2847b726
---

# Unkeyed Permission Implementation Plan

**Goal:** Every level of permission must allow a variable before a target may mark it `unkeyed`.

**Architecture:** `env.unkeyed` beside each `env.read`: the ceiling and the plugin table in
`buildl.toml` (`crates/buildl/src/manifest/schema.rs:111,237`, `interpret.rs:311-313,118-120`) and
a plugin's `extension.toml` (`crates/buildl-lua/src/plugin/manifest.rs:98-107`), each refused when
it names a variable its own `env.read` does not (E4). airsl 0.1.4's manifest parser refuses
`env.unkeyed` as an unknown key, so `buildl-lua` parses `extension.toml` with `toml`, takes
`capabilities.env.unkeyed` out, and hands the rest to airsl's `RawManifest` and
`Manifest::validate` (`deny.toml` and `crates/expected-edges.txt` allow `toml` in `buildl-lua`). The wanted set gains `unkeyed` and
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
ceiling; it prints no table per plugin, so a plugin's `unkeyed` wants show as `read X unkeyed`
want rows with their `declared by plugin` line.

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

## Review findings

One reviewer pass over Tasks 1-3 (`14-reviewer-p03`), one fix round (`16-coder-p03-fix`):

- 🟡 `book/src/ch11-03-four-crates.md:191` — the `buildl-lua` row had no `toml` — fixed: row and the reasons paragraph name it.
- 🟡 `types/attribution.rs` `within`'s `Want::Unkeyed` arm untested — fixed: `within_holds_an_unkeyed_want_by_the_unkeyed_names`.
- 🟡 `buildl-lua/src/plugin/manifest.rs` split path untested — fixed: 6 tests (unknown `[extension]` key, unknown table, `fs.read`, a module flag, `[limits] memory`, `api`/name mismatch), each still refused with `env.unkeyed` present; no code change needed.
- 🔵 the two `HOME:` attribution lines looked alike for both marks — fixed: an unkeyed want renders `HOME (unkeyed)` (`a_key_mark_and_an_unkeyed_mark_read_differently` red, then green); doc-line wraps, the `Attribution` Display order list and a weak `contains` assertion — fixed.
- 🔵 strong-typing `table: String` in `UnkeyedNotRead` — declined: the other `Error` variants carry strings; not local.
- 🟡 plan text not amended for the airsl premise and the doctor rows — fixed in this plan's Architecture and Task 3.
- Gate after the fixes: `cargo make dod` exit 0, `cargo deny check` ok.

## Deviations

- 2026-11-07 — airsl 0.1.4 refuses `env.unkeyed` in `extension.toml` (`deny_unknown_fields` on `[capabilities.env]`); `buildl-lua` gained a `toml` dependency to take the key out, and `deny.toml`'s `toml` ban gained `buildl-lua` as a wrapper. The spec does not say how the manifest is parsed, so no spec claim is disproven.
- 2026-11-07 — `doctor` has no per-plugin table; plugin `unkeyed` wants show as want rows (Task 3 text amended).
- 2026-11-07 — `plan/pinned.rs` changed in Task 2: `Sites.env` no longer holds unkeyed names, so E8 reads both maps. The attribution JSON gained an `unkeyed` map; `WantedSet` serializes `unkeyed`, so the goldens `types/plan.rs`, `flows/golden/plan.json`, `buildl-cli/tests/cli/output.rs` changed.
- 2026-11-07 — `crates/buildl/tests/e2e/{approver,doctor}.rs` needed no edit.
- 2026-11-07 — the author kept this split (buildl takes `env.unkeyed` out, airsl reads the rest) over buildl owning the `extension.toml` reader; airsl's `entry` and `api` checks stay airsl's.
