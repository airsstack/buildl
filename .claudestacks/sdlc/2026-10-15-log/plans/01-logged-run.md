---
status: done
created: 2026-10-15
built-on: 280eda6f4573
---

# A log line read back Implementation Plan

**Goal:** A line of the build log reads back as a typed event exactly when it is the bytes buildl
writes for one, and as a damaged line otherwise.

**Architecture:** `types::LoggedRun` is the read-side twin of the private `EventView`
(`outcome_event.rs`): the same twelve keys, each with its own type, deriving `Serialize` and
`Deserialize` with `deny_unknown_fields`, read only through `canonical::from_slice`, which
accepts nothing but the bytes it would write. `types::LogLine` is what a reader hands on per
line: `Event(LoggedRun)` or `Damaged { line }`. `OutcomeClass` and `InfraKind` gain
`Deserialize`. A round-trip test pins the twin to the writer, so the two cannot drift silently.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `serde`, `serde_json` (tests), the canonical
serializer. Definition of Done: `cargo make dod` and `cargo deny check`.

**Content authority:** spec §1 P1, P4–P6, §3.1, §3.2 (`LogLine`), §5, §6 (rows for
`types/logged_run.rs`, `types/outcome.rs`), §7 (first risk).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`, after the test slice's ten plans
(`.claudestacks/sdlc/2026-10-15-test/plans/`) are done. One commit per task, Conventional
Commits, scope `buildl-core`. The seven plans of this chain run in number order.

Facts the tasks rest on, each read at commit `0cca515` (no code has changed since the spec's
`7a16e8e`):

| Fact | Where |
|---|---|
| `EventView` holds `at: Timestamp`, `class: &OutcomeClass`, `command: &Command`, `diagnostic: Option<&str>`, `file: String`, `finished: Option<Timestamp>`, `key: Option<Digest>`, `label: &Label`, `outputs: &[Produced]`, `started: Option<Timestamp>`, `stderr: Option<Digest>`, `stdout: Option<Digest>` — twelve fields, written in that (sorted) order by the canonical serializer | `crates/buildl-core/src/types/outcome_event.rs:44-59` |
| `outcome_event.rs`'s module doc says reading a log back is something "nothing in this crate does", and the `Serialize` impl's doc says "There is no `Deserialize`" | `outcome_event.rs:10-11`, `:61-64` |
| `outcome_event.rs`'s tests build events with `command`, `lib_file`, `root_file`, `instant`; `a_skipped_target_is_logged_without_a_key_an_instant_or_a_digest` and `an_event_holds_exactly_the_twelve_keys_of_the_log_format` | `outcome_event.rs:87-130`, `:212-240`, `:243` |
| `Deserialize` is already derived on `Timestamp` (`types/timestamp.rs:25`), `Digest` (`types/digest.rs:27`), `Label` (`types/label.rs:26`), `OutputName` (`types/output_name.rs:22`), `Command` (`types/argument.rs:153`), `Produced` (`types/action_key.rs:39`), `OutputRefusal` (`types/outcome.rs:40`) | as cited |
| `InfraKind` (`outcome.rs:20`) and `OutcomeClass` (`outcome.rs:58`) derive `Serialize` only; both are `#[non_exhaustive]` | `crates/buildl-core/src/types/outcome.rs:18-99` |
| `outcome.rs`'s test helper `every_class()` returns the eight classes; `every_refusal_serializes_as_its_name_and_reads_back` reads back with `serde_json::from_str` | `outcome.rs:181`, `:333` |
| `canonical::from_slice<T: DeserializeOwned + Serialize>` deserializes, re-serializes and compares: a difference is `Error::NonCanonicalJson { offset }`, a parse failure `Error::UnreadableJson` | `crates/buildl-core/src/json/canonical.rs:129-141` |
| `types/mod.rs` declares one `pub mod` per file and re-exports each type; the root `lib.rs` re-exports the `types` list | `crates/buildl-core/src/types/mod.rs:70-180`; `crates/buildl-core/src/lib.rs:122-135` |
| After the test slice, `ActionOutcome` carries `role`, which the log does not write; `EventView` is unchanged | `.claudestacks/sdlc/2026-10-15-test/plans/03-test-outcomes.md`, Task 1 |

## Files that change

```
crates/buildl-core/src/types/outcome.rs        — [modify] Deserialize on OutcomeClass and InfraKind; a test
crates/buildl-core/src/types/logged_run.rs     — [create] LoggedRun and its unit tests
crates/buildl-core/src/types/log_line.rs       — [create] LogLine, a type definition only
crates/buildl-core/src/types/outcome_event.rs  — [modify] its docs name the reader; the twin test
crates/buildl-core/src/types/mod.rs            — [modify] declare and re-export both new types
crates/buildl-core/src/lib.rs                  — [modify] LoggedRun and LogLine in the root re-exports
```

## Order of work

### Task 1 — An outcome class reads back as written

**Files:** `crates/buildl-core/src/types/outcome.rs`.

**Change:** `InfraKind` and `OutcomeClass` derive `Deserialize` beside `Serialize`, with no serde
attribute, so they read exactly the forms they write (a name, or an object of one key holding the
fields). The two type docs say they read back as written.

**Proof:**
- `every_class_reads_back_as_written` in `outcome.rs`'s tests — each of `every_class()`, written
  with `canonical::to_string`, reads back through `canonical::from_slice` to an equal class; and
  each of the four `InfraKind`s does the same.

**Done when:** `cargo test -p buildl-core --lib types::outcome` passes and the Definition of Done
is green. Commit `feat(buildl-core): an outcome class reads back as written`.

### Task 2 — `LoggedRun` and `LogLine`

**Files:** `types/logged_run.rs`, `types/log_line.rs`, `types/outcome_event.rs`, `types/mod.rs`,
`lib.rs` (all `crates/buildl-core/src`).

**Change:**
- `LoggedRun`, all fields public, documented "One event of the build log as it reads back: the
  twelve keys a build writes, each with its own type": `at: Timestamp`, `class: OutcomeClass`,
  `command: Command`, `diagnostic: Option<String>`, `file: String`, `finished: Option<Timestamp>`,
  `key: Option<Digest>`, `label: Label`, `outputs: Vec<Produced>`, `started: Option<Timestamp>`,
  `stderr: Option<Digest>`, `stdout: Option<Digest>`, declared in that order; derives
  `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`; `#[serde(deny_unknown_fields)]`. Its
  rustdoc says a line is one of these exactly when `canonical::from_slice` reads it, so a line
  missing a key, holding another, or not in canonical form is not an event; and that `file` is
  the build file's path as written, not a `Provenance` (the log does not hold the directory).
  The module doc follows the house shape and names its twin, `OutcomeEvent`'s serialized form.
- `LogLine`, `Debug, Clone, PartialEq, Eq`: `Event(LoggedRun)` or `Damaged { line:
  NonZeroUsize }` ("a whole line that is not an event, by its 1-based number in the file").
  `log_line.rs` holds only the type, and its module doc ends with "This file holds only a type
  definition, so it carries no logic to unit-test." (as `types/scope.rs:12`).
- `types/mod.rs`: `pub mod log_line;`, `pub mod logged_run;`, their `pub use` lines, and one
  Responsibilities bullet: "`LoggedRun` and `LogLine` — a line of the build log as it reads
  back: an event, or a damaged line."; `lib.rs` adds both to the root list.
- `outcome_event.rs`'s module doc says the log is read back as `LoggedRun`, and the `Serialize`
  impl's doc drops "There is no `Deserialize`… never read back here" for "It is read back as a
  `LoggedRun`, whose keys are these".

**Proof** (in `logged_run.rs`'s tests):
- `an_event_line_reads_back_as_a_logged_run_with_the_same_bytes` — for a built event (key,
  outputs, both instants, both digests), a failed one with a diagnostic, and a skipped one (no
  key, instants or digests), the bytes `canonical::to_vec(&event)` writes read back through
  `canonical::from_slice::<LoggedRun>` and write the same bytes again; the fields equal the
  event's (`file` is `lib/build.lua`, `diagnostic` the text).
- `a_line_with_a_key_missing_is_not_an_event` — the built event's line without `"finished"` is
  refused.
- `a_line_with_an_unknown_key_is_not_an_event` — the line with `"role":"Build"` added in its
  sorted place is refused.
- `a_line_out_of_canonical_form_is_not_an_event` — the line with a space after its first `:` is
  refused as `NonCanonicalJson`.
- `an_event_holds_exactly_the_twelve_keys_of_the_log_format` (`outcome_event.rs:243`) — existing,
  unchanged, passes.

**Done when:** `cargo test -p buildl-core --lib types::` passes and the Definition of Done is
green. Commit `feat(buildl-core): a log line read back as a run`.

## Risks

- **The twin drifts from the writer** (spec §7). The round-trip test fails on any field added to
  `EventView` without `LoggedRun`, or renamed in one of them.
- Validated fields keep their grammar on the way back: `Label`, `OutputName`, `Argument` and
  `Command` read through `try_from` (`label.rs:27`, `output_name.rs:23`, `argument.rs:30`,
  `:154`), so a line holding a label or a command outside its grammar is damaged, not a run.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.

## Review findings

- reversion guard — the missing-key test asserted only `is_err` — `types/logged_run.rs` — fixed: asserts `NonCanonicalJson`
- reversion guard — nothing guarded `deny_unknown_fields` (the canonical check refuses the line anyway) — `types/logged_run.rs` — fixed: the unknown-key test asserts `UnreadableJson`; `cargo test -p buildl-core --lib types::logged_run` → `4 passed; 0 failed`
- consistency — `crate::error::Error` written inline in a test — `types/logged_run.rs` — fixed: a `use` line as siblings have
- doc-comment-discipline — `outcome_event.rs`'s module doc said `LoggedRun` reads a log back; reading is a port's — `types/outcome_event.rs:11` — fixed; `cargo make dod` → `Build Done in 27.88 seconds.`
- unit-test-mandate — the `InfraKind` list in `every_class_reads_back_as_written` is written by hand — `types/outcome.rs:344` — kept: it mirrors the existing `every_kind_of_infrastructure_failure_serializes_as_its_name` list beside it

## Probe results

- No throwaway probe: the named tests cover each premise. Seen: a line without `"finished"` is refused by the canonical re-serialization (serde fills an absent `Option` with `None`), not by serde.

## Deviations

- 2026-10-15 — `LogLine` carries `#[expect(clippy::large_enum_variant, reason = …)]`: `Event` is 336 bytes, `Damaged` 8. Kept to hold the spec's shape `Event(LoggedRun)` (spec §3.2, `spec.md:74`), which plans 02 and 03 match on; boxing would change the approved spec.
- 2026-10-15 — tests name `crate::error::Error`, as `guard-module-edges` refuses `crate::Error`.
