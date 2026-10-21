---
status: done
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
names (`grep -o '^| *`buildl [a-z]*' README.md docs/design.md`; README's rows start `| ` with a
space, design's `|` without).

**Done when:** the check holds and `cargo make dod` is green. Commit
`docs(repo): say verify is built`.

---

## Risks

- Renumbering is not needed: every change edits inside an existing section, and the §6 record is
  appended.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `grep -c '```mermaid' docs/*.md` equal before and after.

## Review findings

Tasks 1–3 — one reviewer over all three, claims checked against the built code: 0🔴 3🟡 8🔵, spec compliant; Mermaid counts unchanged (design 13, architecture 3, building blocks 7, walkthrough 10, README 3); every new § reference resolves; no §6 record moved.
- comment-matches-code — the built-commands sentence left out `clean` and `gc` — `docs/design.md:218` — fixed (blocking)
- comment-matches-code — "a skipped target never decides" the exit contradicted verify's exit 3 — `docs/design.md:368` — fixed: the rule is said to cover `build` and `test` (blocking)
- comment-matches-code — the kind difference read one way round only — `docs/design.md:717` — fixed: "a file where the other holds a tree" (blocking)
- comment-matches-code — not cached also covers content gone or a manifest that does not hash to its digest — `docs/design.md:218` — fixed
- comment-matches-code — a failed re-run prints a block, not one line — `docs/design.md:218` — fixed
- comment-matches-code — "what a re-run stores is named by no row" is untrue when it reproduces — `docs/design.md:218`, `docs/architecture.md` §6 — fixed: "what only a re-run stored"
- comment-matches-code — the strategy-refusal paragraph named only `Wiring::runs` — `docs/architecture.md:268` — fixed: `Wiring::verifies`, `Wiring::verify_ports`
- comment-matches-code — the lock record named only `plan`, `build`, `test` — `docs/architecture.md:401` — fixed: adds `verify`, `clean`, `gc`
- README-consistency — the exit-code gloss — `README.md:130` — fixed: 1 also a mismatch, 3 also skipped or not put back
- comment-matches-code — "the same five each time" — `README.md:150` — fixed: "while their keys stay the same"
- drift — task 3's grep pattern could never match README's rows — task 3 — fixed (plan amended)

After the fix round: Mermaid counts unchanged; `grep -o '^| *`buildl [a-z]*' README.md docs/design.md` gives the same command names in both; `re-runs a sample of cache-hit actions` gone; `grep -c sample docs/design.md` → 3. Plan-wide, with plan 03: `cargo make dod` exit 0, 1622 tests passed; `cargo deny check` green.

## Probe results

- `docs/architecture.md:188-195` held §3's per-command table; `docs/design.md:716` held "re-runs a sample of cache-hit actions"; the README anchors held. The `exec/sandbox.rs` line numbers the plan cites predate plan 02's `OutPaths` change, so the §6 record names `InputSandbox::prepare` and `OutPaths` instead of lines. None came out against the plan.

## Deviations

- 2026-10-21 — tasks 1–3 were written while plan 03's code was being built (files disjoint); the review checked them against the CLI as built.
- 2026-10-21 — task 2: the e2e table in building blocks covers the nine `verify.rs` tests in seven rows (cache-and-log with no scratch directory, and never built with content gone, share a row); the §6 record cites code by symbol, not line.
- 2026-10-21 — task 3: grep pattern made space-tolerant (plan amended).
