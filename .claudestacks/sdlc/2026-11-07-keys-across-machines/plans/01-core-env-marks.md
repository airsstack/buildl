---
status: approved
created: 2026-11-07
built-on: 55ab2847b726
---

# Core Env Marks Implementation Plan

**Goal:** An `unkeyed` variable's value reaches the command but stays out of the action key and the remote action.

**Architecture:** All in `buildl-core`, pure logic over the ports it already has. A new `EnvMark`
(`key` / `unkeyed`) type; `Declaration.env` and the graph node's `env` carry a name → mark map,
built in the load's staging step (`crates/buildl-core/src/load/traversal.rs:276-278`), which for
now marks every staged name `key` (plan 02 stages the marks). `OwnComponents` keeps `env` for `key`
variables and gains the sorted `unkeyed` names; `plan` differences gain the four new cases of spec
§4. The plan, and so `plan --json`, holds no `unkeyed` value: the execute phase asks `Host::env`
for each `unkeyed` name when it makes the action ready, so the local command gets every value while
`RemoteAction.env` is built from `key` variables and the pinned three only (spec §2 "Where the mark
travels", §5).

**Tech Stack:** Rust 1.94, `buildl-core` (no I/O, `crates/buildl-core/clippy.toml`), serde with the
canonical JSON serializer.

**Content authority:** `spec.md` §2, §4, §5, §9 (core and remote rows).

---

## Files that change

```
crates/buildl-core/src/types/env_mark.rs        — create: EnvMark, its parse and rendering
crates/buildl-core/src/types/mod.rs             — modify: export EnvMark
crates/buildl-core/src/types/declaration.rs     — modify: env becomes BTreeMap<EnvName, EnvMark> (:98)
crates/buildl-core/src/load/traversal.rs        — modify: staged names become the map, each `key` for now (:276-278)
crates/buildl-core/src/types/target_graph.rs    — modify: node env becomes the same map (:50)
crates/buildl-core/src/resolve/assembly.rs      — modify: copy the map (today without_repeats, :167)
crates/buildl-core/src/plan/pinned.rs           — modify: read the map's names (:26-37)
crates/buildl-core/src/grants/wanted.rs         — modify: iterate the map's names (:57)
crates/buildl-core/src/grants/plugins.rs        — modify: iterate the map's names (:64)
crates/buildl-core/src/plan/authority.rs        — modify: iterate the map's names (:82)
crates/buildl-core/src/types/action_key.rs      — modify: OwnComponents gains `unkeyed: BTreeSet<EnvName>`
crates/buildl-core/src/plan/components.rs       — modify: values of key variables; names of unkeyed ones
crates/buildl-core/src/plan/assembly.rs         — modify: pass marks to the gathering (:106)
crates/buildl-core/src/plan/difference.rs       — modify: UnkeyedAdded, UnkeyedRemoved, NowUnkeyed, NowKeyed
crates/buildl-core/src/types/ready_action.rs    — modify: env keeps every value; `unkeyed` names the unkeyed ones (:38)
crates/buildl-core/src/execute/assembly.rs      — modify: ready() asks Host::env for the unkeyed names (:276-288)
crates/buildl-core/src/execute/remote_action.rs — modify: env from key variables and the pinned three (:46-47)
crates/buildl-core/src/ports/two_machines.rs    — create (#[cfg(test)]): a fake Host for two machines
crates/buildl-core/src/ports/mod.rs             — modify: declare the test module
crates/buildl/src/report/plan_text.rs           — modify: texts of the four new differences (:26), their tests
crates/buildl/src/remote/encode.rs              — test only: a different timeout gives a different Action digest
crates/buildl-core/tests/flows/plan.rs          — modify: construct declarations with the map
```

Every other construction site of `Declaration` or a graph node the compiler names in
`crates/buildl-core`, `crates/buildl-lua` and `crates/buildl` (test fixtures) is changed in Task 1
to build the map; the build is green after each task.

## Order of work

### Task 1 — Add `EnvMark` and carry it through declarations and the graph

**Files:** Create `crates/buildl-core/src/types/env_mark.rs`; modify `types/mod.rs`,
`types/declaration.rs:98`, `load/traversal.rs:276-278`, `types/target_graph.rs:50`,
`resolve/assembly.rs:167`, `plan/pinned.rs:26-37`, `grants/wanted.rs:57`, `grants/plugins.rs:64`,
`plan/authority.rs:82`, `crates/buildl-core/tests/flows/plan.rs`, and the construction sites the
compiler names.

**Change:** `EnvMark` is an enum `Key` / `Unkeyed` with `parse(&str)` accepting exactly `"key"` and
`"unkeyed"`, and `as_str`. `Declaration.env` and the node's `env` become
`BTreeMap<EnvName, EnvMark>`, so names are held sorted (spec §2). The staging step marks every
staged name `Key` (the Lua side is unchanged until plan 02). The loops over `node.env` read its
keys; their behaviour is unchanged.

**Proof:** written first, seen failing, then passing —
- `env_mark_parses_key_and_unkeyed` — `"key"` and `"unkeyed"` parse; `"pass"`, `""`, `"Key"` do not.
- `declaration_env_is_sorted_by_name` — a map built in another order serializes the same.
- `resolve_keeps_each_mark` — a node's env holds the declared marks.
- `a_pinned_name_is_refused_with_either_mark` — `TZ` marked `key` and marked `unkeyed` each give
  today's pinned refusal (E8, exit 5 through `Error`'s exit mapping).

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): mark each declared variable key or unkeyed`.

### Task 2 — Keep `unkeyed` values out of the key, and explain every change

**Files:** Modify `types/action_key.rs` (`OwnComponents`, its doc example), `plan/components.rs`,
`plan/assembly.rs:106`, `plan/difference.rs`, `crates/buildl/src/report/plan_text.rs`; create
`ports/two_machines.rs` (`#[cfg(test)]`) and declare it in `ports/mod.rs`.

**Change:** `OwnComponents` gains `unkeyed: BTreeSet<EnvName>`; `env` holds only `key` variables
with their values (unset → `None`, as today). `components::environment` asks `Host::env` only for
`key` names. `difference.rs` reports, for one target: a `key` value change, addition or removal as
`Difference::Env` (unchanged); an `unkeyed` name added → `UnkeyedAdded`; removed →
`UnkeyedRemoved`; `key`→`unkeyed` → `NowUnkeyed`; `unkeyed`→`key` → `NowKeyed`. `plan_text.rs`
renders them `unkeyed variable X added`, `unkeyed variable X removed`, `variable X is now
unkeyed`, `variable X is now keyed`. The fake `TwoMachines` host returns different `HOME` and
`PATH` values per machine.

**Proof:** written first, seen failing, then passing —
- `two_machines_share_a_key_when_home_and_path_are_unkeyed` (core) — one target, both machines, same digest.
- `a_key_variable_value_still_splits_the_key` (core) — `RUSTFLAGS` marked `key` with two values gives two digests.
- `a_tool_or_input_change_still_splits_the_key` (core) — another tool fingerprint or input digest gives another digest.
- `an_unkeyed_value_change_is_not_a_difference` (core) — no difference reported.
- `an_unset_unkeyed_variable_gives_the_same_key` (core) — unset and set give the same digest.
- core, one per row of spec §4's table: `unkeyed_added_is_a_difference`, `unkeyed_removed_is_a_difference`, `key_to_unkeyed_is_a_difference`, `unkeyed_to_key_is_a_difference`.
- `plan_text` (in `crates/buildl/src/report/plan_text.rs`), one per new text: `prints_unkeyed_added`, `prints_unkeyed_removed`, `prints_now_unkeyed`, `prints_now_keyed`.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): keep unkeyed variable values out of the action key`.

### Task 3 — Give the command every value, the remote only `key` ones

**Files:** Modify `types/ready_action.rs:38`, `execute/assembly.rs:276-288`,
`execute/remote_action.rs:46-47`; test in `crates/buildl/src/remote/encode.rs`.

**Change:** `ReadyAction` gains `unkeyed: BTreeSet<EnvName>`; `ready()` fills `ReadyAction.env` from
the key's `env` values as today and adds, for each `unkeyed` name, the value `Host::env` gives now
(none when unset), so the local executors give every value (`crates/buildl/src/exec/run.rs:61-68`,
unchanged). `remote_action` builds `RemoteAction.env` from `ReadyAction.env` minus `unkeyed`, plus
the pinned three. The serialized `Plan` holds no `unkeyed` value. `encode::command` is unchanged.

**Proof:** written first, seen failing, then passing —
- `ready_action_holds_unkeyed_values` (core) — an `unkeyed` `HOME` with a value is in `ReadyAction.env`.
- `two_machines_give_the_same_remote_action` (core, `TwoMachines`) — the two machines' `RemoteAction`s are equal, holding `RUSTFLAGS` and the pinned three, not `HOME` or `PATH`.
- `a_different_timeout_gives_a_different_action` (in `encode.rs`) — two `RemoteAction`s differing only in timeout give different `Action` digests, the same `Command` digest.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): send only keyed variables to the remote`.

---

## Risks

- Task 1 changes a type every crate builds; the compiler names every site, and the staging step
  keeps today's list form working, so no fixture breaks before plan 02.
- Task 3 reads `unkeyed` values at execute time, not plan time: a value changed between `plan` and
  the run reaches the command, which is what an `unkeyed` variable means (no key holds it).
- Cache rows written before this plan no longer read back (spec §4, unpublished): a stored golden
  row is regenerated, not kept.
- Not done here: permission of `unkeyed` (plan 03), the map form in Lua (plan 02), the refusals
  (plan 04).

## Proof

- `cargo make dod` and `cargo deny check` pass.
- `cargo test -p buildl-core two_machines` passes.
