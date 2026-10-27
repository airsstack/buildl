# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status

buildl is a sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime. The workspace holds four crates — `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — with their dependency edges wired per `architecture-building-blocks.md` §7 (airsl `0.1`, only in `buildl-lua`; toolchain `stable`, `rust-version` 1.94), and the `cargo make dod` gate, `cargo deny` check and CI matrix are live. Milestone 1 is done: the `buildl` binary runs `check`, `graph`, `plan` and `build` on a real workspace, with actions run as plain processes (isolation tier 1; tier 2 came with Milestone 3). Milestone 2 has begun: `query` answers `deps()` and `rdeps()` over the target graph, `test` runs the test targets (cached by key), `log` shows what a target printed when it last ran, `clean` removes the output directory, `gc` prunes the store, and build files load in parallel. Milestone 3 has begun: `build` and `test` run each action in the input sandbox (isolation tier 2) by default, in a directory outside the workspace holding copies of its declared inputs, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned; `strategy = "bare"` runs at tier 1, and `os-sandbox` and `container` are refused. `verify` re-runs cached actions in the sandbox, all of them or the `--sample N` lowest by key, and names those whose outputs differ from the cache; it records nothing and runs only under `strategy = "sandbox"`. Milestone 4 is done: `build`, `test` and `verify` ask the approver `buildl.toml`'s `[approver]` table names — `manifest` (the default), `interactive`, which asks once on a terminal and remembers a yes outside the workspace, or `deny` — after planning and before anything runs; a program a dependency built runs with no `proc.run` entry; and `doctor` prints the ceiling, the approver, the execution settings and every want, refusing nothing. Milestone 5 is built: a build file calls `buildl.use` to load a plugin — Lua kept in the workspace or fetched from git by `buildl plugins fetch` into `buildl.lock` and the store, named in `buildl.toml`'s `[plugins.<name>]` and described by an `extension.toml` — and what the plugin declares carries its name to `plan`, `doctor`, the approver and `graph`, and is held to the plugin's manifest and the workspace's cap for it before the ceiling; a discovery target (`buildl.discover`, read back with `buildl.discovered`) is a cached action whose output a second declaration pass consumes, run first by `build` and `test`. Milestone 6 is built: a `[remote]` table in `buildl.toml` names a cache that speaks the Remote Execution API v2 over gRPC, and `build` and `test` look up a target the local cache cannot serve there, report a hit as `fetched` once its blobs are checked and stored, and upload what they ran only when `BUILDL_REMOTE_UPLOAD` or `--remote-upload` says to, and asking for an upload under `strategy = "bare"` is refused as a usage error (exit code 2); `BUILDL_REMOTE_STRICT` and `--remote-strict` make a failed call fail the run, `--no-remote` leaves the remote out, and `doctor` prints the remote's settings without connecting. Milestone 7 is built: `buildl watch [target]...` takes `build`'s flags, builds, then builds again when a file the build reads changes or when the build files declare a different graph, reading the workspace anew each cycle; file events come from `notify`, changes under `out/`, the state directory and `.git` are ignored, and it ends on Ctrl-C, a usage error (exit code 2) or a file-watcher failure (exit code 3). Milestone 8 is built: with `execute = true` in `[remote]`, `BUILDL_REMOTE_EXECUTE` or `--remote-execute`, `build` and `test` send a target neither the local cache nor the remote cache can serve to the server's Execution service, check the result as a fetched one is, store it and report it as `remote`, or as `fetched` when the server answers from its own cache; `--no-remote-execute` and `--no-remote` turn it off, `--remote-jobs` and `[remote] jobs` bound how many targets are with the server at once while `[execution] workers` still bounds the actions run here, every remote call is retried on `UNAVAILABLE`, `RESOURCE_EXHAUSTED`, `ABORTED` and `CANCELLED` up to `[remote] retries` times, asking for execution under `strategy = "bare"` is refused as a usage error (exit code 2), and a CI job runs the binary against NativeLink. What comes next is on the claudestacks-sdlc status board (`/claudestacks-sdlc:status`), which counts it from the roadmap under `.claudestacks/sdlc/roadmaps/buildl/` and the chains that name its milestones; read it there rather than inferring a state from this file.

The pre-development blocker is **resolved** (2026-09-13): fine-grained per-file C/C++ compilation is out of scope for v1, and buildl's mechanism for dynamic input discovery is the **discovery target** (design §10) — a cached action whose output a later declaration pass consumes — not depfile parsing. See `docs/fundamental-walkthrough.md` §6 for the reasoning and design §13 for the scope note. Implementation may begin.

## Documents and their roles

- `docs/design.md` — what buildl is and why. The authoritative spec: two-phase model, `buildl.toml` ceiling, Lua declaration API, action keys, plugins, isolation ladder, sequencing (§13).
- `docs/architecture.md` — how each part works in Rust: core types, phase implementations, error architecture, decision records (§6), determinism engineering rules (§5).
- `docs/architecture-building-blocks.md` — how the parts are put together: C4 views, the four crates, ports and adapters, dependency rules, testing layers, publishing. Owns the workspace layout.
- `docs/fundamental-walkthrough.md` — maps the design onto build-system theory (Build Systems à la Carte); names the depfile gap.
- `.claudestacks/sdlc/roadmaps/buildl/` — the claudestacks-sdlc roadmap: `roadmap.md` (vision, non-goals) and one file per milestone — design §13 items 1–8 as `01`–`08`, and `09-first-publish`. A chain joins a milestone through the `milestone:` key in its `intent.md`. Progress is never written down: the status board counts it from the chains each time it runs.
- `README.md` — condensed public summary of the design and architecture documents. Keep it consistent with the docs when either changes.

Section cross-references between the docs (e.g. "design §8.5", "§12.4 ledger") are load-bearing — preserve them when renumbering or restructuring.

## Core architecture (the big picture)

**Declare, then execute.** Build files (Lua) only *declare* targets; the Rust host schedules, sandboxes, parallelizes, and caches. Lua never runs a compiler or holds a real artifact path. The founding constraint, inherited from airsl: **nothing is ambient** — authority is derived from the build graph (the "wanted set"), intersected with the workspace ceiling in `buildl.toml`, never requested per-file.

**Pipeline:** Load → Resolve → Plan → Execute → Record. Phases communicate through serializable values (`Vec<Declaration>` → `TargetGraph` → `Plan` → `Vec<ActionOutcome>`), never by calling each other; each CLI command (`check`/`graph`/`query`/`plan`/`build`) is the pipeline truncated at a handoff.

**Incrementality:** the action key — SHA-256 over command, input hashes, each dependency's output hashes, env values, tool fingerprints, settings, os/arch. A key folds what a dependency *produced*, not the dependency's key, so a dependency rebuilt to the same bytes leaves downstream targets clean (early cutoff), and a target downstream of a dirty one is *conditional* until that output is known; `KeyComponents` is kept beside the digest so `plan` explains dirtiness as a struct diff. Crash-safety invariant: **cas first, cache row after, log always** — failures are never cached.

**Structural rules (architecture-building-blocks.md §7):**
- airsl is a dependency of the `buildl-lua` crate only; no other crate knows Lua exists.
- Dependency inversion: `buildl-core` holds domain data, ports (traits), and the pure logic of every phase, and depends on no buildl crate, no airsl, and no I/O API. Adapters depend on `buildl-core`; `buildl-lua` never depends on `buildl`, and adapter modules inside `buildl` never import each other (`cargo make guard-adapter-isolation`); what they share lives in `state` and `search_path`. `buildl` is the single composition root, and the only adapter crate that depends on another adapter crate (`buildl-lua`). Every pipeline flow must be testable in `buildl-core` against fake ports.
- Build files run with two globals: `airsstack` (airsl's default root table, holding the curated host modules — `json`, `path`, `regex`, `hash`, `glob`) and `buildl` (buildl's own module table — the declaration framework). They are one table bound twice: the `buildl` `HostModule` installs as `airsstack.buildl` and binds the same table to the global `buildl` inside `install` (design §5). Never rename the root table to `buildl`.

## Workspace shape

Four crates, published to crates.io in dependency order: `buildl-core` (domain data, ports, pure logic) → `buildl-lua` (the airsl adapter) → `buildl` (the remaining adapters and the composition root — the complete framework) → `buildl-cli` (thin clap shell, the `buildl` binary). The core is a crate, never a `mod core`: a crate-root module named `core` shadows Rust's built-in `core` crate. Workspace policy inherited verbatim from airsl: `unsafe_code = "forbid"`, `unwrap_used` and `panic` denied, pedantic + nursery clippy at warn, every dependency commented with its reason in the workspace `Cargo.toml`.

## Settled decisions — do not relitigate

Recorded in `architecture.md` §6 with rationale: plain threads for the build, with a tokio runtime confined to the remote adapter and reached through `block_on`, own graph arenas (petgraph declined), rayon confined to the `Digester` adapter, SHA-256 (REAPI compatibility over blake3 speed), JSON caches via one canonical sorted-key serializer, std mpsc channels, Lua exposure confined to `buildl-lua`. Structure recorded in `architecture-building-blocks.md` §10: four dependency-inverted crates, `buildl` as composition root, open ports behind one `Ports` bundle.

Determinism house rules (architecture.md §5) apply to any future code: no `HashMap` iteration reaches any output; all JSON through the one canonical serializer; parallelism never observable in artifacts; wall clock read only through the `Clock` port.

## Conventions

- Both spec docs carry `**Author:** rstlix0x0 · **Date:** … · **Reviewers:** —` headers and numbered reference lists; new design content should cite into those lists.
- License is Apache-2.0, following airsstack convention. buildl is part of the airsstack ecosystem and the first consumer of airsl's policy model and host-module seam.

## Development process

Every slice of work is a claudestacks-sdlc chain under `.claudestacks/sdlc/<date>-<topic>/`, taken through intent → spec → plans → execution.

|Topic|Practice|
|---|---|
|Where work happens|a git worktree on its own branch, never `main`|
|Commits|Conventional Commits, `type(scope): summary`; scope is the crate name, or `repo` for cross-cutting changes; one commit per plan task|
|Gates|`cargo make dod` (fmt, clippy `-D warnings`, doc `-D warnings`, tests, doctests) and `cargo deny check`, both green before each commit|
|Review|one reviewer pass per batch of tasks and one fix round; a disproven plan premise is amended in the plan, not only in the code|
|Findings|closed inside the slice that finds them: a slice is not done while a finding of its own is open, and nothing is carried forward to a later slice, plan or milestone|

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

**3. Finish the objective. Do not manufacture work.**

The task as asked is the deliverable. Deliver it and stop.

- **Do not hunt for edge cases.** Once the thing asked for works and is verified, it is done.
  Probing for further weaknesses, hypothetical failure modes, or "one more hole" is scope the
  user did not ask for and does not want.
- **Do not report every problem found.** Report only what blocks the objective or changes a
  decision the user must make. A defect you can fix in one edit: fix it silently and move on.
  A theoretical gap outside the task's scope: leave it out entirely.
- **No running "still open" lists.** Do not end responses with outstanding-items tables,
  open questions, or things "left for you to decide" unless the user genuinely cannot proceed
  without answering.
- **Ask at most what changes the work.** Prefer deciding with a stated assumption over asking.
  Several rounds of questions on one task is a failure, not thoroughness.
- **Rigour serves the objective, not itself.** Verifying a claim the deliverable rests on is
  right. Verifying claims nobody asked about, or re-verifying what is already proven, is
  over-complication and wastes the user's time.

When in doubt: what was asked for, done and checked, in the fewest moves.
