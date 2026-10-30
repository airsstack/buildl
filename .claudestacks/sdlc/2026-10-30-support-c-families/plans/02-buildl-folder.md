---
status: done
created: 2026-10-30
built-on: edb4e55efd6a
---

# `buildl.folder()` Implementation Plan

**Goal:** a build file, and a plugin it uses, can read the build file's folder through
`buildl.folder()`, as spec §3 defines.

**Architecture:** one function added to `primitives_table` (crates/buildl-lua/src/module.rs:129),
returning `results.directory()` as a Lua string. Every table built for one evaluation shares that
build file's `Results` (spec P15), so a plugin's `buildl.folder()` gives the calling build file's
folder with no further change.

**Tech Stack:** Rust (`buildl-lua`, mlua through airsl 0.1.4); mdBook.

---

## Files that change

```
crates/buildl-lua/src/module.rs                  — [modify] `folder` in primitives_table; unit tests
crates/buildl-lua/tests/flows/plugins.rs         — [modify] folder from a subdirectory build file and from a plugin
book/src/appendix-03-lua-api.md                  — [modify] `folder()` row, section, and the sample "buildl holds" line
skills/buildl/references/lua-api.md              — [modify] `folder()` row and section
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| `primitives_table` builds the `buildl` table; tables of one evaluation share `results` | spec P15 (module.rs:123-128, :129) |
| `Results::directory()` returns the build file's `Directory` | spec P15 (results.rs:39); `Results::new(discovered, directory)` at results.rs:25, `Results::none()` (test) at :34 |
| module.rs unit tests build a table with `Directory::root()` through the `evaluate` helper (:358) | read at commit f0f427d |
| flows tests already drive a plugin from a subdirectory build file: `sources_in_a_plugin_walks_the_calling_file_s_directory` (tests/flows/plugins.rs:541), `a_plugin_reads_discovered_relative_to_the_build_file` (tests/flows/discovery.rs:214) | read at commit f0f427d |
| the appendix's "buildl holds" line is the recorded output of a sample build file that lists `pairs(buildl)` | book/src/appendix-03-lua-api.md:24-36 |
| both function lists say "eleven functions" | book/src/appendix-03-lua-api.md:5; skills/buildl/references/lua-api.md:4 |
| the appendix's function table is at :7-19, sections `subdir` :176 and `sources` :186; the skill's lua-api.md table is at :7-19, sections from :130 | read at commit f0f427d |

## Order of work

### Task 1 — Add `buildl.folder()`

**Files:**
- Modify `crates/buildl-lua/src/module.rs` — register `folder` in `primitives_table`, a function taking no arguments that returns the folder of `results.directory()` as a string; update the module doc listing the primitives if it lists them
- Modify `crates/buildl-lua/tests/flows/plugins.rs` — the two flow tests below, built like `sources_in_a_plugin_walks_the_calling_file_s_directory`

**Change:** `buildl.folder()` returns `""` in the root build file and the folder (`"lib"`) in `lib/build.lua`, from the build file's table and from a plugin's.

**Proof:** written first, seen failing, then passing —
- `module.rs`: `folder_is_empty_at_the_root` — a build file at the root that declares a target whose name is built from `"x" .. buildl.folder()` stages the name `x`
- `tests/flows/plugins.rs`: `folder_names_a_subdirectory_build_file` — `lib/build.lua` declaring a target named after `buildl.folder()` stages it as `//lib:lib`
- `tests/flows/plugins.rs`: `a_plugin_reads_the_calling_build_file_s_folder` — a plugin function returning `buildl.folder()`, called from `lib/build.lua`, gives `lib`

**Done when:** `cargo make dod` and `cargo deny check` are green. Commit
`feat(buildl-lua): add buildl.folder()`.

### Task 2 — Document `buildl.folder()`

**Files:**
- Modify `book/src/appendix-03-lua-api.md` — "eleven functions" (:5) becomes twelve; a `folder()` row in the function table (returns a string); a `## folder()` section after `## sources(pattern)` saying what it returns at the root and below it and inside a plugin; the sample output (:30-36) replaced by the output of running that sample build file's `buildl check` with Task 1's binary, not edited by hand
- Modify `skills/buildl/references/lua-api.md` — "eleven functions" (:4) becomes twelve; the same row and a short section

**Change:** the book and the `buildl` skill describe `folder()`.

**Proof:** `cargo make book-build` succeeds; `cargo make skills` passes (its snippet check runs every `lua` block, so any example using `folder()` is checked against Task 1's binary).

**Done when:** both pass and `cargo make dod` is green. Commit
`docs(repo): document buildl.folder()`.

## Risks

- A plugin reading `folder()` depends on plugin tables sharing the caller's `Results`; `a_plugin_reads_the_calling_build_file_s_folder` pins it.
- Not done here: no other new primitive; no change to how commands resolve paths.

## Proof

`cargo make dod`, `cargo deny check`, `cargo make book-build`, `cargo make skills` green after Task 2.

## Review findings

- spec — `folder(1)` ignores the extra argument although spec §3 says it takes none — crates/buildl-lua/src/module.rs:204 — declined: every primitive takes its arguments as `Value` and ignores extras, so refusing them in `folder` alone would be inconsistent
- spec — spec §10 places the below-root and plugin `folder()` tests in module.rs; this plan put them in tests/flows/plugins.rs — same coverage, location only; recorded under Deviations
- no finding on the docs (appendix-03, skills lua-api.md)

## Probe results

- structural: `primitives_table` at crates/buildl-lua/src/module.rs:129; `Results::directory()` at crates/buildl-lua/src/results.rs:39 — `grep -n` found both
- behavioural: the named tests were the probes — `folder_is_empty_at_the_root` failed first with "attempt to call a nil value (field 'folder')", then passed with `folder_names_a_subdirectory_build_file` and `a_plugin_reads_the_calling_build_file_s_folder`
- the appendix's "buildl holds" line was re-run with `buildl check` on the sample build file using the built binary and pasted unedited (it now lists `folder`)
- gates: `cargo make dod` exit 0 (both batches), `cargo deny check` ok, `cargo make skills` exit 0, `cargo make book-build` green

## Deviations

- 2026-10-30 — plan: `folder_names_a_subdirectory_build_file` asserts the staged name `lib` (a staged target holds a bare name, not a label); the declaration is `//lib:lib` in the `lib` directory as the plan says
- 2026-10-30 — spec §10 (test placement): the below-root and plugin tests live in tests/flows/plugins.rs as this plan says, not in module.rs as spec §10's table lists; coverage is the same. Not a disproof of a behavioural claim; no marker written
