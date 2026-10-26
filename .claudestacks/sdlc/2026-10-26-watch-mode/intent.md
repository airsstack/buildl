---
status: done
created: 2026-10-26
milestone: buildl/07
---

# Intent: watch mode

## Problem

An edit is not followed by a build unless someone runs one.

- **The edit loop is manual.** A developer edits a source file or a build file, switches
  to a terminal, and runs `buildl build` again. Design §7 lists `buildl watch [target]` as
  a command, "persistent process, re-plans on file change", and design §8.6 draws its
  loop; the binary has no such command, and README lists it as "not yet".
- **Everything a watch needs exists except the watching.** A build that finds nothing
  dirty runs no action, a dependency rebuilt to the same bytes leaves its dependents clean
  (early cutoff), and the stat cache spares the hash of an unchanged file. What is missing
  is something that notices a change and starts that build.
- **A naive watcher would be heavy.** An editor's save is several file events, a build
  writes its own files under `out/` and `.buildl/`, and a checkout touches thousands of
  files. A watcher that builds on every event would build in a loop, or once per event.

Agreed with the user, and overridable:

- **What a cycle runs:** a build of the targets asked for, the whole workspace when none
  are named — what `buildl build` would do at that moment. Tests stay `buildl test`.
- **Full reload:** every cycle reads the build files and `buildl.toml` again, plans and
  builds, as `buildl build` does; no graph is kept from one cycle to the next (milestone
  07's goal, design §13 item 7).
- **Events, not polling:** changes are learned from the operating system's file events
  (FSEvents, inotify, ReadDirectoryChangesW), through the `notify` crate design §8.6
  names. Nothing re-scans the workspace on a timer.
- **What starts a cycle:** a change to a file the build reads — a declared input, a build
  file or `buildl.toml` — or a file created where a build file could pick it up. Changes
  under `out/`, `.buildl/` and `.git/`, editor swap files, and files no target reads start
  nothing.
- **Coalescing:** a burst of events within a 50 ms window is one cycle (design §8.6); one
  cycle runs at a time, and any number of changes during it lead to one more cycle after
  it, never one per change.
- **Between cycles the workspace is free:** another `buildl` command (`plan`, `log`) can
  run while watch waits.
- **Failures:** an action that fails, a build file that does not load, or a want the
  ceiling or the approver refuses is reported, and watching goes on; the next change
  starts a new cycle. Watch stops on Ctrl-C, on a usage error, or when the operating
  system's watcher fails.

## Affected systems

- `buildl-core` — the watch loop: what a change does, coalescing, one cycle at a time, and
  which failures end watching, testable against fake ports.
- `buildl` — the file-event adapter over `notify`, and running one cycle over the wiring
  `build` already uses.
- `buildl-cli` — the `watch` subcommand with `build`'s flags (`--set`, `--keep-going`,
  `--stream`, the remote switches), its report between cycles, and its exit codes.
- Workspace `Cargo.toml` and `deny.toml` — the `notify` dependency.
- `docs/design.md` §7, §8.6, §13 item 7; `docs/architecture.md` (status line, the watch
  loop's place in the pipeline, §6 decision records); `docs/architecture-building-blocks.md`
  (watch as an embedder of the composition root); `.claudestacks/sdlc/roadmaps/buildl/07-watch-mode.md`;
  `README.md`; `CLAUDE.md` Status; the crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|Start|`buildl watch [target]...` builds once, as `buildl build [target]...` would, then waits for changes.|
|Source change|saving a file a target reads starts one cycle; only the targets it makes dirty run.|
|Build-definition change|saving a build file or `buildl.toml` starts one cycle that reads the workspace again, so a new or changed target is built.|
|New file|a file created where a build file's glob can match it starts a cycle.|
|Ignored change|a change under `out/`, `.buildl/` or `.git/`, an editor swap file, or a file no target reads starts no cycle, so a build never triggers itself.|
|Bursts|an editor save, a `git checkout` or a formatter run over many files is one cycle, or one more after the running one; never one per event.|
|Unchanged bytes|a save that leaves a file's bytes as they were runs no action.|
|Failure|a failed action, a load error or a refusal is printed, and watch waits for the next change instead of exiting.|
|Workspace lock|`buildl plan` and `buildl log` work while watch waits between cycles.|
|Stop|Ctrl-C ends watch; a build it interrupts leaves the cache as an interrupted `buildl build` does (design §8.5).|
|Report|each cycle says what change started it and ends with the build's summary line; watch says when it is waiting.|
|Keys|watch changes no action key, cache row, log line or output: a cycle's results are those `buildl build` would give.|
|Tests|core tests of the loop against fake ports, adapter tests that write real files and see real events, and CLI tests of the binary cover each row above; no test waits on a fixed sleep to pass.|
|Documents|the design, both architecture documents, README and `CLAUDE.md` describe watch as built.|

## Constraints

- Full reload only: what a cycle does is what `buildl build` does; watch adds no second
  way to load, plan or execute.
- No persistent Lua engine and no `ext.on` dispatch (milestone 07 non-goal, design §8.6).
- The structural rules hold (building blocks §7): the loop's logic in `buildl-core`,
  testable against fake ports and free of any I/O API and of the wall clock (the
  `clippy.toml` ban); the file-event code only in `buildl`; adapter modules of `buildl` do
  not import each other (`cargo make guard-adapter-isolation`); the CLI stays a thin
  shell.
- Determinism (`architecture.md` §5): when and how often cycles run is never observable
  in what is built.
- The crash-safety invariant (cas first, cache row after, log always) holds for a cycle
  that is interrupted.
- The light-load rule of this repository: watch must not keep a CPU busy while nothing
  changes.
- `cargo make dod` and `cargo deny check` green before every commit; every new dependency
  carries its reason in the workspace `Cargo.toml`; work on this branch, never `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- A persistent engine dispatching through `ext.on` (milestone 07 non-goal).
- Keeping the graph between cycles and skipping Load when only a source file changed.
- A polling fallback for filesystems that send no events (network mounts, some container
  bind mounts).
- A watcher that keeps the stat cache warm between separate `buildl` runs (design §11.1).
- Watching tests: `buildl test` in a loop.
- Remote execution (milestone 08).

Candidate direction: a file-event port in `buildl-core` driving a loop that calls the
existing build once per cycle, with one `notify` adapter in `buildl`; design settles how
"a file the build reads" is known to the loop and when the workspace lock is taken.
