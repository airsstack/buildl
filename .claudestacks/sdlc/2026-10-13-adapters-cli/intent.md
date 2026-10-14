---
status: done
created: 2026-10-13
milestone: buildl/01
---

# Intent: adapters and CLI — from a pipeline that passes its tests to a `buildl` that builds

## Problem

`buildl-core` holds the whole pipeline — Load, Resolve, Plan, Execute, Record — and every flow of it
runs against fakes. `buildl-lua` evaluates a real build file. Nothing else is real, so nobody can
build anything with buildl.

- **The `buildl` crate is empty.** Its `lib.rs` is a doc comment: no module, no re-export. The port
  catalog (`architecture-building-blocks.md` §6) assigns it an adapter for every port except
  `DeclarationSource`, and none exists: nothing reads `buildl.toml`, hashes a file, finds a tool on
  `PATH`, stores a blob, writes a cache row or a log line, runs a process, or reads the clock.
- **Nothing is wired together.** `Pipeline<P: Ports>` has only ever been given `FakePorts`. The
  composition root the documents name — `LocalPorts`, the `Workspace` facade — does not exist, and
  `LuaSource` has never been bound to a pipeline beside real adapters.
- **The binary does nothing.** `buildl-cli`'s `main` is empty: no argument parser, no command, no
  exit code. The four commands of milestone 1 — `check`, `graph`, `plan`, `build` (design §7, §13) —
  cannot be run.
- **A build shows nothing.** `Pipeline::build` returns outcomes as values. Design §8.7 promises an
  output block per action, a status line on a terminal, plain blocks in CI, `--stream`, a summary
  such as `1 failed, 2 skipped, 2 built, 9 cached`, and a cache hit that shows again what the action
  printed when it really ran. The `Reporter` port that would carry this is in the catalog and is not
  a trait anywhere.
- **`plan` cannot explain itself to a person.** A `Plan` carries each verdict and its reason as
  values; nothing renders them.
- **A failure has no exit code.** Design §8.7 gives each failure class its own, so CI tells "fix your
  code" from "fix your builder". No mapping exists.
- **`build` cannot be asked for one target.** Design §7 has `buildl build [target]`. Every phase so
  far works on the whole graph; choosing what a named target needs was left out of I7 and I8.
- **Invocation values have no way in.** `--set name=value` and `--keep-going` are read by Plan and
  Execute as values; nothing parses them from a command line.
- **The determinism promise is untested on real I/O.** `architecture.md` §5 rule 5 has the test suite
  build a fixture workspace twice and compare `graph.json`, `cache.json` and the outputs byte for
  byte. That test needs real adapters and has never run. `graph.json` is in the workspace layout
  (design §3) and nothing writes it.
- **Some of design §7's commands belong to no milestone.** `test`, `clean`, `gc`, `log` and `doctor`
  are in the command table and in no row of design §13 or `docs/roadmap.md` §2. Work that has no
  owner is how this slice itself went unnoticed until now.

This is the last slice of milestone 1 (`docs/roadmap.md` §3, I9). Milestone 1 is done when it is.

Assumed, and overridable:

- **One chain.** `buildl-cli` is a shell over `buildl` and neither can be shown to work without the
  other; the slice's done condition spans both. Its size is handled by the plan fan-out.
- **Everything milestone 1 still owes is in this slice.** Nothing found here is handed on
  (`docs/roadmap.md` §6). That includes what I7 and I8 named as belonging to "the adapters and CLI":
  the `Reporter` port, output blocks, the status line, `--stream`, showing a cached target's output
  again, argument parsing, exit codes, and building one named target.
- **The `Approver` port is milestone 4 and tier 2 isolation (`InputSandbox`) is milestone 3**, as
  design §13 has them. Where a document lists either as part of this slice, the document is amended.
- **`test`, `clean`, `gc`, `log` and `doctor` are not milestone 1.** Design §13 names four commands
  for it. This slice does not build them; it gives each one a milestone in the roadmap so none is
  left without an owner.

Questions the documents leave open, which the design pass has to close:

- What the `Reporter` port is, where it is declared, and how output reaches it while a build runs,
  given that a build returns its outcomes only at the end.
- What `--stream` needs from a strategy that today hands back captured output once the action ends.
- How a named target narrows a build, in which phase, and what the other three commands do with a
  target argument.
- Which exit code each outcome class and each kind of error gets.
- What `check`, `graph` and `plan` print, in which formats (`--json`, `--dot`), and what is stable
  enough to be compared byte for byte.
- When `graph.json` is written (`architecture.md` §8's open question).
- How each state file is written so a crash leaves it whole, and what a corrupt one does to a run.
- What `buildl.toml` holds for this milestone, what an unknown key does, and what the defaults are
  when the file or a section is absent.
- What a tool's fingerprint is made of.
- How the worker count and the timeout reach the dispatcher and the strategy.
- What the tier 1 strategy guarantees and what it openly does not.
- Where the workspace root is found from the directory the command was run in.
- Which platforms the adapters support, and what happens to a hard link on a filesystem without them.
- What the `Workspace` facade offers an embedder, and what `buildl` re-exports.

## Affected systems

- `crates/buildl` — every adapter of the catalog except `DeclarationSource` and `Approver`,
  `LocalPorts`, the `Workspace` facade, the re-export of `buildl-core`, adapter tests, the end-to-end
  tests and their fixture workspaces, the crate rustdoc and `README.md`
- `crates/buildl-cli` — the argument surface, the four commands, exit codes, its tests, the crate
  rustdoc and `README.md`
- `crates/buildl-core` — the `Reporter` port and whatever a build needs to report while it runs;
  building what one named target needs; anything a real adapter shows a port's contract to be missing
- `crates/buildl-lua` — only where binding `LuaSource` beside real adapters shows a gap
- Workspace `Cargo.toml`, `deny.toml`, `crates/expected-*-edges.txt`, `Makefile.toml`, CI — new
  dependencies with their reasons, the guards' golden files, and whatever the end-to-end tests need
  to run in the gate
- `docs/roadmap.md` — the I9 row, §2 (milestone 1's status; an owner for every design §7 command),
  §4 on completion
- `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md`, `README.md`,
  `CLAUDE.md` — only where a decision closed here contradicts or settles text there

## Desired outcome

- The `buildl` binary runs `check`, `graph`, `plan` and `build` on a real workspace: a `buildl.toml`,
  Lua build files, real tools, real files.
- `buildl build` on a fixture workspace produces its outputs under `out/`. A second build of the
  unchanged workspace runs nothing and reports every target as cached. Built twice from clean, the
  outputs, `cache.json` and `graph.json` are byte-identical — the slice's done condition in
  `docs/roadmap.md` §3.
- Editing one input rebuilds what depends on it and nothing else; an edit that leaves an output's
  bytes unchanged leaves everything downstream cached.
- `buildl build <target>` builds that target and what it needs, and nothing else.
- `buildl plan` says, for every target, whether it would run and why, in words a person can act on.
- `buildl graph` emits the graph as `--json` and `--dot`; `buildl check` evaluates every build file
  and runs nothing.
- `--set name=value` reaches the plan and `--keep-going` reaches the build, each with the effect
  design §8.7 and §10.2 describe.
- A build prints one block per action that produced output, never interleaved; a status line on a
  terminal and plain blocks elsewhere; a closing summary; and, on a cache hit, what the action
  printed when it last ran. `--stream` shows one long action's output live.
- A failed build exits with a code that names its class; a successful one exits zero. An error says
  what was wanted, what was found and where it was declared.
- An interrupted build leaves no half-written file: every row in `cache.json` names outputs that are
  in the cas, and the next build finishes the rest.
- A state file that is not what buildl writes is refused as corrupt, not trusted.
- Every port in the catalog that milestone 1 uses has a real adapter, each tested on its own, and one
  place — `buildl` — names them together.
- An embedder gets a wired workspace from the `buildl` crate without copying anything from the CLI.
- Every command in design §7 has a milestone in the roadmap.
- The documents describe what the two crates now hold, `docs/roadmap.md` shows I9 done and
  milestone 1 done, and no follow-up is open.
- `cargo make dod` and `cargo deny check` pass, including every guard, on every platform of the CI
  matrix.

## Constraints

- **Nothing is carried forward** (`docs/roadmap.md` §6). Whatever this slice finds — in its own
  code, in `buildl-core`, in `buildl-lua`, in a document — is closed inside it before it is done.
- The dependency rules of `architecture-building-blocks.md` §7 hold: adapters depend on
  `buildl-core`; adapter modules inside `buildl` never import each other; `buildl` is the only place
  that names more than one concrete adapter; `buildl-cli` depends on `buildl` alone and reaches core
  types through its re-exports; no crate but `buildl-lua` knows Lua exists.
- `buildl-core` still names no I/O API and gains no production implementation of a port. Pipeline
  logic stays there: an adapter decides nothing the core can decide.
- The settled decisions of `architecture.md` §6 are not reopened: plain threads and std `mpsc`, no
  async runtime; rayon confined to the `Digester` adapter; SHA-256; one canonical serializer for
  every JSON file, and its reader for reading one back.
- **Cas first, cache row after, log always; failures are never cached** (design §8.5, §8.7) — now
  against a real disk, where a write can stop half-way.
- Determinism house rules (`architecture.md` §5): no `HashMap` iteration reaches any output;
  parallelism is never observable in an artifact; the wall clock is read only by the `Clock` adapter
  and reaches no cache key and no output.
- No shell, ever: a command is an argument vector, run as written.
- The ceiling in `buildl.toml` is never exceeded, and the manifest is inert data: reading it runs
  nothing.
- Printing is owned by one reporter; a worker never touches the terminal (`architecture.md` §3.4).
- Classification stays structural: an exit code comes from an outcome's class or an error's variant,
  never from message text.
- Every port contract is honoured as `buildl-core` states it. Where a real adapter cannot, the
  contract is changed in the core, with its fake and its tests, in this slice.
- Workspace policy and the Rust guidelines apply unchanged: `unsafe_code = "forbid"`, no `unwrap` or
  `panic`, export-only `lib.rs`, every new dependency commented with its reason and passing
  `cargo deny`, no internal planning references in rustdoc or crate READMEs.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.
- airsl stays on the `0.1` series.

## Non-goals

- `buildl query` and `rdeps` — milestone 2.
- The input sandbox, tier 2 (`InputSandbox`) — milestone 3. The OS sandbox and containers —
  milestone 8.
- The `Approver` port, the interactive prompt and CI detection — milestone 4.
- Plugins, `b.use()`, `b.fetch`, hermetic toolchains and `$dep://` — milestone 5.
- The remote cache — milestone 6. Watch mode — milestone 7. Remote execution — milestone 8.
- Building `buildl test`, `clean`, `gc`, `log` and `doctor`. This slice only gives each a milestone
  in the roadmap.
- Demand-driven loading of build files (design §11.1), per-target grant narrowing and discovery
  targets (design §14).
- Publishing to crates.io (`architecture-building-blocks.md` §9).
