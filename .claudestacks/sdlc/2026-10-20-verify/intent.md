---
status: done
created: 2026-10-20
milestone: buildl/03
---

# Intent: I17 buildl verify

## Problem

Nothing checks that an action produces the same bytes twice.

- **A nondeterministic tool poisons the cache silently.** A tool that stamps the time, a random
  seed or its working directory into its output gets cached under a key that promises
  reproducible bytes. Every later build serves that entry, and nothing tells the user. A shared
  remote cache (design §11.2, §13 items 6 and 8) would spread the defect to every machine.
- **Switching to the sandbox re-checks nothing.** `strategy` is not part of the action key, so a
  target cached under `bare` is a cache hit under `sandbox` (design §12.2). A hidden input in
  such a target only shows when the target next rebuilds. Design §12.2 names `buildl verify` as
  the way to check a cached target, and design §12.3 promises it: it "re-runs a sample of
  cache-hit actions and compares output digests". It does not exist.

Second and last slice of milestone 3 (design §13 item 3), after I16 input sandbox, which it needs.

Assumed, and overridable:

- **`verify` checks cache entries; it does not build.** A target that is not cached is skipped,
  and the report says so.
- **A mismatch is reported, not repaired.** The cache row stays as it is; deciding what to do
  about a nondeterministic tool is the user's call.

## Affected systems

- `buildl-core` — a verify pipeline beside build and test: plan, re-run the cached actions,
  compare what they produce with what the cache holds, and report; nothing recorded.
- `buildl` — `Workspace` gains `verify`; the sandbox runs the re-run.
- `buildl-cli` — a `verify` command, its report lines and exit codes.
- `docs/design.md` §4, §7, §8.7, §12.2, §12.3; `docs/architecture.md`;
  `docs/architecture-building-blocks.md`; `README.md`; `CLAUDE.md` Status; the crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|Command|`buildl verify` takes the same target arguments as `build`: the targets named, or every target when none is named, with what they need.|
|Which actions|Every cache hit in scope is re-run; `--sample N` re-runs N of them instead, chosen the same way on every run and every machine (no randomness), so two runs of one command check the same targets.|
|Re-run|Each chosen action runs again in the input sandbox, from the same inputs and dependency outputs its key was computed over, and the digests of what it produces are compared, output by output, with the ones its cache row records.|
|Nothing recorded|`verify` writes no cache row and no log event. A re-run's outputs never reach `out/`: before re-running, `verify` puts each cached target's outputs in place under `out/` as `build` does for a cache hit, so `out/` ends as `build` would leave it. What a re-run stores in the cas is named by no row or log event, so the next `gc` removes it. A target that is not cached is skipped and counted, not built.|
|Mismatch|The report names the target, the output and the first file whose bytes differ, and `verify` exits non-zero. The cache row is left as it is.|
|Strategy|`verify` refuses to run under `bare`, before anything runs, with an error naming `strategy = "sandbox"`: at tier 1 a re-run would write into `out/`. `os-sandbox` and `container` are refused as `build` refuses them.|
|Failure|A re-run that fails, times out or is killed is reported as such, distinct from a mismatch, and also exits non-zero.|
|Tests|Core tests against fake ports, end-to-end tests through `Workspace` and CLI tests of the binary cover each row above, including a deterministic target that verifies clean and a nondeterministic one that is named.|
|Documents|The design, both architecture documents, README and `CLAUDE.md` describe `verify` as built.|

## Constraints

- The action key, cache, cas and log formats do not change.
- The determinism rules of `architecture.md` §5 hold: the sample, the order of the report and
  every line of it are the same on every run; no `HashMap` iteration reaches output; the wall
  clock only through the `Clock` port.
- The settled decisions of architecture §6 stand: `strategy` is not a key component, plain
  threads, no process group, no signal handler. `unsafe_code` stays forbidden.
- The structural rules hold: the verify logic lives in `buildl-core` and is testable against fake
  ports; adapter modules of `buildl` do not import each other (`cargo make guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Repairing the cache on a mismatch: evicting the row, rebuilding, or marking the target.
- Verifying under `os-sandbox` or `container` (tiers 3 and 4), or remotely.
- Explaining why bytes differ beyond naming the first differing file (no byte-level diff).
- Re-running `test` targets' checks as tests: a test target is verified as an action like any
  other.
