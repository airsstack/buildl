---
status: approved
created: 2026-10-20
depends-on: [01, 02, 03, 04]
built-on: 5fb250597c90
---

# Documentation Implementation Plan

**Goal:** The design, architecture, README and `CLAUDE.md` describe tier 2 as built.

**Architecture:** Text only. Each change replaces a sentence that says tier 2 is to come, or
that only `bare` runs, with what plans 01–04 built; the four decisions the spec settled become
decision records in architecture §6, and the §8 open question on materialising inputs is marked
settled. Section numbers and cross-references are kept (CLAUDE.md: they are load-bearing);
Mermaid blocks stay Mermaid.

**Tech Stack:** Markdown, `cargo make dod` (doc step), `cargo deny check`.

**Content authority:** spec §2 (all rows), §6, §7, §8; spec P16–P22 as the evidence the decision
records cite.

---

## Files that change

```
docs/design.md                         — [modify] §4, §8.5, §8.7, §12.2, §12.3, §12.4
docs/architecture.md                   — [modify] §3.4, §6, §8
docs/architecture-building-blocks.md   — [modify] status line, §6 port catalog row
README.md                              — [modify] status paragraph, "Running it"
CLAUDE.md                              — [modify] Status paragraph
```

## Order of work

### Task 1 — Design document

**Files:** `docs/design.md`

**Change:**
- §4 (`:130`): replace "Until the input sandbox ships … `strategy = "bare"`" with: `bare` and
  `sandbox` run, `sandbox` the default; `os-sandbox` and `container` are refused by `build` and
  `test` with an error naming `strategy = "sandbox"`, and every other command works under any
  strategy; a workspace whose tools read undeclared files writes `strategy = "bare"` (spec §7).
- §8.5 (`:285`): the action runs in its exec dir outside the workspace; its outputs reach `out/`
  only from the cas after they are stored; a failed or timed-out action leaves nothing there.
- §8.7 (`:363`, the exit-5 row): add "a target that declares a variable the sandbox pins".
- §12.2 (`:664-676`): tier 2 as built — exec dir `<temp>/buildl-<workspace id>/<action id>`,
  copies not links, what it does not stop (absolute paths, climbing out, files outside the
  workspace, the network); a target cached under `bare` stays cached under `sandbox`, and
  `verify` is how a cached target is checked; outputs match across tiers only for tools that do
  not record their working directory (P21). The Mermaid ladder (`:668-672`) is unchanged.
- §12.3 (`:684`) and §12.4 (`:702`): the pinned values as built (`TZ=UTC`, `LC_ALL=C`,
  `SOURCE_DATE_EPOCH=315532800`, 1980-01-01, why that date), and that declaring one is refused
  at Plan.

**Proof:** `grep -n 'runs `bare` alone\|Until the input sandbox ships' docs/design.md` → no match;
`grep -c '315532800' docs/design.md` ≥ 1; design.md cites only its own sections, so every
`§` reference in it still resolves (`grep -o '§[0-9.]*' docs/design.md | sort -u` checked
against `grep -n '^#' docs/design.md`).

**Done when:** the checks above hold and `cargo make dod` is green. Commit
`docs(repo): describe the input sandbox in the design`.

### Task 2 — Architecture documents

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`

**Change:**
- architecture §3.4 (`:265`): replace "`InputSandbox` (tier 2: temp dir, symlinked declared
  inputs) arrives with item 3" with `InputSandbox` and `Local` as built (spec §3.3, §3.4):
  copies, exec dir outside the workspace, pinned env from `Isolation::pinned`, `Local` choosing
  by manifest, `Refused` keeping `plan` working.
- architecture §6 (`:365`): four new decision records, each with its reason and evidence —
  materialise by copy, not link (P18, P19, P20); exec dir outside the workspace (P16, P17); a
  stable exec dir per target (P21); the pinned set lives in core (spec §3.1).
- architecture §8 (`:418`): the open question on `ExecDir` materialisation marked settled,
  pointing to the §6 record.
- building-blocks status line (`:3`): `InputSandbox` no longer listed as to come.
- building-blocks §6 port catalog (`:454`): the `InputSandbox` row as built, beside `Bare`.

**Proof:** `grep -n 'symlinked declared inputs' docs/architecture.md` → no match;
`grep -n 'for the input sandbox the' docs/architecture-building-blocks.md` → no match; nothing
is renumbered, so the only references to check are those this task adds: each new `§N` it
writes resolves to a heading of the document it names.

**Done when:** the checks hold and `cargo make dod` is green. Commit
`docs(repo): record the input sandbox decisions`.

### Task 3 — README and `CLAUDE.md`

**Files:** `README.md`, `CLAUDE.md`

**Change:**
- README status (`:7`): actions run at tier 2, the input sandbox, by default; tier 1 on request;
  the input sandbox drops out of "designed and not built".
- README "Running it" (`:150-157`): the manifest needs no `strategy` line; `strategy = "bare"` is
  the way out for tools that read undeclared files.
- `CLAUDE.md` Status: milestone 3 has begun — `build` and `test` run in the input sandbox by
  default, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned.

**Proof:** `grep -n 'Only the first isolation tier is built' README.md` → no match; README and
design §4 agree on the default (read side by side).

The release note spec §7 asks for has no file in this repository (`git ls-files | grep -iE
'change|release'` → nothing): it is the pull request's description, which says that a
workspace with no `strategy` line now runs sandboxed and that `strategy = "bare"` opts back.

**Done when:** the checks hold and `cargo make dod` is green. Commit
`docs(repo): say the input sandbox is built`.

---

## Risks

- Renumbering is not needed: every change edits inside an existing section. A new §6 record is
  appended after the last one, so no record number moves.
- Not done here: `verify` (chain I17), whose text design §12.3 keeps as designed.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- No `mermaid` block in `docs/` is removed or converted (`grep -c '```mermaid' docs/*.md` equal
  before and after).
