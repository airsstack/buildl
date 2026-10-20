---
status: done
created: 2026-10-19
depends-on: [02]
built-on: 62323287dd81
---

# Parallel Load Documents Implementation Plan

**Goal:** The documents describe parallel load as built.

**Architecture:** Documents only. Section numbers and cross-references stay as they are; Mermaid
blocks stay Mermaid (CLAUDE.md, "Chat ASCII, document Mermaid"). Line numbers are from commit
`bc6bea6`, before the clean-gc chain's documents task; `execute` re-greps each anchor phrase
quoted below before editing.

**Tech Stack:** Markdown.

**Content authority:** spec §5.

---

## Files that change

```
docs/architecture.md                   — [modify] §3 table, §3.1, §4 error table, §5 rule 3, §6 record
docs/design.md                         — [modify] §4 lead sentence, §8.1, §11.1
docs/architecture-building-blocks.md   — [modify] :148, :191, :415, :425, :433, :445, :530
README.md                              — [modify] :7 status
CLAUDE.md                              — [modify] Status
```

## Order of work

### Task 1 — Describe parallel load as built

**Files:**
- Modify `docs/architecture.md`:
  - §3 phase table, row `§3.1 \`declare\`` (`:190`): ports `DeclarationSource`, `Dispatcher`.
  - §3.1 paragraph at `:208` ("Load evaluates build files one at a time today … is its design"):
    the built design — the queue runs through the `Dispatcher` port on up to `[execution] workers`
    threads; each subdirectory is handed out as soon as a result names it; a serial replay over
    the results decides the declarations and the error (serial's first failure, serial's
    `requested_by`); an `AtomicBool` stops new evaluations once the replay meets that error; the
    sorted merge and the determinism rule sentence stay.
  - §4 lead sentence (`:315`, "Eight kinds carry data a caller branches on"): "Nine kinds".
  - §4 error table (`:316-322`): a `LostBuildFiles` row beside `DispatcherFault` — raised when the
    dispatcher returned and left a build file without a result; carries `directories`, in order.
  - §5 rule 3 (`:352`): drop "— Load is sequential today, so this clause has nothing to hold yet".
  - §6 record "A dispatcher that loses a task" (`:375`): names `Error::LostBuildFiles` for Load
    beside `Error::DispatcherFault` for a build.
- Modify `docs/design.md`:
  - §4 lead sentence (`:122`, "`[execution]` says how the workspace wants its actions run"):
    "… wants its build files evaluated and its actions run". The table (`:124-126`, columns
    `Key | Written as | When absent`) has no meaning column, so the `workers` row is unchanged.
  - §8.1 (`:240`, "Load parallelises across engines if needed"): build files are evaluated on the
    same workers as actions, each with its own engine.
  - §11.1 (`:607`, "Load parallelises across engines"): add "on `[execution] workers` threads".
- Modify `docs/architecture-building-blocks.md`:
  - `:148` `dispatch` row: worker threads for Load and Execute.
  - `:191` `pub trait DeclarationSource {` → `pub trait DeclarationSource: Sync {`.
  - `:415` `load` row: ports `DeclarationSource`, `Dispatcher`.
  - `:425`: "`pipeline::check`, `pipeline::graph` and `pipeline::query`, for a caller that holds a
    `DeclarationSource` and nothing else" → "… a `DeclarationSource` and a `Dispatcher`"; the
    `pipeline::gc` sentence the clean-gc chain added names its dispatcher too.
  - `:433` `DeclarationSource` row: "Load — one build file at a time" → "Load — one build file per
    call, several calls at once".
  - `:445` `Dispatcher` row: "running ready actions in parallel" → "evaluating build files and
    running ready actions in parallel".
  - `:530` failure table: a row "the dispatcher loses a build file | `Error::LostBuildFiles`
    naming every directory without a result; nothing is declared".
- Modify `README.md` `:7`: "it evaluates build files on airsl" → "it evaluates build files on
  airsl, in parallel,".
- Modify `CLAUDE.md` Status (`:7`): the Milestone 2 sentence adds "and build files load in
  parallel".

**Change:** documents only.

**Proof:**
- `grep -n "one at a time today\|Load is sequential today" docs/architecture.md` → no match.
- `grep -n "Nine kinds" docs/architecture.md` matches, and §4's table has nine rows.
- `grep -n "LostBuildFiles" docs/architecture.md docs/architecture-building-blocks.md` → matches
  in §4, §6 and the failure table.
- `grep -n "pub trait DeclarationSource" docs/architecture-building-blocks.md` shows `: Sync`.
- Every `§` reference touched still resolves (`grep -n "^#\+ " docs/*.md`).

**Done when:** the greps hold and `cargo make dod` is green. Commit
`docs(repo): describe parallel load as built`.

---

## Risks

- The clean-gc chain's documents task edits `:425`, the architecture §3 table and CLAUDE.md
  Status first; line numbers shift. Anchors here are quoted phrases for that reason.

## Proof

- `cargo make dod` and `cargo deny check` green.
- `git diff --stat` touches only the five files above.

## Review findings

One reviewer pass over `4e524f0`: Proof greps hold, every § cited resolves, each changed sentence
checked against the code, DoD and `cargo deny check` green. No blocking finding.

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | 🔵 | architecture §3.1: `LostBuildFiles` condition broader than the code (replay error and dispatcher `Err` come first) | fixed: "returns success, the replay met no error" |
| 2 | 🔵 | architecture §4 row: "in order" unnamed | fixed: "in directory order" |
| 3 | 🔵 | design.md:57 "execution the parallel back phase" reads as serial Load | fixed: both phases run on the worker pool |
| 4 | 🔵 | design §8.1 "same workers as actions" — same count, not same threads | fixed: "through the same dispatcher … up to `[execution] workers` at once" |
| 5 | 🔵 | README status omits parallel loading from roadmap item 2 | fixed |
| 6 | 🔵 | `thread_pool.rs` module doc: the only place a thread "that runs an action" starts | fixed: "that runs a task" |
| 7 | 🔵 | spec §5 names design §4's `workers` row; the plan changed the lead sentence (no meaning column) | declined — recorded in this plan's Task 1 when written; the spec's intent (design §4 says workers serve Load) holds |

## Probe results

- The anchors' line numbers had moved after the clean-gc chain's documents task; each was
  re-grepped. The `pipeline::gc` sentence already named its `Dispatcher`.

## Deviations

- The fix round touched `crates/buildl/src/dispatch/thread_pool.rs` (module doc only), outside
  this plan's five files; it describes this chain's own code.
