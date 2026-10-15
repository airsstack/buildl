---
status: done
created: 2026-10-15
depends-on: [09]
built-on: fefed82cb705
---

# Documentation of `test` Implementation Plan

**Goal:** The design, architecture and summary documents describe `buildl test` as built.

**Architecture:** Prose and table edits only, in the documents spec §4 names, each at the line
that today says something this chain changed. Mermaid stays Mermaid, section numbers stay, and
the numbered cross-references (`design §8.5`, `§12.4`, …) are kept. `README.md` stays consistent
with the two specs (CLAUDE.md).

**Tech Stack:** Markdown. Definition of Done: `cargo make dod` (its doc and test steps read no
Markdown here, but the gate runs before every commit) and `cargo deny check`.

**Content authority:** spec §4, and the behaviour plans `01`–`09` built.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `repo`.

Each row below names the line as it reads at commit `1799d47`. The edit says what the text must
say afterwards; the wording is the implementer's, in the documents' voice. No roadmap file
changes (spec P17).

## Files that change

```
docs/design.md                         — [modify] §3, §5, §7, §8.4, §8.5, §8.7, §9.1
docs/architecture.md                   — [modify] status line, settling, record ordering, narrowing, the action cache, the cache decision row
docs/architecture-building-blocks.md   — [modify] the run values row, narrow and Pipeline, the ActionCache port, the test tables
README.md                              — [modify] the command table and the flags sentence
CLAUDE.md                              — [modify] Status
```

## Order of work

### Task 1 — `docs/design.md`

**Files:** `docs/design.md`.

**Change:**

| Line | Today | Afterwards |
|---|---|---|
| `:75` | `cache.journal      # rows recorded since cache.json was written; …` | the journal holds a row, or the forgetting of one, recorded since `cache.json` was written |
| `:170` | `b.target` bullet: `outputs` (default: the target name) | a sentence after the bullet, or in it: a `b.test` target takes the same table, and its `outputs` default to none — its record is its exit status and what it printed; naming `outputs` holds it to them |
| `:211` | `` `buildl test` / `clean` / `gc` `` on one row, not built | `buildl test [target]...` on a row of its own (Record; "run test targets and what they need"); `clean` / `gc` keep theirs |
| `:216` | "`check`, `graph`, `query`, `plan` and `build` are built … with none, every target; `plan` takes exactly what `build` takes …" | `test` is among the built commands; with no targets `plan` and `build` take every target but tests (and the tests a target depends on), and `test` every test; `test` takes `--set`, `--stop`, `--stream`, `--rerun`, `--verbose`; naming a target that is not a test is a usage error |
| `:282` | stopping is the default, `--keep-going` for CI | the same for `build`; `test` keeps going by default and takes `--stop` |
| `:302` | the invariant: a cache row is written only after its outputs exist in the cas | a clause: removing a row — a failure under the key the row was recorded under — is durable before the target's log line, as writing one is |
| `:328-368` (§8.7) | blocks for every action that printed or failed; summary line; "Failures are never cached" | what `test` prints (a failing or skipped test's block, every dependency's block as `build` prints it, a passing test only with `--verbose`, a live one under `--stream`), its summary line (`tests: 3 passed (2 cached), 1 failed, 1 skipped`, `no tests`, `tests (rerun): …`), and in "Failures are never cached": a failure under the recorded key forgets that row, so a failed rerun leaves no pass behind |
| `:385` (§9.1) | "tests are cached by the same key, so unchanged code skips its tests, with the log recording why" | the same, plus: `buildl test --rerun` runs them anyway, and a rerun that fails forgets the pass |

**Proof:** `grep -n 'buildl test' docs/design.md` shows the new row and the §7 paragraph;
`grep -n 'forget' docs/design.md` shows §3, §8.5, §8.7 and §9.1; `grep -c '```mermaid'
docs/design.md` is unchanged; every `§` reference in the edited lines names a section that
exists.

**Done when:** the Definition of Done is green. Commit `docs(repo): design describes buildl test`.

### Task 2 — The architecture documents, README and CLAUDE.md

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`, `README.md`,
`CLAUDE.md`.

**Change:**

| File:line | Today | Afterwards |
|---|---|---|
| `architecture.md:3` | status: the build-system core is implemented | `test` among what is built |
| `architecture.md:182` | a started target is settled: when not declared `always`, its key equals the recorded one … | the forcing rule: `always` first, then a rerun test, then the key comparison; `DirtyReason::Rerun` named beside `Always`; the summary example reads as today, and a sentence says a test run sums up as `tests: …` from `Summary.tests` |
| `architecture.md:231` | the record ordering: the calling thread "writes the cache row, appends the log event and releases dependents" | it writes the row — or, for a failure under the key the row was recorded under, forgets it — before it appends the log event |
| `architecture.md:373` | the decision row "A build of named targets — The graph is narrowed between Resolve and Plan" | narrowing also takes the run's `Purpose`: a whole workspace means every target but tests for a build and every test for a test run |
| `architecture.md:284` | the action cache is a `BTreeMap<Label, CacheRow …>` | `ActionCache` also forgets a row; when (a failure under the recorded key) |
| `architecture.md:376` | the cache decision row: a snapshot and a journal folded once per build | the journal's forget line `{"label":…,"row":null}`, replayed as a removal; `cache.json`'s format unchanged |
| `architecture-building-blocks.md:179` | the run values row: `Scope`, `LiveOutput`, … | `Purpose` and `Rerun` join it |
| `architecture-building-blocks.md:411` | `narrow`: `TargetGraph` and a `Scope` → … the whole graph for `Scope::Workspace` | `narrow` takes a `Purpose`: the whole workspace means every target but tests for a build and every test for a test run; `Error::NotATest` |
| `architecture-building-blocks.md:418` | "`plan` and `build` take a `Scope` and narrow …" | `test` too, with `Purpose::Test` and a `Rerun` |
| `architecture-building-blocks.md:413`, `:415` | plan and execute rows: inputs and outputs | each also takes the run's `Rerun` |
| `architecture-building-blocks.md:433` | the `ActionCache` port: "reading the last build's, and recording one durably for each target that was built" | and forgetting one durably when a failure contradicts it |
| `architecture-building-blocks.md:494` | the fake `ActionCache`: "rows held in memory, with labels it can be told to fail on" | it also keeps what it forgot, and can be told to fail a forget |
| `architecture-building-blocks.md:502-508` | core flow scenarios | rows for a test run: tests only and their dependencies; a rerun runs a cached test and not its dependency; a failed rerun forgets the row |
| `architecture-building-blocks.md:526-534` | end-to-end scenarios | rows: a test with no outputs passes and is cached; a build of the workspace runs no test |
| `README.md:124` | `` `buildl test` / `clean` / `gc` `` not yet | `buildl test [target]...` on its own row, available; `clean` / `gc` not yet |
| `README.md:128` | "`plan` and `build` take any number of targets and `--set …`; `build` adds `--keep-going` and `--stream`." | `test` takes targets and `--set`, and adds `--stop`, `--stream`, `--rerun`, `--verbose`; with no targets `plan`/`build` take every target but tests and `test` every test |
| `CLAUDE.md:7` | "Milestone 2 has begun: `query` answers …" | `query` and `test` are built (`test` runs the test targets, cached by key) |

**Proof:** `grep -n 'Purpose\|Rerun\|forget' docs/architecture.md` and the same grep of
`docs/architecture-building-blocks.md` each show their edited rows (`architecture.md` holding
`Purpose` at the narrowing row and `forget` at `:231`); `grep -n 'buildl test' README.md`
shows the row marked `yes`; `grep -c '```mermaid'` of both architecture documents is unchanged.

**Done when:** the Definition of Done is green. Commit
`docs(repo): the architecture documents and README describe buildl test`.

## Risks

- **I12 edits the same lines next** (`design.md:212`, `README.md:125`, `CLAUDE.md:7`): this plan
  leaves the `log` rows alone so the log chain's documentation plan meets them as they read now.
- Line numbers drift as rows are added; each task finds its lines by the "Today" text, not by
  number alone.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `git diff --stat` touches only the five files above.

## Review findings

- risk — README's status named `query` and `log` from the second roadmap item, not `test` — `README.md` — fixed
- risk — a building-blocks end-to-end row said the cached test prints `tests: 1 passed (1 cached)`, which no end-to-end test checks — `docs/architecture-building-blocks.md` — fixed: the clause is dropped
- risk — a building-blocks end-to-end row said `test` with no target runs every test, which only the CLI tests check — `docs/architecture-building-blocks.md` — fixed: the clause is dropped
- risk — design §8.7 said a skipped test gets a block; `block` writes nothing for a skipped target that printed nothing, so it gets none — `docs/design.md` — fixed, and the same wording in `buildl-cli/src/main.rs`'s command table (plan `09`) with it. Spec §2's "a failing or skipped test's block" is §3.7's rule that such a test is shown as `build` shows it, which for a skipped target is nothing; the behaviour follows §3.7
- amendment hygiene — edits outside this plan's rows (design `:3`, `:72`, `:360`; architecture `:388`; building-blocks `:601`; README `:7`) — kept, and listed under Deviations

## Probe results

- None: every statement was checked against the code by the review.

## Deviations

- The log slice's documentation landed first on this branch: its `log` rows were left as they read, `test` is ordered before `log`, and CLAUDE.md's Status names `query`, `test` and `log`.
- Lines outside the plan's rows, each saying something this chain changed: design `:3` (status), `:72` (the lock is taken by `test` too), `:360` (exit 2 for a target that is not a test); architecture `:388` and building-blocks `:601` (the lock is taken by `plan`, `build` or `test`); README `:7` (status).
- The forgetting of a row is described for any failure under the recorded key (`--rerun`, `always`, a missing output), as `execute/record.rs` does it, not for tests alone.
- Both tasks are one commit, `docs(repo): the documents describe buildl test`.
