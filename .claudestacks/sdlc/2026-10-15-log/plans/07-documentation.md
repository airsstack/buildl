---
status: done
created: 2026-10-15
depends-on: [06]
built-on: 280eda6f4573
---

# Documentation of `log` Implementation Plan

**Goal:** The design, architecture and summary documents describe `buildl log` as built.

**Architecture:** Prose and table edits only, in the documents spec §4 names, each where the text
today says the log is never read back or `log` is not built. Mermaid stays Mermaid (a port added
to a Mermaid node's label is still Mermaid), section numbers stay, and numbered cross-references
are kept. `README.md` stays consistent with the two specs (CLAUDE.md).

**Tech Stack:** Markdown. Definition of Done: `cargo make dod` and `cargo deny check`, run before
each commit.

**Content authority:** spec §4, and the behaviour plans `01`–`06` built.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`, after the test slice's documentation plan
(`.claudestacks/sdlc/2026-10-15-test/plans/10-documentation.md`), which edits some of the same
paragraphs and leaves the `log` rows alone. One commit per task, Conventional Commits, scope
`repo`.

Each row names the line as it reads at commit `0cca515`; the test slice's documentation plan may
have moved it, so each edit is found by its "Today" text. The edit says what the text must say
afterwards; the wording is the implementer's, in the documents' voice. No roadmap file changes
(`2026-10-15-test/spec.md` §1, its P17).

## Files that change

```
docs/design.md                         — [modify] §3, §7, §8.7
docs/architecture.md                   — [modify] §3's table, §3.5, §4's error table, §5 rule on reading JSON
docs/architecture-building-blocks.md   — [modify] the ports node, the adapter and port rows, pure logic, the edge rule, the fake
README.md                              — [modify] the command table and the sentence after it
CLAUDE.md                              — [modify] Status
```

## Order of work

### Task 1 — `docs/design.md`

**Files:** `docs/design.md`.

**Change:**

| Line | Today | Afterwards |
|---|---|---|
| `:84` (§3) | "The cache is read back by the same rule it is written by: a file that is not byte for byte what buildl writes … is refused as corrupt" | a sentence after it: the log is read back leniently — a whole line that is not byte for byte an event is skipped, and `buildl log` warns which lines; the rest stays readable; a last line without a newline is ignored |
| `:212` (§7) | `` `buildl log <target>` `` · — · show what a target printed when it last ran | `buildl log <target> [--stdout \| --stderr]` as built, its phase column naming that it reads the log and the cas only |
| `:216` (§7) | the paragraph listing the built commands (as the test slice leaves it) | `log` among them; like `query` it takes no lock, loads no build file and writes nothing; it does not follow an alias; a target with no run is a usage error, a blob gone from the cas an infrastructure failure |
| `:328` (§8.7) | "That enables `buildl log <target>` (show the last real output), …" | `buildl log <target>` shows it: the newest run with stored output — built, cached (the real run's bytes) or failed — with one header line on stderr naming the target, how it ended and when, and newer lines without output; with no flag the stored stdout goes to stdout and the stored stderr to stderr, and `--stdout` or `--stderr` prints only that one, on stdout |

**Proof:** `grep -n 'buildl log' docs/design.md` shows §7 and §8.7 as built; `grep -c '```mermaid'
docs/design.md` is unchanged.

**Done when:** the Definition of Done is green. Commit `docs(repo): design describes buildl log`.

### Task 2 — The architecture documents, README and CLAUDE.md

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`, `README.md`,
`CLAUDE.md`.

**Change:**

| File:line | Today | Afterwards |
|---|---|---|
| `architecture.md` §3.5 (`:276-287`) | cas, action cache, log: the log appended to | the log read back: `LoggedRun`, the twin of the written event, read with the canonical reader; `RunHistory` hands lines to `history::last_run`, which `pipeline::log` runs before reading two blobs |
| `architecture.md` §4 (`:318`, the `UnknownTarget` row) | the error table | rows for `NoRun` (the label, the nearest label with a run, the damaged lines) and `MissingOutput` (the label) |
| `architecture.md` §5 (`:330`) | `log.jsonl` among what the canonical serializer writes | and what the canonical reader takes back, line by line |
| `architecture.md` §3 (`:188-193`, the table of logic, ports and adapters) | one row per phase | a row for reading the log: `history::last_run` and `pipeline::log` in `buildl-core`, ports `RunHistory` and `ContentStore`, adapter crate `buildl`; a sentence that `suggest` is a crate-level module `resolve` and `history` share, and `history` is not a phase |
| `architecture-building-blocks.md:110` (Mermaid `PORTS` node) | `ContentStore, ActionCache, EventLog` | `RunHistory` beside `EventLog` |
| `architecture-building-blocks.md:144` | `` |`log`|`log.jsonl`|`EventLog`| `` | serves `EventLog` and `RunHistory` |
| `architecture-building-blocks.md:434` | the `EventLog` port row | a row after it: `RunHistory` · the log's lines, each an event or a damaged line · `.buildl/log.jsonl`, read only · `buildl` · 2 (design §13 item 2; the "Since" note at `:423` says an item-2 row is built) |
| `architecture-building-blocks.md:405-418` (§5.4) | pure logic rows; "`Pipeline` strings these together …" | a `last_run` row (`[LogLine]` and a label → the newest run with output, or `NoRun`; no port); `log`, like `check`, `graph` and `query`, is a function over its own ports, taking no lock |
| `architecture-building-blocks.md:456` | the module-edge rule's row | the edge list now holds `history` and `suggest` |
| `architecture-building-blocks.md:495` | the fake `EventLog` | a fake `RunHistory` row: the lines handed over, and whether to fail |
| `README.md:125` | `` `buildl log <target>` `` not yet | available, with `--stdout` / `--stderr` |
| `README.md:128` | the flags sentence (as the test slice leaves it) | `log` takes one target and at most one of `--stdout`, `--stderr`; it takes no lock |
| `CLAUDE.md:7` | Milestone 2: `query` and `test` built (as the test slice leaves it) | `query`, `test` and `log` built |

**Proof:** `grep -n 'RunHistory' docs/architecture.md docs/architecture-building-blocks.md` shows
the edited rows in both; `grep -n 'buildl log' README.md` shows the row marked `yes`;
`grep -c '```mermaid'` of both architecture documents is unchanged.

**Done when:** the Definition of Done is green. Commit
`docs(repo): the architecture documents and README describe buildl log`.

## Risks

- **The test slice's documentation plan edits the same paragraphs first** (`design.md:216`,
  `README.md:128`, `CLAUDE.md:7`); this plan finds each by its text after that plan, so it adds
  `log` to what that plan wrote rather than rewriting it.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `git diff --stat` touches only the five files above.

## Review findings

- risk — README's and design's lists of commands that work under any strategy left out `log`, which never reaches the strategy (`Workspace::log` uses `Wiring::layout` only) — `README.md`, `docs/design.md` — fixed
- drift — README's roadmap item 2 had dropped `log` from the commands it brings, against design §13 — `README.md` — fixed: reverted
- risk — `RunHistory`'s port row said Since `1`, where `log` arrives with design §13 item 2 — `docs/architecture-building-blocks.md` — fixed: `2`, and the note defining "Since" says an item-2 row listed there is built; Task 2 amended
- missing — the §5.2 port listing had no `RunHistory`, and the §5.1 domain table no `LogLine` or `LoggedRun` — `docs/architecture-building-blocks.md` — fixed
- missing — design §8.7's exit table named neither a target with no run (2) nor an output gone from the store (3) — `docs/design.md` — fixed
- nit — `suggest` was described too narrowly; §5 rule 2's added sentence broke the wrap; "offers the nearest label" where it can offer one; the header said newer lines are named, where it counts them and names the newest — `docs/architecture.md`, `docs/design.md` — fixed
- nit — `log + cas` stands in a column headed "Stops after" — kept: the wording Task 1 and Task 2 ask for, naming what the command reads
- no finding — README's Status paragraph and the usage example now name `log`: consistency edits the README rule requires

## Probe results

- No throwaway probe. Every statement was checked against the code as built; `log` needing no `strategy` line was read from `Workspace::log` and `Wiring::runs`.

## Deviations

- The test slice's documentation plan has not run on this branch. Each row marked "as the test slice leaves it" was applied to the text as it reads now, adding `log` only; `buildl test` is described as built nowhere, and CLAUDE.md's Status names `query` and `log`.
