---
status: done
created: 2026-10-29
depends-on: [05]
built-on: 66bd139caa00
---

# AI Skills Documentation Implementation Plan

**Goal:** a reader of the book, the README or `CLAUDE.md` finds how to install the skills and what
they do.

**Architecture:** One new book page under Getting Started, `ch01-04-with-an-ai-agent.md`, holds the
account; `SUMMARY.md`, `ch01-01-installation.md`, `README.md` and `CLAUDE.md` point at it (spec §7).
No chapter is renumbered. Diagrams in the book are Mermaid, as the book's convention is.

**Tech Stack:** mdBook 0.4.52 with mdbook-mermaid 0.16.2 (Makefile.toml book tasks; ci.yml `book` job).

---

## Files that change

```
book/src/ch01-04-with-an-ai-agent.md  — [create] the page
book/src/SUMMARY.md                   — [modify] list the page
book/src/ch01-00-getting-started.md   — [modify] its list of the section's pages gains the page
book/src/appendix-05-references.md    — [modify] add the Agent Skills specification
book/src/ch01-01-installation.md      — [modify] one line pointing at the page
README.md                             — [modify] a Learn more row and the install command
CLAUDE.md                             — [modify] Documents and their roles; Status
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| Getting Started's entries are `ch01-01` … `ch01-03`, the last *What Did It Do?* | book/src/SUMMARY.md:5-8 |
| `ch01-01-installation.md` ends with *Check that it worked* (`buildl --version`) | book/src/ch01-01-installation.md:51-60 |
| README's *Learn more* table has rows Start, Concepts, Features, Reference, Examples | README.md:52-62 |
| CLAUDE.md's *Documents and their roles* lists `book/`, the roadmap, `README.md` | CLAUDE.md:11-18 |
| the install commands, options and steps | spec §5.2–5.4 |
| what each skill does, the loop, the two causes of exit 4, the parity check | spec §3.3, §4.4–4.7 |
| the book's references list, which new citations go into; it has no entry for the Agent Skills specification yet | CLAUDE.md *Conventions*; `grep -i 'agent skills\|agentskills' book/src/appendix-05-references.md` finds nothing |

## Order of work

### Task 1 — The book page

**Files:**
- Create `book/src/ch01-04-with-an-ai-agent.md`
- Modify `book/src/SUMMARY.md` — `- [With an AI Agent](ch01-04-with-an-ai-agent.md)` after *What Did
  It Do?*, at the same depth
- Modify `book/src/ch01-00-getting-started.md` — its count and list of the section's pages gain the
  new page
- Modify `book/src/ch01-01-installation.md` — one sentence after *Check that it worked*'s console
  block, linking the page for working with an AI agent
- Modify `book/src/appendix-05-references.md` — an entry for the Agent Skills specification,
  `https://agentskills.io/specification`, in the file's existing entry form

**Change:** the page, in the book's voice, in this order: what the two skills are and that they follow
the Agent Skills format (citing the new `appendix-05-references.md` entry); installing with `install.sh --claude`
and `--dir`, with the time `cargo install --git` takes; what an agent does with the `buildl` skill — the
loop as a Mermaid diagram, and that a refusal is answered by asking you, never by widening the
ceiling; migrating with `buildl-migrate` — prepare → run → compare as a Mermaid diagram, what the
table's verdicts mean, and that the old build is never touched; the C/C++ case.

**Proof:**
- `cargo make book-build` succeeds
- `grep -n "ch01-04-with-an-ai-agent" book/src/SUMMARY.md book/src/ch01-01-installation.md` finds both
  links
- every command shown on the page matches spec §5.2 and §4.4, checked line by line while writing

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`docs(repo): add the book page on using buildl with an AI agent`.

### Task 2 — README and CLAUDE.md

**Files:**
- Modify `README.md` — a row `AI agents` in *Learn more* linking the page at
  `https://airsstack.github.io/buildl/ch01-04-with-an-ai-agent.html`, and under *Install* the
  `curl -fsSLO … && sh install.sh --claude` pair of spec §5.2
- Modify `CLAUDE.md` — *Documents and their roles* gains `skills/` (the two skills in the Agent Skills
  format, the installer, `skills/tests/`; checked by `cargo make skills`) and `.claude-plugin/` (the
  Claude Code marketplace pointing at `skills/`); *Status* gains one sentence saying the skills exist
  and where the book describes them

**Change:** both front pages point at the skills and the page.

**Proof:**
- `grep -n "ch01-04-with-an-ai-agent" README.md` finds the row
- `grep -n "skills/" CLAUDE.md` finds the new entry
- the README and the book say the same install commands (CLAUDE.md: "Keep it consistent with the
  book")

**Done when:** that holds, and `cargo make dod` and `cargo deny check` are green. Commit
`docs(repo): point README and CLAUDE.md at the AI skills`.

## Risks

- The README links the published book page, which exists only after the Pages workflow runs on
  `main`; until the merge the link answers 404. The same holds for every other README link added
  before a merge.
- The page restates commands from the installer; a later change to `install.sh` must change the
  page too. The page cites spec §5 as its source while being written.

## Proof

`cargo make book-build` succeeds, and the three greps find their links.

## Review findings

One reviewer pass over Tasks 1–2, one fix round (`cargo make book-build` after it: `Build Done in 1.23 seconds`, no warnings; the four greps find `SUMMARY.md:9`, `ch01-01-installation.md:61`, `ch01-00-getting-started.md:11`, `README.md:70`).

- docs — `ch01-00-getting-started.md` still said "three sections" and listed three pages — book; spec §7 and the plan's Files amended — fixed
- docs — "from a clone it builds the buildl in that clone" held only for `--dir` — ch01-04 — fixed
- docs — the refusal list read as if `[approver]` and `strategy = "bare"` changes were allowed after a yes; the skill forbids them outright — ch01-04 — fixed: two "never", two "on your yes"
- docs — `--branch main` shown for any `--ref`; `--ref` ignored by a clone `--dir` unstated; "copied" for `--claude`; reference cited by number; error sample claimed verbatim; run row omitted the old tests, `graph` and `doctor`; FAIL row too broad; README omitted the skip of programs already on PATH; CLAUDE.md Status sentence's subject — ch01-04, README.md, CLAUDE.md — fixed
- coverage — nothing checks the page's commands against `install.sh` — ch01-04 — declined: the plan's Risks already carries it

## Probe results

- `cargo make book-build`: `Build Done in 1.02 seconds`, html backend and mermaid preprocessor ran
- README's `curl -fsSLO https://raw.githubusercontent.com/airsstack/buildl/main/skills/install.sh` / `sh install.sh --claude` equal the page's lines 29–30, as spec §5.2
- gates once for the batch: `cargo make dod` exit 0, `cargo deny check` `advisories ok, bans ok, licenses ok, sources ok`

## Deviations

- 2026-10-29 — `book/src/ch01-00-getting-started.md` added to the plan's Files and spec §7 after review.
- 2026-10-29 — Tasks 1 and 2 ran in parallel (disjoint files).
