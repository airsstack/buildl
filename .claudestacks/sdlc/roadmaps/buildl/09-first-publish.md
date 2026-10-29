---
status: approved
created: 2026-10-15
---

# Milestone: First publish to crates.io

## Goal

buildl is ready to be used by someone who has never read its source, and the four crates —
`buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — are published to crates.io in the
order the book's Publishing section sets (`book/src/ch11-03-four-crates.md`). Ready means:

1. **Examples.** A top-level `examples/` directory holds workspaces for Go, Rust and
   Python, between them showing several use cases, and CI runs buildl on each one.
2. **Book.** An mdbook book, laid out like airsl's, is the documentation of buildl: CI
   builds it and GitHub Pages publishes it. It replaces `docs/`.
3. **READMEs.** `README.md` is rewritten short and points at the book and the examples;
   each crate's `README.md` matches what the crate does; `CLAUDE.md` points at the book
   instead of `docs/`.
4. **AI skills.** A top-level `skills/` directory holds skills in the Agent Skills format
   (agentskills.io/specification): `buildl`, for writing build files and reading what
   buildl reports, and `buildl-migrate`, for moving a Makefile, a justfile, plain shell
   scripts or npm scripts to `build.lua`, with a parity check run as prepare → run →
   compare. The skills' scripts are Lua run by `airsl`, and none is shell or Python.
   `.claude-plugin/marketplace.json` offers the same skills as a Claude Code plugin,
   without copying them. One shell script, `skills/install.sh`, installs: the `buildl`
   and `airsl` commands, then the skills, as the Claude Code plugin or into a folder
   another agent reads; it is the only shell script, and it does nothing but install. A
   book page,
   `book/src/ch01-04-with-an-ai-agent.md`, says how to install and use them, and
   `README.md` and `CLAUDE.md` point at it.

The work runs in that order — examples, book, READMEs, AI skills — and the publish comes
last.

## Non-goals

Feature work of any other milestone.
