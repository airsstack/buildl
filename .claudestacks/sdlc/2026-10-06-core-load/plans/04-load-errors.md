---
status: approved
created: 2026-10-06
depends-on: [03]
---

# Load Errors Implementation Plan

**Goal:** Every way Load can fail is an `Error` variant a caller branches on by field.

**Architecture:** All additions land in the one file `crates/buildl-core/src/error.rs`, beside the
existing `Error` and `NameKind`, because the crate keeps a single error type that every module
returns without conversion. Four small classification enums — `EvaluationLimit`,
`EvaluationFailure`, `DeclarationField`, `ActionFound` — carry what a caller needs to branch on, so
no caller parses a message; each gets a hand-written `Display` that `Error`'s `#[error(...)]`
strings interpolate. Five `Error` variants are appended after `FloatRejected`; none reshapes an
existing one. `InvalidDeclaration` wraps its grammar failure as `#[source] source: Box<Self>`, and
`MissingBuildFile` renders its requester through one private helper, `requested_by_suffix`, called
from the `#[error]` format arguments. `lib.rs` re-exports the four new public enums.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `thiserror` 2, `cargo-make`.

**Content authority:** spec §6.1 (new variants, `EvaluationFailure`, `DeclarationField`,
`ActionFound`), §7 (`error.rs` row of the module layout), §4.2–§4.3 (when each variant is raised).

---

## Context an implementer needs

Plans `01`–`03` are `done`. What they leave that this plan reads:

| Item | From | Used here for |
|---|---|---|
| `NameKind` with 14 variants (`Directory` … `FieldName`) and the 14-line `name_kind_renders_a_human_phrase` test | `01` | unchanged; this plan inserts around it |
| `Diagnostic::new(impl Into<String>)`, `Display` | `01` | `Error::Evaluation.diagnostic` |
| `FieldName` | `01` | `EvaluationFailure::{UnknownField, WrongFieldType}` |
| `Directory::root()`, `Directory::parse`, `TargetName::parse` | existing | test fixtures |
| `Provenance::new(PathBuf, Directory)`, `Display` = the file path | existing | every site-carrying variant |
| `Written<T>::new`, `Written<T>::as_written` | `02` | `EvaluationFailure` fields and their `Display` |
| `DeclarationOrder::new(u32)`, `Display` = `#{n}` | `03` | `order` fields |
| `Declaration` | `03` | `Error::Nondeterministic` |

At the start of this plan `crates/buildl-core/src/error.rs` is the foundation file plus plan `01`'s
`NameKind` growth: module doc with three Responsibilities bullets (`Error`, `NameKind`, `Result`),
`use core::fmt;` and no other import, `Error` with `InvalidName`, `CanonicalJson`,
`FloatRejected`, then `NameKind` and its `Display`, then a test module that opens with
`use super::{Error, NameKind};` and holds four tests: `name_kind_renders_a_human_phrase`,
`invalid_name_states_what_was_found`, `canonical_json_states_the_value_could_not_be_serialized`,
`float_rejected_states_where_the_float_was_found`. That test module has **no**
`#![expect(clippy::unwrap_used, …)]` header, because none of those tests unwraps.

`crates/buildl-core/src/lib.rs` at the start of this plan ends with:

```rust
pub mod error;
pub mod json;
pub mod ports;
pub mod types;

pub use error::{Error, NameKind, Result};
pub use ports::Clock;
pub use types::{
    Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
    Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
    Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
    SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
    StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
};
```

This plan changes only the `pub use error::…` line of `lib.rs`; its doc text is untouched (plan
`08` owns the crate doc). `pub mod load;`, `pub mod pipeline;` and the `DeclarationSource`/`Ports`
re-exports arrive in plans `05`–`07`.

`cargo test -p buildl-core --lib` reports **111** tests at the start; the `error::` filter reports
**4**. Each task below adds tests to `error::tests` only.

Gate facts that bite in this file, each of which turns `cargo make dod` red under `-D warnings`:

- `missing_docs` is warn: every new `pub` enum, variant and field carries a `///` line. The
  private `requested_by_suffix` carries one too, by house style.
- `#![expect(clippy::unwrap_used, …)]` must appear **exactly when** the test module first unwraps.
  An `expect` that is not fulfilled warns, and the warning fails the gate. Task 2 adds the header,
  because its `lib_file()` fixture is the module's first `.unwrap()`; task 1 must not.
- A test-only helper that no test calls is `dead_code`. `lib_file()` therefore arrives in task 2
  with its first caller, not earlier.
- Rustdoc runs with `-D warnings`, so an intra-doc link to an item that does not exist yet is a
  broken link. `DeclarationField`'s doc links [`Error::InvalidDeclaration`] and `ActionFound`'s doc
  links [`Error::ActionConflict`]; each enum therefore lands in the same task as the variant it
  links to (tasks 4 and 5).
- Every `use` list is unused-import-clean at the end of each task. The `use crate::types::{…}`
  line in the file body and the two `use` lines in the test module grow one name at a time, and
  each task gives their exact rustfmt form.
- No `#[allow]`; never write the strings `clippy::disallowed_`, `clippy::all`, `clippy::style` in
  this crate (`guard-core-purity` greps for them).
- thiserror 2 accepts a helper call in format arguments:
  `#[error("… {}", helper(.field.as_ref()))]` — used by `MissingBuildFile`.

Run `cargo fmt --all` before every `cargo make dod`. Every Rust block below is in rustfmt's
canonical form; retyping can drift, and `fmt-check` is the gate's first step.

Commits follow Conventional Commits with scope `buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/error.rs — [modify] EvaluationLimit, EvaluationFailure (task 1)
                                  [modify] Error::Evaluation, test-module unwrap header (task 2)
                                  [modify] Error::MissingBuildFile, requested_by_suffix (task 3)
                                  [modify] DeclarationField, Error::InvalidDeclaration (task 4)
                                  [modify] ActionFound, Error::ActionConflict (task 5)
                                  [modify] Error::Nondeterministic (task 6)
crates/buildl-core/src/lib.rs   — [modify] re-export the new public enums (tasks 1, 4, 5)
```

Item order in `error.rs` at the end of this plan:

```
//! module doc
use core::fmt;  use crate::types::{…};
Result<T>
Error { InvalidName, CanonicalJson, FloatRejected,
        Evaluation, MissingBuildFile, InvalidDeclaration, ActionConflict, Nondeterministic }
fn requested_by_suffix
EvaluationLimit      + Display
EvaluationFailure    + Display
DeclarationField     + Display
ActionFound          + Display
NameKind             + Display
mod tests
```

---

## Task 1 — `EvaluationLimit` and `EvaluationFailure`

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. In `crates/buildl-core/src/error.rs`, replace the test module's import
   line

   ```rust
       use super::{Error, NameKind};
   ```

   with

   ```rust
       use super::{Error, EvaluationFailure, EvaluationLimit, NameKind};
       use crate::types::{FieldName, Written};
   ```

   and append this test after `float_rejected_states_where_the_float_was_found`, as the module's
   last item:

   ```rust
       #[test]
       fn every_evaluation_failure_renders_a_human_phrase() {
           let field = || Written::<FieldName>::new("dep");
           let cases = [
               (EvaluationFailure::Syntax, "syntax error"),
               (EvaluationFailure::Runtime, "runtime error"),
               (EvaluationFailure::Refused, "operation refused"),
               (
                   EvaluationFailure::LimitReached {
                       limit: EvaluationLimit::Instructions,
                   },
                   "instruction ceiling reached",
               ),
               (
                   EvaluationFailure::LimitReached {
                       limit: EvaluationLimit::Memory,
                   },
                   "memory ceiling reached",
               ),
               (
                   EvaluationFailure::LimitReached {
                       limit: EvaluationLimit::Staging,
                   },
                   "staging cap reached",
               ),
               (
                   EvaluationFailure::UnknownField { field: field() },
                   r#"unknown field "dep""#,
               ),
               (
                   EvaluationFailure::WrongFieldType { field: field() },
                   r#"field "dep" has the wrong type"#,
               ),
           ];
           for (failure, rendered) in cases {
               assert_eq!(failure.to_string(), rendered);
           }
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0432]: unresolved imports `super::EvaluationFailure`, `super::EvaluationLimit`
      --> crates/buildl-core/src/error.rs:112:24
   ```

3. Write the code. In the module doc, insert after the `NameKind` bullet:

   ```rust
   //! - [`EvaluationFailure`] and [`EvaluationLimit`] — how an adapter failed to evaluate a build
   //!   file.
   ```

   so the Responsibilities list reads `Error`, `NameKind`, `EvaluationFailure`/`EvaluationLimit`,
   `Result`. Replace the import block

   ```rust
   use core::fmt;
   ```

   with

   ```rust
   use core::fmt;

   use crate::types::{FieldName, Written};
   ```

   Insert between the closing `}` of `enum Error` and the `/// Which domain name an
   [`Error::InvalidName`] is about.` doc of `NameKind`:

   ```rust
   /// A ceiling an evaluation can reach.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum EvaluationLimit {
       /// The instruction ceiling.
       Instructions,
       /// The memory ceiling.
       Memory,
       /// The cap on how many declarations one file may stage.
       Staging,
   }

   impl fmt::Display for EvaluationLimit {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Instructions => "instruction ceiling",
               Self::Memory => "memory ceiling",
               Self::Staging => "staging cap",
           })
       }
   }

   /// How an adapter failed to evaluate one build file.
   ///
   /// A closed set, so an adapter reports every failure through it without a new variant.
   #[derive(Debug, Clone, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum EvaluationFailure {
       /// The build file does not parse.
       Syntax,
       /// Evaluation raised an error.
       Runtime,
       /// The runtime refused an operation the declaration policy does not grant.
       Refused,
       /// A ceiling was reached.
       LimitReached {
           /// Which ceiling.
           limit: EvaluationLimit,
       },
       /// An option table holds a field its primitive does not accept.
       UnknownField {
           /// The field, as written.
           field: Written<FieldName>,
       },
       /// A field holds a value of the wrong type.
       WrongFieldType {
           /// The field, as written.
           field: Written<FieldName>,
       },
   }

   impl fmt::Display for EvaluationFailure {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           match self {
               Self::Syntax => f.write_str("syntax error"),
               Self::Runtime => f.write_str("runtime error"),
               Self::Refused => f.write_str("operation refused"),
               Self::LimitReached { limit } => write!(f, "{limit} reached"),
               Self::UnknownField { field } => write!(f, "unknown field {:?}", field.as_written()),
               Self::WrongFieldType { field } => {
                   write!(f, "field {:?} has the wrong type", field.as_written())
               }
           }
       }
   }
   ```

   `EvaluationFailure` is not `Copy`: two of its variants hold a `Written<FieldName>`, which owns
   a `String`. `UnknownField` and `WrongFieldType` render the field `{:?}`-quoted, so a field name
   written with a space or a quote stays legible on one line.

   In `crates/buildl-core/src/lib.rs`, replace

   ```rust
   pub use error::{Error, NameKind, Result};
   ```

   with

   ```rust
   pub use error::{Error, EvaluationFailure, EvaluationLimit, NameKind, Result};
   ```

4. Run and confirm green. New test: `every_evaluation_failure_renders_a_human_phrase`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 5 tests
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): classify how an adapter fails to evaluate a build file`.

---

## Task 2 — `Error::Evaluation`

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. Replace the test module's opening, from `mod tests {` through the
   `use crate::types::{FieldName, Written};` line:

   ```rust
   mod tests {
       use super::{Error, EvaluationFailure, EvaluationLimit, NameKind};
       use crate::types::{FieldName, Written};
   ```

   with the version below. It adds the module's `unwrap_used` expectation — `lib_file()` is the
   module's first `.unwrap()` — the `PathBuf` import, the widened `crate::types` import, and the
   `lib_file()` fixture every later test in this plan uses:

   ```rust
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use super::{Error, EvaluationFailure, EvaluationLimit, NameKind};
       use crate::types::{Diagnostic, Directory, FieldName, Provenance, Written};

       fn lib_file() -> Provenance {
           Provenance::new(
               PathBuf::from("lib/build.lua"),
               Directory::parse("lib").unwrap(),
           )
       }
   ```

   Insert this test between `float_rejected_states_where_the_float_was_found` and
   `every_evaluation_failure_renders_a_human_phrase`:

   ```rust
       #[test]
       fn evaluation_names_the_file_the_failure_and_the_message() {
           let err = Error::Evaluation {
               provenance: lib_file(),
               failure: EvaluationFailure::Syntax,
               diagnostic: Diagnostic::new("unexpected symbol near '}'"),
           };
           assert_eq!(
               err.to_string(),
               "lib/build.lua: syntax error: unexpected symbol near '}'"
           );
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0599]: no variant named `Evaluation` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:255:26
   ```

3. Write the code. Append this variant to `enum Error`, after `FloatRejected`:

   ```rust
       /// An adapter could not evaluate a build file.
       #[error("{provenance}: {failure}: {diagnostic}")]
       Evaluation {
           /// The build file.
           provenance: Provenance,
           /// What kind of failure it was.
           failure: EvaluationFailure,
           /// The adapter's message, for display only.
           diagnostic: Diagnostic,
       },
   ```

   Replace

   ```rust
   use crate::types::{FieldName, Written};
   ```

   with

   ```rust
   use crate::types::{Diagnostic, FieldName, Provenance, Written};
   ```

4. Run and confirm green. New test: `evaluation_names_the_file_the_failure_and_the_message`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 6 tests
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): report a build file the adapter could not evaluate`.

---

## Task 3 — `Error::MissingBuildFile` and its requester suffix

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. Append after `every_evaluation_failure_renders_a_human_phrase`, as the
   module's last item:

   ```rust
       #[test]
       fn missing_build_file_names_the_root_or_the_requester() {
           let root = Error::MissingBuildFile {
               directory: Directory::root(),
               requested_by: None,
           };
           assert_eq!(root.to_string(), "no build file in // (the workspace root)");
           let requested = Error::MissingBuildFile {
               directory: Directory::parse("lib/text").unwrap(),
               requested_by: Some(lib_file()),
           };
           assert_eq!(
               requested.to_string(),
               "no build file in //lib/text (requested by lib/build.lua)"
           );
       }
   ```

   The test module's imports need no change: `Directory` arrived in task 2.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0599]: no variant named `MissingBuildFile` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:317:27
   error[E0599]: no variant named `MissingBuildFile` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:322:32
   ```

3. Write the code. Append this variant to `enum Error`, after `Evaluation`:

   ```rust
       /// A directory that had to be evaluated holds no build file.
       #[error(
           "no build file in //{directory}{}",
           requested_by_suffix(.requested_by.as_ref())
       )]
       MissingBuildFile {
           /// The directory.
           directory: Directory,
           /// The build file whose `subdir` call requested it; `None` for the workspace root.
           requested_by: Option<Provenance>,
       },
   ```

   Insert the helper directly after the closing `}` of `enum Error`, before `EvaluationLimit`:

   ```rust
   /// The suffix naming who required a missing build file.
   fn requested_by_suffix(requested_by: Option<&Provenance>) -> String {
       requested_by.map_or_else(
           || " (the workspace root)".to_owned(),
           |provenance| format!(" (requested by {provenance})"),
       )
   }
   ```

   It stays private: it exists only to feed the `#[error]` format arguments, and the
   `missing_build_file_names_the_root_or_the_requester` test covers both its arms through
   `Display`. Replace

   ```rust
   use crate::types::{Diagnostic, FieldName, Provenance, Written};
   ```

   with

   ```rust
   use crate::types::{Diagnostic, Directory, FieldName, Provenance, Written};
   ```

4. Run and confirm green. New test: `missing_build_file_names_the_root_or_the_requester`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 7 tests
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): report a directory with no build file and who requested it`.

---

## Task 4 — `DeclarationField` and `Error::InvalidDeclaration`

One task, because `DeclarationField`'s doc links [`Error::InvalidDeclaration`]: landing the enum
alone would leave a broken intra-doc link, which `RUSTDOCFLAGS=-D warnings` fails.

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Replace the test module's two import lines

   ```rust
       use super::{Error, EvaluationFailure, EvaluationLimit, NameKind};
       use crate::types::{Diagnostic, Directory, FieldName, Provenance, Written};
   ```

   with

   ```rust
       use super::{DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind};
       use crate::types::{
           DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, TargetName, Written,
       };
   ```

   and append after `missing_build_file_names_the_root_or_the_requester`, as the module's last
   items:

   ```rust
       #[test]
       fn invalid_declaration_names_the_site_the_field_and_the_grammar() {
           let source = TargetName::parse("a/b").unwrap_err();
           let err = Error::InvalidDeclaration {
               provenance: lib_file(),
               order: DeclarationOrder::new(2),
               field: DeclarationField::Dep,
               source: Box::new(source),
           };
           assert_eq!(
               err.to_string(),
               r#"lib/build.lua: declaration #2: invalid dep: invalid target name: "a/b" — may hold only ASCII letters, digits, '.', '-' and '_'"#
           );
           assert!(std::error::Error::source(&err).is_some());
       }

       #[test]
       fn every_declaration_field_renders_a_human_phrase() {
           let cases = [
               (DeclarationField::Name, "name"),
               (DeclarationField::Dep, "dep"),
               (DeclarationField::Input, "input"),
               (DeclarationField::Output, "output"),
               (DeclarationField::Env, "env"),
               (DeclarationField::Run, "run"),
               (DeclarationField::Rule, "rule"),
               (DeclarationField::Description, "description"),
               (DeclarationField::Target, "target"),
               (DeclarationField::SettingName, "setting name"),
               (DeclarationField::SettingValue, "setting value"),
               (DeclarationField::Subdir, "subdir"),
           ];
           for (field, rendered) in cases {
               assert_eq!(field.to_string(), rendered);
           }
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0432]: unresolved import `super::DeclarationField`
      --> crates/buildl-core/src/error.rs:218:17
   error[E0599]: no variant named `InvalidDeclaration` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:356:26
   ```

3. Write the code. In the module doc, insert after the `EvaluationFailure`/`EvaluationLimit`
   bullet:

   ```rust
   //! - [`DeclarationField`] — which field of a declaration an [`Error::InvalidDeclaration`] is
   //!   about.
   ```

   Append this variant to `enum Error`, after `MissingBuildFile`:

   ```rust
       /// A value in a declaration did not satisfy its grammar.
       #[error("{provenance}: declaration {order}: invalid {field}: {source}")]
       InvalidDeclaration {
           /// The build file.
           provenance: Provenance,
           /// The call's position in the file.
           order: DeclarationOrder,
           /// The field holding the value.
           field: DeclarationField,
           /// The grammar failure.
           #[source]
           source: Box<Self>,
       },
   ```

   `source` is `Box<Self>`, not the spec's `Box<Error>`: the two spell the same type, and inside
   the enum's own definition clippy's `use_self` (nursery) rejects `Box<Error>` with
   `unnecessary structure name repetition`. The box breaks the
   recursion; `#[source]` makes `std::error::Error::source` return the inner grammar failure,
   which the test's last assertion pins.

   Insert after the `impl fmt::Display for EvaluationFailure` block, before `NameKind`'s doc:

   ```rust
   /// Which field of a declaration an [`Error::InvalidDeclaration`] is about.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum DeclarationField {
       /// The declared name.
       Name,
       /// An entry of `deps`.
       Dep,
       /// An entry of `inputs`.
       Input,
       /// An entry of `outputs`.
       Output,
       /// An entry of `env`.
       Env,
       /// The `run` command or one of its arguments.
       Run,
       /// The `rule` reference.
       Rule,
       /// A rule's `desc`.
       Description,
       /// An alias's target.
       Target,
       /// A setting's name.
       SettingName,
       /// A setting's default value.
       SettingValue,
       /// A `subdir` request.
       Subdir,
   }

   impl fmt::Display for DeclarationField {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Name => "name",
               Self::Dep => "dep",
               Self::Input => "input",
               Self::Output => "output",
               Self::Env => "env",
               Self::Run => "run",
               Self::Rule => "rule",
               Self::Description => "description",
               Self::Target => "target",
               Self::SettingName => "setting name",
               Self::SettingValue => "setting value",
               Self::Subdir => "subdir",
           })
       }
   }
   ```

   Replace

   ```rust
   use crate::types::{Diagnostic, Directory, FieldName, Provenance, Written};
   ```

   with

   ```rust
   use crate::types::{DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, Written};
   ```

   In `crates/buildl-core/src/lib.rs`, replace

   ```rust
   pub use error::{Error, EvaluationFailure, EvaluationLimit, NameKind, Result};
   ```

   with

   ```rust
   pub use error::{DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result};
   ```

4. Run and confirm green. New tests:
   `invalid_declaration_names_the_site_the_field_and_the_grammar`,
   `every_declaration_field_renders_a_human_phrase`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 9 tests
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): report an invalid declaration field with its grammar failure`.

---

## Task 5 — `ActionFound` and `Error::ActionConflict`

One task, for the same reason as task 4: `ActionFound`'s doc links [`Error::ActionConflict`].

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Replace the test module's import line

   ```rust
       use super::{DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind};
   ```

   with

   ```rust
       use super::{
           ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind,
       };
   ```

   and append after `every_declaration_field_renders_a_human_phrase`, as the module's last item:

   ```rust
       #[test]
       fn action_conflict_states_what_was_found() {
           let both = Error::ActionConflict {
               provenance: lib_file(),
               order: DeclarationOrder::new(0),
               found: ActionFound::Both,
           };
           assert_eq!(
               both.to_string(),
               "lib/build.lua: declaration #0: a target names both 'rule' and 'run'"
           );
           let neither = Error::ActionConflict {
               provenance: lib_file(),
               order: DeclarationOrder::new(1),
               found: ActionFound::Neither,
           };
           assert_eq!(
               neither.to_string(),
               "lib/build.lua: declaration #1: a target names neither 'rule' nor 'run'"
           );
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0432]: unresolved import `super::ActionFound`
      --> crates/buildl-core/src/error.rs:283:9
   error[E0599]: no variant named `ActionConflict` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:458:27
   error[E0599]: no variant named `ActionConflict` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:467:30
   ```

3. Write the code. In the module doc, insert after the `DeclarationField` bullet:

   ```rust
   //! - [`ActionFound`] — what an [`Error::ActionConflict`] found instead of one action.
   ```

   Append this variant to `enum Error`, after `InvalidDeclaration`:

   ```rust
       /// A target named both or neither of `rule` and `run`.
       #[error("{provenance}: declaration {order}: a target names {found}")]
       ActionConflict {
           /// The build file.
           provenance: Provenance,
           /// The call's position in the file.
           order: DeclarationOrder,
           /// What was found instead of exactly one.
           found: ActionFound,
       },
   ```

   Insert after the `impl fmt::Display for DeclarationField` block, before `NameKind`'s doc:

   ```rust
   /// What an [`Error::ActionConflict`] found instead of exactly one of `rule` and `run`.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum ActionFound {
       /// Both were given.
       Both,
       /// Neither was given.
       Neither,
   }

   impl fmt::Display for ActionFound {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Both => "both 'rule' and 'run'",
               Self::Neither => "neither 'rule' nor 'run'",
           })
       }
   }
   ```

   `ActionFound` is the one new enum without `#[non_exhaustive]`: "exactly one of two" has
   exactly two ways to be wrong, so the set is closed by arithmetic, and a caller may match it
   exhaustively.

   In `crates/buildl-core/src/lib.rs`, replace

   ```rust
   pub use error::{DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result};
   ```

   with

   ```rust
   pub use error::{
       ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
   };
   ```

   This is the line's final form.

4. Run and confirm green. New test: `action_conflict_states_what_was_found`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 10 tests
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): report a target that names both or neither of rule and run`.

---

## Task 6 — `Error::Nondeterministic`

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing test. Append after `action_conflict_states_what_was_found`, as the module's
   last item:

   ```rust
       #[test]
       fn nondeterministic_names_the_file_to_fix() {
           let err = Error::Nondeterministic {
               provenance: lib_file(),
               first_run: None,
               second_run: None,
           };
           assert_eq!(
               err.to_string(),
               "lib/build.lua: declarations differ between two evaluations"
           );
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib error::
   error[E0599]: no variant named `Nondeterministic` found for enum `error::Error`
      --> crates/buildl-core/src/error.rs:509:26
   ```

3. Write the code. Append this variant to `enum Error`, after `ActionConflict`, as its last
   variant:

   ```rust
       /// Two evaluations of the same workspace declared different things.
       #[error("{provenance}: declarations differ between two evaluations")]
       Nondeterministic {
           /// The build file to fix: the one whose declaration the other evaluation lacks.
           provenance: Provenance,
           /// The first evaluation's declaration at the first difference; `None` past its end.
           first_run: Option<Box<Declaration>>,
           /// The second evaluation's declaration at the first difference; `None` past its end.
           second_run: Option<Box<Declaration>>,
       },
   ```

   The two declarations are boxed so the variant does not inflate every `Result` in the crate to
   the size of a `Declaration`; the message names only the file, and the two values are for a
   caller that wants to show the difference.

   Replace

   ```rust
   use crate::types::{DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, Written};
   ```

   with

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, Written,
   };
   ```

   This is the line's final form.

4. Run and confirm green. New test: `nondeterministic_names_the_file_to_fix`.

   ```
   $ cargo test -p buildl-core --lib error::
   running 11 tests
   test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): report two evaluations that declared different things`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all
$ cargo make dod
$ cargo make guard-core-purity
$ cargo make guard-crate-edges
$ cargo deny check
```

All exit 0; `cargo deny check` reports `advisories ok, bans ok, licenses ok, sources ok`. This plan
adds no dependency, so `crates/buildl-core/Cargo.toml` and `crates/expected-edges.txt` are
untouched.

`cargo test -p buildl-core --lib` reports **118** tests (111 at the start, plus 7), and the
`error::` filter reports **11**:

| Test | Task |
|---|---|
| `every_evaluation_failure_renders_a_human_phrase` | 1 |
| `evaluation_names_the_file_the_failure_and_the_message` | 2 |
| `missing_build_file_names_the_root_or_the_requester` | 3 |
| `invalid_declaration_names_the_site_the_field_and_the_grammar` | 4 |
| `every_declaration_field_renders_a_human_phrase` | 4 |
| `action_conflict_states_what_was_found` | 5 |
| `nondeterministic_names_the_file_to_fix` | 6 |

At the end of this plan `crates/buildl-core/src/error.rs` holds the five Load variants and the four
classification enums of spec §6.1, every one rendered by a pinned `Display` string, and
`buildl_core::{ActionFound, DeclarationField, EvaluationFailure, EvaluationLimit}` resolve from
outside the crate. No caller of these variants exists yet: plan `06`'s Load raises
`Evaluation`, `MissingBuildFile`, `InvalidDeclaration` and `ActionConflict`, and plan `07`'s
`check` raises `Nondeterministic`.
