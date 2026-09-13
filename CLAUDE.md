# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status: scaffold in place, implementation not started

buildl is a sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime. The repository holds the design documents plus a workspace scaffold: the `buildl` and `buildl-cli` crates exist as doc-comment stubs with no dependencies wired (`buildl-core` and `buildl-lua` are planned, not yet created), and the `cargo make dod` gate and CI matrix are live. No pipeline module is implemented.

The pre-development blocker is **resolved** (2026-09-13): fine-grained per-file C/C++ compilation is out of scope for v1, and buildl's mechanism for dynamic input discovery is the **discovery target** (design §10) — a cached action whose output a later declaration pass consumes — not depfile parsing. See `docs/fundamental-walkthrough.md` §6 for the reasoning and design §13 for the scope note. Implementation may begin.

## Documents and their roles

- `docs/design.md` — what buildl is and why. The authoritative spec: two-phase model, `buildl.toml` ceiling, Lua declaration API, action keys, plugins, isolation ladder, sequencing (§13).
- `docs/architecture.md` — how each part works in Rust: core types, phase implementations, error architecture, decision records (§6), determinism engineering rules (§5).
- `docs/architecture-building-blocks.md` — how the parts are put together: C4 views, the four crates, ports and adapters, dependency rules, testing layers, publishing. Owns the workspace layout.
- `docs/fundamental-walkthrough.md` — maps the design onto build-system theory (Build Systems à la Carte); names the depfile gap.
- `README.md` — condensed public summary of the design and architecture documents. Keep it consistent with the docs when either changes.

Section cross-references between the docs (e.g. "design §8.5", "§12.4 ledger") are load-bearing — preserve them when renumbering or restructuring.

## Core architecture (the big picture)

**Declare, then execute.** Build files (Lua) only *declare* targets; the Rust host schedules, sandboxes, parallelizes, and caches. Lua never runs a compiler or holds a real artifact path. The founding constraint, inherited from airsl: **nothing is ambient** — authority is derived from the build graph (the "wanted set"), intersected with the workspace ceiling in `buildl.toml`, never requested per-file.

**Pipeline:** Load → Resolve → Plan → Execute → Record. Phases communicate through serializable values (`Vec<Declaration>` → `TargetGraph` → `Plan` → `Vec<ActionOutcome>`), never by calling each other; each CLI command (`check`/`graph`/`plan`/`build`) is the pipeline truncated at a handoff.

**Incrementality:** the action key — SHA-256 over command, input hashes, dep action keys, env values, tool fingerprints, settings, os/arch. Dep *keys* fold into the hash so dirtiness propagates with no marking pass; `KeyComponents` is kept beside the digest so `plan` explains dirtiness as a struct diff. Crash-safety invariant: **cas first, cache row after, log always** — failures are never cached.

**Structural rules (architecture-building-blocks.md §7):**
- airsl is a dependency of the `buildl-lua` crate only; no other crate knows Lua exists.
- Dependency inversion: `buildl-core` holds domain data, ports (traits), and the pure logic of every phase, and depends on no buildl crate, no airsl, and no I/O API. Adapters (`buildl-lua`, `buildl`) depend on `buildl-core` only, never on each other. `buildl` is the single composition root. Every pipeline flow must be testable in `buildl-core` against fake ports.
- Build files run with two globals: `airsstack` (airsl's default root table, holding the curated host modules — `json`, `path`, `regex`, `hash`, `glob`) and `buildl` (buildl's own module table — the declaration framework). They are one table bound twice: the `buildl` `HostModule` installs as `airsstack.buildl` and binds the same table to the global `buildl` inside `install` (design §5). Never rename the root table to `buildl`.

## Planned workspace shape

Four crates, published to crates.io in dependency order: `buildl-core` (domain data, ports, pure logic) → `buildl-lua` (the airsl adapter) → `buildl` (the remaining adapters and the composition root — the complete framework) → `buildl-cli` (thin clap shell, the `buildl` binary). The core is a crate, never a `mod core`: a crate-root module named `core` shadows Rust's built-in `core` crate. Workspace policy inherited verbatim from airsl: `unsafe_code = "forbid"`, `unwrap_used` and `panic` denied, pedantic + nursery clippy at warn, every dependency commented with its reason in the workspace `Cargo.toml`.

## Settled decisions — do not relitigate

Recorded in `architecture.md` §6 with rationale: plain threads (no async runtime), own graph arenas (petgraph declined), rayon confined to the `Digester` adapter, SHA-256 (REAPI compatibility over blake3 speed), JSON caches via one canonical sorted-key serializer, std mpsc channels, Lua exposure confined to `buildl-lua`. Structure recorded in `architecture-building-blocks.md` §10: four dependency-inverted crates, `buildl` as composition root, open ports behind one `Ports` bundle.

Determinism house rules (architecture.md §5) apply to any future code: no `HashMap` iteration reaches any output; all JSON through the one canonical serializer; parallelism never observable in artifacts; wall clock read only through the `Clock` port.

## Conventions

- Both spec docs carry `**Author:** rstlix0x0 · **Date:** … · **Reviewers:** —` headers and numbered reference lists; new design content should cite into those lists.
- License is Apache-2.0, following airsstack convention. buildl is part of the airsstack ecosystem and the first consumer of airsl's policy model and host-module seam.

## Response style

Two rules govern how answers are written in this repository.

**1. Concise mode is always on.** Level `full`, stored at `$AIRSSTACK_HOME/cc/concise.json`
(default `$HOME/.airsstack/cc/concise.json`) and re-injected every turn by the
`claudestacks` `UserPromptSubmit` hook. Drop articles where unambiguous, filler, hedging and
pleasantries; fragments are fine; prefer the short synonym. Never compress away technical
substance — code blocks, shell commands and error text stay verbatim, technical terms stay
exact. Write normally for security warnings, irreversible-action confirmations and ordered
multi-step instructions. Invoke the `claudestacks:concise` skill to change or restore the level.

**2. Prefer a diagram or table to a narrative paragraph.** Anything with structure — a
pipeline, a state machine, a module graph, a decision matrix, a milestone ladder — is shown,
not described. Three sentences of prose tracing a data path is a fenced ASCII diagram
instead:

```text
  build.lua ──► Load ──► Resolve ──► Plan ──► Execute ──► Record
                 │         │          │         │           │
             Vec<Decl>  TargetGraph  Plan   Vec<Outcome>  cas + row + log
                 └── check   └── graph  └── plan       └── build
```

Comparisons, option sets and per-file breakdowns go in Markdown tables. Prose is for the
argument connecting them — the reasoning, the trade-off, the recommendation — not for
restating what the picture already shows.

**Chat ASCII, document Mermaid.** This rule governs terminal responses only. The design
documents use Mermaid deliberately (GitHub renders it) — never convert a `mermaid` block in
`docs/` to ASCII, and keep using Mermaid when adding diagrams there.
