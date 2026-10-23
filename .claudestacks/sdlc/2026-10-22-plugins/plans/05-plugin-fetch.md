---
status: approved
created: 2026-10-22
depends-on: [03, 04]
built-on: 0f868841fc48
---

# Plugin Fetch Implementation Plan

**Goal:** `buildl plugins fetch` pins every git plugin in `buildl.lock` and stores its files under `.buildl/plugins/`.

**Architecture:** The fetch lives in the `plugins` adapter module plan 03 created, beside
`lock`, `tree_digest` and `check`, so it shares them without importing another adapter module.
It runs the system `git` found through the shared `search_path` module (no new dependency,
spec §3, P16), under the state lock (`state/lock.rs:34`). `Workspace` gains `fetch_plugins`; the
CLI gains a nested `plugins fetch` subcommand (`cli.rs:54` has no nested subcommand yet; clap's
`#[command(subcommand)]` on a variant is the form). It loads no build file and asks no approver.

**Tech Stack:** Rust, `buildl`, `buildl-core`, `buildl-cli` (clap derive), git ≥ 2.39 on
`PATH`, `cargo make dod`, `cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §3, §2 (`buildl.lock`), §7 (git failure, moved tag, tree with a link),
§9 (fetch from a local `file://` repository; a tag, a commit, a moved tag refused), P22.

---

## Files that change

```
crates/buildl/src/plugins/git.rs        — [create] run git: init, fetch one ref at depth 1, checkout, rev-parse; stderr captured
crates/buildl/src/plugins/fetch.rs      — [create] `fetch(root, state, sources, git) -> Result<Fetched>` per spec §3
crates/buildl/src/plugins/mod.rs        — [modify] export fetch
crates/buildl/src/workspace.rs          — [modify] `Workspace::fetch_plugins` (takes the state lock)
crates/buildl-core/src/error.rs         — [modify] PluginFetch { plugin, diagnostic } (exit 3); PluginTree and PluginChanged are plan 03's
crates/buildl-cli/src/cli.rs            — [modify] `Command::Plugins(PluginsArgs)` with subcommand `Fetch`
crates/buildl-cli/src/run.rs            — [modify] dispatch `plugins fetch`; print one line per plugin fetched or unchanged
crates/buildl-cli/src/exit.rs           — [modify] PluginFetch → 3 (PluginTree → 5 and PluginChanged → 6 are plan 03's)
crates/buildl/tests/e2e/fetch.rs        — [create] fetch from local file:// repositories the test creates
crates/buildl/tests/e2e/main.rs         — [modify] `mod fetch;`
crates/buildl-cli/tests/cli/exit_codes.rs — [modify] fetch exit codes through the binary
```

## Order of work

### Task 1 — Running git

**Files:** `plugins/git.rs`, `plugins/mod.rs`.

**Change:** a `Git` value holding the program path (found through `search_path`; not found →
`PluginFetch` naming `git`). Operations, each one process with captured stderr and a non-zero
exit turned into `PluginFetch { plugin, diagnostic: <stderr> }`:
`init(dir)`; `fetch_rev(dir, url: &GitUrl, rev: &Rev)` running `git fetch --depth 1 <url> <refspec>`
where the refspec is `refs/tags/<tag>` for `Rev::Tag` and the commit for `Rev::Commit` (P22: both
work over `file://`; a missing tag fails with `fatal: couldn't find remote ref refs/tags/<tag>`); `checkout_fetched(dir)` running `git -c advice.detachedHead=false checkout -q FETCH_HEAD`;
`head_commit(dir)` running `git rev-parse HEAD` parsed as `CommitId`.

**Proof:** written first, seen failing, then passing (unit, a `file://` repo built in a temp dir by
the test with `git init`/`commit`/`tag`) —
- `fetching_a_tag_checks_out_its_commit`
- `fetching_a_commit_checks_it_out`
- `a_branch_name_is_not_found_as_a_tag` — repo has branch `main`, no tag `main` → `PluginFetch`
- `an_annotated_tag_resolves_to_its_commit`
- `git_missing_from_path_is_a_fetch_error`

**Done when:** `cargo test -p buildl plugins::git` passes; `cargo make dod` green. Commit
`feat(buildl): run git to fetch one plugin revision`.

### Task 2 — `fetch`: pin, store, lock, prune

**Files:** `plugins/fetch.rs`, `plugins/mod.rs`, `buildl-core/src/error.rs`.

**Change:** spec §3's loop, git plugins in name order: an entry matching (`git`, `rev`) whose
store dir digests to `sha256` → unchanged; otherwise `init` a temp dir under
`<state>/plugins/`, `fetch_rev`, `checkout_fetched`, `head_commit`, remove `.git/`, `tree_digest`
(plan 03; a link or special file → `PluginTree`, exit 5). A matching entry whose commit or digest differs →
`PluginChanged` (plan 03's error; exit 6), temp dir removed. Else rename to
`<state>/plugins/<name>/<commit>/` (replacing a partial dir) and record the entry. After all:
write `buildl.lock` only when an entry changed (canonical, temp-then-rename), and remove store
dirs under `<state>/plugins/` that no entry names. Path plugins are skipped.

**Proof:** written first, seen failing, then passing (e2e `tests/e2e/fetch.rs`) —
- `first_fetch_writes_the_lock_and_the_store`
- `a_second_fetch_changes_nothing` — lock bytes and mtime unchanged
- `a_moved_tag_is_refused_as_changed` — retag to a new commit in the source repo → exit-6 class
- `a_changed_rev_refetches_and_prunes_the_old_store`
- `a_link_in_the_plugin_is_refused`
- `the_lock_is_canonical_json` — keys sorted, one serializer
- `after_fetch_check_finds_the_plugin` — plan 03's `check` succeeds

**Done when:** `cargo test -p buildl --test e2e fetch` passes; `cargo make dod` and
`cargo make guard-adapter-isolation` green. Commit `feat(buildl): fetch git plugins into the store`.

### Task 3 — `Workspace::fetch_plugins` and `buildl plugins fetch`

**Files:** `workspace.rs`, `cli.rs`, `run.rs`, `exit.rs`, `tests/cli/exit_codes.rs`.

**Change:** `Workspace::fetch_plugins()` takes the state lock and calls `fetch`; it reads
`buildl.toml` for the sources and evaluates no build file. `buildl plugins fetch` prints, per git
plugin in name order, `fetched <name> <commit>` or `unchanged <name> <commit>` on stderr, exit 0.
`exit.rs`: `PluginFetch` → 3.

**Proof:** written first, seen failing, then passing —
- CLI `plugins_fetch_exits_0_and_reports_each_plugin`
- CLI `plugins_fetch_with_no_git_exits_3` — `PATH` without git
- CLI `build_after_fetch_succeeds_offline` — fetch, then the source repo is deleted, then `build`
- `exit.rs` unit `plugin_fetch_exits_3`

**Done when:** `cargo test --workspace` passes; `cargo make dod` and `cargo deny check` green.
Commit `feat(buildl-cli): buildl plugins fetch`.

## Risks

- A host that refuses to serve a bare commit (no `allowReachableSHA1InWant`) fails a
  commit-pinned `rev`; tags always work. Passed on as git's error (exit 3), spec §3.
- Tests need `git` on `PATH`; CI's `ubuntu-latest` and `macos-latest` have it (P16). A test that
  finds no git fails, never skips silently.
- The engineer's own git configuration applies (credentials, `url.*.insteadOf`); hooks of the
  plugin repository are never fetched, so none of its code runs during `fetch`.
- Most risky: Task 2's rename over a partial store dir after an interrupted run.

## Proof

`cargo make dod`, `cargo deny check` green; `buildl plugins fetch` against a local repo writes a
canonical lock and store, a second run changes nothing, and `build` then works with the source
repository gone.
