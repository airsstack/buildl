# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status: design phase — no code exists yet

buildl is a sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime. Nothing is implemented; the repository currently contains only design documents. Work here means editing docs, not writing code — until the one pre-development blocker is resolved: **the depfile problem** (dynamic input discovery, e.g. C header dependencies the compiler reports at execute time). See `docs/fundamental-walkthrough.md` §6; the decision is either "fine-grained C is out of scope for v1" or Ninja-style depfile support host-side.

## Documents and their roles

- `docs/design.md` — what buildl is and why. The authoritative spec: two-phase model, `buildl.toml` ceiling, Lua declaration API, action keys, plugins, isolation ladder, sequencing (§13).
- `docs/architecture.md` — how it is built in Rust: module layout, core types, phase implementations, decision records (§6), determinism engineering rules (§5).
- `docs/fundamental-walkthrough.md` — maps the design onto build-system theory (Build Systems à la Carte); names the depfile gap.
- `README.md` — condensed public summary of the two above. Keep it consistent with the docs when either changes.

Section cross-references between the docs (e.g. "design §8.5", "§12.4 ledger") are load-bearing — preserve them when renumbering or restructuring.

## Core architecture (the big picture)

**Declare, then execute.** Build files (Lua) only *declare* targets; the Rust host schedules, sandboxes, parallelizes, and caches. Lua never runs a compiler or holds a real artifact path. The founding constraint, inherited from airsl: **nothing is ambient** — authority is derived from the build graph (the "wanted set"), intersected with the workspace ceiling in `buildl.toml`, never requested per-file.

**Pipeline:** Load → Resolve → Plan → Execute → Record. Phases communicate through serializable values (`Vec<Declaration>` → `TargetGraph` → `Plan` → `Vec<ActionOutcome>`), never by calling each other; each CLI command (`check`/`graph`/`plan`/`build`) is the pipeline truncated at a handoff.

**Incrementality:** the action key — SHA-256 over command, input hashes, dep action keys, env values, tool fingerprints, settings, os/arch. Dep *keys* fold into the hash so dirtiness propagates with no marking pass; `KeyComponents` is kept beside the digest so `plan` explains dirtiness as a struct diff. Crash-safety invariant: **cas first, cache row after, log always** — failures are never cached.

**Structural rules (architecture.md §1.1):**
- airsl is a dependency of the `declare` module only; no other module knows Lua exists.
- Modules depend strictly downward; phase modules meet only through `types/` and `store/`.

## Planned workspace shape

Cargo workspace mirroring airsl: `crates/buildl` (library — all logic) and `crates/buildl-cli` (thin clap shell). Workspace policy inherited verbatim from airsl: `unsafe_code = "forbid"`, `unwrap_used` and `panic` denied, pedantic + nursery clippy at warn, every dependency commented with its reason in the workspace `Cargo.toml`.

## Settled decisions — do not relitigate

Recorded in `architecture.md` §6 with rationale: plain threads (no async runtime), own graph arenas (petgraph declined), rayon confined to `plan`, SHA-256 (REAPI compatibility over blake3 speed), JSON caches via one canonical sorted-key serializer, std mpsc channels, Lua exposure confined to `declare`.

Determinism house rules (architecture.md §5) apply to any future code: no `HashMap` iteration reaches any output; all JSON through the one canonical serializer; parallelism never observable in artifacts; wall clock read only via `clock.rs`.

## Conventions

- Both spec docs carry `**Author:** rstlix0x0 · **Date:** … · **Reviewers:** —` headers and numbered reference lists; new design content should cite into those lists.
- License is Apache-2.0, following airsstack convention. buildl is part of the airsstack ecosystem and the first consumer of airsl's policy model and host-module seam.
