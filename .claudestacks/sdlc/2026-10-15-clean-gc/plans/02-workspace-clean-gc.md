---
status: executing
created: 2026-10-19
depends-on: [01]
built-on: 137a1ada1042
---

# Workspace clean and gc Implementation Plan

**Goal:** The `Workspace` facade runs `clean` and `gc` against a real workspace's state.

**Architecture:** `LocalCas` implements `Sweep` by walking its shard directories. `Workspace::clean`
lives in the facade alone (design §7 gives it no phase): ceiling check on the output directory,
lock, then `state::clear_out`. `Workspace::gc` takes the lock, opens the real cache, cas and log,
calls `pipeline::gc`, then folds, as `Workspace::build` does. `Cleaned` and `clear_out` live in
`state`, beside `clear_path`, because the facade and `report` may both import `state`
(`Makefile.toml:212-217`) and `report` may not import `workspace`; `report::message` gains the two
summary lines.

**Tech Stack:** Rust (`buildl`), `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2, §3.5, §3.7, §4 rows `LocalCas` adapter and e2e via `Workspace`.

---

## Files that change

```
crates/buildl/src/cas/local_cas.rs        — [modify] `impl Sweep for LocalCas` and its unit tests
crates/buildl/src/state/clear_out.rs      — [create] `Cleaned { Removed, Absent }`, `clear_out`, unit tests
crates/buildl/src/state/mod.rs            — [modify] declare `clear_out`, export `Cleaned` and `clear_out`
crates/buildl/src/local_ports.rs          — [modify] `Wiring::out` getter; `Wiring::writable_out`: the output directory, held to the ceiling
crates/buildl/src/workspace.rs            — [modify] `Workspace::out`, `Workspace::clean`, `Workspace::gc`
crates/buildl/src/report/message.rs       — [modify] `clean_line`, `gc_line` and their unit tests
crates/buildl/src/report/mod.rs           — [modify] export `clean_line`, `gc_line`
crates/buildl/src/lib.rs                  — [modify] re-export `Cleaned` beside `Workspace`
crates/buildl/tests/e2e/clean_gc.rs       — [create] end-to-end tests through `Workspace`
crates/buildl/tests/e2e/main.rs           — [modify] `mod clean_gc;`
```

## Order of work

### Task 1 — Implement `Sweep` for `LocalCas`

**Files:**
- Modify `crates/buildl/src/cas/local_cas.rs` — `impl Sweep for LocalCas`
- Test `crates/buildl/src/cas/local_cas.rs` — its `#[cfg(test)] mod tests` (begins `:276`); a test
  helper there is already named `held` (`:353`), so the new tests call the port as `Sweep::held(&cas)`
  or rename that helper

**Change:** `held` reads every two-hex-character directory under `cas/` (`BlobPath::of`,
`blob.rs:24-33`); a name of the other 62 hex characters, with or without `.x`, joins the two into a
digest (`Digest::from_hex`, `digest.rs:56`); `tmp`, foreign names and non-hex names are ignored; a
missing `cas/` returns an empty set. `remove` clears the blob and the twin through `clear_path`
(`state/clear.rs:25`) and returns the sum of their sizes, `0` when neither exists; empty shard
directories stay. I/O errors become `Error::Adapter { port: PortName::Sweep, … }`.

**Proof:**
- `held_lists_blob_only_twin_only_and_both` — three digests stored as blob only, `.x` only, and
  both: `held` is those three, once each, in digest order.
- `held_ignores_tmp_and_foreign_names` — a file in `cas/tmp`, `cas/zz/notes`, `cas/ab/short`:
  `held` is empty and the files remain.
- `held_is_empty_without_a_cas_directory` — fresh layout, nothing made: empty set, and no
  directory created.
- `remove_deletes_both_names_and_returns_their_bytes` — blob 5 bytes, twin 5 bytes: `remove`
  returns 10, neither path exists, the shard directory still exists.
- `remove_of_an_absent_digest_returns_zero`.

**Done when:** `cargo test -p buildl --lib cas` passes, `cargo make guard-adapter-isolation` green,
and `cargo make dod` green. Commit `feat(buildl): list and remove what the local store holds`.

### Task 2 — Add `Workspace::clean`

**Files:**
- Create `crates/buildl/src/state/clear_out.rs` — `pub enum Cleaned { Removed, Absent }` (whether
  the output directory held something) and
  `pub(crate) fn clear_out(root: &Path, out: &Directory) -> io::Result<Cleaned>`: looks at
  `root.join(out.as_str())` with `symlink_metadata`, then `clear_path` (`state/clear.rs:25`)
- Modify `crates/buildl/src/state/mod.rs` — `mod clear_out;`, `pub use clear_out::Cleaned;`,
  `pub(crate) use clear_out::clear_out;`
- Modify `crates/buildl/src/lib.rs` — `pub use state::Cleaned;` next to `pub use workspace::Workspace` (`lib.rs:168`)
- Modify `crates/buildl/src/local_ports.rs` — `Wiring::out(&self) -> &Directory`, the manifest's
  output directory (`types/workspace.rs:20`); `Wiring::writable_out(&self) -> Result<&Directory>`: the
  manifest's output directory when some `ceiling.write` entry `contains` it (`directory.rs:77`),
  else `Error::Refused { want: Want::Write(out), site: None, ceiling }`, the same check `Wiring::lock`
  makes for the state directory (`local_ports.rs:192-201`)
- Modify `crates/buildl/src/workspace.rs` — `pub fn out(&self) -> &Directory` forwarding to
  `Wiring::out` (the CLI names the directory in its line); `pub fn clean(&self) -> Result<Cleaned>`
- Modify `crates/buildl/src/report/message.rs` — `pub fn clean_line(cleaned: &Cleaned, out: &Directory) -> String`
- Modify `crates/buildl/src/report/mod.rs` — export `clean_line` (`mod.rs:49`)

**Change:** `clean` = `wiring.writable_out()?` (refused before the lock), `wiring.lock()?` (which sweeps
staging leftovers, spec P2), `clear_out(root, out)`; an I/O error becomes
`Error::Adapter { port: PortName::ContentStore, … }`. `clear_out` returns `Removed` when something
stood at the path, `Absent` otherwise. Rows, cas and log untouched. `clean_line` renders
`clean: removed out\n` / `clean: out holds nothing\n` with the manifest's directory name; like
`check_line` (`message.rs:32-48`, "Ends with a newline.") each line ends with a newline.

**Proof:**
- `clear_out.rs` unit tests `a_directory_of_files_is_removed` (→ `Removed`, path gone, a sibling
  directory remains), `a_missing_directory_is_absent` (→ `Absent`), `a_symlink_is_removed_not_followed`
  (a link at the path to a directory outside: → `Removed`, the link gone, the target's files remain).
- `local_ports.rs` unit tests `writable_out_is_refused_when_the_ceiling_does_not_allow_writing_it`
  and `writable_out_is_the_output_directory_when_the_ceiling_allows_it`.
- `message.rs` unit tests `clean_line_names_the_removed_directory` → `"clean: removed out\n"`,
  `clean_line_says_when_nothing_was_there` → `"clean: out holds nothing\n"`.
- `workspace.rs` `mod tests` (beside `build_is_refused_while_another_buildl_holds_the_workspace`,
  `:675`): `clean_is_refused_before_the_lock_when_the_ceiling_leaves_out_the_output_directory`
  (`Error::Refused`, and no `.buildl/lock` was made) and `clean_is_refused_while_another_buildl_holds_the_workspace`.
- e2e `clean_gc.rs`:
  - `clean_then_build_restores_every_output_from_the_store` — build fixture `logged`, `clean` → `Removed`, `out/` gone, `.buildl/cas`
    and `cache.json` unchanged; build again: every outcome `cached`, `out/` lists the same files.
  - `clean_without_an_output_directory_is_absent` — fresh workspace: `Absent`.
  - `clean_removes_staging_leftovers` — a file placed in `.buildl/cas/tmp` before `clean` is gone after.
  - `clean_is_refused_when_the_ceiling_leaves_out_the_output_directory` — manifest whose
    `[ceiling] write` omits `out`: `Error::Refused`, and `out/` is untouched.

**Done when:** `cargo test -p buildl` passes and `cargo make dod` is green. Commit
`feat(buildl): add Workspace::clean`.

### Task 3 — Add `Workspace::gc`

**Files:**
- Modify `crates/buildl/src/workspace.rs` — `pub fn gc(&self) -> Result<Collected>`
- Modify `crates/buildl/src/report/message.rs` — `pub fn gc_line(collected: &Collected) -> String`
- Modify `crates/buildl/src/report/mod.rs` — export `gc_line`
- Modify `crates/buildl/tests/e2e/clean_gc.rs` and `crates/buildl/tests/e2e/main.rs` (`mod clean_gc;`,
  first added in Task 2)

**Change:** `gc` = `wiring.lock()?`; `LocalCache::open(&layout)?`; `LocalCas::new(root, &layout)`;
`LocalLog::new(&layout)`; `pipeline::gc(&wiring.source(), wiring.entry(), &mut cache, &cas, &cas,
&log)`; then `cache.fold()` whatever `gc` returned, and the adapters are dropped before the lock,
as `Workspace::build` does (`workspace.rs:179-195`, its `concluded` helper decides which error
wins). `gc_line` uses `counted` (`message.rs:25`) for all three counts:
`gc: dropped 1 row, removed 5 blobs (2048 bytes)\n`. `counted` takes `usize` today and
`bytes_removed` is `u64`; a cast trips pedantic clippy, so `counted` becomes generic over the count,
`N: Copy + Display + PartialEq + From<u8>` (singular when `count == N::from(1)`), which both `usize` and
`u64` satisfy, and its existing callers and tests stay unchanged.

**Proof:**
- `message.rs` unit tests `gc_line_counts_singular_and_plural` (1 row / 1 blob / 1 byte vs 2 / 2 / 2),
  `gc_line_of_nothing_removed` → `"gc: dropped 0 rows, removed 0 blobs (0 bytes)\n"`, and
  `counted_counts_a_u64` (`counted(1_u64, …)` singular, `counted(u64::MAX, …)` plural).
- `workspace.rs` `mod tests`: `gc_folds_the_rows_it_forgot_and_removes_the_journal` (after `gc`
  dropping one row: `cache.json` lacks it and `cache.journal` is gone, as
  `a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal`, `:716`, checks for a build)
  and `gc_is_refused_while_another_buildl_holds_the_workspace`.
- e2e `clean_gc.rs`:
  - `gc_drops_an_undeclared_targets_row_and_its_only_blobs` — fixture `logged` (`hello`, and
    `quiet` depending on it; both succeed), in a copy where `quiet` copies its own `quiet.txt`
    so it stores bytes no kept row shares: build, remove `quiet` from `build.lua`, `gc`:
    `rows_dropped == 1`, `blobs_removed > 0`; `cache.json` has no row for `quiet`; a `build` after
    `gc` reports `hello` cached, so nothing a kept row needs was removed.
  - `log_of_a_dropped_target_still_reads_after_gc` — `Workspace::log` of that target returns the
    same `Shown` before and after `gc`.
  - `a_second_gc_removes_nothing` — all zeros.
  - `outputs_in_out_still_read_after_gc` — every file under `out/` reads the same bytes after `gc`.
  - `gc_removes_staging_leftovers` — a file in `.buildl/cas/tmp` is gone.
  - `a_broken_build_file_stops_gc_and_leaves_the_store` — a build file with a Lua error: `gc`
    returns the error `Workspace::check` returns, and `.buildl/cas` lists the same files as before.
  - `gc_is_refused_while_the_lock_is_held` — a held `StateLock`: `Error::Adapter` naming `ActionCache`.

**Done when:** `cargo test -p buildl` passes and `cargo make dod` is green. Commit
`feat(buildl): add Workspace::gc`.

---

## Risks

- **`clean` and `out/`'s hard links** (spec P23): `clear_path` unlinks names only, so blob bytes in
  the cas stay whole — `clean_then_build_restores_every_output_from_the_store` would show a breach.
- The `chain` fixture has one failing target (spec P1), so the e2e tests use `logged`
  (`crates/buildl/tests/fixtures/logged/build.lua`: `hello`, `quiet` depending on it, alias
  `greeting`), whose targets all succeed.
- `Cleaned` in `state` is a placement forced by the adapter-isolation guard: `report` cannot import
  `workspace`.
- Not done here: the CLI and documents (plan 03).

## Proof

- `cargo make dod` (which runs every `guard-*`) and `cargo deny check` green after each task.
- `cargo test -p buildl --test e2e clean_gc` passes as a whole.
