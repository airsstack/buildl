---
status: done
created: 2026-10-14
depends-on: [04]
---

# Input Adapters Implementation Plan

**Goal:** The `buildl` crate gains the four adapters that read a build's surroundings before anything runs.

**Architecture:** Four modules of `crates/buildl/src`, each implementing one port of `buildl-core` and naming no other adapter module. `manifest/` finds the workspace root (`WorkspaceRoot::find`: the nearest `buildl.toml` at or above a canonical start directory) and reads the file once into `TomlManifest`: serde tables that refuse every key they do not declare (`schema.rs`), turned into core values with every default, each refusal placed at the line and column of the value (`interpret.rs`, `memory.rs`), and held as a `Workspace`, an `EntryName` and a `DeclarationSettings` (`toml_manifest.rs`). `host/` is `LocalHost`: the platform the binary was compiled for, and one environment variable read each time it is asked for. `digest/` is `FileDigester`: one `symlink_metadata` per input for `stat`, and for `digest` a rayon parallel map whose results are collected in input order and folded in order, each file streamed through a 64 KiB buffer into the core's `Hasher`. `tools/` is `PathTools`: the file `SearchPath::locate` returns, streamed into `Hasher`. Nothing in the four modules writes a file. Rayon is named in `digest/file_digester.rs` and nowhere else; `tools/` names the shared `search_path` module and nothing else of the crate.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor; `toml` 0.8.23 (parse only), `rayon` 1.12.0, `serde` 1.0.229 (derive), `tempfile` 3.27.0 for temp directories in tests; `cargo-make`. This plan adds no dependency and edits no manifest.

**Content authority:** spec §4.1 (workspace root), §4.2 (`buildl.toml`), §4.3 (`host/`), §4.4 (`digest/`), §4.6 (`tools/`); §11, the rows raised by `manifest/`, `digest/`, `host/` and `tools/`; §12.2, the unit tests of `manifest/`, `host/`, `digest/` and `tools/`; §14 (the versions of `toml`, `rayon`, `serde`, `tempfile`); §17, the rows for `symlink_metadata` kinds, rayon's ordered collect and the platform text.

---

## Context an implementer needs

What the plan this one depends on leaves in `crates/buildl`, and what this plan does with it:

| Item | State | Used as |
|---|---|---|
| `crates/buildl/src/manifest/mod.rs`, `host/mod.rs`, `digest/mod.rs`, `tools/mod.rs` | each exists and holds a `//!` doc comment and nothing else | replaced whole: by task 1 (and tasks 2, 3, 4, 6), task 8, task 9 and task 10 |
| `crates/buildl/src/lib.rs` | declares `pub mod manifest;`, `pub mod host;`, `pub mod digest;`, `pub mod tools;` and `pub mod search_path;` | not touched |
| `crates/buildl/src/search_path.rs` | `pub struct SearchPath` with `pub fn new(directories: impl IntoIterator<Item = PathBuf>) -> Self` and `pub fn locate(&self, program: &Program, base: &Path) -> Option<PathBuf>`; derives `Debug, Clone, PartialEq, Eq, Default` | the lookup `tools/` fingerprints through (task 10) |
| `crates/buildl/Cargo.toml` | `[dependencies]` holds `rayon`, `serde`, `tempfile`, `thiserror` and `toml`, each `{ workspace = true }`; the lockfile resolves them to the versions in **Tech Stack** | not touched; tests use `tempfile::tempdir()` |
| `Makefile.toml` | `cargo make dod` runs `guard-adapter-isolation` before clippy | the guard reads these four modules; `tools -> search_path` is the one edge they add |

What `buildl-core` provides, as the plans before this one leave it. Every item is reached as `buildl_core::<Item>`:

| Item | Shape | Where |
|---|---|---|
| `Manifest` | `fn workspace(&self) -> Result<Workspace>` | `crates/buildl-core/src/ports/manifest.rs` |
| `Host` | `fn platform(&self) -> Platform`; `fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>` | `ports/host.rs` |
| `Digester` | `fn stat(&self, path: &SourcePath) -> Result<InputStat>`; `fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>>` | `ports/digester.rs` |
| `ToolResolver` | `fn fingerprint(&self, program: &Program) -> Result<Option<Digest>>` | `ports/tool_resolver.rs` |
| `InputStat`, `InputKind` | `InputStat::{Missing, File(FileStat), Other(InputKind)}`; `InputKind::{Symlink, Directory, Special}`, shown as `a symbolic link`, `a directory`, `a pipe, a socket or a device` | `types/input_stat.rs` |
| `FileStat` | `pub modified: Timestamp`, `pub size: u64` | `types/file_stat.rs` |
| `Timestamp` | `Timestamp::from_unix_nanos(u64)`, `as_unix_nanos(self) -> u64` | `types/timestamp.rs` |
| `Digest`, `Hasher` | `Digest::of(&[u8])`; `Hasher::new()`, `update(&mut self, &[u8])`, `finish(self) -> Digest` | `types/digest.rs` |
| `Workspace`, `Execution` | `Workspace { out: Directory, state: Directory, ceiling: Ceiling, execution: Execution }`; `Execution { workers, timeout, strategy }`, `Default` | `types/workspace.rs` |
| `Ceiling` | `Ceiling { run: BTreeSet<Program>, env: BTreeSet<EnvName>, read: BTreeSet<Directory>, write: BTreeSet<Directory> }` | `types/authority.rs` |
| `Directory` | `root()`, `parse(impl Into<String>) -> Result<Self>` (`""` is the root, `"."` is refused), `is_root()`, `contains(&self, &Self)` | `types/directory.rs` |
| `EntryName`, `Program`, `EnvName`, `EnvValue`, `Platform`, `SourcePath` | each `parse(impl Into<String>) -> Result<Self>` and `as_str()` | `types/` |
| `Workers`, `Timeout`, `Isolation` | each `parse(&str) -> Result<Self>` and `Default`: `Workers::PerCore`, ten minutes, `Isolation::Sandbox`; `Workers::of(usize)` | `types/` |
| `Error::Adapter { port: PortName, diagnostic: Diagnostic }` | shown as `{port}: {diagnostic}`; `PortName::Manifest`, `Host`, `Digester`, `ToolResolver` are shown as `manifest`, `host`, `digester`, `tool resolver`; `Diagnostic::new(impl Into<String>)` | `error.rs`, `types/diagnostic.rs` |
| `Error::InvalidName` | what every `parse` above returns, shown as `invalid <kind>: "<value>" — <reason>` | `error.rs` |

Facts of the gate that shape the tasks:

- The workspace denies warnings, and `cargo clippy --all-targets` builds the library both without
  and with its tests. An item nothing public reaches is dead code in the first build. `memory.rs`,
  `schema.rs` and `interpret.rs` are private and are first called by `TomlManifest::parse`, which
  task 6 writes. Until then their `mod` lines in `manifest/mod.rs` carry
  `#[cfg_attr(not(test), expect(dead_code, reason = "<why>"))]`, with the reason written out in each task. It is `expect`, not `allow`: the attribute turns
  into an error once the code is live, and task 6 removes all three.
- For the same reason every private item must be used by a test in the task that adds it: the
  second build is the one with tests.
- Tests unwrap. Each `mod tests` opens with
  an inner `#![expect(clippy::unwrap_used, reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal")]`, as the tests of `buildl-core` and `buildl-lua` do.
- No test sets an environment variable or changes the current directory. `std::env::set_var` is
  `unsafe` in this edition and the workspace forbids `unsafe`. `host/` is tested through what the
  process already holds, and through `value`, a function over a value handed to it.
- A test that needs a workspace makes one with `tempfile::tempdir()` and canonicalises its path:
  on macOS a temp directory is reached through a link (`/var` to `/private/var`).
- The pipe in the `digest/` tests is made with the system's `mkfifo`, run by its name.

Texts the design leaves to this plan. The design fixes the form `buildl.toml:<line>:<column>: <key>: <reason>` and, for a value a core type refuses, that type's own text. For a refusal no core type makes, the reason is written here in the same shape, `invalid <kind>: <value> — <reason>`:

| Key | Refused when | Reason |
|---|---|---|
| `workspace.name` | empty | `invalid workspace name: "" — must not be empty` |
| `workspace.config` | not `1` | `invalid config version: 2 — must be 1` |
| `workspace.out` | the root | `invalid output directory: "" — must not be the workspace root` |
| `workspace.out` | `.buildl`, a directory holding it or one inside it | `invalid output directory: ".buildl/out" — must not be the state directory .buildl, hold it or lie inside it` |
| `ceiling.fs.read`, `ceiling.fs.write` | `""` | `invalid directory: "" — must not be empty; the workspace root is written "."` |
| `declaration.memory` | outside its grammar | `invalid memory size: "16" — must end in one of the units B, KB, MB and GB`; also `must not be empty`, `must start with a whole number`, `must not be zero`, `is too large` |
| `declaration.instructions`, `declaration.walk_entries` | below 1 | `invalid count: 0 — must be at least 1` |
| `execution.workers` | negative | the core's text, through `Workers::parse` on the number's decimal form: `invalid worker count: "-1" — must be a whole number, written in digits only` |

Two more texts are this plan's: a modification time `u64` nanoseconds cannot hold is `<path>: its modification time is after the year 2554` (the design names only `before 1970`), and an environment value holding a NUL byte is `environment variable <NAME> holds a NUL byte`.

One thing the tables do that the design does not say. A struct that derives `Deserialize` is also read from an array, by position, and entries beyond its fields are dropped. A probe with the tables as the design gives them (each `#[serde(deny_unknown_fields)]` and nothing more; serde 1.0.229, toml 0.8.23, parse only) read three texts and printed what it got:

```text
"execution = [4, \"1s\", \"bare\"]\n"
  -> OK: proc.run = [], workers = Some(4), timeout = Some("1s"), strategy = Some("bare")
"[ceiling]\nproc = [[\"cc\"], [\"ld\"]]\n"
  -> OK: proc.run = ["cc"], workers = None, timeout = None, strategy = None
"ceiling = 3\n"
  -> ERR: invalid type: integer `3`, expected struct CeilingTable
```

The second text loses `ld` without a word, which is what refusing unknown keys exists to prevent. So every field of `schema.rs` that holds a table is read through `table`, a function that takes a table and refuses every other form (task 3). With it the three texts are refused as `buildl.toml:1:13: invalid type: sequence, expected a table`, `buildl.toml:2:8: invalid type: sequence, expected a table` and ``buildl.toml:1:11: invalid type: integer `3`, expected a table``: all three are cases of task 6's `a_value_of_the_wrong_type_is_refused_at_its_position`. Every line the design quotes from the parser is unchanged, and the same task's tests hold each of them.

How every task is laid out, and what its quoted output is. A source file here is a `//!` documentation block, a blank line, the code, a blank line, and the `#[cfg(test)] mod tests` module. A task that creates a file first writes the documentation block and the tests, runs them and sees them fail to compile, then puts the code between the two. A task that changes a file replaces its tests module first, then everything above it. All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is in rustfmt's form.

Each quoted run was captured from a prototype of this plan built task by task, on macOS (`Darwin 25.3.0 arm64`, rustc 1.99.0), in a copy of the workspace holding the `buildl-core` this plan builds on, the skeleton described above, and nothing else in `crates/buildl`. A failing run is quoted by the lines that name the cause. A passing run is quoted by its `passed` and `failed` figures; its `filtered out` figure counts the crate's other tests and depends on which other modules have landed, so it is left out. After every task the five commands `cargo make dod` runs were run for the `buildl` crate (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, `cargo test --all-targets`, `cargo test --doc`) together with the body of `guard-adapter-isolation`, and all exited `0`; after task 10 the same five ran for the whole workspace and exited `0`. `cargo make dod` and `cargo deny check` read `Makefile.toml`, `deny.toml` and the golden files, which belong to the plan this one depends on, so they were run once, on the tree task 10 leaves, inside a copy of that plan's own prototype: `cargo make dod` exited `0`, printing `guard-adapter-isolation: OK (1 edges)` and ending `[cargo-make] INFO - Build Done in 30.17 seconds.`, and `cargo deny --offline check -D unused-wrapper` exited `0`, printing `advisories ok, bans ok, licenses ok, sources ok`. They were not run after each of tasks 1 to 9.

## File structure

```
crates/buildl/src/manifest/mod.rs            — [modify] declare and export the manifest module's parts
crates/buildl/src/manifest/root.rs           — [create] WorkspaceRoot; the manifest's file name
crates/buildl/src/manifest/memory.rs         — [create] the grammar of a memory size
crates/buildl/src/manifest/schema.rs         — [create] the tables of buildl.toml as serde reads them
crates/buildl/src/manifest/interpret.rs      — [create] tables to values; where a refusal is
crates/buildl/src/manifest/toml_manifest.rs  — [create] TomlManifest, DeclarationSettings
crates/buildl/src/host/mod.rs                — [modify] declare and export LocalHost
crates/buildl/src/host/local_host.rs         — [create] LocalHost
crates/buildl/src/digest/mod.rs              — [modify] declare and export FileDigester
crates/buildl/src/digest/file_digester.rs    — [create] FileDigester
crates/buildl/src/tools/mod.rs               — [modify] declare and export PathTools
crates/buildl/src/tools/path_tools.rs        — [create] PathTools
```

Every task ends with the same gate, run from the repository root:

```
$ cargo fmt --all -- --check
$ cargo make dod
$ cargo deny check
```

All three must exit `0`: `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with a `[cargo-make] INFO - Build Done` line, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

### Task 1 — Find the workspace root

**Files:**
- Create `crates/buildl/src/manifest/root.rs`
- Modify `crates/buildl/src/manifest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/manifest/root.rs` with its documentation and its tests:

   ````rust
   //! Finding the workspace a command was started in.
   //!
   //! Its own file because the root is found before anything is read: a directory is a workspace
   //! root because it holds `buildl.toml`, whatever that file says. Every adapter that touches the
   //! workspace is handed the path found here, so this is the one place it is made canonical.
   //!
   //! Responsibilities: [`WorkspaceRoot`], and the name of the manifest file.
   //!
   //! Non-responsibilities: reading the manifest. A `buildl.toml` that cannot be read, or says
   //! something this version refuses, fails where it is read.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::os::unix::fs::symlink;
       use std::path::{Path, PathBuf};

       use super::WorkspaceRoot;

       /// A temp directory holding `ws/a/b/c`, with a manifest in `ws`; and its canonical path.
       fn tree() -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let top = fs::canonicalize(dir.path()).unwrap();
           fs::create_dir_all(top.join("ws/a/b/c")).unwrap();
           fs::write(top.join("ws/buildl.toml"), "").unwrap();
           (dir, top)
       }

       #[test]
       fn finds_the_nearest_manifest_above_the_start() {
           let (_dir, top) = tree();
           let outer = WorkspaceRoot::find(&top.join("ws/a/b/c")).unwrap();
           assert_eq!(outer.as_path(), top.join("ws"));

           fs::write(top.join("ws/a/buildl.toml"), "").unwrap();
           let nested = WorkspaceRoot::find(&top.join("ws/a/b/c")).unwrap();
           assert_eq!(nested.as_path(), top.join("ws/a"));
       }

       #[test]
       fn a_manifest_of_any_kind_marks_the_root() {
           let (_dir, top) = tree();
           fs::create_dir(top.join("ws/a/buildl.toml")).unwrap();
           symlink(top.join("nowhere"), top.join("ws/a/b/buildl.toml")).unwrap();
           assert_eq!(
               WorkspaceRoot::find(&top.join("ws/a")).unwrap().as_path(),
               top.join("ws/a")
           );
           assert_eq!(
               WorkspaceRoot::find(&top.join("ws/a/b/c"))
                   .unwrap()
                   .as_path(),
               top.join("ws/a/b")
           );
       }

       #[test]
       fn a_start_reached_through_a_link_gives_the_real_root() {
           let (_dir, top) = tree();
           fs::write(top.join("ws/a/buildl.toml"), "").unwrap();
           symlink(top.join("ws/a/b"), top.join("link")).unwrap();
           let root = WorkspaceRoot::find(&top.join("link")).unwrap();
           assert_eq!(root.as_path(), top.join("ws/a"));
           assert_eq!(root.within(), Path::new("b"));
       }

       #[test]
       fn no_manifest_up_to_the_filesystem_root_is_an_error() {
           let error = WorkspaceRoot::find(Path::new("/")).unwrap_err();
           assert_eq!(
               error.to_string(),
               "manifest: no buildl.toml in / or any directory above it"
           );
       }

       #[test]
       fn a_start_that_does_not_exist_is_an_error_naming_it() {
           let (_dir, top) = tree();
           let start = top.join("ws/missing");
           let error = WorkspaceRoot::find(&start).unwrap_err();
           assert_eq!(
               error.to_string(),
               format!(
                   "manifest: {}: No such file or directory (os error 2)",
                   start.display()
               )
           );
       }

       #[test]
       fn within_is_the_start_relative_to_the_root_and_empty_at_the_root() {
           let (_dir, top) = tree();
           let below = WorkspaceRoot::find(&top.join("ws/a/b")).unwrap();
           assert_eq!(below.within(), Path::new("a/b"));
           let at_root = WorkspaceRoot::find(&top.join("ws")).unwrap();
           assert_eq!(at_root.within(), Path::new(""));
       }
   }
   ````

   `no_manifest_up_to_the_filesystem_root_is_an_error` starts at `/`, the one directory with nothing above it; it holds as long as the machine has no `/buildl.toml`.

2. Replace the whole content of `crates/buildl/src/manifest/mod.rs` with:

   ````rust
   //! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
   //!
   //! Its own directory because the manifest is the one file that is both the marker of a
   //! workspace root and the ceiling on what a build may do, and no other module of this crate
   //! knows its name or its format.
   //!
   //! Responsibilities:
   //!
   //! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
   //!   started.
   //!
   //! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
   //! belong to the pipeline, which is handed what the manifest says as a value.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod root;

   pub use root::WorkspaceRoot;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::root
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::WorkspaceRoot`
   error[E0432]: unresolved import `root::WorkspaceRoot`
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. In `crates/buildl/src/manifest/root.rs`, put the code between the documentation block and the tests: after the blank line that follows the last `//!` line, with one blank line between its last line and `#[cfg(test)]`:

   ````rust
   use std::fs;
   use std::path::{Path, PathBuf};

   use buildl_core::{Diagnostic, Error, PortName, Result};

   /// The name of the file that marks a workspace root and holds its manifest.
   pub(crate) const MANIFEST_FILE: &str = "buildl.toml";

   /// The canonical, absolute directory that holds a workspace's `buildl.toml`, and where the
   /// search for it began.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::fs;
   /// use std::path::Path;
   ///
   /// use buildl::manifest::WorkspaceRoot;
   ///
   /// let workspace = tempfile::tempdir()?;
   /// fs::write(workspace.path().join("buildl.toml"), "")?;
   /// fs::create_dir_all(workspace.path().join("lib/text"))?;
   ///
   /// let root = WorkspaceRoot::find(&workspace.path().join("lib/text"))?;
   /// assert_eq!(root.as_path(), fs::canonicalize(workspace.path())?);
   /// assert_eq!(root.within(), Path::new("lib/text"));
   /// # Ok::<(), Box<dyn std::error::Error>>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct WorkspaceRoot {
       root: PathBuf,
       within: PathBuf,
   }

   impl WorkspaceRoot {
       /// The nearest directory, `start` itself or one above it, that holds `buildl.toml`.
       ///
       /// `start` is made canonical first, so a root reached through a symbolic link is the real
       /// directory. A directory holds the manifest when anything at all is named `buildl.toml` in
       /// it: the nearest one wins, and one that turns out to be unreadable fails when it is read
       /// instead of being passed over for a workspace further up.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], when `start` cannot be made
       /// canonical, and when neither it nor any directory above it holds `buildl.toml`.
       pub fn find(start: &Path) -> Result<Self> {
           let canonical = fs::canonicalize(start)
               .map_err(|error| failed(format!("{}: {error}", start.display())))?;
           canonical
               .ancestors()
               .find(|directory| fs::symlink_metadata(directory.join(MANIFEST_FILE)).is_ok())
               .map(|root| Self {
                   root: root.to_path_buf(),
                   within: canonical
                       .strip_prefix(root)
                       .map_or_else(|_| PathBuf::new(), Path::to_path_buf),
               })
               .ok_or_else(|| {
                   failed(format!(
                       "no {MANIFEST_FILE} in {} or any directory above it",
                       canonical.display()
                   ))
               })
       }

       /// The root: canonical and absolute.
       #[must_use]
       pub fn as_path(&self) -> &Path {
           &self.root
       }

       /// Where the search began, relative to the root; empty when it began at the root.
       #[must_use]
       pub fn within(&self) -> &Path {
           &self.within
       }
   }

   /// A failure to find a workspace, as the manifest adapter's error.
   fn failed(message: String) -> Error {
       Error::Adapter {
           port: PortName::Manifest,
           diagnostic: Diagnostic::new(message),
       }
   }
   ````

   The start directory is made canonical before the search, so the root found is a real directory whatever link led to it. A directory holds the manifest when `symlink_metadata` of `buildl.toml` in it succeeds, so an entry of any kind marks the root: one that cannot be read fails when it is read, and is never passed over for a workspace further up.

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::root
   ```

   Expected: a `test result: ok.` line reading `6 passed; 0 failed`.

6. Run the gate (the three commands under **File structure**). Expected: all exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/manifest/root.rs crates/buildl/src/manifest/mod.rs
   $ git commit -m "feat(buildl): find the workspace root from a start directory"
   ```

### Task 2 — Read a memory size

**Files:**
- Create `crates/buildl/src/manifest/memory.rs`
- Modify `crates/buildl/src/manifest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/manifest/memory.rs` with its documentation and its tests:

   ````rust
   //! The grammar of a memory size, such as `16MB`.
   //!
   //! Its own file because the manifest writes its one memory ceiling as text with a unit, and a
   //! size read from text has things to get wrong that a number does not: a missing unit, an
   //! unknown one, a size of zero, and a size no machine can count.
   //!
   //! Responsibilities: [`memory_bytes`].
   //!
   //! Non-responsibilities: what the size limits, and saying where in the manifest a rejected
   //! size stands. The reason returned here is joined to its key and its position by the caller.

   #[cfg(test)]
   mod tests {
       use super::memory_bytes;

       #[test]
       fn each_unit_is_1024_times_the_one_before() {
           let cases = [
               ("1B", 1),
               ("1KB", 1024),
               ("1MB", 1024 * 1024),
               ("1GB", 1024 * 1024 * 1024),
               ("16MB", 16 * 1024 * 1024),
               ("016MB", 16 * 1024 * 1024),
               ("17179869183GB", 17_179_869_183 * 1024 * 1024 * 1024),
           ];
           for (raw, expected) in cases {
               assert_eq!(
                   memory_bytes(raw).map(core::num::NonZeroU64::get),
                   Ok(expected),
                   "{raw:?}"
               );
           }
       }

       #[test]
       fn zero_a_missing_unit_and_an_overflow_are_rejected() {
           let number = "must start with a whole number";
           let unit = "must end in one of the units B, KB, MB and GB";
           let cases = [
               ("", "must not be empty"),
               ("MB", number),
               ("-1MB", number),
               (" 16MB", number),
               ("16", unit),
               ("16 MB", unit),
               ("16mb", unit),
               ("16MiB", unit),
               ("16M", unit),
               ("1.5GB", unit),
               ("16MB ", unit),
               ("0MB", "must not be zero"),
               ("000B", "must not be zero"),
               ("17179869184GB", "is too large"),
               ("18446744073709551616B", "is too large"),
           ];
           for (raw, reason) in cases {
               assert_eq!(memory_bytes(raw), Err(reason), "{raw:?}");
           }
       }
   }
   ````

   `17179869183GB` is the largest whole number of gibibytes a `u64` counts in bytes; one more is 2^64.

2. Replace the whole content of `crates/buildl/src/manifest/mod.rs` with:

   ````rust
   //! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
   //!
   //! Its own directory because the manifest is the one file that is both the marker of a
   //! workspace root and the ceiling on what a build may do, and no other module of this crate
   //! knows its name or its format.
   //!
   //! Responsibilities:
   //!
   //! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
   //!   started.
   //!
   //! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
   //! belong to the pipeline, which is handed what the manifest says as a value.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the manifest reader, which is not written yet"
       )
   )]
   mod memory;
   mod root;

   pub use root::WorkspaceRoot;
   ````

   The attribute on `mod memory;` is the one **Context** describes: nothing public calls `memory_bytes` until task 6, which removes it.

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::memory
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::memory_bytes`
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. In `crates/buildl/src/manifest/memory.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use core::num::NonZeroU64;

   /// The units a size may be written in, each with its length in bytes: every one is 1024 times
   /// the one before.
   const UNITS: [(&str, u64); 4] = [
       ("B", 1),
       ("KB", 1024),
       ("MB", 1024 * 1024),
       ("GB", 1024 * 1024 * 1024),
   ];

   /// The number of bytes `raw` states, or the reason it states none.
   ///
   /// A size is a whole number in ASCII digits directly followed by one unit: `B`, `KB`, `MB` or
   /// `GB`. It is never zero.
   pub(crate) fn memory_bytes(raw: &str) -> core::result::Result<NonZeroU64, &'static str> {
       if raw.is_empty() {
           return Err("must not be empty");
       }
       let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
       let (count, unit) = raw.split_at(digits);
       if count.is_empty() {
           return Err("must start with a whole number");
       }
       let Some(unit_bytes) = UNITS
           .into_iter()
           .find_map(|(word, bytes)| (word == unit).then_some(bytes))
       else {
           return Err("must end in one of the units B, KB, MB and GB");
       };
       let bytes = count
           .parse::<u64>()
           .ok()
           .and_then(|count| count.checked_mul(unit_bytes))
           .ok_or("is too large")?;
       NonZeroU64::new(bytes).ok_or("must not be zero")
   }
   ````

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::memory
   ```

   Expected: a `test result: ok.` line reading `2 passed; 0 failed`.

6. Run the gate. Expected: all exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/manifest/memory.rs crates/buildl/src/manifest/mod.rs
   $ git commit -m "feat(buildl): read a memory size written with a unit"
   ```

### Task 3 — Declare the tables of `buildl.toml`

**Files:**
- Create `crates/buildl/src/manifest/schema.rs`
- Modify `crates/buildl/src/manifest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/manifest/schema.rs` with its documentation and its tests:

   ````rust
   //! The tables of `buildl.toml`, as they are written.
   //!
   //! Its own file because what the file may hold is one decision, and it is made here: every
   //! table and every key is optional, and a key no table declares is an error. The manifest is the
   //! ceiling on what a build may do, so a misspelt key must not silently drop a grant. For the
   //! same reason a table is read only from a table: left to itself, the derived reader also takes
   //! an array, by position, and drops the entries it has no field for.
   //!
   //! Responsibilities: [`Raw`] and the table it holds for each section, every value kept with the
   //! place it was written at.
   //!
   //! Non-responsibilities: what a value means. Each one is text or a number here; whether it
   //! names a directory, a program or a length of time is decided where the tables are read into
   //! values.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Raw;

       #[test]
       fn a_document_with_no_table_leaves_every_key_out() {
           for text in ["", "# nothing\n\n", "[workspace]\n[ceiling]\n"] {
               let raw: Raw = toml::from_str(text).unwrap();
               assert!(raw.workspace.name.is_none(), "{text:?}");
               assert!(raw.workspace.entry.is_none(), "{text:?}");
               assert!(raw.workspace.out.is_none(), "{text:?}");
               assert!(raw.workspace.config.is_none(), "{text:?}");
               assert!(raw.ceiling.proc.run.is_empty(), "{text:?}");
               assert!(raw.ceiling.env.read.is_empty(), "{text:?}");
               assert!(raw.ceiling.fs.read.is_empty(), "{text:?}");
               assert!(raw.ceiling.fs.write.is_empty(), "{text:?}");
               assert!(raw.declaration.memory.is_none(), "{text:?}");
               assert!(raw.declaration.instructions.is_none(), "{text:?}");
               assert!(raw.declaration.walk_entries.is_none(), "{text:?}");
               assert!(raw.execution.workers.is_none(), "{text:?}");
               assert!(raw.execution.timeout.is_none(), "{text:?}");
               assert!(raw.execution.strategy.is_none(), "{text:?}");
           }
       }

       #[test]
       fn a_value_keeps_the_place_it_was_written_at() {
           let text = "[workspace]\nout = \"gen\"\n\n[ceiling]\nproc.run = [\"cc\", \"ld\"]\n\n[execution]\nworkers = 4\n";
           let raw: Raw = toml::from_str(text).unwrap();

           let out = raw.workspace.out.unwrap();
           assert_eq!(out.get_ref(), "gen");
           assert_eq!(&text[out.span()], "\"gen\"");

           let run: Vec<&str> = raw
               .ceiling
               .proc
               .run
               .iter()
               .map(|program| &text[program.span()])
               .collect();
           assert_eq!(run, ["\"cc\"", "\"ld\""]);

           let workers = raw.execution.workers.unwrap();
           assert_eq!(*workers.get_ref(), 4);
           assert_eq!(&text[workers.span()], "4");
       }

       #[test]
       fn a_table_is_read_from_a_table_and_from_nothing_else() {
           let cases = [
               (
                   "workspace = 3\n",
                   "invalid type: integer `3`, expected a table",
               ),
               (
                   "ceiling = \"open\"\n",
                   "invalid type: string \"open\", expected a table",
               ),
               (
                   "declaration = true\n",
                   "invalid type: boolean `true`, expected a table",
               ),
               (
                   "execution = [4, \"1s\", \"bare\"]\n",
                   "invalid type: sequence, expected a table",
               ),
               (
                   "[ceiling]\nproc = [[\"cc\"], [\"ld\"]]\n",
                   "invalid type: sequence, expected a table",
               ),
               (
                   "[ceiling]\nenv = [[\"HOME\"]]\n",
                   "invalid type: sequence, expected a table",
               ),
               (
                   "[ceiling]\nfs = [[\"src\"], [\"out\"]]\n",
                   "invalid type: sequence, expected a table",
               ),
           ];
           for (text, expected) in cases {
               let error = toml::from_str::<Raw>(text).unwrap_err();
               assert_eq!(error.message(), expected, "{text:?}");
           }
       }

       #[test]
       fn every_table_refuses_a_key_it_does_not_declare() {
           let cases = [
               ("approver = 1\n", "approver"),
               ("[workspace]\nroot = \".\"\n", "root"),
               ("[ceiling]\nnet.connect = []\n", "net"),
               ("[ceiling.proc]\nexec = []\n", "exec"),
               ("[ceiling.env]\nwrite = []\n", "write"),
               ("[ceiling.fs]\nwirte = []\n", "wirte"),
               ("[declaration]\ntime = \"1s\"\n", "time"),
               ("[execution]\nworker = 4\n", "worker"),
           ];
           for (text, key) in cases {
               let error = toml::from_str::<Raw>(text).unwrap_err();
               assert!(
                   error
                       .message()
                       .starts_with(&format!("unknown field `{key}`")),
                   "{text:?}: {}",
                   error.message()
               );
           }
       }
   }
   ````

   The first test reads every field of every table, which is also what keeps a field from being dead code in the build with tests. The third is the proof of what **Context** says about arrays: each of its last four texts was accepted before `table` existed.

2. Replace the whole content of `crates/buildl/src/manifest/mod.rs` with:

   ````rust
   //! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
   //!
   //! Its own directory because the manifest is the one file that is both the marker of a
   //! workspace root and the ceiling on what a build may do, and no other module of this crate
   //! knows its name or its format.
   //!
   //! Responsibilities:
   //!
   //! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
   //!   started.
   //!
   //! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
   //! belong to the pipeline, which is handed what the manifest says as a value.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the manifest reader, which is not written yet"
       )
   )]
   mod memory;
   mod root;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "read by the manifest reader, which is not written yet"
       )
   )]
   mod schema;

   pub use root::WorkspaceRoot;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::schema
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::Raw`
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. In `crates/buildl/src/manifest/schema.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use core::fmt;
   use core::marker::PhantomData;

   use serde::Deserialize;
   use serde::de::value::MapAccessDeserializer;
   use serde::de::{Deserializer, MapAccess, Visitor};
   use toml::Spanned;

   /// Reads a table from a table and from nothing else.
   fn table<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
   where
       D: Deserializer<'de>,
       T: Deserialize<'de>,
   {
       deserializer.deserialize_map(TableOnly(PhantomData))
   }

   /// Takes a map of keys and values, and refuses every other form a value can have.
   struct TableOnly<T>(PhantomData<T>);

   impl<'de, T: Deserialize<'de>> Visitor<'de> for TableOnly<T> {
       type Value = T;

       fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str("a table")
       }

       fn visit_map<A: MapAccess<'de>>(self, map: A) -> core::result::Result<T, A::Error> {
           T::deserialize(MapAccessDeserializer::new(map))
       }
   }

   /// The whole document.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct Raw {
       /// `[workspace]`.
       #[serde(default, deserialize_with = "table")]
       pub(crate) workspace: WorkspaceTable,
       /// `[ceiling]`.
       #[serde(default, deserialize_with = "table")]
       pub(crate) ceiling: CeilingTable,
       /// `[declaration]`.
       #[serde(default, deserialize_with = "table")]
       pub(crate) declaration: DeclarationTable,
       /// `[execution]`.
       #[serde(default, deserialize_with = "table")]
       pub(crate) execution: ExecutionTable,
   }

   /// `[workspace]`: what the workspace is called and where its files go.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct WorkspaceTable {
       /// A name for people.
       pub(crate) name: Option<Spanned<String>>,
       /// The file name of every directory's build file.
       pub(crate) entry: Option<Spanned<String>>,
       /// The directory every output is placed under.
       pub(crate) out: Option<Spanned<String>>,
       /// The version of this file's own format.
       pub(crate) config: Option<Spanned<i64>>,
   }

   /// `[ceiling]`: the most any build of the workspace may be granted.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct CeilingTable {
       /// `proc`: the programs.
       #[serde(default, deserialize_with = "table")]
       pub(crate) proc: ProcTable,
       /// `env`: the environment variables.
       #[serde(default, deserialize_with = "table")]
       pub(crate) env: EnvTable,
       /// `fs`: the directories.
       #[serde(default, deserialize_with = "table")]
       pub(crate) fs: FsTable,
   }

   /// `[ceiling.proc]`.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct ProcTable {
       /// The programs a command may run.
       #[serde(default)]
       pub(crate) run: Vec<Spanned<String>>,
   }

   /// `[ceiling.env]`.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct EnvTable {
       /// The environment variables a target may read.
       #[serde(default)]
       pub(crate) read: Vec<Spanned<String>>,
   }

   /// `[ceiling.fs]`.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct FsTable {
       /// The directories inputs may come from.
       #[serde(default)]
       pub(crate) read: Vec<Spanned<String>>,
       /// The directories a build may write under.
       #[serde(default)]
       pub(crate) write: Vec<Spanned<String>>,
   }

   /// `[declaration]`: the ceilings one build file is evaluated under.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct DeclarationTable {
       /// The memory ceiling, written with a unit.
       pub(crate) memory: Option<Spanned<String>>,
       /// The instruction ceiling.
       pub(crate) instructions: Option<Spanned<i64>>,
       /// The ceiling on directory entries a source listing may visit.
       pub(crate) walk_entries: Option<Spanned<i64>>,
   }

   /// `[execution]`: how actions are run.
   #[derive(Debug, Default, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct ExecutionTable {
       /// How many actions run at once; `0` is one per core.
       pub(crate) workers: Option<Spanned<i64>>,
       /// How long one action may run, written with a unit.
       pub(crate) timeout: Option<Spanned<String>>,
       /// How strongly actions are isolated.
       pub(crate) strategy: Option<Spanned<String>>,
   }
   ````

   Every table and every key is optional: a table field has `default`, a value field is an `Option` or a list with `default`. Every table has `deny_unknown_fields`. Every field that holds a table is read with `table`, so a table is taken from a table and from nothing else. The document itself needs no such guard: the root of a TOML file is always a table. `toml::Spanned` keeps the byte range each value was written at.

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::schema
   ```

   Expected: a `test result: ok.` line reading `4 passed; 0 failed`.

6. Run the gate. Expected: all exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/manifest/schema.rs crates/buildl/src/manifest/mod.rs
   $ git commit -m "feat(buildl): declare the tables of buildl.toml"
   ```

### Task 4 — Place a refusal at its line and column

**Files:**
- Create `crates/buildl/src/manifest/interpret.rs`
- Modify `crates/buildl/src/manifest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/manifest/interpret.rs` with its documentation and its tests:

   ````rust
   //! From the manifest's tables to the values buildl works with, and where a refusal is.
   //!
   //! Its own file because reading a table and understanding it are two steps: the tables hold
   //! text and numbers, and each one becomes a value only through the type that checks it. A value
   //! its type refuses is reported with the line and the column it was written at, which only the
   //! step that still holds the manifest's text can say.
   //!
   //! Responsibilities: [`position`], and the error for a document the parser refused,
   //! [`unparsed`].
   //!
   //! Non-responsibilities: the shape of the tables, and reading the file they come from.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{located, position, unparsed};
       use crate::manifest::schema::Raw;

       #[test]
       fn a_byte_offset_becomes_a_line_and_a_column_in_characters() {
           let text = "# é\nkey = \"ü\" # ok\n\nlast";
           let cases = [
               (0, (1, 1)),
               (2, (1, 3)),
               (3, (1, 3)),
               (4, (1, 4)),
               (5, (2, 1)),
               (11, (2, 7)),
               (14, (2, 9)),
               (21, (3, 1)),
               (22, (4, 1)),
               (26, (4, 5)),
               (99, (4, 5)),
           ];
           for (offset, expected) in cases {
               assert_eq!(position(text, offset), expected, "offset {offset}");
           }
           assert_eq!(position("", 0), (1, 1));
       }

       #[test]
       fn what_the_parser_refuses_is_one_line_at_its_position() {
           let cases = [
               (
                   "[workspace\nout = \"a\"\n",
                   "manifest: buildl.toml:1:11: invalid table header: expected `.`, `]`",
               ),
               (
                   "# é\n[workspace]\nout = \"a\"\nout = \"b\"\n",
                   "manifest: buildl.toml:4:1: duplicate key `out` in table `workspace`",
               ),
           ];
           for (text, expected) in cases {
               let error = toml::from_str::<Raw>(text).unwrap_err();
               assert_eq!(unparsed(text, &error).to_string(), expected, "{text:?}");
           }
       }

       #[test]
       fn a_failure_with_no_place_names_the_file_alone() {
           assert_eq!(
               located("", None, "not UTF-8 text").to_string(),
               "manifest: buildl.toml: not UTF-8 text"
           );
       }
   }
   ````

   In the first test's text, `é` and `ü` are two bytes each: offset 3 is inside `é`, and offset 14 is the eighth character of line 2, so its column is 9. The two parser messages in the second test are what toml 0.8.23 prints; the first is two lines from the parser (`invalid table header` and ``expected `.`, `]` ``), joined with `: `.

2. Replace the whole content of `crates/buildl/src/manifest/mod.rs` with:

   ````rust
   //! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
   //!
   //! Its own directory because the manifest is the one file that is both the marker of a
   //! workspace root and the ceiling on what a build may do, and no other module of this crate
   //! knows its name or its format.
   //!
   //! Responsibilities:
   //!
   //! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
   //!   started.
   //!
   //! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
   //! belong to the pipeline, which is handed what the manifest says as a value.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the manifest reader, which is not written yet"
       )
   )]
   mod interpret;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the manifest reader, which is not written yet"
       )
   )]
   mod memory;
   mod root;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "read by the manifest reader, which is not written yet"
       )
   )]
   mod schema;

   pub use root::WorkspaceRoot;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::interpret
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::located`, `super::position`, `super::unparsed`
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. In `crates/buildl/src/manifest/interpret.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use core::ops::Range;

   use buildl_core::{Diagnostic, Error, PortName};

   use crate::manifest::root::MANIFEST_FILE;

   /// The line and the column of the byte at `offset` in `text`, both counted from 1, the column
   /// in characters.
   ///
   /// An offset past the end is the end, and one inside a character is that character.
   pub(crate) fn position(text: &str, offset: usize) -> (usize, usize) {
       let mut end = offset.min(text.len());
       while !text.is_char_boundary(end) {
           end -= 1;
       }
       let before = text.get(..end).unwrap_or(text);
       let line = before.matches('\n').count() + 1;
       let column = before
           .rsplit('\n')
           .next()
           .map_or(0, |last| last.chars().count())
           + 1;
       (line, column)
   }

   /// What the parser refused: text that is not TOML, a key no table declares, or a value of the
   /// wrong type. The parser's own message, on one line.
   pub(crate) fn unparsed(text: &str, error: &toml::de::Error) -> Error {
       located(text, error.span(), &error.message().replace('\n', ": "))
   }

   /// The manifest's error for `message`, with the line and the column `span` starts at when the
   /// place is known.
   fn located(text: &str, span: Option<Range<usize>>, message: &str) -> Error {
       let place = span.map_or_else(String::new, |span| {
           let (line, column) = position(text, span.start);
           format!(":{line}:{column}")
       });
       Error::Adapter {
           port: PortName::Manifest,
           diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}{place}: {message}")),
       }
   }
   ````

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::interpret
   ```

   Expected: a `test result: ok.` line reading `3 passed; 0 failed`.

6. Run the gate. Expected: all exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/manifest/interpret.rs crates/buildl/src/manifest/mod.rs
   $ git commit -m "feat(buildl): place a manifest refusal at its line and column"
   ```

### Task 5 — Read the directories of a manifest

**Files:**
- Modify `crates/buildl/src/manifest/interpret.rs`

**Steps:**

1. In `crates/buildl/src/manifest/interpret.rs`, replace the tests module, everything from the line `#[cfg(test)]` to the end of the file, with:

   ````rust
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use buildl_core::Directory;

       use super::{directories, located, out, position, unparsed};
       use crate::manifest::schema::Raw;

       fn raw(text: &str) -> Raw {
           toml::from_str(text).unwrap()
       }

       fn dir(text: &str) -> Directory {
           Directory::parse(text).unwrap()
       }

       #[test]
       fn a_byte_offset_becomes_a_line_and_a_column_in_characters() {
           let text = "# é\nkey = \"ü\" # ok\n\nlast";
           let cases = [
               (0, (1, 1)),
               (2, (1, 3)),
               (3, (1, 3)),
               (4, (1, 4)),
               (5, (2, 1)),
               (11, (2, 7)),
               (14, (2, 9)),
               (21, (3, 1)),
               (22, (4, 1)),
               (26, (4, 5)),
               (99, (4, 5)),
           ];
           for (offset, expected) in cases {
               assert_eq!(position(text, offset), expected, "offset {offset}");
           }
           assert_eq!(position("", 0), (1, 1));
       }

       #[test]
       fn what_the_parser_refuses_is_one_line_at_its_position() {
           let cases = [
               (
                   "[workspace\nout = \"a\"\n",
                   "manifest: buildl.toml:1:11: invalid table header: expected `.`, `]`",
               ),
               (
                   "# é\n[workspace]\nout = \"a\"\nout = \"b\"\n",
                   "manifest: buildl.toml:4:1: duplicate key `out` in table `workspace`",
               ),
           ];
           for (text, expected) in cases {
               let error = toml::from_str::<Raw>(text).unwrap_err();
               assert_eq!(unparsed(text, &error).to_string(), expected, "{text:?}");
           }
       }

       #[test]
       fn a_failure_with_no_place_names_the_file_alone() {
           assert_eq!(
               located("", None, "not UTF-8 text").to_string(),
               "manifest: buildl.toml: not UTF-8 text"
           );
       }

       #[test]
       fn dot_is_the_root_and_empty_text_is_refused() {
           let text = "[ceiling]\nfs.read = [\".\", \"src\", \"lib/text\", \"src\"]\n";
           let read = directories(text, "ceiling.fs.read", &raw(text).ceiling.fs.read).unwrap();
           assert_eq!(
               read.into_iter().collect::<Vec<_>>(),
               [Directory::root(), dir("lib/text"), dir("src")]
           );

           let cases = [
               (
                   "[ceiling]\nfs.write = [\"out\", \"\"]\n",
                   "manifest: buildl.toml:2:20: ceiling.fs.write: invalid directory: \"\" — must not be empty; the workspace root is written \".\"",
               ),
               (
                   "[ceiling]\nfs.write = [\"../x\"]\n",
                   "manifest: buildl.toml:2:13: ceiling.fs.write: invalid directory: \"../x\" — must not contain a '.' or '..' segment",
               ),
               (
                   "[ceiling.fs]\nwrite = [\n  \"out\",\n  \"/tmp\",\n]\n",
                   "manifest: buildl.toml:4:3: ceiling.fs.write: invalid directory: \"/tmp\" — must not start or end with '/'",
               ),
           ];
           for (text, expected) in cases {
               let error =
                   directories(text, "ceiling.fs.write", &raw(text).ceiling.fs.write).unwrap_err();
               assert_eq!(error.to_string(), expected, "{text:?}");
           }
       }

       #[test]
       fn out_may_not_be_the_root_or_overlap_the_state_directory() {
           let state = dir(".buildl");
           assert_eq!(out("", None, &state).unwrap(), dir("out"));
           let text = "[workspace]\nout = \"gen/files\"\n";
           assert_eq!(
               out(text, raw(text).workspace.out.as_ref(), &state).unwrap(),
               dir("gen/files")
           );

           let overlap = "must not be the state directory .buildl, hold it or lie inside it";
           let cases = [
               (
                   "",
                   "invalid output directory: \"\" — must not be the workspace root".to_owned(),
               ),
               (
                   ".buildl",
                   format!("invalid output directory: \".buildl\" — {overlap}"),
               ),
               (
                   ".buildl/out",
                   format!("invalid output directory: \".buildl/out\" — {overlap}"),
               ),
               (
                   ".",
                   "invalid directory: \".\" — must not contain a '.' or '..' segment".to_owned(),
               ),
               (
                   "../x",
                   "invalid directory: \"../x\" — must not contain a '.' or '..' segment".to_owned(),
               ),
           ];
           for (written, reason) in cases {
               let text = format!("[workspace]\nout = {written:?}\n");
               let error = out(&text, raw(&text).workspace.out.as_ref(), &state).unwrap_err();
               assert_eq!(
                   error.to_string(),
                   format!("manifest: buildl.toml:2:7: workspace.out: {reason}"),
                   "{written:?}"
               );
           }
       }
   }
   ````

   The first three tests are task 4's, unchanged. The two new ones read their values out of a parsed document, so each refusal carries a real position: in `[ceiling]\nfs.write = ["out", ""]\n` the empty string opens at column 20 of line 2.

2. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::interpret
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::directories`, `super::out`
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

3. Replace everything above the line `#[cfg(test)]` with the following, leaving one blank line between its last line and `#[cfg(test)]`:

   ````rust
   //! From the manifest's tables to the values buildl works with, and where a refusal is.
   //!
   //! Its own file because reading a table and understanding it are two steps: the tables hold
   //! text and numbers, and each one becomes a value only through the type that checks it. A value
   //! its type refuses is reported with the line and the column it was written at, which only the
   //! step that still holds the manifest's text can say.
   //!
   //! Responsibilities: [`position`]; the error for a document the parser refused, [`unparsed`];
   //! the directories of a ceiling list, [`directories`]; and the output directory, [`out`].
   //!
   //! Non-responsibilities: the shape of the tables, and reading the file they come from.

   use core::fmt;
   use core::ops::Range;
   use std::collections::BTreeSet;

   use buildl_core::{Diagnostic, Directory, Error, PortName, Result};
   use toml::Spanned;

   use crate::manifest::root::MANIFEST_FILE;

   /// The directory outputs are placed under when the manifest names none.
   const DEFAULT_OUT: &str = "out";

   /// How a ceiling list writes the workspace root.
   const ROOT: &str = ".";

   /// The line and the column of the byte at `offset` in `text`, both counted from 1, the column
   /// in characters.
   ///
   /// An offset past the end is the end, and one inside a character is that character.
   pub(crate) fn position(text: &str, offset: usize) -> (usize, usize) {
       let mut end = offset.min(text.len());
       while !text.is_char_boundary(end) {
           end -= 1;
       }
       let before = text.get(..end).unwrap_or(text);
       let line = before.matches('\n').count() + 1;
       let column = before
           .rsplit('\n')
           .next()
           .map_or(0, |last| last.chars().count())
           + 1;
       (line, column)
   }

   /// What the parser refused: text that is not TOML, a key no table declares, or a value of the
   /// wrong type. The parser's own message, on one line.
   pub(crate) fn unparsed(text: &str, error: &toml::de::Error) -> Error {
       located(text, error.span(), &error.message().replace('\n', ": "))
   }

   /// The directories the ceiling list `key` names, as a set: `.` is the workspace root, and a
   /// directory named twice is held once.
   pub(crate) fn directories(
       text: &str,
       key: &str,
       entries: &[Spanned<String>],
   ) -> Result<BTreeSet<Directory>> {
       set(text, key, entries, directory)
   }

   /// The directory outputs are placed under: the one `written`, or `out` when the manifest names
   /// none. It is never the workspace root, and never `state`, a directory holding it or one
   /// inside it.
   pub(crate) fn out(
       text: &str,
       written: Option<&Spanned<String>>,
       state: &Directory,
   ) -> Result<Directory> {
       let named = optional(text, "workspace.out", written, |raw| {
           let out = Directory::parse(raw.as_str()).map_err(|error| error.to_string())?;
           if out.is_root() {
               return Err(invalid(
                   "output directory",
                   raw,
                   "must not be the workspace root",
               ));
           }
           if out.contains(state) || state.contains(&out) {
               return Err(invalid(
                   "output directory",
                   raw,
                   &format!("must not be the state directory {state}, hold it or lie inside it"),
               ));
           }
           Ok(out)
       })?;
       named.map_or_else(|| Directory::parse(DEFAULT_OUT), Ok)
   }

   /// The directory one entry of a ceiling list names.
   fn directory(raw: &str) -> core::result::Result<Directory, String> {
       match raw {
           ROOT => Ok(Directory::root()),
           "" => Err(invalid(
               "directory",
               &raw,
               "must not be empty; the workspace root is written \".\"",
           )),
           other => Directory::parse(other).map_err(|error| error.to_string()),
       }
   }

   /// Every entry of the list `key`, read by `read`, as a set.
   fn set<T: Ord, E: fmt::Display>(
       text: &str,
       key: &str,
       entries: &[Spanned<String>],
       read: impl Fn(&str) -> core::result::Result<T, E>,
   ) -> Result<BTreeSet<T>> {
       entries
           .iter()
           .map(|entry| read(entry.get_ref()).map_err(|reason| refused(text, key, entry, &reason)))
           .collect()
   }

   /// The value `written` for `key`, read by `read`; `None` when the manifest leaves the key out.
   fn optional<W, T, E: fmt::Display>(
       text: &str,
       key: &str,
       written: Option<&Spanned<W>>,
       read: impl FnOnce(&W) -> core::result::Result<T, E>,
   ) -> Result<Option<T>> {
       written
           .map(|value| read(value.get_ref()).map_err(|reason| refused(text, key, value, &reason)))
           .transpose()
   }

   /// Why a value is refused, in the words every name's own refusal uses: what was written, then
   /// the reason.
   fn invalid(kind: &str, value: &dyn fmt::Debug, reason: &str) -> String {
       format!("invalid {kind}: {value:?} — {reason}")
   }

   /// The manifest's error for a `value` of `key` that is refused for `reason`, at the place the
   /// value was written.
   fn refused<W>(text: &str, key: &str, value: &Spanned<W>, reason: &dyn fmt::Display) -> Error {
       located(text, Some(value.span()), &format!("{key}: {reason}"))
   }

   /// The manifest's error for `message`, with the line and the column `span` starts at when the
   /// place is known.
   fn located(text: &str, span: Option<Range<usize>>, message: &str) -> Error {
       let place = span.map_or_else(String::new, |span| {
           let (line, column) = position(text, span.start);
           format!(":{line}:{column}")
       });
       Error::Adapter {
           port: PortName::Manifest,
           diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}{place}: {message}")),
       }
   }
   ````

   In a ceiling list `.` is the workspace root, because `Directory::parse(".")` is an error and a refusal prints the root as `.`. The empty string is refused there although `Directory::parse("")` is the root: an entry that grants the whole workspace has to be written. A list is a set, so a directory named twice is held once. `set` and `optional` are the two helpers every key goes through: they run a reader over what was written and, when it refuses, place the refusal at the value.

4. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::interpret
   ```

   Expected: a `test result: ok.` line reading `5 passed; 0 failed`.

5. Run the gate. Expected: all exit `0`.

6. Commit:

   ```
   $ git add crates/buildl/src/manifest/interpret.rs
   $ git commit -m "feat(buildl): read the directories of a manifest"
   ```

### Task 6 — Read `buildl.toml` into a workspace

**Files:**
- Create `crates/buildl/src/manifest/toml_manifest.rs`
- Modify `crates/buildl/src/manifest/interpret.rs`
- Modify `crates/buildl/src/manifest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/manifest/toml_manifest.rs` with its documentation and its tests:

   ````rust
   //! [`TomlManifest`], the [`Manifest`] read from a workspace's `buildl.toml`.
   //!
   //! Its own file because it is the adapter's entry point: it turns the manifest's text into the
   //! three values the rest of buildl is assembled from, once, and refuses the whole file when any
   //! part of it is wrong. A manifest is the ceiling on what a build may do, so nothing in it is
   //! skipped and nothing is guessed.
   //!
   //! Responsibilities: [`TomlManifest`], its [`Manifest`] implementation, and
   //! [`DeclarationSettings`].
   //!
   //! Non-responsibilities: the defaults for the ceilings a build file is evaluated under. A
   //! ceiling the manifest leaves out stays absent here, and whoever evaluates build files supplies
   //! its own default.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use core::num::NonZeroU64;
       use std::collections::BTreeSet;

       use buildl_core::{
           Directory, EnvName, Error, Execution, Isolation, Manifest, PortName, Program, Timeout,
           Workers,
       };

       use super::{DeclarationSettings, TomlManifest};

       /// The manifest the design shows, without the table for approving grants.
       const DESIGN_EXAMPLE: &str = r#"[workspace]
   name    = "myproject"
   entry   = "build.lua"
   out     = "out"
   config  = 1                # buildl config version

   [ceiling]                  # maximum authority any build may be granted
   proc.run = ["cc", "ld", "go", "cargo", "bun", "docker", "protoc"]
   env.read = ["HOME", "PATH", "GOCACHE", "CARGO_HOME"]
   fs.read  = ["."]           # workspace-relative; expanded by the host, never by Lua
   fs.write = ["out", ".buildl", "target"]

   [declaration]              # ceilings for the Lua phase — it only declares
   memory       = "16MB"
   instructions = 10_000_000
   walk_entries = 100_000     # directory entries b.sources() may visit per build file

   [execution]
   workers  = 0               # 0 = number of cores
   timeout  = "10m"           # per action
   strategy = "sandbox"       # bare | sandbox | os-sandbox | container
   "#;

       fn dir(text: &str) -> Directory {
           Directory::parse(text).unwrap()
       }

       fn refusal(text: &str) -> String {
           TomlManifest::parse(text).unwrap_err().to_string()
       }

       #[test]
       fn the_design_example_reads_into_a_workspace() {
           let manifest = TomlManifest::parse(DESIGN_EXAMPLE).unwrap();
           let workspace = manifest.workspace().unwrap();

           assert_eq!(workspace.out, dir("out"));
           assert_eq!(workspace.state, dir(".buildl"));
           assert_eq!(
               workspace.ceiling.run,
               ["cc", "ld", "go", "cargo", "bun", "docker", "protoc"]
                   .into_iter()
                   .map(|program| Program::parse(program).unwrap())
                   .collect::<BTreeSet<_>>()
           );
           assert_eq!(
               workspace.ceiling.env,
               ["HOME", "PATH", "GOCACHE", "CARGO_HOME"]
                   .into_iter()
                   .map(|name| EnvName::parse(name).unwrap())
                   .collect::<BTreeSet<_>>()
           );
           assert_eq!(workspace.ceiling.read, BTreeSet::from([Directory::root()]));
           assert_eq!(
               workspace.ceiling.write,
               BTreeSet::from([dir("out"), dir(".buildl"), dir("target")])
           );
           assert_eq!(
               workspace.execution,
               Execution {
                   workers: Workers::PerCore,
                   timeout: Timeout::parse("10m").unwrap(),
                   strategy: Isolation::Sandbox,
               }
           );
           assert_eq!(manifest.entry().as_str(), "build.lua");
           assert_eq!(
               manifest.declaration(),
               DeclarationSettings {
                   memory_bytes: NonZeroU64::new(16 * 1024 * 1024),
                   instructions: NonZeroU64::new(10_000_000),
                   walk_entries: NonZeroU64::new(100_000),
               }
           );
           assert_eq!(manifest.workspace().unwrap(), workspace);
       }

       #[test]
       fn an_empty_file_gives_every_default() {
           for text in [
               "",
               "# nothing\n\n",
               "[workspace]\n[ceiling]\n[declaration]\n[execution]\n",
           ] {
               let manifest = TomlManifest::parse(text).unwrap();
               let workspace = manifest.workspace().unwrap();
               assert_eq!(workspace.out, dir("out"), "{text:?}");
               assert_eq!(workspace.state, dir(".buildl"), "{text:?}");
               assert!(workspace.ceiling.run.is_empty(), "{text:?}");
               assert!(workspace.ceiling.env.is_empty(), "{text:?}");
               assert!(workspace.ceiling.read.is_empty(), "{text:?}");
               assert!(workspace.ceiling.write.is_empty(), "{text:?}");
               assert_eq!(workspace.execution, Execution::default(), "{text:?}");
               assert_eq!(workspace.execution.workers, Workers::PerCore, "{text:?}");
               assert_eq!(
                   workspace.execution.timeout,
                   Timeout::parse("10m").unwrap(),
                   "{text:?}"
               );
               assert_eq!(workspace.execution.strategy, Isolation::Sandbox, "{text:?}");
               assert_eq!(manifest.entry().as_str(), "build.lua", "{text:?}");
               assert_eq!(
                   manifest.declaration(),
                   DeclarationSettings::default(),
                   "{text:?}"
               );
           }
       }

       #[test]
       fn a_key_that_is_written_replaces_its_default() {
           let manifest = TomlManifest::parse(
               r#"[workspace]
   entry = "rules.lua"
   out = "gen/files"

   [declaration]
   memory = "1GB"
   instructions = 5
   walk_entries = 7

   [execution]
   workers = 4
   timeout = "90s"
   strategy = "bare"
   "#,
           )
           .unwrap();
           let workspace = manifest.workspace().unwrap();
           assert_eq!(workspace.out, dir("gen/files"));
           assert_eq!(workspace.state, dir(".buildl"));
           assert_eq!(
               workspace.execution,
               Execution {
                   workers: Workers::of(4),
                   timeout: Timeout::parse("90s").unwrap(),
                   strategy: Isolation::Bare,
               }
           );
           assert_eq!(manifest.entry().as_str(), "rules.lua");
           assert_eq!(
               manifest.declaration(),
               DeclarationSettings {
                   memory_bytes: NonZeroU64::new(1024 * 1024 * 1024),
                   instructions: NonZeroU64::new(5),
                   walk_entries: NonZeroU64::new(7),
               }
           );
       }

       #[test]
       fn neither_the_output_nor_the_state_directory_is_granted_unwritten() {
           let manifest = TomlManifest::parse("[ceiling]\nfs.write = [\"target\"]\n").unwrap();
           assert_eq!(
               manifest.workspace().unwrap().ceiling.write,
               BTreeSet::from([dir("target")])
           );
       }

       #[test]
       fn an_unknown_key_is_refused_at_its_position() {
           let cases = [
               (
                   "[workspace]\nout = \"out\"\n\n[approver]\ndefault = \"interactive\"\n",
                   "manifest: buildl.toml:4:2: unknown field `approver`, expected one of `workspace`, `ceiling`, `declaration`, `execution`",
               ),
               (
                   "[ceiling]\nfs.read = [\".\"]\nfs.wirte = [\"out\"]\n",
                   "manifest: buildl.toml:3:4: unknown field `wirte`, expected `read` or `write`",
               ),
               (
                   "[ceiling]\nnet.connect = [\"example.com\"]\n",
                   "manifest: buildl.toml:2:1: unknown field `net`, expected one of `proc`, `env`, `fs`",
               ),
               (
                   "[execution]\nworker = 4\n",
                   "manifest: buildl.toml:2:1: unknown field `worker`, expected one of `workers`, `timeout`, `strategy`",
               ),
               (
                   "out = \"out\"\n",
                   "manifest: buildl.toml:1:1: unknown field `out`, expected one of `workspace`, `ceiling`, `declaration`, `execution`",
               ),
           ];
           for (text, expected) in cases {
               assert_eq!(refusal(text), expected, "{text:?}");
           }
       }

       #[test]
       fn a_value_of_the_wrong_type_is_refused_at_its_position() {
           let cases = [
               (
                   "[execution]\nworkers = \"4\"\n",
                   "manifest: buildl.toml:2:11: invalid type: string \"4\", expected i64",
               ),
               (
                   "[ceiling]\nproc.run = \"cc\"\n",
                   "manifest: buildl.toml:2:12: invalid type: string \"cc\", expected a sequence",
               ),
               (
                   "[execution]\ntimeout = 10\n",
                   "manifest: buildl.toml:2:11: invalid type: integer `10`, expected a string",
               ),
               (
                   "[execution]\nworkers = 1.5\n",
                   "manifest: buildl.toml:2:11: invalid type: floating point `1.5`, expected i64",
               ),
               (
                   "[ceiling]\nproc.run = [\"cc\", 7]\n",
                   "manifest: buildl.toml:2:19: invalid type: integer `7`, expected a string",
               ),
               (
                   "ceiling = 3\n",
                   "manifest: buildl.toml:1:11: invalid type: integer `3`, expected a table",
               ),
               (
                   "execution = [4, \"1s\", \"bare\"]\n",
                   "manifest: buildl.toml:1:13: invalid type: sequence, expected a table",
               ),
               (
                   "[ceiling]\nproc = [[\"cc\"], [\"ld\"]]\n",
                   "manifest: buildl.toml:2:8: invalid type: sequence, expected a table",
               ),
               (
                   "[declaration]\nmemory = 16\n",
                   "manifest: buildl.toml:2:10: invalid type: integer `16`, expected a string",
               ),
               (
                   "[declaration]\ninstructions = 99999999999999999999\n",
                   "manifest: buildl.toml:2:16: number too large to fit in target type",
               ),
           ];
           for (text, expected) in cases {
               assert_eq!(refusal(text), expected, "{text:?}");
           }
       }

       #[test]
       fn a_value_its_type_rejects_names_the_key_and_the_reason() {
           let cases = [
               (
                   "[execution]\nworkers = 2\ntimeout = \"10\"\n",
                   "manifest: buildl.toml:3:11: execution.timeout: invalid timeout: \"10\" — must end in one of the units ms, s, m and h",
               ),
               (
                   "[execution]\nstrategy = \"docker\"\n",
                   "manifest: buildl.toml:2:12: execution.strategy: invalid execution strategy: \"docker\" — must be one of bare, sandbox, os-sandbox and container",
               ),
               (
                   "[execution]\nworkers = -1\n",
                   "manifest: buildl.toml:2:11: execution.workers: invalid worker count: \"-1\" — must be a whole number, written in digits only",
               ),
               (
                   "[workspace]\nconfig = 2\n",
                   "manifest: buildl.toml:2:10: workspace.config: invalid config version: 2 — must be 1",
               ),
               (
                   "[ceiling]\nenv.read = [\"HOME\", \"MY-VAR\"]\n",
                   "manifest: buildl.toml:2:21: ceiling.env.read: invalid environment variable name: \"MY-VAR\" — may hold only ASCII letters, digits and '_'",
               ),
               (
                   "[ceiling]\nfs.read = [\"../x\"]\n",
                   "manifest: buildl.toml:2:12: ceiling.fs.read: invalid directory: \"../x\" — must not contain a '.' or '..' segment",
               ),
               (
                   "[ceiling]\nfs.write = [\"\"]\n",
                   "manifest: buildl.toml:2:13: ceiling.fs.write: invalid directory: \"\" — must not be empty; the workspace root is written \".\"",
               ),
               (
                   "[ceiling]\nproc.run = [\"cc\", \"$CC\"]\n",
                   "manifest: buildl.toml:2:19: ceiling.proc.run: invalid program: \"$CC\" — must not hold '$'",
               ),
               (
                   "[workspace]\nname = \"\"\n",
                   "manifest: buildl.toml:2:8: workspace.name: invalid workspace name: \"\" — must not be empty",
               ),
               (
                   "[workspace]\nentry = \"lua/build.lua\"\n",
                   "manifest: buildl.toml:2:9: workspace.entry: invalid entry file name: \"lua/build.lua\" — may hold only ASCII letters, digits, '.', '-' and '_'",
               ),
               (
                   "[workspace]\nout = \"\"\n",
                   "manifest: buildl.toml:2:7: workspace.out: invalid output directory: \"\" — must not be the workspace root",
               ),
               (
                   "[workspace]\nout = \".buildl/out\"\n",
                   "manifest: buildl.toml:2:7: workspace.out: invalid output directory: \".buildl/out\" — must not be the state directory .buildl, hold it or lie inside it",
               ),
               (
                   "[declaration]\nmemory = \"16\"\n",
                   "manifest: buildl.toml:2:10: declaration.memory: invalid memory size: \"16\" — must end in one of the units B, KB, MB and GB",
               ),
               (
                   "[declaration]\nmemory = \"0MB\"\n",
                   "manifest: buildl.toml:2:10: declaration.memory: invalid memory size: \"0MB\" — must not be zero",
               ),
               (
                   "[declaration]\ninstructions = 0\n",
                   "manifest: buildl.toml:2:16: declaration.instructions: invalid count: 0 — must be at least 1",
               ),
               (
                   "[declaration]\nwalk_entries = -3\n",
                   "manifest: buildl.toml:2:16: declaration.walk_entries: invalid count: -3 — must be at least 1",
               ),
           ];
           for (text, expected) in cases {
               assert_eq!(refusal(text), expected, "{text:?}");
           }
       }

       #[test]
       fn the_three_spellings_of_a_table_read_alike() {
           let dotted = "[ceiling]\nproc.run = [\"cc\"]\nenv.read = [\"HOME\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n";
           let headers = "[ceiling.proc]\nrun = [\"cc\"]\n\n[ceiling.env]\nread = [\"HOME\"]\n\n[ceiling.fs]\nread = [\".\"]\nwrite = [\"out\", \".buildl\"]\n";
           let inline = "ceiling = { proc = { run = [\"cc\"] }, env = { read = [\"HOME\"] }, fs = { read = [\".\"], write = [\"out\", \".buildl\"] } }\n";

           let expected = TomlManifest::parse(dotted).unwrap();
           let ceiling = expected.workspace().unwrap().ceiling;
           assert_eq!(ceiling.run.len(), 1);
           assert_eq!(ceiling.env.len(), 1);
           assert_eq!(ceiling.read, BTreeSet::from([Directory::root()]));
           assert_eq!(ceiling.write, BTreeSet::from([dir("out"), dir(".buildl")]));

           assert_eq!(TomlManifest::parse(headers).unwrap(), expected);
           assert_eq!(TomlManifest::parse(inline).unwrap(), expected);
       }

       #[test]
       fn every_failure_is_an_adapter_error_naming_the_manifest() {
           let texts = [
               "[workspace\n",
               "[workspace]\nout = \"a\"\nout = \"b\"\n",
               "[approver]\ndefault = \"interactive\"\n",
               "[execution]\nworkers = \"4\"\n",
               "[execution]\ntimeout = \"10\"\n",
               "[workspace]\nconfig = 2\n",
           ];
           for text in texts {
               let error = TomlManifest::parse(text).unwrap_err();
               assert!(
                   matches!(
                       error,
                       Error::Adapter {
                           port: PortName::Manifest,
                           ..
                       }
                   ),
                   "{text:?}: {error:?}"
               );
               let shown = error.to_string();
               assert!(shown.starts_with("manifest: buildl.toml:"), "{shown}");
               assert!(!shown.contains('\n'), "{shown}");
           }
       }
   }
   ````

   `DESIGN_EXAMPLE` is the manifest `docs/design.md` §4 shows, without its `[approver]` table, which this version refuses: the first case of `an_unknown_key_is_refused_at_its_position` is that table. Every expected line is what the code below printed with toml 0.8.23. The first, second, third and fifth cases of the unknown-key test and the first two of the wrong-type test are the lines the design quotes from the parser.

2. Replace the whole content of `crates/buildl/src/manifest/mod.rs` with:

   ````rust
   //! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
   //!
   //! Its own directory because the manifest is the one file that is both the marker of a
   //! workspace root and the ceiling on what a build may do, and no other module of this crate
   //! knows its name or its format.
   //!
   //! Responsibilities:
   //!
   //! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
   //!   started.
   //! - [`TomlManifest`] — the manifest, read and checked once: the workspace it describes, the
   //!   file name of a build file, and the ceilings a build file is evaluated under.
   //! - [`DeclarationSettings`] — those ceilings, each absent when the manifest sets none.
   //!
   //! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
   //! belong to the pipeline, which is handed what the manifest says as a value.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod interpret;
   mod memory;
   mod root;
   mod schema;
   mod toml_manifest;

   pub use root::WorkspaceRoot;
   pub use toml_manifest::{DeclarationSettings, TomlManifest};
   ````

   The three `expect(dead_code)` attributes are gone: `TomlManifest::parse` is public and reaches all three modules.

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::toml_manifest
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::DeclarationSettings`, `super::TomlManifest`
   error[E0432]: unresolved imports `toml_manifest::DeclarationSettings`, `toml_manifest::TomlManifest`
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. In `crates/buildl/src/manifest/toml_manifest.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use core::num::NonZeroU64;

   use buildl_core::{EntryName, Manifest, Result, Workspace};

   use crate::manifest::interpret;
   use crate::manifest::schema::Raw;

   /// A workspace's `buildl.toml`, read and checked once.
   ///
   /// Every table and every key is optional, and a manifest with nothing in it is valid: every
   /// setting takes its default and the ceiling is empty, which allows nothing. A key this version
   /// does not read is an error wherever it stands, and so is a value its type refuses; each is
   /// reported on one line as `buildl.toml:<line>:<column>: <message>`.
   ///
   /// # Examples
   ///
   /// ```
   /// use buildl::manifest::TomlManifest;
   /// use buildl_core::{Isolation, Manifest};
   ///
   /// let manifest = TomlManifest::parse(
   ///     r#"
   /// [ceiling]
   /// proc.run = ["cc"]
   /// fs.write = ["out", ".buildl"]
   ///
   /// [execution]
   /// strategy = "bare"
   /// "#,
   /// )?;
   /// let workspace = manifest.workspace()?;
   /// assert_eq!(workspace.out.as_str(), "out");
   /// assert_eq!(workspace.ceiling.run.len(), 1);
   /// assert_eq!(workspace.execution.strategy, Isolation::Bare);
   /// assert_eq!(manifest.entry().as_str(), "build.lua");
   ///
   /// let typo = TomlManifest::parse("[ceiling]\nfs.wirte = [\"out\"]\n").unwrap_err();
   /// assert_eq!(
   ///     typo.to_string(),
   ///     "manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`"
   /// );
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct TomlManifest {
       workspace: Workspace,
       entry: EntryName,
       declaration: DeclarationSettings,
   }

   impl TomlManifest {
       /// Reads a manifest from its text.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
       /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when `text` is not TOML, holds a
       /// key this version does not read, holds a value of the wrong type, or holds a value its type
       /// refuses.
       pub fn parse(text: &str) -> Result<Self> {
           let raw: Raw = toml::from_str(text).map_err(|error| interpret::unparsed(text, &error))?;
           Ok(Self {
               entry: interpret::entry(text, &raw.workspace)?,
               workspace: interpret::workspace(text, &raw)?,
               declaration: interpret::declaration(text, &raw.declaration)?,
           })
       }

       /// The file name a build file has in every directory.
       #[must_use]
       pub const fn entry(&self) -> &EntryName {
           &self.entry
       }

       /// The declaration ceilings the manifest sets.
       #[must_use]
       pub const fn declaration(&self) -> DeclarationSettings {
           self.declaration
       }
   }

   impl Manifest for TomlManifest {
       /// The workspace as the manifest describes it. Never fails: the manifest was checked when it
       /// was read, and every call returns the same value.
       fn workspace(&self) -> Result<Workspace> {
           Ok(self.workspace.clone())
       }
   }

   /// The `[declaration]` table: each ceiling the manifest sets, `None` for one it leaves out.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub struct DeclarationSettings {
       /// The memory ceiling for evaluating one build file, in bytes.
       pub memory_bytes: Option<NonZeroU64>,
       /// The instruction ceiling for evaluating one build file.
       pub instructions: Option<NonZeroU64>,
       /// The ceiling on directory entries one build file's source listings may visit.
       pub walk_entries: Option<NonZeroU64>,
   }
   ````

5. In `crates/buildl/src/manifest/interpret.rs`, replace everything above the line `#[cfg(test)]` with the following, leaving one blank line between its last line and `#[cfg(test)]`. The tests module stays as task 5 left it:

   ````rust
   //! From the manifest's tables to the values buildl works with, and where a refusal is.
   //!
   //! Its own file because reading a table and understanding it are two steps: the tables hold
   //! text and numbers, and each one becomes a value only through the type that checks it. A value
   //! its type refuses is reported with the line and the column it was written at, which only the
   //! step that still holds the manifest's text can say.
   //!
   //! Responsibilities: the three values a manifest is read into — [`entry`], [`workspace`] and
   //! [`declaration`] — with every default; [`position`]; and the error for a document the parser
   //! refused, [`unparsed`].
   //!
   //! Non-responsibilities: the shape of the tables, and reading the file they come from.

   use core::fmt;
   use core::num::NonZeroU64;
   use core::ops::Range;
   use std::collections::BTreeSet;

   use buildl_core::{
       Ceiling, Diagnostic, Directory, EntryName, EnvName, Error, Execution, Isolation, PortName,
       Program, Result, Timeout, Workers, Workspace,
   };
   use toml::Spanned;

   use crate::manifest::memory::memory_bytes;
   use crate::manifest::root::MANIFEST_FILE;
   use crate::manifest::schema::{
       CeilingTable, DeclarationTable, ExecutionTable, Raw, WorkspaceTable,
   };
   use crate::manifest::toml_manifest::DeclarationSettings;

   /// The file name of a build file when the manifest names none.
   const DEFAULT_ENTRY: &str = "build.lua";

   /// The directory outputs are placed under when the manifest names none.
   const DEFAULT_OUT: &str = "out";

   /// The directory buildl keeps its own files in. The manifest has no key for it.
   const STATE: &str = ".buildl";

   /// The one version of the manifest's own format this code reads.
   const CONFIG: i64 = 1;

   /// How a ceiling list writes the workspace root.
   const ROOT: &str = ".";

   /// The file name a build file has in every directory: `workspace.entry`, or `build.lua`.
   pub(crate) fn entry(text: &str, table: &WorkspaceTable) -> Result<EntryName> {
       optional(text, "workspace.entry", table.entry.as_ref(), |raw| {
           EntryName::parse(raw.as_str())
       })?
       .map_or_else(|| EntryName::parse(DEFAULT_ENTRY), Ok)
   }

   /// The workspace the manifest describes: where its outputs and buildl's own files go, its
   /// ceiling, and how it wants its actions run.
   ///
   /// Nothing is granted that the manifest does not write: neither the output directory nor the
   /// state directory is added to the ceiling.
   pub(crate) fn workspace(text: &str, raw: &Raw) -> Result<Workspace> {
       unused(text, &raw.workspace)?;
       let state = Directory::parse(STATE)?;
       Ok(Workspace {
           out: out(text, raw.workspace.out.as_ref(), &state)?,
           state,
           ceiling: ceiling(text, &raw.ceiling)?,
           execution: execution(text, &raw.execution)?,
       })
   }

   /// The ceilings the manifest sets for evaluating a build file, each `None` when it sets none.
   pub(crate) fn declaration(text: &str, table: &DeclarationTable) -> Result<DeclarationSettings> {
       Ok(DeclarationSettings {
           memory_bytes: optional(text, "declaration.memory", table.memory.as_ref(), |raw| {
               memory_bytes(raw).map_err(|reason| invalid("memory size", raw, reason))
           })?,
           instructions: optional(
               text,
               "declaration.instructions",
               table.instructions.as_ref(),
               |written| count(*written),
           )?,
           walk_entries: optional(
               text,
               "declaration.walk_entries",
               table.walk_entries.as_ref(),
               |written| count(*written),
           )?,
       })
   }

   /// Checks the two keys of `[workspace]` that become no value: a name is not empty, and the
   /// format version is the one this code reads.
   fn unused(text: &str, table: &WorkspaceTable) -> Result<()> {
       optional(text, "workspace.name", table.name.as_ref(), |name| {
           if name.is_empty() {
               return Err(invalid("workspace name", name, "must not be empty"));
           }
           Ok(())
       })?;
       optional(text, "workspace.config", table.config.as_ref(), |version| {
           if *version != CONFIG {
               return Err(invalid("config version", version, "must be 1"));
           }
           Ok(())
       })?;
       Ok(())
   }

   /// The `[ceiling]` table as four sets. A list left out is empty: it allows nothing.
   fn ceiling(text: &str, table: &CeilingTable) -> Result<Ceiling> {
       Ok(Ceiling {
           run: set(text, "ceiling.proc.run", &table.proc.run, |raw| {
               Program::parse(raw)
           })?,
           env: set(text, "ceiling.env.read", &table.env.read, |raw| {
               EnvName::parse(raw)
           })?,
           read: directories(text, "ceiling.fs.read", &table.fs.read)?,
           write: directories(text, "ceiling.fs.write", &table.fs.write)?,
       })
   }

   /// The `[execution]` table, each setting built through its own type and given that type's
   /// default when it is left out.
   fn execution(text: &str, table: &ExecutionTable) -> Result<Execution> {
       Ok(Execution {
           workers: optional(text, "execution.workers", table.workers.as_ref(), |count| {
               Workers::parse(&count.to_string())
           })?
           .unwrap_or_default(),
           timeout: optional(text, "execution.timeout", table.timeout.as_ref(), |raw| {
               Timeout::parse(raw)
           })?
           .unwrap_or_default(),
           strategy: optional(text, "execution.strategy", table.strategy.as_ref(), |raw| {
               Isolation::parse(raw)
           })?
           .unwrap_or_default(),
       })
   }

   /// A ceiling of the declaration phase: a whole number, at least 1.
   fn count(written: i64) -> core::result::Result<NonZeroU64, String> {
       u64::try_from(written)
           .ok()
           .and_then(NonZeroU64::new)
           .ok_or_else(|| invalid("count", &written, "must be at least 1"))
   }

   /// The line and the column of the byte at `offset` in `text`, both counted from 1, the column
   /// in characters.
   ///
   /// An offset past the end is the end, and one inside a character is that character.
   pub(crate) fn position(text: &str, offset: usize) -> (usize, usize) {
       let mut end = offset.min(text.len());
       while !text.is_char_boundary(end) {
           end -= 1;
       }
       let before = text.get(..end).unwrap_or(text);
       let line = before.matches('\n').count() + 1;
       let column = before
           .rsplit('\n')
           .next()
           .map_or(0, |last| last.chars().count())
           + 1;
       (line, column)
   }

   /// What the parser refused: text that is not TOML, a key no table declares, or a value of the
   /// wrong type. The parser's own message, on one line.
   pub(crate) fn unparsed(text: &str, error: &toml::de::Error) -> Error {
       located(text, error.span(), &error.message().replace('\n', ": "))
   }

   /// The directories the ceiling list `key` names, as a set: `.` is the workspace root, and a
   /// directory named twice is held once.
   fn directories(text: &str, key: &str, entries: &[Spanned<String>]) -> Result<BTreeSet<Directory>> {
       set(text, key, entries, directory)
   }

   /// The directory outputs are placed under: the one `written`, or `out` when the manifest names
   /// none. It is never the workspace root, and never `state`, a directory holding it or one
   /// inside it.
   fn out(text: &str, written: Option<&Spanned<String>>, state: &Directory) -> Result<Directory> {
       let named = optional(text, "workspace.out", written, |raw| {
           let out = Directory::parse(raw.as_str()).map_err(|error| error.to_string())?;
           if out.is_root() {
               return Err(invalid(
                   "output directory",
                   raw,
                   "must not be the workspace root",
               ));
           }
           if out.contains(state) || state.contains(&out) {
               return Err(invalid(
                   "output directory",
                   raw,
                   &format!("must not be the state directory {state}, hold it or lie inside it"),
               ));
           }
           Ok(out)
       })?;
       named.map_or_else(|| Directory::parse(DEFAULT_OUT), Ok)
   }

   /// The directory one entry of a ceiling list names.
   fn directory(raw: &str) -> core::result::Result<Directory, String> {
       match raw {
           ROOT => Ok(Directory::root()),
           "" => Err(invalid(
               "directory",
               &raw,
               "must not be empty; the workspace root is written \".\"",
           )),
           other => Directory::parse(other).map_err(|error| error.to_string()),
       }
   }

   /// Every entry of the list `key`, read by `read`, as a set.
   fn set<T: Ord, E: fmt::Display>(
       text: &str,
       key: &str,
       entries: &[Spanned<String>],
       read: impl Fn(&str) -> core::result::Result<T, E>,
   ) -> Result<BTreeSet<T>> {
       entries
           .iter()
           .map(|entry| read(entry.get_ref()).map_err(|reason| refused(text, key, entry, &reason)))
           .collect()
   }

   /// The value `written` for `key`, read by `read`; `None` when the manifest leaves the key out.
   fn optional<W, T, E: fmt::Display>(
       text: &str,
       key: &str,
       written: Option<&Spanned<W>>,
       read: impl FnOnce(&W) -> core::result::Result<T, E>,
   ) -> Result<Option<T>> {
       written
           .map(|value| read(value.get_ref()).map_err(|reason| refused(text, key, value, &reason)))
           .transpose()
   }

   /// Why a value is refused, in the words every name's own refusal uses: what was written, then
   /// the reason.
   fn invalid(kind: &str, value: &dyn fmt::Debug, reason: &str) -> String {
       format!("invalid {kind}: {value:?} — {reason}")
   }

   /// The manifest's error for a `value` of `key` that is refused for `reason`, at the place the
   /// value was written.
   fn refused<W>(text: &str, key: &str, value: &Spanned<W>, reason: &dyn fmt::Display) -> Error {
       located(text, Some(value.span()), &format!("{key}: {reason}"))
   }

   /// The manifest's error for `message`, with the line and the column `span` starts at when the
   /// place is known.
   fn located(text: &str, span: Option<Range<usize>>, message: &str) -> Error {
       let place = span.map_or_else(String::new, |span| {
           let (line, column) = position(text, span.start);
           format!(":{line}:{column}")
       });
       Error::Adapter {
           port: PortName::Manifest,
           diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}{place}: {message}")),
       }
   }
   ````

   What is new: `entry`, `workspace` and `declaration`, the three values `TomlManifest::parse` asks for, with `unused`, `ceiling`, `execution` and `count` below them; `directories` and `out` lose `pub(crate)`, since only this file calls them now. Every default is here and nowhere else: `build.lua`, `out`, `.buildl`, an empty ceiling, and `Execution`'s own defaults. Neither `out` nor `.buildl` is added to `ceiling.fs.write`: a manifest lists them. A declaration ceiling the manifest leaves out stays `None`; whoever evaluates build files has its own defaults.

6. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::toml_manifest
   ```

   Expected: a `test result: ok.` line reading `9 passed; 0 failed`.

7. Run the gate. Expected: all exit `0`.

8. Commit:

   ```
   $ git add crates/buildl/src/manifest/toml_manifest.rs crates/buildl/src/manifest/interpret.rs crates/buildl/src/manifest/mod.rs
   $ git commit -m "feat(buildl): read buildl.toml into a workspace"
   ```

### Task 7 — Read the manifest from the workspace root

**Files:**
- Modify `crates/buildl/src/manifest/toml_manifest.rs`

Five edits to one file, the two in the tests first. Each old text occurs exactly once in the file.

**Steps:**

1. In the tests module of `crates/buildl/src/manifest/toml_manifest.rs`, replace the imports

   ````rust
       use core::num::NonZeroU64;
       use std::collections::BTreeSet;

       use buildl_core::{
           Directory, EnvName, Error, Execution, Isolation, Manifest, PortName, Program, Timeout,
           Workers,
       };

       use super::{DeclarationSettings, TomlManifest};
   ````

   with

   ````rust
       use core::num::NonZeroU64;
       use std::collections::BTreeSet;
       use std::fs;

       use buildl_core::{
           Directory, EnvName, Error, Execution, Isolation, Manifest, PortName, Program, Timeout,
           Workers,
       };

       use super::{DeclarationSettings, TomlManifest};
       use crate::manifest::root::WorkspaceRoot;
   ````

2. Replace the end of the file, the last lines of `every_failure_is_an_adapter_error_naming_the_manifest` and the brace that closes `mod tests`,

   ````rust
               assert!(!shown.contains('\n'), "{shown}");
           }
       }
   }
   ````

   with

   ````rust
               assert!(!shown.contains('\n'), "{shown}");
           }
       }

       #[test]
       fn a_manifest_is_read_from_the_root_it_was_found_at() {
           let dir = tempfile::tempdir().unwrap();
           fs::create_dir_all(dir.path().join("lib/text")).unwrap();
           fs::write(dir.path().join("buildl.toml"), DESIGN_EXAMPLE).unwrap();

           let root = WorkspaceRoot::find(&dir.path().join("lib/text")).unwrap();
           assert_eq!(
               TomlManifest::read(&root).unwrap(),
               TomlManifest::parse(DESIGN_EXAMPLE).unwrap()
           );
       }

       #[test]
       fn a_file_that_is_not_utf8_or_cannot_be_read_is_refused() {
           let not_text = tempfile::tempdir().unwrap();
           fs::write(
               not_text.path().join("buildl.toml"),
               b"[workspace]\nname = \"caf\xE9\"\n",
           )
           .unwrap();
           let root = WorkspaceRoot::find(not_text.path()).unwrap();
           let error = TomlManifest::read(&root).unwrap_err();
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::Manifest,
                   ..
               }
           ));
           assert_eq!(error.to_string(), "manifest: buildl.toml: not UTF-8 text");

           let not_a_file = tempfile::tempdir().unwrap();
           let path = not_a_file.path().join("buildl.toml");
           fs::create_dir(&path).unwrap();
           let root = WorkspaceRoot::find(not_a_file.path()).unwrap();
           let error = TomlManifest::read(&root).unwrap_err();
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::Manifest,
                   ..
               }
           ));
           assert_eq!(
               error.to_string(),
               format!("manifest: buildl.toml: {}", fs::read(&path).unwrap_err())
           );
       }
   }
   ````

   A file that cannot be read is made without a permission, so the test holds for any user: `buildl.toml` is a directory. The text expected is the one `fs::read` gives for that path; on macOS it read `manifest: buildl.toml: Is a directory (os error 21)`.

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib manifest::toml_manifest
   ```

   Expected, three times, once for each call:

   ```text
   error[E0599]: no associated function or constant named `read` found for struct `TomlManifest` in the current scope
   error: could not compile `buildl` (lib test) due to 3 previous errors
   ```

4. At the top of the file, replace the imports

   ````rust
   use core::num::NonZeroU64;

   use buildl_core::{EntryName, Manifest, Result, Workspace};

   use crate::manifest::interpret;
   use crate::manifest::schema::Raw;
   ````

   with

   ````rust
   use core::fmt;
   use core::num::NonZeroU64;
   use std::fs;

   use buildl_core::{Diagnostic, EntryName, Error, Manifest, PortName, Result, Workspace};

   use crate::manifest::interpret;
   use crate::manifest::root::{MANIFEST_FILE, WorkspaceRoot};
   use crate::manifest::schema::Raw;
   ````

5. Replace

   ````rust
   impl TomlManifest {
       /// Reads a manifest from its text.
   ````

   with

   ````rust
   impl TomlManifest {
       /// Reads `buildl.toml` at `root`.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], when the file cannot be read,
       /// is not UTF-8 text, or is refused by [`TomlManifest::parse`].
       pub fn read(root: &WorkspaceRoot) -> Result<Self> {
           let bytes = fs::read(root.as_path().join(MANIFEST_FILE)).map_err(|error| unread(&error))?;
           let text = String::from_utf8(bytes).map_err(|_| unread(&"not UTF-8 text"))?;
           Self::parse(&text)
       }

       /// Reads a manifest from its text.
   ````

6. Replace the end of `DeclarationSettings`

   ````rust
       pub walk_entries: Option<NonZeroU64>,
   }
   ````

   with

   ````rust
       pub walk_entries: Option<NonZeroU64>,
   }

   /// A manifest that could not be read as text at all, as the manifest's error.
   fn unread(reason: &dyn fmt::Display) -> Error {
       Error::Adapter {
           port: PortName::Manifest,
           diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}: {reason}")),
       }
   }
   ````

7. Run the tests again:

   ```
   $ cargo test -p buildl --lib manifest::toml_manifest
   ```

   Expected: a `test result: ok.` line reading `11 passed; 0 failed`.

8. Run the gate. Expected: all exit `0`.

9. Commit:

   ```
   $ git add crates/buildl/src/manifest/toml_manifest.rs
   $ git commit -m "feat(buildl): read the manifest from the workspace root"
   ```

### Task 8 — Report the host

**Files:**
- Create `crates/buildl/src/host/local_host.rs`
- Modify `crates/buildl/src/host/mod.rs`

**Steps:**

1. Create `crates/buildl/src/host/local_host.rs` with its documentation and its tests:

   ````rust
   //! [`LocalHost`], the [`Host`] that is the machine buildl itself runs on.
   //!
   //! Its own file because the platform and the environment are the two ambient facts an action
   //! key folds, and this is the one place either is read. A variable is read when it is asked for
   //! and never before: a snapshot of the environment would read names the workspace's ceiling
   //! does not allow.
   //!
   //! Responsibilities: [`LocalHost`] and its [`Host`] implementation.
   //!
   //! Non-responsibilities: deciding which variables may be read. A name reaches this adapter only
   //! after the ceiling allowed it.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::env;
       use std::ffi::OsString;
       use std::os::unix::ffi::OsStringExt;

       use buildl_core::{EnvName, Error, Host, PortName};

       use super::{LocalHost, value};

       #[cfg(target_os = "linux")]
       const OS: &str = "linux";
       #[cfg(target_os = "macos")]
       const OS: &str = "macos";

       #[cfg(target_arch = "x86_64")]
       const ARCH: &str = "x86_64";
       #[cfg(target_arch = "aarch64")]
       const ARCH: &str = "aarch64";

       fn name(text: &str) -> EnvName {
           EnvName::parse(text).unwrap()
       }

       #[test]
       fn the_platform_is_the_compiled_os_and_architecture() {
           let host = LocalHost::new().unwrap();
           assert_eq!(host.platform().as_str(), format!("{OS}/{ARCH}"));
           assert_eq!(host.platform(), host.platform());
       }

       #[test]
       fn a_variable_reads_as_the_process_holds_it() {
           let host = LocalHost::new().unwrap();
           for variable in ["PATH", "HOME", "BUILDL_TEST_A_NAME_NOTHING_SETS"] {
               let answer = host.env(&name(variable));
               match env::var_os(variable).map(OsString::into_string) {
                   None => assert_eq!(answer.unwrap(), None, "{variable}"),
                   Some(Ok(text)) => {
                       assert_eq!(answer.unwrap().unwrap().as_str(), text, "{variable}");
                   }
                   Some(Err(_)) => assert!(answer.is_err(), "{variable}"),
               }
           }
           assert_eq!(
               host.env(&name("BUILDL_TEST_A_NAME_NOTHING_SETS")).unwrap(),
               None
           );
       }

       #[test]
       fn an_unset_an_empty_and_a_non_unicode_value_are_told_apart() {
           let variable = name("P_VALUE");
           assert_eq!(value(&variable, None).unwrap(), None);
           assert_eq!(
               value(&variable, Some(OsString::new()))
                   .unwrap()
                   .unwrap()
                   .as_str(),
               ""
           );
           assert_eq!(
               value(&variable, Some(OsString::from("a b=c")))
                   .unwrap()
                   .unwrap()
                   .as_str(),
               "a b=c"
           );

           let not_unicode = OsString::from_vec(b"caf\xE9\xFF".to_vec());
           let error = value(&variable, Some(not_unicode)).unwrap_err();
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::Host,
                   ..
               }
           ));
           assert_eq!(
               error.to_string(),
               "host: environment variable P_VALUE does not hold valid Unicode"
           );

           let with_nul = OsString::from_vec(b"a\0b".to_vec());
           assert_eq!(
               value(&variable, Some(with_nul)).unwrap_err().to_string(),
               "host: environment variable P_VALUE holds a NUL byte"
           );
       }
   }
   ````

   The platform expected is chosen by `cfg`, so the Linux leg of the gate asserts `linux/x86_64` and this machine asserted `macos/aarch64`. The second test compares `env` with `std::env::var_os` for two names every process has and one nothing sets, and sets nothing itself. The third hands `value` what `var_os` would return, which is how an empty value and one that is not Unicode are tested without touching the environment: `OsString::from_vec` builds the bytes `caf\xE9\xFF`.

2. Replace the whole content of `crates/buildl/src/host/mod.rs` with:

   ````rust
   //! The machine actions run on, as buildl itself sees it.
   //!
   //! Its own directory because the platform and the environment are ambient: nothing else in this
   //! crate reads either, so what an action key says about the machine has one source.
   //!
   //! Responsibilities:
   //!
   //! - [`LocalHost`] — the platform this binary was compiled for, and the value of an environment
   //!   variable at the moment it is asked for.
   //!
   //! Non-responsibilities: the ceiling on which variables a build may read, which is the
   //! manifest's to state and the pipeline's to hold a build to.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod local_host;

   pub use local_host::LocalHost;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib host::
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::LocalHost`, `super::value`
   error[E0432]: unresolved import `local_host::LocalHost`
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. In `crates/buildl/src/host/local_host.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use std::env;
   use std::ffi::OsString;

   use buildl_core::{Diagnostic, EnvName, EnvValue, Error, Host, Platform, PortName, Result};

   /// The machine buildl runs on: the platform it was compiled for, and its own environment.
   ///
   /// # Examples
   ///
   /// ```
   /// use buildl::host::LocalHost;
   /// use buildl_core::{EnvName, Host};
   ///
   /// let host = LocalHost::new()?;
   /// assert_eq!(host.platform().as_str().split('/').count(), 2);
   ///
   /// let unset = EnvName::parse("BUILDL_DOC_A_NAME_NOTHING_SETS")?;
   /// assert_eq!(host.env(&unset)?, None);
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct LocalHost {
       platform: Platform,
   }

   impl LocalHost {
       /// The host this binary was compiled for.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] if the compiler's text for the operating system or the
       /// architecture is not a platform part.
       pub fn new() -> Result<Self> {
           Ok(Self {
               platform: Platform::parse(format!("{}/{}", env::consts::OS, env::consts::ARCH))?,
           })
       }
   }

   impl Host for LocalHost {
       /// The operating system and the architecture, such as `linux/x86_64` or `macos/aarch64`.
       fn platform(&self) -> Platform {
           self.platform.clone()
       }

       /// The value the variable `name` has in this process now. A variable set to nothing is
       /// `Some("")`, not `None`.
       fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
           value(name, env::var_os(name.as_str()))
       }
   }

   /// What a variable `held` in the environment is as a value: `None` when it is unset.
   fn value(name: &EnvName, held: Option<OsString>) -> Result<Option<EnvValue>> {
       held.map(|held| {
           let text = held
               .into_string()
               .map_err(|_| failed(name, "does not hold valid Unicode"))?;
           EnvValue::parse(text).map_err(|_| failed(name, "holds a NUL byte"))
       })
       .transpose()
   }

   /// A variable whose value cannot be handed on, as the host adapter's error.
   fn failed(name: &EnvName, reason: &str) -> Error {
       Error::Adapter {
           port: PortName::Host,
           diagnostic: Diagnostic::new(format!("environment variable {name} {reason}")),
       }
   }
   ````

   `env` calls `std::env::var_os` each time it is asked and reads nothing else. It checks no ceiling: a name reaches it only after the pipeline held it to `env.read`.

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib host::
   ```

   Expected: a `test result: ok.` line reading `3 passed; 0 failed`.

6. Run the gate. Expected: all exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/host/local_host.rs crates/buildl/src/host/mod.rs
   $ git commit -m "feat(buildl): report the host platform and environment"
   ```

### Task 9 — Stat and hash input files

**Files:**
- Create `crates/buildl/src/digest/file_digester.rs`
- Modify `crates/buildl/src/digest/mod.rs`

**Steps:**

1. Create `crates/buildl/src/digest/file_digester.rs` with its documentation and its tests:

   ````rust
   //! [`FileDigester`], the [`Digester`] that reads the files of a workspace.
   //!
   //! Its own file because it is the one place a declared input is looked at and read, and the one
   //! place in the workspace that hashes on several threads at once. The threads never show: the
   //! digests come back in the order the paths were given, and so does the first failure.
   //!
   //! Responsibilities: [`FileDigester`] and its [`Digester`] implementation.
   //!
   //! Non-responsibilities: remembering a digest between builds, and deciding which files need
   //! one. Both are the caller's; this adapter hashes what it is asked to.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           clippy::panic,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::os::unix::fs::symlink;
       use std::path::{Path, PathBuf};
       use std::process::Command;
       use std::time::{Duration, UNIX_EPOCH};

       use buildl_core::{Digest, Digester, Error, InputKind, InputStat, PortName, SourcePath};

       use super::{BUFFER_BYTES, FileDigester, unix_nanos};

       /// A temp directory, its canonical path, and a digester over it.
       fn workspace() -> (tempfile::TempDir, PathBuf, FileDigester) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           let digester = FileDigester::new(&root);
           (dir, root, digester)
       }

       fn path(text: &str) -> SourcePath {
           SourcePath::parse(text).unwrap()
       }

       /// Makes a named pipe with the system's `mkfifo`.
       fn pipe(at: &Path) {
           let made = Command::new("mkfifo").arg(at).status().unwrap();
           assert!(made.success(), "mkfifo {}", at.display());
       }

       /// A file, a link to it, a dangling link, a directory and a pipe, all under `root`.
       fn one_of_each(root: &Path) {
           fs::write(root.join("file.txt"), "content").unwrap();
           symlink(root.join("file.txt"), root.join("link")).unwrap();
           symlink(root.join("nowhere"), root.join("dangling")).unwrap();
           fs::create_dir(root.join("dir")).unwrap();
           pipe(&root.join("pipe"));
       }

       fn refusal(digester: &FileDigester, text: &str) -> String {
           let error = digester.digest(&[path(text)]).unwrap_err();
           assert!(
               matches!(
                   error,
                   Error::Adapter {
                       port: PortName::Digester,
                       ..
                   }
               ),
               "{error:?}"
           );
           error.to_string()
       }

       #[test]
       fn stat_reports_size_and_modification_time() {
           let (_dir, root, digester) = workspace();
           fs::write(root.join("main.c"), "int main;\n").unwrap();
           let modified = fs::metadata(root.join("main.c"))
               .unwrap()
               .modified()
               .unwrap()
               .duration_since(UNIX_EPOCH)
               .unwrap();

           let stat = match digester.stat(&path("main.c")).unwrap() {
               InputStat::File(stat) => stat,
               other => panic!("expected a regular file, got {other:?}"),
           };
           assert_eq!(stat.size, 10);
           assert_eq!(
               u128::from(stat.modified.as_unix_nanos()),
               modified.as_nanos()
           );
       }

       #[test]
       fn stat_of_nothing_is_missing() {
           let (_dir, root, digester) = workspace();
           fs::write(root.join("file.txt"), "content").unwrap();
           fs::create_dir(root.join("dir")).unwrap();
           for nothing in [
               "missing.c",
               "dir/missing.c",
               "no-dir/missing.c",
               "file.txt/below",
           ] {
               assert_eq!(
                   digester.stat(&path(nothing)).unwrap(),
                   InputStat::Missing,
                   "{nothing}"
               );
           }
       }

       #[test]
       fn stat_reports_a_link_a_directory_and_a_pipe_as_what_they_are() {
           let (_dir, root, digester) = workspace();
           one_of_each(&root);
           let cases = [
               ("link", InputKind::Symlink),
               ("dangling", InputKind::Symlink),
               ("dir", InputKind::Directory),
               ("pipe", InputKind::Special),
           ];
           for (at, kind) in cases {
               assert_eq!(
                   digester.stat(&path(at)).unwrap(),
                   InputStat::Other(kind),
                   "{at}"
               );
           }
           assert!(matches!(
               digester.stat(&path("file.txt")).unwrap(),
               InputStat::File(stat) if stat.size == 7
           ));
       }

       #[test]
       fn stat_of_a_path_that_cannot_be_looked_at_is_an_adapter_error() {
           let (_dir, _root, digester) = workspace();
           let too_long = "n".repeat(300);
           let error = digester.stat(&path(&too_long)).unwrap_err();
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::Digester,
                   ..
               }
           ));
           assert!(
               error
                   .to_string()
                   .starts_with(&format!("digester: {too_long}: ")),
               "{error}"
           );
       }

       #[test]
       fn a_modification_time_is_counted_in_nanoseconds_since_1970() {
           assert_eq!(unix_nanos(UNIX_EPOCH), Ok(0));
           assert_eq!(
               unix_nanos(UNIX_EPOCH + Duration::new(1_791_881_164, 927_344_286)),
               Ok(1_791_881_164_927_344_286)
           );
           assert_eq!(
               unix_nanos(UNIX_EPOCH - Duration::from_nanos(1)),
               Err("its modification time is before 1970")
           );
           assert_eq!(
               unix_nanos(UNIX_EPOCH + Duration::from_secs(20_000_000_000)),
               Err("its modification time is after the year 2554")
           );
       }

       #[test]
       fn digest_refuses_a_path_that_is_not_a_regular_file() {
           let (_dir, root, digester) = workspace();
           one_of_each(&root);
           let cases = [
               (
                   "link",
                   "digester: link: is a symbolic link; an input must be a regular file",
               ),
               (
                   "dangling",
                   "digester: dangling: is a symbolic link; an input must be a regular file",
               ),
               (
                   "dir",
                   "digester: dir: is a directory; an input must be a regular file",
               ),
               ("pipe", "digester: pipe: is not a regular file"),
           ];
           for (at, expected) in cases {
               assert_eq!(refusal(&digester, at), expected, "{at}");
           }
           assert_eq!(
               digester.digest(&[path("file.txt")]).unwrap(),
               [Digest::of(b"content")]
           );
       }

       #[test]
       fn digests_come_back_in_the_order_asked() {
           let (_dir, root, digester) = workspace();
           let contents: Vec<Vec<u8>> = (0_usize..500)
               .map(|index| format!("file {index}\n").repeat(1 + index % 7).into_bytes())
               .collect();
           let paths: Vec<SourcePath> = (0..contents.len())
               .map(|index| path(&format!("f{index:03}.txt")))
               .collect();
           for (at, content) in paths.iter().zip(&contents) {
               fs::write(root.join(at.as_str()), content).unwrap();
           }
           let expected: Vec<Digest> = contents.iter().map(|content| Digest::of(content)).collect();

           for run in 0..20 {
               assert_eq!(digester.digest(&paths).unwrap(), expected, "run {run}");
           }
           assert_eq!(digester.digest(&[]).unwrap(), []);
       }

       #[test]
       fn the_first_failing_path_in_order_is_the_error() {
           let (_dir, root, digester) = workspace();
           fs::create_dir(root.join("f350.txt")).unwrap();
           symlink(root.join("f000.txt"), root.join("f400.txt")).unwrap();
           let paths: Vec<SourcePath> = (0..500)
               .map(|index| path(&format!("f{index:03}.txt")))
               .collect();
           for (index, at) in paths.iter().enumerate() {
               if ![300, 350, 400, 450].contains(&index) {
                   fs::write(root.join(at.as_str()), format!("file {index}\n")).unwrap();
               }
           }

           for run in 0..50 {
               assert_eq!(
                   digester.digest(&paths).unwrap_err().to_string(),
                   "digester: f300.txt: No such file or directory (os error 2)",
                   "run {run}"
               );
           }
       }

       #[test]
       fn a_file_larger_than_the_buffer_hashes_like_its_bytes() {
           let (_dir, root, digester) = workspace();
           let sizes = [
               0,
               1,
               BUFFER_BYTES - 1,
               BUFFER_BYTES,
               BUFFER_BYTES + 1,
               3 * BUFFER_BYTES + 17,
           ];
           for size in sizes {
               let content: Vec<u8> = (0..size)
                   .map(|index| u8::try_from(index % 251).unwrap())
                   .collect();
               fs::write(root.join("blob.bin"), &content).unwrap();
               assert_eq!(
                   digester.digest(&[path("blob.bin")]).unwrap(),
                   [Digest::of(&content)],
                   "{size} bytes"
               );
           }
       }

       #[test]
       fn a_file_that_resolves_outside_the_workspace_is_refused() {
           let (_dir, root, digester) = workspace();
           let outside = tempfile::tempdir().unwrap();
           fs::write(outside.path().join("secret.txt"), "elsewhere").unwrap();
           symlink(outside.path(), root.join("vendor")).unwrap();

           assert!(matches!(
               digester.stat(&path("vendor/secret.txt")).unwrap(),
               InputStat::File(stat) if stat.size == 9
           ));
           assert_eq!(
               refusal(&digester, "vendor/secret.txt"),
               "digester: vendor/secret.txt: resolves outside the workspace"
           );

           fs::create_dir(root.join("real")).unwrap();
           fs::write(root.join("real/inside.txt"), "inside").unwrap();
           symlink(root.join("real"), root.join("alias")).unwrap();
           assert_eq!(
               digester.digest(&[path("alias/inside.txt")]).unwrap(),
               [Digest::of(b"inside")]
           );
       }
   }
   ````

   What each test pins:

   | Test | Pins |
   |---|---|
   | `stat_reports_size_and_modification_time` | the modification time is the filesystem's, whole, in nanoseconds |
   | `stat_of_nothing_is_missing` | a path that does not exist, and one that runs through a file, are `Missing` |
   | `stat_reports_a_link_a_directory_and_a_pipe_as_what_they_are` | a link is a link whether or not it leads anywhere; the pipe is made with `mkfifo` |
   | `stat_of_a_path_that_cannot_be_looked_at_is_an_adapter_error` | a 300-byte name is longer than the 255 bytes a file name may have, so the failure needs no permission; on macOS it read `File name too long (os error 63)`, and the test asserts the port and the path, not the system's words |
   | `a_modification_time_is_counted_in_nanoseconds_since_1970` | the two times that have no count, tested on the function, since no filesystem need hold such a file |
   | `digest_refuses_a_path_that_is_not_a_regular_file` | each kind's text; the pipe is refused without being opened, or the test would never return |
   | `digests_come_back_in_the_order_asked` | 500 files, 20 runs, equal to `Digest::of` in input order |
   | `the_first_failing_path_in_order_is_the_error` | four failing paths, at 300, 350, 400 and 450; over 50 runs the error is always the one for 300 |
   | `a_file_larger_than_the_buffer_hashes_like_its_bytes` | sizes on both sides of one buffer, and of none |
   | `a_file_that_resolves_outside_the_workspace_is_refused` | `stat` sees a regular file through a directory link; `digest` refuses one that resolves outside the root and takes one that resolves inside |

2. Replace the whole content of `crates/buildl/src/digest/mod.rs` with:

   ````rust
   //! What stands at the path of a declared input, and the digest of its content.
   //!
   //! Its own directory because reading a workspace's files is one concern with one rule: an input
   //! is a regular file inside the workspace. It is also the only module of the workspace that
   //! hashes on a pool of threads, so that pool stays out of everything else.
   //!
   //! Responsibilities:
   //!
   //! - [`FileDigester`] — one look at a path that follows no symbolic link, and the SHA-256 of
   //!   each file asked for, in the order asked.
   //!
   //! Non-responsibilities: keeping a digest from one build to the next, and naming the target
   //! that declared an input that is missing or is not a file. Both belong to the pipeline.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod file_digester;

   pub use file_digester::FileDigester;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib digest::
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::BUFFER_BYTES`, `super::FileDigester`, `super::unix_nanos`
   error[E0432]: unresolved import `file_digester::FileDigester`
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. In `crates/buildl/src/digest/file_digester.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use std::fs::{self, File, FileType};
   use std::io::{self, ErrorKind, Read};
   use std::path::PathBuf;
   use std::time::{SystemTime, UNIX_EPOCH};

   use buildl_core::{
       Diagnostic, Digest, Digester, Error, FileStat, Hasher, InputKind, InputStat, PortName, Result,
       SourcePath, Timestamp,
   };
   use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

   /// How many bytes of a file are read at a time. No file is held whole.
   const BUFFER_BYTES: usize = 64 * 1024;

   /// Reads the metadata and the content digests of the files under one workspace root.
   ///
   /// A declared input is a regular file. What stands at a path is looked at without following a
   /// symbolic link, so a link is reported as a link whatever it leads to.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::fs;
   ///
   /// use buildl::digest::FileDigester;
   /// use buildl_core::{Digest, Digester, InputStat, SourcePath};
   ///
   /// let workspace = tempfile::tempdir()?;
   /// let root = fs::canonicalize(workspace.path())?;
   /// fs::write(root.join("main.c"), "int main(void) { return 0; }\n")?;
   ///
   /// let digester = FileDigester::new(&root);
   /// let main = SourcePath::parse("main.c")?;
   /// assert!(matches!(digester.stat(&main)?, InputStat::File(stat) if stat.size == 29));
   /// assert_eq!(
   ///     digester.digest(&[main])?,
   ///     [Digest::of(b"int main(void) { return 0; }\n")]
   /// );
   /// assert_eq!(
   ///     digester.stat(&SourcePath::parse("missing.c")?)?,
   ///     InputStat::Missing
   /// );
   /// # Ok::<(), Box<dyn std::error::Error>>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct FileDigester {
       root: PathBuf,
   }

   impl FileDigester {
       /// A digester for the workspace at `root`.
       ///
       /// `root` must be the workspace root's canonical, absolute path: a file that resolves to a
       /// path outside it is refused.
       #[must_use]
       pub fn new(root: impl Into<PathBuf>) -> Self {
           Self { root: root.into() }
       }

       /// The digest of the regular file at `path`, read through `buffer`.
       ///
       /// The path is looked at again before it is opened, so nothing but a regular file is ever
       /// opened: a pipe is refused, not read.
       fn hash(&self, path: &SourcePath, buffer: &mut [u8]) -> Result<Digest> {
           let full = self.root.join(path.as_str());
           let metadata = fs::symlink_metadata(&full).map_err(|error| failed(path, &error))?;
           match kind(metadata.file_type()) {
               None => {}
               Some(InputKind::Special) => return Err(failed(path, &"is not a regular file")),
               Some(found) => {
                   return Err(failed(
                       path,
                       &format_args!("is {found}; an input must be a regular file"),
                   ));
               }
           }
           let resolved = fs::canonicalize(&full).map_err(|error| failed(path, &error))?;
           if !resolved.starts_with(&self.root) {
               return Err(failed(path, &"resolves outside the workspace"));
           }
           let mut file = File::open(&resolved).map_err(|error| failed(path, &error))?;
           let mut hasher = Hasher::new();
           loop {
               match file.read(buffer) {
                   Ok(0) => return Ok(hasher.finish()),
                   Ok(filled) => hasher.update(buffer.get(..filled).unwrap_or_default()),
                   Err(error) if error.kind() == ErrorKind::Interrupted => {}
                   Err(error) => return Err(failed(path, &error)),
               }
           }
       }
   }

   impl Digester for FileDigester {
       /// What stands at `path`, from one look that follows no symbolic link.
       ///
       /// Nothing there, and a path that runs through a file, are both
       /// [`InputStat::Missing`]. The modification time is the filesystem's own, to the
       /// nanosecond, never rounded.
       fn stat(&self, path: &SourcePath) -> Result<InputStat> {
           let metadata = match fs::symlink_metadata(self.root.join(path.as_str())) {
               Ok(metadata) => metadata,
               Err(error) if absent(&error) => return Ok(InputStat::Missing),
               Err(error) => return Err(failed(path, &error)),
           };
           if let Some(found) = kind(metadata.file_type()) {
               return Ok(InputStat::Other(found));
           }
           let modified = metadata
               .modified()
               .map_err(|error| failed(path, &error))
               .and_then(|modified| unix_nanos(modified).map_err(|reason| failed(path, &reason)))?;
           Ok(InputStat::File(FileStat {
               modified: Timestamp::from_unix_nanos(modified),
               size: metadata.len(),
           }))
       }

       /// The digest of every file in `paths`, hashed on several threads and returned in the order
       /// given. When more than one path fails, the error is the one for the first of them in that
       /// order, whichever thread met its failure first.
       fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
           let hashed: Vec<Result<Digest>> = paths
               .par_iter()
               .map_init(
                   || vec![0_u8; BUFFER_BYTES],
                   |buffer, path| self.hash(path, buffer),
               )
               .collect();
           hashed.into_iter().collect()
       }
   }

   /// Whether `error` says that nothing stands at a path: it does not exist, or it runs through
   /// something that is not a directory.
   fn absent(error: &io::Error) -> bool {
       matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
   }

   /// What a file type is when it is not a regular file; `None` for a regular file.
   fn kind(file_type: FileType) -> Option<InputKind> {
       if file_type.is_file() {
           None
       } else if file_type.is_symlink() {
           Some(InputKind::Symlink)
       } else if file_type.is_dir() {
           Some(InputKind::Directory)
       } else {
           Some(InputKind::Special)
       }
   }

   /// A modification time as whole nanoseconds since the Unix epoch, or why it has no such count.
   fn unix_nanos(modified: SystemTime) -> core::result::Result<u64, &'static str> {
       let since_epoch = modified
           .duration_since(UNIX_EPOCH)
           .map_err(|_| "its modification time is before 1970")?;
       u64::try_from(since_epoch.as_nanos())
           .map_err(|_| "its modification time is after the year 2554")
   }

   /// A failure to look at or read `path`, as the digester's error.
   fn failed(path: &SourcePath, reason: &dyn core::fmt::Display) -> Error {
       Error::Adapter {
           port: PortName::Digester,
           diagnostic: Diagnostic::new(format!("{path}: {reason}")),
       }
   }
   ````

   `digest` collects into `Vec<Result<Digest>>` first: rayon's indexed collect keeps input order whatever thread finished first. The second `collect`, over that vector in order, is what makes the error the first failing path's. Collecting straight into `Result<Vec<_>>` from the parallel iterator would return whichever failure a thread met first. `map_init` gives each worker thread one buffer for all the files it hashes. `hash` looks at the path with `symlink_metadata` before opening it, so only a regular file is ever opened.

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib digest::
   ```

   Expected: a `test result: ok.` line reading `10 passed; 0 failed`.

6. Check that rayon is used in this file and in no other of the workspace's sources:

   ```
   $ grep -rn "rayon::" crates/*/src
   ```

   Expected, one line and nothing else:

   ```text
   crates/buildl/src/digest/file_digester.rs:21:use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
   ```

7. Run the gate. Expected: all exit `0`.

8. Commit:

   ```
   $ git add crates/buildl/src/digest/file_digester.rs crates/buildl/src/digest/mod.rs
   $ git commit -m "feat(buildl): stat and hash input files"
   ```

### Task 10 — Fingerprint a tool by its content

**Files:**
- Create `crates/buildl/src/tools/path_tools.rs`
- Modify `crates/buildl/src/tools/mod.rs`

**Steps:**

1. Create `crates/buildl/src/tools/path_tools.rs` with its documentation and its tests:

   ````rust
   //! [`PathTools`], the [`ToolResolver`] that finds a program and hashes the file it names.
   //!
   //! Its own file because what identifies a tool is a choice, and it is made here: the bytes of
   //! the file its name leads to, links followed. No path, size or time is part of it, so a
   //! fingerprint holds no clock value, and nothing is run to take it.
   //!
   //! Responsibilities: [`PathTools`] and its [`ToolResolver`] implementation.
   //!
   //! Non-responsibilities: the lookup itself, which is the search path's, shared with whatever
   //! runs the program so that the file run is the file fingerprinted.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, File};
       use std::os::unix::fs::{PermissionsExt, symlink};
       use std::path::{Path, PathBuf};

       use buildl_core::{Digest, Error, PortName, Program, ToolResolver};

       use super::{BUFFER_BYTES, PathTools};
       use crate::search_path::SearchPath;

       /// A temp directory and its canonical path.
       fn workspace() -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           (dir, root)
       }

       /// Writes `bytes` at `at` with `mode`, making the directory above it.
       fn file(at: &Path, bytes: &[u8], mode: u32) {
           fs::create_dir_all(at.parent().unwrap()).unwrap();
           fs::write(at, bytes).unwrap();
           fs::set_permissions(at, fs::Permissions::from_mode(mode)).unwrap();
       }

       fn program(name: &str) -> Program {
           Program::parse(name).unwrap()
       }

       #[test]
       fn the_fingerprint_is_the_digest_of_the_file() {
           let (_dir, root) = workspace();
           let large: Vec<u8> = (0..3 * BUFFER_BYTES + 17)
               .map(|index| u8::try_from(index % 251).unwrap())
               .collect();
           file(&root.join("bin/small"), b"#!/bin/sh\n", 0o755);
           file(&root.join("bin/large"), &large, 0o755);
           file(
               &root.join("scripts/gen.sh"),
               b"#!/bin/sh\necho gen\n",
               0o755,
           );
           let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));

           assert_eq!(
               tools.fingerprint(&program("small")).unwrap(),
               Some(Digest::of(b"#!/bin/sh\n"))
           );
           assert_eq!(
               tools.fingerprint(&program("large")).unwrap(),
               Some(Digest::of(&large))
           );
           assert_eq!(
               tools.fingerprint(&program("scripts/gen.sh")).unwrap(),
               Some(Digest::of(b"#!/bin/sh\necho gen\n"))
           );
           let absolute = root.join("bin/small");
           assert_eq!(
               tools
                   .fingerprint(&program(absolute.to_str().unwrap()))
                   .unwrap(),
               Some(Digest::of(b"#!/bin/sh\n"))
           );
       }

       #[test]
       fn the_fingerprint_holds_no_path_and_no_time() {
           let (_dir, root) = workspace();
           file(&root.join("first/tool"), b"same bytes", 0o755);
           file(&root.join("second/other-name"), b"same bytes", 0o700);
           let first = PathTools::new(&root, SearchPath::new([root.join("first")]));
           let second = PathTools::new(&root, SearchPath::new([root.join("second")]));
           assert_eq!(
               first.fingerprint(&program("tool")).unwrap(),
               second.fingerprint(&program("other-name")).unwrap()
           );
       }

       #[test]
       fn a_linked_tool_has_its_targets_fingerprint() {
           let (_dir, root) = workspace();
           file(&root.join("real/real-tool"), b"version 1", 0o755);
           fs::create_dir(root.join("bin")).unwrap();
           symlink(root.join("real/real-tool"), root.join("bin/linked")).unwrap();
           let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));

           assert_eq!(
               tools.fingerprint(&program("linked")).unwrap(),
               Some(Digest::of(b"version 1"))
           );

           file(&root.join("real/real-tool"), b"version 2", 0o755);
           assert_eq!(
               tools.fingerprint(&program("linked")).unwrap(),
               Some(Digest::of(b"version 2"))
           );
       }

       #[test]
       fn a_program_that_is_not_found_is_none() {
           let (_dir, root) = workspace();
           file(&root.join("bin/not-executable"), b"data", 0o644);
           fs::create_dir(root.join("bin/a-directory")).unwrap();
           let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));
           for name in [
               "no-such-tool",
               "not-executable",
               "a-directory",
               "scripts/missing.sh",
               "/no/such/directory/tool",
           ] {
               assert_eq!(tools.fingerprint(&program(name)).unwrap(), None, "{name}");
           }

           let nowhere = PathTools::new(&root, SearchPath::new([]));
           assert_eq!(nowhere.fingerprint(&program("sh")).unwrap(), None);
       }

       #[test]
       fn a_tool_that_cannot_be_read_is_an_adapter_error() {
           let (_dir, root) = workspace();
           let tool = root.join("bin/sealed");
           file(&tool, b"unreadable", 0o111);
           if File::open(&tool).is_ok() {
               // Whoever runs this test may read any file, so no file here can be unreadable.
               return;
           }
           let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));
           let error = tools.fingerprint(&program("sealed")).unwrap_err();
           assert!(matches!(
               error,
               Error::Adapter {
                   port: PortName::ToolResolver,
                   ..
               }
           ));
           assert_eq!(
               error.to_string(),
               format!(
                   "tool resolver: {}: Permission denied (os error 13)",
                   tool.display()
               )
           );
       }
   }
   ````

   Every lookup is over a `SearchPath::new` of temp directories, so no test depends on the machine's `PATH`. `a_tool_that_cannot_be_read_is_an_adapter_error` gives a file execute bits and no read bit; a user who may read any file (root) cannot make such a file unreadable, so the test first tries to open it and returns when that succeeds.

2. Replace the whole content of `crates/buildl/src/tools/mod.rs` with:

   ````rust
   //! The programs commands name: finding each one, and what identifies it.
   //!
   //! Its own directory because a tool is part of what an action is: when the program a command
   //! names changes, every target that runs it must be rebuilt, and this module is what says a
   //! program changed.
   //!
   //! Responsibilities:
   //!
   //! - [`PathTools`] — the content digest of the file a program's name leads to, or nothing when
   //!   the name leads to no file.
   //!
   //! Non-responsibilities: the rule for finding a program, which is the search path's and is
   //! shared with whatever runs it; and deciding which programs a build may run.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod path_tools;

   pub use path_tools::PathTools;
   ````

3. Run the tests and see them fail to compile:

   ```
   $ cargo test -p buildl --lib tools::
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::BUFFER_BYTES`, `super::PathTools`
   error[E0432]: unresolved import `path_tools::PathTools`
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. In `crates/buildl/src/tools/path_tools.rs`, put the code between the documentation block and the tests, with one blank line before `#[cfg(test)]`:

   ````rust
   use std::fs::File;
   use std::io::{ErrorKind, Read};
   use std::path::{Path, PathBuf};

   use buildl_core::{Diagnostic, Digest, Error, Hasher, PortName, Program, Result, ToolResolver};

   use crate::search_path::SearchPath;

   /// How many bytes of a program are read at a time. No program is held whole.
   const BUFFER_BYTES: usize = 64 * 1024;

   /// Finds the programs commands name, and fingerprints each by its content.
   ///
   /// A fingerprint changes exactly when the bytes of the file the name leads to change. A
   /// launcher that picks the real tool when it runs is fingerprinted as the launcher: a change
   /// behind it, like a change to a library a tool loads, is not seen.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::fs;
   /// use std::os::unix::fs::PermissionsExt;
   ///
   /// use buildl::search_path::SearchPath;
   /// use buildl::tools::PathTools;
   /// use buildl_core::{Digest, Program, ToolResolver};
   ///
   /// let workspace = tempfile::tempdir()?;
   /// let bin = fs::canonicalize(workspace.path())?.join("bin");
   /// fs::create_dir(&bin)?;
   /// fs::write(bin.join("gen"), "#!/bin/sh\necho generated\n")?;
   /// fs::set_permissions(bin.join("gen"), fs::Permissions::from_mode(0o755))?;
   ///
   /// let tools = PathTools::new(workspace.path(), SearchPath::new([bin]));
   /// assert_eq!(
   ///     tools.fingerprint(&Program::parse("gen")?)?,
   ///     Some(Digest::of(b"#!/bin/sh\necho generated\n"))
   /// );
   /// assert_eq!(tools.fingerprint(&Program::parse("no-such-tool")?)?, None);
   /// # Ok::<(), Box<dyn std::error::Error>>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct PathTools {
       root: PathBuf,
       search: SearchPath,
   }

   impl PathTools {
       /// A resolver for the workspace at `root`, looking bare names up in `search`.
       ///
       /// `root` is the directory a program named by a relative path is looked for in.
       #[must_use]
       pub fn new(root: impl Into<PathBuf>, search: SearchPath) -> Self {
           Self {
               root: root.into(),
               search,
           }
       }
   }

   impl ToolResolver for PathTools {
       /// The SHA-256 of the file `program` leads to; `Ok(None)` when the lookup finds no file.
       fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
           self.search
               .locate(program, &self.root)
               .map(|located| content(&located).map_err(|error| failed(&located, &error)))
               .transpose()
       }
   }

   /// The digest of the file at `located`, links followed, read in pieces.
   fn content(located: &Path) -> std::io::Result<Digest> {
       let mut file = File::open(located)?;
       let mut buffer = vec![0_u8; BUFFER_BYTES];
       let mut hasher = Hasher::new();
       loop {
           match file.read(&mut buffer) {
               Ok(0) => return Ok(hasher.finish()),
               Ok(filled) => hasher.update(buffer.get(..filled).unwrap_or_default()),
               Err(error) if error.kind() == ErrorKind::Interrupted => {}
               Err(error) => return Err(error),
           }
       }
   }

   /// A located program that could not be read, as the tool resolver's error.
   fn failed(located: &Path, error: &std::io::Error) -> Error {
       Error::Adapter {
           port: PortName::ToolResolver,
           diagnostic: Diagnostic::new(format!("{}: {error}", located.display())),
       }
   }
   ````

   The file opened is the one `locate` returned, links followed, so a linked tool has its target's fingerprint. A name the lookup does not find is `Ok(None)`: the pipeline turns that into the error naming the target.

5. Run the tests again:

   ```
   $ cargo test -p buildl --lib tools::
   ```

   Expected: a `test result: ok.` line reading `5 passed; 0 failed`.

6. Run the gate. Expected: all exit `0`. `guard-adapter-isolation`, which `cargo make dod` runs, now counts one more edge, `tools -> search_path`.

7. Commit:

   ```
   $ git add crates/buildl/src/tools/path_tools.rs crates/buildl/src/tools/mod.rs
   $ git commit -m "feat(buildl): fingerprint a tool by its content"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0` on the tree task 10 leaves.
- `cargo test -p buildl --lib -- manifest:: host:: digest:: tools::` gives `46 passed`: 6 in `manifest::root`, 2 in `manifest::memory`, 4 in `manifest::schema`, 5 in `manifest::interpret`, 11 in `manifest::toml_manifest`, 3 in `host`, 10 in `digest`, 5 in `tools`.
- `cargo test -p buildl --doc` runs five examples from this plan: `WorkspaceRoot`, `TomlManifest`, `LocalHost`, `FileDigester`, `PathTools`.
- Every test the design names for these four modules is present under its name: `root.rs` 4, `memory.rs` 2, `interpret.rs` 3, `toml_manifest.rs` 8, `host/` 2, `digest/` 8, `tools/` 3. The other sixteen tests are this plan's own.
- `grep -rn "dead_code" crates/buildl/src/manifest` prints nothing: the three attributes of tasks 2 to 4 are gone after task 6.
- `grep -rn "rayon::" crates/*/src` prints one line, in `crates/buildl/src/digest/file_digester.rs`.
- `grep -rn "crate::" crates/buildl/src/manifest crates/buildl/src/host crates/buildl/src/digest crates/buildl/src/tools` shows paths that begin `crate::manifest::` inside `manifest/` and `crate::search_path::SearchPath` inside `tools/`, and no other module of the crate.
- No file outside the four module directories changed: `git diff --stat` over the ten commits lists only paths under `crates/buildl/src/manifest`, `host`, `digest` and `tools`.
- On the `ubuntu-latest` leg of CI the same tests prove what was only run on macOS: the kinds `symlink_metadata` reports (`stat_of_nothing_is_missing`, `stat_reports_a_link_a_directory_and_a_pipe_as_what_they_are`), rayon's ordered collect (`digests_come_back_in_the_order_asked`, `the_first_failing_path_in_order_is_the_error`), and the platform text (`the_platform_is_the_compiled_os_and_architecture`, whose Linux arm expects `linux/x86_64`).
