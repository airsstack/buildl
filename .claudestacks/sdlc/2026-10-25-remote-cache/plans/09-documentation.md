---
status: done
created: 2026-10-25
depends-on: [08]
built-on: 0c4fd0bb55e2
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

## Review findings

- consistency — CLAUDE.md "Settled decisions" still said no async runtime; CLAUDE.md said uploading under `bare` is skipped (it is refused, exit 2) — fixed (blocking)
- accuracy — "the only `dyn`" in the ports and the decision record (`Dispatcher::dispatch` takes a closure trait object, `ports/dispatcher.rs:53`) — fixed: one of the two (blocking)
- CLAUDE.md rule — architecture.md §3.8 diagram was ASCII — fixed: Mermaid (blocking)
- accuracy — "an upload reads each input once"; "groups of at most 32 MiB" — fixed: hashed once, the server's missing ones re-read and re-checked; a blob over 32 MiB is its own group (blocking)
- accuracy — design §7 said a failed upload leaves the local result standing either way (strict fails before the row) and exit 3 omitted a remote cache that cannot be set up — fixed in §7 and the exit table (blocking)
- accuracy — tokio's direct dependents and what keeps other members off it — fixed: the `Cargo.lock` set; `guard-crate-edges` named
- accuracy — design §4 `header` refusals, §7 variables not read under `--no-remote` or their own flag, `doctor` defaults, §12 `timeout` and pinned env splitting remote entries — fixed
- accuracy — README named servers never run; its exit codes 2 and 3 omitted the remote causes — fixed: only bazel-remote 2.6.4 named as run; causes listed
- records — spec §2 `Action.timeout` and §7 exit 3 differed from the build — fixed: spec amended (user's yes, 2026-10-26), this plan re-pointed
- Gate: `cargo make dod` → `Build Done in 154.99 seconds` (exit 0), run once after the crate-doc edits; the fix round changed no Rust

## Probe results

- `grep -n "later" docs/architecture-building-blocks.md` — three generic hits (:3, :497, :689), none for the remote cache
- every `§` reference added resolves; the only new headings are architecture.md §3.8 and README "The remote cache"; nothing renumbered

## Deviations

- 2026-10-26 — Tasks 1 and 2 ran in parallel (disjoint files); only Task 2 touched Rust, so only it ran the gate.
- 2026-10-26 — Task 1 also corrected text the remote cache made stale outside the listed sections (building-blocks §8, §9 rule 2, §10; architecture §3.8, §4).
- 2026-10-26 — one reviewer pass reading only, then one fix commit `a2dd9da`.
