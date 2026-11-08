---
status: done
created: 2026-11-08
depends-on: [04]
built-on: f3bc0d9fa2e7
---

# Flags And Settings Implementation Plan

**Goal:** A folder of this machine is refused after a flag and inside a setting a command names.

**Architecture:** Two additions to plan 04's check, both in `buildl-core`. `MachineFolders::find_in`
(`crates/buildl-core/src/types/machine_folders.rs`) also matches a folder after a flag, a `-` and
one or more letters at a word start (spec §6). `components::environment`'s neighbour in
`crates/buildl-core/src/plan/components.rs`, where E6 is checked, searches the value of each setting
a node's command names (`referenced`, `:136`) with the same `find_in`, and refuses a hit as E10,
exit 5 (spec §6, §7). The setting check runs where the key is gathered, so `plan`, `build`, `test`,
`verify` and `watch` refuse it; `check` and `graph` take no `--set` and do not.

**Tech Stack:** Rust 1.94, `buildl-core` (pure), `buildl-cli` (exit codes).

**Content authority:** `spec.md` §6 (the match, the flag rows, settings), §7 (E10), P22, P23.

---

## Files that change

```
crates/buildl-core/src/types/machine_folders.rs — modify: a flag before the folder is a boundary
crates/buildl-core/src/plan/components.rs       — modify: search the settings a command names (E10)
crates/buildl-core/src/error.rs                 — modify: E10 MachinePathInSetting
crates/buildl-cli/src/exit.rs                   — modify: E10 → exit 5, case counts
crates/buildl-core/tests/flows/machine_paths.rs — test: a setting holding home is refused
crates/buildl-cli/tests/cli/exit_codes.rs       — test: E10 exits 5, and a joined flag exits 5
```

## Order of work

Tasks 1 and 2 touch disjoint files and may run side by side.

### Task 1 — Match a folder after a flag

**Files:** Modify `crates/buildl-core/src/types/machine_folders.rs`.

**Change:** the start boundary of `find_in` also holds where the folder follows a flag: a `-` and one
or more ASCII letters, the `-` starting the text or following a character that cannot be part of a
path. The end boundary is unchanged.

**Proof:** written first, seen failing, then passing — one test per new row of spec §6's table, with
`HOME=/home/al`:
- `matches_after_a_joined_flag` — `-I/home/al/include`.
- `matches_after_a_long_joined_flag` — `-isystem/home/al/x` and `cc -L/home/al/lib`.
- `matches_after_an_equals` — `--prefix=/home/al` (guards today's behaviour).
- `does_not_match_a_longer_name_after_a_flag` — `-I/home/alice/x`.
- `does_not_take_a_dash_inside_a_name_for_a_flag` — `/opt/x-y/home/al`.
- `does_not_match_after_a_dot` — `-I./home/al`.
- the existing match-table tests stay green.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): find this machine's folders after a flag`.

### Task 2 — Refuse a setting that holds this machine's folder

**Files:** Modify `crates/buildl-core/src/plan/components.rs`, `error.rs`, `crates/buildl-cli/src/exit.rs`;
test in `crates/buildl-core/tests/flows/machine_paths.rs`, `crates/buildl-cli/tests/cli/exit_codes.rs`.

**Change:** as the key is gathered, each setting a node's command names is searched with
`MachineFolders::find_in`, in node order then setting-name order; the first hit is
`Error::MachinePathInSetting` (E10): `build.lua: //:setting: the setting prefix holds this machine's
path /Users/hiraq; it would give the target another key on every machine`. Exit 5. A setting no
command names is not searched. Its value is searched whether it is the `buildl.option` default or a
`--set` override.

**Proof:** written first, seen failing, then passing —
- `a_setting_holding_home_is_refused` (flow) — E10 with its message, from a `--set` value.
- `a_default_holding_home_is_refused` (flow) — the same from the default.
- `a_setting_no_command_names_is_not_searched` (flow) — no refusal.
- `a_setting_without_a_folder_is_keyed` (flow) — `jobs = "8"` passes.
- `machine_path_in_setting_exits_5` (in `exit_codes.rs`) — `buildl plan --set prefix=$HOME/opt` exits 5 with E10.
- `machine_path_after_a_joined_flag_exits_5` (in `exit_codes.rs`) — `run = { "echo", "-I$HOME/include" }`, `buildl check` exits 5 with E5 (spec P22).

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): refuse settings that hold this machine's folders`.

---

## Risks

- Task 1 widens a check that runs on every load; a false match blocks a workspace. The negative rows
  guard it, and plan 05's `cargo make examples` runs every example through it.
- A setting holding a path the user meant (an install prefix under their home) is now refused; the
  message names the setting so it can be moved inside the workspace or passed another way.

## Proof

- `cargo make dod` and `cargo deny check` pass.
- `cargo test -p buildl-core machine_folders` passes.

## Review findings

One reviewer pass over Tasks 1-2 (`03-reviewer-p08`, blocking: none), one fix round (`04-coder-p08-fix`):

- 🔵 the "one or more letters" part of the flag rule was untested — fixed: `-/home/al` and `-1/home/al` do not match.
- 🔵 `a_setting_without_a_folder_is_keyed` asserted only that `plan` is Ok, with `prefix=8` — fixed: `jobs = "8"`, and the setting is asserted in the key's components.
- 🔵 tests in `exit_codes.rs` and `error.rs` used the author's real home as the fake one — fixed: `/home/al`.
- 🔵 the flow module's doc line ran to 120 columns — fixed.
- 🔵 a joined option whose value is not a path (`sed -es/home/al/x/`) matches the flag rule and is refused — accepted: the spec's rule (§6), rare.
- Checked and holding: `"-I/home/al"` and `-Wl,-rpath,/home/al` match; `x-I/home/al`, `--I/home/al` do not; the setting check reads the effective value (default or `--set`), only settings a command names, in node then name order, inside `plan()`, so `plan`, discovery, `build`, `test`, `watch` and `verify` refuse.

## Deviations

- 2026-11-08 — `plan/assembly.rs` changed (not listed): it calls `check_settings` before `environment`, and the error-order doc of `plan()` gives both checks item 9 (a separate item trips `doc_lazy_continuation`).
- 2026-11-08 — `check_settings` takes `&MachineFolders`, not the host, so it is pure; E10 names the first node in graph order whose command names the setting.
