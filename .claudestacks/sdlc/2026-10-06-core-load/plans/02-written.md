---
status: approved
created: 2026-10-06
depends-on: [01]
---

# Written Implementation Plan

**Goal:** A build file's as-written text carries the type it is meant to become, with exactly one conversion into that type.

**Architecture:** One generic newtype, `Written<T>`, in `crates/buildl-core/src/types/written.rs`.
Its marker is `PhantomData<fn() -> T>`, so the type parameter costs nothing at runtime, keeps the
value `Send + Sync` and covariant whatever `T` is, and makes the destination part of the type: a
`Written<Label>` has `resolve` and nothing else, so it can never be turned into a `Directory`.
Construction is infallible; each destination type gets its own inherent `impl Written<X>` block with
one method. Nine of the eleven are plain delegations to `X::parse`; two combine the text with a
`Directory` of the same family — `Written<Label>::resolve` delegates to `Label::resolve`, and
`Written<Directory>::under` joins two directories and re-parses the result, which is how an escaping
`subdir("../x")` fails without an extra check. No serde and no `Display`: a `Written<T>` never leaves
the port boundary.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `cargo-make`. No new dependency.

**Content authority:** spec §3.1 (`Written<T>` and its conversion table), §1.1 (`Directory` has no
join; `Directory::parse` refuses every escape form), §8 (accept and reject sides of each grammar).

---

## Context an implementer needs

Plan `01` is `done` when this plan starts. It leaves:

| Where | What plan 01 left |
|---|---|
| `crates/buildl-core/src/types/` | `source_path.rs`, `output_name.rs`, `env_name.rs`, `argument.rs` (`Argument`, `Command`), `description.rs`, `setting.rs` (`SettingName`, `SettingValue`), `entry_name.rs`, `field_name.rs`, `diagnostic.rs`, and the private `grammar.rs`. Every string newtype has `parse(raw: impl Into<String>) -> Result<Self>` and `as_str`. |
| `crates/buildl-core/src/error.rs` | `NameKind` with the ten new variants. This plan uses only the existing `NameKind::Directory`. |
| `crates/buildl-core/src/types/mod.rs` | `mod grammar;`, a `pub mod` and `pub use` per type, alphabetical; the `Responsibilities` list ends with the `[`Diagnostic`]` bullet. |
| `crates/buildl-core/src/lib.rs` | `pub use types::{Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Label, NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName, Timestamp};` in rustfmt form. |

The existing types this plan calls, unchanged: `Directory::parse`, `Directory::root` (const),
`Directory::is_root` (const), `Directory`'s `Display` (the raw string, `""` for the root),
`TargetName::parse`, `Label::resolve(&str, &Directory)` (bare, `:name` and `//dir:name` forms; `//lib`
is rejected), `Error::InvalidName { kind, value, reason }` rendering `invalid {kind}: {value:?} — {reason}`.

Gate facts that shape this plan's code. Each turns `cargo make dod` red under `-D warnings`:

| Fact | Consequence here |
|---|---|
| `missing_docs` | every `pub` item, including each `impl Written<X>` method, carries a doc comment |
| `missing_errors_doc` | every `Result`-returning method has a `# Errors` section |
| `must_use_candidate` | `new` and `as_written` carry `#[must_use]`; the `Result`-returning conversions need none (`Result` is already `must_use`) |
| `missing_const_for_fn` | does not fire: `new` calls `Into::into` and `as_written` dereferences a `String`, the same precedent as `TargetName::as_str` |
| unused imports fail clippy | the `use crate::types::{…}` lists, in the module and in its tests, grow task by task; each task gives the exact list |
| `RUSTDOCFLAGS=-D warnings` fails a broken intra-doc link | the module doc links `[`Directory`]`, `[`Label`]` and `[`Written::under`]`, which only resolve once task 3 lands; tasks 1–2 use a shorter module doc, task 3 writes the final one |
| an unfulfilled `#![expect]` warns | the test module's `#![expect(clippy::unwrap_used, …)]` arrives in task 2, the first task whose tests unwrap; task 1's single test does not |
| no `#[allow]`, no `clippy::(disallowed_\|all\|style)` strings | none are written |

Run `cargo fmt --all` before every `cargo make dod`. Every Rust block below is rustfmt-canonical,
but retyping can drift, and `fmt-check` is the gate's first step.

All work happens in the worktree, never on `main`. Commits follow Conventional Commits with scope
`buildl-core`, one per task.

### File map

```
crates/buildl-core/src/types/written.rs — [create] Written<T>, new/as_written (task 1)
                                          [modify] Written<TargetName>::parse, Written<Label>::resolve (task 2)
                                          [modify] Written<Directory>::under, final module doc (task 3)
                                          [modify] SourcePath, OutputName conversions (task 4)
                                          [modify] Argument, SettingValue conversions (task 5)
                                          [modify] SettingName, FieldName conversions (task 6)
                                          [modify] EnvName, Description conversions (task 7)
crates/buildl-core/src/types/mod.rs     — [modify] pub mod written; pub use written::Written; bullet (task 1)
crates/buildl-core/src/lib.rs           — [modify] add Written to the types re-export (task 1)
```

The eight plain conversions are grouped by the grammar their destination uses (plan 01's
`grammar::path`, `grammar::text`, `grammar::key`, then the two types with their own grammar), and
each task inserts its blocks at the position the final file holds them in. After task 7 the file is
in its final order: `TargetName`, `Label`, `Directory`, `SourcePath`, `OutputName`, `EnvName`,
`Argument`, `Description`, `SettingName`, `SettingValue`, `FieldName`.

---

## Task 1 — Record text exactly as written

**Files:**
- Create `crates/buildl-core/src/types/written.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Wire the module in. In `crates/buildl-core/src/types/mod.rs`:
   - add the bullet `//! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.`
     directly after the `[`Diagnostic`]` bullet;
   - add `pub mod written;` directly after `pub mod timestamp;`;
   - add `pub use written::Written;` directly after `pub use timestamp::Timestamp;`.

   The file then reads:

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
   //! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
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
   pub mod written;

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
   pub use written::Written;
   ```

   In `crates/buildl-core/src/lib.rs`, append `Written` to the `types` re-export. Its doc text is
   unchanged; the re-export becomes:

   ```rust
   pub use types::{
       Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName,
       Label, NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName,
       Timestamp, Written,
   };
   ```

2. Write the failing test. Create `crates/buildl-core/src/types/written.rs` with the test module
   only. It does not unwrap, so it carries no `#![expect]`:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       use super::Written;
       use crate::types::Label;

       #[test]
       fn keeps_the_text_exactly_as_written() {
           let written = Written::<Label>::new("../not a label");
           assert_eq!(written.as_written(), "../not a label");
       }
   }
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `written::Written`
     --> crates/buildl-core/src/types/mod.rs
   error[E0432]: unresolved import `super::Written`
    --> crates/buildl-core/src/types/written.rs:5:9
   ```

4. Replace the whole file with the type above its test module. The module doc is the short form;
   task 3 replaces its last two paragraphs once the items they link to exist:

   ```rust
   //! Text exactly as a build file wrote it, typed by what it is meant to become.
   //!
   //! Its own file because an as-written value is its own concept: it has a destination type but has
   //! not been validated against it yet. Keeping the destination in the type means a value written
   //! as a label can only ever be resolved as a label.
   //!
   //! Responsibilities: [`Written`].
   //!
   //! Non-responsibilities: deciding anything between two different concepts.

   use core::marker::PhantomData;

   /// Text as a build file wrote it, not yet validated as the `T` it is meant to become.
   ///
   /// Construction never fails; validation happens in the one conversion each `T` has. The marker is
   /// `PhantomData<fn() -> T>`, so a `Written<T>` is `Send`, `Sync` and covariant whatever `T` is.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
   pub struct Written<T> {
       text: String,
       target: PhantomData<fn() -> T>,
   }

   impl<T> Written<T> {
       /// Records `text` exactly as written.
       #[must_use]
       pub fn new(text: impl Into<String>) -> Self {
           Self {
               text: text.into(),
               target: PhantomData,
           }
       }

       /// The text exactly as written.
       #[must_use]
       pub fn as_written(&self) -> &str {
           &self.text
       }
   }

   #[cfg(test)]
   mod tests {
       use super::Written;
       use crate::types::Label;

       #[test]
       fn keeps_the_text_exactly_as_written() {
           let written = Written::<Label>::new("../not a label");
           assert_eq!(written.as_written(), "../not a label");
       }
   }
   ```

5. Run and confirm green. This task adds `keeps_the_text_exactly_as_written`:

   ```
   $ cargo test -p buildl-core --lib written
   running 1 test
   test types::written::tests::keeps_the_text_exactly_as_written ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

   The filtered count is plan 01's unit-test total (98 when plan 01 lands as specified).

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): record a build file's text typed by its destination`.

---

## Task 2 — Convert written target names and label references

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing tests. Replace the whole test module with this one. These tests unwrap, so the
   module now opens with the `#![expect]`:

   ```rust
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Written;
       use crate::types::{Directory, Label, TargetName};

       #[test]
       fn keeps_the_text_exactly_as_written() {
           let written = Written::<Label>::new("../not a label");
           assert_eq!(written.as_written(), "../not a label");
       }

       #[test]
       fn a_target_name_parses_or_refuses() {
           assert_eq!(
               Written::<TargetName>::new("app").parse().unwrap().as_str(),
               "app"
           );
           assert!(Written::<TargetName>::new("//other:x").parse().is_err());
       }

       #[test]
       fn a_label_resolves_every_reference_form_against_its_base() {
           let base = Directory::parse("app").unwrap();
           for (raw, resolved) in [
               ("main.o", "//app:main.o"),
               (":util.o", "//app:util.o"),
               ("//lib:text", "//lib:text"),
           ] {
               assert_eq!(
                   Written::<Label>::new(raw)
                       .resolve(&base)
                       .unwrap()
                       .to_string(),
                   resolved
               );
           }
           assert!(Written::<Label>::new("//lib").resolve(&base).is_err());
       }
   }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `parse` found for struct `Written<T>` in the current scope
     --> crates/buildl-core/src/types/written.rs:59:47
   error[E0599]: no method named `resolve` found for struct `Written<T>` in the current scope
     --> crates/buildl-core/src/types/written.rs:75:22
   error: could not compile `buildl-core` (lib test) due to 4 previous errors
   ```

3. Replace the line `use core::marker::PhantomData;` with:

   ```rust
   use core::marker::PhantomData;

   use crate::error::Result;
   use crate::types::{Directory, Label, TargetName};
   ```

   Then insert, directly after the `impl<T> Written<T>` block:

   ```rust
   impl Written<TargetName> {
       /// Validates the text as a target name.
       ///
       /// # Errors
       ///
       /// Returns whatever [`TargetName::parse`] rejects.
       pub fn parse(&self) -> Result<TargetName> {
           TargetName::parse(self.text.clone())
       }
   }

   impl Written<Label> {
       /// Resolves the text as a reference written in the build file of `base`.
       ///
       /// # Errors
       ///
       /// Returns whatever [`Label::resolve`] rejects.
       pub fn resolve(&self, base: &Directory) -> Result<Label> {
           Label::resolve(&self.text, base)
       }
   }
   ```

4. Run and confirm green. This task adds `a_target_name_parses_or_refuses` and
   `a_label_resolves_every_reference_form_against_its_base`:

   ```
   $ cargo test -p buildl-core --lib written
   running 3 tests
   test types::written::tests::keeps_the_text_exactly_as_written ... ok
   test types::written::tests::a_target_name_parses_or_refuses ... ok
   test types::written::tests::a_label_resolves_every_reference_form_against_its_base ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert written target names and label references`.

---

## Task 3 — Join a written directory under its base

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing tests. Insert these three tests at the end of the test module, directly after
   `a_label_resolves_every_reference_form_against_its_base`:

   ```rust
       #[test]
       fn a_directory_joins_under_its_base() {
           let root = Directory::root();
           let lib = Directory::parse("lib").unwrap();
           assert_eq!(Written::<Directory>::new("lib").under(&root).unwrap(), lib);
           assert_eq!(
               Written::<Directory>::new("text")
                   .under(&lib)
                   .unwrap()
                   .as_str(),
               "lib/text"
           );
       }

       #[test]
       fn a_directory_never_escapes_or_stays_empty() {
           let lib = Directory::parse("lib").unwrap();
           for raw in ["", "..", "../x", "/x", "a//b", "a b"] {
               assert!(
                   Written::<Directory>::new(raw).under(&lib).is_err(),
                   "{raw:?} should be rejected under lib"
               );
               assert!(
                   Written::<Directory>::new(raw)
                       .under(&Directory::root())
                       .is_err(),
                   "{raw:?} should be rejected under the root"
               );
           }
       }

       #[test]
       fn an_empty_directory_reports_the_directory_grammar() {
           let err = Written::<Directory>::new("")
               .under(&Directory::root())
               .unwrap_err();
           assert_eq!(
               err.to_string(),
               r#"invalid directory: "" — must not be empty"#
           );
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `under` found for struct `Written<T>` in the current scope
      --> crates/buildl-core/src/types/written.rs:113:53
   error: could not compile `buildl-core` (lib test) due to 5 previous errors
   ```

3. Write the final module doc. Replace its last two paragraphs:

   ```rust
   //! Responsibilities: [`Written`].
   //!
   //! Non-responsibilities: deciding anything between two different concepts.
   ```

   with:

   ```rust
   //! Responsibilities: [`Written`], and one conversion per destination type.
   //!
   //! Non-responsibilities: deciding anything between two different concepts. Every conversion here
   //! either parses its own destination type or combines it with a [`Directory`] of the same family —
   //! a [`Label`] contains a directory, and [`Written::under`] joins two directories.
   ```

4. Replace `use crate::error::Result;` with:

   ```rust
   use crate::error::{Error, NameKind, Result};
   ```

   Then insert, directly after the `impl Written<Label>` block:

   ```rust
   impl Written<Directory> {
       /// Joins the text under `base` and validates the result as a directory.
       ///
       /// The text names a directory relative to `base`, so a `..` or a leading `/` produces a path
       /// the directory grammar refuses: a written directory can never leave the workspace.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when the text is empty, and whatever [`Directory::parse`]
       /// rejects about the joined path.
       pub fn under(&self, base: &Directory) -> Result<Directory> {
           if self.text.is_empty() {
               return Err(Error::InvalidName {
                   kind: NameKind::Directory,
                   value: String::new(),
                   reason: "must not be empty",
               });
           }
           if base.is_root() {
               Directory::parse(self.text.clone())
           } else {
               Directory::parse(format!("{base}/{}", self.text))
           }
       }
   }
   ```

   The empty-text check is required, not defensive: under the root, `Directory::parse("")` is the
   root itself, so without the check an empty `subdir` would resolve to the root instead of
   failing; under any other base the join `lib/` would fail with the trailing-slash reason rather
   than "must not be empty".

5. Run and confirm green. This task adds `a_directory_joins_under_its_base`,
   `a_directory_never_escapes_or_stays_empty` and `an_empty_directory_reports_the_directory_grammar`:

   ```
   $ cargo test -p buildl-core --lib written
   running 6 tests
   test types::written::tests::keeps_the_text_exactly_as_written ... ok
   test types::written::tests::a_target_name_parses_or_refuses ... ok
   test types::written::tests::an_empty_directory_reports_the_directory_grammar ... ok
   test types::written::tests::a_directory_never_escapes_or_stays_empty ... ok
   test types::written::tests::a_directory_joins_under_its_base ... ok
   test types::written::tests::a_label_resolves_every_reference_form_against_its_base ... ok
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): join a written directory under its base`.

---

## Task 4 — Convert written source paths and output names

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing test. In the test module, replace `use crate::types::{Directory, Label, TargetName};` with:

   ```rust
       use crate::types::{Directory, Label, OutputName, SourcePath, TargetName};
   ```

   and append this test after `an_empty_directory_reports_the_directory_grammar`:

   ```rust
       #[test]
       fn every_plain_conversion_parses_its_own_type() {
           assert!(Written::<SourcePath>::new("src/a.c").parse().is_ok());
           assert!(Written::<SourcePath>::new("../a.c").parse().is_err());
           assert!(Written::<OutputName>::new("bin/app").parse().is_ok());
           assert!(Written::<OutputName>::new("/app").parse().is_err());
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `parse` found for struct `Written<SourcePath>` in the current scope
      --> crates/buildl-core/src/types/written.rs:181:55
   error[E0599]: no method named `parse` found for struct `Written<OutputName>` in the current scope
      --> crates/buildl-core/src/types/written.rs:183:55
   error: could not compile `buildl-core` (lib test) due to 4 previous errors
   ```

3. Replace the module's `use crate::types::{Directory, Label, TargetName};` with:

   ```rust
   use crate::types::{Directory, Label, OutputName, SourcePath, TargetName};
   ```

   Then insert, directly after the `impl Written<Directory>` block:

   ```rust
   impl Written<SourcePath> {
       /// Validates the text as a source path.
       ///
       /// # Errors
       ///
       /// Returns whatever [`SourcePath::parse`] rejects.
       pub fn parse(&self) -> Result<SourcePath> {
           SourcePath::parse(self.text.clone())
       }
   }

   impl Written<OutputName> {
       /// Validates the text as an output name.
       ///
       /// # Errors
       ///
       /// Returns whatever [`OutputName::parse`] rejects.
       pub fn parse(&self) -> Result<OutputName> {
           OutputName::parse(self.text.clone())
       }
   }
   ```

   `Written<SourcePath>::parse` takes no base. Joining a declaring directory and a source path
   combines two different concepts, so that join is Load's (plan 06), which builds the joined text
   from `as_written()` and passes it to `SourcePath::parse`.

4. Run and confirm green. This task adds `every_plain_conversion_parses_its_own_type`:

   ```
   $ cargo test -p buildl-core --lib written
   running 7 tests
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert written source paths and output names`.

---

## Task 5 — Convert written arguments and setting values

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing test. In the test module, replace
   `use crate::types::{Directory, Label, OutputName, SourcePath, TargetName};` with:

   ```rust
       use crate::types::{
           Argument, Directory, Label, OutputName, SettingValue, SourcePath, TargetName,
       };
   ```

   and replace `every_plain_conversion_parses_its_own_type` with:

   ```rust
       #[test]
       fn every_plain_conversion_parses_its_own_type() {
           assert!(Written::<SourcePath>::new("src/a.c").parse().is_ok());
           assert!(Written::<SourcePath>::new("../a.c").parse().is_err());
           assert!(Written::<OutputName>::new("bin/app").parse().is_ok());
           assert!(Written::<OutputName>::new("/app").parse().is_err());
           assert!(Written::<Argument>::new("$in").parse().is_ok());
           assert!(Written::<Argument>::new("a\0b").parse().is_err());
           assert!(Written::<SettingValue>::new("").parse().is_ok());
           assert!(Written::<SettingValue>::new("a\0b").parse().is_err());
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `parse` found for struct `Written<Argument>` in the current scope
      --> crates/buildl-core/src/types/written.rs:209:49
   error[E0599]: no method named `parse` found for struct `Written<SettingValue>` in the current scope
      --> crates/buildl-core/src/types/written.rs:211:50
   error: could not compile `buildl-core` (lib test) due to 4 previous errors
   ```

3. Replace the module's `use crate::types::{Directory, Label, OutputName, SourcePath, TargetName};` with:

   ```rust
   use crate::types::{Argument, Directory, Label, OutputName, SettingValue, SourcePath, TargetName};
   ```

   Then insert, directly after the `impl Written<OutputName>` block:

   ```rust
   impl Written<Argument> {
       /// Validates the text as a command argument.
       ///
       /// # Errors
       ///
       /// Returns whatever [`Argument::parse`] rejects.
       pub fn parse(&self) -> Result<Argument> {
           Argument::parse(self.text.clone())
       }
   }

   impl Written<SettingValue> {
       /// Validates the text as a setting value.
       ///
       /// # Errors
       ///
       /// Returns whatever [`SettingValue::parse`] rejects.
       pub fn parse(&self) -> Result<SettingValue> {
           SettingValue::parse(self.text.clone())
       }
   }
   ```

4. Run and confirm green. This task adds no test function; it extends
   `every_plain_conversion_parses_its_own_type`:

   ```
   $ cargo test -p buildl-core --lib written
   running 7 tests
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert written arguments and setting values`.

---

## Task 6 — Convert written setting names and field names

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing test. In the test module, replace the `use crate::types::{…};` block with:

   ```rust
       use crate::types::{
           Argument, Directory, FieldName, Label, OutputName, SettingName, SettingValue, SourcePath,
           TargetName,
       };
   ```

   and replace `every_plain_conversion_parses_its_own_type` with:

   ```rust
       #[test]
       fn every_plain_conversion_parses_its_own_type() {
           assert!(Written::<SourcePath>::new("src/a.c").parse().is_ok());
           assert!(Written::<SourcePath>::new("../a.c").parse().is_err());
           assert!(Written::<OutputName>::new("bin/app").parse().is_ok());
           assert!(Written::<OutputName>::new("/app").parse().is_err());
           assert!(Written::<Argument>::new("$in").parse().is_ok());
           assert!(Written::<Argument>::new("a\0b").parse().is_err());
           assert!(Written::<SettingName>::new("test_filter").parse().is_ok());
           assert!(Written::<SettingName>::new("a.b").parse().is_err());
           assert!(Written::<SettingValue>::new("").parse().is_ok());
           assert!(Written::<SettingValue>::new("a\0b").parse().is_err());
           assert!(Written::<FieldName>::new("deps").parse().is_ok());
           assert!(Written::<FieldName>::new("a b").parse().is_err());
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `parse` found for struct `Written<SettingName>` in the current scope
      --> crates/buildl-core/src/types/written.rs:234:60
   error[E0599]: no method named `parse` found for struct `Written<FieldName>` in the current scope
      --> crates/buildl-core/src/types/written.rs:238:51
   error: could not compile `buildl-core` (lib test) due to 4 previous errors
   ```

3. Replace the module's
   `use crate::types::{Argument, Directory, Label, OutputName, SettingValue, SourcePath, TargetName};`
   with:

   ```rust
   use crate::types::{
       Argument, Directory, FieldName, Label, OutputName, SettingName, SettingValue, SourcePath,
       TargetName,
   };
   ```

   Insert, directly after the `impl Written<Argument>` block (so before `impl Written<SettingValue>`):

   ```rust
   impl Written<SettingName> {
       /// Validates the text as a setting name.
       ///
       /// # Errors
       ///
       /// Returns whatever [`SettingName::parse`] rejects.
       pub fn parse(&self) -> Result<SettingName> {
           SettingName::parse(self.text.clone())
       }
   }
   ```

   Insert, directly after the `impl Written<SettingValue>` block (the last block before
   `#[cfg(test)]`):

   ```rust
   impl Written<FieldName> {
       /// Validates the text as a field name.
       ///
       /// # Errors
       ///
       /// Returns whatever [`FieldName::parse`] rejects.
       pub fn parse(&self) -> Result<FieldName> {
           FieldName::parse(self.text.clone())
       }
   }
   ```

4. Run and confirm green. This task adds no test function; it extends
   `every_plain_conversion_parses_its_own_type`:

   ```
   $ cargo test -p buildl-core --lib written
   running 7 tests
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert written setting names and field names`.

---

## Task 7 — Convert written environment names and descriptions

**Files:**
- Modify `crates/buildl-core/src/types/written.rs`

**Steps:**

1. Write the failing test. In the test module, replace the `use crate::types::{…};` block with:

   ```rust
       use crate::types::{
           Argument, Description, Directory, EnvName, FieldName, Label, OutputName, SettingName,
           SettingValue, SourcePath, TargetName,
       };
   ```

   and replace `every_plain_conversion_parses_its_own_type` with its final form:

   ```rust
       #[test]
       fn every_plain_conversion_parses_its_own_type() {
           assert!(Written::<SourcePath>::new("src/a.c").parse().is_ok());
           assert!(Written::<SourcePath>::new("../a.c").parse().is_err());
           assert!(Written::<OutputName>::new("bin/app").parse().is_ok());
           assert!(Written::<OutputName>::new("/app").parse().is_err());
           assert!(Written::<EnvName>::new("PATH").parse().is_ok());
           assert!(Written::<EnvName>::new("1X").parse().is_err());
           assert!(Written::<Argument>::new("$in").parse().is_ok());
           assert!(Written::<Argument>::new("a\0b").parse().is_err());
           assert!(Written::<Description>::new("compile $in").parse().is_ok());
           assert!(Written::<Description>::new("a\nb").parse().is_err());
           assert!(Written::<SettingName>::new("test_filter").parse().is_ok());
           assert!(Written::<SettingName>::new("a.b").parse().is_err());
           assert!(Written::<SettingValue>::new("").parse().is_ok());
           assert!(Written::<SettingValue>::new("a\0b").parse().is_err());
           assert!(Written::<FieldName>::new("deps").parse().is_ok());
           assert!(Written::<FieldName>::new("a b").parse().is_err());
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib
   error[E0599]: no method named `parse` found for struct `Written<EnvName>` in the current scope
      --> crates/buildl-core/src/types/written.rs:257:49
   error[E0599]: no method named `parse` found for struct `Written<Description>` in the current scope
      --> crates/buildl-core/src/types/written.rs:261:60
   error: could not compile `buildl-core` (lib test) due to 4 previous errors
   ```

3. Replace the module's `use crate::types::{…};` block with its final form:

   ```rust
   use crate::types::{
       Argument, Description, Directory, EnvName, FieldName, Label, OutputName, SettingName,
       SettingValue, SourcePath, TargetName,
   };
   ```

   Insert, directly after the `impl Written<OutputName>` block (so before `impl Written<Argument>`):

   ```rust
   impl Written<EnvName> {
       /// Validates the text as an environment variable name.
       ///
       /// # Errors
       ///
       /// Returns whatever [`EnvName::parse`] rejects.
       pub fn parse(&self) -> Result<EnvName> {
           EnvName::parse(self.text.clone())
       }
   }
   ```

   Insert, directly after the `impl Written<Argument>` block (so before `impl Written<SettingName>`):

   ```rust
   impl Written<Description> {
       /// Validates the text as a description.
       ///
       /// # Errors
       ///
       /// Returns whatever [`Description::parse`] rejects.
       pub fn parse(&self) -> Result<Description> {
           Description::parse(self.text.clone())
       }
   }
   ```

4. Run and confirm green. This task adds no test function; it completes
   `every_plain_conversion_parses_its_own_type`:

   ```
   $ cargo test -p buildl-core --lib written
   running 7 tests
   test types::written::tests::keeps_the_text_exactly_as_written ... ok
   test types::written::tests::a_target_name_parses_or_refuses ... ok
   test types::written::tests::an_empty_directory_reports_the_directory_grammar ... ok
   test types::written::tests::a_directory_never_escapes_or_stays_empty ... ok
   test types::written::tests::a_directory_joins_under_its_base ... ok
   test types::written::tests::every_plain_conversion_parses_its_own_type ... ok
   test types::written::tests::a_label_resolves_every_reference_form_against_its_base ... ok
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert written environment names and descriptions`.

---

## Verification summary (plan-level)

- `crates/buildl-core/src/types/written.rs` holds, in order: `impl<T> Written<T>`, then one
  `impl Written<X>` block for each of `TargetName`, `Label`, `Directory`, `SourcePath`, `OutputName`,
  `EnvName`, `Argument`, `Description`, `SettingName`, `SettingValue`, `FieldName` — eleven
  conversions, one per destination type, and no `Serialize`, `Deserialize` or `Display`.
- `cargo test -p buildl-core --lib written` → `7 passed`: `keeps_the_text_exactly_as_written`,
  `a_target_name_parses_or_refuses`, `a_label_resolves_every_reference_form_against_its_base`,
  `a_directory_joins_under_its_base`, `a_directory_never_escapes_or_stays_empty`,
  `an_empty_directory_reports_the_directory_grammar`, `every_plain_conversion_parses_its_own_type`.
- `buildl_core::Written` and `buildl_core::types::Written` both resolve (the re-exports in
  `types/mod.rs` and `lib.rs`).
- `cargo fmt --all` then `cargo make dod` green; `cargo make guard-core-purity`,
  `cargo make guard-crate-edges` and `cargo deny check` green. `crates/buildl-core/Cargo.toml` and
  `crates/expected-edges.txt` unchanged.
- Plans 03 and 04 consume this plan's output: the staged types hold `Written<…>` fields, and
  `EvaluationFailure::{UnknownField, WrongFieldType}` hold a `Written<FieldName>` whose
  `as_written()` their `Display` renders.
