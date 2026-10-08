---
status: done
created: 2026-10-08
---

# Intent: I-lua — evaluating a real `build.lua` through airsl

## Problem

I3 gave `buildl-core` a complete Load phase: the `DeclarationSource` port, the staged values it
returns, the closed `EvaluationFailure` set, and the traversal and merge that turn staged files into
`Vec<Declaration>`. Nothing implements the port yet. `buildl-lua` holds only a crate doc, so no real
build file can be evaluated, and the pipeline runs only against `FakePorts`:

- **No evaluator.** Nothing reads a build file, runs it in an airsl sandbox and returns its
  `StagedFile`, reports a missing file as `Evaluated::Absent`, or classifies an airsl failure as an
  `EvaluationFailure`.
- **No declaration surface.** Design §5 promises build files a `buildl` module table (`target`,
  `rule`, `alias`, `test`, `option`, `subdir`, `sources`), reachable as both `airsstack.buildl` and
  the global `buildl`, beside airsl's curated modules. None of it exists.
- **No `b.sources()`.** Design §5 makes it the declaration phase's only filesystem read, returning
  sorted results under the workspace-read grant. It is unbuilt, and where its globbing runs is still
  open (`architecture-building-blocks.md` §11).
- **The determinism promise has a hole.** Design §12.1 says dropping `os` and curating the module set
  makes declaration deterministic by construction. airsl's surfaces keep `math` whole, and Lua 5.4
  seeds `math.random` from entropy, so a build file can still declare something different on each
  run.
- **Untrusted input can exhaust host memory.** Staged declarations sit in a Rust-side buffer that the
  `[declaration] memory` ceiling does not count (roadmap follow-up #12).

This is the next rung of the milestone 1 ladder (`docs/roadmap.md` §3), and its port has been ready
since I3 closed. It is independent of I4–I6. The `buildl` adapters and the CLI need it before the
first real `buildl check` can run.

## Affected systems

- `crates/buildl-lua`: its first real code, `Cargo.toml`, crate rustdoc and `README.md`
- workspace `Cargo.toml`: the airsl requirement (follow-up #4) and any dependency the slice adds,
  each with a reason comment
- `docs/roadmap.md`: the I-lua row, §4 on completion, and §5 (close #3 for `buildl-lua`, #4 and
  #12)
- `docs/design.md` §5 and §12.1, `docs/architecture-building-blocks.md` §11: only where a decision
  here settles or contradicts their text

## Desired outcome

- `build.lua` fixtures evaluated through real airsl produce exactly the expected staged values. This
  is the roadmap's done condition.
- Every design §5 primitive is callable from a build file as `buildl.<name>` and as
  `airsstack.buildl.<name>`, and both names refer to one table. The airsl root table stays
  `airsstack`, with the curated modules (`json`, `path`, `regex`, `hash`, `glob`) reachable and no
  others: `fs`, `proc`, `env`, `time` and `stdio` are absent. `buildl` is a child of `airsstack`
  and does not inherit from it. The table holds only buildl's own API, so `buildl.json` is `nil`
  and build files reach airsl modules through `airsstack`.
- `b.sources(pattern)` returns sorted paths relative to the declaring directory, cannot read outside
  the workspace, and is the declaration phase's only filesystem access.
- Each build file runs on a fresh engine. Evaluating the same file twice yields byte-identical staged
  output, `math.random` included. The one entropy source left open, address-derived text from
  `tostring` or `string.format` of a table or function, is named and recorded as a follow-up.
- Every way an evaluation can fail maps onto I3's `EvaluationFailure` (`Syntax`, `Runtime`,
  `Refused`, `LimitReached{Instructions|Memory|Staging}`, `UnknownField`, `WrongFieldType`), with a
  `Diagnostic` a person can act on. No change to `buildl-core` is needed.
- The per-file staging cap is enforced and refused as `LimitReached{Staging}` (follow-up #12).
- The airsl floor is raised to the version the adapter is tested against (follow-up #4).
  `buildl-lua`'s rustdoc and `README.md` describe what the crate holds (follow-up #3).
- An end-to-end test runs core's `load` over a fixture workspace through this adapter and yields
  `Vec<Declaration>`.
- `cargo make dod` and `cargo deny check` pass, including `guard-crate-edges`.

## Constraints

- `buildl-lua` depends only on `buildl-core` and `airsl` among buildl-relevant crates, never on
  `buildl` (`architecture-building-blocks.md` §7). No other crate learns Lua exists.
- `buildl-core` is not amended. I3 fixed the `DeclarationSource` contract and the
  `EvaluationFailure` set so this slice could not need to. A disproven premise is raised, not patched
  around.
- The adapter records what a build file wrote. Validation, label resolution, deduplication,
  traversal and ordering stay in Load (I3 decision D2). Lua never recurses into another build file
  and never holds a real artifact path (design §5).
- airsl stays on the `0.1` series (`docs/roadmap.md` §3). Moving to `0.2` is its own decision.
- Never rename the root table to `buildl`. The module is installed as `airsstack.buildl` and binds
  the same table to the global (design §5, CLAUDE.md).
- Declaration is safe on untrusted input (design §2). Every resource a build file can consume is
  bounded, and every refusal names what was refused.
- Determinism house rules (`architecture.md` §5): no `HashMap` iteration reaches output, and
  `pairs` order cannot change the result.
- `buildl.toml` and the `Manifest` port do not exist yet. Whatever the adapter needs from them
  (workspace root, entry name, `[declaration]` limits) arrives some other way until they do.
- Workspace policy and the Rust guidelines apply unchanged (forbid `unsafe`, deny `unwrap`/`panic`,
  pedantic and nursery clippy).

## Non-goals

- The `buildl.toml` parser, the `Manifest` port, the ceiling, the wanted set and grants: I4 and
  milestone 4.
- `LocalPorts` and the composition root in `buildl`, and any `buildl-cli` command wiring: the
  adapters slice.
- Parallel load across engines.
- Plugins and `buildl.use` (design §10): milestone 5.
- Implicit `subdir` by file discovery: rejected in I3, not deferred.
- Follow-ups #3 for `buildl` and `buildl-cli`, and #5–#11: assigned elsewhere.
- Moving airsl to `0.2`.
