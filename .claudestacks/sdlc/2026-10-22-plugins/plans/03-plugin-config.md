---
status: done
created: 2026-10-22
depends-on: [01]
built-on: 5fa60e343706
---

# Plugin Configuration Implementation Plan

**Goal:** Every command that loads build files reads `[plugins]` and `buildl.lock` and finds each named plugin checked on disk, or refuses before any build file runs.

**Architecture:** The values that describe a plugin's source — `PluginSource`, `Rev`, `GitUrl`,
`TagName`, `CommitId` — are domain values in `buildl-core`, because two adapter modules of
`buildl` share them (the manifest reader produces them, the new `plugins` module consumes them)
and adapter modules may not import each other (`Makefile.toml:209`, `guard-adapter-isolation`;
shared code lives in `buildl-core`, `state` or `search_path`). The manifest adapter reads
`[plugins.<name>]` strictly: caps into `Workspace::plugins` (plan 01), sources returned by a
`TomlManifest` method as `entry` and `declaration` are today (`toml_manifest.rs:100,106`). A new
adapter module `crates/buildl/src/plugins/` reads `buildl.lock` through the canonical reader
(`json/canonical.rs:129`), digests a store directory as an `OutputManifest::Tree` (spec §2, P15),
and returns the checked directories. The composition root runs the check once per command;
handing the directories to `LuaSource` is plan 04.

**Tech Stack:** Rust, `buildl`, `buildl-core`, `toml` 0.8 (already a dependency), std `fs` for
the walk (no new dependency), `cargo make dod`, `cargo deny check`. Guideline:
`claudestacks-guideline-rust:rust-guidelines` (strong types; one canonical type per concept).

**Content authority:** spec §2, §7 (`[plugins]` invalid, `PluginNotFetched`, `PluginChanged`,
path directory missing, tree with a link), §8 (`buildl` row), §9 (`buildl` end-to-end rows for a
changed store and offline commands).

---

## Files that change

```
crates/buildl-core/src/types/plugin_source.rs — [create] PluginSource, Rev, GitUrl, TagName, CommitId
crates/buildl-core/src/types/mod.rs         — [modify] export them
crates/buildl-core/src/error.rs             — [modify] PluginNotFetched { plugin, reason }, PluginChanged { plugin, reason }, PluginTree { plugin, path }
crates/buildl-cli/src/exit.rs               — [modify] PluginNotFetched → 5, PluginChanged → 6, PluginTree → 5
crates/buildl/src/manifest/schema.rs        — [modify] Raw gains `plugins: BTreeMap<String, PluginTable>`; PluginTable, deny_unknown_fields
crates/buildl/src/manifest/interpret.rs     — [modify] `workspace` (:60) fills Workspace::plugins; new `plugins` fn returns the sources
crates/buildl/src/manifest/toml_manifest.rs — [modify] `TomlManifest::plugins()` (adapter method, like `entry`)
crates/buildl/src/plugins/mod.rs            — [create] export-only: Lock, LockEntry, Checked, check, tree_digest
crates/buildl/src/plugins/lock.rs           — [create] buildl.lock read/write
crates/buildl/src/plugins/tree_digest.rs    — [create] digest a directory as an OutputManifest::Tree
crates/buildl/src/plugins/check.rs          — [create] spec §2's table
crates/buildl/src/lib.rs                    — [modify] `pub mod plugins`
crates/buildl/src/local_ports.rs            — [modify] `Wiring::source` (:157) becomes fallible and runs `check`; caller at :395
crates/buildl/src/workspace.rs              — [modify] callers of `Wiring::source` (:178, :193, :209, :228, :435) propagate the error
crates/buildl/tests/e2e/plugins.rs          — [create] store checks through Workspace
crates/buildl/tests/e2e/main.rs             — [modify] `mod plugins;`
```

## Order of work

### Task 1 — The source values and `[plugins.<name>]`

**Files:** `buildl-core/src/types/plugin_source.rs`, `types/mod.rs`, `manifest/schema.rs`,
`manifest/interpret.rs`, `manifest/toml_manifest.rs`.

**Change:** core values, each parsed at construction —
- `CommitId` — exactly 40 lowercase hex characters;
- `TagName` — non-empty, no whitespace or control characters, no `..`, not starting with `-`
  or `/`, not ending in `/` or `.lock` (the parts of git's ref-name rules a manifest can get
  wrong; git refuses the rest when fetching);
- `GitUrl` — non-empty, no whitespace or control characters; `GitUrl::github("o/r")` builds
  `https://github.com/o/r` and refuses anything not `owner/repo`;
- `Rev` — `Tag(TagName)` or `Commit(CommitId)`: a 40-hex value is a commit, anything else a tag;
- `PluginSource` — `Path(Directory)` or `Git { url: GitUrl, rev: Rev }`.

The manifest reads `[plugins.<name>]` per spec §2's key table: name → `PluginName`; exactly one
of `path`, `git`, `github`; `rev` required with `git`/`github`, refused with `path`; `path` not
`out`/`.buildl` nor inside or containing them; `proc.run`/`env.read` as `Program`/`EnvName`;
`network` → `PluginNetwork`. Unknown keys and grammar errors are refused with line and column
(P12). Caps fill `Workspace::plugins`; `TomlManifest::plugins()` returns
`BTreeMap<PluginName, PluginSource>`.

**Proof:** written first, seen failing, then passing —
- core units `commit_id_accepts_40_hex_only`, `tag_name_refuses_dotdot_and_leading_dash`,
  `github_short_form_expands`, `forty_hex_is_a_commit_else_a_tag`
- manifest units `a_path_plugin_is_read`, `a_git_plugin_is_read`, `github_expands_to_a_url`,
  `two_sources_are_refused`, `no_source_is_refused`, `rev_with_path_is_refused`,
  `git_without_rev_is_refused`, `path_inside_out_is_refused`, `an_unknown_plugin_key_names_its_line`,
  `caps_reach_workspace_plugins`

A tag name and a branch name look alike; "tags only" is enforced by `fetch`, which asks git for
`refs/tags/<tag>` (plan 05).

**Done when:** `cargo test -p buildl-core -p buildl` pass; `cargo make dod` green. Commit
`feat(buildl): read [plugins] tables from buildl.toml`.

### Task 2 — `buildl.lock` and the tree digest

**Files:** `plugins/mod.rs`, `plugins/lock.rs`, `plugins/tree_digest.rs`, `lib.rs`, `error.rs`,
`exit.rs`.

**Change:** `Lock` reads `<root>/buildl.lock` (absent → empty) through `canonical::from_slice`:
per plugin name a `LockEntry { commit: CommitId, git: GitUrl, rev: Rev, sha256: Digest }`,
serialized with `rev` as written; it writes through `canonical::to_vec` and `state/write.rs`
(temp-file-then-rename). A damaged lock is `Error::CorruptState` (exit 6), as a damaged state
file is. `tree_digest(plugin, dir)` walks with `std::fs` in sorted order, skips `.git/` at the top
level, refuses a symbolic link or special file with `Error::PluginTree { plugin, path }`
(exit 5), records each regular file's `Digest::of(bytes)` and executable bit as a `TreeEntry`,
builds `Tree::new` (`types/tree.rs:64`) and returns `Digest::of(canonical::to_vec(&OutputManifest::Tree(..)))`.

**Proof:** written first, seen failing, then passing —
- `lock` units `a_lock_round_trips_canonically`, `a_missing_lock_is_empty`,
  `a_damaged_lock_is_corrupt_state`
- `tree_digest` units `same_files_same_digest`, `an_edited_file_changes_the_digest`,
  `the_executable_bit_changes_the_digest`, `dot_git_is_ignored`, `a_symlink_is_refused_as_plugin_tree`
- `exit.rs` unit `plugin_tree_exits_5`

**Done when:** `cargo test -p buildl plugins` passes; `cargo make dod` and
`cargo make guard-adapter-isolation` green. Commit `feat(buildl): buildl.lock and plugin tree digests`.

### Task 3 — Check every plugin once per command

**Files:** `plugins/check.rs`, `local_ports.rs`, `workspace.rs`, `error.rs`, `exit.rs`,
`tests/e2e/plugins.rs`, `tests/e2e/main.rs`.

**Change:** `check(state, sources, lock) -> Result<Checked>` (`Checked`: name → absolute
directory) per spec §2: a path plugin's directory must exist (else `PluginNotFetched` naming the
path); a git plugin needs a lock entry whose `git` and `rev` equal the manifest's (else
`PluginNotFetched` naming `buildl plugins fetch`), its store dir
`<state>/plugins/<name>/<commit>/` (else `PluginNotFetched`), and a tree digest equal to
`sha256`; a different digest, or a store that now holds a link or special file, is
`PluginChanged` (exit 6): the store was changed after it was fetched. `Wiring::source` becomes
`-> Result<LuaSource>`: it runs `check` once and keeps `Checked` for plan 04; its callers
(`workspace.rs:178,193,209,228,435`, `local_ports.rs:395`) propagate the error. Commands that
load build files (`check`, `graph`, `query`, `doctor`, `plan`, `build`, `test`, `verify`, `gc`)
therefore check; `log` and `clean` do not call `source`.

**Proof:** written first, seen failing, then passing (e2e through `Workspace`) —
- `a_workspace_without_plugins_is_unchanged` — the `chain` fixture still builds
- `a_missing_path_plugin_refuses_check`
- `an_unfetched_git_plugin_refuses_build_naming_fetch`
- `a_stale_lock_entry_refuses` — `rev` in `buildl.toml` differs from the lock
- `an_edited_store_refuses_as_changed`, `a_link_added_to_the_store_refuses_as_changed`
- `log_and_clean_do_not_check_plugins`
- `exit.rs` unit `plugin_not_fetched_exits_5_and_changed_exits_6`

**Done when:** `cargo test --workspace` passes; `cargo make dod` green. Commit
`feat(buildl): check named plugins before loading build files`.

## Risks

- Most risky: Task 3's signature change to `Wiring::source`; every caller is listed and the
  compiler finds a missed one.
- `.git` is skipped at the top level only; a nested `.git` directory in a plugin is digested.
- Branch names cannot be told from tags by the manifest; `fetch` enforces tags (plan 05).

## Proof

`cargo make dod`, `cargo deny check` and `cargo make guard-adapter-isolation` green; a workspace
naming an unfetched git plugin is refused by `check` with exit 5; an edited store with exit 6.

## Review findings

Reviewer verdict: no blocking finding. All fixed in one round, verified `cargo make dod` → `Build Done in 25.13 seconds` (p03) and, after merging into the plugins branch, `Build Done in 15.40 seconds`, exit 0.

- correctness — a failed `buildl.lock` write exited 5 as a manifest error; spec §7 gives a store failure exit 3 — `plugins/lock.rs` — fixed: `PortName::PluginStore` (infrastructure, exit 3) for writes; reads stay `Manifest`; test `a_lock_that_cannot_be_written_is_a_plugin_store_failure`
- doc — `plugins/mod.rs` claimed it names no other module; it uses `state` — fixed
- test-coverage — refusing a non-Unicode file name was untested — added `#[cfg(target_os = "linux")]` test `a_name_that_is_not_unicode_is_refused_as_plugin_tree` (runs on CI Linux only; unexecuted on macOS)
- security — a `git` URL could name a transport helper (`ext::…`) — fixed: `GitUrl` refuses a `<word>::` prefix; `ssh://[::1]/repo` and `https://host/a::b` still accepted
- format — doc lines re-wrapped in `tree_digest.rs`
- duplication — the `0o111` executable rule lived in two places — `FileMode::of_unix_bits` shared by `tree_digest` and `exec/outputs.rs`
- consistency — tree-walk read failures named the content store — now `PluginStore` (exit 3 unchanged)
- strong-types — `plugin_directory` returned a `String` error — typed `PathFault`, text unchanged
- fixture — e2e `extension.toml` fixture made a valid manifest shape
- lint — `strip_suffix(".lock").is_some()` kept: clippy `case_sensitive_file_extension_comparisons` refuses `ends_with`
- no change — `[workspace]` read before `[plugins]` (plugin paths need the out/state paths); a damaged committed lock failing every loading command (exit 6) is intended; documented order of checks needs no test
- spec/plan — deviations below recorded here

## Probe results

- Structural: `git grep -n -e "pub fn from_slice" -e "pub fn to_vec" -e "fn source(" -e "guard-adapter-isolation" -- crates Makefile.toml` → `json/canonical.rs:129`, `json/canonical.rs:74`, `local_ports.rs:157`, `Makefile.toml:209`. No behavioural premise outside the plan's own tests.

## Deviations

- 2026-10-23 — `check(root, state, sources, lock)`: takes the workspace root first, since a path plugin is relative to it and `StateLayout` does not hold it.
- 2026-10-23 — `Wiring::source` runs `check` and drops `Checked` (it has nowhere to keep it yet); plan 04 Task 4 hands its directories to `LuaSource::new`. Until then every loading command digests each git store once and discards the result.
- 2026-10-23 — `CommitId`, `TagName`, `GitUrl`, `Rev` serialize as strings (serde in core) so `LockEntry` can derive serde.
- 2026-10-23 — `GitUrl` also refuses a leading `-` (git would read it as an option) and a `<word>::` transport prefix.
- 2026-10-23 — a plugin `path` is refused when it is, holds or lies inside the configured output or state directory (not the literal `out`).
- 2026-10-23 — a store directory with no files, or one now holding a link or special file, is `PluginChanged`.
- 2026-10-23 — executed in a second worktree (`worktree-m5-plugins-03`) beside plan 02, merged into the plugins branch; one export-list conflict in `buildl-core/src/lib.rs`.
