---
status: approved
created: 2026-10-26
depends-on: [05]
built-on: a08d695c7505
---

# Watch Documentation Implementation Plan

**Goal:** The design, architecture, README and `CLAUDE.md` describe watch mode as built.

**Architecture:** Documentation only, the list of spec §9. Diagrams in `docs/` stay Mermaid
(`CLAUDE.md`, "Chat ASCII, document Mermaid"); section numbers and cross-references are
kept (`CLAUDE.md`, "Section cross-references … are load-bearing"). New design content cites
into each document's numbered reference list.

**Tech Stack:** Markdown, Mermaid, `cargo make dod` (the crate docs build under `-D warnings`).

**Content authority:** spec §9; the behaviour as plans 01-05 built it.

---

## Files that change

```
docs/design.md                         — [modify] §7 table, §8.6 text and diagram, §13 item 7
docs/architecture.md                   — [modify] status line (:3), the watch loop beside the pipeline (§3), §6 decision records
docs/architecture-building-blocks.md   — [modify] watch as an embedder of the composition root (§10 row :711), notify in the dependency view
README.md                              — [modify] status paragraph (:7), command table row (:187), roadmap item 7 (:259)
CLAUDE.md                              — [modify] Status: Milestone 7 built
crates/buildl/src/lib.rs, crates/buildl-cli/src/main.rs — [modify] crate docs, where they list commands or modules
```

---

### Task 1 — Design and architecture documents

**Files:**
- Modify `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md`

**Change:**
- design §7 table (`docs/design.md:292`): the `watch` row describes the check pass in a phrase.
- design §8.6 (`:432-446`): the Mermaid diagram gains the ignore step and the check pass
  (unchanged graph → wait); the text replaces "Events invalidate stat-cache entries" with what
  is built (the stat cache needs nothing from watch: a changed file's stat differs) and
  states the reference, the check pass, the ignored directories, and that each cycle opens
  the workspace again.
- design §13 item 7 (`:955`): "Built."
- architecture status line (`docs/architecture.md:3`): watch built; remote execution remains.
- architecture §3: a subsection `3.9 watch` after §3.8 (`:394`): `Changes`, `Cycles`,
  `WatchReport`, `react`, `Reopening`, `NotifyChanges`, and why the lock is taken per cycle.
- architecture §6 (`:550`): the three decision records spec §9 names — `notify` 8.2.1 with
  its single-crate CC0 exception and its place among `walkdir`'s wrappers; the check pass over
  event kinds (kinds are merged on macOS); the lock taken per cycle, which needs the
  workspace opened again per cycle and leaves `plan`, `log`, `graph` and `doctor` usable
  between cycles.
- building blocks: §10's composition-root row (`docs/architecture-building-blocks.md:711`)
  names watch as built; `notify` joins wherever the document lists external dependencies.

**Proof:** `grep -n -i -E "watch.*(not yet|not built|not implemented)|(not yet|not built|not implemented).*watch" docs/*.md`
prints nothing; every `§` reference touched still resolves to its heading.

**Done when:** that `grep` prints nothing and `grep -n "3.9" docs/architecture.md` finds the
new subsection. Commit `docs(repo): describe watch mode as built`.

### Task 2 — README, CLAUDE.md and crate docs

**Files:**
- Modify `README.md`, `CLAUDE.md`, the crate docs listed above

**Change:** README status paragraph (`README.md:7`) adds watch mode from the seventh item;
the command table row (`:187`) drops "not yet"; the roadmap line (`:259`) says built.
`CLAUDE.md` Status gains one sentence for Milestone 7 in the voice of its neighbours: `buildl
watch` builds, then builds again when a file the build reads changes, reading the workspace
anew each cycle. Crate docs of `buildl` and `buildl-cli` that enumerate commands or modules
gain `watch` (`buildl-core`'s module table and rule 1 were updated by plan 01 Task 4).

**Proof:** `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes; README and
the docs say the same thing about watch.

**Done when:** the stack's Definition of Done is green. Commit `docs(repo): mark watch mode
built in the README and CLAUDE.md`.

---

## Risks

- The milestone file `.claudestacks/sdlc/roadmaps/buildl/07-watch-mode.md` is not edited here:
  closing it is the `milestone` skill's roll-up once the chain is done (spec §9).

## Proof

`cargo make dod` green; no document calls watch unbuilt.
