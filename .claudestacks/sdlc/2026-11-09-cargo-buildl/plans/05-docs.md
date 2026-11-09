---
status: approved
created: 2026-11-09
depends-on: [04]
built-on: 072796d9b51f
---

# cargo-buildl Docs Implementation Plan

**Goal:** Point a Rust engineer at `cargo install buildl-cargo` and `cargo buildl init` everywhere buildl tells them how to start.

**Architecture:** A new book page after the rust plugin page, plus short pointers in the README, the two skills, CLAUDE.md's workspace shape and the CHANGELOG. The text follows what plans 02–04 built.

**Tech Stack:** mdBook, Markdown, the Agent Skills format.

**Content authority:** spec §10 (the user chapter, `SUMMARY.md`, `README.md`, skills and `CHANGELOG.md` rows).

---

## Files that change

```
book/src/ch08-06-cargo-buildl.md     — create: install, init, what it writes, the generated section and the engineer's part, sync and sync --check in CI, pass-through, exit codes, the §6 mapping table
book/src/SUMMARY.md                  — modify: the page after "The Rust Plugin"
book/src/ch08-05-the-rust-plugin.md  — modify: one line pointing at cargo buildl init
README.md                            — modify: a Cargo project starts with cargo install buildl-cargo && cargo buildl init
skills/buildl/SKILL.md               — modify: in the Cargo part, cargo buildl init and sync
skills/buildl-migrate/SKILL.md       — modify: "A Cargo build" starts with cargo buildl init when buildl-cargo is installed
CLAUDE.md                            — modify: the workspace has five crates, buildl-cargo the cargo subcommand
CHANGELOG.md                         — modify: Unreleased → Added: buildl-cargo
book/src/appendix-05-references.md   — modify: an entry for The Cargo Book, "External tools: Custom subcommands"
```

## Order of work

### Task 1 — Write the cargo-buildl page and the pointers

**Files:**
- Create `book/src/ch08-06-cargo-buildl.md`.
- Modify `book/src/SUMMARY.md`, `book/src/ch08-05-the-rust-plugin.md`, `README.md`, `skills/buildl/SKILL.md`, `skills/buildl-migrate/SKILL.md`, `CLAUDE.md`, `CHANGELOG.md`, `book/src/appendix-05-references.md`.

**Change:** The page describes what was built, in the book's style: the `examples/rust-workspace` generated section as the example, the §6 mapping as a table, the §8 and §9 messages and exit codes, and `sync --check` as the CI step. It cites into `appendix-05-references.md` for cargo's subcommand rule. Every other file gets a short pointer, not a second copy of the page.

**Proof:**
- `cargo make book-build` builds the book, page included.
- `cargo make skills` passes: every `lua` block in the skills still checks.

**Done when:** `cargo make book-build`, `cargo make skills`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): document cargo buildl`.

---

## Risks

- `cargo make skills` evaluates every `lua` block in the skills. A new snippet must be a complete build file, or the check fails.

## Proof

- `cargo make book-build` and `cargo make skills` are green.
- `grep -rn 'cargo buildl init' README.md book/src skills` finds the page and each pointer.
