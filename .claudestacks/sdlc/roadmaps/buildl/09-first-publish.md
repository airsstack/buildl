---
status: approved
created: 2026-10-15
---

# Milestone: First publish to crates.io

## Goal

buildl is ready to be used by someone who has never read its source, and the four crates —
`buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — are published to crates.io in the
order of architecture-building-blocks §9. Ready means:

1. **Examples.** A top-level `examples/` directory holds workspaces for Go, Rust and
   Python, between them showing several use cases, and CI runs buildl on each one.
2. **Book.** An mdbook book, laid out like airsl's, is the documentation of buildl: CI
   builds it and GitHub Pages publishes it. It replaces `docs/`.
3. **READMEs.** `README.md` is rewritten short and points at the book and the examples;
   each crate's `README.md` matches what the crate does; `CLAUDE.md` points at the book
   instead of `docs/`.

The work runs in that order — examples, book, READMEs — and the publish comes last.

## Non-goals

Feature work of any other milestone.
