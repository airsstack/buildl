---
status: approved
created: 2026-09-13
spec: skipped
---

# Intent: Workspace crates for the dependency-inverted layout

> skip: the crate layout and dependency rules are already settled in `docs/architecture-building-blocks.md` §4 and §7; nothing is left for a design pass to decide.

## Problem

The workspace does not match the architecture the docs now describe. `docs/architecture-building-blocks.md` settles four dependency-inverted crates. The workspace has only two stubs, `buildl` and `buildl-cli`, and no dependency edges between them beyond `buildl-cli → buildl`. `buildl-core` and `buildl-lua` do not exist yet. Until they do, the next chain (the `buildl-core` foundation types) has nowhere to land, and the compiler cannot enforce the dependency rules.

The scaffold also still describes an earlier state of the project:

- `crates/buildl` and `crates/buildl-cli` rustdoc, `Cargo.toml` comments and READMEs still say "design phase" and "empty scaffold", point readers at `docs/`, and name the depfile question as open. The depfile question was resolved on 2026-09-13.
- Workspace `Cargo.toml` comments place dependencies in modules of the former single-crate layout (`declare`, `plan`, `manifest`, `store`).
- The `deny.toml` and `ci.yml` comments, and the status line in `CLAUDE.md`, describe the two-crate scaffold.

The airsl requirement is `0.1.3`. The decision is to accept the newest `0.1.x` release. Every published airsl release (`0.1.0` through `0.1.4`) declares `rust-version = 1.94`. The workspace pins toolchain and `rust-version` to `1.91`, where cargo refuses the dependency with `airsl@0.1.4 requires rustc 1.94`.

## Affected systems

- Workspace root: `Cargo.toml` (members, `rust-version`, dependency catalog and its comments), `rust-toolchain.toml`, `deny.toml`, `.github/workflows/ci.yml` comments, `CLAUDE.md` status line
- New crates: `crates/buildl-core`, `crates/buildl-lua`
- Existing crates: `crates/buildl`, `crates/buildl-cli` (manifests, rustdoc, READMEs)

## Desired outcome

- The workspace has four members: `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli`. Each is a documented stub with no domain types or logic.
- Crate dependency edges match `docs/architecture-building-blocks.md` §7, so the compiler enforces rules 1, 4 and the cross-crate half of rule 2:
  - `buildl-lua` → `buildl-core`, `airsl`
  - `buildl` → `buildl-core`, `buildl-lua`
  - `buildl-cli` → `buildl`
  - `buildl-core` → no buildl crate and no airsl
- The airsl requirement accepts any `0.1.x` release, and `cargo update` resolves to the newest.
- The pinned toolchain channel and the workspace `rust-version` are both `1.94`, matching airsl's minimum supported Rust version.
- No rustdoc, README, manifest or tooling comment describes the pre-resolution scaffold or the single-crate module layout.
- `cargo make dod` passes.
- `cargo deny check` passes. Any duplicate-version or license finding introduced by airsl's dependency graph is fixed, or recorded in `deny.toml` with a reason.

## Constraints

- Dependency rules from `docs/architecture-building-blocks.md` §1.2 and §7 are hard limits. airsl is reachable only from `buildl-lua`. `buildl-core` may use only `std` (without its I/O, process, thread, env and clock APIs), `serde`, `serde_json`, `thiserror` and `sha2`.
- The workspace policy is unchanged:
  - `unsafe_code = "forbid"`
  - `unwrap_used` and `panic` denied
  - pedantic and nursery clippy at warn
  - every workspace dependency commented with its reason
- The core is a crate, never a `mod core`, because a crate-root `core` module shadows Rust's built-in `core` crate.
- Rust guideline rules apply to the stubs:
  - `lib.rs` is export-only, with its crate-level doc.
  - rustdoc and crate READMEs carry no internal planning references, including `docs/` section citations.
- Wildcard (`*`) version requirements are not allowed; `deny.toml` bans them.
- Any edit under `docs/` keeps its Mermaid diagrams and its section cross-references intact.

## Non-goals

- Domain types, ports, pure logic, `Pipeline`, `FakePorts`. These belong to the `buildl-core` foundation chain and the chains after it.
- Any `DeclarationSource` implementation or `buildl` module table in `buildl-lua`.
- Any adapter code or CLI argument parsing.
- Publishing: no `cargo publish --dry-run`, no crates.io readiness checks.
- Moving airsl to `0.2` or any other breaking series.
