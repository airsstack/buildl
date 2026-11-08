---
status: done
created: 2026-11-07
depends-on: [01]
built-on: 55ab2847b726
---

# Lua Env Marks Implementation Plan

**Goal:** A build file declares every variable with an explicit `key` or `unkeyed` mark.

**Architecture:** As names are today, marks are staged by Lua and checked by the core, so E1 and E2
carry the core's `build.lua: //:app:` prefix (`crates/buildl-core/src/load/traversal.rs:276-278`),
and E9 keeps the existing name refusal's `build.lua: declaration #0: invalid env:` form,
rather than a Lua-side `line 1: buildl.target 'app':` one
(`crates/buildl-lua/tests/flows/failures.rs:130-131`). `buildl-lua` reads `env` on `target`,
`test` and `discover` (`crates/buildl-lua/src/primitives.rs:24-34`, today `:77`) into staged entries:
each a written name with a written mark, or a list entry with no mark, or a mark that is not a
string. The staging step in the core turns them into plan 01's map and refuses E1, E2 and E9
(spec §2, §7). The crates' fixtures move to the map form in the same commit.

**Tech Stack:** Rust 1.94, airsl `0.1` (only in `buildl-lua`), `buildl-core` load.

**Content authority:** `spec.md` §2, §7 (E1, E2, E9).

---

## Files that change

```
crates/buildl-core/src/types/build_file.rs   — modify: StagedTarget.env becomes staged entries (:148)
crates/buildl-core/src/load/traversal.rs     — modify: entries → map; E1, E2, E9 (:276-278)
crates/buildl-core/src/error.rs              — modify: E1 EnvNotMarked, E2 EnvBadMark, exit 5
crates/buildl-lua/src/env_marks.rs           — create: stage an `env` table's entries
crates/buildl-lua/src/lib.rs                 — modify: declare the module
crates/buildl-lua/src/primitives.rs          — modify: use env_marks (:77); fixtures (:186, :202)
crates/buildl-lua/tests/fixtures/every_field/build.lua (:16), crates/buildl-lua/tests/flows/discovery.rs (:91),
crates/buildl/tests/fixtures/chain/build.lua (:8), crates/buildl/tests/e2e/approver.rs (:89-90, :113),
crates/buildl/tests/e2e/doctor.rs (:86-87), crates/buildl-cli/tests/cli/arguments.rs,
crates/buildl-cli/tests/cli/exit_codes.rs    — modify: the map form; exit-code cases for E1, E2, E9
```

## Order of work

### Task 1 — Stage and check `env` marks, and move the crates' fixtures

**Files:** All of the block above.

**Change:** `env_marks::stage(value)` stages, for an `env` table, one entry per pair: a string key
with a string value becomes (written name, written mark); an integer key, which is what a list
entry has (`{ "HOME" }`, `{ [1] = "key" }`), becomes a list entry; a string key with a non-string
value (`true`) becomes a not-a-string mark; any other key type is staged as a bad name. An absent
`env` or `{}` stages nothing. The core's staging step then refuses: any list entry, so also a mixed
table `{ "HOME", PATH = "unkeyed" }`, as E1 (`env must mark each variable "key" or "unkeyed", such
as env = { HOME = "unkeyed" }`); a mark other than `"key"` / `"unkeyed"`, or not a string, as E2
(`env HOME is marked "pass"; mark it "key" or "unkeyed"`); a bad name with the existing
`Written<EnvName>` refusal (E9). E1 wins when a table has both list entries and bad marks. Every
fixture and inline build file that declares `env` in the files above moves to the map form
(`HOME`/`PATH` that only find programs become `"unkeyed"` where the test asserts nothing about
their value), so the gate is green at the commit.

**Proof:** written first, seen failing, then passing —
- `env_table_of_marks_is_read` — `{ RUSTFLAGS = "key", HOME = "unkeyed" }` gives both marks.
- `empty_or_absent_env_declares_nothing` — `{}` and no `env` give an empty map.
- `env_list_is_refused` — `{ "HOME" }` gives E1 with its message and the `build.lua: //:app:` prefix.
- `env_integer_key_is_a_list_entry` — `{ [1] = "key" }` gives E1.
- `env_mixed_table_is_refused` — `{ "HOME", PATH = "unkeyed" }` gives E1.
- `env_bad_mark_is_refused` — `"pass"` and `true` give E2 naming the variable and the mark.
- `env_bad_name_is_refused` — `"A-B"` gives the `EnvName` refusal (E9).
- `env_on_test_and_discover_is_read_the_same` — `test` and `discover` take the same table.
- in `crates/buildl-cli/tests/cli/exit_codes.rs`: `env_list_exits_5`, `env_bad_mark_exits_5`, `env_bad_name_exits_5`.
- the existing tests of every fixture file above, green on the map form.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-lua): read env as a table of key and unkeyed marks`.

---

## Risks

- One commit changes the parser and every fixture; a fixture missed by the list above shows up as
  a red test in the gate before the commit, not after.
- Not done here: examples, plugins and skills (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` pass.
- `git grep -n -E "env *= *\{ *(\\\\?[\"']|\[)" -- crates` prints nothing (the list form in Lua files and in Rust strings, `"`, `\"` or `'`).

## Review findings

One reviewer pass (`09-reviewer-p02`), one fix round (`10-coder-p02-fix`):

- 🟡 list entries in `env` counted 0 bytes against the staging budget — fixed: each entry holding no text is charged a fixed record size; `staging::tests::charges_every_env_entry_that_holds_no_text` and flow `failures::an_env_list_shared_by_many_targets_reaches_the_staging_limit` red, then green.
- 🟡 a non-UTF-8 `env` key was refused as "must be a string" — fixed: `StagedEnv::NotUtf8Name`, refused as "must be valid UTF-8"; core and Lua tests.
- 🔵 "an userdata" → "a userdata"; the name-order test uses 26 names; the one-helper `flows/staged_env.rs` folded into `flows/common.rs` — fixed.
- 🟡 the Architecture premise that every refusal carries the `//:app:` prefix — amended in this plan (E9 keeps the existing `declaration #0: invalid env:` form).

## Deviations

- 2026-11-07 — files beyond the list: `crates/buildl-cli/src/exit.rs` (E1, E2 → exit 5), `buildl-core/tests/flows/{discovery,doctor,plan,main}.rs`, `buildl-lua/src/{staging,values}.rs`.
- 2026-11-07 — Lua stages entries sorted (marked names in name order, then list entries, then non-name keys), so the first refusal does not hang on `pairs` order.
