---
status: done
created: 2026-11-10
built-on: 6643cfeb82a2
---

# Ignore Port Implementation Plan

**Goal:** `buildl-core` can ask "is this workspace path ignored?" through a port whose answers and errors are typed.

**Architecture:** A new port trait `IgnoreRules` in `crates/buildl-core/src/ports/`, beside the
other ports and re-exported from `ports/mod.rs` and the crate root like them. It speaks two new
value types in `types/`: `EntryKind` (`File`, `Folder`, `Unknown`) and `Ignoring` (the deciding
`.buildlignore` and the pattern as written). Two new `Error` variants carry the refusals of spec §5;
the refused path is an `IgnoredPath`, `Input(SourcePath)` or `Subdir(Directory)`, so the field and
the path cannot disagree. `NothingIgnored`, a port that ignores nothing, sits beside the trait the
way `NoRemote` sits beside `RemoteCache` (`crates/buildl-core/src/ports/no_remote.rs:1-22`): every
test, doctest and test bundle in the workspace that needs a rule set and is not about ignoring uses
it. No phase uses the port yet: plan 03 wires it into load, plan 04 into watch. The command line
maps both new variants to exit code 5 now.

**Tech Stack:** Rust 1.94, `buildl-core` (no I/O, `crates/buildl-core/clippy.toml`), `thiserror`; one
match in `buildl`.

**Content authority:** `spec.md` §3 (`IgnoreRules`, `EntryKind`, `Ignoring`, `Error::Ignored`,
`Error::IgnoreFile` rows), §5.

---

## Files that change

```
crates/buildl-core/src/types/entry_kind.rs       — create: EntryKind (File, Folder, Unknown)
crates/buildl-core/src/types/ignoring.rs         — create: Ignoring and IgnoredPath
crates/buildl-core/src/types/mod.rs              — modify: declare and index the two files
crates/buildl-core/src/ports/ignore_rules.rs     — create: the IgnoreRules port trait
crates/buildl-core/src/ports/nothing_ignored.rs  — create: NothingIgnored, the rule set that ignores nothing
crates/buildl-core/src/ports/mod.rs              — modify: declare and re-export both (:43-80)
crates/buildl-core/src/error.rs                  — modify: Error::Ignored and Error::IgnoreFile (enum :50-628)
crates/buildl-core/src/lib.rs                    — modify: re-export the new port, port impl and types (:182-213)
crates/buildl/src/cli/exit.rs                    — modify: both variants in the Definition arm (:122-148)
```

## Order of work

### Task 1 — Add the value types

**Files:**
- Create `crates/buildl-core/src/types/entry_kind.rs` — `EntryKind`
- Create `crates/buildl-core/src/types/ignoring.rs` — `Ignoring`, `IgnoredPath`
- Modify `crates/buildl-core/src/types/mod.rs` — declare and index them
- Modify `crates/buildl-core/src/lib.rs` — re-export them

**Change:** `EntryKind` is `File`, `Folder` or `Unknown` (a path its asker cannot classify).
`Ignoring` holds `file: String`, the workspace-relative, `/`-separated path of the `.buildlignore`
whose pattern decided (a `String`, not a `SourcePath`, because the folder holding it may have a name
the source-path grammar refuses), and `pattern: String`, that pattern as written; both read through
accessors. `IgnoredPath` is `Input(SourcePath)` or `Subdir(Directory)`; its display is the path. All
derive `Debug`, `Clone`, `PartialEq`, `Eq`.

**Proof:**
- `an_ignoring_keeps_its_file_and_pattern` — an `Ignoring` built from `notes/.buildlignore` and
  `drafts/` returns both unchanged.
- `an_ignored_path_displays_as_its_path` — `Input` of `notes/a.md` and `Subdir` of `examples` display
  as `notes/a.md` and `examples`.

**Done when:** `cargo test -p buildl-core --all-targets --all-features` passes and `cargo make dod`
and `cargo deny check -D unused-wrapper` are green. Commit
`feat(buildl-core): add the EntryKind, Ignoring and IgnoredPath values`.

### Task 2 — Add the two errors

**Files:**
- Modify `crates/buildl-core/src/error.rs` — `Error::Ignored`, `Error::IgnoreFile`
- Modify `crates/buildl/src/cli/exit.rs` — both in the Definition arm

**Change:** `Error::Ignored { provenance, origin, order, path: IgnoredPath, ignoring: Ignoring }`
mirrors `InvalidDeclaration`'s site fields (`error.rs:151-167`). Its message, after the build file's
provenance, is `inputs: <path> is ignored by <file> (<pattern>)` for `Input` and
`subdir: <path> is ignored by <file> (<pattern>)` for `Subdir` (spec §5).
`Error::IgnoreFile { file: String, line: Option<u32>, reason: String }`: `<file>:<line>: <reason>`
with a line, `<file>: <reason>` without. `exit.rs` maps both to `Definition` (5).

**Proof:**
- `an_ignored_input_names_the_path_the_file_and_the_pattern` — `Input` of `notes/a.md`,
  `.buildlignore`, `notes/` displays as spec §5's first row after the provenance.
- `an_ignored_subdir_says_subdir` — `Subdir` of `examples` with `examples/` reads `subdir: examples is
  ignored by .buildlignore (examples/)`.
- `an_ignore_file_error_with_a_line` — `lib/.buildlignore`, line 1, `error parsing glob '[z-a]':
  invalid range; 'z' > 'a'` displays as spec §5's bad-line row.
- `an_ignore_file_error_without_a_line` — `lib/.buildlignore`, no line, `a symlink, not a file`
  displays as `lib/.buildlignore: a symlink, not a file`.
- `both_ignore_errors_exit_with_the_definition_code` (in `exit.rs`'s tests) — each maps to 5.

**Done when:** `cargo test -p buildl-core -p buildl --all-targets --all-features` passes and
`cargo make dod` and `cargo deny check -D unused-wrapper` are green. Commit
`feat(buildl-core): add the Ignored and IgnoreFile errors`.

### Task 3 — Add the IgnoreRules port and NothingIgnored

**Files:**
- Create `crates/buildl-core/src/ports/ignore_rules.rs` — the trait
- Create `crates/buildl-core/src/ports/nothing_ignored.rs` — `NothingIgnored`
- Modify `crates/buildl-core/src/ports/mod.rs`, `crates/buildl-core/src/lib.rs` — declare and re-export

**Change:** `pub trait IgnoreRules: Sync` with one method,
`fn ignored(&self, path: &str, kind: EntryKind) -> Result<Option<Ignoring>>`. Its doc states the
contract: `path` is workspace-relative, `/`-separated and never empty (the workspace root itself is
never asked); `Ok(None)` is not ignored; `Ok(Some)` names the deciding pattern; a `.buildlignore` that
cannot be used is `Err(Error::IgnoreFile)`; the decision is spec §2.3's root-down rule.
`NothingIgnored` (`Debug, Clone, Copy, Default, PartialEq, Eq`) answers `Ok(None)` to every question.

**Proof:**
- `nothing_ignored_ignores_nothing` — `NothingIgnored` answers `None` for a file, a folder and an
  `Unknown` path.
(The trait itself declares no logic: the unit-test mandate's structural exemption for a port
declaration, as the other trait files of `ports/` use it.)

**Done when:** `cargo test -p buildl-core --all-targets --all-features` passes, `cargo make
guard-module-edges` passes, and `cargo make dod` and `cargo deny check -D unused-wrapper` are green.
Commit `feat(buildl-core): add the IgnoreRules port`.

---

## Risks

- `Error` is `#[non_exhaustive]` and matched in `exit.rs`; a variant left out of the Definition arm
  would fall to another arm. Task 2's exit test pins both.
- The message text is asserted in plan 03's and plan 05's CLI tests; a change of wording here is
  carried there.

## Proof

- `cargo make dod` and `cargo deny check -D unused-wrapper` green after each task.
- `cargo make guard-module-edges` and `cargo make guard-core-purity` pass: the new files use only
  `ports -> types`, `ports -> error` and `types -> error`, which exist
  (`crates/expected-module-edges.txt:43-44,48`).

## Review findings

One reviewer pass over c21a313..dceae96: 0 blocking.

- ❓ `Error::Ignored` keeps `order` but its message omits it — declined: the message text is fixed by spec §5 and this plan; `order` stays for callers that read fields.
- ❓ `IgnoreRules` doc stated the rule unclearly (deeper file vs ignored parent; last line in a file) — fixed in 2465268: the doc states the root-down rule, deeper file first, last matching line wins, first ignored step decides. `cargo check -p buildl-core --all-targets` → clean.
- 🔵 `line_suffix(Option<&u32>)` → takes `Option<u32>` — fixed in 2465268.
- 🔵 `Ignoring::new` lacked `#[must_use]` — fixed in 2465268.
- 🔵 `entry_kind.rs` no-tests line worded unlike its siblings — fixed in 2465268.
- 🔵 `path: &str` and `Ignoring`'s `String` fields are plain strings — declined: this plan names those types with its reasons.

## Deviations

- 2026-11-10 — Task 2: `Error::Ignored` holds `path: Box<IgnoredPath>` and `ignoring: Box<Ignoring>`, not plain values. Unboxed, `Error` passed clippy's `result_large_err` limit (128 bytes) and `cargo make dod` failed workspace-wide. Display text unchanged; later plans build them with `Box::new`.
- 2026-11-10 — Task 2: `exit.rs`'s `every_error_variant_has_its_code` counts raised (27→29, 54→56) for the two new variants.
- 2026-11-10 — Tasks 2 and 3 ran in one coder with one gate run (disjoint files), committed separately.
