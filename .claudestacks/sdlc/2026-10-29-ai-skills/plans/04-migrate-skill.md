---
status: approved
created: 2026-10-29
depends-on: [02, 03]
built-on: 45bb359e2387
---

# `buildl-migrate` Skill Implementation Plan

**Goal:** an agent that loads `skills/buildl-migrate/` can move a Makefile, justfile, shell or npm
build to buildl and prove it with the parity check, leaving the old build untouched.

**Architecture:** The skill folder `skills/buildl-migrate/` gains its `SKILL.md` and four per-tool
references beside the `scripts/` plan 03 wrote (spec §4). `SKILL.md` holds the migration steps, the
exact prepare → run → compare commands, how to write `parity.json`, and the two stop rules; the
build-file rules themselves stay in the `buildl` skill (plan 02), which `SKILL.md` names rather than
repeats.

**Tech Stack:** Markdown, YAML frontmatter, Lua build files.

---

## Files that change

```
skills/buildl-migrate/SKILL.md            — [create] frontmatter and the migration procedure
skills/buildl-migrate/references/make.md  — [create] Make features → buildl
skills/buildl-migrate/references/just.md  — [create] just features → buildl
skills/buildl-migrate/references/shell.md — [create] shell build scripts → buildl
skills/buildl-migrate/references/npm.md   — [create] npm scripts → buildl
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| the frontmatter, verbatim | spec §4.2 |
| the commands of each step, with their grants and the absolute-`W`-outside-`P` rule | spec §4.4 |
| `parity.json`'s fields and the `judge`/`reason` entry | spec §4.5; validated by `parity_lib.config` (plan 03 Task 2), which `SKILL.md` names as the authority |
| the checks and their verdicts | spec §4.6; `parity_lib.rows` (plan 03 Task 3) |
| the two stop rules | spec §4.7 |
| buildl v1 builds no per-`.o` C/C++ targets; the whole-program shape | book/src/ch11-09-what-it-promises.md:20-27; spec §1 P33 |
| the recipe forms `compare` reads per tool | `parity_lib.recipes` (plan 03 Task 1) |
| step 2 writes `buildl graph > W/new.graph.json` (every target, tests included) and `buildl doctor > W/new.doctor.txt`; no `plan --json` | spec §4.4 step 2; §1 P16 |

## Order of work

### Task 1 — `SKILL.md`

**Files:**
- Create `skills/buildl-migrate/SKILL.md`

**Change:** the frontmatter of spec §4.2, verbatim. The body, in this order:
1. What the skill does and what it never does (edit or delete the old build, `parity.json`, `W`).
2. The steps: read the old build; write `build.lua` and `buildl.toml` under the `buildl` skill's rules;
   list `old.programs`, `old.env` and `old.network` recipe by recipe; list in `skip` the
   intermediate files an earlier run left in `P` (`*.o`, `build/`, `dist/`); write `parity.json`.
   `"judge": "tests"` is used only for the C/C++ case of spec §4.7.
3. The parity check: the three steps of spec §4.4 as fixed commands, with `<skill dir>` for the
   script's folder, `P` and `W` absolute and `W` outside `P`, and step 2's seven commands exactly as
   spec §4.4 lists them (status files, `buildl graph > W/new.graph.json`, `buildl doctor >
   W/new.doctor.txt`). Under the `interactive` or `deny` approver, `buildl build` in `W/new` is
   refused (spec §4.4, P38): the agent asks the user to run the `W/new` commands in their own
   terminal, and never changes `[approver]`.
4. Reading the table: each verdict and what the agent does — a FAIL is fixed in the new build, never
   in `parity.json` to make it pass.
5. The two stop rules of spec §4.7, with the question the agent asks in each; the C/C++ offer says,
   before the user answers, that a project with no test command cannot pass parity this way.
   Plan 03 Task 5's `examples/python` run decides whether this part also says how to treat
   time-stamped archives.
6. A closing list pointing at each `references/` file.

**Proof:**
- `cargo make skills` passes: `skills-ref validate skills/buildl-migrate` accepts the folder, and its
  `lua` blocks pass `buildl check`
- `wc -l < skills/buildl-migrate/SKILL.md` is below 500
- by hand, once: following `SKILL.md` step 3 literally over a copy of
  `skills/tests/fixtures/compare/match/P` — a real `cp`/`grep` project with a test recipe mapped to a
  test target (plan 03 Task 5) — into a fresh absolute `W` ends in `parity: PASS` — a probe; it
  leaves no file in the repository

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`feat(repo): add the buildl-migrate skill`.

### Task 2 — The four tool references

**Files:**
- Create `skills/buildl-migrate/references/make.md`, `just.md`, `shell.md`, `npm.md`

**Change:** each maps the tool's features to buildl, as a table: a rule or recipe → `target` or
`test`; a pattern rule → `rule`; recursive make, or a script calling scripts in other folders →
`subdir`; a download → `network = true`; an environment variable read → `env` and `env.read`; a
phony or non-build recipe → the stop rule of spec §4.7. `make.md` holds the per-file C/C++ case and
the whole-program target. Each states which recipe forms `compare` reads for that tool, as
`parity_lib.recipes` implements them, and that other forms are not seen by check 1. Every `lua`
example is a complete build file.

**Proof:**
- `cargo make skills` passes with every `lua` block in the four files checked

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`docs(repo): add the buildl-migrate skill's references`.

## Risks

- `SKILL.md` is instructions to an agent; nothing here measures how well an agent follows it (spec
  §9 leaves evaluations out). The by-hand run in Task 1 proves only that the commands, followed
  literally, work.
- The commands in step 3 are POSIX shell run by the agent; they assume `W` holds no spaces. `SKILL.md`
  says to pick such a `W`.

## Proof

`cargo make skills` passes with both skills in place, and the by-hand walk of Task 1 ends in
`parity: PASS`.
