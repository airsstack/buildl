---
status: approved
created: 2026-10-20
milestone: buildl/03
---

# Intent: I16 input sandbox — run each action with its declared inputs only

## Problem

buildl runs every action at tier 1, `bare`, and tier 1 confines nothing (design §12.2).

- **A hidden input is silent.** An action runs in the workspace root and can read any file there,
  declared or not. A file it reads without declaring it is in no action key, so editing it
  rebuilds nothing and the cache serves a stale output — the defect the remote cache and remote
  execution (design §11.2, §13 items 6 and 8) would spread to every machine sharing it.
- **The default strategy is refused.** `strategy` defaults to `"sandbox"` (design §4), and
  `build` and `test` refuse every strategy but `bare` (`local_ports.rs`), so every workspace must
  write `strategy = "bare"` to build at all.
- **Time, locale and time zone are the machine's.** Design §12.3 and the §12.4 ledger promise
  that from tier 2 they are pinned (`TZ=UTC`, `LC_ALL=C`, `SOURCE_DATE_EPOCH`); nothing pins them.
- **Parallel actions share one tree.** At tier 1 they are kept apart only by each owning its own
  output paths, and an action that fails or is interrupted can leave a partial output under `out/`
  (design §8.5).

First slice of milestone 3 (design §13 item 3): **I16 input sandbox** → I17 `verify`, which re-runs
actions in this sandbox.

Assumed, and overridable:

- **No per-target strategy.** The strategy stays workspace-wide; `b.target` gains no field, and
  `strategy = "bare"` is the escape hatch for a workspace whose tools cannot run sandboxed.
- **Hidden inputs fail through the tool.** An undeclared file is absent, and the action fails with
  the tool's own error ("file not found"); buildl does not guess which file was missing.
- **Unix only**, as today: macOS and Linux.

## Affected systems

- `buildl` — a tier-2 execution strategy beside `Bare`; choosing between the two from the
  manifest at run time; removing an interrupted run's leftovers when the state lock is taken.
- `buildl-core` — only if the pinned variables need a check at Plan (a target declaring one of
  them); the `ExecStrategy` port and `ReadyAction` already carry what a sandbox needs.
- `buildl-cli` — tests and messages for the strategies it now runs and still refuses.
- `docs/design.md` §4, §8.5, §12.2, §12.3; `docs/architecture.md` §3.4, §6 and the §8 open
  question on how inputs are materialised; `docs/architecture-building-blocks.md` §5; `README.md`;
  `CLAUDE.md` Status; the `buildl` crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|Strategy|`build` and `test` run under `bare` and `sandbox`; `sandbox`, the default, needs no line in `buildl.toml`. `os-sandbox` and `container` are still refused before anything runs, as today.|
|Declared inputs only|Under `sandbox` an action runs in a directory of its own that holds, at their workspace-relative paths, exactly its declared inputs and its dependencies' outputs (each file of a directory output). Any other workspace file is absent there, so an action that reads one fails.|
|Commands unchanged|A command is the same text at every tier (design §8.5): the paths it names are true in the action's directory, and a program named through `$in` or `$deps` runs from there. Tools outside the workspace are found as at tier 1.|
|Pinned environment|Under `sandbox` an action's environment is its declared variables plus `TZ=UTC`, `LC_ALL=C` and `SOURCE_DATE_EPOCH` set to one fixed value, the same on every machine and run. A target that declares one of those three names is refused at Plan with an error naming it, never silently overridden.|
|Outputs|An action writes its outputs inside its own directory. Nothing reaches `out/` unless the action succeeded and its outputs were stored in the cas; a failed, timed-out or interrupted action leaves no partial output under `out/`.|
|Parallel actions|Actions running at once cannot see or write each other's files.|
|Clean up|An action's directory is removed once its outputs are stored, whatever the action's result. Directories an interrupted run left behind are removed when the next `plan`, `build`, `test`, `clean` or `gc` takes the state lock.|
|Cache across tiers|`strategy` stays outside the action key (architecture §6): a target cached under `bare` stays cached under `sandbox`. Design §12.2 says so, and that `verify` (I17) is how a cached target is checked.|
|Same results|A workspace that builds under `bare` with every input declared builds the same outputs, byte for byte, under `sandbox` — given tools that do not read the clock, locale or time zone, and do not record the directory they run in.|
|Tests|Adapter tests against a real file system, end-to-end tests through `Workspace`, and CLI tests of the binary cover each row above, including an undeclared input failing under `sandbox` and succeeding under `bare`.|
|Documents|The design, both architecture documents, README and `CLAUDE.md` describe tier 2 as built; the §8 open question on materialising inputs is settled and recorded in architecture §6.|

## Constraints

- Crash safety (design §8.5) holds: cas first, row after, log always; failures are never cached.
- The action key, cache, cas and log formats do not change.
- The determinism rules of `architecture.md` §5 hold; the sandbox's own location never reaches a
  key, a row or a log event.
- The settled decisions of architecture §6 stand: `strategy` is not a key component, plain
  threads, no process group, no signal handler. `unsafe_code` stays forbidden.
- The structural rules hold: `buildl-core` stays free of I/O, and adapter modules of `buildl` do
  not import each other (`cargo make guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- `os-sandbox` and `container` (tiers 3 and 4): no network seal, no hiding of system files outside
  the workspace, no protection against an action that reaches the real workspace by an absolute
  path or by climbing out of its directory.
- `buildl verify` — chain I17.
- A per-target strategy, or any change to the Lua declaration API.
- Stopping the processes an action starts (architecture §6, "Stopping an action").
- Naming the undeclared file an action tried to read.
