---
status: done
created: 2026-10-10
---

# Intent: Resolve — from `Vec<Declaration>` to `TargetGraph`

## Problem

`buildl-core` can load a workspace: Load turns its build files into a validated, sorted
`Vec<Declaration>`, and `Pipeline::check` stops there. Nothing relates one declaration to another, so
the pipeline cannot go further:

- **No graph value.** `architecture.md` §2 names `TargetGraph` as the second phase handoff and §3.2
  gives its shape, but no type holds a graph. `NodeId` exists and indexes nothing. I5 Plan has no
  input to plan over.
- **Nothing checks a declaration against the others.** Load validates each value alone and leaves
  every question that needs all build files open (I3 spec §11): whether a label is declared twice,
  whether a `deps` entry, a `rule =` reference or an alias target names anything, whether it names the
  right kind of thing, whether a `$opt:name` placeholder names a declared setting, and whether the
  dependencies form a cycle. A workspace wrong in any of these ways passes `check` today.
- **Errors across files are not attributable.** Design §8.2 requires three failures that each name
  their sites: a duplicate names both declarations, an unknown reference suggests the nearest declared
  name, and a cycle reports its whole path. No error variant carries any of this.
- **The pipeline has no `graph` handoff.** `Pipeline` has one method, `check`. The `graph` command
  (design §7) has nothing to serialize.
- **The "no phase names another" rule is unguarded.** Roadmap follow-up #11 was deferred until two
  phase modules exist. This slice adds the second, so the rule is breakable from here on with a green
  gate.

This is the next rung of the milestone 1 ladder (`docs/roadmap.md` §3): I5 Plan consumes this slice's
output and is blocked on it.

Decided in the dialogue: **grants leave this slice.** `docs/roadmap.md` listed "grants (wanted set ∩
ceiling)" under I4, but `architecture.md` §3.2 and `architecture-building-blocks.md` §5.4 give resolve
no port, `architecture.md` §3.3 runs the ceiling check in Plan, and the `graph` command's output does
not depend on a grant. The wanted set, the ceiling intersection and the `Manifest` and `Approver` ports
move to I5 Plan. Follow-up #13, which needs the `Manifest` port, moves with them.

Questions the documents leave open, which the design pass has to close:

- What a graph node is: `architecture.md` §2 draws `nodes: Vec<Target>`, while Load hands over four
  kinds of declaration, and rules, aliases and settings are not targets.
- Where a `rule` reference is expanded into the command of the target that uses it — here or in I5
  (left open by the I3 intent).
- What each reference may name: whether a dependency may name an alias, whether an alias may name
  another alias, and which kind mismatches are errors.
- Whether Resolve stops at the first failure or reports every one it finds, given that the crate's
  `Result` carries one `Error`.
- The order failures are reported in when a workspace has several, so the same workspace always
  reports the same one.
- Which settings checks belong here: `architecture.md` §8 leans towards validating `--set` against
  declarations at Resolve, but no invocation value reaches the pipeline yet.
- Whether `Pipeline::graph` repeats `check`'s double load or loads once.
- The serialized shape of the graph, which becomes `graph.json` and has to stay diff-stable.

## Affected systems

- `crates/buildl-core` — the graph type, the Resolve logic as a second phase module, new error
  variants, `Pipeline::graph`, the flow tests and their fakes, the crate rustdoc and `README.md`
- `Makefile.toml` and a golden file beside `crates/expected-edges.txt` — the guard of follow-up #11
- `docs/roadmap.md` — the I4 and I5 rows, follow-ups #11 and #13, §4 on completion
- `docs/architecture.md`, `docs/architecture-building-blocks.md`, `docs/design.md`, `README.md` — only
  where a decision closed here contradicts or settles text there

## Desired outcome

- A `Vec<Declaration>` becomes one `TargetGraph` through the Resolve logic in `buildl-core`, and the
  pipeline can be run up to the `graph` handoff — the slice's done condition in `docs/roadmap.md` §3.
- The graph carries what later phases need without recomputing it: forward and reverse edges, lookup by
  label, and a dependency count per node (design §8.2).
- A label declared twice, a reference that names nothing, a reference that names the wrong kind of
  thing and a dependency cycle each fail with an error that names every declaration site involved. The
  unknown-reference error suggests the nearest declared name; the cycle error lists the whole path.
- A workspace that fails Resolve fails the same way on every run.
- The same set of declarations always serializes to the same bytes through the canonical serializer,
  and a golden `graph.json` pins that shape.
- A `cargo make` guard turns the gate red when one phase module imports another, closing follow-up #11.
- Every new type and flow is tested in `buildl-core` with no Lua, filesystem, process or thread.
- `buildl-core`'s rustdoc and `README.md` describe what the crate now holds, and `docs/roadmap.md`
  shows grants and follow-up #13 under I5.
- `cargo make dod` and `cargo deny check` pass, including every guard.

## Constraints

- Resolve is pure: it takes declarations and returns a graph or an error, and uses no port
  (`architecture.md` §3.2, `architecture-building-blocks.md` §5.4). The `Ports` bundle gains nothing in
  this slice.
- `buildl-core`'s dependency set is unchanged (`architecture-building-blocks.md` §1.2). No graph
  library: own arenas, petgraph declined (`architecture.md` §6).
- No recursion over the graph: a deep graph must not overflow the call stack (`architecture.md` §3.2).
- Phases communicate through values; no phase module names another, and only `pipeline/` names more
  than one (`architecture.md` §1.1, the five rules in `buildl-core`'s crate doc).
- Determinism house rules (`architecture.md` §5) bind every line: no `HashMap` iteration reaches any
  output, all JSON goes through the canonical serializer.
- `Provenance` is threaded, never reconstructed (`architecture.md` §2): every error site comes from the
  declaration that carries it.
- From Resolve onward a target is a `NodeId`; strings appear only where something is reported
  (`architecture.md` §2).
- The error model stays one `#[non_exhaustive]` enum with structured fields; variants are added, none
  reshaped (`architecture.md` §4).
- The graph is a serializable handoff (`architecture.md` §1.1).
- `Declaration` and the `DeclarationSource` contract are not reshaped; `buildl-lua` is not touched.
- Workspace policy and the Rust guidelines apply unchanged: export-only `lib.rs`, no internal planning
  references in rustdoc or crate READMEs.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The wanted set, the ceiling intersection, grants, the `Manifest` and `Approver` ports — I5 Plan.
- Roadmap follow-up #13 (excluding `out` and `.buildl` from `b.sources()`) — I5, with the `Manifest`
  port.
- The action key, `KeyComponents`, dirtiness, tool resolution — I5.
- Placeholder expansion (`$in`, `$out`, `$deps`, `$opt:name` substitution) — the executor's, I6.
- `--set` values reaching the pipeline, and checking them against declared settings.
- Selecting the subgraph a requested target needs, and demand-driven loading (design §11.1).
- `query` and `rdeps` — milestone 2.
- Writing `.buildl/graph.json` to disk, the `--dot` rendering, and argument parsing — the `buildl`
  adapters and the CLI slice.
- Parallel resolve.
- Roadmap follow-up #5, which is assigned to the CLI slice.
