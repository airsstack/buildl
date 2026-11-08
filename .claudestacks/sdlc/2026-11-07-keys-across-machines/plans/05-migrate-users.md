---
status: done
created: 2026-11-07
depends-on: [02, 03, 04, 08]
built-on: f3bc0d9fa2e7
---

# Migrate Users Implementation Plan

**Goal:** Every plugin, example and skill outside the crates declares its variables with marks and allows the unkeyed ones.

**Architecture:** No Rust. The `c` plugin, the examples (except `examples/remote`, plan 06) and the
skills move from `env = { "X" }` to the map form, and their `buildl.toml`/`extension.toml` gain
`env.unkeyed` for `HOME` and `PATH`, which only find programs (spec §8). Each is checked by its own
existing runner.

**Tech Stack:** Lua build files, TOML, Markdown; `cargo make plugins`, `examples/check.sh`,
`cargo make skills`.

**Content authority:** `spec.md` §8.

---

## Files that change

```
plugins/c/graph.lua:191,201, plugins/c/extension.toml, plugins/c/README.md:191,197, plugins/c/tests/graph_test.lua
examples/{go,go-discovery,plugins,polyglot,rust,c,cpp,python}/buildl.toml and their build.lua,
examples/plugins/plugins/gobin/{main.lua,extension.toml}, examples/c/README.md, examples/cpp/README.md
skills/buildl/SKILL.md, skills/buildl/references/{lua-api,plugins,buildl-toml,exit-codes}.md
skills/buildl-migrate/SKILL.md, skills/buildl-migrate/references/{make,npm,just,shell}.md,
skills/buildl-migrate/scripts/parity_lib.lua
skills/tests/parity_test.lua, skills/tests/snippets.lua, skills/tests/fixtures/compare/**
```

## Order of work

### Task 1 — Mark `PATH` unkeyed in the `c` plugin

**Files:** Modify `plugins/c/graph.lua:191,201`, `plugins/c/extension.toml`,
`plugins/c/README.md:191,197`, `plugins/c/tests/graph_test.lua`.

**Change:** the link and archive steps declare `env = { PATH = "unkeyed" }`; the manifest gains
`env.unkeyed = ["PATH"]`; the README's setup shows `env.unkeyed = ["PATH"]` in the ceiling and the
plugin table, and says why.

**Proof:** written first, seen failing, then passing — `graph_test.lua`'s expectations of the
declared `env` changed first; `cargo make plugins` passes.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): mark PATH unkeyed in the c plugin`.

### Task 2 — Mark the examples' variables

**Files:** Modify the examples listed above.

**Change:** `HOME` (go, go-discovery, plugins with gobin, polyglot, rust) and `PATH` (rust,
polyglot, c, cpp) become `"unkeyed"`; each `buildl.toml` and `gobin/extension.toml` gains
`env.unkeyed`; the READMEs show the new form. No `HOME` or `PATH` stays `key`, since its value is
this machine's folder and plan 04 refuses it (E6).

**Proof:** written first, seen failing, then passing — `cargo make examples`
(`Makefile.toml:363-381`, which builds the binary, sets `BUILDL` and runs `examples/check.sh` on
each example but `remote`) fails on the list form after plan 02, and passes after the edit, every
example building and testing as before.

**Done when:** `cargo make examples`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): mark the examples' variables key or unkeyed`.

### Task 3 — Teach the marks in the skills

**Files:** Modify the skills files listed above.

**Change:** every `env = { … }` in the skills' text and tables uses the map form; the `buildl`
skill says a variable that only finds programs (`HOME`, `PATH`) is `unkeyed`, allowed by
`env.unkeyed`, and why (sharing the cache between machines); `exit-codes.md` lists E1-E10;
`buildl-migrate`'s references map a recipe's variables to `key` and its parity script reads
`env.unkeyed` beside `env.read`, with its fixtures and tests.

**Proof:** written first, seen failing, then passing — `skills/tests/parity_test.lua` gains a case
reading `env.unkeyed`; `cargo make skills` passes.

**Done when:** `cargo make skills`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): teach key and unkeyed variables in the skills`.

---

## Risks

- `cargo make examples` builds real toolchains (Go, Rust, C); its time is the cost of Task 2's check.
- Not done here: `examples/remote` (plan 06), the book (plan 07).

## Proof

- `git grep -n -E "env *= *\{ *[\"'\[]" -- plugins examples skills` prints nothing.
- `cargo make plugins`, `cargo make skills`, `cargo make examples` pass.

## Review findings

One reviewer pass over Tasks 1-3 (`04-reviewer-p05`, blocking: `skills/buildl/SKILL.md:98`), one fix round (`05-coder-p05-fix`, `cargo make skills` green):

- 🔴 the skill said `HOME`/`PATH` marked `key` "works but misses the cache"; a value holding this machine's folder is refused (E6, exit 5) — fixed: the row gives the E6 message and says mark it `unkeyed`.
- 🔵 `exit-codes.md` lead-in said every case names a target; E4 names a toml table, E9 a declaration — fixed.
- 🔵 the E4 fix offered an edit to a plugin's `extension.toml` — fixed: the manifest is the plugin author's.
- 🔵 example texts used the author's real home — fixed: `/home/alice`.
- 🔵 the E9 row had no error text — fixed: `build.lua: declaration #0: invalid env: invalid environment variable name: "A-B" — may hold only ASCII letters, digits and '_'` (`env_name.rs:32`).
- 🔵 the starter `buildl.toml` lacked the `env.unkeyed` line the prose named — fixed: `env.unkeyed = []`.
- 🔵 `proc.run` not realigned (`SKILL.md`, `plugins/c/extension.toml`) — fixed.
- 🔵 new prose lines past 100 columns (examples READMEs, gobin, c README, migrate skill, parity_lib) — fixed; the go README's code span kept on one line.
- 🔵 parity check 6's `env.unkeyed` row cannot fail alone on real doctor output (buildl refuses it as E4 first) — accepted: the plan asked for the check.
- Gates: `cargo make plugins` 73 passed; `cargo make examples` c, cpp, go-discovery, go, plugins, polyglot, python, rust pass; `cargo make skills` green; `cargo make dod` exit 0; `cargo deny check` "advisories ok, bans ok, licenses ok, sources ok".

## Deviations

- 2026-11-08 — `examples/c/prepare.sh` and `examples/cpp/prepare.sh` changed (not listed): they match the `[plugins.c]` table's `github` line by exact spacing, which re-aligning the table for `env.unkeyed` changed.
- 2026-11-08 — `examples/go/README.md` and `examples/rust/README.md` changed (not listed): they showed the old form.
- 2026-11-08 — the skills' fixtures `skills/tests/fixtures/compare/{match,mismatch}/W/new.{doctor.txt,graph.json}` gained the `env.unkeyed` doctor line and the map-form `"env":{}`; `skills/tests/snippets.lua` needed no change.
