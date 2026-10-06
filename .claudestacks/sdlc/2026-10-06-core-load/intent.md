---
status: approved
created: 2026-10-06
---

# Intent: Load — from a workspace root to `Vec<Declaration>`

## Problem

`buildl-core` holds the vocabulary of milestone 1 — `Label`, `Directory`, `Provenance`, `Digest`,
the error enum, the canonical serializer, the `Clock` port — but no pipeline phase. Nothing in the
workspace can turn a workspace's build files into the list of declarations the rest of the pipeline
consumes:

- **No declaration value.** `architecture.md` §2 names `Declaration` as the first phase handoff, and
  design §5 lists what a build file can declare (`target`, `rule`, `alias`, `test`, `option`), but no
  type carries a declaration, so I4 Resolve has no input to resolve.
- **No port to evaluate a build file.** `architecture-building-blocks.md` §5.2 sketches
  `DeclarationSource::evaluate(&BuildFile) -> Result<StagedFile>`; neither the trait nor its two
  argument types exist. I-lua, the airsl adapter, has nothing to implement until they do.
- **No traversal.** Only the root build file is known up front; every other one is discovered when an
  evaluated file calls `b.subdir`. `architecture.md` §3.1 assigns the directory queue, `subdir`
  handling and the sorted merge to Load logic in `buildl-core`. None of it is written.
- **No pipeline.** `Pipeline<P: Ports>`, the `Ports` bundle and `FakePorts` are placed in this slice
  by `architecture-building-blocks.md` §5.3 and §8 and by `docs/roadmap.md` §3. Until they exist no
  flow can be exercised end to end, and every later slice has nothing to extend.

This is the next rung of the milestone 1 ladder (`docs/roadmap.md` §3): I4 Resolve consumes its output
and I-lua implements its port, so both are blocked on it.

Questions the documents leave open, which the design pass has to close:

- The shape of `Declaration`: `architecture.md` §2 draws one struct (`action`, `inputs`, `deps`,
  `flags`), while design §5 declares five different things, some of which (`rule`, `option`) are not
  targets.
- When a dependency string becomes a `Label`: `architecture.md` §2 says labels are parsed at Resolve,
  yet draws `Declaration.deps: Vec<Label>`, which implies Load.
- The merge's sort key: `architecture.md` §3.1 sorts by (directory, declaration order), but declaration
  order inside a file varies with Lua's per-engine string-hash seed under `pairs`, which design §12.1
  promises cannot change the graph, and which `architecture.md` §5 rule 5's double-run check would then
  flag.
- The queue's rules for a directory requested twice, a `subdir` that escapes the workspace, and a
  requested directory with no build file.
- Whether `check`'s double evaluation and staging-hash comparison (`architecture.md` §5 rule 5) belongs
  to this slice.
- How the entry file name (`buildl.toml` `[workspace] entry`) reaches Load before the `Manifest` port
  exists.

Decided in the dialogue: `subdir` is explicit only. A build file is evaluated because an evaluated
build file named its directory, never because it exists on disk. This closes design §14's open item on
implicit `subdir`.

Candidate direction, agreed in the dialogue: a build file's `subdir` requests come back from
`evaluate` as raw values inside `StagedFile`, and Load in `buildl-core` owns everything after —
resolving them, deduplicating, rejecting escapes, queueing and the sorted merge.

## Affected systems

- `crates/buildl-core` — new declaration and build-file types, the `DeclarationSource` port, the Load
  logic, the `Pipeline` / `Ports` / `FakePorts` skeleton, new error variants, the crate rustdoc and
  `README.md`
- `docs/roadmap.md` — the I3 row and §4 on completion
- `docs/architecture.md`, `docs/architecture-building-blocks.md`, `docs/design.md` — only where a
  decision closed here contradicts or settles text there

## Desired outcome

- A fake `DeclarationSource` holding fixed build files, rooted at a workspace root, yields one
  `Vec<Declaration>` through the Load logic in `buildl-core` — the slice's done condition in
  `docs/roadmap.md` §3.
- Every declaration carries the `Provenance` of the file that declared it, and every Load error that
  can name a build file or a `subdir` call carries that provenance.
- The output is identical whatever order the build files are evaluated in and whatever order a file
  issues its declarations and `subdir` calls, so parallel load can be added later without changing a
  byte of output.
- `Pipeline<P: Ports>`, the `Ports` bundle and `FakePorts` exist, and the pipeline can be run up to the
  `check` handoff.
- The `DeclarationSource` contract is complete enough for I-lua to implement against without amending
  `buildl-core`.
- Every new type and flow is unit-tested in `buildl-core` against fakes, with no Lua, filesystem,
  process or thread.
- `buildl-core`'s rustdoc and `README.md` describe what the crate now holds.
- `cargo make dod` and `cargo deny check` pass, including `guard-crate-edges` and `guard-core-purity`.

## Constraints

- `buildl-core`'s dependency set is unchanged: `std` minus its filesystem, process, thread, network,
  environment and clock APIs, plus `serde`, `serde_json`, `thiserror`, `sha2`
  (`architecture-building-blocks.md` §1.2). No airsl, no buildl crate.
- Phases communicate through values; no phase module names another, and only `pipeline/` may name more
  than one phase (`architecture.md` §1.1, the five rules in `buildl-core`'s crate doc).
- Determinism house rules (`architecture.md` §5) bind every line: no `HashMap` iteration reaches any
  output, all JSON goes through the canonical serializer, parallelism is never observable.
- Lua never recurses and never holds a real artifact path (`architecture.md` §3.1, design §5). The
  traversal belongs to the host.
- Traversal is explicit: the set of evaluated build files is exactly the root plus the closure of
  `subdir` requests. Load never discovers a build file by looking at the filesystem.
- The error model stays one `#[non_exhaustive]` enum with structured fields; new variants are added,
  none reshaped (`architecture.md` §4).
- Every new handoff value is serializable (`architecture.md` §1.1).
- Workspace policy and the Rust guidelines apply unchanged: export-only `lib.rs`, no internal planning
  references in rustdoc or crate READMEs.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The airsl engine, the declaration policy, the `buildl` module table and its binding to the `buildl`
  global — I-lua.
- `b.sources()` globbing and where it runs (`architecture-building-blocks.md` §11) — I-lua.
- Label resolution across files, duplicate and unknown-reference detection, cycles, `TargetGraph` — I4.
- The `Manifest` port, the ceiling, the wanted set and grants — I4.
- Expanding a `rule` into the targets that use it, and settings validation — I4 or I5, as their specs
  decide.
- Implicit `subdir` via file discovery, in any form — rejected, not deferred.
- Parallel load. Load runs serially in `buildl-core`, which cannot name a thread.
- Any adapter in `buildl`, any argument parsing in `buildl-cli`.
- Publishing a shared fake-ports test kit (`architecture-building-blocks.md` §11).
- Roadmap follow-ups 3, 4 and 11, which are assigned to other slices.
