---
status: done
created: 2026-10-26
depends-on: [01]
built-on: a08d695c7505
---

# Notify Adapter Implementation Plan

**Goal:** `buildl` turns the operating system's file events under a workspace root into debounced `ChangeBatch`es.

**Architecture:** A new adapter module, `crates/buildl/src/watch/`, implements plan 01's
`Changes` port over `notify` 8.2.1 (spec §5, P1). `NotifyChanges::start(root)` canonicalizes
the root (P4) and starts a recursive `notify::recommended_watcher` sending into a std
`mpsc` channel; `next` blocks on `recv` for the first kept path, then gathers with
`recv_timeout(50 ms)` until a quiet window, so no timer runs while nothing changes. It drops
only paths outside the root; dropping `out`, state and `.git` is `react`'s (plan 01). It
imports no other adapter module of `buildl` (`cargo make guard-adapter-isolation`, the
guard reads every `crate::<module>` text, `Makefile.toml:211-275`). The only time value lives
here, as the `exec` adapter's deadline does (`docs/architecture.md:547`).

**Tech Stack:** Rust, `buildl`, `notify` 8.2.1, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §5, §8 (adapter row), P1-P5.

---

## Files that change

```
Cargo.toml                                  — [modify] [workspace.dependencies] notify = "8.2.1", with its reason
crates/buildl/Cargo.toml                    — [modify] [dependencies] notify = { workspace = true }, with its reason
deny.toml                                   — [modify] [licenses] exception for notify; [bans] walkdir wrappers gain notify; a notify ban confined to buildl
crates/expected-edges.txt                   — [modify] `# buildl` section gains notify (guard-crate-edges golden file)
crates/buildl/src/watch/mod.rs              — [create] module docs and re-export of NotifyChanges only
crates/buildl/src/watch/notify_changes.rs   — [create] NotifyChanges: start, Changes::next; unit tests on temp dirs
crates/buildl/src/lib.rs                    — [modify] `pub mod watch;`
Cargo.lock                                  — [modify] by cargo
```

---

### Task 1 — The dependency and its license

**Files:**
- Modify `Cargo.toml`, `crates/buildl/Cargo.toml`, `deny.toml`, `crates/expected-edges.txt`, `Cargo.lock`

**Change:** add `notify = "8.2.1"` to `[workspace.dependencies]` with a comment in the house
form (`Cargo.toml:20-26` shows it): what it is for (the file events of `buildl watch`), that
it is named by the `watch` adapter module only, and its default features (FSEvents on macOS,
inotify on Linux). Add it to `crates/buildl/Cargo.toml` `[dependencies]` with a one-line
reason, as the neighbours at `crates/buildl/Cargo.toml:12-40` have. In `deny.toml`:
- `[licenses]`: replace `exceptions = []` with an exception for the crate `notify` allowing
  `CC0-1.0`, commented: `notify` alone carries it (P3), CC0 is a public-domain dedication,
  and it stays off the general `allow` list so no other crate arrives under it.
- `[bans]`: `notify` depends on `walkdir` directly
  (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/notify-8.2.1/Cargo.toml:77`), which
  `deny.toml` bans outside `wrappers = ["airsl", "buildl-lua"]`. Add `notify` to that
  `wrappers` list and extend its reason: notify walks a directory to watch it, inside the
  `watch` adapter of buildl; buildl-core still traverses nothing
  (`docs/architecture-building-blocks.md:35` is unchanged, since it governs `buildl-core`).
- `[bans]`: add `{ crate = "notify", wrappers = ["buildl"], reason = … }`, the house way to
  confine a dependency to one crate, as `rayon`, `tonic`, `prost` and `uuid` are.
- `[bans] multiple-versions = "deny"`: if `cargo deny check bans` reports a second version of
  a crate `notify` brings (for example `windows-sys`, `bitflags`, `mio`), add a `skip` entry
  for the older version with its reason, in the form of the three entries already there.

In `crates/expected-edges.txt`, add `notify` to the `# buildl` section, in that section's
order, so `cargo make guard-crate-edges` (part of `cargo make dod`, `Makefile.toml:62`) passes.

**Proof:** `cargo deny check` passes (licenses, bans, advisories, sources);
`cargo make guard-crate-edges` passes; `cargo tree -p buildl -e normal -i notify` shows it
under `buildl` only.

**Done when:** `cargo deny check` and the stack's Definition of Done are green. Commit
`build(buildl): add notify for file events`.

### Task 2 — `NotifyChanges`

**Files:**
- Create `crates/buildl/src/watch/mod.rs`, `crates/buildl/src/watch/notify_changes.rs`
- Modify `crates/buildl/src/lib.rs`

**Change:** `NotifyChanges::start(root: &Path) -> Result<Self>`: canonicalize `root`, create
the watcher with an `mpsc::Sender` as its handler, `watch(root, RecursiveMode::Recursive)`;
each failure is `Error::Adapter { port: PortName::Changes, … }` naming the root. It keeps the
watcher alive for as long as it lives. `impl Changes for NotifyChanges`: wait with `recv` for
an event; an `Err(notify::Error)` or a closed channel is the same adapter error. For each
event path, a private pure function `relative(root: &Path, path: &Path) -> Option<ChangedPath>`
strips the canonical root, joins the components with `/` (lossy UTF-8), and parses a
`ChangedPath`; a path outside the root, equal to it, or refused is `None` and dropped. Once a path is
kept, keep receiving with `recv_timeout(WINDOW)`, `WINDOW` = 50 ms (design §8.6), until a
timeout; return `ChangeBatch::new(kept)`. A batch empty after dropping keeps waiting.
`watch/mod.rs` holds module docs and the re-export only.

**Proof:** written first, seen failing, then passing — the `relative` tests are pure; the
others run on a `tempfile` workspace, and every call to `next` runs on a helper thread whose
result the test receives with `recv_timeout` (a generous bound), so a dropped or late event
fails the test instead of hanging it; no assertion depends on a fixed sleep being long enough:
- `relative_strips_the_root` — `/r/lib/a.c` under root `/r` is `lib/a.c`
- `relative_drops_a_path_outside_the_root` — `/elsewhere/a.c` under `/r` is `None`
- `relative_drops_the_root_itself` — `/r` under `/r` is `None`
- `a_write_yields_its_relative_path` — write `a.c` after `start`; `next` holds `a.c`
- `a_nested_write_is_slash_separated` — `lib/words/a.txt`
- `a_burst_is_one_batch` — five writes in a row; one `next` holds all five paths
- `writes_while_nobody_asks_drain_into_one_batch` — write three files, then call `next` once: one batch with the three
- `a_root_given_through_a_link_still_relativizes` — start on a symlink to the temp dir (on macOS `std::env::temp_dir()` is under `/var`, a link to `/private/var`, P4); paths come back relative
- `out_and_state_writes_are_still_reported` — writes under `out/` and `.buildl/` reach the batch (dropping them is `react`'s)
- `start_on_a_missing_root_is_a_changes_error` — `Error::Adapter` naming `PortName::Changes`

**Done when:** `cargo test -p buildl -- watch::` passes on macOS and in CI on Linux (P15),
`cargo make guard-adapter-isolation` passes, and the stack's Definition of Done is green.
Commit `feat(buildl): watch a workspace's files through notify`.

---

## Risks

- Linux behaviour was not probed (spec §1, "not verified"); CI's `ubuntu-latest` job is the
  proof. If inotify reports a path differently, Task 2's tests fail there, not silently.
- A large tree on Linux can exceed `fs.inotify.max_user_watches`; that surfaces from `start`
  as the Changes error (spec §10). No mitigation here.
- `notify` delivering an event for the root itself (seen on macOS when the root was removed,
  in the spec's probe) is dropped as "equal to the root".

## Proof

`cargo deny check` and `cargo make dod` green, with `guard-adapter-isolation` and
`guard-crate-edges`; `cargo test -p buildl -- watch::` covers spec §8's adapter row.

## Review findings

- correctness — a rescan event (inotify queue overflow, FSEvents MustScanSubDirs; notify 8.2.1 `inotify.rs:212-213`, `fsevent.rs:115-116`, `EventKind::Other` + `Flag::Rescan`, no paths) was dropped silently, so changes the system lost were never reported — `crates/buildl/src/watch/notify_changes.rs`. Fixed and verified: `next` returns `Error::Adapter { port: PortName::Changes, … }` ("the operating system dropped file events under <root>; changes may have been missed"); `gather_fails_when_the_system_dropped_events` failed before the fix (closed-channel error instead), passes after; `cargo make dod` green (fix coder, run once, workspace-wide).
- unit-test-mandate — "a batch empty after dropping keeps waiting", "a notify `Err` is the Changes error" and "a closed channel is the Changes error" had no test — `notify_changes.rs`. Fixed and verified: gathering moved into private `gather(root, &Receiver<Delivery>)`, fed by hand-made channels in `gather_keeps_waiting_after_a_batch_that_drops_to_nothing`, `gather_collects_every_event_of_a_burst`, `gather_fails_on_a_watcher_error`, `gather_fails_when_the_channel_is_closed`; same `cargo make dod` run, green.
- spec §8 adapter row — `a_burst_is_one_batch` wrote before `next` started, duplicating `writes_while_nobody_asks_drain_into_one_batch` — `notify_changes.rs`. Fixed: `next` starts on the helper thread first, then the five writes; same gate run, green.
- doc-comment-discipline — docs named only "a path outside the root" as dropped — `notify_changes.rs`, `watch/mod.rs`. Fixed: they name the root itself and an invalid changed path too, and the rescan error.

## Probe results

- notify 8.2.1 version, MSRV 1.85, CC0-1.0 licence and the `deny.toml` exception were proven at spec time (spec P1–P3) and re-shown by Task 1's gate: `cargo deny check` — advisories, bans, licenses, sources ok, no `skip` needed; `cargo tree -p buildl -e normal -i notify` — `notify v8.2.1` under `buildl` only; `cargo make guard-crate-edges` ok.
- `Cargo.lock` additions (reviewer read): inotify, inotify-sys, kqueue, kqueue-sys, notify 8.2.1, notify-types, objc2-core-foundation, objc2-core-services; the rest of notify's tree was already present.
- Task 2's file-system tests ran red (7 of 10 against a stub) then green on macOS; Linux is proven by CI (risk 1, still to run on push).

## Deviations

- 2026-10-26 — Rescan events are treated as a watcher failure (exit 3 through the Changes error) rather than dropped. The plan and spec §5 did not name them; spec §6 makes a watcher failure the exit, and dropping them would miss changes silently. Decided by the orchestrator, no spec text contradicted.
- 2026-10-26 — Gathering lives in a private `gather` over the channel (with free functions `take` and `closed`) so its error and empty-batch paths are unit-tested without the file system; `Changes::next` calls it.
- 2026-10-26 — File-system tests assert `contains` (extra paths allowed), since FSEvents may report a directory beside the written file.
