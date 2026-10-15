---
status: approved
created: 2026-10-15
depends-on: [01]
built-on: 280eda6f4573
---

# `pipeline::log` Implementation Plan

**Goal:** `buildl-core` returns the stored output of a target's newest logged run that has one,
read through two ports.

**Architecture:** A new port, `RunHistory`, hands over a log's lines (`LogLine`, plan `01`). A
new logic module, `history`, holds the pure choice `last_run(lines, label) -> Result<LastRun>`.
It needs the nearest-label suggestion, which lives in `resolve/suggest.rs` today; since only
`pipeline` may reach `resolve` (`crates/expected-module-edges.txt`), `suggest.rs` moves to the
crate's top level, unchanged, and both `resolve` and `history` reach it there. The free function
`pipeline::log(history, cas, label) -> Result<Shown>` — like `check`, `graph` and `query`, it
needs only its own ports — chooses the run and reads both digests from the content store. Two
errors are new: `NoRun` (no run with output) and `MissingOutput` (a blob gone).

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `thiserror`. Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §2 (Run shown), §3.2 (the port), §3.3, §3.4, §5, §6 (rows for
`history/last_run.rs`, `suggest.rs`, `error.rs`, `tests/flows/log.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl-core`.

Facts the tasks rest on, each read at commit `0cca515`:

| Fact | Where |
|---|---|
| `resolve/suggest.rs` holds `pub(crate) fn nearest<'a, T, I>(wanted: &str, candidates: I) -> Option<&'a T>` and a private `distance`, and imports only `core::fmt`; its six tests: `distance_counts_single_byte_edits`, `the_nearest_candidate_within_the_limit_is_returned`, `a_candidate_at_the_limit_is_offered_and_one_past_it_is_not`, `a_short_name_still_allows_one_edit`, `equally_near_candidates_resolve_to_the_lowest_whatever_their_order`, `no_candidates_offer_nothing` | `crates/buildl-core/src/resolve/suggest.rs:11-107` |
| Its users: `resolve/reach.rs:16` and `resolve/references.rs:15` (`use crate::resolve::suggest::nearest;`); declared `mod suggest;` at `resolve/mod.rs:19` | `grep -rn 'suggest' crates/buildl-core/src` |
| `guard-module-edges` lists, per top-level entry of `src/` (a file or a directory), every `crate::<module>` it names on non-comment lines, sorted, against `crates/expected-module-edges.txt`; it refuses `crate::{…}`, `super::super`, `super::` in a `mod.rs`, and `crate::<Item>`; `lib.rs` is skipped | `Makefile.toml:151-207` |
| `expected-module-edges.txt` today: 24 sorted lines, `error -> types` … `types -> json`; `pipeline -> ports` and `pipeline -> types` exist, `pipeline -> resolve` is the only edge into `resolve` | `crates/expected-module-edges.txt` |
| `lib.rs` declares `pub mod error, execute, json, load, pipeline, plan, ports, resolve, types` (`:98-106`), its "Where things live" table lists each (`:20-33`), and re-exports the port traits and `pipeline::Pipeline` (`:111-120`) | `crates/buildl-core/src/lib.rs` |
| `Error::UnknownTarget`'s message `target {label} was asked for but is not declared{suggestion_suffix}`; `suggestion_suffix(Option<&T>)` gives ` (did you mean {nearest}?)` or nothing; tests `unknown_target_names_the_label_and_the_nearest_one`, `unknown_target_without_a_near_label_offers_nothing` | `crates/buildl-core/src/error.rs:285-295`, `:385-387`, `:1170`, `:1182` |
| After the test slice, `Error::NotATest` follows `UnknownTarget`, then `InvalidQuery` | `.claudestacks/sdlc/2026-10-15-test/plans/01-selection.md`, Task 1 |
| `PortName` (`#[non_exhaustive]`) lists eleven ports with a `Display` word each (`EventLog` → `event log`) | `error.rs:509-551` |
| `ports/event_log.rs` is the model of a one-method port file: module doc, "This file holds only a trait definition, so it carries no logic to unit-test.", `# Errors` naming its `PortName`; `ports/mod.rs` declares and re-exports each port | `crates/buildl-core/src/ports/event_log.rs:1-25`; `ports/mod.rs` |
| `ContentStore::get(&self, &Digest) -> Result<Option<Vec<u8>>>`: `None` for content not held | `crates/buildl-core/src/ports/content_store.rs:45` |
| `pipeline/mod.rs` re-exports `driver::{Pipeline, check, graph, query}`; its Responsibilities line names them | `crates/buildl-core/src/pipeline/mod.rs:6`, `:12-14` |
| Flows: `FakeCas` implements `ContentStore` in memory and has `blob(digest)` and `forget(digest)` (removes a blob) | `crates/buildl-core/tests/flows/common.rs:320-375`, `:331`, `:354` |
| `LogLine { Event(LoggedRun), Damaged { line: NonZeroUsize } }`; `LoggedRun`'s public `label`, `class`, `stdout`, `stderr` | `plans/01-logged-run.md`, Task 2 |
| A skipped event has `null` digests; a cached one carries the real run's | spec P3, P4; `outcome_event.rs:212-240` |
| `cargo test` takes one name filter | `.claudestacks/sdlc/2026-10-14-query/plans/01-core-query.md`, facts table |

## Files that change

```
crates/buildl-core/src/suggest.rs              — [create, moved] nearest and distance, their tests, from resolve/
crates/buildl-core/src/resolve/suggest.rs      — [delete, moved] to src/suggest.rs
crates/buildl-core/src/resolve/mod.rs          — [modify] no mod suggest
crates/buildl-core/src/resolve/reach.rs        — [modify] use crate::suggest::nearest
crates/buildl-core/src/resolve/references.rs   — [modify] use crate::suggest::nearest
crates/buildl-core/src/error.rs                — [modify] NoRun, MissingOutput, PortName::RunHistory; tests
crates/buildl-core/src/ports/run_history.rs    — [create] the RunHistory trait
crates/buildl-core/src/ports/mod.rs            — [modify] declare and re-export it
crates/buildl-core/src/history/mod.rs          — [create] the module's doc and exports
crates/buildl-core/src/history/last_run.rs     — [create] LastRun, last_run, unit tests
crates/buildl-core/src/pipeline/log.rs         — [create] Shown and log
crates/buildl-core/src/pipeline/mod.rs         — [modify] declare log; re-export log and Shown
crates/buildl-core/src/lib.rs                  — [modify] modules, re-exports, the module table
crates/expected-module-edges.txt               — [modify] the new edges
crates/buildl-core/tests/flows/common.rs       — [modify] FakeHistory
crates/buildl-core/tests/flows/log.rs          — [create] flows of pipeline::log
crates/buildl-core/tests/flows/main.rs         — [modify] declare mod log
```

## Order of work

### Task 1 — `suggest` moves out of `resolve`

**Files:** `suggest.rs`, `resolve/suggest.rs`, `resolve/mod.rs`, `resolve/reach.rs`,
`resolve/references.rs`, `lib.rs`, `crates/expected-module-edges.txt`.

**Change:** a move, with `git mv`, and no change in behaviour. `lib.rs` declares `mod suggest;`
(crate-private) and adds a "Where things live" row, written as plain code text, not an intra-doc link (the module
is private, and a private link fails `doc -D warnings`): "`suggest` — the nearest declared name to one
that names nothing, for any module that refuses a name". The module doc's "Its own file because"
sentence no longer says "of this phase". The two users import `crate::suggest::nearest`.
`expected-module-edges.txt` gains `resolve -> suggest` in its sorted place.

**Proof:** the six moved tests pass under their new path, and every `resolve::` test passes
unchanged — the refactor kept its behaviour.

**Done when:** `cargo test -p buildl-core --lib suggest::` and
`cargo test -p buildl-core --lib resolve::` pass and the Definition of Done is green,
`guard-module-edges` included. Commit
`refactor(buildl-core): the nearest-name suggestion leaves resolve`.

### Task 2 — The errors of reading a log, and the `RunHistory` port

**Files:** `error.rs`, `ports/run_history.rs`, `ports/mod.rs`, `lib.rs`.

**Change:**
- `Error::NoRun { label: Label, suggestion: Option<Label>, damaged: Vec<NonZeroUsize> }` after
  `InvalidQuery`, doc "A log was asked for a target with no run in it." Fields: "The label that
  was asked for"; "The nearest label the log has a run for, if one is near enough"; "Every damaged
  line of the log, by number, in order: one of them may have been this target's run". Message
  `target {label} has no run in the log{suggestion_suffix}{damaged_suffix}`, where a new private
  `damaged_suffix(&[NonZeroUsize])` gives nothing for no line,
  `; 1 damaged line was skipped (line 14)` for one, and
  `; 2 damaged lines were skipped (lines 14, 90)` for more (numbers joined by `, `).
- `Error::MissingOutput { label: Label }` after `NoRun`, doc "What a target printed in its last
  logged run is no longer in the content store." Message exactly
  `target {label}: the output of its last logged run is no longer in the store`. It names no
  time (spec §3.4).
- `PortName::RunHistory` after `EventLog`, doc "The build log, read back.", `Display`
  `run history`.
- `ports/run_history.rs`: `pub trait RunHistory { fn lines(&self) -> Result<Vec<LogLine>>; }`,
  documented "The lines of a build log, oldest first: each whole line an event or a damaged
  line." with `# Errors` naming `PortName::RunHistory`. Module doc after `event_log.rs`'s: why
  its own file (Execute appends through `EventLog`, a reader reads through this, and neither
  carries the other's method), and the trait-only line.
- `ports/mod.rs` and `lib.rs` re-export `RunHistory`.
- `buildl-cli`'s `Exit::of_error` keeps compiling: `Error` is `#[non_exhaustive]` with a last
  wildcard arm; plan `06` gives both errors their exit codes.

**Proof** (in `error.rs`'s tests):
- `no_run_names_the_target_and_a_suggestion` — `//:ap` with suggestion `//:app` and no damage →
  `target //:ap has no run in the log (did you mean //:app?)`; without the suggestion →
  `target //:ap has no run in the log`.
- `no_run_names_the_damaged_lines` — `//:app`, lines 14 and 90 →
  `target //:app has no run in the log; 2 damaged lines were skipped (lines 14, 90)`; line 14
  alone → `…; 1 damaged line was skipped (line 14)`.
- `every_port_name_renders_a_human_phrase` (`error.rs:1302`) gains `(PortName::RunHistory,
  "run history")`.
- `missing_output_names_the_target` — `//:app` →
  `target //:app: the output of its last logged run is no longer in the store`.

**Done when:** `cargo test -p buildl-core --lib error::tests` passes and the Definition of Done
is green. Commit `feat(buildl-core): a port that reads the log back, and its errors`.

### Task 3 — `history::last_run` and `pipeline::log`

`last_run` is crate-private, so it lands with its first caller: alone it would fail the gate as
dead code.

**Files:** `history/mod.rs`, `history/last_run.rs`, `pipeline/log.rs`, `pipeline/mod.rs`,
`lib.rs`, `crates/expected-module-edges.txt`, `tests/flows/common.rs`, `tests/flows/log.rs`,
`tests/flows/main.rs`.

**Change:**
- `history/mod.rs`: module doc (the read side of the build log: which of a target's logged runs
  a reader is shown; not reading the file, a port's, nor showing it, an adapter's),
  `mod last_run;`, `pub use last_run::LastRun;`, `pub(crate) use last_run::last_run;`, and the
  export-only line ("This file holds only module declarations and re-exports, so it carries no
  logic to unit-test.").
- `pub struct LastRun { pub run: LoggedRun, pub newer: Vec<OutcomeClass>, pub damaged:
  Vec<NonZeroUsize> }`, `Debug, Clone, PartialEq, Eq`, documented: the run shown; the class of
  each newer line for the label that had no stored output, newest first; every damaged line of
  the file, in line order.
- `pub(crate) fn last_run(lines: &[LogLine], label: &Label) -> Result<LastRun>`: `damaged` is
  every `Damaged` line of the whole slice, in order. Walking events newest first, one for another
  label is passed; one for `label` with either digest `None` has its class pushed to `newer`; the
  first one with both digests is the run. With none, `Error::NoRun { label, suggestion, damaged
  }`, `suggestion` being `nearest` of the label's text over the distinct labels that *have a run*
  (an event with both digests), collected in a `BTreeSet<Label>`. Spec §3.3 says "the distinct
  labels of the log's events"; that set can hold the asked label itself (all its lines skipped),
  and `nearest` keeps a distance of 0 (`suggest.rs:24-30`), so it would suggest itself. Labels with
  a run never include the asked one, and each is a label `log` can show — what the error's field
  doc says.
- `pipeline/log.rs`: `pub struct Shown { pub last: LastRun, pub stdout: Vec<u8>, pub stderr:
  Vec<u8> }` (`Debug, Clone, PartialEq, Eq`, "A target's last run with what it printed, read
  back") and `pub fn log<H: RunHistory, C: ContentStore>(history: &H, cas: &C, label: &Label) ->
  Result<Shown>`: `history.lines()?`, `last_run`, then `cas.get` of the run's stdout and stderr
  digests; a `None` from either is `Error::MissingOutput { label }`. Rustdoc `# Errors`: the
  port's failures, `NoRun`, `MissingOutput`. The module doc says it reads no build file and takes
  nothing but these two ports.
- `pipeline/mod.rs`: `pub mod log;` beside `pub mod driver;`, and `pub use log::{Shown, log};`;
  the Responsibilities line names them. `lib.rs` (spec §3.6): `pub mod history;`,
  `pub use history::LastRun;`, and `pub use pipeline::{Pipeline, Shown, log};`; a "Where things live" row "`history` — the read side
  of the build log: the newest run of a target that left output", and the `pipeline` row names
  `log` among the functions over ports alone. The Status paragraph (`lib.rs:84-92`, "[`pipeline::check`],
  [`pipeline::graph`] and [`pipeline::query`] run the three commands…") gains a sentence: a
  [`LoggedRun`] read back from the log through [`RunHistory`], and [`pipeline::log`] returning a
  target's last run with what it printed. Rule 1 still holds (`history` is not a phase).
- `expected-module-edges.txt` gains, sorted, `history -> error`, `history -> suggest`,
  `history -> types`, `pipeline -> history`.
- Flows: `FakeHistory { lines: Vec<LogLine>, broken: bool }` in `common.rs`, implementing
  `RunHistory` (an `Adapter` error naming `RunHistory` when `broken`).

**Proof** (in `last_run.rs`'s tests, over helpers that make a `LoggedRun` of a label and class,
with or without digests):
- `the_newest_run_with_output_is_chosen` — two `Built` events of `//:app` (older stdout `a`,
  newer `b`): the run's stdout is `b`; `newer` is empty.
- `a_newer_skipped_line_is_noted_not_chosen` — `Built`, then `Skipped { because: //lib:text }`
  with no digests: the run is the `Built` one; `newer` is `[Skipped { //lib:text }]`.
- `a_cached_line_is_a_run` — a newest `Cached` event with digests is the run.
- `a_failed_line_is_a_run` — a newest `ActionFailure { 1 }` with digests is the run.
- `another_labels_lines_are_passed` — `//:app` `Built`, then `//:lib` `Built`: `//:app`'s run is
  its own and `newer` is empty.
- `damaged_lines_of_the_whole_file_are_collected_in_order` — `Damaged 1`, `//:app` `Built`,
  `Damaged 3`: `damaged` is `[1, 3]`, one older and one newer than the run.
- `a_label_with_only_skipped_lines_has_no_run` — `//:app` only skipped, beside a `Built`
  `//:apq`: `NoRun` for `//:app` with suggestion `//:apq`; with `//:app` alone, suggestion `None`
  — never `//:app` itself.
- `a_label_never_logged_has_no_run_and_a_suggestion` — events of `//:app`; asked `//:ap`: `NoRun`
  with suggestion `//:app`.
- `no_run_carries_the_damaged_lines` — `Damaged 2` and no event of the label: `NoRun`'s `damaged`
  is `[2]`.

(in `tests/flows/log.rs`):
- `log_returns_both_streams_of_the_last_run` — a `FakeCas` holding `out\n` and `err\n`, a history
  of one `Built` event of `//:app` naming them: `Shown`'s `stdout` is `out\n`, `stderr` `err\n`,
  `last.run.label` `//:app`.
- `a_blob_gone_from_the_store_is_missing_output` — the same with the stderr blob forgotten:
  `MissingOutput { //:app }`.
- `an_empty_log_has_no_run` — no lines: `NoRun` for `//:app` with no suggestion and no damage.
- `a_history_that_cannot_be_read_is_its_ports_failure` — `broken`: `Adapter` naming
  `RunHistory`.

**Done when:** `cargo test -p buildl-core --lib history::` and
`cargo test -p buildl-core --test flows log` pass and the Definition of Done is green,
`guard-module-edges` included. Commit `feat(buildl-core): read a target's last run from the log`.

## Risks

- **`guard-module-edges` reads modules from the file tree**: a forgotten edge fails the gate with
  a diff, which is the check. Inside `history/`, `last_run.rs` names its own module as
  `crate::history::…`, never `super::super`.
- **Most risky step:** Task 3's rule that a line is a run only with *both* digests. A failure that
  stored its output has both (spec P3); one that ran nothing has neither (P4). The first two
  `last_run` tests pin the boundary.
- **An empty run.** A run whose digests name empty content (`e3b0…`) is shown as empty bytes;
  `FakeCas` and `LocalCas` hold the empty blob (spec P3), so it is never "missing".

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
