---
status: approved
created: 2026-10-25
depends-on: [08]
built-on: 5ec007f25407
---

# Documentation Implementation Plan

**Goal:** Every document spec §11 names describes the remote cache as built.

**Architecture:** Documents only, after the code is done, so they describe what exists. Section
cross-references between the docs are load-bearing (`CLAUDE.md`, "Documents and their roles");
new sections are added without renumbering existing ones, and Mermaid blocks stay Mermaid.

**Tech Stack:** Markdown, Mermaid. `cargo make dod` (doctests of crate docs).

**Content authority:** spec §11; the built behaviour of plans 01-08.

---

## Files that change

```
docs/design.md                        — [modify] §4 the [remote] table; §7 the switches, `fetched` and `doctor`'s remote section; §11.2 the remote cache built; §13 item 6 built
docs/architecture.md                  — [modify] status line; §3.4 the worker's remote steps; §3.5 fetch and upload against the crash-safety invariant; §6 the async-runtime record amended
docs/architecture-building-blocks.md  — [modify] C4 context: the remote cache no longer "later" (:70, :76); port catalog §6 gains RemoteCache; §7 dependency rules name tonic/tokio confined to buildl
README.md                             — [modify] the remote cache in the summary, the build/test flags and `doctor`'s remote section, kept consistent with the docs
CLAUDE.md                             — [modify] Status: milestone 6 built
crates/buildl-core/src/lib.rs         — [modify] crate docs mention the remote cache port
crates/buildl/src/lib.rs              — [modify] crate docs list the remote adapter
```

`.claudestacks/sdlc/roadmaps/buildl/06-remote-cache.md` is unchanged: a milestone file carries
no progress (spec §11).

## Order of work

### Task 1 — Design and architecture documents

**Files:** `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md`.

**Change:** each section spec §11 names says what plans 01-08 built, citing into the documents'
numbered reference lists where it uses an outside source (the REAPI proto at its pinned commit).
The §6 decision record keeps "None — threads" for the build and records the amendment: a tokio
runtime confined to the remote adapter, reached only through `block_on`, with the reason
(tonic needs it; the core and the scheduler stay synchronous).

**Proof:** `grep -n "later" docs/architecture-building-blocks.md` no longer shows the remote
cache marked "later"; every `§` reference added resolves to an existing heading.

**Done when:** `cargo make dod` is green and `grep -n "later" docs/architecture-building-blocks.md`
shows no remote cache line. Commit `docs(repo): the remote cache in the design and architecture`.

### Task 2 — README, CLAUDE.md and crate docs

**Files:** `README.md`, `CLAUDE.md`, both `lib.rs`.

**Change:** README and CLAUDE.md Status say milestone 6 is built and what it does, in the docs'
own terms; the crate docs mention the port and the adapter.

**Proof:** `cargo make dod` green (crate docs compile, doctests pass).

**Done when:** commit `docs(repo): remote cache built; milestone 6 status`.

## Risks

- A renumbered section breaks cross-references; nothing is renumbered.

## Proof

`cargo make dod` green; the documents, README and CLAUDE.md agree with each other and with the
built behaviour.
