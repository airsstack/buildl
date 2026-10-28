---
status: approved
created: 2026-10-27
milestone: buildl/09
---

# Intent: The buildl book (mdbook)

## Problem

buildl's documentation is four spec documents under `docs/` — `design.md`,
`architecture.md`, `architecture-building-blocks.md`, `fundamental-walkthrough.md`, about
2,900 lines — written for its designers. Someone who has never read the source has no
place that teaches them to install buildl, write a build file and use its commands. The
user wants documentation in the mdbook approach airsl uses, and the current `docs/`
removed.

## Affected systems

- `docs/` — the four spec documents, removed.
- A new `book/` — the mdbook source, laid out like airsl's (`book/book.toml`,
  `book/src/SUMMARY.md`, numbered chapter files, an appendix).
- `.github/workflows/` — CI builds the book; a Pages workflow publishes it.
- `Makefile.toml` — a task to build the book locally, as airsl has.
- Everything that cites `docs/` or a spec section ("design §8.5", "architecture.md §5
  rule 4"): `crates/buildl-core/clippy.toml`, rustdoc and comments in the crates,
  `CLAUDE.md`, `README.md`.
- `examples/` — the book teaches from them and links to them.

## Desired outcome

- A book at `https://airsstack.github.io/buildl/` that takes a reader from installing
  buildl, through a first build file, to every command and feature: targets and labels,
  caching and `plan`, tests, the sandbox, the `buildl.toml` ceiling and the approver,
  discovery targets, plugins, remote cache and execution, `watch`, plus a command-line
  and configuration reference.
- The spec content rewritten into the book as chapters, not carried as-is: user-facing
  parts in the chapters above, and the design and architecture (two-phase model, action
  keys, the four crates, ports and adapters, determinism rules, the decision records) in
  an "Under the Hood" part, as airsl's book does.
- `docs/` gone, and no file outside the chain history pointing at it or at a spec `§`
  number: each citation names the book chapter that now holds the content.
- CI fails when the book does not build; a push to `main` touching the book republishes
  it.

## Constraints

- Layout, tooling and workflows follow airsl's book: mdBook pinned to one version across
  CI, Pages and `Makefile.toml`; Pages deployed from an Actions artifact, never a branch.
- Every command, output and file shown in the book is what the current binary does; the
  examples under `examples/` are the source for worked walkthroughs.
- Nothing in the spec documents is lost without a decision: each part either lands in a
  chapter or is named as dropped in the spec.
- Turning on Pages (source "GitHub Actions" in the repository settings) is the user's
  act; the repository is public.
- Chain history under `.claudestacks/sdlc/` — including its `rfcs/` copies of the old
  docs — is left as it is.
- Assumption: this chain repoints every reference that removing `docs/` breaks,
  `CLAUDE.md` and `README.md` included, with the smallest edit; rewriting `README.md`
  short and refreshing the crate READMEs and `CLAUDE.md` stays with the readmes chain.

## Non-goals

- Rewriting `README.md`, the crate READMEs or `CLAUDE.md` beyond repointing links (the
  readmes chain).
- Publishing the crates (the first-publish chain).
- API documentation for docs.rs beyond what rustdoc already holds.
- A custom domain, translations, or versioned copies of the book.
- Changing any buildl behaviour the book describes; a gap the writing finds is fixed in
  the book's wording, or raised to the user.
