---
status: approved
created: 2026-10-29
depends-on: [01]
built-on: 1703d7070b45
---

# `buildl` Skill Implementation Plan

**Goal:** an agent that loads `skills/buildl/` can write build files `buildl check` accepts, act on
every exit code, and answer a refusal by asking the user.

**Architecture:** One skill folder in the Agent Skills format: a `SKILL.md` under 500 lines holding
the working loop, the exit-code table, the rules never broken, the common mistakes and a starter
build, and four reference files loaded on demand (spec §3). No `assets/`, no scripts. Every `lua`
block is a complete build file, so `cargo make skills` (plan 01) checks it.

**Tech Stack:** Markdown, YAML frontmatter, Lua build files.

---

## Files that change

```
skills/buildl/SKILL.md                — [create] frontmatter and the body spec §3.3 lists
skills/buildl/references/lua-api.md   — [create] the build file API, short, for an agent
skills/buildl/references/buildl-toml.md — [create] buildl.toml's tables, short
skills/buildl/references/exit-codes.md — [create] codes 0–7 with the text each prints
skills/buildl/references/plugins.md   — [create] buildl.use, extension.toml, plugins fetch
```

## Facts this plan relies on

Every fact the skill states is read, while writing it, from the book page named, not from the spec
or memory:

| Content | Read from |
|---|---|
| the eleven functions and their option tables, labels, placeholders, `sources` patterns, removed globals | book/src/appendix-03-lua-api.md (the eleven: :5-21; `$input`: :75-77; `print` removed: :236) |
| `buildl.toml` tables and keys; ceiling keys `proc.run`, `env.read`, `fs.read`, `fs.write` | book/src/appendix-02-buildl-toml.md (ceiling: :48-51) |
| exit codes 0–7, names, whose fault, sample text | book/src/appendix-04-exit-codes.md (:8-15 and each code's section) |
| commands and flags: `check`, `plan --json`, `build`, `test`, `log <TARGET>`, `doctor`, `clean`, `gc`, `query` | book/src/appendix-01-cli.md |
| approvers `manifest`, `interactive`, `deny`; the `no terminal to ask on` refusal | book/src/ch04-03-approvers.md:35-48, :70-77 |
| exit 4's three refusals — ceiling, plugin (`(plugin <name>@<version>)` in the error), approver; exit 3's causes | spec §1 P34, P35 (book/src/ch11-04-errors.md:181-182; ch08-03-how-buildl-decides.md:50-56, :66-75) |
| a program a dependency built needs no `proc.run` entry | spec §1 P36 |
| plugins: `buildl.use`, `extension.toml` fields, `[plugins.<name>]`, `buildl plugins fetch` | book/src/ch08-01-writing-a-plugin.md, ch08-02-plugins-from-git.md, examples/plugins/ |
| starter `run` and `env` lines; Rust needs `env = { "HOME", "PATH" }`, without `PATH` ``error: linker `cc` not found`` | examples/go/build.lua, examples/rust/build.lua:10-11, examples/rust/README.md:10-11, examples/python/build.lua |
| the frontmatter rules | spec §1 P1 (agentskills.io/specification.md:28-35, :58-67) |

## Order of work

### Task 1 — `SKILL.md`

**Files:**
- Create `skills/buildl/SKILL.md`

**Change:** the frontmatter of spec §3.2, verbatim. The body holds, in this order, spec §3.3's
parts A to F:
- A, how buildl works (declare, then buildl runs; the ceiling; a plugin's manifest and the
  workspace's cap for it; the approver; placeholders);
- B, the loop `check` → `plan` → `build` → `test`, as a diagram, saying `check` does not apply the
  ceiling, and what `plan --json` holds (per target its verdict, reason and key components; the
  build's `wanted`; tests only when named);
- C, the eight-row exit-code table with what the agent does for each — exit 3's missing program or
  failed plugin fetch reported, not retried;
- D, the four rules never broken (no widening of `[ceiling]`, a `[plugins.<name>]` cap or an
  `extension.toml` without the user's yes to that exact change); the three causes of exit 4 told
  apart by the error line and `doctor`, with what to tell the user for each; and the question the
  agent asks for a ceiling or cap refusal, after which it makes exactly the change agreed;
- E, the mistakes table;
- F, one starter `build.lua` and `buildl.toml` pair as `lua` and `toml` blocks, and the three-row
  table of per-language `run` lines with the `env` each needs (spec §3.3 F: Go `HOME`; Rust `HOME`
  and `PATH`);
- a closing list pointing at each file in `references/` with one line on when to read it.

A block showing a mistake on purpose (part E) is fenced `lua no-check` with the reason beside it.

**Proof:**
- `cargo make skills` passes: `skills-ref validate skills/buildl` accepts the folder, and the starter
  `build.lua` passes `buildl check`
- `wc -l < skills/buildl/SKILL.md` is below 500 (spec §1 P3)
- by hand, once: changing the frontmatter `name` to `buildl-x` makes `cargo make skills` fail at the
  `skills-ref` step; revert

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`feat(repo): add the buildl skill`.

### Task 2 — The four references

**Files:**
- Create `skills/buildl/references/lua-api.md`, `buildl-toml.md`, `exit-codes.md`, `plugins.md`

**Change:** each is short and written for an agent: tables of keys, types and defaults, one
example per function or table, and the error text an agent will meet. They restate the book
pages listed above and add nothing the book does not say. `buildl-toml.md` covers `[workspace]`,
`[ceiling]`, `[declaration]`, `[approver]`, `[execution]`, `[plugins.<name>]` and `[remote]` (spec
§3.4). Every `lua` example is a complete build file; a plugin's own `main.lua`, a build file
needing a plugin, or one calling `buildl.subdir` (it needs a child build file) is fenced
`lua no-check` with the reason beside it (spec §6.2).

**Proof:**
- `cargo make skills` passes, with every `lua` block in the four files checked
- by hand, once: a misspelt key put in one `lua` example makes `cargo make skills` fail, naming that
  file's folder; revert

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`docs(repo): add the buildl skill's references`.

## Risks

- The book pages are long; a reference that copies too much stops being short. Keep each reference
  to tables and one example per entry.
- `cargo make skills` proves every `lua` block is a valid declaration, not that the prose around it
  is right; the prose is held to the book pages above, read while writing.

## Proof

`cargo make skills` passes with `skills/buildl/` in place, and fails on each probe above.
