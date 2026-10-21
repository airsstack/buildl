---
status: approved
created: 2026-10-21
depends-on: [01, 02, 03]
built-on: 6c7f83a41771
---

# Documentation Implementation Plan

**Goal:** The design, architecture, README and `CLAUDE.md` describe `buildl verify` as built.

**Architecture:** Text only. Each change replaces a sentence that says `verify` is to come, or
that lists the commands without it, with what plans 01–03 built. One decision record is added to
architecture §6. Section numbers and cross-references are kept; Mermaid stays Mermaid.

**Tech Stack:** Markdown, `cargo make dod` (doc step), `cargo deny check`.

**Content authority:** spec §2, §3, §6, §7.

---

## Files that change

```
docs/design.md                         — [modify] status line, §4, §7, §8.7, §12.2, §12.3, §13
docs/architecture.md                   — [modify] status line, the commands-as-truncations text, §6
docs/architecture-building-blocks.md   — [modify] status line, core module rows, e2e test table
README.md                              — [modify] status paragraph, "Running it"
CLAUDE.md                              — [modify] Status paragraph
```

## Order of work

### Task 1 — Design document

**Files:** `docs/design.md`

**Change:**
- Status line (`:3`): of item 3, the input sandbox and `verify` are built.
- §4 (`:130`): `verify` joins `build` and `test` as a command that runs actions; it runs under
  `sandbox` only, `bare` refused with the line to write.
- §7 (the commands table near `:212`): a `verify` row — Plan, restore the cached outputs, re-run
  them (all, or `--sample N` lowest key digests) in the sandbox, compare; no row, no log.
- §8.7: verify's exits (0, 1 mismatch, a failed re-run's class, 3 skipped/unplaced, 5 refused).
- §12.2 (`:694`): `verify` as built.
- §12.3 (`:716`): "re-runs a sample of cache-hit actions" → all of them, or `--sample N`, chosen by
  lowest key digest so the same keys check the same targets; names the target, output and first
  differing file.
- §13 item 3 (`:747`): `verify` built.

**Proof:** `grep -n 're-runs a sample of cache-hit actions' docs/design.md` → no match;
`grep -c 'sample' docs/design.md` ≥ 1; every section number in
`grep -o '§[0-9][0-9.]*' docs/design.md | sort -u` appears in `grep -n '^#' docs/design.md`.

**Done when:** the checks hold and `cargo make dod` is green. Commit
`docs(repo): describe verify in the design`.

### Task 2 — Architecture documents

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`

**Change:**
- architecture status line; §3's per-command table (`architecture.md:188-195`, beside the `log`
  and `gc` rows): a `verify` row — Plan → restore → re-run → compare, without Record.
- architecture §6: a new record appended after the last — "verify restores `out/` from the cache
  and its sandbox leaves output paths alone" — reason: the sandbox copies dependency outputs from
  `out/` (`exec/sandbox.rs:135-138`) and `build`'s sandbox clears output paths in `prepare`
  (`:93-101`), so a re-run must not clear what a concurrent re-run copies; evidence: spec V8, V9.
- building-blocks status line; core module rows gain `verify`; the e2e table gains
  `tests/e2e/verify.rs`.

**Proof:** each new `§N` reference resolves to a heading of the document it names; no record
number moves.

**Done when:** the checks hold and `cargo make dod` is green. Commit
`docs(repo): record the verify decisions`.

### Task 3 — README and `CLAUDE.md`

**Files:** `README.md`, `CLAUDE.md`

**Change:** README status (`:7`) lists `verify` among the commands; the Commands table
(`:114-128`) gains a `verify` row; "Running it" shows `buildl verify` and `--sample N`; the
sequence item (`:185`) says `verify` is built. `CLAUDE.md` Status: milestone 3 — `verify` re-runs
cached actions in the sandbox and names those that differ.

**Proof:** `grep -c 'verify' README.md` covers the status line, the Commands table row and
"Running it"; the README Commands table and design §7's commands table have the same command
names (`grep -o '^|`buildl [a-z]*' README.md docs/design.md`).

**Done when:** the check holds and `cargo make dod` is green. Commit
`docs(repo): say verify is built`.

---

## Risks

- Renumbering is not needed: every change edits inside an existing section, and the §6 record is
  appended.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `grep -c '```mermaid' docs/*.md` equal before and after.
