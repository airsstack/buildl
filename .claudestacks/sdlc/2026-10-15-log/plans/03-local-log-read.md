---
status: done
created: 2026-10-15
depends-on: [02]
built-on: 280eda6f4573
---

# Reading `log.jsonl` back Implementation Plan

**Goal:** `LocalLog` hands over the whole lines of `log.jsonl`, each an event or a damaged line,
without creating, cutting or writing the file.

**Architecture:** `LocalLog` implements the `RunHistory` port (plan `02`) beside `EventLog`. The
read never goes through `open` — which creates the file and cuts a last partial line, the
append side's rules — but reads the file's bytes once: a missing file is no lines, a last line
without a newline is left out and left in place, and each whole line is read with
`canonical::from_slice::<LoggedRun>` (plan `01`), `Ok` an event and `Err` a damaged line by its
1-based number. A read failure names `PortName::RunHistory`.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `tempfile` (tests). Definition of Done:
`cargo make dod` and `cargo deny check`.

**Content authority:** spec §1 P7, P8, §2 (Reads), §3.5 (first bullet), §5, §6 (row for
`log/local_log.rs`).

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-i12-build-log` (branch
`worktree-m2-i11-i12-test-log`), never on `main`. One commit, Conventional Commits, scope
`buildl`.

Facts the task rests on, each read at commit `0cca515`:

| Fact | Where |
|---|---|
| `LocalLog { path, file: Option<File> }`; `new(layout)` touches nothing; `append` opens through `open(path)` (read, append, create, mode `0o644`, then `set_len` to the end of the whole lines) | `crates/buildl/src/log/local_log.rs:30-103` |
| `failed(what, path, error)` makes `Error::Adapter` naming `PortName::EventLog` | `local_log.rs:35-41` |
| Its module doc says reading the log back is something "nothing here does"; Responsibilities: `LocalLog` and its `EventLog` implementation | `local_log.rs:1-11` |
| Its tests: `BUILT_A` (the line of the built `//:a`), helpers `workspace()` (`:151`), `event(name, class)` (`:160`), `built_a()`, `killed_b()`; `a_cut_last_line_is_removed_before_the_first_append` (`:232`), `a_log_that_cannot_be_opened_is_an_adapter_error` (`:257`) | `local_log.rs:105-270` |
| The cache journal reads whole lines the same way: `fs::read`, `NotFound` → nothing, `split_inclusive(b'\n')` then `strip_suffix(b"\n")`, so a last line without a newline is left out | `crates/buildl/src/cache/journal.rs:76-93` |
| `RunHistory::lines(&self) -> Result<Vec<LogLine>>`; `PortName::RunHistory`; `LogLine::{Event(LoggedRun), Damaged { line: NonZeroUsize }}` | `plans/02-pipeline-log.md` Task 2; `plans/01-logged-run.md` Task 2 |
| `guard-adapter-isolation`: `log` may name only `state` and `search_path` of this crate | `Makefile.toml`, `[tasks.guard-adapter-isolation]` |

## Files that change

```
crates/buildl/src/log/local_log.rs   — [modify] RunHistory for LocalLog; failed names its port; docs; tests
```

## Order of work

### Task 1 — `LocalLog` reads its file back

**Files:** `crates/buildl/src/log/local_log.rs`.

**Change:**
- `impl RunHistory for LocalLog`: `fs::read(&self.path)`; `NotFound` → `Ok(vec![])`; any other
  read error → `Error::Adapter` naming `PortName::RunHistory` with the path, as `failed` words it.
  The bytes are split into whole lines as the journal does; each line, numbered from 1, is
  `LogLine::Event` when `canonical::from_slice::<LoggedRun>` reads it and `LogLine::Damaged {
  line }` otherwise. Nothing is opened for writing, created, cut or written; `self.file` is not
  touched. Its doc says so, and that it can run while a build appends: it sees every whole line
  written so far.
- `failed` takes the `PortName` to name; `append` passes `EventLog`.
- The module doc: Responsibilities name both implementations; the non-responsibility "reading the
  log back … which nothing here does" becomes "choosing which line to show, which the core's
  `history` does; and bounding the log's size".

**Proof** (in `local_log.rs`'s tests, written first, seen failing to compile, then passing):
- `a_log_written_by_append_reads_back_line_for_line` — append `built_a()` and `killed_b()`;
  `lines()` is two `Event`s whose `LoggedRun`s write, with `canonical::to_vec`, the same bytes
  as the two events.
- `a_missing_log_has_no_lines_and_is_not_created` — on a fresh layout, `lines()` is empty and
  `layout.log()` does not exist afterwards.
- `a_last_line_without_a_newline_is_ignored_and_kept` — the file is `BUILT_A` then `{"at":`
  with no newline: `lines()` is one `Event`, and the file's bytes are unchanged afterwards.
- `a_hand_edited_line_is_damaged_with_its_number` — three lines: `BUILT_A`, `not json`, `BUILT_A`
  with `"Built"` spelt `"built"`: `lines()` is `Event`, `Damaged { 2 }`, `Damaged { 3 }`.
- `reading_writes_nothing` — after an append, the file's length and modification time are the
  same before and after `lines()`.
- `a_log_that_cannot_be_read_is_a_run_history_error` — `layout.log()` made a directory:
  `lines()` is `Error::Adapter` naming `PortName::RunHistory`.
- The existing tests pass; `a_log_that_cannot_be_opened_is_an_adapter_error` still names
  `EventLog`.

**Done when:** `cargo test -p buildl --lib log::` passes and the Definition of Done is green,
`guard-adapter-isolation` included. Commit `feat(buildl): the local log reads its lines back`.

## Risks

- **Reading beside a writer.** A build appends one whole line per write; a read that meets a line
  half-written sees it without its newline and leaves it out, which is the same rule as for a
  line a dead run left. Not probed concurrently; plan `05`'s e2e test reads while the lock is
  held but not mid-write.
- **The whole file is read** for one command (spec §7); not measured.

## Proof

- `cargo make dod` and `cargo deny check` green.

## Review findings

- nit — `lines`' doc gave one reason a last line has no newline, where the module doc gives two — `log/local_log.rs` — fixed: names an unfinished append and a run that died

## Probe results

- No throwaway probe: the named tests cover the read rules. Seen failing first (`lines` not found), then passing.

## Deviations

- None. Line numbers count from `NonZeroUsize::MIN` with `saturating_add`, so no `unwrap` outside tests.
