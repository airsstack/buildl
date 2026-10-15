---
status: approved
created: 2026-10-15
built-on: cbc3939d9dab
---

# Spec: log — show what a target printed when it last ran

Every build already stores what each action printed and writes both digests into `log.jsonl`;
nothing reads them back. This spec adds the read side: a port that reads the log's lines, a pure
choice of a target's newest run with output, and `buildl log <target> [--stdout|--stderr]`,
which prints that run's stored bytes, with a header on stderr. It loads no build file, takes no
lock and writes no state.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-15 at commit `7a16e8e`.

|#|Premise|Evidence|
|---|---|---|
|P1|A log line is one canonical object of twelve keys, written from `OutcomeEvent` through `EventView`.|`crates/buildl-core/src/types/outcome_event.rs:46-59` (`at, class, command, diagnostic, file, finished, key, label, outputs, started, stderr, stdout`); test `an_event_holds_exactly_the_twelve_keys_of_the_log_format` (`:243`); `crates/buildl/src/log/local_log.rs` writes `canonical` JSON and a newline per event.|
|P2|Nothing reads a log back.|`outcome_event.rs:10-11` ("reading a log back, which nothing in this crate does"), `:63-64` ("There is no `Deserialize`"); `local_log.rs:10-11` ("reading the log back … which nothing here does").|
|P3|A line's digests name raw blobs in the cas; empty output is stored too; a cached line carries the digests of the real run.|Probe, scratch workspace after the builds of the test slice's spec probes: `log.jsonl` 11 lines; last line `"stdout":"3e2bc902…dfc8d"`; `cat .buildl/cas/3e/2bc90282e2f9dfdecd656988c04e988fdf5fb9dd584ce322fe48b2a31dfc8d` → `out/copy expected.txt differ: char 1, line 1`; `cas/e3/b0c44298…b855` exists (SHA-256 of nothing); 5 lines of class `Cached`, the first for `//:copy` with `"stdout":"e3b0c442…"`. `execute/record.rs:61` `cached(label, row)` builds the outcome from the row.|
|P4|A skipped line has `null` digests.|`outcome_event.rs:212-240` (test `a_skipped_target_is_logged_without_a_key_an_instant_or_a_digest`: `"stderr":null,"stdout":null`).|
|P5|Every event field type reads back from JSON except the outcome class.|`Deserialize` derived on `Label` (`types/label.rs:26`), `Digest` (`types/digest.rs:27`), `Timestamp` (`types/timestamp.rs:25`), `Command` (`types/argument.rs:153`), `Produced` (`types/action_key.rs:39`), `OutputRefusal` (`types/outcome.rs:40`); not on `OutcomeClass` (`outcome.rs:58`, `Serialize` only) nor `InfraKind` (`:20`).|
|P6|The canonical reader accepts only bytes it would write.|`crates/buildl-core/src/json/canonical.rs:129-140` `from_slice<T: DeserializeOwned + Serialize>`: deserialize, re-serialize, compare; a difference is `Error::NonCanonicalJson`, a parse failure `Error::UnreadableJson`.|
|P7|`LocalLog` and `LocalCas` touch nothing on disk when made.|`local_log.rs:81-83` ("Touches nothing on disk until the first event"); `crates/buildl/src/cas/local_cas.rs:72-76` ("Touches nothing on disk: the store's directories are made by the first put").|
|P8|`LocalLog` opens for append, creating the file and cutting a last line without a newline.|`local_log.rs:64-79` (`open`: `.append(true).create(true)`, `set_len(whole)`).|
|P9|`ContentStore::get` returns `None` for a missing blob, and removes a blob whose bytes fail its digest.|`ports/content_store.rs:32`; `local_cas.rs:242-257` (`NotFound` → `Ok(None)`; digest mismatch → `clear_path` on both modes, `Ok(None)`).|
|P10|The nearest-label suggestion lives in `resolve`, and only `pipeline` may reach `resolve`.|`crates/buildl-core/src/resolve/suggest.rs:19` (`pub(crate) fn nearest`), `:36` (`distance`); `crates/expected-module-edges.txt` (full file: `pipeline -> resolve` is the only edge into `resolve`; `resolve -> error, types`), enforced by `guard-module-edges`.|
|P11|A relative target is read against the directory the workspace was opened from.|`crates/buildl/src/workspace.rs:62-70` (`Workspace::label`).|
|P12|`query` is the model for a lock-free, state-free command.|`workspace.rs:114-116` (`Workspace::query` calls `pipeline::query` with the source alone).|
|P13|Error texts for a target asked for on the command line, and the suggestion suffix.|`crates/buildl-core/src/error.rs:286-289` (`target {label} was asked for but is not declared{}`), `:385` `suggestion_suffix`.|
|P14|Exit codes: usage and infrastructure.|`crates/buildl-cli/src/exit.rs:79-80` (`UnknownTarget`, `InvalidQuery` → `Usage`, 2); `:56`, `:81`, `:86` (→ `Infrastructure`, 3).|
|P15|An outcome's words, as a build's trailer prints them, come from one private function.|`crates/buildl/src/report/block.rs:100-120` `fn failure_text(class: &OutcomeClass) -> Option<String>`: `action failure (exit status {status})`, `action failure (ended by signal {signal})`, `action failure (declared output {name} was not produced)`, `action failure (output {name} cannot be kept: …)`, `infrastructure failure (…)`; `None` for `Cached`, `Built`, `Skipped`. Used by `trailer` (`:130`), which takes an `OutcomeEvent`.|
|P17|A bad label from the command line is made a usage error by the CLI, not by its error's exit mapping.|`crates/buildl-cli/src/run.rs:155-160` (`scope`: `workspace.label(raw).map_err(… Cli::command().error(ErrorKind::ValueValidation, …))`), the same at `:194-199` (`query`); `Error::InvalidName` → `Internal` (`exit.rs:108-114`).|
|P18|The state directory's layout is made only under the lock today.|`crates/buildl/src/local_ports.rs:185-200` (`Wiring::lock` builds `StateLayout::new(self.root.as_path(), state)` and acquires the lock), `:214-230` (`ports` takes `lock.layout()`); `crates/buildl/src/state/layout.rs:39` `StateLayout::new(root, state)`.|
|P19|Formatting a time is an adapter's job.|`crates/buildl-core/src/types/timestamp.rs:9-11` ("Non-responsibilities: reading the clock, and formatting … a human-readable rendering belongs to whichever adapter shows it").|
|P16|A clap group of flags that exclude each other.|`crates/buildl-cli/src/cli.rs:74-83` (`GraphFormat`, `#[group(multiple = false)]`).|

## 2. The command

```text
buildl log <TARGET> [--stdout | --stderr]
```

|Aspect|Behaviour|
|---|---|
|Target|`//dir:name`, or `:name` / `name` read against the current directory (P11). An alias is not followed.|
|Reads|The workspace root (`buildl.toml`), `.buildl/log.jsonl`, the cas. No build file is loaded; no lock is taken; nothing is created or truncated. The one write is the store's own rule (P9): a blob that fails its digest is removed by the read that finds it.|
|Run shown|The newest line for the label whose `stdout` and `stderr` digests are both present: built, cached (the real run's bytes, P3) or failed. Newer lines without them (skipped, P4; a failure before anything ran) are noted, not shown.|
|stderr|First, one header line, then the damage warning if any (§3.4). Then, with no flag, the stored stderr.|
|stdout|With no flag, the stored stdout. `--stdout`: only the stored stdout. `--stderr`: only the stored stderr, on stdout.|
|Exit|0 when a run is shown (an empty run included); 2 for a label with no run, both flags, a bad label (wrapped as a clap usage error, as `scope` does, P17); 3 for a blob no longer in the store, or a log that cannot be read (`Error::Adapter` naming `RunHistory` falls to `Infrastructure`, `exit.rs:86`).|

## 3. Components

### 3.1 `types::LoggedRun` (new file `types/logged_run.rs`)

The read-side twin of `EventView` (P1): the same twelve keys with their own types (P5) — `at:
Timestamp`, `class: OutcomeClass`, `command: Command`, `diagnostic: Option<String>`, `file:
String`, `finished: Option<Timestamp>`, `key: Option<Digest>`, `label: Label`, `outputs:
Vec<Produced>`, `started: Option<Timestamp>`, `stderr: Option<Digest>`, `stdout: Option<Digest>`
— deriving `Serialize` and `Deserialize`, `deny_unknown_fields`. Read through
`canonical::from_slice` (P6), a line is an event exactly when it is the bytes buildl writes for
one. `OutcomeClass` and `InfraKind` gain `Deserialize` (P5).

A test pins the twin to the original: an `OutcomeEvent` written by `canonical` reads back as a
`LoggedRun` that writes the same bytes.

### 3.2 `types::LogLine` (new file `types/log_line.rs`) and the `RunHistory` port

```rust
pub enum LogLine { Event(LoggedRun), Damaged { line: NonZeroUsize } }

/// The lines of a build log, oldest first.
pub trait RunHistory {
    fn lines(&self) -> Result<Vec<LogLine>>;
}
```

New file `ports/run_history.rs`, beside `EventLog` (`ports/event_log.rs`): Execute appends, `log`
reads, and neither port carries the other's method. Its error is `Error::Adapter` naming a new
`PortName::RunHistory`.

### 3.3 `suggest` (moved) and `history::last_run` (new module)

`resolve/suggest.rs` moves to a top-level `suggest.rs`, its tests with it, because P10 forbids a
second module from reaching `resolve`. `expected-module-edges.txt` gains `resolve -> suggest`,
`history -> error`, `history -> suggest`, `history -> types`, `pipeline -> history`; no logic
module reaches another phase.

```rust
pub struct LastRun { pub run: LoggedRun, pub newer: Vec<OutcomeClass>, pub damaged: Vec<NonZeroUsize> }

pub(crate) fn last_run(lines: &[LogLine], label: &Label) -> Result<LastRun>;
```

Reads every line. `damaged` holds every `Damaged` line of the whole file, in line order — older
and newer than the run alike, since a damaged line may be any target's. Walking the events newest
first: an event for another label is passed; an event for `label` without both digests is pushed
to `newer` (newest first) until the run is found; the first with both is the run. With no such
event, `Error::NoRun { label, suggestion, damaged }`, the suggestion being `nearest` over the
distinct labels of the log's events, and `damaged` the same file-wide list, so the warning is not
lost in exactly the case where a damaged line may have been this target's run:

```text
target <label> has no run in the log<suggestion_suffix>
target <label> has no run in the log; 2 damaged lines were skipped (lines 14, 90)
```

worded after `UnknownTarget` (P13); the second form when `damaged` is not empty. A label whose
every line is skipped has no run.

### 3.4 `pipeline::log`

```rust
pub fn log<H: RunHistory, C: ContentStore>(history: &H, cas: &C, label: &Label) -> Result<Shown>;
pub struct Shown { pub last: LastRun, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
```

`last_run`, then `cas.get` on each digest (P9). A `None` from either is
`Error::MissingOutput { label }`, in words:

```text
target <label>: the output of its last logged run is no longer in the store
```

The text names no time: a time is formatted by an adapter (P19), and the run's `at` is, for a
cached line, the build that hit the cache rather than the run that printed (`outcome_event.rs:25`).

`pipeline::log` is a free function, like `check`, `graph` and `query`, since it needs two ports
only.

### 3.5 Adapters

- `LocalLog: RunHistory` (`crates/buildl/src/log/local_log.rs`). Opens read-only, never through
  `open` (P8): a missing `log.jsonl` gives no lines and is not created. Reads whole lines only — a
  last line without a newline is ignored and left as it is. Each line goes through
  `canonical::from_slice::<LoggedRun>`: `Ok` → `Event`, `Err` → `Damaged { line }` (1-based).
- `Wiring::layout(&self) -> StateLayout` (new, `local_ports.rs`): `StateLayout::new(root,
  &workspace.state)` without the lock (P18); `Wiring::lock` uses it too.
- `Workspace::log(&self, label: &Label) -> Result<Shown>` (`workspace.rs`): no lock, no
  `wiring.ports()`; `pipeline::log` over `LocalLog::new(&layout)` and `LocalCas::new(root,
  &layout)`, which touch nothing on disk (P7). Beside `Workspace::query` (P12).
- `report::block`: `failure_text` (P15) becomes `pub(crate)`, so a header and a trailer word a
  failure the same way.
- `report::log_text` (new `crates/buildl/src/report/log_text.rs`): `utc(Timestamp) -> String`,
  `header(&LastRun) -> String`, `damage(&[NonZeroUsize]) -> Option<String>`. `utc` writes
  `YYYY-MM-DD HH:MM:SS UTC` from the Unix nanoseconds by the civil-from-days algorithm — pure
  arithmetic, no time-zone data, no clock (P19).

```text
//:app: action failure (exit status 1) at 2026-10-15 07:03:12 UTC · newer: skipped (//lib:text failed)
//:app: built at 2026-10-15 07:01:40 UTC
//:app: cached at 2026-10-15 07:05:00 UTC · newer: 3 runs without output, newest skipped (//lib:text failed)
log.jsonl: 2 damaged lines skipped (lines 14, 90)
```

How a run ended: `failure_text` for a failure; `built` and `cached` for the two successes. A
`newer` entry: `skipped (<because> failed)` for a skip, `failure_text` otherwise (a failure
before anything ran, such as `infrastructure failure (the command could not be started)`).
`report` names only `state` and `search_path` (`guard-adapter-isolation`).

### 3.6 CLI

- `Command::Log(LogArgs)` after `Test`; `LogArgs { target: String, streams: Streams }`, with
  `--stdout` / `--stderr` as a clap group of at most one (P16) mapped to
  `Streams { Both, Stdout, Stderr }` — an enum, not two bools (rust guideline, strong types).
- Exports: `LoggedRun`, `LogLine`, `RunHistory`, `LastRun`, `Shown` and `pipeline::log` are
  re-exported from `buildl-core`'s `lib.rs`, as the existing types and ports are.
- `run.rs` `log`: `Workspace::label(target)`, its error wrapped as a clap `ValueValidation` usage
  error as `scope` does (P17), `Workspace::log`, then stderr ← header, damage
  warning; `Both`: stdout ← stored stdout, stderr ← stored stderr; `Stdout`: stdout ← stored
  stdout; `Stderr`: stdout ← stored stderr. Bytes are written as stored.
- `exit.rs`: `Error::NoRun` → `Usage` (2) beside `UnknownTarget`; `Error::MissingOutput` →
  `Infrastructure` (3) (P14).
- `main.rs`: the command-table row `buildl log <TARGET> [--stdout | --stderr]` · stdout: the
  stored stdout, or the chosen stream · stderr: the header, and the stored stderr with no flag.

## 4. Documentation

|File|Change|
|---|---|
|`docs/design.md` §3 (`:84`)|The log is read back leniently: a line that is not an event is skipped with a warning, unlike `cache.json`.|
|`docs/design.md` §7 (`:212`)|The `log` row as built, its flags; like `query`, it takes no lock; `log` among the built commands.|
|`docs/design.md` §8.7 (`:328`)|The sentence promising `buildl log` describes it: the newest run with output, the header on stderr, `--stdout`/`--stderr`.|
|`docs/architecture.md`|`RunHistory`, `history::last_run`, `pipeline::log`, `LoggedRun`; `suggest` moved out of `resolve`.|
|`docs/architecture-building-blocks.md` (`:144`, `:434`, the module-edge list)|Adapter row `log` serves `EventLog` and `RunHistory`; port row `RunHistory` · `.buildl/log.jsonl` · `buildl`; `log` beside `query` as lock-free; the new edges.|
|`README.md` (`:125`)|`buildl log` available.|
|`CLAUDE.md` Status|Milestone 2: `query`, `test`, `log` built.|

Mermaid in `docs/` stays Mermaid; section numbers are not changed; line numbers are re-checked
when the plan is written. No roadmap file changes (`2026-10-15-test/spec.md` §1, its P17).

## 5. Determinism and rules

- JSON is read only through `canonical::from_slice`; nothing is written.
- No `HashMap` reaches output: `damaged` is in line order, `newer` newest first, the suggestion's
  candidates are a `BTreeSet<Label>`.
- `buildl-core` gains no I/O (`guard-core-purity`) and no dependency (`guard-crate-edges`); module
  edges as §3.3 (`guard-module-edges`); `log` and `report` in `buildl` name only `state` and
  `search_path` (`guard-adapter-isolation`).
- No clock is read: the time shown is the run's own `at`.

## 6. Testing

|Layer|File|Tests|
|---|---|---|
|unit|`types/logged_run.rs`|`an_event_line_reads_back_as_a_logged_run_with_the_same_bytes` · `a_line_with_a_key_missing_is_not_an_event` · `a_line_with_an_unknown_key_is_not_an_event` · `a_line_out_of_canonical_form_is_not_an_event`|
|unit|`types/outcome.rs`|`every_class_reads_back_as_written`|
|unit|`history/last_run.rs`|`the_newest_run_with_output_is_chosen` · `a_newer_skipped_line_is_noted_not_chosen` · `a_cached_line_is_a_run` · `a_failed_line_is_a_run` · `another_labels_lines_are_passed` · `damaged_lines_of_the_whole_file_are_collected_in_order` (one older, one newer than the run) · `a_label_with_only_skipped_lines_has_no_run` · `a_label_never_logged_has_no_run_and_a_suggestion` · `no_run_carries_the_damaged_lines`|
|unit|`suggest.rs`|its existing tests, moved|
|unit|`error.rs`|`no_run_names_the_target_and_a_suggestion` · `no_run_names_the_damaged_lines` · `missing_output_names_the_target`|
|core flow|`tests/flows/log.rs` (new)|`log_returns_both_streams_of_the_last_run` · `a_blob_gone_from_the_store_is_missing_output` · `an_empty_log_has_no_run`|
|adapter|`log/local_log.rs`|`a_log_written_by_append_reads_back_line_for_line` · `a_missing_log_has_no_lines_and_is_not_created` · `a_last_line_without_a_newline_is_ignored_and_kept` · `a_hand_edited_line_is_damaged_with_its_number` · `reading_writes_nothing` (length and modification time unchanged)|
|adapter|`report/log_text.rs`|`utc_of_known_instants` (the epoch, a leap day, the last second of a year) · `a_header_words_a_failure_as_a_trailer_does` · `a_header_names_the_class_and_the_utc_time` · `a_header_notes_one_newer_line_without_output` · `a_header_counts_several_newer_lines` · `the_damage_warning_lists_the_lines` · `no_damage_no_warning`|
|e2e|`crates/buildl/tests/e2e/log.rs` (new; fixtures `chain`, `failing`)|`log_after_a_build_shows_what_each_target_printed` · `log_after_a_cached_build_shows_the_real_run` · `log_while_a_build_holds_the_lock_still_reads` · `log_with_a_blob_removed_is_missing_output` · `log_creates_no_state_directory` · `log_reads_with_a_build_file_broken_since` · `log_shows_a_target_removed_from_the_build_files_since_it_ran` · `log_of_an_alias_is_no_run`|
|CLI|`crates/buildl-cli/tests/cli/output.rs`|`log_prints_each_stream_on_its_own_channel` · `log_stdout_prints_only_stdout` · `log_stderr_prints_stored_stderr_on_stdout` · `log_header_is_on_stderr`|
|CLI|`crates/buildl-cli/tests/cli/exit_codes.rs`|`log_of_a_target_that_never_ran_exits_2` · `log_with_a_blob_removed_exits_3`|
|CLI|`crates/buildl-cli/tests/cli/arguments.rs`|`log_rejects_both_stream_flags` · `log_reads_a_relative_target` · `log_rejects_a_bad_label_with_exit_2`|
|CLI|`crates/buildl-cli/tests/cli/exit_codes.rs`|`log_with_a_broken_build_file_exits_0`|
|unit|`crates/buildl-cli/src/exit.rs`|`no_run_is_a_usage_error` · `missing_output_is_an_infrastructure_failure`|

## 7. Risks

- **The log and the twin can drift.** A later change to `EventView` that misses `LoggedRun` makes
  every new line read as damaged. The round-trip test of §3.1 fails on any such drift.
- **Lines written before a format change read as damaged.** Not reachable today: the format does
  not change in this branch (the test slice keeps `EventView`, its spec §3.6).
- **A whole log is read for one label.** `log.jsonl` is not bounded (non-goal); reading it is
  linear in its size. Not measured.

## 8. Non-goals

- Searching or filtering output; reading it for meaning.
- Following an alias (would need `graph.json` read back).
- Older runs, a target's history, a `--json` form.
- Bounding or rotating `log.jsonl`.
- Which blobs `gc` keeps (I13).
