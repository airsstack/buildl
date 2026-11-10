---
status: done
created: 2026-11-10
depends-on: [01]
built-on: 6643cfeb82a2
---

# Ignore Rules Adapter Implementation Plan

**Goal:** `buildl` answers the `IgnoreRules` port from the workspace's `.buildlignore` files the way git answers from `.gitignore`.

**Architecture:** A new adapter module `crates/buildl/src/ignore_rules/` holding one public type,
`BuildlIgnore`, which implements `buildl_core::IgnoreRules`. It is a cheap-clone handle
(`Arc<Inner>`) so the composition root can give one instance to the walk and to the pipeline (plan
03, plan 05). `Inner` holds the canonical workspace root and a per-folder cache, behind a `Mutex`
for `Sync`, of that folder's parsed `.buildlignore`, absent or present. `BuildlIgnore` implements
`Debug` by hand (the root only) and `PartialEq`/`Eq` by comparing roots, so the composition's
existing `assert_eq!` on `Wiring::source()` (`crates/buildl/src/local_ports.rs:724-757`) keeps
working once `LuaSource` holds it (plan 05).

A folder's file is read once per instance (`FolderRules`): bytes checked as UTF-8, a leading BOM
dropped, each unescaped `{` and `}` escaped (spec §2.1), each line given to
`ignore::gitignore::GitignoreBuilder::add_line` with the file's path. `FolderRules` keeps, beside
the built `Gitignore`, a map from each glob's text as handed to the crate to the line as written,
so `Ignoring.pattern` and error text show what the author wrote, not the escaped form. The
decision walks the folders on the way to the path from the root down, then the path (spec §2.3),
never calling `matched_path_or_any_parents`. `Unknown` is resolved by looking at the path (spec
§3). The module imports no other adapter module of `buildl`.

**Tech Stack:** Rust 1.94, `ignore` 0.4.34 (new), `tempfile` for tests.

**Content authority:** `spec.md` §2 (all), §3 (`ignore_rules` adapter row), §5 (`IgnoreFile` rows), §6
(rows on empty files, re-include, files above the root, symlinked files, watch `Unknown` rows), §7.

---

## Files that change

```
Cargo.toml                                       — modify: ignore = "0.4" with its reason (workspace deps :22-108)
crates/buildl/Cargo.toml                         — modify: ignore = { workspace = true } (deps :13-45)
deny.toml                                        — modify: ignore in walkdir's and globset's wrappers (:102,:109); a ban entry for ignore, wrappers = ["buildl"]
crates/expected-edges.txt                        — modify: `ignore` in the `# buildl` section (:15-31)
Cargo.lock                                       — modify: the one new package
crates/buildl/src/lib.rs                         — modify: `pub mod ignore_rules;` among the adapter modules (:192-211)
crates/buildl/src/ignore_rules/mod.rs            — create: module docs and re-export only
crates/buildl/src/ignore_rules/folder_rules.rs   — create: reading and parsing one folder's .buildlignore
crates/buildl/src/ignore_rules/buildl_ignore.rs  — create: BuildlIgnore, the port implementation and the root-down decision
```

## Order of work

### Task 1 — Add the dependency

**Files:**
- Modify `Cargo.toml` — `ignore = "0.4"`, comment: `.gitignore`-syntax matching for `.buildlignore`,
  confined to the `ignore_rules` adapter of `buildl`
- Modify `crates/buildl/Cargo.toml` — the dependency
- Modify `deny.toml` — `ignore` added to the `wrappers` of `walkdir` and `globset`, each reason
  extended with "`ignore` matches `.buildlignore` patterns inside the ignore-rules adapter of
  buildl"; a new ban entry `{ crate = "ignore", wrappers = ["buildl"] }` whose reason confines it to
  that adapter
- Modify `crates/expected-edges.txt` — `ignore` in the `# buildl` section, where `cargo tree` lists it
- Modify `Cargo.lock`

**Change:** the crate is in the graph and the supply-chain policy allows it; Task 2 is its first use.

**Proof:** no test: a manifest change. Its check is the policy run and the edge guard.

**Done when:** `cargo deny check -D unused-wrapper` passes, `cargo make guard-crate-edges` passes,
and `cargo make dod` is green. Commit `build(buildl): add the ignore crate for .buildlignore`.

### Task 2 — Decide paths from .buildlignore files

**Files:**
- Modify `crates/buildl/src/lib.rs` — declare `ignore_rules`
- Create `crates/buildl/src/ignore_rules/mod.rs` — module docs (responsibilities, spec §2's rules in
  an engineer's terms), `pub use BuildlIgnore`
- Create `crates/buildl/src/ignore_rules/folder_rules.rs` — `FolderRules`, `Decision`
- Create `crates/buildl/src/ignore_rules/buildl_ignore.rs` — `BuildlIgnore`

**Change:**

`FolderRules::read(root, folder) -> Result<Option<FolderRules>>` reads `<folder>/.buildlignore`:
absent → `Ok(None)`; a symlink → `IgnoreFile { line: None, reason: "a symlink, not a file" }`;
unreadable (a folder by that name included) → `IgnoreFile { line: None }` with the I/O error's
text; bytes not UTF-8 → `IgnoreFile` with the first line holding invalid bytes and reason
`not UTF-8`. Otherwise a leading `\u{feff}` is dropped, unescaped braces are escaped by spec §2.1's
rule, and each line goes to `add_line` (builder rooted at the folder, `from` = the file's path); the
first `Err` becomes `IgnoreFile { file, line, reason }` whose reason is `error parsing glob
'<line as written>': <the crate error's err text>` (spec §5; for `[z-a]` this is the crate's own
display). `FolderRules::decide(relative, is_folder) -> Decision`, where `Decision` is `Ignored
(Ignoring)`, `Included` (a `!` line matched) or `NoMatch`; `relative` is the path relative to that
folder.

`BuildlIgnore::new(root)` over the canonical workspace root. `impl IgnoreRules`: for `path`, take
each folder on the way down (`notes`, then `notes/keep` for `notes/keep/b.md`), then the path itself.
For each, ask the `FolderRules` of the root and of every folder above the one asked, deepest first,
each with the path made relative to its own folder; the first `Ignored` or `Included` decides; an
`Ignored` answers `Some` and stops. A folder on the way is asked as a folder; the path as its
`kind`. `Unknown` is resolved with `symlink_metadata`: an existing folder as a folder, an existing
file or a missing path as a file. Files above the root are never read. Rules are cached per folder.

**Proof:**
- `a_missing_file_has_no_rules` — a workspace with no `.buildlignore` keeps every path asked.
- `an_empty_or_comment_only_file_ignores_nothing` — an empty file and one of a blank line and a `#`
  line keep every path.
- `a_bom_does_not_hide_the_first_pattern` — a root file `\u{feff}notes/` ignores the folder `notes`.
- `braces_are_literal` — a root file `{a` ignores the file `{a` and not `a`; `{a,b}` ignores the
  file `{a,b}` and not `a`; `\{a` ignores the file `{a`; `\\{a` ignores the file `\{a` (spec §2.1).
- `the_pattern_is_reported_as_written` — `{a` ignoring `{a` answers with pattern `{a`, not `\{a`.
- `a_bad_pattern_is_refused_with_its_line` — `[z-a]` on line 1 of `lib/.buildlignore` gives
  `IgnoreFile` with that file, line 1 and the spec §5 text.
- `non_utf8_text_is_refused_with_its_line` — a valid line 1 and invalid bytes on line 2 give line 2,
  `not UTF-8`.
- `a_symlinked_file_is_refused` — a `.buildlignore` linking to a real file gives `IgnoreFile`, no
  line, `a symlink, not a file`.
- `an_unreadable_file_is_refused` — a folder named `.buildlignore` gives `IgnoreFile`, no line.
- `case_matters` — `Notes/` does not ignore `notes`.
- `trailing_slash_matches_folders_only` — `docs/` ignores the folder `docs`, not the file `docs`.
- `a_root_pattern_ignores_at_any_depth` — root `docs/` ignores `lib/docs/d.md`, keeps `lib/docs.txt`.
- `a_leading_slash_anchors_to_the_file` — root `/a.log` ignores `a.log`, keeps `lib/a.log`.
- `a_nested_file_applies_below_its_folder` — `src/.buildlignore` `gen/*` and `!gen/keep.rs`:
  `src/gen/x.rs` ignored, `src/gen/keep.rs` kept, the root's `gen/x.rs` kept.
- `a_deeper_file_wins` — root `*.log`, `logs/.buildlignore` `!keep.log`: `logs/keep.log` kept,
  `logs/other.log` and `a.log` ignored.
- `no_reinclude_under_an_ignored_folder` — root `notes/` and `!notes/keep/`: `notes/keep/b.md` is
  ignored by `notes/` (spec §2.3 checked example).
- `a_self_ignoring_file_still_applies` — `sub/.buildlignore` `x.txt` and `.buildlignore`:
  `sub/.buildlignore` and `sub/x.txt` ignored, `sub/y.txt` kept.
- `other_ignore_files_are_never_read` — a `.gitignore` and a `.ignore` holding `src/` keep `src/a.c`.
- `a_file_above_the_root_is_never_read` — a `.buildlignore` ignoring everything in the parent of the
  root keeps every path.
- `unknown_on_an_existing_folder_is_a_folder` — `notes/` and an existing `notes`: ignored.
- `unknown_on_a_missing_path_is_a_file` — `notes/` and a missing `notes`: kept; `/a.log` and a
  missing `a.log`: ignored.
- `a_missing_file_under_an_ignored_folder_is_ignored` — `notes/` and a missing `notes/a.md` asked
  `Unknown`: ignored.
- `a_bad_file_on_the_way_is_an_error` — a bad `lib/.buildlignore` makes asking `lib/l.rs` return
  `IgnoreFile`.
- `each_file_is_read_once` — removing a `.buildlignore` after the first question leaves the second
  answer unchanged for that instance; a new instance sees the removal.
- `instances_over_one_root_are_equal` — two instances over the same root compare equal; their
  `Debug` shows the root.

**Done when:** `cargo test -p buildl --all-targets --all-features` passes, `cargo make
guard-adapter-isolation` passes, and `cargo make dod` and `cargo deny check -D unused-wrapper` are
green. Commit `feat(buildl): decide ignored paths from .buildlignore files`.

---

## Risks

- Each `Gitignore` is rooted at its folder; a path handed to it must be relative to that folder. The
  riskiest step is the prefix handling, so the tests ask paths at three depths and from rules in two
  folders.
- `symlink_metadata` on a path that changes between the question and the build is a race; the watch
  uses `Unknown` only to drop events, and the next cycle's load reads the workspace afresh.

## Proof

- `cargo make dod` and `cargo deny check -D unused-wrapper` green after each task.
- `cargo make guard-crate-edges` and `cargo make guard-adapter-isolation` pass.

## Review findings

One reviewer pass over 2465268..9c41107: 0 blocking. Fixed in 54a2079 (`cargo make dod` exit 0; `guard-adapter-isolation`, `cargo deny check -D unused-wrapper` green) unless declined.

- 🟡 no test pinned that a bad line holding braces is reported as written — fixed: `a_bad_line_with_braces_is_reported_as_written` (`{[z-a]`).
- 🔵 no test pinned the trimmed map key — fixed: `a_pattern_is_reported_without_its_trailing_spaces`.
- 🔵 no test pinned `NotADirectory` as no file — fixed: `a_path_under_a_regular_file_is_not_ignored`.
- 🔵 doc said every file is read once, but a refused file is read again — fixed: doc corrected.
- 🔵 index arithmetic with silent `get(..).unwrap_or(..)` fallbacks — fixed: each (folder, relative path) pair built directly with `split_at`.
- 🔵 `a_bad_file_on_the_way_is_an_error` duplicated another test — fixed: asks `lib/deep/l.rs`.
- 🔵 `new` accepted a relative root — fixed: documented as the canonical root, `debug_assert!(root.is_absolute())`.
- 🔵 crate doc's port table left out `ignore_rules` — fixed.
- 🔵 `is_folder: bool` parameter — declined: this plan names the signature; private to the module.
- ❓ a `.buildlignore` under a folder symlinked outside the workspace is read through the link — declined: spec §2.2 refuses only a symlinked `.buildlignore` itself; the build reads its inputs through the same folder link, so reading the ignore file there exposes nothing new.

## Deviations

- 2026-11-10 — Task 2: `FolderRules`, `Decision`, `read`, `decide` are `pub(super)`, not `pub`: the workspace's `unreachable_pub` lint denies plain `pub` in a private module.
- 2026-11-10 — Task 2: `ErrorKind::NotADirectory` when reading a folder's `.buildlignore` is treated as no file (a path under a regular file); the plan did not say. Pinned by a test.
- 2026-11-10 — Task 2: `a_symlinked_file_is_refused` is `#[cfg(unix)]`.
- 2026-11-10 — Tasks 1 and 2 ran in one coder with one gate run (disjoint files), committed separately.
