---
status: done
created: 2026-10-11
---

# Intent: Execute and Record — from `Plan` to a finished build

## Problem

`buildl-core` can plan a workspace: Plan gives every target a verdict — clean, dirty with a reason, or
conditional on a dependency's output — and `Pipeline::plan` stops there. Nothing acts on a verdict, so
the pipeline can say what a build would do and cannot do it.

- **Nothing runs.** Design §8.4 describes a ready queue, workers and per-target dependency counters;
  `architecture.md` §2.1 gives every target a state during a build. None of it exists. A dirty target
  stays dirty forever, because no build ever produces what its verdict is waiting for.
- **A conditional verdict has nobody to settle it.** Plan leaves a target downstream of a dirty one
  with an unfinished key, to be finished "from the graph and its dependencies' new rows"
  (`architecture.md` §3.3). Nothing finishes it. Early cutoff — a dependency rebuilt to the same bytes
  leaves its dependents alone (design §8.5) — is reachable in the key and observable nowhere.
- **Nothing is ever recorded.** `ActionCache` can read a row and cannot write one; a `CacheRow` holds a
  key and output digests and no captured output; there is no cas, no log, and no value for what one
  action did. Every `plan` of every workspace therefore reports `NeverBuilt`. The flow tests for Plan
  stand in for the missing phase by writing rows by hand (`crates/buildl-core/tests/flows/plan.rs`,
  `record` and `build`).
- **The crash-safety invariant is a sentence, not a property.** Design §8.5 and §8.7 promise that a
  cache row exists only once its outputs are in the cas, that a failure is never cached, and that an
  interrupted build keeps its finished work. Nothing orders a cas write before a row, so nothing can be
  tested to hold.
- **A failure has no shape.** Design §8.7 separates an action that failed from a builder that is sick,
  stops new work by default, continues past a failure under `--keep-going`, and reports what was never
  attempted as skipped rather than failed. `OutcomeClass` (`architecture.md` §4) does not exist, and
  no target can be skipped.
- **A command is never expanded.** `$in`, `$out`, `$deps` and `$opt:name` are still text in every
  command the graph holds; Plan left expansion to the executor. A command whose first argument holds a
  placeholder named no program at plan time, so it was neither checked against `proc.run` nor
  fingerprinted (roadmap follow-up #14).
- **A plan trusts a cache row it cannot check.** A clean verdict says the key matches; it does not say
  the outputs that row names are still in the cas (roadmap follow-up #15).
- **The pipeline has no `build` handoff.** `Pipeline` has `check`, `graph` and `plan`. The `Ports`
  bundle names seven ports, none of which stores content, appends to a log, runs an action or
  dispatches work; the `Clock` port exists and no flow reaches it.
- **The documents disagree with each other, and with what Plan settled.**
  - `architecture.md` §3.5 has the action cache "rewritten atomically once per build"; design §8.5 has
    an interrupted build remember its finished work. A cache written once, at the end, remembers
    nothing of an interrupted build.
  - `architecture.md` §2.1 moves a target into `Clean` only because "plan says key matches". A
    conditional target that turns out unchanged once its dependency has run has no transition.
  - `OutcomeClass` carries a `Refusal` variant marked plan-time only. Plan reports a refusal as an
    error and returns no plan, so no outcome can hold one.
  - Design §8.5 links outputs into `out/` after hashing them into the cas. The port catalog
    (`architecture-building-blocks.md` §6) has a port that stores content and none that places a
    stored output anywhere.
  - `Dispatcher` is in the catalog and in the fakes table and has no signature in
    `architecture-building-blocks.md` §5.2; `ReadyAction`, `RawOutcome` and `OutcomeEvent` are named
    there and described nowhere.

This is the last rung of the milestone 1 ladder inside `buildl-core` (`docs/roadmap.md` §3). The
`buildl` adapters and the CLI come after it and are blocked on the ports it settles.

Decided in the dialogue:

- **One chain, as the roadmap has it.** Execute and Record stay together: the slice ends at the `build`
  command's handoff, and neither half can be shown to work without the other. Its size is handled by
  the plan fan-out.
- **The `Reporter` port leaves this slice.** A build returns what happened as values; printing output
  blocks and a status line belongs with the adapters and the CLI.
- **A build is of the whole graph.** Selecting the subgraph a requested target needs stays out, as it
  did for Plan.
- **Follow-ups #14, #15 and #16 are closed here.**

Assumed, and overridable:

- **Design §8.5 is the binding statement of crash safety.** Finished work survives an interrupted
  build. Where `architecture.md` §3.5's wording contradicts that, the wording is amended.
- **The `Approver` port stays out**, as in Plan: milestone 4.

Questions the documents leave open, which the design pass has to close:

- When a recorded row becomes durable, so that an interrupted build keeps finished work and a failed
  write leaves no row.
- What happens to a conditional target at run time, what states a target passes through, and what a
  target that was not run because nothing changed is reported as.
- Where placeholder expansion happens, given that `buildl-core` holds no real path and each isolation
  tier lays its execution directory out differently; and what `$in`, `$out`, `$deps` and `$opt:name`
  each expand to.
- What follows for a command whose program is known only after expansion: what it is checked against,
  and whether it reaches the key.
- How a produced output reaches the cas and then `out/`, and through which port.
- What a clean target whose outputs are missing from the cas becomes.
- The shape of the dispatching port, such that the scheduling logic is tested on one thread and an
  adapter may run actions in parallel.
- What an outcome, a ready action, a strategy's raw result and a log event each hold.
- Which failure classes an outcome can carry, what distinguishes them, and what becomes of `Refusal`.
- What `build` returns, and when a build as a whole is an error rather than a set of outcomes.
- Which invocation and manifest values reach a build — keep-going, the per-action timeout — and how.
- What a target that always runs records, and whether a whole-graph build runs test targets.
- What a failure of a port itself — the cas, the cache, the log — does to the build in progress.

## Affected systems

- `crates/buildl-core` — the outcome and event types, `CacheRow`, the Execute and Record logic as new
  phase modules, the new ports and the grown `ActionCache`, the `Ports` bundle and `PortSet`, new error
  variants, `Pipeline::build`, the flow tests and their fakes, the crate rustdoc and `README.md`
- `crates/expected-module-edges.txt` — the module-edge guard's golden file
- `docs/roadmap.md` — the I6 row, follow-ups #14–#16, §4 on completion
- `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md`, `README.md`,
  `CLAUDE.md` — only where a decision closed here contradicts or settles text there: the cache's
  durability, the target state machine, `OutcomeClass`, the port signatures and the catalog

## Desired outcome

- A `Plan` becomes a finished build through the Execute and Record logic in `buildl-core`, and the
  pipeline can be run up to the `build` handoff against fakes — the slice's done condition in
  `docs/roadmap.md` §3.
- Every target of a build ends in exactly one reported state, and the states add up to a summary of
  the kind design §8.7 shows: `1 failed, 2 skipped, 2 built, 9 cached`.
- A target runs only after every target it depends on has finished successfully, and never twice in
  one build.
- A conditional target runs exactly when a dependency it waits on produced something other than what
  the previous build recorded. A dependency rebuilt to the same bytes leaves its dependents not run:
  early cutoff holds in a build, not only in a plan.
- After a failure, no new work starts by default. Under keep-going, everything not downstream of the
  failure is built. Everything downstream is skipped and names the failure it is downstream of.
- An action that failed and a builder that could not run it are told apart by what happened, never by
  message text, and the difference is carried on the outcome.
- A cache row exists only for a target whose outputs are in the cas. A failed action leaves no row; a
  cache write that fails leaves no row; a build interrupted part-way keeps the rows of the work it
  finished. The documents state one rule.
- A second build of an unchanged workspace runs nothing and reports every target as cached.
- A command reaches a strategy with its placeholders expanded. A program first named by expansion is
  held to the ceiling before it runs (follow-up #14).
- A target is never reported as cached while an output its row names is missing from the cas
  (follow-up #15).
- What an action printed is kept beside what it produced: captured output goes into the cas and its
  digests into the row, so a later build can show it again (follow-up #16).
- Every outcome, whatever its class, yields one log event that names the target, the command as it
  ran, the digests involved, how long it took and the class.
- The same plan with the same action results yields the same rows and the same cas contents whatever
  order the actions finish in.
- A build that fails does so the same way on every run of the same workspace.
- The flows `architecture-building-blocks.md` §8 assigns to Execute and Record run in `buildl-core`
  against fakes: a failure under keep-going yields the expected summary; a cas write that succeeds
  followed by a cache write that fails leaves no row; a rebuilt output that is byte-identical leaves
  downstream targets clean.
- Every new port has a fake built from the public API alone, and every new type and flow is tested
  with no Lua, filesystem, process or thread.
- `buildl-core`'s rustdoc and `README.md` describe what the crate now holds, and `docs/roadmap.md`
  shows the slice's state with follow-ups #14–#16 closed.
- `cargo make dod` and `cargo deny check` pass, including every guard.

## Constraints

- `buildl-core` names no I/O API (`architecture-building-blocks.md` §1.2): no filesystem, process,
  thread, environment or clock call. Storing, running, dispatching and reading the time each arrive
  through a port. Every trait lives in `ports`, and the crate gains no production implementation of
  one.
- **Cas first, cache row after, log always; failures are never cached** (design §8.5, §8.7).
- Scheduling is single-writer and holds no lock (`architecture.md` §3.4); no parallelism enters the
  core, and none is observable in any artifact (`architecture.md` §5 rule 3).
- The wall clock is read only through the `Clock` port, and only for event timestamps and durations
  (`architecture.md` §5 rule 4).
- No shell, ever: a command is an argument vector (`architecture.md` §3.4).
- A failure never cancels an action already running (design §8.7).
- Classification is structural, made at the site that knows (`architecture.md` §4).
- The ceiling is never exceeded: anything first known at run time is intersected with it and fails
  closed (design §4, §6).
- Determinism house rules (`architecture.md` §5) bind every line: no `HashMap` iteration reaches any
  output, and every serialized byte goes through the one canonical serializer.
- Phases communicate through values; no phase module names another, and only `pipeline/` names more
  than one (`architecture.md` §1.1). `cargo make guard-module-edges` stays green.
- No recursion over the graph (`architecture.md` §3.2).
- A target is a `NodeId` inside the phases; strings appear only where something is reported, and no
  `NodeId` reaches a file.
- `Provenance` is threaded, never reconstructed: every error and failure that names a target takes its
  site from the graph.
- The error model stays one `#[non_exhaustive]` enum with structured fields; variants are added, none
  reshaped (`architecture.md` §4).
- `buildl-core`'s dependency set is unchanged.
- `Declaration`, `TargetGraph` and the `DeclarationSource` contract are not reshaped; `buildl-lua`,
  `buildl` and `buildl-cli` are not touched.
- Workspace policy and the Rust guidelines apply unchanged: export-only `lib.rs`, no internal planning
  references in rustdoc or crate READMEs.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The `Reporter` port, output blocks, the status line, `--stream`, and replaying a cached target's
  output — the adapters and CLI slices.
- Selecting the subgraph a requested target needs, and demand-driven loading (design §11.1).
- The `Approver` port, the interactive prompt and CI detection — milestone 4.
- Every real adapter: the cas on disk, `.buildl/cache.json`, `.buildl/log.jsonl`, the thread-pool
  dispatcher, the `Bare` and `InputSandbox` strategies, the system clock — the `buildl` adapters
  slice. Reading a cache row back (follow-up #17) and the `buildl-lua` half of follow-up #13 stay
  there too.
- Parsing `--keep-going` and `--set`, mapping an outcome class to an exit code, and rendering a
  summary for a terminal — the CLI slice. Follow-up #5 stays there.
- The isolation tiers above the port itself: the input sandbox (milestone 3), the OS sandbox and
  containers (milestone 8), and enforcing `network = true`.
- `$dep://` placeholders, hermetic toolchains and `b.fetch` — milestone 5.
- `buildl test`, `clean`, `gc`, `log`, `doctor`, and writing `graph.json` during a build.
- Watch mode's re-plan, `query` and `rdeps`, the remote cache and remote execution.
- Per-target grant narrowing (design §14).
- Discovery targets (design §10).
