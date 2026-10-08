---
status: done
created: 2026-10-06
---

# Value Types Implementation Plan

**Goal:** Every field value a build file can declare has a validated type that cannot be built invalid.

**Architecture:** Ten validated value types in `crates/buildl-core/src/types/`, one file per
concept (`argument.rs` holds `Argument` and `Command`, `setting.rs` holds `SettingName` and
`SettingValue`), plus `Diagnostic`, which is accepted as-is because it is only ever displayed. The
character grammars that several types share live once, in a private `types/grammar.rs`: each
function returns the reason a candidate is rejected and each type wraps that reason into
`Error::InvalidName` with its own `NameKind`, so a grammar is shared without one type wrapping
another. `Directory::parse` and `TargetName::parse` move onto the same grammars, which removes
their duplicated checks. Every string type follows `target_name.rs`'s surface exactly and
serializes as its string through `try_from = "String"`, so deserialization re-runs validation.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` 1.0 with `derive`, `thiserror` 2,
`cargo-make`.

**Content authority:** spec §3.2 (new value types and their grammars), §6.1 (`NameKind` gains one
variant per new validated type), §7 (module layout of `types/`), §8 (the "every new type" row).

---

## Context an implementer needs

This is the first plan of the chain; nothing in it depends on another plan. It starts from the
tree the core-foundation chain left: `crates/buildl-core/src/types/` holds `digest.rs`,
`directory.rs`, `label.rs`, `node_id.rs`, `provenance.rs`, `target_name.rs`, `timestamp.rs` and an
export-only `mod.rs`; `crates/buildl-core/src/error.rs` holds `Error::InvalidName { kind, value,
reason }` (display `invalid {kind}: {value:?} — {reason}`) and `NameKind` with the four variants
`Directory | TargetName | Label | Digest`; `crates/buildl-core/src/lib.rs` ends with
`pub use types::{Digest, Directory, Label, NodeId, Provenance, TargetName, Timestamp};`. The
unit-test total before task 1 is 54 (`cargo test -p buildl-core`), and there are no doctests.

Plans `02` (`Written<T>`) and `03` (`Declaration`, `BuildFile`) build on the types made here and add
their own lines to `types/mod.rs` and `lib.rs`; this plan touches neither of their files. No
crate manifest, dependency or `crates/expected-edges.txt` line changes.

Gate facts that turn `cargo make dod` red under `-D warnings`, each one already satisfied by the
code below:

- **`dead_code` on the grammar module.** `grammar.rs` is private (`mod grammar;`, its functions
  `pub(super)`), so a grammar function with no caller outside its own tests is dead code. Each
  function therefore arrives in the task that adds its first caller: `path` and `name` in task 2
  (with the `Directory`/`TargetName` refactor), `text` in task 6 (`Argument`), `key` in task 8
  (`SettingName`). The module doc names only the functions present at each step, because an
  intra-doc link to a missing function fails `cargo doc` with `RUSTDOCFLAGS=-D warnings`.
- **`pub(crate)` inside a private module** fails `clippy::redundant_pub_crate`; `pub(super)` is clean.
- **`clippy::missing_const_for_fn`**: `is_name_byte` is a `const fn`; `as_str` on a `String`
  newtype cannot be and passes as written.
- **`clippy::unwrap_used` is denied.** Every test module that unwraps opens with
  `#![expect(clippy::unwrap_used, reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal")]`,
  and only those: `grammar.rs`'s and `diagnostic.rs`'s tests do not unwrap, so they carry no
  `expect` (an unfulfilled `expect` warns, which fails the gate). `error.rs`'s test module gains no
  `expect` here either — task 1 only adds `assert_eq!`s.
- **`missing_docs`, `must_use_candidate`, `doc_markdown`, `missing_errors_doc`**: every `pub` item
  is documented, every value-returning `pub fn` is `#[must_use]` (the `Result`-returning `parse`
  and `Command::new` are exempt — `Result` is already `must_use`), bare identifiers in docs are
  backticked, and every `Result`-returning `pub fn` has an `# Errors` section.
- **`guard-core-purity`** greps `crates/buildl-core` for `clippy::(disallowed_|all\b|style\b)`;
  nothing below writes those strings. No `#[allow]` anywhere.

`Directory`'s and `TargetName`'s existing tests stay green and unchanged through task 2's refactor:
the reasons the grammar returns are the exact strings the two `parse` functions returned before.
`Directory::parse` keeps its own empty-string case (the workspace root, accepted) ahead of the
grammar call, because the shared `path` grammar rejects the empty string — a source path is never
the root.

Run `cargo fmt --all` before every `cargo make dod`. Every Rust block below is in rustfmt's form,
but `fmt-check` is the gate's first step and a re-wrapped line fails it before any test runs.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional
Commits with scope `buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/error.rs              — [modify] ten new NameKind variants and phrases (task 1)
crates/buildl-core/src/types/grammar.rs      — [create] path, name (task 2); [modify] text (task 6), key (task 8)
crates/buildl-core/src/types/directory.rs    — [modify] parse calls grammar::path; MAX_LEN removed (task 2)
crates/buildl-core/src/types/target_name.rs  — [modify] parse calls grammar::name; MAX_LEN removed (task 2)
crates/buildl-core/src/types/source_path.rs  — [create] SourcePath (task 3)
crates/buildl-core/src/types/output_name.rs  — [create] OutputName (task 4)
crates/buildl-core/src/types/env_name.rs     — [create] EnvName (task 5)
crates/buildl-core/src/types/argument.rs     — [create] Argument and Command (task 6)
crates/buildl-core/src/types/description.rs  — [create] Description (task 7)
crates/buildl-core/src/types/setting.rs      — [create] SettingName and SettingValue (task 8)
crates/buildl-core/src/types/entry_name.rs   — [create] EntryName (task 9)
crates/buildl-core/src/types/field_name.rs   — [create] FieldName (task 10)
crates/buildl-core/src/types/diagnostic.rs   — [create] Diagnostic (task 11)
crates/buildl-core/src/types/mod.rs          — [modify] `mod grammar;` (task 2); one module, re-export and bullet per type (tasks 3–11)
crates/buildl-core/src/lib.rs                — [modify] the `pub use types::{…}` list grows per type (tasks 3–11)
```

---

## Task 1 — Add a `NameKind` variant per new value type

Every type this plan adds reports its rejections as `Error::InvalidName { kind, .. }`, so the kinds
must exist before the first type compiles.

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. In `crates/buildl-core/src/error.rs`, extend the existing test
   `name_kind_renders_a_human_phrase` so it reads:

   ```rust
   #[test]
   fn name_kind_renders_a_human_phrase() {
       assert_eq!(NameKind::Directory.to_string(), "directory");
       assert_eq!(NameKind::TargetName.to_string(), "target name");
       assert_eq!(NameKind::Label.to_string(), "label");
       assert_eq!(NameKind::Digest.to_string(), "digest");
       assert_eq!(NameKind::SourcePath.to_string(), "source path");
       assert_eq!(NameKind::OutputName.to_string(), "output name");
       assert_eq!(NameKind::EnvName.to_string(), "environment variable name");
       assert_eq!(NameKind::Argument.to_string(), "argument");
       assert_eq!(NameKind::Command.to_string(), "command");
       assert_eq!(NameKind::Description.to_string(), "description");
       assert_eq!(NameKind::SettingName.to_string(), "setting name");
       assert_eq!(NameKind::SettingValue.to_string(), "setting value");
       assert_eq!(NameKind::EntryName.to_string(), "entry file name");
       assert_eq!(NameKind::FieldName.to_string(), "field name");
   }
   ```

2. Run and confirm failure (ten errors, one per new variant; the first shown):

   ```
   $ cargo test -p buildl-core
   error[E0599]: no variant, associated function, or constant named `SourcePath` found for enum `NameKind` in the current scope
     --> crates/buildl-core/src/error.rs:90:30
   …
   error: could not compile `buildl-core` (lib test) due to 10 previous errors
   ```

3. Append the variants to `NameKind`, after `Digest`, so the enum reads:

   ```rust
   /// Which domain name an [`Error::InvalidName`] is about.
   ///
   /// A type rather than a string so classification is read as a field. Non-exhaustive for the same
   /// reason [`Error`] is.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum NameKind {
       /// A workspace-relative directory path.
       Directory,
       /// The name half of a label.
       TargetName,
       /// A whole label.
       Label,
       /// A content digest.
       Digest,
       /// A workspace-relative source file path.
       SourcePath,
       /// A path inside a target's output directory.
       OutputName,
       /// An environment variable name.
       EnvName,
       /// One element of a command's argument vector.
       Argument,
       /// A whole command: a non-empty argument vector.
       Command,
       /// A rule's one-line description.
       Description,
       /// A build setting's name.
       SettingName,
       /// A build setting's value.
       SettingValue,
       /// The file name every directory's build file has.
       EntryName,
       /// A field name in a declaration's option table.
       FieldName,
   }
   ```

   and replace the `impl fmt::Display for NameKind` block with:

   ```rust
   impl fmt::Display for NameKind {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Directory => "directory",
               Self::TargetName => "target name",
               Self::Label => "label",
               Self::Digest => "digest",
               Self::SourcePath => "source path",
               Self::OutputName => "output name",
               Self::EnvName => "environment variable name",
               Self::Argument => "argument",
               Self::Command => "command",
               Self::Description => "description",
               Self::SettingName => "setting name",
               Self::SettingValue => "setting value",
               Self::EntryName => "entry file name",
               Self::FieldName => "field name",
           })
       }
   }
   ```

   The doc comment and derives above `pub enum NameKind` are unchanged; they are shown so the
   block is complete. `#[non_exhaustive]` makes the additions non-breaking for downstream crates.

4. Run and confirm green — no test is added; the extended one now passes:

   ```
   $ cargo test -p buildl-core
   running 54 tests
   test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add a name kind for each declaration value type`.

---

## Task 2 — Share the path and name grammars through one private module

`Directory::parse` and `TargetName::parse` move onto the shared grammars in the same task that
creates them, because a grammar function without a caller outside its tests is dead code.

**Files:**
- Create `crates/buildl-core/src/types/grammar.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/types/directory.rs`
- Modify `crates/buildl-core/src/types/target_name.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/grammar.rs` with the test module
   only (it does not unwrap, so it carries no `expect`):

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       use super::{name, path};

       #[test]
       fn path_accepts_nested_workspace_relative_paths() {
           for raw in ["lib", "lib/text", "src/main.c", "a-b_c.d/e"] {
               assert_eq!(path(raw), Ok(()), "{raw} should pass");
           }
       }

       #[test]
       fn path_rejects_each_documented_form_with_its_reason() {
           assert_eq!(path(""), Err("must not be empty"));
           assert_eq!(path(&"a".repeat(1025)), Err("must be at most 1024 bytes"));
           assert_eq!(path("/lib"), Err("must not start or end with '/'"));
           assert_eq!(path("lib/"), Err("must not start or end with '/'"));
           assert_eq!(
               path("lib//text"),
               Err("must not contain an empty path segment")
           );
           assert_eq!(
               path("lib/../x"),
               Err("must not contain a '.' or '..' segment")
           );
           assert_eq!(path("."), Err("must not contain a '.' or '..' segment"));
           assert_eq!(
               path("a b"),
               Err("segments may hold only ASCII letters, digits, '.', '-' and '_'")
           );
           assert_eq!(path(&"a".repeat(1024)), Ok(()));
       }

       #[test]
       fn name_rejects_each_documented_form_with_its_reason() {
           assert_eq!(name("main.o"), Ok(()));
           assert_eq!(name(""), Err("must not be empty"));
           assert_eq!(name(&"a".repeat(257)), Err("must be at most 256 bytes"));
           assert_eq!(name(&"a".repeat(256)), Ok(()));
           assert_eq!(name(".."), Err("must not be '.' or '..'"));
           assert_eq!(
               name("a/b"),
               Err("may hold only ASCII letters, digits, '.', '-' and '_'")
           );
       }
   }
   ```

   Declare the module privately in `crates/buildl-core/src/types/mod.rs`: insert `mod grammar;`
   followed by one blank line directly above `pub mod digest;`, so the declarations open:

   ```rust
   mod grammar;

   pub mod digest;
   pub mod directory;
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `super::name`, `super::path`
    --> crates/buildl-core/src/types/grammar.rs:5:17
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! The character grammars several domain types share.
   //!
   //! Its own file because a grammar shared by two types belongs to neither: a source path and a
   //! directory are different concepts written in the same characters, and a setting name and a
   //! field name likewise. Each type keeps its own [`NameKind`](crate::NameKind) and wraps the
   //! reason returned here into its own error.
   //!
   //! Responsibilities: the shared grammars — [`path`] and [`name`].
   //!
   //! Non-responsibilities: construction. Nothing here builds a value; each function only says why
   //! a candidate is rejected.

   /// Longest accepted path, in bytes.
   const PATH_MAX_LEN: usize = 1024;

   /// Longest accepted name or key, in bytes.
   const NAME_MAX_LEN: usize = 256;

   /// Whether `byte` may appear in a path segment or a name.
   const fn is_name_byte(byte: u8) -> bool {
       byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' || byte == b'_'
   }

   /// A non-empty workspace-relative path: `/`-separated segments of ASCII letters, digits, `.`,
   /// `-` and `_`, none of them empty, `.` or `..`, at most 1024 bytes in all.
   pub(super) fn path(raw: &str) -> Result<(), &'static str> {
       if raw.is_empty() {
           return Err("must not be empty");
       }
       if raw.len() > PATH_MAX_LEN {
           return Err("must be at most 1024 bytes");
       }
       if raw.starts_with('/') || raw.ends_with('/') {
           return Err("must not start or end with '/'");
       }
       for segment in raw.split('/') {
           if segment.is_empty() {
               return Err("must not contain an empty path segment");
           }
           if segment == "." || segment == ".." {
               return Err("must not contain a '.' or '..' segment");
           }
           if !segment.bytes().all(is_name_byte) {
               return Err("segments may hold only ASCII letters, digits, '.', '-' and '_'");
           }
       }
       Ok(())
   }

   /// A single name: ASCII letters, digits, `.`, `-` and `_`, neither `.` nor `..`, 1 to 256 bytes.
   pub(super) fn name(raw: &str) -> Result<(), &'static str> {
       if raw.is_empty() {
           return Err("must not be empty");
       }
       if raw.len() > NAME_MAX_LEN {
           return Err("must be at most 256 bytes");
       }
       if raw == "." || raw == ".." {
           return Err("must not be '.' or '..'");
       }
       if !raw.bytes().all(is_name_byte) {
           return Err("may hold only ASCII letters, digits, '.', '-' and '_'");
       }
       Ok(())
   }
   ```

   Then move `Directory` onto `path`. In `crates/buildl-core/src/types/directory.rs`, add
   `use crate::types::grammar;` directly below `use crate::error::{Error, NameKind, Result};`, and
   replace the whole `impl Directory { … }` block with (the `MAX_LEN` constant goes — its value now
   lives in `grammar.rs` as `PATH_MAX_LEN`):

   ```rust
   impl Directory {
       /// The workspace root.
       #[must_use]
       pub const fn root() -> Self {
           Self(String::new())
       }

       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` exceeds 1024 bytes, starts or ends with `/`,
       /// holds an empty, `.` or `..` segment, or holds a character outside ASCII alphanumerics,
       /// `.`, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           if raw.is_empty() {
               return Ok(Self(raw));
           }
           match grammar::path(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::Directory,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The path as a string slice; empty for the workspace root.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }

       /// Whether this is the workspace root.
       #[must_use]
       pub const fn is_root(&self) -> bool {
           self.0.is_empty()
       }
   }
   ```

   Move `TargetName` onto `name`. In `crates/buildl-core/src/types/target_name.rs`, add
   `use crate::types::grammar;` directly below `use crate::error::{Error, NameKind, Result};`, and
   replace the whole `impl TargetName { … }` block with (its `MAX_LEN` becomes `NAME_MAX_LEN`):

   ```rust
   impl TargetName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`,
       /// or holds a character outside ASCII alphanumerics, `.`, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::name(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::TargetName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The name as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }
   ```

   No other line of either file changes, and neither file's tests are touched.

4. Run and confirm green; the refactored types' existing tests (`types::directory::tests::*`,
   `types::target_name::tests::*`) pass unchanged:

   ```
   $ cargo test -p buildl-core
   running 57 tests
   test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::grammar::tests::path_accepts_nested_workspace_relative_paths`
   - `types::grammar::tests::path_rejects_each_documented_form_with_its_reason`
   - `types::grammar::tests::name_rejects_each_documented_form_with_its_reason`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `refactor(buildl-core): share the path and name grammars through one private module`.

---

## Task 3 — `SourcePath`, a file a target reads

The first declaration value: the shared path grammar with the root excluded.

**Files:**
- Create `crates/buildl-core/src/types/source_path.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/source_path.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::SourcePath;

       #[test]
       fn source_path_accepts_the_documented_forms() {
           for raw in ["src/main.c", "lib/text/a-b_c.d", "main.o"] {
               assert!(SourcePath::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn source_path_rejects_the_documented_forms() {
           for raw in ["", "/etc/passwd", "../x", "src/../x", "a b", "src/"] {
               assert!(
                   SourcePath::parse(raw).is_err(),
                   "{raw:?} should be rejected"
               );
           }
       }

       #[test]
       fn source_path_round_trips_through_json_as_a_string() {
           let value = SourcePath::parse("src/main.c").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<SourcePath>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "src/main.c");
           assert!(serde_json::from_str::<SourcePath>("\"../x\"").is_err());
       }

       #[test]
       fn source_path_from_str_routes_through_parse() {
           assert_eq!(
               "src/main.c".parse::<SourcePath>().unwrap().as_str(),
               "src/main.c"
           );
           assert!("".parse::<SourcePath>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod source_path;` between `pub mod provenance;` and `pub mod target_name;`;
   - add `pub use source_path::SourcePath;` between `pub use provenance::Provenance;` and `pub use target_name::TargetName;`;
   - add this bullet as the last item of the `Responsibilities:` list, directly below the
     `Timestamp` bullet:

   ```rust
   //! - [`SourcePath`] — the validated values a declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{Digest, Directory, Label, NodeId, Provenance, SourcePath, TargetName, Timestamp};
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `source_path::SourcePath`
     --> crates/buildl-core/src/types/mod.rs:39:9
   error[E0432]: unresolved import `super::SourcePath`
     --> crates/buildl-core/src/types/source_path.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A workspace-relative path to a source file, validated at construction.
   //!
   //! Its own file because a source file is a different concept from a directory even though both
   //! are written in the same characters: a source path is never the workspace root.
   //!
   //! Responsibilities: [`SourcePath`], its [`SourcePath::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: the filesystem. A `SourcePath` names a file; whether it exists is an
   //! adapter's question.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// A workspace-relative path to a file a target reads, such as `src/main.c`.
   ///
   /// Valid paths are non-empty, at most 1024 bytes, hold no leading or trailing `/`, and consist of
   /// `/`-separated non-empty segments of ASCII letters, digits, `.`, `-` and `_`, none of which is
   /// `.` or `..` — so a source path can never leave the workspace.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct SourcePath(String);

   impl SourcePath {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 1024 bytes, starts or ends with `/`, holds an
       /// empty, `.` or `..` segment, or holds a character outside ASCII alphanumerics, `.`, `-`
       /// and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::path(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::SourcePath,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for SourcePath {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for SourcePath {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for SourcePath {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<SourcePath> for String {
       fn from(value: SourcePath) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for SourcePath {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 61 tests
   test result: ok. 61 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::source_path::tests::source_path_accepts_the_documented_forms`
   - `types::source_path::tests::source_path_rejects_the_documented_forms`
   - `types::source_path::tests::source_path_round_trips_through_json_as_a_string`
   - `types::source_path::tests::source_path_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate a workspace-relative source path`.

---

## Task 4 — `OutputName`, a path inside a target's output directory

Same grammar as `SourcePath`, a different concept: it is relative to the target, not the workspace.

**Files:**
- Create `crates/buildl-core/src/types/output_name.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/output_name.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::OutputName;

       #[test]
       fn output_name_accepts_the_documented_forms() {
           for raw in ["app", "bin/app", "main.o"] {
               assert!(OutputName::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn output_name_rejects_the_documented_forms() {
           for raw in ["", "/app", "../app", "a b"] {
               assert!(
                   OutputName::parse(raw).is_err(),
                   "{raw:?} should be rejected"
               );
           }
       }

       #[test]
       fn output_name_round_trips_through_json_as_a_string() {
           let value = OutputName::parse("bin/app").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<OutputName>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "bin/app");
           assert!(serde_json::from_str::<OutputName>("\"/app\"").is_err());
       }

       #[test]
       fn output_name_from_str_routes_through_parse() {
           assert_eq!("bin/app".parse::<OutputName>().unwrap().as_str(), "bin/app");
           assert!("".parse::<OutputName>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod output_name;` between `pub mod node_id;` and `pub mod provenance;`;
   - add `pub use output_name::OutputName;` between `pub use node_id::NodeId;` and `pub use provenance::Provenance;`;
   - replace the declaration-values bullet added in task 3 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`] — the validated values a declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Digest, Directory, Label, NodeId, OutputName, Provenance, SourcePath, TargetName, Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `output_name::OutputName`
     --> crates/buildl-core/src/types/mod.rs:39:9
   error[E0432]: unresolved import `super::OutputName`
     --> crates/buildl-core/src/types/output_name.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A path inside a target's output directory, validated at construction.
   //!
   //! Its own file because an output is named relative to the target that produces it, not to the
   //! workspace: the same grammar as a source path, a different concept.
   //!
   //! Responsibilities: [`OutputName`], its [`OutputName::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: where the output directory is. The executor decides that.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// A path inside a target's output directory, such as `app` or `bin/app`.
   ///
   /// Valid names follow the source-path grammar: non-empty, at most 1024 bytes, `/`-separated
   /// non-empty segments of ASCII letters, digits, `.`, `-` and `_`, none of which is `.` or `..`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct OutputName(String);

   impl OutputName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 1024 bytes, starts or ends with `/`, holds an
       /// empty, `.` or `..` segment, or holds a character outside ASCII alphanumerics, `.`, `-`
       /// and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::path(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::OutputName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for OutputName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for OutputName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for OutputName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<OutputName> for String {
       fn from(value: OutputName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for OutputName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 65 tests
   test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::output_name::tests::output_name_accepts_the_documented_forms`
   - `types::output_name::tests::output_name_rejects_the_documented_forms`
   - `types::output_name::tests::output_name_round_trips_through_json_as_a_string`
   - `types::output_name::tests::output_name_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate a target's output name`.

---

## Task 5 — `EnvName`, an environment variable name

The portable shell-variable grammar. It is used by this type alone, so it stays private in the type's own file rather than in `grammar.rs`.

**Files:**
- Create `crates/buildl-core/src/types/env_name.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/env_name.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::EnvName;

       #[test]
       fn env_name_accepts_the_documented_forms() {
           for raw in ["PATH", "GOCACHE", "_x1", "cargo_home"] {
               assert!(EnvName::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn env_name_rejects_the_documented_forms() {
           for raw in ["", "1PATH", "A-B", "A B", "A.B"] {
               assert!(EnvName::parse(raw).is_err(), "{raw:?} should be rejected");
           }
           assert!(EnvName::parse("A".repeat(257)).is_err());
           assert!(EnvName::parse("A".repeat(256)).is_ok());
       }

       #[test]
       fn env_name_round_trips_through_json_as_a_string() {
           let value = EnvName::parse("GOCACHE").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<EnvName>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "GOCACHE");
           assert!(serde_json::from_str::<EnvName>("\"1PATH\"").is_err());
       }

       #[test]
       fn env_name_from_str_routes_through_parse() {
           assert_eq!("GOCACHE".parse::<EnvName>().unwrap().as_str(), "GOCACHE");
           assert!("".parse::<EnvName>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod env_name;` between `pub mod directory;` and `pub mod label;`;
   - add `pub use env_name::EnvName;` between `pub use directory::Directory;` and `pub use label::Label;`;
   - replace the declaration-values bullet from task 4 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`] — the validated values a declaration's fields
   //!   hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Digest, Directory, EnvName, Label, NodeId, OutputName, Provenance, SourcePath, TargetName,
       Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `env_name::EnvName`
     --> crates/buildl-core/src/types/mod.rs:39:9
   error[E0432]: unresolved import `super::EnvName`
     --> crates/buildl-core/src/types/env_name.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! An environment variable name, validated at construction.
   //!
   //! Its own file because an environment variable a target reads is part of its action key and is
   //! later checked against the workspace ceiling, so the name must be well formed before either.
   //!
   //! Responsibilities: [`EnvName`], its [`EnvName::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: the environment. Nothing here reads a variable's value.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};

   /// Longest accepted name, in bytes.
   const MAX_LEN: usize = 256;

   /// The portable shell-variable grammar, returning the reason a candidate is rejected.
   fn env_name_grammar(raw: &str) -> core::result::Result<(), &'static str> {
       let Some(first) = raw.bytes().next() else {
           return Err("must not be empty");
       };
       if raw.len() > MAX_LEN {
           return Err("must be at most 256 bytes");
       }
       if !(first.is_ascii_alphabetic() || first == b'_') {
           return Err("must start with an ASCII letter or '_'");
       }
       if !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
           return Err("may hold only ASCII letters, digits and '_'");
       }
       Ok(())
   }

   /// An environment variable name, such as `PATH` or `GOCACHE`.
   ///
   /// Valid names are 1 to 256 bytes, start with an ASCII letter or `_`, and hold only ASCII letters,
   /// digits and `_` — the portable shell-variable grammar.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct EnvName(String);

   impl EnvName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, starts with a character other than
       /// an ASCII letter or `_`, or holds a character outside ASCII alphanumerics and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match env_name_grammar(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::EnvName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for EnvName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for EnvName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for EnvName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<EnvName> for String {
       fn from(value: EnvName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for EnvName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 69 tests
   test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::env_name::tests::env_name_accepts_the_documented_forms`
   - `types::env_name::tests::env_name_rejects_the_documented_forms`
   - `types::env_name::tests::env_name_round_trips_through_json_as_a_string`
   - `types::env_name::tests::env_name_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate an environment variable name`.

---

## Task 6 — `Argument` and `Command`, a non-empty argument vector

`Argument` is the first caller of the `text` grammar, so `text` arrives here. `Command` adds one
rule of its own — at least one argument — and serializes as a JSON array through
`try_from = "Vec<Argument>"`, so an empty array is rejected on the way in.

**Files:**
- Modify `crates/buildl-core/src/types/grammar.rs`
- Create `crates/buildl-core/src/types/argument.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing grammar test. In `crates/buildl-core/src/types/grammar.rs`, change the test
   module's import to `use super::{name, path, text};` and append this test after
   `name_rejects_each_documented_form_with_its_reason`, as the module's last test:

   ```rust
   #[test]
   fn text_rejects_only_a_nul_byte() {
       assert_eq!(text(""), Ok(()));
       assert_eq!(text("$opt:name with spaces"), Ok(()));
       assert_eq!(text("a\0b"), Err("must not contain a NUL byte"));
   }
   ```

2. Write the failing type tests. Create `crates/buildl-core/src/types/argument.rs` with the test
   module only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{Argument, Command};

       fn args(raw: &[&str]) -> Vec<Argument> {
           raw.iter().map(|a| Argument::parse(*a).unwrap()).collect()
       }

       #[test]
       fn argument_accepts_the_documented_forms() {
           for raw in ["cc", "$in", "", "-o", "a b"] {
               assert!(Argument::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn argument_rejects_a_nul_byte() {
           assert!(Argument::parse("a\0b").is_err());
           assert!("a\0b".parse::<Argument>().is_err());
       }

       #[test]
       fn argument_round_trips_through_json_as_a_string() {
           let value = Argument::parse("$out").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(json, r#""$out""#);
           assert_eq!(serde_json::from_str::<Argument>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "$out");
           assert_eq!(value.as_str(), "$out");
           assert!(serde_json::from_str::<Argument>(r#""a\u0000b""#).is_err());
       }

       #[test]
       fn command_keeps_its_arguments_in_order() {
           let command = Command::new(args(&["cc", "-c", "$in"])).unwrap();
           let rendered: Vec<&str> = command.arguments().iter().map(Argument::as_str).collect();
           assert_eq!(rendered, ["cc", "-c", "$in"]);
       }

       #[test]
       fn command_rejects_an_empty_argument_vector() {
           let err = Command::new(Vec::new()).unwrap_err();
           assert_eq!(
               err.to_string(),
               r#"invalid command: "" — must hold at least one argument"#
           );
       }

       #[test]
       fn command_round_trips_through_json_as_an_array() {
           let command = Command::new(args(&["go", "test"])).unwrap();
           let json = serde_json::to_string(&command).unwrap();
           assert_eq!(json, r#"["go","test"]"#);
           assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
           assert!(serde_json::from_str::<Command>("[]").is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod argument;` directly below `mod grammar;` and its blank line, above `pub mod digest;`;
   - add `pub use argument::{Argument, Command};` above `pub use digest::Digest;`;
   - replace the declaration-values bullet from task 5 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`] — the validated
   //!   values a declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Argument, Command, Digest, Directory, EnvName, Label, NodeId, OutputName, Provenance,
       SourcePath, TargetName, Timestamp,
   };
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `argument::Argument`, `argument::Command`
     --> crates/buildl-core/src/types/mod.rs:38:20
   error[E0432]: unresolved import `super::text`
     --> crates/buildl-core/src/types/grammar.rs:69:29
   error[E0432]: unresolved imports `super::Argument`, `super::Command`
     --> crates/buildl-core/src/types/argument.rs:10:17
   ```

4. Add `text` to `crates/buildl-core/src/types/grammar.rs`, after `name` and before the test
   module:

   ```rust
   /// Free text that can travel in an argument vector: anything but a `NUL` byte.
   pub(super) fn text(raw: &str) -> Result<(), &'static str> {
       if raw.contains('\0') {
           return Err("must not contain a NUL byte");
       }
       Ok(())
   }
   ```

   and change the module doc's responsibilities line to:

   ```rust
   //! Responsibilities: the shared grammars — [`path`], [`name`] and [`text`].
   ```

   Replace the placeholder doc comment of `crates/buildl-core/src/types/argument.rs` and add the
   implementation above its test module:

   ```rust
   //! One argument of a command, and a whole command, each validated at construction.
   //!
   //! Its own file because a command is nothing but its arguments: the two types are read and
   //! changed together, and a command's only rule beyond its arguments' is that it has one.
   //!
   //! Responsibilities: [`Argument`] and [`Command`], their constructors, and their rendering.
   //!
   //! Non-responsibilities: placeholder expansion. `$in`, `$out`, `$deps` and `$opt:name` stay text
   //! here; the executor expands them when the action runs.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// One element of a command's argument vector, such as `cc` or `$in`.
   ///
   /// Any UTF-8 text without a `NUL` byte, the empty string included: an argument vector cannot
   /// carry `NUL`, and an empty argument is a legitimate thing to pass.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Argument(String);

   impl Argument {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` holds a `NUL` byte.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::text(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::Argument,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The argument as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for Argument {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for Argument {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for Argument {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Argument> for String {
       fn from(value: Argument) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for Argument {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }

   /// A command to run: a non-empty argument vector whose first element names the program.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "Vec<Argument>", try_from = "Vec<Argument>")]
   pub struct Command(Vec<Argument>);

   impl Command {
       /// Wraps `arguments` as a command.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `arguments` is empty: a command with no program is
       /// meaningless.
       pub fn new(arguments: Vec<Argument>) -> Result<Self> {
           if arguments.is_empty() {
               return Err(Error::InvalidName {
                   kind: NameKind::Command,
                   value: String::new(),
                   reason: "must hold at least one argument",
               });
           }
           Ok(Self(arguments))
       }

       /// The arguments, program first.
       #[must_use]
       pub fn arguments(&self) -> &[Argument] {
           &self.0
       }
   }

   impl From<Command> for Vec<Argument> {
       fn from(value: Command) -> Self {
           value.0
       }
   }

   impl TryFrom<Vec<Argument>> for Command {
       type Error = Error;
       fn try_from(arguments: Vec<Argument>) -> Result<Self> {
           Self::new(arguments)
       }
   }
   ```

   `arguments()` is not `const`: it derefs a `Vec`, which is not a const operation, the same reason
   `as_str` is not.

5. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 76 tests
   test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::grammar::tests::text_rejects_only_a_nul_byte`
   - `types::argument::tests::argument_accepts_the_documented_forms`
   - `types::argument::tests::argument_rejects_a_nul_byte`
   - `types::argument::tests::argument_round_trips_through_json_as_a_string`
   - `types::argument::tests::command_keeps_its_arguments_in_order`
   - `types::argument::tests::command_rejects_an_empty_argument_vector`
   - `types::argument::tests::command_round_trips_through_json_as_an_array`

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): validate a command and its arguments`.

---

## Task 7 — `Description`, a rule's one status line

Its grammar — at most 1024 bytes, no control character — is used by this type alone, so it stays private in the type's file.

**Files:**
- Create `crates/buildl-core/src/types/description.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/description.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Description;

       #[test]
       fn description_accepts_the_documented_forms() {
           for raw in ["", "compile $in", "link → app"] {
               assert!(Description::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn description_rejects_the_documented_forms() {
           for raw in ["a\nb", "a\tb", "a\0b"] {
               assert!(
                   Description::parse(raw).is_err(),
                   "{raw:?} should be rejected"
               );
           }
           assert!(Description::parse("a".repeat(1025)).is_err());
           assert!(Description::parse("a".repeat(1024)).is_ok());
       }

       #[test]
       fn description_round_trips_through_json_as_a_string() {
           let value = Description::parse("compile $in").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<Description>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "compile $in");
           assert!(serde_json::from_str::<Description>("\"a\\nb\"").is_err());
       }

       #[test]
       fn description_from_str_routes_through_parse() {
           assert_eq!(
               "compile $in".parse::<Description>().unwrap().as_str(),
               "compile $in"
           );
           assert!("a\nb".parse::<Description>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod description;` between `pub mod argument;` and `pub mod digest;`;
   - add `pub use description::Description;` between `pub use argument::{Argument, Command};` and `pub use digest::Digest;`;
   - replace the declaration-values bullet from task 6 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`] —
   //!   the validated values a declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Argument, Command, Description, Digest, Directory, EnvName, Label, NodeId, OutputName,
       Provenance, SourcePath, TargetName, Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `description::Description`
     --> crates/buildl-core/src/types/mod.rs:40:9
   error[E0432]: unresolved import `super::Description`
     --> crates/buildl-core/src/types/description.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A rule's one-line description, validated at construction.
   //!
   //! Its own file because a description is shown on the status line while an action runs, so it
   //! must fit on one line.
   //!
   //! Responsibilities: [`Description`], its [`Description::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: placeholder expansion. A `$in` inside a description stays text here.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};

   /// Longest accepted description, in bytes.
   const MAX_LEN: usize = 1024;

   /// One status line, returning the reason a candidate is rejected.
   fn description_grammar(raw: &str) -> core::result::Result<(), &'static str> {
       if raw.len() > MAX_LEN {
           return Err("must be at most 1024 bytes");
       }
       if raw.chars().any(char::is_control) {
           return Err("must be one line with no control characters");
       }
       Ok(())
   }

   /// A rule's description, such as `compile $in`.
   ///
   /// Any UTF-8 text of at most 1024 bytes with no control character, the empty string included.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Description(String);

   impl Description {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` exceeds 1024 bytes or holds a control character such as a newline.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match description_grammar(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::Description,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for Description {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for Description {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for Description {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Description> for String {
       fn from(value: Description) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for Description {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 80 tests
   test result: ok. 80 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::description::tests::description_accepts_the_documented_forms`
   - `types::description::tests::description_rejects_the_documented_forms`
   - `types::description::tests::description_round_trips_through_json_as_a_string`
   - `types::description::tests::description_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate a rule's one-line description`.

---

## Task 8 — `SettingName` and `SettingValue`, a build setting

`SettingName` is the first caller of the `key` grammar, so `key` arrives here and completes
`grammar.rs`. `SettingValue` reuses `text`.

**Files:**
- Modify `crates/buildl-core/src/types/grammar.rs`
- Create `crates/buildl-core/src/types/setting.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing grammar test. In `crates/buildl-core/src/types/grammar.rs`, change the test
   module's import to `use super::{key, name, path, text};` and insert this test between
   `name_rejects_each_documented_form_with_its_reason` and `text_rejects_only_a_nul_byte`:

   ```rust
   #[test]
   fn key_rejects_each_documented_form_with_its_reason() {
       assert_eq!(key("test_filter"), Ok(()));
       assert_eq!(key("a-b"), Ok(()));
       assert_eq!(key(""), Err("must not be empty"));
       assert_eq!(key(&"a".repeat(257)), Err("must be at most 256 bytes"));
       assert_eq!(
           key("a.b"),
           Err("may hold only ASCII letters, digits, '-' and '_'")
       );
   }
   ```

2. Write the failing type tests. Create `crates/buildl-core/src/types/setting.rs` with the test
   module only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{SettingName, SettingValue};

       #[test]
       fn setting_name_accepts_the_documented_forms() {
           for raw in ["test_filter", "a-b", "X1"] {
               assert!(SettingName::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn setting_name_rejects_the_documented_forms() {
           for raw in ["", "a.b", "a:b", "a b"] {
               assert!(
                   SettingName::parse(raw).is_err(),
                   "{raw:?} should be rejected"
               );
           }
       }

       #[test]
       fn setting_name_round_trips_through_json_as_a_string() {
           let value = SettingName::parse("test_filter").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<SettingName>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "test_filter");
           assert!(serde_json::from_str::<SettingName>("\"a.b\"").is_err());
       }

       #[test]
       fn setting_name_from_str_routes_through_parse() {
           assert_eq!(
               "test_filter".parse::<SettingName>().unwrap().as_str(),
               "test_filter"
           );
           assert!("".parse::<SettingName>().is_err());
       }

       #[test]
       fn setting_value_accepts_the_documented_forms() {
           for raw in ["", "TestLogin", "a b c"] {
               assert!(SettingValue::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn setting_value_rejects_the_documented_forms() {
           assert!(SettingValue::parse("a\0b").is_err());
           assert!(SettingValue::parse("\0").is_err());
       }

       #[test]
       fn setting_value_round_trips_through_json_as_a_string() {
           let value = SettingValue::parse("TestLogin").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<SettingValue>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "TestLogin");
           assert!(serde_json::from_str::<SettingValue>("\"a\\u0000b\"").is_err());
       }

       #[test]
       fn setting_value_from_str_routes_through_parse() {
           assert_eq!(
               "TestLogin".parse::<SettingValue>().unwrap().as_str(),
               "TestLogin"
           );
           assert!("a\0b".parse::<SettingValue>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod setting;` between `pub mod provenance;` and `pub mod source_path;`;
   - add `pub use setting::{SettingName, SettingValue};` between `pub use provenance::Provenance;` and `pub use source_path::SourcePath;`;
   - replace the declaration-values bullet from task 7 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
   //!   [`SettingName`], [`SettingValue`] — the validated values a declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Argument, Command, Description, Digest, Directory, EnvName, Label, NodeId, OutputName,
       Provenance, SettingName, SettingValue, SourcePath, TargetName, Timestamp,
   };
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `setting::SettingName`, `setting::SettingValue`
     --> crates/buildl-core/src/types/mod.rs:49:19
   error[E0432]: unresolved import `super::key`
     --> crates/buildl-core/src/types/grammar.rs:77:17
   error[E0432]: unresolved imports `super::SettingName`, `super::SettingValue`
     --> crates/buildl-core/src/types/setting.rs:10:17
   ```

4. Add `key` to `crates/buildl-core/src/types/grammar.rs`, between `name` and `text`:

   ```rust
   /// A key: ASCII letters, digits, `-` and `_`, 1 to 256 bytes.
   pub(super) fn key(raw: &str) -> Result<(), &'static str> {
       if raw.is_empty() {
           return Err("must not be empty");
       }
       if raw.len() > NAME_MAX_LEN {
           return Err("must be at most 256 bytes");
       }
       if !raw
           .bytes()
           .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
       {
           return Err("may hold only ASCII letters, digits, '-' and '_'");
       }
       Ok(())
   }
   ```

   and change the module doc's responsibilities line to its final form:

   ```rust
   //! Responsibilities: the four shared grammars — [`path`], [`name`], [`key`] and [`text`].
   ```

   `grammar.rs` now holds all four grammars, and its tests run in the order `path_accepts…`,
   `path_rejects…`, `name_rejects…`, `key_rejects…`, `text_rejects…`.

   Replace the placeholder doc comment of `crates/buildl-core/src/types/setting.rs` and add the
   implementation above its test module:

   ```rust
   //! A build setting's name and value, each validated at construction.
   //!
   //! Its own file because the two halves of `b.option(name, { default })` are read together: the
   //! name is written bare in `--set name=value` and inside `$opt:name`, and the value is substituted
   //! into an argument vector.
   //!
   //! Responsibilities: [`SettingName`] and [`SettingValue`], their constructors, and their
   //! rendering.
   //!
   //! Non-responsibilities: whether a setting is declared. Matching a `$opt:name` reference to its
   //! declaration needs every build file, so it belongs to the phase that builds the graph.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// A build setting's name, such as `test_filter`.
   ///
   /// Valid names are 1 to 256 bytes of ASCII letters, digits, `-` and `_`. Excluding `.` and `:`
   /// is what lets a `$opt:name` reference end cleanly.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct SettingName(String);

   impl SettingName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, or holds a character outside ASCII
       /// alphanumerics, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::key(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::SettingName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for SettingName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for SettingName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for SettingName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<SettingName> for String {
       fn from(value: SettingName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for SettingName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }

   /// A build setting's value, such as its declared default.
   ///
   /// Any UTF-8 text without a `NUL` byte, the empty string included: the value is substituted into
   /// an argument vector, which cannot carry `NUL`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct SettingValue(String);

   impl SettingValue {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` holds a `NUL` byte.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::text(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::SettingValue,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for SettingValue {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for SettingValue {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for SettingValue {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<SettingValue> for String {
       fn from(value: SettingValue) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for SettingValue {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

5. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 89 tests
   test result: ok. 89 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::grammar::tests::key_rejects_each_documented_form_with_its_reason`
   - `types::setting::tests::setting_name_accepts_the_documented_forms`
   - `types::setting::tests::setting_name_rejects_the_documented_forms`
   - `types::setting::tests::setting_name_round_trips_through_json_as_a_string`
   - `types::setting::tests::setting_name_from_str_routes_through_parse`
   - `types::setting::tests::setting_value_accepts_the_documented_forms`
   - `types::setting::tests::setting_value_rejects_the_documented_forms`
   - `types::setting::tests::setting_value_round_trips_through_json_as_a_string`
   - `types::setting::tests::setting_value_from_str_routes_through_parse`

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): validate a build setting's name and value`.

---

## Task 9 — `EntryName`, the build file's file name

The target-name grammar under a different concept: a file name joined to every directory to locate its build file.

**Files:**
- Create `crates/buildl-core/src/types/entry_name.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/entry_name.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::EntryName;

       #[test]
       fn entry_name_accepts_the_documented_forms() {
           for raw in ["build.lua", "BUILD", "build-file.lua"] {
               assert!(EntryName::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn entry_name_rejects_the_documented_forms() {
           for raw in ["", "lib/build.lua", "..", "a b"] {
               assert!(EntryName::parse(raw).is_err(), "{raw:?} should be rejected");
           }
       }

       #[test]
       fn entry_name_round_trips_through_json_as_a_string() {
           let value = EntryName::parse("build.lua").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<EntryName>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "build.lua");
           assert!(serde_json::from_str::<EntryName>("\"lib/build.lua\"").is_err());
       }

       #[test]
       fn entry_name_from_str_routes_through_parse() {
           assert_eq!(
               "build.lua".parse::<EntryName>().unwrap().as_str(),
               "build.lua"
           );
           assert!("".parse::<EntryName>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod entry_name;` between `pub mod directory;` and `pub mod env_name;`;
   - add `pub use entry_name::EntryName;` between `pub use directory::Directory;` and `pub use env_name::EnvName;`;
   - replace the declaration-values bullet from task 8 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
   //!   [`SettingName`], [`SettingValue`], [`EntryName`] — the validated values a declaration's
   //!   fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Argument, Command, Description, Digest, Directory, EntryName, EnvName, Label, NodeId,
       OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName, Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `entry_name::EntryName`
     --> crates/buildl-core/src/types/mod.rs:46:9
   error[E0432]: unresolved import `super::EntryName`
     --> crates/buildl-core/src/types/entry_name.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! The file name of every directory's build file, validated at construction.
   //!
   //! Its own file because the entry name is a file name, not a target name, even though the two
   //! share a grammar: it is joined to a directory to locate a build file.
   //!
   //! Responsibilities: [`EntryName`], its [`EntryName::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: choosing it. The workspace manifest names it; this type only checks it.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// The file name a build file has in every directory, such as `build.lua`.
   ///
   /// Valid names are non-empty, at most 256 bytes, are neither `.` nor `..`, and hold only ASCII
   /// letters, digits, `.`, `-` and `_` — so an entry name can never point into another directory.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct EntryName(String);

   impl EntryName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`, or holds a character
       /// outside ASCII alphanumerics, `.`, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::name(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::EntryName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for EntryName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for EntryName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for EntryName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<EntryName> for String {
       fn from(value: EntryName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for EntryName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 93 tests
   test result: ok. 93 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::entry_name::tests::entry_name_accepts_the_documented_forms`
   - `types::entry_name::tests::entry_name_rejects_the_documented_forms`
   - `types::entry_name::tests::entry_name_round_trips_through_json_as_a_string`
   - `types::entry_name::tests::entry_name_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate the build file's entry name`.

---

## Task 10 — `FieldName`, an option-table field

The setting-name (`key`) grammar under a different concept: the field an evaluation error names.

**Files:**
- Create `crates/buildl-core/src/types/field_name.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/field_name.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::FieldName;

       #[test]
       fn field_name_accepts_the_documented_forms() {
           for raw in ["deps", "outputs", "always"] {
               assert!(FieldName::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn field_name_rejects_the_documented_forms() {
           for raw in ["", "a.b", "a b"] {
               assert!(FieldName::parse(raw).is_err(), "{raw:?} should be rejected");
           }
       }

       #[test]
       fn field_name_round_trips_through_json_as_a_string() {
           let value = FieldName::parse("deps").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(serde_json::from_str::<FieldName>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "deps");
           assert!(serde_json::from_str::<FieldName>("\"a.b\"").is_err());
       }

       #[test]
       fn field_name_from_str_routes_through_parse() {
           assert_eq!("deps".parse::<FieldName>().unwrap().as_str(), "deps");
           assert!("".parse::<FieldName>().is_err());
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod field_name;` between `pub mod env_name;` and `pub mod label;`;
   - add `pub use field_name::FieldName;` between `pub use env_name::EnvName;` and `pub use label::Label;`;
   - replace the declaration-values bullet from task 9 with:

   ```rust
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
   //!   [`SettingName`], [`SettingValue`], [`EntryName`], [`FieldName`] — the validated values a
   //!   declaration's fields hold.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with
   (rustfmt's form; the `pub use error::…` and `pub use ports::Clock;` lines above it are
   unchanged):

   ```rust
   pub use types::{
       Argument, Command, Description, Digest, Directory, EntryName, EnvName, FieldName, Label,
       NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName, Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `field_name::FieldName`
     --> crates/buildl-core/src/types/mod.rs:49:9
   error[E0432]: unresolved import `super::FieldName`
     --> crates/buildl-core/src/types/field_name.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! The name of a field in a declaration's option table, validated at construction.
   //!
   //! Its own file because a field name appears in an evaluation error — an unknown field, or a field
   //! holding the wrong type — where it is reported back to the author of the build file.
   //!
   //! Responsibilities: [`FieldName`], its [`FieldName::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: knowing which fields exist. The adapter that reads the option table
   //! decides that.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// A field name in a declaration's option table, such as `deps` or `outputs`.
   ///
   /// Valid names are 1 to 256 bytes of ASCII letters, digits, `-` and `_`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct FieldName(String);

   impl FieldName {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, or holds a character outside ASCII
       /// alphanumerics, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::key(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::FieldName,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for FieldName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for FieldName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for FieldName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<FieldName> for String {
       fn from(value: FieldName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for FieldName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 97 tests
   test result: ok. 97 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::field_name::tests::field_name_accepts_the_documented_forms`
   - `types::field_name::tests::field_name_rejects_the_documented_forms`
   - `types::field_name::tests::field_name_round_trips_through_json_as_a_string`
   - `types::field_name::tests::field_name_from_str_routes_through_parse`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate an option-table field name`.

---

## Task 11 — `Diagnostic`, an adapter's message carried for display

The one value with no grammar: any text is accepted because nothing branches on it. It has no
`parse`, no serde and no ordering — it never reaches a cache or a sorted list.

**Files:**
- Create `crates/buildl-core/src/types/diagnostic.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/types/diagnostic.rs` with the test module
   only (it does not unwrap, so it carries no `expect`):

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       use super::Diagnostic;

       #[test]
       fn keeps_and_renders_the_text_verbatim() {
           let diagnostic = Diagnostic::new("build.lua:3: unexpected symbol near '}'");
           assert_eq!(
               diagnostic.as_str(),
               "build.lua:3: unexpected symbol near '}'"
           );
           assert_eq!(
               diagnostic.to_string(),
               "build.lua:3: unexpected symbol near '}'"
           );
       }
   }
   ```

   Edit `crates/buildl-core/src/types/mod.rs` in three places:

   - add `pub mod diagnostic;` between `pub mod description;` and `pub mod digest;`;
   - add `pub use diagnostic::Diagnostic;` between `pub use description::Description;` and
     `pub use digest::Digest;`;
   - add this bullet directly below the declaration-values bullet (which keeps its task 10 form):

   ```rust
   //! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
   ```

   Replace the whole `pub use types::…;` item at the end of `crates/buildl-core/src/lib.rs` with:

   ```rust
   pub use types::{
       Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName,
       Label, NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName,
       Timestamp,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `diagnostic::Diagnostic`
     --> crates/buildl-core/src/types/mod.rs:47:9
   error[E0432]: unresolved import `super::Diagnostic`
    --> crates/buildl-core/src/types/diagnostic.rs:5:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! An adapter's rendered message for a failure, carried for display only.
   //!
   //! Its own file because the message is the one piece of an evaluation failure that is not
   //! structured: the structured part is the failure's kind, and this text only explains it to a
   //! person. Nothing branches on it.
   //!
   //! Responsibilities: [`Diagnostic`] and its rendering.
   //!
   //! Non-responsibilities: classification. What went wrong is the failure's kind, never something
   //! parsed out of this text.

   use core::fmt;

   /// A human-readable message an adapter attaches to a failure, such as a Lua error's text.
   ///
   /// Any text is accepted: it is shown, never parsed.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct Diagnostic(String);

   impl Diagnostic {
       /// Wraps `text`.
       #[must_use]
       pub fn new(text: impl Into<String>) -> Self {
           Self(text.into())
       }

       /// The message as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for Diagnostic {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }
   ```

   `new` is not `const`: `Into::into` is a trait call, which a `const fn` cannot make.

4. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 98 tests
   test result: ok. 98 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   Tests this task adds:

   - `types::diagnostic::tests::keeps_and_renders_the_text_verbatim`

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): carry an adapter's diagnostic text for display`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all
$ cargo make dod
$ cargo make guard-core-purity
$ cargo make guard-crate-edges
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
```

All exit 0. `cargo test -p buildl-core` reports 98 unit tests (54 before the plan, 44 added) and no
doctests. `crates/buildl-core/Cargo.toml` and `crates/expected-edges.txt` are unchanged.

At the end of this plan `crates/buildl-core/src/types/mod.rs` reads exactly as follows; plans `02`
and `03` add `written`, `declaration` and `build_file` to it:

```rust
//! The domain vocabulary: the validated values the pipeline's phases hand to one another.
//!
//! Its own directory because these are the crate's values with a grammar to get wrong, and each
//! is parsed once at the edge so no later phase re-checks it. Validation lives in the sibling
//! file named for the type; this file is the index.
//!
//! Responsibilities:
//!
//! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
//! - [`Directory`] — a workspace-relative directory path.
//! - [`TargetName`] — the name half of a label.
//! - [`Label`] — a target's absolute name.
//! - [`NodeId`] — a target's handle inside the graph's arenas.
//! - [`Provenance`] — the build file and directory a declaration came from.
//! - [`Timestamp`] — an instant on the host wall clock.
//! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
//!   [`SettingName`], [`SettingValue`], [`EntryName`], [`FieldName`] — the validated values a
//!   declaration's fields hold.
//! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
//!
//! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
//! two of them to decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod grammar;

pub mod argument;
pub mod description;
pub mod diagnostic;
pub mod digest;
pub mod directory;
pub mod entry_name;
pub mod env_name;
pub mod field_name;
pub mod label;
pub mod node_id;
pub mod output_name;
pub mod provenance;
pub mod setting;
pub mod source_path;
pub mod target_name;
pub mod timestamp;

pub use argument::{Argument, Command};
pub use description::Description;
pub use diagnostic::Diagnostic;
pub use digest::Digest;
pub use directory::Directory;
pub use entry_name::EntryName;
pub use env_name::EnvName;
pub use field_name::FieldName;
pub use label::Label;
pub use node_id::NodeId;
pub use output_name::OutputName;
pub use provenance::Provenance;
pub use setting::{SettingName, SettingValue};
pub use source_path::SourcePath;
pub use target_name::TargetName;
pub use timestamp::Timestamp;
```

and `crates/buildl-core/src/lib.rs` ends with:

```rust
pub use error::{Error, NameKind, Result};
pub use ports::Clock;
pub use types::{
    Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName,
    Label, NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName,
    Timestamp,
};
```

Every grammar boundary is pinned by a test that names its reason string, every string type
round-trips through JSON as a string and rejects an invalid string on the way in, and `FromStr`
on each type routes through `parse`.

## Review findings

- test-coverage — `key` grammar test rejected 257 bytes but never accepted 256, so a `>`→`>=` slip went unpinned — `types/grammar.rs` `key_rejects_each_documented_form_with_its_reason`. Fixed (added `assert_eq!(key(&"a".repeat(256)), Ok(()));`) and verified: `cargo test -p buildl-core --lib grammar` → `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 93 filtered out`.
- test-coverage — `EnvName` and `Description` rejection tests assert rejection, not the reason string — `types/env_name.rs`, `types/description.rs`. Not acted on (non-blocking).
- docs — crate-doc row for `types` still reads "names, identities, instants" — `lib.rs:25`. Not acted on; plan 08 rewrites crate docs.
- naming — `NameKind`/`InvalidName` now classify non-names (`Argument`, `Command`, `Description`, `SettingValue`) — `error.rs`. Spec-mandated; wording only.
- style — nine doc-comment lines at 101–116 chars, copied verbatim from the plan — 8 files. Gate passes (rustfmt does not wrap comments).
- correctness — `Description` "one line" rule uses `char::is_control`, which admits U+2028/U+2029 and bidi overrides — `types/description.rs`. Meets spec's "no control characters"; not acted on.
- process — plan asks one commit per task; execution left one uncommitted tree — whole plan. Commit split is the user's call.

## Probe results

- Plan code compiles and passes the gate as written — prototype built from plans 01–07 at session scratchpad `proto/`, `cargo make dod` green during planning; execution matched: every red step produced the error kind the plan names (E0599 ×10 task 1, E0432 tasks 2–11), green counts 54→98. Not against the plan.

## Deviations

- 2026-10-06 — tasks 1–11 run by one coder sequentially rather than one coder per task (every task shares `types/mod.rs`/`lib.rs`, so no parallel batch existed); `cargo make dod` run after tasks 2, 5, 11 rather than after each; per-task commits not made (user holds the commit gate).
