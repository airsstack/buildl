---
status: done
created: 2026-10-26
depends-on: [05]
built-on: 0e431e5caec9
---

# Remote Report Implementation Plan

**Goal:** A target the server ran is reported as a success named `remote`, in output and in the exit status.

**Architecture:** `OutcomeClass` is `#[non_exhaustive]` (`buildl-core/src/types/outcome.rs:65`),
so `buildl` and `buildl-cli` match it with `_` arms the compiler does not flag. Every place that
names `Fetched` as a success, or as a word, gains `Remote` beside it: the exit status, the
failure text and its trailer, the test blocks that hide passed tests, the block marker, the build
and test summaries, and `buildl log`'s class word. The count comes from `Tally.remote` (plan 05).
Spec §8: `Remote` "counts as success wherever `Built` and `Fetched` do".

**Tech Stack:** Rust, `buildl` (`report` module), `buildl-cli` (`exit`), `cargo make dod`.

**Content authority:** spec §8 (`Remote` is a success), §9 (the word `remote`, `(remote)`, the summary order, the test share).

---

## Files that change

```
crates/buildl-cli/src/exit.rs          — [modify] Exit::of_class (:47-59): Remote is Success beside Fetched; tests
crates/buildl/src/report/block.rs      — [modify] failure_text (:143-166) gives no text for Remote, so trailer (:176) prints none; marker (:227) " (remote)"; summary pairs (:66) and test shares (:90) gain remote after fetched; tests (:297-341)
crates/buildl/src/report/terminal.rs   — [modify] hides (:171) hides a passed Remote test as it hides Fetched; summary test (:677-683)
crates/buildl/src/report/log_text.rs   — [modify] ended (:66-74) names the class remote; test (:142)
```

The success sites are every `Fetched` arm outside `buildl-core`: `grep -rn "Fetched" crates/buildl/src
crates/buildl-cli/src` lists them (control: the search finds `block.rs:227`); each that treats
`Fetched` as a success or names it gains `Remote`.

## Order of work

### Task 1 — `Remote` is a success

**Files:** `exit.rs`, `block.rs` (`failure_text`), `terminal.rs` (`hides`).

**Change:** `Exit::of_class(Remote)` is `Success`; `failure_text(Remote)` is `None`, so no FAILED
trailer follows a remote block; `hides` hides a passed remote test under `TestBlocks::Failed`.

**Proof:** written first, seen failing, then passing —
- `a_remote_class_exits_as_success` (`exit.rs` tests)
- `a_remote_target_has_no_failure_text` (`block.rs` tests) — and no trailer
- `a_passed_remote_test_is_hidden_when_only_failures_are_shown` (`terminal.rs` tests)

**Done when:** `cargo test -p buildl report` and `cargo test -p buildl-cli exit` pass; `cargo make dod`
green. Commit `fix(buildl): a target the remote ran is a success`.

### Task 2 — The word `remote`

**Files:** `block.rs` (marker, summaries), `terminal.rs` (summary test), `log_text.rs`.

**Change:** a block for an `OutcomeClass::Remote` target ends ` (remote)`, as `(fetched)` does
(`block.rs:227`). The build summary lists `remote` after `fetched`
(`1 built, 3 cached, 2 fetched, 4 remote`), and omits it when zero, as the others are omitted.
The test summary's shares list `remote` after `fetched`
(`tests: 6 passed (1 cached, 2 fetched, 3 remote)`). `buildl log` names the class `remote`.

**Proof:** —
- `a_remote_block_is_marked_remote` (`block.rs` tests)
- `the_summary_counts_remote_after_fetched` — beside `:325-330`
- `a_zero_remote_count_is_not_shown` — the existing `:330` string unchanged
- `a_remote_test_passed_and_shows_its_share` — beside `:334-341`
- `the_log_names_a_remote_run` (`log_text.rs` tests, beside `:142`)
- `the_terminal_summary_counts_remote` (`terminal.rs` tests, beside `:677-683`)

**Done when:** `cargo test -p buildl report` passes; `cargo make dod` green. Commit
`feat(buildl): show targets the remote ran`.

## Risks

- A `_` arm missed by the grep would show only as a wrong word or status; plan 09 Task 3's
  end-to-end tests check the exit status and the summary of a remote build.
- The summary order is part of the documented output (design §7); docs change in plan 12.

## Proof

`cargo make dod` green; the existing summary tests (`block.rs:330`, `:341`, `terminal.rs:683`)
pass unchanged, since a zero count is not shown.

## Review findings

Task 1:
- test-coverage — `every_class()` in `exit.rs` tests listed every class but `Remote` — fixed: `(OutcomeClass::Remote, Exit::Success)`; `cargo test -p buildl-cli --bin buildl exit` → `29 passed; 0 failed`
- risk — between the two commits `buildl log` would print an empty class word for `Remote` — closed by Task 2's `the_log_names_a_remote_run`
- docs — `failure_text` doc over the line width; `Remote` missing from `a_target_that_did_not_fail_has_no_trailer` — fixed
- kept — `hides` and `failure_text` repeat the success set `OutcomeClass::succeeded` holds, as before this change

Task 2:
- docs — two doc lines over the line width — fixed
- test-coverage — `a_zero_remote_count_is_not_shown` did not show its zero — fixed: `remote: 0` written out; `cargo test -p buildl --lib report` → `141 passed; 0 failed`
- spec — `test_line` now counts `remote` as passed; spec §9 requires it (`tests: 6 passed (1 cached, 2 fetched, 3 remote)`) — no amendment needed

Gates: `cargo make dod` passes fmt, clippy, doc and the guards, and every test but one buildl-cli watch test, a different one per run (`watch::a_watch_is_quiet_while_nothing_changes`, `watch::an_edit_rebuilds`), which passes alone (`cargo test -p buildl-cli --test cli watch` → `5 passed; 0 failed`); doctests → ok.

Named tests: `conformance.py … 06` → `plan 06: 9/9 named tests present`.

## Probe results

- the sites the plan names — `grep -n` → `exit.rs` `of_class` :47, `block.rs` `failure_text` :143, marker :227, `terminal.rs` `hides` :161, `log_text.rs` `ended` :70 — as planned
