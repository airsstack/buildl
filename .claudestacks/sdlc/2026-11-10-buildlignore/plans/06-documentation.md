---
status: done
created: 2026-11-10
depends-on: [03, 04, 05]
built-on: 6643cfeb82a2
---

# Documentation Implementation Plan

**Goal:** The book, the rust plugin's README and the `buildl` skill describe `.buildlignore` as built.

**Architecture:** Prose and tables only, in the files spec §9 lists. Every example output is copied
from a run of the built binary, not written by hand. The book keeps Mermaid where it uses it and
keeps numbered rules on `ch11-03` and `ch11-05` (CLAUDE.md).

**Tech Stack:** mdBook (`cargo make book-build`), Markdown, the `skills` checks (`cargo make skills`).

**Content authority:** `spec.md` §9; the behaviour described is §2, §5 and §6.

---

## Files that change

```
book/src/ch03-02-inputs-outputs-deps.md   — modify: a section "Leaving paths out: .buildlignore" after "Inputs, and buildl.sources" (:47)
book/src/ch10-00-watch.md                 — modify: "What starts a build" (:69) names ignored paths
book/src/appendix-03-lua-api.md           — modify: subdir (:206), sources (:216-226) and the inputs option against ignored paths
book/src/appendix-02-buildl-toml.md       — modify: walk_entries (:67) does not count ignored folders
book/src/appendix-04-exit-codes.md        — modify: code 5 bullets (:172-189) gain the five messages of spec §5
book/src/ch11-03-four-crates.md           — modify: the port catalog (:420) and the adapter list gain IgnoreRules and ignore_rules
book/src/ch11-04-errors.md                — modify: the variant table (:129-142) gains Ignored and IgnoreFile
plugins/rust/README.md                    — modify: inputs row (:84) and workspace note (:145) mention .buildlignore
skills/buildl/SKILL.md                    — modify: where it describes sources (:118)
skills/buildl/references/lua-api.md       — modify: sources (:16-25) and subdir against ignored paths
book/src/appendix-01-cli.md               — modify (amended): the watch paragraph names ignored paths
book/src/ch08-05-the-rust-plugin.md       — modify (amended): inputs row, as the plugin README
book/src/ch11-06-decision-records.md      — modify (amended): LuaSource::new also takes the ignore rules
skills/buildl/references/buildl-toml.md   — modify (amended): walk_entries does not count ignored folders
```

## Order of work

### Task 1 — The user chapters and appendices

**Files:**
- Modify `book/src/ch03-02-inputs-outputs-deps.md`, `ch10-00-watch.md`, `appendix-03-lua-api.md`,
  `appendix-02-buildl-toml.md`, `appendix-04-exit-codes.md`

**Change:** `ch03-02` gains the section: what a `.buildlignore` is, nesting, the syntax rules of
spec §2.1 including the BOM and literal braces, the root-down rule with the `notes/` and
`!notes/keep/` example, that files above the root and `.gitignore` are never read, that a
`.buildlignore` is listed like any file, and a console run showing a target staying clean after an
ignored file is edited. `ch10-00` adds ignored paths to what starts no build. `appendix-03` adds to
`sources`, `inputs` and `subdir` what each does with an ignored path. `appendix-02` notes that
ignored folders cost no `walk_entries`. `appendix-04` lists the five messages of spec §5 (ignored
input, ignored subdir, bad pattern, not UTF-8, unusable file) under code 5 with a run of each. A
search for `sources(` in the root `README.md` confirms it describes nothing this changes, so it
stays as it is (CLAUDE.md keeps it consistent with the book).

**Proof:** no test: prose. Checked by the book build and by reading each claim against spec §2-§6.

**Done when:** `cargo make book-build` succeeds and `cargo make dod` and `cargo deny check -D unused-wrapper` are green. Commit `docs(repo): describe .buildlignore in the book`.

### Task 2 — Under the hood, the plugin README and the skill

**Files:**
- Modify `book/src/ch11-03-four-crates.md`, `book/src/ch11-04-errors.md`, `plugins/rust/README.md`,
  `skills/buildl/SKILL.md`, `skills/buildl/references/lua-api.md`

**Change:** `ch11-03` lists the `IgnoreRules` port in the catalog and the `ignore_rules` adapter
among `buildl`'s adapters, and says the walk reaches it through the port. `ch11-04` adds the two
variants to its table with their exit code. The rust plugin README's inputs row and workspace note
say a `.buildlignore` keeps folders such as documentation out of every target's inputs. The skill
says the same where it describes `sources`, so an agent writing a build file suggests a
`.buildlignore` instead of hand-filtering `sources`.

**Proof:** no test of its own: `cargo make skills` checks every `lua` block the skill holds still
passes `buildl check`.

**Done when:** `cargo make book-build` and `cargo make skills` succeed and `cargo make dod` and `cargo deny check -D unused-wrapper` are green. Commit
`docs(repo): describe .buildlignore under the hood, in the rust plugin and in the skill`.

---

## Risks

- `README.md` at the root must stay consistent with the book (CLAUDE.md); it does not describe
  `sources`, so it needs no change, which Task 1 confirms by a search for `sources(` in it.
- `cargo make skills` needs `airsl`, `skills-ref` and `shellcheck` on the machine; without them the
  task's check cannot run and the execution record says so.

## Proof

- `cargo make book-build` and `cargo make skills` succeed; every console block in the new text was
  produced by the built binary.

## Review findings

One reviewer pass over 63ae2b0..1ea0aae: 0 blocking. All ten 🔵 fixed in b6665c6; `cargo make book-build`, `cargo make skills`, `cargo make dod`, `cargo deny check -D unused-wrapper` → green.

- 🟡 four files changed outside the plan's list — amended in "Files that change".
- 🔵 appendix-03: an ignored declaring folder is unreachable (load refuses an ignored `subdir`) — fixed.
- 🔵 appendix-04: the `Ignored` message's ` (plugin <chain>)` suffix was missing — fixed.
- 🔵 ch03-02: `watch` drops a change to an ignored `build.lua` or `buildl.toml` — added.
- 🔵 ch10-00, ch11-03: when edited `.buildlignore` lines apply (read lazily per open, at the latest from the next open) — fixed.
- 🔵 ch10-00: the loop calls `react` with no rules after `kept` filtered — diagram and prose fixed.
- 🔵 overlong edited lines in ch10-00, ch11-03, appendix-01 — rewrapped.
- 🔵 ch11-03: "after the milestones" → "outside any milestone" — fixed.
- 🔵 ch11-03: `Cycles` catalog row names `type Ignore` and `rules()` — fixed.
- 🔵 ch11-03: `NothingIgnored` named beside `NoRemote` — fixed.
- 🔵 skills/buildl/SKILL.md: quoted message in its general form — fixed.
- Deviation "What starts a build" table without rows: reviewer judged the requirement met (Mermaid drop node, bullet and paragraph); the table is the recorded run of one session.

## Deviations

- 2026-11-10 — Files outside the list, made stale by plans 04 and 05: `appendix-01-cli.md` (watch paragraph), `ch08-05-the-rust-plugin.md` (inputs row), `ch11-06-decision-records.md` (`LuaSource::new(root, limits, skipped, ignore, plugins)`), `skills/buildl/references/buildl-toml.md` (`walk_entries`). Committed with Task 1 (appendix-01) and Task 2 (the rest).
- 2026-11-10 — ch10-00 "What starts a build": the table is a recorded watch session; no row added, since a row there would not come from a run. Ignored paths are named in the drop node, the batch bullet and a new paragraph.
- 2026-11-10 — appendix-04 also shows the walk route of a bad pattern as its own run, since its text (`buildl.sources: …` inside an evaluation error) differs from the load route.
- 2026-11-10 — Tasks 1 and 2 ran in one coder with one gate run, committed separately (22b2c9b, 1ea0aae).
