---
status: approved
created: 2026-10-15
depends-on: [02]
built-on: 280eda6f4573
---

# The words of a last run Implementation Plan

**Goal:** `buildl`'s report renders a last run's header line and the damaged-lines warning.

**Architecture:** Three pure renderings in a new `report/log_text.rs`: `utc(Timestamp)` writes
`YYYY-MM-DD HH:MM:SS UTC` by the civil-from-days arithmetic (no time-zone data, no clock — the
core leaves formatting a time to an adapter, `timestamp.rs:9-11`); `header(&LastRun)` names the
target, how the run ended and when, and what newer lines had no output; `damage(&[NonZeroUsize])`
is the warning, when there is one. A failure is worded by `block.rs`'s `failure_text`, made
`pub(crate)`, so a header and a build's trailer say the same thing.

**Tech Stack:** Rust 2024 (`rust-version` 1.94). Definition of Done: `cargo make dod` and
`cargo deny check`.

**Content authority:** spec §1 P15, P19, §3.5 (the `report` bullets and the header examples), §5,
§6 (row for `report/log_text.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit per task, Conventional Commits,
scope `buildl`.

Facts the tasks rest on, each read at commit `0cca515` or computed:

| Fact | Where |
|---|---|
| `fn failure_text(class: &OutcomeClass) -> Option<String>` (private): `None` for `Cached`, `Built`, `Skipped`; `action failure (exit status {status})`, `action failure (ended by signal {signal})`, `action failure (declared output {name} was not produced)`, `action failure (output {name} cannot be kept: …)`, `infrastructure failure (…)`, and `a failure this buildl cannot name` for a class it does not know; `trailer` writes `FAILED {label}: {failure_text}` first | `crates/buildl/src/report/block.rs:98-120`, `:122-140` |
| `block.rs`'s module doc lists its Responsibilities; `report/mod.rs` declares each file privately and re-exports what callers use | `block.rs:1-16`; `crates/buildl/src/report/mod.rs` |
| `Timestamp::as_unix_nanos(self) -> u64` | `crates/buildl-core/src/types/timestamp.rs:41` |
| `LastRun { run: LoggedRun, newer: Vec<OutcomeClass>, damaged: Vec<NonZeroUsize> }`; `LoggedRun.at`, `.class`, `.label` | `plans/02-pipeline-log.md` Task 3; `plans/01-logged-run.md` Task 2 |
| Instants, by `date -u -r <seconds>` on this machine: `1709208000` → `2024-02-29 12:00:00`; `1704067199` → `2023-12-31 23:59:59`; `1792047792` → `2026-10-15 07:03:12` | run 2026-10-15 |
| `guard-adapter-isolation`: `report` may name only `state` and `search_path` of this crate, besides itself | `Makefile.toml`, `[tasks.guard-adapter-isolation]` |

## Files that change

```
crates/buildl/src/report/block.rs      — [modify] failure_text is pub(crate)
crates/buildl/src/report/log_text.rs   — [create] utc, header, damage, with unit tests
crates/buildl/src/report/mod.rs        — [modify] declare log_text; re-export its three
```

## Order of work

### Task 1 — `utc`

**Files:** `report/log_text.rs`, `report/mod.rs` (`crates/buildl/src`).

**Change:** `pub fn utc(at: Timestamp) -> String`, `#[must_use]`: the instant's whole seconds
(nanoseconds dropped, never rounded up) as `YYYY-MM-DD HH:MM:SS UTC`, computed from days since
the epoch by the civil-from-days algorithm in integer arithmetic. The module doc follows the
house shape: why its own file (what `buildl log` writes besides the stored bytes; pure
renderings), Responsibilities (`utc`, `header`, `damage`), Non-responsibilities (writing, and
choosing the run, which the core's `history` does). `report/mod.rs` gains `mod log_text;` and
`pub use log_text::utc;`.

**Proof** (in `log_text.rs`'s tests):
- `utc_of_known_instants` — `0` → `1970-01-01 00:00:00 UTC`; `1709208000` s →
  `2024-02-29 12:00:00 UTC` (a leap day); `1704067199` s → `2023-12-31 23:59:59 UTC` (the last
  second of a year); `1792047792` s plus `999_999_999` ns → `2026-10-15 07:03:12 UTC`.

**Done when:** `cargo test -p buildl --lib report::log_text` passes and the Definition of Done is
green. Commit `feat(buildl): an instant written in UTC`.

### Task 2 — `header` and `damage`

**Files:** `report/block.rs`, `report/log_text.rs`, `report/mod.rs`.

**Change:**
- `failure_text` becomes `pub(crate)`; its doc says a header words a failure with it too.
- A private `ended(class) -> String` in `log_text.rs`: `failure_text` for a failure; `built`,
  `cached`; `skipped ({because} failed)` for a skip.
- `pub fn header(last: &LastRun) -> String`: `{label}: {ended(class)} at {utc(at)}`, then, when
  `newer` is not empty, ` · newer: ` and either `ended` of its one class, or
  `{n} runs without output, newest {ended(newest)}` for `n` > 1; a newline ends it.
- `pub fn damage(lines: &[NonZeroUsize]) -> Option<String>`: `None` for none;
  `log.jsonl: 1 damaged line skipped (line 14)\n` for one;
  `log.jsonl: 2 damaged lines skipped (lines 14, 90)\n` for more.
- `report/mod.rs` re-exports `damage` and `header` from `log_text` (`block`'s own `header` is
  crate-private and not re-exported, so the names do not meet).

**Proof** (in `log_text.rs`'s tests, over a helper that makes a `LastRun` of `//:app` at
`1792047792` s):
- `a_header_names_the_class_and_the_utc_time` — `Built` →
  `//:app: built at 2026-10-15 07:03:12 UTC\n`; `Cached` → `//:app: cached at …\n`.
- `a_header_words_a_failure_as_a_trailer_does` — `ActionFailure { 1 }`: the header's text after
  `//:app: ` and before ` at` equals what `trailer` writes after `FAILED //:app: ` on its first
  line (`action failure (exit status 1)`).
- `a_header_notes_one_newer_line_without_output` — `ActionFailure { 1 }`, newer
  `[Skipped { //lib:text }]` →
  `//:app: action failure (exit status 1) at 2026-10-15 07:03:12 UTC · newer: skipped (//lib:text failed)\n`.
- `a_header_counts_several_newer_lines` — `Cached`, newer `[Skipped { //lib:text },
  InfraFailure { Run }, Skipped { //lib:text }]` →
  `//:app: cached at 2026-10-15 07:03:12 UTC · newer: 3 runs without output, newest skipped (//lib:text failed)\n`.
- `the_damage_warning_lists_the_lines` — `[14, 90]` →
  `Some("log.jsonl: 2 damaged lines skipped (lines 14, 90)\n")`; `[14]` →
  `Some("log.jsonl: 1 damaged line skipped (line 14)\n")`.
- `no_damage_no_warning` — `[]` → `None`.

**Done when:** `cargo test -p buildl --lib report::` passes and the Definition of Done is green,
`guard-adapter-isolation` included. Commit `feat(buildl): the header of a logged run`.

## Risks

- **Civil-from-days is easy to get wrong at year and century edges.** The leap day and the last
  second of a year are the cases the spec names; both are pinned from `date -u`.
- **`·` is not ASCII.** It is the spec's separator; a terminal without UTF-8 shows it as bytes.
  `log_text.rs`'s unit tests pin it; no CLI test has a newer line.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
