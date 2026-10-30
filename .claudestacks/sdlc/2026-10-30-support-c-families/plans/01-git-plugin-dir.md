---
status: executing
created: 2026-10-30
built-on: edb4e55efd6a
---

# Git Plugin Subfolder Implementation Plan

**Goal:** a git plugin can be fetched from a subfolder of its repository, named by an optional `dir`
key, as spec §2 defines.

**Architecture:** `dir` is a `Directory` carried on `PluginSource::Git` and on every lock row; the
manifest reads it, lock matching compares it, and `bring_in` keeps only that folder of the fetched
tree, after checking that no segment of it is a link. `check`, the Lua loader and `require` stay as
they are, because the stored folder is the plugin (spec P9).

**Tech Stack:** Rust 1.94 workspace (`buildl-core`, `buildl`, `buildl-cli`), serde, the canonical
JSON serializer, `git` via `crates/buildl/src/plugins/git.rs`; mdBook for the book.

---

## Files that change

```
crates/buildl-core/src/types/directory.rs         — [modify] doc comment: a relative folder; the holder says relative to what
crates/buildl-core/src/types/plugin_source.rs     — [modify] PluginSource::Git gains `dir: Directory`
crates/buildl/src/manifest/schema.rs               — [modify] PluginTable gains `dir`
crates/buildl/src/manifest/interpret.rs            — [modify] read `dir` with git/github; refuse it beside `path`
crates/buildl/src/manifest/toml_manifest.rs        — [modify] tests for `dir`; existing Git constructions take `dir`
crates/buildl/src/plugins/lock.rs                  — [modify] LockEntry gains `dir`, always written
crates/buildl/src/plugins/fetch.rs                 — [modify] match on dir; bring_in keeps only <dir>
crates/buildl/src/plugins/check.rs                 — [modify] match on dir; test fixtures take dir
crates/buildl/tests/e2e/fetch.rs                   — [modify] subfolder fetch tests; Source helper commits into a folder
crates/buildl/tests/e2e/plugins.rs                 — [modify] the `fetched` helper's LockEntry takes `dir`
crates/buildl-cli/tests/cli/exit_codes.rs          — [modify] a subfolder plugin fetches, loads and builds offline
book/src/ch08-02-plugins-from-git.md               — [modify] `dir` in the key table and the lock table
book/src/appendix-02-buildl-toml.md                — [modify] `dir` in the [plugins.<name>] table and its example
skills/buildl/references/buildl-toml.md            — [modify] `dir` row in the [plugins.<name>] key table
skills/buildl/references/plugins.md                — [modify] `dir` in *Plugins from git*
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| `PluginSource::Git { url, rev }` is built at interpret.rs:197, matched at check.rs:75 and fetch.rs:96, and constructed in tests at toml_manifest.rs:424, :431, :445, check.rs:183, :268, tests/e2e/fetch.rs:91; `LockEntry { … }` literals are at fetch.rs:196, :357, check.rs:198, :345 and tests/e2e/plugins.rs:47 | spec P5; read at commit f0f427d |
| `bring_in` clones into staging, removes `.git`, digests, renames to the store; no-file commits raise `PluginNotFetched` | spec P6 (fetch.rs:154-168, :189-201) |
| `LockEntry` has `commit`, `git`, `rev`, `sha256`, `deny_unknown_fields` | spec P7 (lock.rs:24-37) |
| lock rows match on `git` and `rev` | spec P8 (fetch.rs:99-101, check.rs:76-78) |
| `Directory::parse` refuses `..`, leading `/`, empty segments; `""` is the root; `is_root` exists | spec P10 |
| a `path` plugin refuses `rev` with "a path plugin has no rev" | spec P11 (interpret.rs:148-155) |
| the e2e `Source` helper (tests/e2e/fetch.rs:25-82) makes a git repo, commits `lib/go.lua` and `extension.toml`, tags `v1.0.0`, and commits with a fixed identity | read at commit f0f427d |
| the CLI tests build a plugin repository with `plugin_repository` (exit_codes.rs:1064) and a workspace with `workspace_using` (:1125); `build_after_fetch_succeeds_offline` is at :1238; :1219 and :1234 compare the lock's bytes with themselves | read at commit f0f427d |
| `tests/e2e/plugins.rs`'s `fetched` helper (:35) writes the store and lock directly, without git | read at commit f0f427d |
| appendix-02's `[plugins.<name>]` table has a Line column naming `schema.rs` lines (:99-106), and :138 says "every key but the three that fetch a plugin from git" | read at commit f0f427d |

## Order of work

### Task 1 — Read `dir` from `buildl.toml`

**Files:**
- Modify `crates/buildl-core/src/types/directory.rs` — widen the type's doc comment as spec §2.1 says
- Modify `crates/buildl-core/src/types/plugin_source.rs` — `PluginSource::Git` gains `dir: Directory`, documented as the repository folder holding the plugin, the root when absent
- Modify `crates/buildl/src/manifest/schema.rs` — `PluginTable.dir: Option<Spanned<String>>`, documented
- Modify `crates/buildl/src/manifest/interpret.rs` — in `source`, the path arm refuses `dir` with "a path plugin has no dir" (the same form as the `rev` refusal); `pinned` parses `dir` with `Directory::parse`, refusing a bad value with the field `dir` named and its place, and defaults to `Directory::root()`
- Modify `crates/buildl/src/manifest/toml_manifest.rs`, `crates/buildl/src/plugins/check.rs`, `crates/buildl/tests/e2e/fetch.rs` — every existing `PluginSource::Git` construction passes `dir: Directory::root()`

**Change:** `buildl.toml` accepts `dir` beside `git`/`github`; nothing reads it yet beyond the manifest.

**Proof:** written first, seen failing, then passing — in `toml_manifest.rs` tests:
- `a_git_plugin_with_dir_is_read` — `git` + `rev` + `dir = "plugins/c"` gives `PluginSource::Git` with that `dir`
- `a_github_plugin_with_dir_is_read` — the same with `github`
- `a_git_plugin_without_dir_is_at_the_root` — no `dir` gives `Directory::root()`
- `dir_with_path_is_refused` — `path` + `dir` is refused naming `dir` and "a path plugin has no dir"
- `a_dir_that_climbs_is_refused` — `dir = "a/../b"` is refused naming `dir`
- `a_dir_with_a_leading_slash_is_refused` — `dir = "/plugins/c"` is refused naming `dir`

**Done when:** `cargo make dod` and `cargo deny check` are green. Commit
`feat(buildl): read a git plugin's dir from buildl.toml`.

### Task 2 — Record `dir` in the lock and match on it

**Files:**
- Modify `crates/buildl/src/plugins/lock.rs` — `LockEntry.dir: Directory`, documented, always serialized (`""` for the root), no serde default
- Modify `crates/buildl/src/plugins/fetch.rs` — the `pinned` filter (:99-101) also requires `entry.dir == *dir`; `bring_in` writes `dir` into the entry it returns (the subfolder step itself is Task 3)
- Modify `crates/buildl/src/plugins/check.rs` — the lock filter (:76-78) also requires `dir`; fixtures build entries with `dir`
- Modify tests that assert lock bytes: `lock.rs` tests (:127-155) and `tests/e2e/fetch.rs` `the_lock_is_canonical_json` (:375); exit_codes.rs:1219 and :1234 compare the lock with itself and need no change
- Modify `crates/buildl/tests/e2e/plugins.rs` — the `fetched` helper's `LockEntry` (:47) takes `dir: Directory::root()`

**Change:** every lock row holds `dir`; a source whose `dir` differs from the row's reads as not pinned.

**Proof:** written first, seen failing, then passing —
- `lock.rs`: `a_root_row_writes_an_empty_dir` — a row with the root `dir` serializes with `"dir":""`
- `lock.rs`: `a_row_with_dir_round_trips` — write then read gives the same entry with `dir = "plugins/c"`
- `lock.rs`: `a_row_without_dir_is_refused` — reading a row lacking `dir` is an error from `Lock::read`
- `check.rs`: `a_row_whose_dir_differs_is_not_pinned` — a lock row for `plugins/a` does not satisfy a source naming `plugins/b`; the error is the "does not pin … run `buildl plugins fetch`" one
- `fetch.rs` (unit tests, from :294): `a_row_whose_dir_differs_is_not_reused` — `fetch` given a lock row whose `dir` differs from the source's does not report the plugin `Unchanged`
- `tests/e2e/fetch.rs`: `the_lock_is_canonical_json` updated to the bytes with `dir`

**Done when:** `cargo make dod` and `cargo deny check` are green. Commit
`feat(buildl): record a git plugin's dir in buildl.lock`.

### Task 3 — Fetch keeps only the plugin's folder

**Files:**
- Modify `crates/buildl/src/plugins/fetch.rs` — `bring_in` gains the step of spec §2.1: when `dir` is not the root, walk its segments from the staging folder with `symlink_metadata`, each a folder and not a link, else `PluginNotFetched` with "`<rev>` fetched as `<commit>`, which has no folder `<dir>`"; digest `<staging>/<dir>`; rename it into the store; remove what is left of the staging folder. A helper for the segment walk lives beside `bring_in`, named for what it answers (e.g. `plugin_folder`)
- Modify `crates/buildl/tests/e2e/fetch.rs` — the `Source` helper gains a constructor that commits the plugin (`extension.toml`, `lib/go.lua`) under a given folder and one other file outside it, and a way to commit a symbolic link
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs` — a subfolder variant of `plugin_repository`

**Change:** a plugin fetched with `dir` is stored and digested as that folder alone; a missing folder or a link anywhere along `dir` is refused; a changed `dir` re-fetches.

**Proof:** written first, seen failing, then passing — in `tests/e2e/fetch.rs`:
- `a_subfolder_plugin_is_stored_without_the_rest_of_the_repository` — the store folder holds `extension.toml` and `lib/go.lua` and not the outside file
- `a_subfolder_plugin_is_digested_alone` — the lock's `sha256` equals `tree_digest` of the stored folder
- `a_missing_folder_is_refused_naming_it` — `dir = "nope"` gives `PluginNotFetched` naming `nope`; the store holds nothing for the plugin
- `a_folder_that_is_a_link_is_refused` — `dir` naming a committed link to a folder is refused
- `a_folder_under_a_linked_parent_is_refused` — `dir = "plugins/c"` where `plugins` is a committed link is refused, and nothing outside the staging folder is moved
- `a_changed_dir_refetches` — fetch with `dir = "a"`, then with `dir = "b"`, re-fetches and the lock row holds `b`

and in `crates/buildl-cli/tests/cli/exit_codes.rs`:
- `a_plugin_fetched_from_a_subfolder_builds_offline` — `buildl plugins fetch` exits 0, then `buildl build` with no `git` on `PATH` exits 0, the plugin's entry `require`-ing a sibling module

Spec §10 places the "loads and `require`s its siblings" test in `tests/e2e/plugins.rs`; it lives in
exit_codes.rs instead, because plugins.rs's `fetched` helper writes the store without git and so
cannot exercise a subfolder fetch, while the CLI test runs fetch and load together.

**Done when:** `cargo make dod` and `cargo deny check` are green. Commit
`feat(buildl): fetch a git plugin from a subfolder`.

### Task 4 — Document `dir`

**Files:**
- Modify `book/src/ch08-02-plugins-from-git.md` — a `dir` row in the key table (:19-23) with a sentence that only that folder is stored and digested; a `dir` row in the lock table (:64-68); the sample lock line gains `"dir":""`
- Modify `book/src/appendix-02-buildl-toml.md` — a `dir` row in the `[plugins.<name>]` table (from :92) with its `schema.rs` line, every Line value after it renumbered to Task 1's `schema.rs`, :138's "the three that fetch a plugin from git" made four, and `dir` in one git example (:196-200)
- Modify `skills/buildl/references/buildl-toml.md` — a `dir` row in the key table (:112-120)
- Modify `skills/buildl/references/plugins.md` — `dir` in *Plugins from git* (:83-100)

**Change:** the book and the `buildl` skill describe `dir`.

**Proof:** `cargo make book-build` succeeds; `cargo make skills` passes (its snippet check runs every `lua` block of the skill files).

**Done when:** both commands pass and `cargo make dod` is green. Commit
`docs(repo): document a git plugin's dir`.

## Risks

- **Lock bytes change for every git plugin** (Task 2): every test that asserts lock text is updated in the same task; buildl is unpublished, so no older lock is read (intent constraint).
- **Task 3 is the risky step**: a mistaken rename could move files outside the staging folder. The segment walk runs before any digest or rename, and `a_folder_under_a_linked_parent_is_refused` asserts nothing outside moved.
- Not done here: `watch_layout` stays as it is (it watches `path` plugins only); no change to `check` or the loader.

## Proof

`cargo make dod`, `cargo deny check`, `cargo make book-build` and `cargo make skills` green on the
branch after Task 4.
