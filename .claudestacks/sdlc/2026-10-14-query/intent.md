---
status: done
created: 2026-10-14
milestone: buildl/02
---

# Intent: query — ask the graph what a target needs and what needs it

## Problem

Milestone 1 is done: `buildl` runs `check`, `graph`, `plan` and `build` on a real workspace. Nobody
can yet ask it a question about the graph.

- **"What does this target need?" has no answer short of reading every build file.** `buildl graph`
  prints the whole graph; finding one target's transitive dependencies in it is manual work.
- **"What does this change touch?" has no answer at all.** Design §11.1 calls reverse queries "the
  killer feature": `buildl query "rdeps(//proto:go)"` maps a diff to the targets that must rebuild
  and the tests that must run. Design §10.2 counts on `rdeps` to run only the tests a diff touches,
  and §10.3 on it for package-granular CI. Without it, CI on a large repository rebuilds and retests
  everything.
- **The command is promised and absent.** Design §7 lists `buildl query <expr>` (stops after
  Resolve) with `deps()` and `rdeps()`; README marks it "not yet". `buildl-cli` has no `query`
  subcommand and nothing parses an expression.
- **The documents describe it inconsistently.** Design §7 cites "(§10.1)" for `query`, but §10.1 is
  about customization; `rdeps` appears in §10.2 and §11.1. `docs/roadmap.md` §2, design §13 item 2
  and the README's sequencing list name "`query` / `rdeps`" as if they were two commands, while the
  command tables show `rdeps` only as a function inside `query`.

This is the first slice of milestone 2 (design §13 item 2). The milestone 2 ladder agreed with the
author: **I10 query** → I11 test → I12 log → I13 clean + gc → I14 parallel load → I15 demand-driven
load (named-target commands only).

Assumed, and overridable:

- **One chain.** The expression, its evaluation over the graph and the CLI command cannot be shown to
  work apart; the slice spans `buildl-core`, `buildl` and `buildl-cli` like I9 did.
- **The whole graph is loaded eagerly.** `rdeps` needs every reverse edge, so `query` never uses the
  demand-driven loading of I15.
- **The roadmap gains the milestone 2 ladder in this slice**, since this is the chain that opens
  milestone 2.

## Affected systems

- `buildl-core` — evaluating `deps()` / `rdeps()` over `TargetGraph` (its reverse edges already
  exist, `architecture.md` §3.2); the `Pipeline` entry for a command that stops after Resolve.
- `buildl` — the `Workspace` facade method for `query`.
- `buildl-cli` — the `query` subcommand, its output on stdout, its exit codes.
- `docs/design.md` §7, `docs/roadmap.md` §2–§3, `README.md` command table, `docs/architecture.md`
  and `docs/architecture-building-blocks.md` where they describe commands and pipeline entries.

## Desired outcome

- `buildl query "deps(//app:server)"` prints every target `//app:server` depends on, transitively.
- `buildl query "rdeps(//proto:go)"` prints every target that depends on `//proto:go`, transitively.
- Output is sorted labels, one per line, on stdout only; `--json` gives the same set as canonical
  JSON. The same workspace always gives byte-identical output.
- A malformed expression or a label nothing declares fails with the design §8.7 exit code for a bad
  command line (2), with a nearest-label suggestion as `build` gives today.
- The documents agree: design §7 cites the right sections, the roadmap shows milestone 2's ladder and
  this slice's status, README marks `query` as available.
- An end-to-end test runs the `buildl` binary against a fixture workspace and checks both functions.

## Constraints

- `query` stops after Resolve (design §7): load and resolve only, no plan, no execution, no state
  directory, no lock — like `check` and `graph`.
- The determinism rules of `architecture.md` §5 hold: no `HashMap` iteration in output, JSON only
  through the canonical serializer.
- The structural rules hold: `buildl-core` stays free of I/O, phase modules do not import each other,
  adapter modules of `buildl` do not import each other (`cargo make` guards).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one
  (`docs/roadmap.md` §6).

## Non-goals

- Any query function beyond `deps()` and `rdeps()`: set operators, depth limits, filters such as
  "tests only", file-path arguments. Each arrives when a slice needs it.
- `test`, `log`, `clean`, `gc` (I11–I13) and parallel or demand-driven loading (I14–I15).
- Running anything `query` returns — mapping a diff to a build or test run is a shell pipe over its
  output, not part of the command.
