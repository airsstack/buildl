---
status: approved
created: 2026-10-29
depends-on: [05]
built-on: 45bb359e2387
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
