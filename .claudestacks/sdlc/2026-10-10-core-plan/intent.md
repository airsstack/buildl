---
status: done
created: 2026-10-10
---

# Intent: Plan — from `TargetGraph` to `Plan`

## Problem

`buildl-core` can resolve a workspace: Resolve turns its declarations into a validated `TargetGraph`,
and `Pipeline::graph` stops there. Nothing looks at the world outside the graph, so the pipeline
cannot answer the question design §13 names as buildl's defining capability — _does this need to run
at all?_

- **No action key.** Design §8.3 and `architecture.md` §2 make the action key the whole incremental
  mechanism, and design §12.4 lists what it must account for: source files, products of other
  targets, environment values, tools, build settings, host OS and architecture. None of `KeyComponents`,
  `ActionKey` or `Plan` exists. A target has no identity beyond its label, so nothing can be compared
  with a previous build.
- **No dirtiness, and no explanation of it.** Design §7 requires that `plan` explains itself from day
  one: every target that would rebuild names why. Nothing computes which targets are dirty, and
  nothing retains the reason.
- **Nothing bounds a build's authority.** Design §6 derives the wanted set from the graph and
  intersects it with the ceiling in `buildl.toml`, failing closed before anything runs. The pipeline
  knows no ceiling and derives no wanted set, so a workspace that names a program, an environment
  variable or an output root outside the ceiling would pass every command that exists today.
- **The pipeline cannot learn anything from outside.** Plan needs file contents, file metadata, the
  tools a command names, the keys of the previous build and the workspace manifest. `Ports` names one
  port, `DeclarationSource`. The port catalog (`architecture-building-blocks.md` §6) lists
  `Manifest`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache` for these, and lists nothing
  for two key components `buildl-core` is forbidden to read itself: environment values and the host
  OS and architecture.
- **No invocation value reaches the pipeline.** Build settings fold into the key (design §10.2), and
  the value that matters is the one a run overrides with `--set`. `Pipeline`'s methods take the entry
  file name and nothing else. Where an override is checked against the declared settings is still
  open in `architecture.md` §8.
- **The pipeline has no `plan` handoff.** `Pipeline` has `check` and `graph`. The `plan` command
  (design §7) has nothing to report, and I8 Execute + Record has no input.
- **Two promises in the documents contradict each other.** Design §8.3 and `architecture.md` §3.3
  fold each dependency's _action key_ into a target's key. Design §8.5, `fundamental-walkthrough.md`
  §4 and a flow test in `architecture-building-blocks.md` §8 promise early cutoff: a rebuilt output
  that is byte-identical leaves downstream targets clean. A key that holds a dependency's action key
  changes whenever that dependency's inputs change, whatever it produced, so both cannot hold as
  written.
- **`b.sources()` lists build outputs** (roadmap follow-up #13). Which directories to skip is known
  only from the manifest, and nothing in `buildl-core` knows the manifest.

This is the next rung of the milestone 1 ladder (`docs/roadmap.md` §3): I8 Execute + Record consumes
this slice's output and is blocked on it.

Decided in the dialogue:

- **One chain, as the roadmap has it.** The action key, dirtiness and the authority check stay
  together, so the slice ends at the `plan` command's handoff like every slice since I4. Its size is
  handled by the plan fan-out.
- **Early cutoff is binding.** Whatever the key holds for a dependency, this slice must not close off
  early cutoff. Where design §8.3's wording contradicts that, the wording is amended.
- **Setting overrides reach Plan.** A `plan` run accepts overrides, checks them against the declared
  settings and keys on the effective values.
- **Follow-up #13 is split.** This chain delivers what the fix needs from `buildl-core`: the
  workspace's output directories, known from the manifest. The change to `buildl-lua`'s sources walk
  is re-targeted in `docs/roadmap.md` to the adapters slice, where a manifest is first read.

Assumed, and overridable: **the `Approver` port leaves this slice.** Design §13 makes the approver
milestone 4. Here the wanted set is intersected with the ceiling and anything outside it refuses the
plan; nothing asks anyone.

Questions the documents leave open, which the design pass has to close:

- What a key holds for a dependency so that early cutoff holds, and what follows for a target
  downstream of a dirty one: whether Plan's verdict on it is final or provisional until its
  dependencies have run.
- Which port supplies environment values, and which supplies the host OS and architecture.
- Whether the build file that declared a target is itself a key input (design §7 rule 2 and the last
  row of §12.4 say build files are inputs; `architecture.md` §2's component list does not name them).
- Which arguments of a command name a tool, and what a tool fingerprint is.
- What an output root is in the wanted set, and how a path is compared with the `fs.read` and
  `fs.write` ceilings.
- How `always = true` and `network = true` appear in a plan.
- What a dirty reason holds when a target was never built, and when several components differ.
- Whether Plan only reads the stat cache or also updates it, and who persists the update.
- Which failure is reported when a workspace has several — a refusal, a missing tool, an unknown
  override — and whether one or all, so the same workspace always reports the same thing.
- What the `Manifest` port exposes, and how much of `buildl.toml` that is.
- The serialized shape of a `Plan`.

## Affected systems

- `crates/buildl-core` — the key, plan and authority types, the Plan logic as a third phase module,
  the new ports and the `Ports` bundle, new error variants, `Pipeline::plan`, the flow tests and
  their fakes, the crate rustdoc and `README.md`
- `crates/expected-module-edges.txt` — the module-edge guard's golden file
- `docs/roadmap.md` — the I7 row, follow-up #13's split, §4 on completion
- `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md`,
  `docs/fundamental-walkthrough.md`, `README.md` — only where a decision closed here contradicts or
  settles text there: the early-cutoff contradiction, the port catalog's gaps, the settings question

## Desired outcome

- A `TargetGraph` becomes one `Plan` through the Plan logic in `buildl-core`, and the pipeline can be
  run up to the `plan` handoff — the slice's done condition in `docs/roadmap.md` §3.
- Every target has an action key that accounts for every entry of the design §12.4 ledger that is
  Plan's to account for. The same graph in the same world always yields the same key bytes.
- Every dirty target carries a reason that names what differs from the previous build, and a target
  that was never built says so.
- Early cutoff is reachable: nothing in the key or the plan forces a target to rebuild when its
  dependencies' outputs are unchanged. The documents state one rule, not two.
- The wanted set is derived from the graph. A build that wants a program, an environment variable or
  an output root outside the ceiling is refused at plan time, before anything could run, and the
  refusal names what was wanted, what the ceiling allows, the target and the file that declared it.
- A tool a command names that cannot be found fails the plan, naming the target and its declaring
  file (design §9.6).
- A setting override changes the keys of exactly the targets that refer to that setting, and an
  override that names no declared setting is refused.
- A file unchanged since the previous run is not hashed again.
- `buildl-core` knows the workspace's output directories from the manifest.
- A workspace that fails Plan fails the same way on every run.
- The two flows `architecture-building-blocks.md` §8 assigns to Plan run in `buildl-core` against
  fakes: a wanted program outside the ceiling is refused before anything runs, and one changed input
  yields a dirty reason that is the difference between two sets of key components.
- Every new port has a fake built from the public API alone, and every new type and flow is tested
  with no Lua, filesystem, process or thread.
- `buildl-core`'s rustdoc and `README.md` describe what the crate now holds, and `docs/roadmap.md`
  shows the slice's state and follow-up #13's split.
- `cargo make dod` and `cargo deny check` pass, including every guard.

## Constraints

- `buildl-core` names no I/O API (`architecture-building-blocks.md` §1.2): every fact about files,
  tools, the environment, the host and the manifest arrives through a port. Every trait lives in
  `ports`, and the crate gains no production implementation of one.
- `buildl-core`'s dependency set is unchanged. Parallel hashing is the `Digester` adapter's; no
  parallelism enters the core (`architecture.md` §6).
- Keys are SHA-256 over bytes from the one canonical serializer, so the key of a value and the file
  of a value cannot disagree (`architecture.md` §5 rule 2).
- Determinism house rules (`architecture.md` §5) bind every line: no `HashMap` iteration reaches any
  output, and Plan does not read the clock.
- The ceiling is never exceeded: a request is intersected with it, never unioned, and a request
  outside it fails closed (design §4, §6).
- Phases communicate through values; no phase module names another, and only `pipeline/` names more
  than one (`architecture.md` §1.1). `cargo make guard-module-edges` stays green.
- No recursion over the graph (`architecture.md` §3.2).
- From Resolve onward a target is a `NodeId`; strings appear only where something is reported
  (`architecture.md` §2).
- `Provenance` is threaded, never reconstructed: every error that names a target takes its site from
  the graph.
- The error model stays one `#[non_exhaustive]` enum with structured fields; variants are added, none
  reshaped (`architecture.md` §4).
- `Plan` is a serializable handoff (`architecture.md` §1.1).
- `Declaration`, `TargetGraph` and the `DeclarationSource` contract are not reshaped; `buildl-lua` is
  not touched.
- Workspace policy and the Rust guidelines apply unchanged: export-only `lib.rs`, no internal planning
  references in rustdoc or crate READMEs.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The `Approver` port, the interactive prompt and CI detection — milestone 4.
- Running anything: `Schedule`, `Dispatcher`, `ExecStrategy`, and every write to the cas, the action
  cache and the log — I8.
- Placeholder expansion (`$in`, `$out`, `$deps`, `$opt:name` substitution) — the executor's, I8.
- Every real adapter: the `buildl.toml` reader, the filesystem digester, `.buildl/statcache.json`,
  the `PATH` lookup, `.buildl/cache.json` — the `buildl` adapters slice.
- The `buildl-lua` half of follow-up #13 — the adapters slice.
- Parsing `--set`, and rendering a plan for a terminal — the CLI slice. Follow-up #5 stays there too.
- Selecting the subgraph a requested target needs, and demand-driven loading (design §11.1).
- Hermetic toolchains, `b.fetch` and tools that arrive as dependencies (design §9.6 level 3) —
  milestone 5.
- Per-target grant narrowing (design §14).
- Keying the stat cache by device and inode (`architecture.md` §8).
- `query` and `rdeps`, the remote cache, watch mode's re-plan, `doctor` and `verify`.
- Parallel plan.
