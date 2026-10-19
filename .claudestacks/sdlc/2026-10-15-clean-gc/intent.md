---
status: approved
created: 2026-10-15
milestone: buildl/02
---

# Intent: I13 clean and gc — drop the outputs, prune the store

## Problem

A workspace's state only grows, and the one way to reset its outputs is to delete directories by
hand.

- **Nothing removes anything.** Every build adds blobs to the cas (`cas/local_cas.rs`), rows to
  the action cache and lines to `log.jsonl`; a row of a target since removed from the build files
  stays in `cache.json` for good, and the blobs it names with it. Architecture §3.5 says it
  plainly: "until then the store, the log and the rows of targets no longer declared only grow."
- **`clean` and `gc` are promised and absent.** Design §7 lists `buildl clean` / `gc` ("drop
  outputs; prune the cas"); README marks them "not yet"; `buildl-cli` has neither subcommand.
  Architecture §3.5 holds `gc`'s design — mark from the roots, follow each output to its manifest,
  sweep what is unmarked — and nothing implements it.
- **I12 set a condition.** `buildl log` reads the stdout and stderr blobs of a target's newest run
  with output, and reports a missing blob as an error. A `gc` that sweeps those blobs would turn a
  readable log into that error.

Fourth slice of milestone 2 (design §13 item 2): I10 query → I11 test → I12 log → **I13 clean +
gc** → I14 parallel load → I15 demand-driven load. It shares a branch and one pull request with
I14, executed before it.

Assumed, and overridable:

- **One chain.** `clean` and `gc` are small, touch the same state directory and the same CLI, and
  are listed together in design §7.
- **`clean` takes no target.** It drops the whole output directory.

## Affected systems

- `buildl-core` — which blobs and rows a `gc` keeps, decided against fake ports; the pipeline
  truncated for `gc`.
- `buildl` — listing and removing what the cas and the action cache hold; removing the output
  directory; the `Workspace` facade entries for `clean` and `gc`.
- `buildl-cli` — the `clean` and `gc` subcommands, their output and exit codes.
- `docs/design.md` §7 and the state sections that say the store only grows; `README.md`'s command
  table and status line; `CLAUDE.md`'s Status; `docs/architecture.md` §3.5 and
  `docs/architecture-building-blocks.md` where they describe the store, its ports and the
  commands.

## Desired outcome

|Topic|Outcome|
|---|---|
|`clean`|Removes the workspace's output directory and nothing else. Rows, cas and log stay, so the next build puts every output back from the cas and runs nothing that was clean before.|
|`gc` roots|`gc` loads and resolves the build files. The rows of declared targets are kept; the row of a target no longer declared is dropped.|
|`gc` marks|Kept: every blob a kept row names — each output's manifest and every file that manifest names, and the row's stdout and stderr — and, for every label in `log.jsonl`, the stdout and stderr of its newest run with output, so `buildl log` reads after `gc` what it read before.|
|`gc` sweep|Every unmarked blob is removed, with its executable twin; leftovers of interrupted writes in the store's staging directory are removed too.|
|Report|Each command prints one summary line of what it removed: `gc` the rows dropped, the blobs removed and the bytes freed.|
|Broken build file|A build file that fails to load stops `gc` before anything is removed, with the same error `check` gives. `clean` reads no build file.|
|Lock|Both take the workspace lock, as `build` does, and are refused while another process holds it.|
|Interrupted|A `gc` stopped at any point leaves no row naming a blob the store no longer holds: rows go before blobs. At worst, unreferenced blobs remain for the next `gc`.|
|`out/` survives `gc`|An output in `out/` is a hard link, so sweeping its blob never breaks a file already placed.|
|Tests|Core logic against fake ports, adapter tests against a real store and cache, end-to-end tests through `Workspace`, and CLI tests of the binary cover each row above.|
|Documents|Design §7, README, `CLAUDE.md` and both architecture documents describe `clean` and `gc` as built; README marks them available.|

## Constraints

- Crash safety (design §8.5) holds: a row never names content the store does not hold.
- The log, cache and cas formats do not change.
- The determinism rules of `architecture.md` §5 hold: no `HashMap` iteration reaches output, and
  JSON is read and written only through the one canonical serializer.
- The structural rules hold: `buildl-core` stays free of I/O, phase modules do not import each
  other, adapter modules of `buildl` do not import each other (`cargo make` guards).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Removing a single target's outputs, or a full reset of `.buildl/` (deleting the directory does
  that).
- Bounding or rotating `log.jsonl`; `gc` keeps the log as it is.
- Pruning `statcache.json`.
- Reference counting, or keeping blobs by age or size: mark-and-sweep only (architecture §3.5).
- A `gc` that keeps rows without loading the build files.
