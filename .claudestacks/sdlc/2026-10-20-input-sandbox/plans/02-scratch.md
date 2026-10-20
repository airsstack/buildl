---
status: approved
created: 2026-10-20
built-on: 5fb250597c90
---

# Scratch Directory Implementation Plan

**Goal:** Every command that takes the state lock removes the workspace's scratch directory when it takes the lock and again when it lets go.

**Architecture:** A new `state::Scratch` names the per-workspace scratch directory outside the
workspace and each target's exec dir in it, claims it safely, and removes it. It lives in
`state` because both the lock and the sandbox (plan 03) need it and adapter modules do not import
each other. `Wiring` builds one `Scratch` when it reads the workspace and hands it to
`StateLock::acquire`, which removes it after taking the lock and in `Drop`, and reports the lock
file's owner, as an `Owner` newtype, for the sandbox's `claim`.

**Tech Stack:** Rust (`buildl`), `std::os::unix::fs::{DirBuilderExt, MetadataExt}`, `cargo make dod`,
`cargo deny check`.

**Content authority:** spec §2 rows Where an action runs, Clean up, Scratch safety; §3.2; §4
rows 2–3; §5 row `buildl` unit `state/scratch.rs`, `state/lock.rs`.

---

## Files that change

```
crates/buildl/src/state/scratch.rs         — [create] `Scratch` and its unit tests
crates/buildl/src/state/mod.rs             — [modify] declare `scratch`, export `Scratch` and `Owner`, list them in the doc
crates/buildl/src/workspace.rs             — [modify] `plan`, `clean`, `gc` `# Errors` name the scratch failure
crates/buildl/src/state/lock.rs            — [modify] `acquire(layout, scratch)`, `owner()`, `LockError::Scratch`, removal on take and drop
crates/buildl/src/local_ports.rs           — [modify] `Wiring` holds a `Scratch`; `lock()` maps `LockError::Scratch` to `ExecStrategy`
crates/buildl/src/statcache/local_stat_cache.rs — [modify] test call of `acquire` (`:186`)
crates/buildl/tests/e2e/common.rs          — [modify] `Fixture::scratch()`
crates/buildl/tests/e2e/clean_gc.rs        — [modify] `acquire` call (`:195`)
crates/buildl/tests/e2e/log.rs             — [modify] `acquire` call (`:57`)
crates/buildl/tests/e2e/recovery.rs        — [modify] `acquire` call (`:87`)
crates/buildl/tests/e2e/facade.rs          — [modify] `acquire` calls (`:45`, `:178`)
```

## Order of work

### Task 1 — Add `Scratch`

**Files:**
- Create `crates/buildl/src/state/scratch.rs`
- Modify `crates/buildl/src/state/mod.rs` — `mod scratch;`, `pub use scratch::Scratch;`, a
  Responsibilities bullet

**Change:** `Scratch` (`Debug`, `Clone`) holds the scratch path and the workspace root.
- `pub fn new(temp: &Path, root: &Path) -> Self` — the path is
  `temp/buildl-<first 32 hex digits of Digest::of(root's bytes)>` (`Digest::of`,
  `buildl-core/src/types/digest.rs:43`; its `Display` is hex, `:127`). Touches nothing on disk.
- `pub fn dir(&self) -> &Path`.
- `pub(crate) fn action(&self, label: &Label) -> PathBuf` — `dir/<first 32 hex of Digest::of(label text)>`.
- `pub struct Owner(u32)` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`): the user a directory
  belongs to, built only from file metadata (`pub(crate) fn of(&Metadata) -> Self`, through
  `MetadataExt::uid`), so a uid never travels as a bare `u32` (rust guideline, strong-types).
- `pub(crate) fn claim(&self, owner: Owner) -> io::Result<()>` — creates the directory with mode
  `0700` (`DirBuilderExt::mode`); on `AlreadyExists` (another worker, or a leftover) falls through
  to the check. The check (`symlink_metadata`, `MetadataExt::{uid, mode}`): a directory, not a
  link; `Owner::of(metadata) == owner`; `mode & 0o777 == 0o700`; its canonical path does not start with the
  canonical root. Each refusal is an `io::Error` whose text gives the reason and **no path**
  (spec §2 Diagnostics): "the scratch directory is a symbolic link", "… is not a directory",
  "… belongs to another user", "… has mode 0755, not 0700", "… lies inside the workspace".
- `pub(crate) fn remove(&self) -> io::Result<()>` — `clear_path(dir)` (`state/clear.rs:25`).

No `unsafe`. The module doc says why it is outside the workspace (tools that search upward
would find the workspace from inside it; spec P16) and why the path is stable per target (a
compile with debug info records its directory; spec P21) — the reasons in plain words, the
spec IDs only here, never in rustdoc.

**Proof:** unit tests in `scratch.rs`, each with its own `tempfile` base as `temp`:
- `ids_are_stable_and_distinct` — two `Scratch::new` on one root give one `dir`; two roots give
  two; `action` of one label is equal across calls and differs for two labels; `dir` is
  `temp/buildl-` + 32 hex digits and `action` is `dir/` + 32 hex digits.
- `claim_makes_the_directory_0700` — after `claim(own uid)` the directory exists with mode
  `0o700`.
- `claim_refuses_a_link` — `dir` is a symlink to a `0700` directory: error, text contains
  "symbolic link" and not the path.
- `claim_refuses_a_wrong_mode` — existing `dir` at `0755`: error naming `0755`.
- `claim_refuses_another_owner` — an existing `dir` claimed with the `Owner` of a file the test
  does not own (`/` is root's): error "another user". Skipped when the test runs as root.
- `claim_refuses_a_scratch_inside_the_root` — `temp` is a directory under the root: error
  "inside the workspace".
- `claim_from_many_threads_at_once_succeeds` — eight threads claim one fresh `Scratch` at once:
  all `Ok`.
- `remove_removes_a_tree_with_read_only_files` — files `0444` below `dir`: `remove` leaves
  nothing; `remove` with nothing there is `Ok`.

**Done when:** `cargo test -p buildl --lib state::scratch` passes and `cargo make dod` is green.
Commit `feat(buildl): add the scratch directory outside the workspace`.

### Task 2 — Remove the scratch directory with the lock

**Files:**
- Modify `crates/buildl/src/state/lock.rs` — `acquire(layout: StateLayout, scratch: Scratch)`
  (`lock.rs:48`); a field for the scratch and one for the owner; `pub const fn owner(&self) -> Owner`;
  `LockError::Scratch { path: PathBuf, source: io::Error }`, rendered
  `"{path}: removing the scratch directory: {source}"`; the doc of `acquire` and the module doc
  say the scratch directory is removed
- Modify `crates/buildl/src/local_ports.rs` — `Wiring` gains `scratch: Scratch`, built in `read`
  (`local_ports.rs:89-99`) as `Scratch::new(&std::env::temp_dir(), root.as_path())`, plus
  `pub const fn scratch(&self) -> &Scratch`; `lock()` (`:217-231`) passes `self.scratch.clone()`
  and maps `LockError::Scratch` to `Error::Adapter { port: PortName::ExecStrategy }` and every
  other `LockError` to `ActionCache` as today; its `# Errors` says so
- Modify `crates/buildl/src/workspace.rs` — the `# Errors` of `plan` (`:176`), `clean` (`:231`)
  and `gc` (`:258`) add: `Error::Adapter` naming `PortName::ExecStrategy` when the scratch
  directory cannot be removed
- Modify the test callers of `acquire`: `statcache/local_stat_cache.rs:186`, `state/lock.rs`
  tests (`:192`–`:291`), and e2e `clean_gc.rs:195`, `log.rs:57`, `recovery.rs:87`,
  `facade.rs:45`, `facade.rs:178`, which pass `Fixture::scratch()`
- Modify `crates/buildl/tests/e2e/common.rs` — `pub(crate) fn scratch(&self) -> Scratch`,
  `Scratch::new(&env::temp_dir(), self.root())`, beside `state()` (`common.rs:80`)

**Change:** after the lock is taken and the staging sweep ran (`lock.rs:63-64`), `acquire`
removes the scratch directory; a failure there is `LockError::Scratch` and the lock is let go.
The owner is `Owner::of` the lock file's metadata, read once at acquire. `Drop` (`:88-94`) removes the scratch
directory before unlocking and ignores a failure. Every command that locks (`workspace.rs:184`,
`:211`, `:237`, `:266`, `:313`, spec P15) gets this through `Wiring::lock`.

**Proof:**
- `lock.rs` `taking_the_lock_removes_the_scratch_directory` — a scratch dir holding a leftover
  tree exists before `acquire`; after it, the dir is gone.
- `lock.rs` `dropping_the_lock_removes_the_scratch_directory` — create the dir while the lock is
  held; after `drop`, it is gone.
- `lock.rs` `the_owner_is_the_lock_files_owner` — `owner()` equals `Owner::of` the lock file's metadata.
- `lock.rs` `a_scratch_that_cannot_be_removed_fails_the_lock` — the scratch dir holds a `0500`
  directory with a file in it: `acquire` is `LockError::Scratch` naming the scratch path; the
  lock is free again afterwards (a second `acquire` after fixing the mode succeeds). The test
  restores the mode so its temp directory can be removed.
- `local_ports.rs` `a_scratch_that_cannot_be_removed_is_the_exec_strategys_adapter_error` — the
  same set-up at `wiring.scratch().dir()`: `wiring.lock()` is `Error::Adapter { port: ExecStrategy }`;
  the test restores the mode and removes the directory.
- Existing lock, stat cache and e2e tests pass with the new argument.

**Done when:** `cargo test -p buildl` passes and `cargo make dod` is green. Commit
`feat(buildl): remove the scratch directory when the state lock is taken and let go`.

---

## Risks

- **Shared temp directory.** Tests that use `Wiring` or `Fixture` put their scratch under the
  real `std::env::temp_dir()`; each workspace has its own digest, so parallel tests do not meet,
  and the lock's `Drop` removes what each made.
- **A scratch directory nobody can remove** (another user's, or one left with a `0500` directory
  in it) fails every command that locks, exit 3, until removed by hand; spec §4 asks for exactly
  that. Plan 03's `ExecDir` removal is what keeps buildl's own runs from leaving one.
- Not done here: `claim` and `action` are used by the sandbox (plan 03).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` green: only `state` and `local_ports` change.
