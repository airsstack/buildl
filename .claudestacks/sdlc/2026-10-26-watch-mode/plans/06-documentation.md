---
status: done
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

## Review findings

- doc accuracy — the building-blocks port-catalog intro still said a row with a later item "describes what is designed", covering the built `Changes`, `Cycles` and `WatchReport` rows — `docs/architecture-building-blocks.md`. Fixed: the built list names the item-7 rows.
- doc completeness — the building-blocks C4 L3 diagram (adapters, DATA, PORTS, LOGIC) and §5.1, §5.2, §5.4 left watch out where item 6 had added remote — `docs/architecture-building-blocks.md`. Fixed: `watch` adapter, the nine watch types, the three ports (with signatures in §5.2) and `react`, `watch`, `watched_inputs` rows added; diagrams stay Mermaid.
- doc accuracy — §10's composition-root row named only the root and approvals directory as kept across cycles — building blocks. Fixed: the search path too.
- doc accuracy — "until a window passes with no kept path" did not match `gather`, whose wait any event extends — `docs/architecture.md` §3.9. Fixed: "no event, a dropped path included".
- citation — notify's wrapper ban cited building blocks §7; the text is in §4 — `docs/architecture.md` §6. Fixed; the unverifiable "newest stable release" clause dropped.
- doc accuracy — design §11.1 still called watch mode "already designed" — `docs/design.md`. Fixed: built, keeping no stat cache warm.
- consistency — README's row read `buildl watch [target]` and omitted the graph-change trigger — `README.md`. Fixed: `[target]...` and the trigger as design §7 states it.
- Verified: `grep -n -i -E "watch.*(not yet|not built|not implemented)|(not yet|not built|not implemented).*watch" docs/*.md` prints nothing (exit 1); `grep -n "^### 3.9" docs/architecture.md` → `456:### 3.9 \`watch\` — the loop around builds`. Task 2's `cargo make dod` green (coder, run once); the fix round touched no Rust file.

## Probe results

- No throwaway probe: documentation only. Every watch claim was checked against the code by the reviewer, by reading (ignored directories dropped first, trigger naming kept paths, check pass, rescan → exit 3, per-cycle reopen refusing a manifest above the root, lock per cycle inside the build, graph without the lock, no signal handler). The "seventeen modules — sixteen adapters, and `plugins`" count was checked against `crates/buildl/src` and the guard's directory listing.

## Deviations

- 2026-10-26 — Beyond the plan's list, so no document contradicts the build: design's status line, §2 diagram label and §7 "are built" list; both adapter-isolation records' module counts; the building-blocks status line, crate dependency table, module table, port catalog, C4 L3 diagram and §5.1/§5.2/§5.4 (review fix); design §11.1 (review fix).
- 2026-10-26 — Two status sentences reworded because they matched the plan's grep ("Everything else from item 2 on … not built"; "Remote execution and watch are designed for but not implemented").
- 2026-10-26 — Tasks 1 and 2 ran in parallel (disjoint files).
