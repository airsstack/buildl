---
status: done
created: 2026-10-15
milestone: buildl/02
---

# Intent: I14 parallel load — evaluate build files across workers

## Problem

Load evaluates every build file one after another, on one thread.

- **Serial today.** `load::load` (`load/traversal.rs`) pops one directory from a queue, evaluates
  its build file, queues the directories it names with `subdir`, and only then takes the next.
  Each evaluation builds and drops its own airsl engine (~136 µs plus the file's own work, and
  `buildl.sources` walks the filesystem), so a workspace of many build files pays for each in
  turn.
- **Designed, not built.** Architecture §3.1 says so: "Load evaluates build files one at a time
  today … Parallel load: the directory queue is processed by a small pool; each worker owns its
  engines, staging buffers merge at the end." Design §11.1 counts on it ("Load parallelises
  across engines"), and design §13 item 2 puts it in this milestone.
- **The pieces allow it.** `LuaSource` is `Send + Sync` and keeps no engine between files, and
  Load already sorts its result by directory, name and the whole declaration, so the result does
  not depend on the order files were evaluated in.

Fifth slice of milestone 2 (design §13 item 2): I10 query → I11 test → I12 log → I13 clean + gc →
**I14 parallel load** → I15 demand-driven load. It shares a branch and one pull request with I13,
executed after it.

Assumed, and overridable:

- **One chain.** Running the queue on several workers and keeping the result identical cannot be
  shown to work apart.
- **The error a failing load reports is the one serial load reports**: the first failure in the
  order the serial queue visits directories, whatever the timing.

## Affected systems

- `buildl-core` — the Load traversal: the directory queue, run on several workers, and how their
  results and failures come back together.
- `buildl` — handing Load the worker count from `[execution] workers` for every command that
  loads.
- `buildl-lua` — `LuaSource` evaluated from several threads at once, if anything there must
  change.
- `docs/architecture.md` §3.1; `docs/design.md` §4 (`workers`) and §11.1; `README.md`;
  `CLAUDE.md`'s Status; `docs/architecture-building-blocks.md` where it describes Load and its
  port.

## Desired outcome

|Topic|Outcome|
|---|---|
|Parallel|Build files are evaluated on up to `[execution] workers` workers at once (`0` is one per core, as for actions). `workers = 1` evaluates one at a time.|
|Same result|For any worker count and any timing, Load returns the same declarations in the same order as serial load, so `check`, `graph.json`, `query`, `plan`, `build`, `test` and `gc` show no difference.|
|Same error|A load with one or more failing build files reports the same error whatever the worker count and timing: the one serial load reports.|
|Stop on failure|Once a failure is certain to be the one reported, no new build file starts evaluating; files already running finish and are discarded.|
|Every loading command|`check`, `graph`, `query`, `plan`, `build`, `test` and `gc` all load in parallel. `log` and `clean` still read no build file.|
|Limits unchanged|Each build file keeps its own engine and its own byte budget, as today.|
|Tests|Core tests against a fake source prove evaluations overlap and that the result and the error do not depend on completion order; adapter and end-to-end tests load a real multi-directory workspace at several worker counts and compare against `workers = 1`.|
|Documents|Architecture §3.1 and design §4 and §11.1 describe parallel load as built; README and `CLAUDE.md` say so.|

## Constraints

- Determinism (architecture §5): parallelism is never observable in any output, and no
  `HashMap` iteration reaches one.
- The settled decisions of architecture §6 hold: plain threads and std `mpsc`, no async runtime,
  rayon confined to the `Digester` adapter.
- The structural rules hold: `buildl-core` stays free of I/O and of airsl, phase modules do not
  import each other, adapter modules of `buildl` do not import each other (`cargo make` guards).
- The declaration format, the Lua API and every state file stay as they are.
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Demand-driven loading — loading only the directories named targets need (I15).
- Parallelism inside one build file, or one engine shared between files.
- Caching evaluation results between runs.
- A separate setting for the number of load workers.
