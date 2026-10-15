---
status: approved
created: 2026-10-15
milestone: buildl/02
---

# Intent: I12 log — show what a target printed when it last ran

## Problem

Every build already records what each action printed, and nothing reads it back.

- **The write side is done.** Each action's stdout and stderr go into the cas by digest
  (`execute/record.rs`); every `log.jsonl` event holds both digests beside its label, class,
  command, key and times (`types/outcome_event.rs`, twelve keys); a cache hit carries the digests
  of the run that really happened, and a failed run carries its own. A skipped target's digests
  are `null`.
- **The read side does not exist.** `OutcomeEvent` has no `Deserialize` and its module says
  nothing in `buildl-core` reads a log back; `LocalLog` only appends; design §7 lists
  `buildl log <target>` and design §8.7 promises it ("show the last real output"); README marks
  it "not yet"; `buildl-cli` has no `log` subcommand.
- **I11 makes it the everyday way to see test output.** `buildl test` prints a block only for a
  failing test; a passing test's output is read back with `buildl log`.

Third slice of milestone 2 (design §13 item 2): I10 query → I11 test → **I12 log** → I13 clean
+ gc → I14 parallel load → I15 demand-driven load. It shares a branch and one pull request with
I11, executed after it.

Assumed, and overridable:

- **One chain.** Reading the log, choosing the run and printing it cannot be shown to work apart.

## Affected systems

- `buildl-core` — reading a log event back; choosing a label's run from the events.
- `buildl` — reading `log.jsonl` and the cas blobs it names; the `Workspace` facade entry for
  `log`.
- `buildl-cli` — the `log` subcommand, its flags, its exit codes.
- `docs/design.md` §3, §7, §8.7; `README.md`'s command table; `CLAUDE.md`'s Status; `docs/architecture.md` and
  `docs/architecture-building-blocks.md` where they describe the log, its port and the commands.
  The milestone 2 ladder is written nowhere outside the chains, and the status board counts
  progress, so no roadmap file changes.

## Desired outcome

|Topic|Outcome|
|---|---|
|Which run|`buildl log <target>` shows the newest run of the target that has output: built, cached (the bytes of the run that really happened) or failed. A newer line with no output — a skipped target, or a failure before anything ran — does not hide it.|
|Header|One header line on stderr names the target, how that run ended and when; when newer lines had no output, it says so (`newer: skipped (//lib:text failed)`).|
|Streams|With no flag, stored stdout goes to stdout and stored stderr to stderr. `--stdout` prints only the stored stdout, `--stderr` only the stored stderr, either one on stdout, so a pipe reads only what was asked for. Both flags together are a usage error, exit 2. Bytes are printed as stored; buildl filters nothing.|
|No Load|`log` reads only the workspace root, `log.jsonl` and the cas. A broken build file does not stop it; a target since removed from the build files still shows its last run. `:name` and `name` are read against the current directory. An alias is not followed.|
|Unknown target|A label with no run in the log is exit 2, with the nearest label from the log's labels suggested.|
|No lock|`log` takes no lock and writes nothing, like `query`: it runs while a build is going and sees every whole line written so far.|
|Missing blob|A run whose stored output is no longer in the cas — pruned, deleted, a copied workspace — is a clear error naming the target and the run, exit 3; never wrong output, never a crash.|
|Damaged lines|A whole line that is not an event is skipped, and one warning on stderr names how many and which lines; the rest of the log stays readable. A line cut short at the end is ignored, as `LocalLog` already treats it.|
|Tests|Core logic against fake ports, adapter tests against a real log and cas, end-to-end tests through `Workspace`, and CLI tests of the binary cover each row above.|
|Documents|Design §7 and §8.7, README and both architecture documents describe `log` as built; README marks it available.|

## Constraints

- `log` reads state and writes none: no lock, no `.buildl/` created, no clock read. The one
  exception is the store's own rule: a blob whose bytes fail its digest is removed by the read
  that finds it, as in every build, and the run reads as missing output.
- The log format does not change: the twelve keys of an event are read as written.
- The determinism rules of `architecture.md` §5 hold, and JSON is read and written only through
  the one canonical serializer.
- The structural rules hold: `buildl-core` stays free of I/O, phase modules do not import each
  other, adapter modules of `buildl` do not import each other (`cargo make` guards).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Searching or filtering the output (`grep`, `less` and `tail` do that), or reading a runner's
  output for meaning.
- Following an alias, which would need `graph.json` read back.
- Showing a run older than the newest one with output, listing a target's history, or a
  `--json` form of an event.
- Bounding or rotating `log.jsonl`.
- Which blobs `gc` keeps (I13); `log` only reports a missing one.
