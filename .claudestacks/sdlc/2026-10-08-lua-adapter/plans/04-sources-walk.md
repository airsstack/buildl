---
status: done
created: 2026-10-08
depends-on: [01, 03]
---

# Sources Walk Implementation Plan

**Goal:** `buildl.sources`' host-side walk returns the sorted regular files under the declaring directory that match a pattern, and nothing outside the workspace.

**Architecture:** One file, `sources.rs`. `Sources` pairs the canonical workspace root with one declaring directory. `find` compiles the pattern with the same `globset` flags airsl's own `glob` module uses (`literal_separator`, `backslash_escape`). It canonicalises the declaring directory and refuses it if the result is not under the root, then walks it with walkdir without following any link, the root included. It keeps regular files whose path relative to the declaring directory matches, refuses a matching non-UTF-8 name, and sorts by bytes. The Lua engine holds no filesystem grant, so this is the declaration phase's only read.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §6.2 (the walk), D1 (host-side), D2 (what it returns), D14 (symlinked base), D15 (accepted cost), D16 (non-UTF-8 names); §1.1 W1–W3 (probe results).

---

## Context an implementer needs

**Conventions every task follows.** The workspace lints are strict. `unwrap`, `expect` and `panic!`
are denied outside tests, pedantic and nursery clippy run at `-D warnings`, and `missing_docs` and
`unreachable_pub` are on. Crate-internal items are therefore `pub(crate)` inside private modules. A
test module opens with `#![expect(clippy::unwrap_used, reason = …)]`, adding `clippy::panic` where
it panics. `lib.rs` holds only module docs, `mod` declarations and `pub use` re-exports. Every file
opens with `//!` docs naming its responsibilities and non-responsibilities, and carries no internal
planning vocabulary. Every commit is Conventional Commits with scope `buildl-lua`, or `repo` for
workspace files. Every task ends green on `cargo make dod` before it commits.

**Where the code comes from.** Every code block below was compiled, linted and tested in a scratch
copy of the workspace at every task boundary of this plan set, on macOS with rustc 1.98 and clippy
1.98. The expected outputs are the outputs those runs printed.

| Item | From | Used for |
|---|---|---|
| `globset::GlobBuilder` (0.4.20) | plan `01` dependency | the matcher |
| `walkdir::WalkDir` (2.5.0), `.follow_root_links(false)` (`walkdir-2.5.0/src/lib.rs:365`; the default is `true`, `:293`) | plan `01` dependency | the walk |
| `Refusal` | plan `03`, `refusal.rs` | errors |
| `Directory::{root, parse, as_str}`, `EvaluationFailure::{Runtime, Refused}` | `buildl-core` | the declaring directory; failure kinds |
| `tempfile::tempdir` | plan `01` dev-dependency | test workspaces |

Tests canonicalise every temporary root first, because the walk compares canonical paths (on macOS
`/var` is a symlink to `/private/var`). The non-UTF-8 test runs on Linux only: APFS refuses
non-UTF-8 file names, so on macOS the module has 8 tests and on Linux 9.

## File structure

```text
crates/buildl-lua/src/sources.rs — [create] Sources and its walk, with unit tests
crates/buildl-lua/src/lib.rs     — [modify] declare `mod sources;`
```

### Task 1 — The host-side walk

On Linux the green run reports 9 tests, not 8 (see Context).

**Files:**
- Create `crates/buildl-lua/src/sources.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod sources;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/sources.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! The host-side walk behind `buildl.sources(pattern)`, the declaration phase's only filesystem
   //! read.
   //!
   //! Its own file because it is the one place a build file's evaluation touches the filesystem. The
   //! Lua engine holds no filesystem grant at all, so containment is enforced here and nowhere else:
   //! the walk starts at the declaring directory, refuses a directory that resolves outside the
   //! workspace, never follows a symlink, and returns regular files only.
   //!
   //! Responsibilities: [`Sources`], one declaring directory's walk.
   //!
   //! Non-responsibilities: turning the paths into inputs. They are returned to Lua as text relative
   //! to the declaring directory, and joined with it after evaluation.

   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until `LuaSource` evaluates a build file"
       )
   )]

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::os::unix::fs::symlink;
       use std::path::{Path, PathBuf};

       use buildl_core::{Directory, EvaluationFailure};

       use super::Sources;

       /// A canonical temporary workspace holding `files`, each created empty.
       fn workspace(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           for file in files {
               let path = root.join(file);
               fs::create_dir_all(path.parent().unwrap()).unwrap();
               fs::write(path, "").unwrap();
           }
           (dir, root)
       }

       fn sources(root: &Path, directory: &str) -> Sources {
           let directory = if directory.is_empty() {
               Directory::root()
           } else {
               Directory::parse(directory).unwrap()
           };
           Sources::new(root, &directory)
       }

       #[test]
       fn returns_matching_regular_files_sorted_by_bytes() {
           let (_dir, root) = workspace(&["b.c", "a/z.c", "a.c", "sub/c.c", "a.h", ".hidden/h.c"]);
           assert_eq!(
               sources(&root, "").find("**/*.c").unwrap(),
               [".hidden/h.c", "a.c", "a/z.c", "b.c", "sub/c.c"]
           );
       }

       #[test]
       fn a_star_stops_at_a_directory_boundary() {
           let (_dir, root) = workspace(&["a.c", "sub/b.c"]);
           assert_eq!(sources(&root, "").find("*.c").unwrap(), ["a.c"]);
       }

       #[test]
       fn paths_are_relative_to_the_declaring_directory() {
           let (_dir, root) = workspace(&["lib/src/text.c", "app/main.c"]);
           assert_eq!(
               sources(&root, "lib").find("src/*.c").unwrap(),
               ["src/text.c"]
           );
       }

       #[test]
       fn a_pattern_cannot_reach_outside_the_declaring_directory() {
           let (_dir, root) = workspace(&["lib/a.c", "app/b.c"]);
           assert!(sources(&root, "lib").find("../**/*.c").unwrap().is_empty());
       }

       #[test]
       fn symlinks_below_the_directory_are_neither_returned_nor_entered() {
           let (_dir, root) = workspace(&["a.c", "outside/secret.c"]);
           symlink(root.join("outside"), root.join("link")).unwrap();
           symlink(root.join("a.c"), root.join("alias.c")).unwrap();
           let (_other, elsewhere) = workspace(&["far.c"]);
           symlink(&elsewhere, root.join("far")).unwrap();
           assert_eq!(
               sources(&root, "").find("**/*.c").unwrap(),
               ["a.c", "outside/secret.c"]
           );
       }

       #[test]
       fn a_declaring_directory_that_resolves_outside_the_workspace_is_refused() {
           let (_dir, root) = workspace(&[]);
           let (_other, elsewhere) = workspace(&["far.c"]);
           symlink(&elsewhere, root.join("lib")).unwrap();
           let refusal = sources(&root, "lib").find("*.c").unwrap_err();
           assert_eq!(refusal.failure(), &EvaluationFailure::Refused);
       }

       #[test]
       fn an_invalid_pattern_is_refused_with_globset_s_message() {
           let (_dir, root) = workspace(&[]);
           let refusal = sources(&root, "").find("a[").unwrap_err();
           assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
           assert_eq!(
               refusal.diagnostic("sources", None).as_str(),
               "buildl.sources: error parsing glob 'a[': unclosed character class; missing ']'"
           );
       }

       #[test]
       fn a_missing_declaring_directory_is_a_runtime_refusal() {
           let (_dir, root) = workspace(&[]);
           let refusal = sources(&root, "gone").find("*.c").unwrap_err();
           assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
       }

       #[cfg(target_os = "linux")]
       #[test]
       fn a_matching_non_utf8_file_name_is_refused() {
           use std::ffi::OsStr;
           use std::os::unix::ffi::OsStrExt;

           let (_dir, root) = workspace(&[]);
           fs::write(root.join(OsStr::from_bytes(b"bad\xff.c")), "").unwrap();
           let refusal = sources(&root, "").find("*.c").unwrap_err();
           assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib sources::tests
   ```

   ```text
   error[E0432]: unresolved import `super::Sources`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use std::fs;
   use std::path::{Path, PathBuf};

   use buildl_core::{Directory, EvaluationFailure};
   use globset::GlobBuilder;
   use walkdir::WalkDir;

   use crate::refusal::Refusal;

   /// The walk for one declaring directory.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) struct Sources {
       root: PathBuf,
       directory: PathBuf,
   }

   impl Sources {
       /// The walk for `directory`, under the canonical workspace `root`.
       pub(crate) fn new(root: &Path, directory: &Directory) -> Self {
           Self {
               root: root.to_path_buf(),
               directory: root.join(directory.as_str()),
           }
       }

       /// Every regular file under the declaring directory whose relative path matches `pattern`,
       /// `/`-separated and sorted by bytes.
       ///
       /// The matcher uses the same flags as airsl's `glob` module: `*` stops at `/`, and `**/`
       /// matches zero or more directories.
       pub(crate) fn find(&self, pattern: &str) -> Result<Vec<String>, Refusal> {
           let matcher = GlobBuilder::new(pattern)
               .literal_separator(true)
               .backslash_escape(true)
               .build()
               .map_err(|error| Refusal::new(EvaluationFailure::Runtime, error.to_string()))?
               .compile_matcher();
           let base = fs::canonicalize(&self.directory).map_err(|error| {
               Refusal::new(
                   EvaluationFailure::Runtime,
                   format!("{}: {error}", self.directory.display()),
               )
           })?;
           if !base.starts_with(&self.root) {
               return Err(Refusal::new(
                   EvaluationFailure::Refused,
                   format!(
                       "{} resolves outside the workspace",
                       self.directory.display()
                   ),
               ));
           }

           let mut found = Vec::new();
           for entry in WalkDir::new(&base).follow_root_links(false) {
               let entry = entry
                   .map_err(|error| Refusal::new(EvaluationFailure::Runtime, error.to_string()))?;
               if !entry.file_type().is_file() {
                   continue;
               }
               let Ok(relative) = entry.path().strip_prefix(&base) else {
                   continue;
               };
               if !matcher.is_match(relative) {
                   continue;
               }
               let text = relative.to_str().ok_or_else(|| {
                   Refusal::new(
                       EvaluationFailure::Runtime,
                       format!("non-UTF-8 file name {}", relative.display()),
                   )
               })?;
               found.push(text.to_owned());
           }
           found.sort();
           Ok(found)
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib sources::tests
   ```

   ```text
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): walk the declaring directory for buildl.sources`.

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --lib sources::tests` reports 8 passed on macOS, 9 on Linux.
- `cargo make dod` exits `0`.

## Review findings

- strong-types — `root` is documented as canonical but not checked. A non-canonical root fails safe, because every call is refused — `crates/buildl-lua/src/sources.rs:40`
- doc-comment-discipline — the `find` rustdoc does not list its refusal cases — `sources.rs:52`
- reversion guard — `.backslash_escape(true)` is globset's Unix default, so no test catches its removal — `sources.rs:54`
- reversion guard — no test fails if `.follow_root_links(false)` is removed; the D14 test refuses at `starts_with` first — `sources.rs:76`
- unit-test-mandate — no test covers the walk io-error branch — `sources.rs:78`
- D16 — a failed `strip_prefix` skips the entry silently. This cannot happen in practice — `sources.rs:82`
- risk — the Linux-only non-UTF-8 test has never compiled or run here, so CI ubuntu will run it first. globset builds a byte-mode regex, so it is expected to pass — `sources.rs:207`
- spec drift — §6.2 canonicalises the base before building the matcher; the code builds the matcher first. The difference shows only when a call has both a bad pattern and a missing directory — spec §6.2
- spec drift — §6.2 renders a walk io error as `<path>: <io error>`; the code passes walkdir's message through: `IO error for operation on <path>: …` — spec §6.2

Blocking set: none. The reviewer re-ran `cargo make dod` and it exited 0, and confirmed that airsl 0.1.4 `src/modules/glob.rs` uses `literal_separator(true)` and `backslash_escape(true)`.

## Probe results

- The task's red step proved its premises. `cargo test -p buildl-lua --lib sources::tests` gave `error[E0432]: unresolved import super::Sources`, then `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out` on macOS.
- Batch gate: `cargo make dod` gave a buildl-lua lib total of `41 passed` and `Build Done`. `cargo deny check` gave `advisories ok, bans ok, licenses ok, sources ok`.

## Deviations

- 2026-10-08 — The plan's per-task commit was not made. Commits are left to the user.
