---
status: done
created: 2026-10-04
depends-on: [01]
---

# Label Types Implementation Plan

**Goal:** A target's name is a validated, absolute `Label` that cannot be built invalid.

**Architecture:** Three newtypes in `crates/buildl-core/src/types/`, one file each, because a label
is two grammars joined by a `:` and `Label::resolve` validates the name half while reusing the
directory half untouched. `Label` is absolute by construction — there is no representation of an
unresolved reference, so no later phase asks whether the label it holds is relative. All three
serialize as their `Display` string, which is what lets `Label` be a JSON object key.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` 1.0 with `derive`, `cargo-make`.

---

## Context an implementer needs

Plan `01` is `done`: `crates/buildl-core/src/lib.rs` declares and re-exports `pub mod error`, and
`crates/buildl-core/src/error.rs` holds `Error::InvalidName { kind, value, reason }`, `NameKind`
with the four variants `Directory | TargetName | Label | Digest`, and
`pub type Result<T> = core::result::Result<T, Error>`. `crates/buildl-core/Cargo.toml` already
depends on `serde`, `serde_json`, `sha2` and `thiserror`, and `crates/expected-edges.txt` already
records those four edges — **this plan changes neither file**. It is independent of plan `04` in
content, but the two are not file-disjoint: both append a line to `crates/buildl-core/src/lib.rs`
(`pub mod types;` here, `pub mod json;` there). Run them in either order, or concurrently only if
you reconcile that one file by hand. Plan `03` depends on this one, because its `Provenance` holds
a `Directory`.

The grammar this plan implements comes from the spec, and `design.md` §5 is where the forms appear
in the wild: `b.target("app", { deps = { "main.o", "util.o", "//lib:text" } })` at `design.md:135-136`
mixes bare same-directory names with an absolute label, `b.alias("default", "app")` at `:140` and
`b.test("app_test", …)` at `:141` add two more names. Every name in that section —
`main.o`, `util.o`, `app`, `default`, `app_test`, `text` — must parse.

`//lib` is rejected rather than resolved to `//lib:lib`. Bazel adopts that shorthand; `design.md`
documents no such rule, and allowing it would give one target two spellings, so `Display` → `parse`
would stop round-tripping and two cache keys could mean the same target.

Three facts about the gate, each of which turns `cargo make dod` red under `-D warnings`:

- `clippy::missing_const_for_fn` fires on any method whose body is a field read or `String::new()`.
  `Directory::root` and `Directory::is_root` are `const fn` for that reason; `as_str` cannot be,
  because dereferencing a `String` is not a const operation.
- `clippy::doc_markdown` fires on a bare crate or item name in a doc comment. Write `` `serde_json` ``,
  not `serde_json`.
- `clippy::unwrap_used` is **denied** workspace-wide (`Cargo.toml:71`), so every test module opens
  with the `#![expect(clippy::unwrap_used, reason = "…")]` shown in task 1 and repeated in each
  subsequent task's test module.

Run `cargo fmt` before every `cargo make dod` in this plan. `fmt-check` is the gate's first step
(`Makefile.toml:46-50`), and every Rust block below is given in rustfmt's canonical form — but
retyping or re-wrapping one can drift, and a drifted block turns the gate red before a single test
runs.

All work happens in the worktree, on a branch, never on `main`. Commits follow Conventional
Commits with scope `buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/types/mod.rs         — [create] export-only index, grown by each task
crates/buildl-core/src/types/directory.rs   — [create] Directory and its tests (task 1)
crates/buildl-core/src/types/target_name.rs — [create] TargetName and its tests (task 2)
crates/buildl-core/src/types/label.rs       — [create] Label::parse and its tests (task 3)
                                              [modify] Label::resolve and its tests (task 4)
crates/buildl-core/src/lib.rs               — [modify] declare and re-export types (task 1)
```

---

## Task 1 — `Directory`, the `//<dir>` half

**Files:**
- Create `crates/buildl-core/src/types/directory.rs`
- Create `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/mod.rs`:

   ```rust
   //! The domain vocabulary: the validated values the pipeline's phases hand to one another.
   //!
   //! Its own directory because these are the crate's values with a grammar to get wrong, and each
   //! is parsed once at the edge so no later phase re-checks it. Validation lives in the sibling
   //! file named for the type; this file is the index.
   //!
   //! Responsibilities:
   //!
   //! - [`Directory`] — a workspace-relative directory path.
   //!
   //! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
   //! two of them to decide something belongs to the module for the phase that decides it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod directory;

   pub use directory::Directory;
   ```

2. Add to `crates/buildl-core/src/lib.rs`, below the existing `pub mod error;` block:

   ```rust
   pub mod types;

   pub use types::Directory;
   ```

3. Write the failing tests. Create `crates/buildl-core/src/types/directory.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 5.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Directory;

       #[test]
       fn accepts_the_documented_forms() {
           for raw in ["", "lib", "lib/text", "a-b_c.d"] {
               assert!(Directory::parse(raw).is_ok(), "{raw} should parse");
           }
       }

       #[test]
       fn rejects_the_documented_forms() {
           for raw in [
               "/lib",
               "lib/",
               "lib//text",
               ".",
               "..",
               "lib/../x",
               "lib:x",
               "a b",
           ] {
               assert!(Directory::parse(raw).is_err(), "{raw} should be rejected");
           }
           assert!(Directory::parse("a".repeat(1025)).is_err());
       }

       #[test]
       fn the_empty_path_is_the_workspace_root() {
           let root = Directory::parse("").unwrap();
           assert!(root.is_root());
           assert_eq!(root, Directory::root());
           assert_eq!(root.as_str(), "");
           assert!(!Directory::parse("lib").unwrap().is_root());
       }

       #[test]
       fn round_trips_through_json_as_a_string() {
           let dir = Directory::parse("lib/text").unwrap();
           let json = serde_json::to_string(&dir).unwrap();
           assert_eq!(json, r#""lib/text""#);
           assert_eq!(serde_json::from_str::<Directory>(&json).unwrap(), dir);
           assert!(serde_json::from_str::<Directory>(r#""/lib""#).is_err());
       }
   }
   ```

4. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Directory`
    --> crates/buildl-core/src/types/directory.rs:10:9
   ```

5. Replace the file's placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A workspace-relative directory path, validated at construction.
   //!
   //! Its own file because two types hold one: a label's directory half and a provenance record's
   //! declaring directory. Parsing it once here means neither re-checks it.
   //!
   //! Responsibilities: [`Directory`], its [`Directory::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: the filesystem. A `Directory` is a name, and nothing here asks whether
   //! the directory exists — that question belongs to a storage adapter.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};

   /// A workspace-relative directory path, such as `lib` or `lib/text`.
   ///
   /// The empty string is the workspace root. Valid paths are at most 1024 bytes, hold no leading
   /// or trailing `/`, and consist of `/`-separated non-empty segments of ASCII letters, digits,
   /// `.`, `-` and `_`, none of which is `.` or `..`. The character set deliberately excludes
   /// whitespace and `:`: the first would be a quoting hazard in the executor's argument
   /// expansion, and the second is a label's one unambiguous split point.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Directory(String);

   impl Directory {
       /// Longest accepted path, in bytes.
       const MAX_LEN: usize = 1024;

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
           let invalid = |reason: &'static str| Error::InvalidName {
               kind: NameKind::Directory,
               value: raw.clone(),
               reason,
           };

           if raw.len() > Self::MAX_LEN {
               return Err(invalid("must be at most 1024 bytes"));
           }
           if raw.is_empty() {
               return Ok(Self(raw));
           }
           if raw.starts_with('/') || raw.ends_with('/') {
               return Err(invalid("must not start or end with '/'"));
           }
           for segment in raw.split('/') {
               if segment.is_empty() {
                   return Err(invalid("must not contain an empty path segment"));
               }
               if segment == "." || segment == ".." {
                   return Err(invalid("must not contain a '.' or '..' segment"));
               }
               if !segment
                   .bytes()
                   .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
               {
                   return Err(invalid(
                       "segments may hold only ASCII letters, digits, '.', '-' and '_'",
                   ));
               }
           }
           Ok(Self(raw))
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

   impl fmt::Display for Directory {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for Directory {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for Directory {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Directory> for String {
       fn from(value: Directory) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for Directory {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

   `root()` and `is_root()` go beyond the spec's §3.1 property table, which names only `parse`,
   `as_str`, `Display`, `AsRef<str>` and `FromStr`. Both are needed: `Label::resolve`'s
   root-directory case and `Display`'s `//:name` rendering each depend on one. Recorded here as a
   widening of that table rather than a silent addition.

   The `From`/`TryFrom` pair is not decoration, and the reason is **validation on the way in**, not
   the wire format. A serde newtype struct is already transparent, so a bare derive would serialize
   as `"lib/text"` and even work as a map key — but it would also *deserialize* without running
   `parse`:

   ```
   plain derive, to_string       = "lib/text"
   plain derive, from_str "/lib" = Ok(Plain("/lib"))     ← an invalid Directory, built from JSON
   ```

   `try_from = "String"` routes deserialization through `Directory::parse`, which is what the
   `from_str::<Directory>(r#""/lib""#).is_err()` assertion in step 3 pins. Dropping the attribute
   because "the derive looks equivalent" silently reintroduces that hole — and the same applies to
   `TargetName` and `Digest`. `Label` needs the attribute for a second reason as well: it has two
   fields, so a bare derive emits an object and fails as a map key with `key must be a string`.

6. Run and confirm green:

   ```
   $ cargo test -p buildl-core
   running 6 tests
   ......
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

7. Run the gate, then commit `feat(buildl-core): validate a workspace-relative directory path`:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

---

## Task 2 — `TargetName`, the `:<name>` half

**Files:**
- Create `crates/buildl-core/src/types/target_name.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/target_name.rs` with the test
   module only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::TargetName;

       #[test]
       fn accepts_every_name_the_design_declares() {
           for raw in [
               "app", "main.o", "util.o", "app_test", "default", "text", "a-b",
           ] {
               assert!(TargetName::parse(raw).is_ok(), "{raw} should parse");
           }
       }

       #[test]
       fn rejects_the_documented_forms() {
           for raw in ["", "a/b", "a:b", ".", "..", " x"] {
               assert!(TargetName::parse(raw).is_err(), "{raw} should be rejected");
           }
           assert!(TargetName::parse("a".repeat(257)).is_err());
       }

       #[test]
       fn round_trips_through_json_as_a_string() {
           let name = TargetName::parse("main.o").unwrap();
           let json = serde_json::to_string(&name).unwrap();
           assert_eq!(json, r#""main.o""#);
           assert_eq!(serde_json::from_str::<TargetName>(&json).unwrap(), name);
           assert!(serde_json::from_str::<TargetName>(r#""a/b""#).is_err());
       }
   }
   ```

   Add `pub mod target_name;` and `pub use target_name::TargetName;` to
   `crates/buildl-core/src/types/mod.rs`, extend its `Responsibilities` list with a `[`TargetName`]`
   bullet, and replace `pub use types::Directory;` with `pub use types::{Directory, TargetName};` in `crates/buildl-core/src/lib.rs`.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::TargetName`
    --> crates/buildl-core/src/types/target_name.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! The name half of a label, validated at construction.
   //!
   //! Its own file because the two halves of a label have different grammars and are validated
   //! independently — resolving `:sibling` against a base directory checks this half alone.
   //!
   //! Responsibilities: [`TargetName`], its [`TargetName::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: uniqueness. A `TargetName` is well formed, not unclaimed; whether two
   //! declarations fight over one belongs to the phase that builds the graph.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};

   /// The name half of a label, such as `app` or `main.o`.
   ///
   /// Valid names are non-empty, at most 256 bytes, are neither `.` nor `..`, and hold only ASCII
   /// letters, digits, `.`, `-` and `_`. Excluding `/` and `:` is what makes a label's single `:`
   /// an unambiguous split point.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct TargetName(String);

   impl TargetName {
       /// Longest accepted name, in bytes.
       const MAX_LEN: usize = 256;

       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`,
       /// or holds a character outside ASCII alphanumerics, `.`, `-` and `_`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           let invalid = |reason: &'static str| Error::InvalidName {
               kind: NameKind::TargetName,
               value: raw.clone(),
               reason,
           };

           if raw.is_empty() {
               return Err(invalid("must not be empty"));
           }
           if raw.len() > Self::MAX_LEN {
               return Err(invalid("must be at most 256 bytes"));
           }
           if raw == "." || raw == ".." {
               return Err(invalid("must not be '.' or '..'"));
           }
           if !raw
               .bytes()
               .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
           {
               return Err(invalid(
                   "may hold only ASCII letters, digits, '.', '-' and '_'",
               ));
           }
           Ok(Self(raw))
       }

       /// The name as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for TargetName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for TargetName {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for TargetName {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<TargetName> for String {
       fn from(value: TargetName) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for TargetName {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   running 9 tests
   .........
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): validate the name half of a label`.

---

## Task 3 — `Label`, absolute by construction

**Files:**
- Create `crates/buildl-core/src/types/label.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/label.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Label;

       #[test]
       fn parses_and_round_trips_the_absolute_form() {
           for raw in ["//lib:text", "//lib/text:core", "//:top"] {
               let label = Label::parse(raw).unwrap();
               assert_eq!(label.to_string(), raw);
               assert_eq!(Label::parse(&label.to_string()).unwrap(), label);
           }
       }

       #[test]
       fn exposes_both_halves() {
           let label = Label::parse("//lib/text:core").unwrap();
           assert_eq!(label.directory().as_str(), "lib/text");
           assert_eq!(label.name().as_str(), "core");
           assert!(Label::parse("//:top").unwrap().directory().is_root());
       }

       #[test]
       fn rejects_every_form_that_is_not_one_absolute_label() {
           for raw in [
               "//lib", "//lib:", "//:", "//a:b:c", "lib:text", ":sibling", "main.o",
           ] {
               assert!(Label::parse(raw).is_err(), "{raw} should be rejected");
           }
       }

       #[test]
       fn orders_by_directory_then_name() {
           let mut labels = ["//lib:z", "//:a", "//lib:a", "//app:m"]
               .map(|raw| Label::parse(raw).unwrap())
               .to_vec();
           labels.sort();
           let rendered: Vec<String> = labels.iter().map(ToString::to_string).collect();
           assert_eq!(rendered, ["//:a", "//app:m", "//lib:a", "//lib:z"]);
       }

       #[test]
       fn round_trips_through_json_as_a_string() {
           let label = Label::parse("//lib:text").unwrap();
           let json = serde_json::to_string(&label).unwrap();
           assert_eq!(json, r#""//lib:text""#);
           assert_eq!(serde_json::from_str::<Label>(&json).unwrap(), label);
           assert!(serde_json::from_str::<Label>(r#""//lib""#).is_err());
       }
   }
   ```

   The ordering test is what pins the field order: `Ord` derives over the fields as declared, and
   `architecture.md:72`'s `by_label: BTreeMap~Label_NodeId~` is a deterministic iteration source
   only if that order is directory-then-name. A test rather than a comment, so reordering the
   struct fails rather than silently changing graph output.

   Add `pub mod label;` and `pub use label::Label;` to `crates/buildl-core/src/types/mod.rs`, extend
   its `Responsibilities` list, and widen the re-export in `crates/buildl-core/src/lib.rs` to
   `pub use types::{Directory, Label, TargetName};`.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Label`
    --> crates/buildl-core/src/types/label.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A whole, absolute target name, validated at construction.
   //!
   //! Its own file because it is the type the rest of the pipeline names a target by, and because
   //! it owns the grammar that joins the two halves. There is deliberately no representation of an
   //! unresolved reference: a `Label` is absolute, so no later phase asks whether the one it holds
   //! still needs resolving.
   //!
   //! Responsibilities: [`Label`], the absolute [`Label::parse`] form, and its rendering.
   //!
   //! Non-responsibilities: existence. A `Label` names a target; whether one was declared under
   //! that name belongs to the phase that builds the graph.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::{Directory, TargetName};

   /// A target's absolute name, written `//<directory>:<name>`.
   ///
   /// Rendering is exact and round-trips: `Label::parse(label.to_string())` returns an equal value
   /// for every `Label`. A directory-only reference such as `//lib` is rejected rather than read as
   /// `//lib:lib`, so one target never has two spellings.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Label {
       directory: Directory,
       name: TargetName,
   }

   impl Label {
       /// Joins an already-validated directory and name.
       #[must_use]
       pub const fn new(directory: Directory, name: TargetName) -> Self {
           Self { directory, name }
       }

       /// Parses the absolute form `//<directory>:<name>`.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` lacks the `//` prefix or a `:`, and propagates
       /// the failure of either half's own grammar.
       pub fn parse(raw: &str) -> Result<Self> {
           let invalid = |reason: &'static str| Error::InvalidName {
               kind: NameKind::Label,
               value: raw.to_owned(),
               reason,
           };

           let body = raw
               .strip_prefix("//")
               .ok_or_else(|| invalid("must start with '//'"))?;
           let (directory, name) = body
               .split_once(':')
               .ok_or_else(|| invalid("must hold ':' and a target name"))?;

           Ok(Self {
               directory: Directory::parse(directory)?,
               name: TargetName::parse(name)?,
           })
       }

       /// The directory half.
       #[must_use]
       pub const fn directory(&self) -> &Directory {
           &self.directory
       }

       /// The name half.
       #[must_use]
       pub const fn name(&self) -> &TargetName {
           &self.name
       }
   }

   impl fmt::Display for Label {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           write!(f, "//{}:{}", self.directory, self.name)
       }
   }

   impl FromStr for Label {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Label> for String {
       fn from(value: Label) -> Self {
           value.to_string()
       }
   }

   impl TryFrom<String> for Label {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(&raw)
       }
   }
   ```

   `//a:b:c` needs no explicit check: `split_once` takes the first `:`, leaving `b:c`, which
   `TargetName::parse` rejects because `:` is outside its character set. The test in step 1 proves
   it rather than assuming it.

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   running 14 tests
   ..............
   test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): parse a target's absolute label`.

---

## Task 4 — `Label::resolve`, the three reference forms

This closes the relative-label half of `design.md` §14's open question. It is the only place a
reference as a build file writes it becomes a `Label`.

**Files:**
- Modify `crates/buildl-core/src/types/label.rs`

**Steps:**

1. Add the failing tests to the existing test module in `crates/buildl-core/src/types/label.rs`,
   and extend its `use super::Label;` to `use super::Label; use crate::types::Directory;`:

   ```rust
   #[test]
   fn resolves_every_reference_form_a_build_file_may_write() {
       let base = Directory::parse("app").unwrap();
       assert_eq!(
           Label::resolve("//lib:text", &base).unwrap().to_string(),
           "//lib:text"
       );
       assert_eq!(
           Label::resolve(":sibling", &base).unwrap().to_string(),
           "//app:sibling"
       );
       assert_eq!(
           Label::resolve("main.o", &base).unwrap().to_string(),
           "//app:main.o"
       );
       assert_eq!(
           Label::resolve("main.o", &Directory::root())
               .unwrap()
               .to_string(),
           "//:main.o"
       );
   }

   #[test]
   fn resolve_rejects_a_malformed_reference_in_either_form() {
       let base = Directory::parse("app").unwrap();
       assert!(Label::resolve("//lib", &base).is_err());
       assert!(Label::resolve(":", &base).is_err());
       assert!(Label::resolve("a/b", &base).is_err());
       assert!(Label::resolve("", &base).is_err());
   }

   #[test]
   fn resolve_is_usable_as_a_json_map_key() {
       let base = Directory::root();
       let mut map = std::collections::BTreeMap::new();
       map.insert(Label::resolve("main.o", &base).unwrap(), 7_u32);
       map.insert(Label::resolve("//lib:text", &base).unwrap(), 9_u32);
       assert_eq!(
           serde_json::to_string(&map).unwrap(),
           r#"{"//:main.o":7,"//lib:text":9}"#
       );
   }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0599]: no function or associated item named `resolve` found for struct `Label`
   ```

3. Add the method to the `impl Label` block, after `parse`:

   ```rust
   /// Resolves a reference as a build file may write it, against the declaring directory.
   ///
   /// Three forms are accepted: the absolute `//<directory>:<name>`, which ignores `base`;
   /// `:<name>`, a sibling in `base`; and a bare `<name>`, also in `base`. A single dependency
   /// list may mix them, which is why all three resolve here rather than at the call site.
   ///
   /// # Errors
   ///
   /// Returns [`Error::InvalidName`] when the reference fails its grammar — for the absolute form,
   /// whatever [`Label::parse`] rejects; otherwise whatever [`TargetName::parse`] rejects.
   pub fn resolve(raw: &str, base: &Directory) -> Result<Self> {
       if raw.starts_with("//") {
           return Self::parse(raw);
       }
       let name = raw.strip_prefix(':').unwrap_or(raw);
       Ok(Self {
           directory: base.clone(),
           name: TargetName::parse(name)?,
       })
   }
   ```

   `strip_prefix(':').unwrap_or(raw)` is `Option::unwrap_or`, which `clippy::unwrap_used` does not
   target — that lint fires on `.unwrap()`, not on `.unwrap_or()`. This exact line was compiled
   under the workspace's lint set with `-D warnings` and is clean.

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   running 17 tests
   .................
   test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): resolve a build file's label references against its directory`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
```

All three exit 0. `cargo make deny` should be unchanged from plan `01` — this plan adds no
dependency, and `crates/expected-edges.txt` is untouched, which is the property that let it run
beside plans `03` and `04`.

At the end of this plan `buildl-core` exports `Directory`, `TargetName` and `Label`; every form in
`design.md` §5 parses; and `//lib` is rejected with a recorded reason.

---

## Review findings

- unit-test-mandate (risk) — `FromStr` was an unguarded invariant on all three types: no test exercised it, because the round-trip tests go through `serde_json::from_str` and the serde `try_from` attribute, a different code path. Confirmed independently with `grep -rn 'parse::<' crates/ --include='*.rs'`, which returned nothing — `crates/buildl-core/src/types/directory.rs:107`, `target_name.rs:84`, `label.rs:106`. Fixed: one accept and one reject assertion per type in the turbofish `parse::<T>()` form, added to each file's existing test module. Verified by mutation, not merely by a green run — see Probe results.
- doc-comment-discipline (nit) — `label.rs`'s type doc claimed round-tripping with the snippet `Label::parse(label.to_string())`, which does not typecheck: `parse` takes `&str` and `to_string()` yields `String`, with no coercion at an argument position. The claim was true, only the snippet wrong — `crates/buildl-core/src/types/label.rs:23`. Fixed to `Label::parse(&label.to_string())`.
- doc-comment-discipline (nit) — `directory.rs`'s module doc justified the file by "two types hold one: a label's directory half and a provenance record's declaring directory", but `grep -rn 'Provenance' crates/ --include='*.rs'` returned nothing: forward-work narration about a type not in the tree — `crates/buildl-core/src/types/directory.rs:3`. Fixed to justify the file by the holder that exists today.
- unit-test-mandate (nit) — `Label::new` had no test and no call site, so mis-assigning its two fields would have been invisible to the gate — `crates/buildl-core/src/types/label.rs:36`. Fixed: `new_joins_the_directory_and_name_in_order` pins it the way `orders_by_directory_then_name` pins field order.
- unit-test-mandate (nit) — `AsRef<str>` has no test and no call site on `Directory` (`:101`) or `TargetName` (`:78`). Accepted as-is, not fixed: it is a one-line delegation to `as_str`, which is itself pinned, and the trait has no in-tree consumer yet to pin it against.
- strong-types (nit) — when one half fails, the error carries that half's `NameKind` and value rather than the whole label: `Label::parse("//lib:")` renders `invalid target name: "" — must not be empty`, losing the spelling the user typed — `crates/buildl-core/src/types/label.rs:61`. Accepted as-is: the rustdoc documents the behaviour deliberately, and this is diagnostic quality rather than a defect. Worth revisiting when the reporting adapter exists and can add the outer context.
- spec §3.1 (amendment) — `Directory::root` and `is_root` widen the property table, which still names only `parse`, `as_str`, `Display`, `AsRef<str>` and `FromStr`. Recorded in this plan's task 1 with rationale rather than as a spec edit, so §3.1 is not later read as the full surface.
- spec §3.3 (amendment) — the spec's signature block writes `pub fn new`; the implementation is `pub const fn new`, and `directory()` / `name()` are `const` too. A strict widening, forced by `clippy::missing_const_for_fn` and prescribed by this plan's task-3 code block.

## Probe results

- **Claim: `clippy::unwrap_used` does not fire on `Option::unwrap_or`, so `resolve`'s `raw.strip_prefix(':').unwrap_or(raw)` is safe to write as given.** The plan asserted this citing only itself ("this exact line was compiled ... and is clean"), so it needed independent proof. Throwaway crate with two functions, one using `.unwrap_or(raw)` and one using `.unwrap()`, compiled under `cargo clippy -- -D clippy::unwrap_used`. Real output:
  ```
  error: used `unwrap()` on an `Option` value
   --> src/lib.rs:6:5
  6 |     raw.strip_prefix(':').unwrap()
    = note: requested on the command line with `-D clippy::unwrap-used`
  error: could not compile `unwrapprobe` (lib) due to 1 previous error
  ```
  One error, at the `.unwrap()` line only; the `.unwrap_or(raw)` line passed. The control is built in — the same run proves the lint was active. **Came out for the plan.** Probe deleted.
- **Claim: the new `FromStr` guards would actually catch the regression they were written for.** A passing test is not evidence of that, so the guard was mutation-tested: `Directory`'s `FromStr` body was replaced with `Ok(Self(raw.to_owned()))` — the exact break the reviewer described — and the suite re-run. Real output:
  ```
  test types::directory::tests::from_str_routes_through_parse ... FAILED
  thread 'types::directory::tests::from_str_routes_through_parse' panicked at crates/buildl-core/src/types/directory.rs:184:9:
  assertion failed: "/lib".parse::<Directory>().is_err()
  test result: FAILED. 20 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
  ```
  The file was then restored from a backup copy and the suite returned to `21 passed; 0 failed`. The guard is real, not decorative.
- **Claim: `docs/architecture.md:72` is `by_label: BTreeMap~Label_NodeId~`, which is why `Label`'s field order is load-bearing.** Read directly. Line 72 is exactly `        by_label: BTreeMap~Label_NodeId~` inside the `class TargetGraph` block. Confirmed, and `Label`'s fields are declared `directory` then `name` as required.
- **Claim: the label corpus comes from `docs/design.md:135-136`, `:140`, `:141`.** Read directly, all three exact: `:135-136` is `b.target("app", {` / `deps = { "main.o", "util.o", "//lib:text" },`; `:140` is `b.alias("default", "app")`; `:141` is `b.test("app_test", { deps = { "app" }, ... })`. The names that must parse are therefore `main.o`, `util.o`, `app`, `default`, `app_test`, `text`, and directory `lib`. Confirmed.
- **Claim: `FromStr` was untested before the fix round.** `grep -rn 'parse::<' crates/ --include='*.rs'` → no output. Control: the same grep for `from_str` found the serde round-trip tests, so the method was sound. **Came out against the plan**, which had treated `FromStr` as covered by the round-trip assertions.
- **Claim: `Provenance` is not yet in the tree.** `grep -rn 'Provenance' crates/ --include='*.rs'` → no output. Confirmed; it arrives in plan `03`.
- **Each task's red step and test total.** Every one matched the plan's prediction exactly: task 1 `error[E0432] ... super::Directory` at `directory.rs:10:9` then `running 6 tests`; task 2 the same error for `super::TargetName` at `target_name.rs:10:9` then `running 9 tests`; task 3 for `super::Label` at `label.rs:10:9` then `running 14 tests`; task 4 `error[E0599] ... no associated function ... named `resolve`` then `running 17 tests`.

## Deviations

- **2026-10-04 — no commits.** Each task's final step names a commit; none was run. The commit gate belongs to the author, and no agent in this flow runs a commit. Messages held for the author: `feat(buildl-core): validate a workspace-relative directory path` (task 1), `feat(buildl-core): validate the name half of a label` (task 2), `feat(buildl-core): parse a target's absolute label` (task 3), and `feat(buildl-core): resolve a build file's label references against its directory` (task 4).
- **2026-10-04 — four tests beyond the plan's count, from the review fix round.** The plan ends at 17 tests; the tree holds 21. The four added are `from_str_routes_through_parse` on each of the three types, plus `new_joins_the_directory_and_name_in_order` on `Label`. They add no behaviour — they pin invariants the plan left unguarded.
- **2026-10-04 — task 4's red step produced ten compile errors, not the single `error[E0599]` line the plan quotes.** Same root cause and same error code; the three new tests simply call `resolve` from ten sites. Matches in kind, not in count.
- **2026-10-04 — the orchestrator's brief for task 1 misstated the expected test total** as "6 new on top of the 2 that already exist" where the plan means 6 in total. The coder read the plan correctly, added 4, and reported the number explicitly as instructed, so the error never reached the code. Recorded because the brief, not the plan, was the wrong half.
- **2026-10-04 — `crates/buildl-core/src/types/` was registered intent-to-add** so the reviewer's diff against HEAD would include the four new files. Nothing was committed.
