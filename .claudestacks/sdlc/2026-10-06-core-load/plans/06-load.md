---
status: approved
created: 2026-10-06
depends-on: [04, 05]
---

# Load Implementation Plan

**Goal:** `buildl_core::load` turns a `DeclarationSource`'s staged build files into one sorted `Vec<Declaration>`.

**Architecture:** One phase module, `crates/buildl-core/src/load/`: an export-only `mod.rs` and
`traversal.rs`, which holds the public `load` and its private steps — `build_file`, the `Site`
error-wrapping helper, `resolve_subdirs`, `convert` with one helper per declaration kind, and
`sort`. `load` takes the one port it needs (`&S: DeclarationSource`), not the `Ports` bundle. Pure
helpers are unit-tested in `traversal.rs`; every flow through the port is tested in one
integration-test binary, `crates/buildl-core/tests/flows/`, against `FakeSource`, a fake built from
the public API alone — so the compiler proves on every build that an adapter crate can build every
staged value and implement the port.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `std` collections only (`VecDeque`,
`BTreeSet`, `BTreeMap`, `RefCell`), `cargo-make`. No new dependency.

**Content authority:** spec §4 (Load: §4.1 signature, §4.2 traversal, §4.3 staged to typed, §4.4
merge), §5.3 (fakes), §7 (module layout: `load/`, `tests/flows/`), §8 (the load scenarios).

---

## Context an implementer needs

Plans `01`–`05` are done. What they leave that this plan builds on:

| From | Item | Used here as |
|---|---|---|
| 01 | `EntryName`, `SourcePath`, `OutputName`, `EnvName`, `Argument`, `Command`, `Description`, `SettingName`, `SettingValue`, `Diagnostic` | the validated field types; `EntryName` is `load`'s parameter |
| 02 | `Written<T>` with `parse()` per target type, `Written<Label>::resolve(&Directory)`, `Written<Directory>::under(&Directory)`, `as_written()` | every staged value's conversion |
| 03 | `Declaration::new/provenance/item`, `Declared` (+ `name()`), `Target`/`Rule`/`Alias`/`Setting` (pub fields), `Action`, `TargetRole`, `NetworkAccess`, `Freshness`, `BuildFile::new/provenance/directory/file`, `Evaluated`, `StagedFile`, `StagedSubdir`, `StagedDeclaration`, `StagedItem`, `StagedTarget`/`StagedRule`/`StagedAlias`/`StagedSetting`, `DeclarationOrder::new` | Load's input and output |
| 04 | `Error::{Evaluation, MissingBuildFile, InvalidDeclaration, ActionConflict}`, `DeclarationField`, `ActionFound`, `EvaluationFailure`, `EvaluationLimit` | every error Load raises or passes through |
| 05 | `ports::DeclarationSource` (`fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>`), `ports::Ports` | the port Load calls; the fake implements it |

`crates/buildl-core/src/lib.rs` code section as plan `05` leaves it (the crate doc above it is the
one `crates/buildl-core/src/lib.rs:1-52` holds today; this plan does not touch it — plan `08` owns
doc prose, including the module-map row for `load`):

```rust
pub mod error;
pub mod json;
pub mod ports;
pub mod types;

pub use error::{
    ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
};
pub use ports::{Clock, DeclarationSource, Ports};
pub use types::{
    Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
    Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
    Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
    SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
    StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
};
```

`crates/buildl-core/tests/` does not exist yet; task 2 creates it.

### Why the tasks are cut this way

Two compiler facts decide the order, and both turn the gate red if ignored:

| Fact | Consequence |
|---|---|
| A private fn reached only from `#[cfg(test)]` code is `dead_code` in the non-test lib build, and `clippy -D warnings` fails on it | every helper is wired into the public `load` in the task that adds it, so `load` exists from task 1 and grows; until task 5 it returns an empty list for every staged file (no flow before task 5 asserts output) |
| `convert`'s `match` over `StagedItem` must be exhaustive | `convert` lands with all four kind arms in task 5; the target arm starts as a minimal `convert_target` (label and a `run`-only action, every list empty), and tasks 6–10 each add one target field behind a test that fails **at runtime** against the previous task's code |

Unused `pub` items in the test binary's `pub mod common` are **not** flagged (probed: a
`tests/flows/main.rs` with `pub mod common;` holding builders no test called passed
`cargo make dod`), so `common.rs` lands whole in task 2. It ships the single-map `FakeSource`
(`files`, `evaluated`) and no `FakePorts`: everything only `check.rs` uses is plan `07`'s.

Gate facts that bite here (from the contract; all verified on this exact code):

- `cargo make dod` = fmt-check, `clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc`, `cargo test --all-targets`, `cargo test --doc`. Run
  `cargo fmt --all` first; every block below is rustfmt's output.
- `clippy::unwrap_used` is denied; a test module or test file that unwraps opens with
  `#![expect(clippy::unwrap_used, reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal")]`
  — and only if it unwraps (an unfulfilled `expect` fails the gate).
- `clippy::module_inception`: the logic file is `load/traversal.rs`, never `load/load.rs`.
- No `#[allow]`; never write `clippy::disallowed_`, `clippy::all` or `clippy::style` anywhere in
  `crates/buildl-core` (`guard-core-purity` greps for them). No `std::fs`/`env`/`thread`/`process`.
- Rustdoc carries no spec/plan/§ citations.
- Integration tests are one binary, `crates/buildl-core/tests/flows/main.rs` (crate doc `//!`
  required); `common.rs` items are `pub` and documented; `mod load;` and its `#[test]` fns are
  private.

Test counts below assume plans `01`–`05` leave **118** `buildl-core` unit tests, **2** doctests and
no integration test (the count the prototype of plans `01`–`05` gives). If yours differ, the
per-task deltas still hold: unit +1, +2, +3, +2, +1, +1, +1, +1, +1; flows 1 → 6 → 9.

All work happens in the worktree, never on `main`. Commits follow Conventional Commits with scope
`buildl-core`; one commit per task.

### Spec §8 scenarios covered here

| Scenario (spec §8) | Test | Task |
|---|---|---|
| `subdir` requests enqueued out of order | `load::subdirs_requested_out_of_order_still_sort_by_directory_then_name` | 11 |
| a file's declarations shuffled, as `pairs` would | `load::a_shuffled_file_loads_to_byte_identical_canonical_json` | 11 |
| a repeated `subdir`; root requests `a/x` while `a` requests `x` | `traversal::subdirs_resolve_under_the_file_sorted_and_without_repeats`, `load::a_directory_requested_twice_is_evaluated_once` | 3, 4 |
| `subdir("../x")`, `subdir("/x")` | `traversal::a_subdir_that_leaves_the_workspace_is_an_invalid_declaration`, `load::a_subdir_cannot_leave_the_workspace` | 3, 4 |
| root absent; a requested directory absent | `load::a_missing_root_names_no_requester`, `load::a_missing_subdir_names_the_file_that_requested_it` | 2, 4 |
| a target with both / neither of `rule` and `run` | `traversal::a_target_needs_exactly_one_of_rule_and_run` | 6 |
| no `outputs` | `traversal::outputs_default_to_the_target_name` | 9 |
| `deps` mixing `main.o`, `:util.o`, `//lib:text` in `//app` | `traversal::a_target_is_named_in_its_directory_and_resolves_every_dep_form` | 7 |
| an invalid name in each field | `traversal::each_invalid_target_field_is_named`, `traversal::rules_aliases_and_settings_convert_and_name_their_invalid_fields`, the `Subdir` tests above | 10, 5, 3 |
| two files requesting one absent directory, calls shuffled | `load::the_requester_of_a_shared_missing_directory_does_not_depend_on_call_order` | 4 |
| a fake holding an `EvaluationFailure` | `load::an_evaluation_failure_passes_through_unchanged` | 4 |
| a target named `//other:x` | `traversal::a_name_cannot_leave_its_directory` | 5 |
| `run = {}` | `traversal::an_empty_run_is_an_invalid_command` | 5 |

The two `check` scenarios of §8 are plan `07`'s.

### File map

```
crates/buildl-core/src/load/mod.rs        — [create] export-only: `pub mod traversal; pub use traversal::load;` (task 1)
crates/buildl-core/src/load/traversal.rs  — [create] `load`, `build_file` and their tests (task 1)
                                            [modify] grown by tasks 3–11, one helper each
crates/buildl-core/src/lib.rs             — [modify] `pub mod load;` and `pub use load::load;` (task 1)
crates/buildl-core/tests/flows/main.rs    — [create] the integration-test binary: `pub mod common; mod load;` (task 2)
crates/buildl-core/tests/flows/common.rs  — [create] `FakeFile`, `FakeSource`, staged-value builders (task 2)
crates/buildl-core/tests/flows/load.rs    — [create] Load flows (task 2); [modify] tasks 4 and 11
```

---

## Task 1 — Join the entry name under a directory

**Files:**
- Create `crates/buildl-core/src/load/mod.rs`
- Create `crates/buildl-core/src/load/traversal.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/load/mod.rs`:

   ```rust
   //! Load, the first phase: from a workspace root to the validated declarations of every build file
   //! it reaches.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: [`load`].
   //!
   //! Non-responsibilities: evaluating a build file, which is the
   //! [`DeclarationSource`](crate::ports::DeclarationSource) port's; and relationships between
   //! declarations, which belong to the phase that builds the graph.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod traversal;

   pub use traversal::load;
   ```

2. In `crates/buildl-core/src/lib.rs`, add `pub mod load;` between `pub mod json;` and
   `pub mod ports;`, and `pub use load::load;` between the `pub use error::{…};` block and
   `pub use ports::{Clock, DeclarationSource, Ports};`. The code section becomes:

   ```rust
   pub mod error;
   pub mod json;
   pub mod load;
   pub mod ports;
   pub mod types;

   pub use error::{
       ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
   };
   pub use load::load;
   pub use ports::{Clock, DeclarationSource, Ports};
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
       Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
       SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

   The module `load` and the function `load` live in different namespaces, so the pair is legal.
   The crate doc is untouched: it holds no `[`load`]` link yet, so nothing is ambiguous.

3. Write the failing test. Create `crates/buildl-core/src/load/traversal.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 5.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use super::build_file;
       use crate::types::{Directory, EntryName};

       #[test]
       fn the_build_file_is_the_entry_joined_under_its_directory() {
           let entry = EntryName::parse("build.lua").unwrap();
           let root = build_file(&Directory::root(), &entry);
           assert_eq!(root.file(), Path::new("build.lua"));
           assert!(root.directory().is_root());
           let lib = build_file(&Directory::parse("lib/text").unwrap(), &entry);
           assert_eq!(lib.file(), Path::new("lib/text/build.lua"));
           assert_eq!(lib.directory().as_str(), "lib/text");
       }
   }
   ```

4. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `traversal::load`
     --> crates/buildl-core/src/load/mod.rs:16:9
   error[E0432]: unresolved import `super::build_file`
     --> crates/buildl-core/src/load/traversal.rs:12:9
   ```

5. Replace the whole of `crates/buildl-core/src/load/traversal.rs`:

   ```rust
   //! The traversal: which build files are evaluated, and what their staged values become.
   //!
   //! Its own file because it is the whole of Load's logic: a breadth-first walk from the workspace
   //! root along `subdir` requests, a conversion of every staged declaration into a validated one,
   //! and one sort that makes the result independent of the order anything was evaluated or declared
   //! in.
   //!
   //! Responsibilities: [`load`], and the private steps it is made of.
   //!
   //! Non-responsibilities: evaluating a build file, which the
   //! [`DeclarationSource`] port does; and checking one declaration
   //! against another, which the phase that builds the graph does.

   use std::path::PathBuf;

   use crate::error::{Error, Result};
   use crate::ports::DeclarationSource;
   use crate::types::{BuildFile, Declaration, Directory, EntryName, Evaluated, Provenance};

   /// Evaluates the workspace root's build file.
   ///
   /// Its declarations are not converted yet: the result is always empty.
   ///
   /// # Errors
   ///
   /// Returns whatever `source` returns for a build file it cannot evaluate, and
   /// [`Error::MissingBuildFile`] when the root holds no build file.
   pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let file = build_file(&Directory::root(), entry);
       match source.evaluate(&file)? {
           Evaluated::Staged(_) => Ok(Vec::new()),
           Evaluated::Absent => Err(Error::MissingBuildFile {
               directory: Directory::root(),
               requested_by: None,
           }),
       }
   }

   /// The build file of `directory`: the entry name, joined under the directory.
   fn build_file(directory: &Directory, entry: &EntryName) -> BuildFile {
       let path = if directory.is_root() {
           PathBuf::from(entry.as_str())
       } else {
           PathBuf::from(format!("{directory}/{entry}"))
       };
       BuildFile::new(Provenance::new(path, directory.clone()))
   }

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use super::build_file;
       use crate::types::{Directory, EntryName};

       #[test]
       fn the_build_file_is_the_entry_joined_under_its_directory() {
           let entry = EntryName::parse("build.lua").unwrap();
           let root = build_file(&Directory::root(), &entry);
           assert_eq!(root.file(), Path::new("build.lua"));
           assert!(root.directory().is_root());
           let lib = build_file(&Directory::parse("lib/text").unwrap(), &entry);
           assert_eq!(lib.file(), Path::new("lib/text/build.lua"));
           assert_eq!(lib.directory().as_str(), "lib/text");
       }
   }
   ```

   The module doc is final from here on. `load` is the fake-it first step: the root only, and an
   empty result — it exists now so `build_file` is reachable from public code and is not
   `dead_code`. Its doc says exactly that, and tasks 3, 4, 5, 6 and 11 rewrite it as the function
   grows.

6. Run and confirm green — one new test, `load::traversal::tests::the_build_file_is_the_entry_joined_under_its_directory`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 119 passed; 0 failed; …
   ```

7. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

8. Commit `feat(buildl-core): locate a directory's build file and load the workspace root`.

---

## Task 2 — Drive Load through a fake build-file source

**Files:**
- Create `crates/buildl-core/tests/flows/main.rs`
- Create `crates/buildl-core/tests/flows/load.rs`
- Create `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Create `crates/buildl-core/tests/flows/main.rs`:

   ```rust
   //! Pipeline flows through the build-file evaluation port, driven by fakes built from the public
   //! API alone.
   //!
   //! One test binary, so the fakes are shared by every flow without being compiled once per file.
   //! That the fakes compile at all is itself a check: an adapter outside this crate can build every
   //! staged value and implement every port the same way.

   pub mod common;

   mod load;
   ```

2. Write the failing flow. Create `crates/buildl-core/tests/flows/load.rs`:

   ```rust
   //! Load, driven through the build-file port by [`FakeSource`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use buildl_core::{EntryName, Error, load};

   use crate::common::{FakeSource, workspace};

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   #[test]
   fn a_missing_root_names_no_requester() {
       let source = FakeSource::new(workspace(vec![]));
       match load(&source, &entry()).unwrap_err() {
           Error::MissingBuildFile {
               directory,
               requested_by,
           } => {
               assert!(directory.is_root());
               assert!(requested_by.is_none());
           }
           other => unreachable!("expected MissingBuildFile, got {other:?}"),
       }
   }
   ```

   `[`FakeSource`]` in the module doc resolves through the `use crate::common::FakeSource` import.

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0583]: file not found for module `common`
    --> crates/buildl-core/tests/flows/main.rs:8:1
   ```

4. Create `crates/buildl-core/tests/flows/common.rs`:

   ```rust
   //! In-memory implementations of the ports, and builders for the staged values they return.
   //!
   //! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

   use std::cell::RefCell;
   use std::collections::BTreeMap;

   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Result, StagedDeclaration, StagedFile, StagedItem,
       StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
   };

   /// What the fake holds for one directory.
   #[derive(Debug, Clone)]
   pub enum FakeFile {
       /// A build file that evaluates to these staged values.
       Staged(StagedFile),
       /// A build file whose evaluation fails this way.
       Fails(EvaluationFailure),
   }

   /// A build-file source holding fixed files per directory, recording every evaluation.
   ///
   /// A directory it holds no file for evaluates to [`Evaluated::Absent`].
   #[derive(Debug)]
   pub struct FakeSource {
       files: BTreeMap<Directory, FakeFile>,
       evaluated: RefCell<Vec<Directory>>,
   }

   impl FakeSource {
       /// A source returning the same files on every evaluation.
       #[must_use]
       pub const fn new(files: BTreeMap<Directory, FakeFile>) -> Self {
           Self {
               files,
               evaluated: RefCell::new(Vec::new()),
           }
       }

       /// Every directory evaluated so far, in evaluation order.
       #[must_use]
       pub fn evaluated(&self) -> Vec<Directory> {
           self.evaluated.borrow().clone()
       }
   }

   impl DeclarationSource for FakeSource {
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
           let directory = file.directory();
           self.evaluated.borrow_mut().push(directory.clone());
           match self.files.get(directory) {
               None => Ok(Evaluated::Absent),
               Some(FakeFile::Staged(staged)) => Ok(Evaluated::Staged(staged.clone())),
               Some(FakeFile::Fails(failure)) => Err(Error::Evaluation {
                   provenance: file.provenance().clone(),
                   failure: failure.clone(),
                   diagnostic: Diagnostic::new("fake evaluation failure"),
               }),
           }
       }
   }

   /// A directory from known-valid text.
   ///
   /// # Panics
   ///
   /// Panics when `raw` is not a valid directory; a test fixture is wrong then.
   #[must_use]
   pub fn dir(raw: &str) -> Directory {
       Directory::parse(raw).unwrap_or_else(|error| unreachable!("fixture directory: {error}"))
   }

   /// A build file making `items` as its declaration calls, then `subdirs` as its `subdir` calls.
   ///
   /// Call order counts declarations first, then subdirs.
   #[must_use]
   pub fn file(items: Vec<StagedItem>, subdirs: &[&str]) -> FakeFile {
       let declarations: Vec<StagedDeclaration> = items
           .into_iter()
           .zip(0_u32..)
           .map(|(item, index)| StagedDeclaration {
               order: DeclarationOrder::new(index),
               item,
           })
           .collect();
       let first_subdir = u32::try_from(declarations.len()).unwrap_or(u32::MAX);
       let subdirs = subdirs
           .iter()
           .zip(first_subdir..)
           .map(|(path, index)| StagedSubdir {
               path: Written::new(*path),
               order: DeclarationOrder::new(index),
           })
           .collect();
       FakeFile::Staged(StagedFile {
           declarations,
           subdirs,
       })
   }

   /// A `b.target(name, { run = { "cc" }, deps = deps })` call.
   #[must_use]
   pub fn target(name: &str, deps: &[&str]) -> StagedItem {
       StagedItem::Target(StagedTarget {
           name: Written::new(name),
           role: TargetRole::Build,
           rule: None,
           run: Some(vec![Written::new("cc")]),
           inputs: Vec::new(),
           deps: deps.iter().map(|dep| Written::new(*dep)).collect(),
           outputs: None,
           env: Vec::new(),
           network: NetworkAccess::Sealed,
           freshness: Freshness::Cached,
       })
   }

   /// A `b.option(name, { default = "" })` call.
   #[must_use]
   pub fn setting(name: &str) -> StagedItem {
       StagedItem::Setting(StagedSetting {
           name: Written::new(name),
           default: Written::new(""),
       })
   }

   /// A workspace from `(directory, file)` pairs.
   #[must_use]
   pub fn workspace(files: Vec<(&str, FakeFile)>) -> BTreeMap<Directory, FakeFile> {
       files
           .into_iter()
           .map(|(directory, file)| (dir(directory), file))
           .collect()
   }
   ```

   `dir` uses `unwrap_or_else(|error| unreachable!(…))`, not `.unwrap()`: `common.rs` carries no
   `#![expect(clippy::unwrap_used)]`, and `unreachable!` is neither `unwrap_used` nor `panic`. The
   builders `file`, `target` and `setting` and `FakeSource::evaluated` have no caller until tasks 4
   and 11; that is gate-clean (see Context). This is the shape plan `07` extends — the single
   `files` map and no `FakePorts`.

5. Run and confirm green — one flow, `load::a_missing_root_names_no_requester`, in the new `flows`
   binary:

   ```
   $ cargo test -p buildl-core
   test result: ok. 119 passed; 0 failed; …
        Running tests/flows/main.rs (…)
   test result: ok. 1 passed; 0 failed; …
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `test(buildl-core): drive load through a public-API fake build-file source`.

---

## Task 3 — Resolve a file's `subdir` requests

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/load/traversal.rs`,
   replace the three `use` lines with the following, and add the `file_in` fixture after them:

   ```rust
       use std::path::{Path, PathBuf};

       use super::{build_file, resolve_subdirs};
       use crate::error::{DeclarationField, Error};
       use crate::types::{DeclarationOrder, Directory, EntryName, Provenance, StagedSubdir, Written};

       fn file_in(directory: &str) -> Provenance {
           let directory = Directory::parse(directory).unwrap();
           let path = if directory.is_root() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(format!("{directory}/build.lua"))
           };
           Provenance::new(path, directory)
       }
   ```

   Then append two tests after `the_build_file_is_the_entry_joined_under_its_directory`:

   ```rust
       #[test]
       fn subdirs_resolve_under_the_file_sorted_and_without_repeats() {
           let subdirs = ["zeta", "alpha", "zeta", "mid"].map(|raw| StagedSubdir {
               path: Written::new(raw),
               order: DeclarationOrder::new(0),
           });
           let resolved = resolve_subdirs(&subdirs, &file_in("lib")).unwrap();
           let rendered: Vec<&str> = resolved.iter().map(Directory::as_str).collect();
           assert_eq!(rendered, ["lib/alpha", "lib/mid", "lib/zeta"]);
       }

       #[test]
       fn a_subdir_that_leaves_the_workspace_is_an_invalid_declaration() {
           for raw in ["../x", "/x", ""] {
               let subdirs = [StagedSubdir {
                   path: Written::new(raw),
                   order: DeclarationOrder::new(7),
               }];
               match resolve_subdirs(&subdirs, &file_in("lib")).unwrap_err() {
                   Error::InvalidDeclaration {
                       provenance,
                       order,
                       field,
                       ..
                   } => {
                       assert_eq!(provenance, file_in("lib"));
                       assert_eq!(order, DeclarationOrder::new(7));
                       assert_eq!(field, DeclarationField::Subdir);
                   }
                   other => unreachable!("expected InvalidDeclaration, got {other:?}"),
               }
           }
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::resolve_subdirs`
     --> crates/buildl-core/src/load/traversal.rs:58:29
   ```

3. Replace the file's import block and `load` (everything between the module doc and
   `build_file`'s doc comment) with:

   ```rust
   use std::collections::VecDeque;
   use std::path::PathBuf;

   use crate::error::{DeclarationField, Error, Result};
   use crate::ports::DeclarationSource;
   use crate::types::{
       BuildFile, Declaration, DeclarationOrder, Directory, EntryName, Evaluated, Provenance,
       StagedSubdir,
   };

   /// Evaluates the workspace's build files, from the root along `subdir` requests.
   ///
   /// Their declarations are not converted yet: the result is always empty.
   ///
   /// # Errors
   ///
   /// Returns the first of:
   ///
   /// - whatever `source` returns for a build file it cannot evaluate;
   /// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
   /// - [`Error::InvalidDeclaration`] when a `subdir` would leave the workspace.
   pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let mut queue = VecDeque::from([(Directory::root(), None)]);

       while let Some((directory, requested_by)) = queue.pop_front() {
           let file = build_file(&directory, entry);
           let staged = match source.evaluate(&file)? {
               Evaluated::Staged(staged) => staged,
               Evaluated::Absent => {
                   return Err(Error::MissingBuildFile {
                       directory,
                       requested_by,
                   });
               }
           };
           let provenance = file.provenance();
           for next in resolve_subdirs(&staged.subdirs, provenance)? {
               queue.push_back((next, Some(provenance.clone())));
           }
       }

       Ok(Vec::new())
   }
   ```

   Then add, after `build_file` and before `#[cfg(test)]`:

   ```rust
   /// Where a staged value was written: the build file, and the call's position in it.
   struct Site<'a> {
       provenance: &'a Provenance,
       order: DeclarationOrder,
   }

   impl Site<'_> {
       /// The directory the build file is evaluated in.
       const fn directory(&self) -> &Directory {
           self.provenance.directory()
       }

       /// Wraps a grammar failure of `field` with this site.
       fn invalid(&self, field: DeclarationField) -> impl FnOnce(Error) -> Error + '_ {
           move |source| Error::InvalidDeclaration {
               provenance: self.provenance.clone(),
               order: self.order,
               field,
               source: Box::new(source),
           }
       }
   }

   /// Resolves a file's `subdir` requests under its directory, sorted and without repeats.
   ///
   /// Sorting here makes the breadth-first order independent of the order the requests were made in.
   fn resolve_subdirs(subdirs: &[StagedSubdir], provenance: &Provenance) -> Result<Vec<Directory>> {
       let mut resolved = Vec::with_capacity(subdirs.len());
       for subdir in subdirs {
           let site = Site {
               provenance,
               order: subdir.order,
           };
           let directory = subdir
               .path
               .under(site.directory())
               .map_err(site.invalid(DeclarationField::Subdir))?;
           resolved.push(directory);
       }
       resolved.sort();
       resolved.dedup();
       Ok(resolved)
   }
   ```

   The escape rule needs no code of its own: `Written<Directory>::under` joins `lib` and `../x`
   into `lib/../x`, and `Directory::parse` refuses the `..` segment; `/x` becomes `lib//x` (empty
   segment); `""` is refused by `under` itself. `Site` is the one place a grammar failure is
   wrapped with its file and call position; `Site::invalid` returns the closure `map_err` takes.
   Sorting per file (spec §4.2) is what makes the next task's requester rule hold. The queue has
   no seen-set yet — a directory requested from two files is evaluated twice; task 4 pins and
   fixes that.

4. Run and confirm green — two new tests, `subdirs_resolve_under_the_file_sorted_and_without_repeats`
   and `a_subdir_that_leaves_the_workspace_is_an_invalid_declaration`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 121 passed; 0 failed; …
   test result: ok. 1 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): follow a build file's subdir requests`.

---

## Task 4 — Evaluate each reached directory once

**Files:**
- Modify `crates/buildl-core/tests/flows/load.rs`
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing flows. Replace the whole of `crates/buildl-core/tests/flows/load.rs`:

   ```rust
   //! Load, driven through the build-file port by [`FakeSource`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::path::Path;

   use buildl_core::{DeclarationField, EntryName, Error, EvaluationFailure, EvaluationLimit, load};

   use crate::common::{FakeFile, FakeSource, dir, file, workspace};

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   #[test]
   fn a_directory_requested_twice_is_evaluated_once() {
       // The root reaches `a/x` directly and again through `a`. A cycle cannot be written at all: a
       // `subdir` path is relative to its own directory and cannot climb out of it.
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![], &["a", "a/x", "a"])),
           ("a", file(vec![], &["x"])),
           ("a/x", file(vec![], &[])),
       ]));
       load(&source, &entry()).unwrap();
       assert_eq!(source.evaluated(), [dir(""), dir("a"), dir("a/x")]);
   }

   #[test]
   fn a_subdir_cannot_leave_the_workspace() {
       for escape in ["../x", "/x"] {
           let source = FakeSource::new(workspace(vec![
               ("", file(vec![], &["lib"])),
               ("lib", file(vec![], &[escape])),
           ]));
           match load(&source, &entry()).unwrap_err() {
               Error::InvalidDeclaration {
                   provenance, field, ..
               } => {
                   assert_eq!(field, DeclarationField::Subdir);
                   assert_eq!(provenance.file(), Path::new("lib/build.lua"));
               }
               other => unreachable!("expected InvalidDeclaration, got {other:?}"),
           }
       }
   }

   #[test]
   fn a_missing_root_names_no_requester() {
       let source = FakeSource::new(workspace(vec![]));
       match load(&source, &entry()).unwrap_err() {
           Error::MissingBuildFile {
               directory,
               requested_by,
           } => {
               assert!(directory.is_root());
               assert!(requested_by.is_none());
           }
           other => unreachable!("expected MissingBuildFile, got {other:?}"),
       }
   }

   #[test]
   fn a_missing_subdir_names_the_file_that_requested_it() {
       let source = FakeSource::new(workspace(vec![("", file(vec![], &["lib"]))]));
       let error = load(&source, &entry()).unwrap_err();
       assert_eq!(
           error.to_string(),
           "no build file in //lib (requested by build.lua)"
       );
   }

   #[test]
   fn the_requester_of_a_shared_missing_directory_does_not_depend_on_call_order() {
       // The root and `a` both request `a/gone`; the root's calls come in either order.
       for root_calls in [["a", "a/gone"], ["a/gone", "a"]] {
           let source = FakeSource::new(workspace(vec![
               ("", file(vec![], &root_calls)),
               ("a", file(vec![], &["gone"])),
           ]));
           let error = load(&source, &entry()).unwrap_err();
           assert_eq!(
               error.to_string(),
               "no build file in //a/gone (requested by build.lua)"
           );
       }
   }

   #[test]
   fn an_evaluation_failure_passes_through_unchanged() {
       let failure = EvaluationFailure::LimitReached {
           limit: EvaluationLimit::Staging,
       };
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![], &["lib"])),
           ("lib", FakeFile::Fails(failure.clone())),
       ]));
       match load(&source, &entry()).unwrap_err() {
           Error::Evaluation {
               provenance,
               failure: found,
               ..
           } => {
               assert_eq!(found, failure);
               assert_eq!(provenance.file(), Path::new("lib/build.lua"));
           }
           other => unreachable!("expected Evaluation, got {other:?}"),
       }
   }
   ```

2. Run and confirm failure — the code compiles; one flow fails at runtime because `a/x` is
   evaluated once from the root and again from `a`:

   ```
   $ cargo test -p buildl-core --test flows
   test load::a_directory_requested_twice_is_evaluated_once ... FAILED
   assertion `left == right` failed
     left: [Directory(""), Directory("a"), Directory("a/x"), Directory("a/x")]
    right: [Directory(""), Directory("a"), Directory("a/x")]
   test result: FAILED. 5 passed; 1 failed; …
   ```

   The other five pass on arrival: they pin behaviour task 3 already built (the requester rule,
   escape refusal through the port, adapter errors passing through `?` unchanged).

3. In `crates/buildl-core/src/load/traversal.rs`, change the first import to
   `use std::collections::{BTreeSet, VecDeque};` and replace `load` (doc comment included) with:

   ```rust
   /// Evaluates the workspace's build files, from the root along `subdir` requests.
   ///
   /// The walk is breadth-first and evaluates each directory once. Nothing is discovered from the
   /// filesystem: the evaluated build files are exactly the root's and those a `subdir` call
   /// reaches. Their declarations are not converted yet: the result is always empty.
   ///
   /// # Errors
   ///
   /// Returns the first of:
   ///
   /// - whatever `source` returns for a build file it cannot evaluate;
   /// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
   /// - [`Error::InvalidDeclaration`] when a `subdir` would leave the workspace.
   pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let mut queue = VecDeque::from([(Directory::root(), None)]);
       let mut seen = BTreeSet::from([Directory::root()]);

       while let Some((directory, requested_by)) = queue.pop_front() {
           let file = build_file(&directory, entry);
           let staged = match source.evaluate(&file)? {
               Evaluated::Staged(staged) => staged,
               Evaluated::Absent => {
                   return Err(Error::MissingBuildFile {
                       directory,
                       requested_by,
                   });
               }
           };
           let provenance = file.provenance();
           for next in resolve_subdirs(&staged.subdirs, provenance)? {
               if seen.insert(next.clone()) {
                   queue.push_back((next, Some(provenance.clone())));
               }
           }
       }

       Ok(Vec::new())
   }
   ```

   `seen` is a `BTreeSet`, never a `HashSet`: it is not iterated today, but the determinism rules
   keep hash ordering out of every phase. The first requester to reach a directory is the one
   `MissingBuildFile` names; breadth-first order plus the per-file sort of task 3 make that the
   same file on every run.

4. Run and confirm green — five new flows, `a_directory_requested_twice_is_evaluated_once`,
   `a_subdir_cannot_leave_the_workspace`, `a_missing_subdir_names_the_file_that_requested_it`,
   `the_requester_of_a_shared_missing_directory_does_not_depend_on_call_order` and
   `an_evaluation_failure_passes_through_unchanged`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 121 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): evaluate each requested directory once`.

---

## Task 5 — Convert rules, aliases and settings

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing tests. In the test module, replace the `use super::…`, `use crate::error::…`
   and `use crate::types::…` lines with:

   ```rust
       use super::{build_file, convert, resolve_subdirs};
       use crate::error::{DeclarationField, Error};
       use crate::types::{
           Declaration, DeclarationOrder, Declared, Directory, EntryName, Freshness, NetworkAccess,
           Provenance, StagedAlias, StagedDeclaration, StagedItem, StagedRule, StagedSetting,
           StagedSubdir, StagedTarget, TargetRole, Written,
       };
   ```

   Add these fixtures after `file_in`:

   ```rust
       fn staged_target(name: &str) -> StagedTarget {
           StagedTarget {
               name: Written::new(name),
               role: TargetRole::Build,
               rule: None,
               run: Some(vec![Written::new("cc")]),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: None,
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           }
       }

       fn declare(item: StagedItem) -> StagedDeclaration {
           StagedDeclaration {
               order: DeclarationOrder::new(4),
               item,
           }
       }

       fn convert_target_in(directory: &str, target: StagedTarget) -> crate::Result<Declaration> {
           convert(declare(StagedItem::Target(target)), &file_in(directory))
       }

       fn invalid_field(result: crate::Result<Declaration>) -> DeclarationField {
           match result.unwrap_err() {
               Error::InvalidDeclaration { field, order, .. } => {
                   assert_eq!(order, DeclarationOrder::new(4));
                   field
               }
               other => unreachable!("expected InvalidDeclaration, got {other:?}"),
           }
       }
   ```

   And append these tests after `a_subdir_that_leaves_the_workspace_is_an_invalid_declaration`:

   ```rust
       #[test]
       fn an_empty_run_is_an_invalid_command() {
           let mut target = staged_target("app");
           target.run = Some(Vec::new());
           let err = convert_target_in("", target).unwrap_err();
           assert!(
               err.to_string()
                   .ends_with(r#"invalid run: invalid command: "" — must hold at least one argument"#),
               "{err}"
           );
       }

       #[test]
       fn a_name_cannot_leave_its_directory() {
           let target = staged_target("//other:x");
           assert_eq!(
               invalid_field(convert_target_in("", target)),
               DeclarationField::Name
           );
       }

       #[test]
       fn rules_aliases_and_settings_convert_and_name_their_invalid_fields() {
           let file = file_in("");
           let rule = StagedRule {
               name: Written::new("cc"),
               run: vec![Written::new("cc"), Written::new("$in")],
               description: Some(Written::new("compile $in")),
           };
           let converted = convert(declare(StagedItem::Rule(rule.clone())), &file).unwrap();
           assert_eq!(converted.item().name(), "cc");

           let mut bad_description = rule;
           bad_description.description = Some(Written::new("two\nlines"));
           assert_eq!(
               invalid_field(convert(declare(StagedItem::Rule(bad_description)), &file)),
               DeclarationField::Description
           );

           let alias = StagedAlias {
               name: Written::new("default"),
               target: Written::new("app"),
           };
           let converted = convert(declare(StagedItem::Alias(alias)), &file).unwrap();
           match converted.item() {
               Declared::Alias(alias) => assert_eq!(alias.target.to_string(), "//:app"),
               other => unreachable!("expected an alias, got {other:?}"),
           }
           let bad_alias = StagedAlias {
               name: Written::new("default"),
               target: Written::new("a/b"),
           };
           assert_eq!(
               invalid_field(convert(declare(StagedItem::Alias(bad_alias)), &file)),
               DeclarationField::Target
           );

           let setting = StagedSetting {
               name: Written::new("test_filter"),
               default: Written::new(""),
           };
           assert!(convert(declare(StagedItem::Setting(setting)), &file).is_ok());
           let bad_name = StagedSetting {
               name: Written::new("a.b"),
               default: Written::new(""),
           };
           assert_eq!(
               invalid_field(convert(declare(StagedItem::Setting(bad_name)), &file)),
               DeclarationField::SettingName
           );
           let bad_value = StagedSetting {
               name: Written::new("x"),
               default: Written::new("a\0b"),
           };
           assert_eq!(
               invalid_field(convert(declare(StagedItem::Setting(bad_value)), &file)),
               DeclarationField::SettingValue
           );
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::convert`
      --> crates/buildl-core/src/load/traversal.rs:126:29
   ```

3. Replace the `use crate::types::{…};` import at the top of the file with:

   ```rust
   use crate::types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Directory, EntryName, Evaluated, Label, Provenance, Rule, Setting, StagedAlias,
       StagedDeclaration, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, Target,
       Written,
   };
   ```

   Replace `load` (doc comment included) with:

   ```rust
   /// Evaluates the workspace's build files and returns their declarations, validated.
   ///
   /// The walk starts at the workspace root and follows `subdir` requests breadth-first, evaluating
   /// each directory once. Nothing is discovered from the filesystem: the evaluated build files are
   /// exactly the root's and those a `subdir` call reaches.
   ///
   /// # Errors
   ///
   /// Returns the first of:
   ///
   /// - whatever `source` returns for a build file it cannot evaluate;
   /// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
   /// - [`Error::InvalidDeclaration`] when a staged value fails its grammar, including a `subdir`
   ///   that would leave the workspace.
   pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let mut queue = VecDeque::from([(Directory::root(), None)]);
       let mut seen = BTreeSet::from([Directory::root()]);
       let mut declarations = Vec::new();

       while let Some((directory, requested_by)) = queue.pop_front() {
           let file = build_file(&directory, entry);
           let staged = match source.evaluate(&file)? {
               Evaluated::Staged(staged) => staged,
               Evaluated::Absent => {
                   return Err(Error::MissingBuildFile {
                       directory,
                       requested_by,
                   });
               }
           };
           let provenance = file.provenance();
           for next in resolve_subdirs(&staged.subdirs, provenance)? {
               if seen.insert(next.clone()) {
                   queue.push_back((next, Some(provenance.clone())));
               }
           }
           for declaration in staged.declarations {
               declarations.push(convert(declaration, provenance)?);
           }
       }

       Ok(declarations)
   }
   ```

   Then add, after `resolve_subdirs` and before `#[cfg(test)]`:

   ```rust
   /// Converts one staged declaration into a validated one.
   fn convert(staged: StagedDeclaration, provenance: &Provenance) -> Result<Declaration> {
       let site = Site {
           provenance,
           order: staged.order,
       };
       let item = match staged.item {
           StagedItem::Target(target) => Declared::Target(convert_target(target, &site)?),
           StagedItem::Rule(rule) => Declared::Rule(convert_rule(rule, &site)?),
           StagedItem::Alias(alias) => Declared::Alias(convert_alias(&alias, &site)?),
           StagedItem::Setting(setting) => Declared::Setting(convert_setting(&setting, &site)?),
       };
       Ok(Declaration::new(provenance.clone(), item))
   }

   /// The label a declared name gets: always in the declaring directory.
   fn declared_label(name: &Written<crate::types::TargetName>, site: &Site<'_>) -> Result<Label> {
       let name = name.parse().map_err(site.invalid(DeclarationField::Name))?;
       Ok(Label::new(site.directory().clone(), name))
   }

   /// A `run` list, every argument validated and the list non-empty.
   fn convert_command(run: &[Written<Argument>], site: &Site<'_>) -> Result<Command> {
       let mut arguments = Vec::with_capacity(run.len());
       for argument in run {
           arguments.push(
               argument
                   .parse()
                   .map_err(site.invalid(DeclarationField::Run))?,
           );
       }
       Command::new(arguments).map_err(site.invalid(DeclarationField::Run))
   }

   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = Action::Run(convert_command(&staged.run.unwrap_or_default(), site)?);
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs: Vec::new(),
           deps: Vec::new(),
           outputs: Vec::new(),
           env: Vec::new(),
           network: staged.network,
           freshness: staged.freshness,
       })
   }

   fn convert_rule(staged: StagedRule, site: &Site<'_>) -> Result<Rule> {
       let label = declared_label(&staged.name, site)?;
       let run = convert_command(&staged.run, site)?;
       let description = match staged.description {
           Some(description) => Some(
               description
                   .parse()
                   .map_err(site.invalid(DeclarationField::Description))?,
           ),
           None => None,
       };
       Ok(Rule {
           label,
           run,
           description,
       })
   }

   fn convert_alias(staged: &StagedAlias, site: &Site<'_>) -> Result<Alias> {
       let label = declared_label(&staged.name, site)?;
       let target = staged
           .target
           .resolve(site.directory())
           .map_err(site.invalid(DeclarationField::Target))?;
       Ok(Alias { label, target })
   }

   fn convert_setting(staged: &StagedSetting, site: &Site<'_>) -> Result<Setting> {
       let name = staged
           .name
           .parse()
           .map_err(site.invalid(DeclarationField::SettingName))?;
       let default = staged
           .default
           .parse()
           .map_err(site.invalid(DeclarationField::SettingValue))?;
       Ok(Setting { name, default })
   }
   ```

   `convert_target` is deliberately minimal: `convert`'s match must cover all four kinds, so the
   target arm exists now, but it only names the target and reads `run` — `rule`, `inputs`,
   `deps`, `outputs` and `env` are ignored. Tasks 6–10 each replace it with a version that handles
   one more field, behind a test that fails against this one. A declared name is a
   `Written<TargetName>` turned into `Label::new(declaring directory, name)`, so `//other:x`
   fails `TargetName`'s grammar as `field: Name` — a declaration cannot leave its directory. An
   empty `run` passes every argument check and then fails `Command::new`, wrapped as `field: Run`.
   `convert_alias` and `convert_setting` borrow their staged value; `convert_target` and
   `convert_rule` take it by value because they move `Option`s out of it.

4. Run and confirm green — three new tests, `an_empty_run_is_an_invalid_command`,
   `a_name_cannot_leave_its_directory` and
   `rules_aliases_and_settings_convert_and_name_their_invalid_fields`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 124 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): convert staged rules, aliases and settings`.

---

## Task 6 — Require exactly one of `rule` and `run`

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing tests. In the test module, replace the `use crate::error::…` and
   `use crate::types::…` lines with:

   ```rust
       use crate::error::{ActionFound, DeclarationField, Error};
       use crate::types::{
           Action, Declaration, DeclarationOrder, Declared, Directory, EntryName, Freshness,
           NetworkAccess, Provenance, StagedAlias, StagedDeclaration, StagedItem, StagedRule,
           StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
       };
   ```

   Add this fixture after `invalid_field`:

   ```rust
       fn target_of(declaration: &Declaration) -> &crate::types::Target {
           match declaration.item() {
               Declared::Target(target) => target,
               other => unreachable!("expected a target, got {other:?}"),
           }
       }
   ```

   And insert these tests immediately before `an_empty_run_is_an_invalid_command`:

   ```rust
       #[test]
       fn a_rule_reference_resolves_against_the_declaring_directory() {
           let mut target = staged_target("main.o");
           target.run = None;
           target.rule = Some(Written::new("cc"));
           let declaration = convert_target_in("app", target).unwrap();
           assert_eq!(
               target_of(&declaration).action,
               Action::UseRule(crate::types::Label::parse("//app:cc").unwrap())
           );
       }

       #[test]
       fn a_target_needs_exactly_one_of_rule_and_run() {
           let mut both = staged_target("app");
           both.rule = Some(Written::new("cc"));
           let mut neither = staged_target("app");
           neither.run = None;
           for (target, expected) in [(both, ActionFound::Both), (neither, ActionFound::Neither)] {
               match convert_target_in("", target).unwrap_err() {
                   Error::ActionConflict { found, order, .. } => {
                       assert_eq!(found, expected);
                       assert_eq!(order, DeclarationOrder::new(4));
                   }
                   other => unreachable!("expected ActionConflict, got {other:?}"),
               }
           }
       }
   ```

2. Run and confirm failure — both fail at runtime against task 5's `run`-only target:

   ```
   $ cargo test -p buildl-core --lib load::
   test load::traversal::tests::a_rule_reference_resolves_against_the_declaring_directory ... FAILED
   test load::traversal::tests::a_target_needs_exactly_one_of_rule_and_run ... FAILED
   called `Result::unwrap()` on an `Err` value: InvalidDeclaration { …, field: Run, source: InvalidName { kind: Command, value: "", reason: "must hold at least one argument" } }
   called `Result::unwrap_err()` on an `Ok` value: Declaration { … action: Run(Command([Argument("cc")])), … }
   test result: FAILED. 6 passed; 2 failed; …
   ```

3. Change the error import at the top of the file to:

   ```rust
   use crate::error::{ActionFound, DeclarationField, Error, Result};
   ```

   Replace the `# Errors` list's last bullet of `load`'s doc comment, so the section reads:

   ```rust
   /// # Errors
   ///
   /// Returns the first of:
   ///
   /// - whatever `source` returns for a build file it cannot evaluate;
   /// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
   /// - [`Error::InvalidDeclaration`] when a staged value fails its grammar, including a `subdir`
   ///   that would leave the workspace;
   /// - [`Error::ActionConflict`] when a target names both or neither of `rule` and `run`.
   ```

   Add this method to `impl Site<'_>`, after `invalid`:

   ```rust
       /// A target at this site found `found` instead of exactly one action.
       fn conflict(&self, found: ActionFound) -> Error {
           Error::ActionConflict {
               provenance: self.provenance.clone(),
               order: self.order,
               found,
           }
       }
   ```

   And replace `convert_target` with:

   ```rust
   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = match (staged.rule, staged.run) {
           (Some(rule), None) => Action::UseRule(
               rule.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Rule))?,
           ),
           (None, Some(run)) => Action::Run(convert_command(&run, site)?),
           (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
           (None, None) => return Err(site.conflict(ActionFound::Neither)),
       };
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs: Vec::new(),
           deps: Vec::new(),
           outputs: Vec::new(),
           env: Vec::new(),
           network: staged.network,
           freshness: staged.freshness,
       })
   }
   ```

   A `rule` reference resolves like a dep (`Written<Label>::resolve` against the declaring
   directory), so `"cc"` in `//app` names `//app:cc` and `"//tools:cc"` names another directory's
   rule. Whether that label names a rule at all is the graph phase's question, not Load's.

4. Run and confirm green — two new tests, `a_rule_reference_resolves_against_the_declaring_directory`
   and `a_target_needs_exactly_one_of_rule_and_run`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 126 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): require exactly one of rule and run on a target`.

---

## Task 7 — Resolve a target's deps against its directory

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing test. Insert it immediately before
   `a_rule_reference_resolves_against_the_declaring_directory`:

   ```rust
       #[test]
       fn a_target_is_named_in_its_directory_and_resolves_every_dep_form() {
           let mut target = staged_target("app");
           target.deps = ["main.o", ":util.o", "//lib:text"]
               .map(Written::new)
               .to_vec();
           let declaration = convert_target_in("app", target).unwrap();
           assert_eq!(declaration.provenance(), &file_in("app"));
           let target = target_of(&declaration);
           assert_eq!(target.label.to_string(), "//app:app");
           let deps: Vec<String> = target.deps.iter().map(ToString::to_string).collect();
           assert_eq!(deps, ["//app:main.o", "//app:util.o", "//lib:text"]);
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib load::
   test load::traversal::tests::a_target_is_named_in_its_directory_and_resolves_every_dep_form ... FAILED
   assertion `left == right` failed
     left: []
    right: ["//app:main.o", "//app:util.o", "//lib:text"]
   test result: FAILED. 8 passed; 1 failed; …
   ```

3. Replace `convert_target` with:

   ```rust
   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = match (staged.rule, staged.run) {
           (Some(rule), None) => Action::UseRule(
               rule.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Rule))?,
           ),
           (None, Some(run)) => Action::Run(convert_command(&run, site)?),
           (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
           (None, None) => return Err(site.conflict(ActionFound::Neither)),
       };
       let mut deps = Vec::with_capacity(staged.deps.len());
       for dep in &staged.deps {
           deps.push(
               dep.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Dep))?,
           );
       }
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs: Vec::new(),
           deps,
           outputs: Vec::new(),
           env: Vec::new(),
           network: staged.network,
           freshness: staged.freshness,
       })
   }
   ```

   Deps become `Label`s here, at Load (spec D2): the three reference forms of a build file —
   bare, `:sibling`, absolute — resolve against the declaring directory, so `Declaration` never
   holds an unresolved reference.

4. Run and confirm green — one new test,
   `a_target_is_named_in_its_directory_and_resolves_every_dep_form`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 127 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): resolve a target's deps against its directory`.

---

## Task 8 — Join a target's inputs under its directory

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing test. Insert it immediately before
   `a_rule_reference_resolves_against_the_declaring_directory`:

   ```rust
       #[test]
       fn inputs_join_under_the_declaring_directory() {
           let mut target = staged_target("app");
           target.inputs = vec![Written::new("src/main.c")];
           let declaration = convert_target_in("app", target).unwrap();
           assert_eq!(target_of(&declaration).inputs[0].as_str(), "app/src/main.c");
           let mut at_root = staged_target("app");
           at_root.inputs = vec![Written::new("main.c")];
           let declaration = convert_target_in("", at_root).unwrap();
           assert_eq!(target_of(&declaration).inputs[0].as_str(), "main.c");
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib load::
   test load::traversal::tests::inputs_join_under_the_declaring_directory ... FAILED
   index out of bounds: the len is 0 but the index is 0
   test result: FAILED. 9 passed; 1 failed; …
   ```

3. Replace the `use crate::types::{…};` import at the top of the file with:

   ```rust
   use crate::types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Directory, EntryName, Evaluated, Label, Provenance, Rule, Setting, SourcePath, StagedAlias,
       StagedDeclaration, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, Target,
       Written,
   };
   ```

   Add, after `convert_command` and before `convert_target`:

   ```rust
   /// An input, joined under the declaring directory and validated as a workspace path.
   fn convert_input(input: &Written<SourcePath>, site: &Site<'_>) -> Result<SourcePath> {
       let directory = site.directory();
       let joined = if directory.is_root() {
           input.as_written().to_owned()
       } else {
           format!("{directory}/{}", input.as_written())
       };
       SourcePath::parse(joined).map_err(site.invalid(DeclarationField::Input))
   }
   ```

   And replace `convert_target` with:

   ```rust
   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = match (staged.rule, staged.run) {
           (Some(rule), None) => Action::UseRule(
               rule.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Rule))?,
           ),
           (None, Some(run)) => Action::Run(convert_command(&run, site)?),
           (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
           (None, None) => return Err(site.conflict(ActionFound::Neither)),
       };
       let mut inputs = Vec::with_capacity(staged.inputs.len());
       for input in &staged.inputs {
           inputs.push(convert_input(input, site)?);
       }
       let mut deps = Vec::with_capacity(staged.deps.len());
       for dep in &staged.deps {
           deps.push(
               dep.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Dep))?,
           );
       }
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs,
           deps,
           outputs: Vec::new(),
           env: Vec::new(),
           network: staged.network,
           freshness: staged.freshness,
       })
   }
   ```

   The join lives here, in Load, not on `Written<SourcePath>`: it combines a `Directory` with a
   `SourcePath`, two different concepts, which the crate rules keep out of `types/`. The joined
   text goes through `SourcePath::parse`, so `../x` under the root fails the `..` rule as
   `field: Input`.

4. Run and confirm green — one new test, `inputs_join_under_the_declaring_directory`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 128 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): join a target's inputs under its directory`.

---

## Task 9 — Default a target's outputs to its name

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing test. Insert it immediately before
   `inputs_join_under_the_declaring_directory`:

   ```rust
       #[test]
       fn outputs_default_to_the_target_name() {
           let declaration = convert_target_in("app", staged_target("app")).unwrap();
           let outputs: Vec<&str> = target_of(&declaration)
               .outputs
               .iter()
               .map(crate::types::OutputName::as_str)
               .collect();
           assert_eq!(outputs, ["app"]);
       }
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib load::
   test load::traversal::tests::outputs_default_to_the_target_name ... FAILED
   assertion `left == right` failed
     left: []
    right: ["app"]
   test result: FAILED. 10 passed; 1 failed; …
   ```

3. Replace the `use crate::types::{…};` import at the top of the file with:

   ```rust
   use crate::types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Directory, EntryName, Evaluated, Label, OutputName, Provenance, Rule, Setting, SourcePath,
       StagedAlias, StagedDeclaration, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, Target, Written,
   };
   ```

   And replace `convert_target` with:

   ```rust
   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = match (staged.rule, staged.run) {
           (Some(rule), None) => Action::UseRule(
               rule.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Rule))?,
           ),
           (None, Some(run)) => Action::Run(convert_command(&run, site)?),
           (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
           (None, None) => return Err(site.conflict(ActionFound::Neither)),
       };
       let mut inputs = Vec::with_capacity(staged.inputs.len());
       for input in &staged.inputs {
           inputs.push(convert_input(input, site)?);
       }
       let mut deps = Vec::with_capacity(staged.deps.len());
       for dep in &staged.deps {
           deps.push(
               dep.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Dep))?,
           );
       }
       let outputs = match staged.outputs {
           Some(written) => {
               let mut outputs = Vec::with_capacity(written.len());
               for output in &written {
                   outputs.push(
                       output
                           .parse()
                           .map_err(site.invalid(DeclarationField::Output))?,
                   );
               }
               outputs
           }
           None => vec![
               OutputName::parse(label.name().as_str())
                   .map_err(site.invalid(DeclarationField::Output))?,
           ],
       };
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs,
           deps,
           outputs,
           env: Vec::new(),
           network: staged.network,
           freshness: staged.freshness,
       })
   }
   ```

   An output is relative to the target's output directory, so it is parsed as written, never
   joined under the declaring directory. The default goes through `OutputName::parse` like any
   other output rather than being assumed valid: every `TargetName` is a valid one-segment
   `OutputName` today, and the parse keeps that a checked fact if either grammar changes.

4. Run and confirm green — one new test, `outputs_default_to_the_target_name`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 129 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): default a target's outputs to its name`.

---

## Task 10 — Validate a target's env names

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing test. Insert it immediately before
   `rules_aliases_and_settings_convert_and_name_their_invalid_fields`:

   ```rust
       #[test]
       fn each_invalid_target_field_is_named() {
           let mut dep = staged_target("app");
           dep.deps = vec![Written::new("//lib")];
           assert_eq!(
               invalid_field(convert_target_in("", dep)),
               DeclarationField::Dep
           );

           let mut input = staged_target("app");
           input.inputs = vec![Written::new("../x")];
           assert_eq!(
               invalid_field(convert_target_in("", input)),
               DeclarationField::Input
           );

           let mut output = staged_target("app");
           output.outputs = Some(vec![Written::new("/abs")]);
           assert_eq!(
               invalid_field(convert_target_in("", output)),
               DeclarationField::Output
           );

           let mut env = staged_target("app");
           env.env = vec![Written::new("1X")];
           assert_eq!(
               invalid_field(convert_target_in("", env)),
               DeclarationField::Env
           );

           let mut run = staged_target("app");
           run.run = Some(vec![Written::new("a\0b")]);
           assert_eq!(
               invalid_field(convert_target_in("", run)),
               DeclarationField::Run
           );

           let mut rule = staged_target("app");
           rule.run = None;
           rule.rule = Some(Written::new("//tools"));
           assert_eq!(
               invalid_field(convert_target_in("", rule)),
               DeclarationField::Rule
           );
       }
   ```

2. Run and confirm failure — the `Dep`, `Input` and `Output` cases pass; the `Env` case finds no
   error because `env` is still ignored:

   ```
   $ cargo test -p buildl-core --lib load::
   test load::traversal::tests::each_invalid_target_field_is_named ... FAILED
   called `Result::unwrap_err()` on an `Ok` value: Declaration { …, outputs: [OutputName("app")], env: [], network: Sealed, freshness: Cached }) }
   test result: FAILED. 11 passed; 1 failed; …
   ```

3. Replace `convert_target` with its final form:

   ```rust
   fn convert_target(staged: StagedTarget, site: &Site<'_>) -> Result<Target> {
       let label = declared_label(&staged.name, site)?;
       let action = match (staged.rule, staged.run) {
           (Some(rule), None) => Action::UseRule(
               rule.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Rule))?,
           ),
           (None, Some(run)) => Action::Run(convert_command(&run, site)?),
           (Some(_), Some(_)) => return Err(site.conflict(ActionFound::Both)),
           (None, None) => return Err(site.conflict(ActionFound::Neither)),
       };
       let mut inputs = Vec::with_capacity(staged.inputs.len());
       for input in &staged.inputs {
           inputs.push(convert_input(input, site)?);
       }
       let mut deps = Vec::with_capacity(staged.deps.len());
       for dep in &staged.deps {
           deps.push(
               dep.resolve(site.directory())
                   .map_err(site.invalid(DeclarationField::Dep))?,
           );
       }
       let outputs = match staged.outputs {
           Some(written) => {
               let mut outputs = Vec::with_capacity(written.len());
               for output in &written {
                   outputs.push(
                       output
                           .parse()
                           .map_err(site.invalid(DeclarationField::Output))?,
                   );
               }
               outputs
           }
           None => vec![
               OutputName::parse(label.name().as_str())
                   .map_err(site.invalid(DeclarationField::Output))?,
           ],
       };
       let mut env = Vec::with_capacity(staged.env.len());
       for name in &staged.env {
           env.push(name.parse().map_err(site.invalid(DeclarationField::Env))?);
       }
       Ok(Target {
           label,
           role: staged.role,
           action,
           inputs,
           deps,
           outputs,
           env,
           network: staged.network,
           freshness: staged.freshness,
       })
   }
   ```

   An env name is validated for its grammar only; whether the workspace ceiling grants it is a
   later phase's question.

4. Run and confirm green — one new test, `each_invalid_target_field_is_named`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 130 passed; 0 failed; …
   test result: ok. 6 passed; 0 failed; …
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): validate a target's env names`.

---

## Task 11 — Sort the merged declarations

**Files:**
- Modify `crates/buildl-core/tests/flows/load.rs`
- Modify `crates/buildl-core/src/load/traversal.rs`

**Steps:**

1. Write the failing flows. In `crates/buildl-core/tests/flows/load.rs`, replace the two
   `buildl_core`/`crate::common` imports with:

   ```rust
   use buildl_core::{
       DeclarationField, Declared, EntryName, Error, EvaluationFailure, EvaluationLimit, json, load,
   };

   use crate::common::{FakeFile, FakeSource, dir, file, setting, target, workspace};
   ```

   Then insert, between `fn entry()` and `a_directory_requested_twice_is_evaluated_once`:

   ```rust
   fn names(source: &FakeSource) -> Vec<String> {
       load(source, &entry())
           .unwrap()
           .iter()
           .map(|declaration| match declaration.item() {
               Declared::Target(target) => target.label.to_string(),
               Declared::Setting(setting) => format!("setting {}", setting.name),
               other => format!("{other:?}"),
           })
           .collect()
   }

   #[test]
   fn every_reached_file_yields_one_sorted_list_of_declarations() {
       let source = FakeSource::new(workspace(vec![
           (
               "",
               file(vec![target("app", &["main.o", "//lib:text"])], &["lib"]),
           ),
           ("lib", file(vec![target("text", &[])], &[])),
       ]));
       assert_eq!(names(&source), ["//:app", "//lib:text"]);
   }

   #[test]
   fn subdirs_requested_out_of_order_still_sort_by_directory_then_name() {
       let source = FakeSource::new(workspace(vec![
           (
               "",
               file(vec![target("z", &[]), target("a", &[])], &["zeta", "alpha"]),
           ),
           ("zeta", file(vec![target("m", &[])], &[])),
           ("alpha", file(vec![setting("b"), target("a", &[])], &[])),
       ]));
       assert_eq!(
           names(&source),
           ["//:a", "//:z", "//alpha:a", "setting b", "//zeta:m"]
       );
   }

   #[test]
   fn a_shuffled_file_loads_to_byte_identical_canonical_json() {
       let ordered = FakeSource::new(workspace(vec![
           (
               "",
               file(
                   vec![target("a", &[]), target("b", &["a"]), setting("s")],
                   &["x", "y"],
               ),
           ),
           ("x", file(vec![], &[])),
           ("y", file(vec![], &[])),
       ]));
       let shuffled = FakeSource::new(workspace(vec![
           (
               "",
               file(
                   vec![setting("s"), target("b", &["a"]), target("a", &[])],
                   &["y", "x"],
               ),
           ),
           ("x", file(vec![], &[])),
           ("y", file(vec![], &[])),
       ]));
       let a = json::canonical::to_vec(&load(&ordered, &entry()).unwrap()).unwrap();
       let b = json::canonical::to_vec(&load(&shuffled, &entry()).unwrap()).unwrap();
       assert_eq!(a, b);
   }
   ```

   The byte comparison goes through `json::canonical::to_vec`, the one serializer every byte of
   output uses, so it asserts the property a cache key will rely on, not just `PartialEq`.

2. Run and confirm the flows fail at runtime — Load still returns declarations in evaluation
   order:

   ```
   $ cargo test -p buildl-core --test flows
   test load::subdirs_requested_out_of_order_still_sort_by_directory_then_name ... FAILED
   test load::a_shuffled_file_loads_to_byte_identical_canonical_json ... FAILED
   assertion `left == right` failed
     left: ["//:z", "//:a", "setting b", "//alpha:a", "//zeta:m"]
    right: ["//:a", "//:z", "//alpha:a", "setting b", "//zeta:m"]
   test result: FAILED. 7 passed; 2 failed; …
   ```

   (`every_reached_file_yields_one_sorted_list_of_declarations` passes on arrival: its input is
   already in order. It pins the whole-workspace flow, not the sort.)

3. Write the failing unit test. In the test module of `crates/buildl-core/src/load/traversal.rs`,
   change `use super::{build_file, convert, resolve_subdirs};` to:

   ```rust
       use super::{build_file, convert, resolve_subdirs, sort};
   ```

   and append, as the module's last test:

   ```rust
       #[test]
       fn sorting_orders_by_directory_then_name_then_value() {
           let declare_in = |directory: &str, name: &str| {
               convert_target_in(directory, staged_target(name)).unwrap()
           };
           let mut declarations = vec![
               declare_in("lib", "a"),
               declare_in("", "zeta"),
               declare_in("", "alpha"),
               declare_in("app", "m"),
           ];
           sort(&mut declarations);
           let rendered: Vec<String> = declarations
               .iter()
               .map(|d| target_of(d).label.to_string())
               .collect();
           assert_eq!(rendered, ["//:alpha", "//:zeta", "//app:m", "//lib:a"]);
       }
   ```

4. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::sort`
   ```

5. Replace `load` (doc comment included) with its final form:

   ```rust
   /// Evaluates the workspace's build files and returns their declarations, validated and sorted.
   ///
   /// The walk starts at the workspace root and follows `subdir` requests breadth-first, evaluating
   /// each directory once. Nothing is discovered from the filesystem: the evaluated build files are
   /// exactly the root's and those a `subdir` call reaches. The result is sorted by declaring
   /// directory, then declared name, then the whole declaration, so it does not depend on the order
   /// anything was evaluated or declared in.
   ///
   /// # Errors
   ///
   /// Returns the first of:
   ///
   /// - whatever `source` returns for a build file it cannot evaluate;
   /// - [`Error::MissingBuildFile`] when the root or a requested directory holds no build file;
   /// - [`Error::InvalidDeclaration`] when a staged value fails its grammar, including a `subdir`
   ///   that would leave the workspace;
   /// - [`Error::ActionConflict`] when a target names both or neither of `rule` and `run`.
   pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
       let mut queue = VecDeque::from([(Directory::root(), None)]);
       let mut seen = BTreeSet::from([Directory::root()]);
       let mut declarations = Vec::new();

       while let Some((directory, requested_by)) = queue.pop_front() {
           let file = build_file(&directory, entry);
           let staged = match source.evaluate(&file)? {
               Evaluated::Staged(staged) => staged,
               Evaluated::Absent => {
                   return Err(Error::MissingBuildFile {
                       directory,
                       requested_by,
                   });
               }
           };
           let provenance = file.provenance();
           for next in resolve_subdirs(&staged.subdirs, provenance)? {
               if seen.insert(next.clone()) {
                   queue.push_back((next, Some(provenance.clone())));
               }
           }
           for declaration in staged.declarations {
               declarations.push(convert(declaration, provenance)?);
           }
       }

       sort(&mut declarations);
       Ok(declarations)
   }
   ```

   And add, after `convert_setting` and before `#[cfg(test)]`:

   ```rust
   /// Sorts by declaring directory, then declared name as bytes, then the whole declaration.
   ///
   /// The last key makes the order total without the order calls were made in, which a `pairs`
   /// loop changes from one evaluation to the next.
   fn sort(declarations: &mut [Declaration]) {
       declarations.sort_by(|a, b| {
           a.provenance()
               .directory()
               .cmp(b.provenance().directory())
               .then_with(|| a.item().name().cmp(b.item().name()))
               .then_with(|| a.cmp(b))
       });
   }
   ```

   `Declared::name()` returns `&str`, so a label's `TargetName` and a setting's `SettingName`
   compare as UTF-8 bytes on one axis — which is why `setting b` sorts after target `a` inside
   `//alpha` (spec §4.4). The tie-break `a.cmp(b)` is `Declaration`'s derived `Ord`; `Declaration`
   carries no declaration order, so two runs of a `pairs` loop sort identically.

6. Run and confirm green — one new unit test, `sorting_orders_by_directory_then_name_then_value`,
   and three new flows, `every_reached_file_yields_one_sorted_list_of_declarations`,
   `subdirs_requested_out_of_order_still_sort_by_directory_then_name` and
   `a_shuffled_file_loads_to_byte_identical_canonical_json`:

   ```
   $ cargo test -p buildl-core
   test result: ok. 131 passed; 0 failed; …
   test result: ok. 9 passed; 0 failed; …
   ```

7. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

8. Commit `feat(buildl-core): sort loaded declarations independent of evaluation order`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all
$ cargo make dod
$ cargo make guard-core-purity
$ cargo make guard-crate-edges
$ cargo deny check
```

All exit 0 (verified on this plan's end state: `cargo deny check` → `advisories ok, bans ok,
licenses ok, sources ok`). `crates/buildl-core/Cargo.toml` and `crates/expected-edges.txt` are
untouched — this plan adds no dependency.

End state:

| Item | State |
|---|---|
| `buildl_core::load` | public, re-exported at the crate root; `load/` names `ports` and `types` only, no other phase |
| `load/traversal.rs` | `load`, `build_file`, `Site`, `resolve_subdirs`, `convert` + `declared_label`, `convert_command`, `convert_input`, `convert_target`, `convert_rule`, `convert_alias`, `convert_setting`, `sort`; 13 unit tests |
| `tests/flows/` | one binary: `main.rs` (`pub mod common; mod load;`), `common.rs` (`FakeFile`, single-map `FakeSource`, `dir`/`file`/`target`/`setting`/`workspace`), `load.rs` (9 flows) |
| Test totals | 131 unit, 9 flows, 2 doctests (given 118 unit tests from plans `01`–`05`) |

Every load scenario of spec §8 has a test (table in Context). `grep -rn 'HashMap\|HashSet' crates/buildl-core/src/load`
returns nothing. Plan `07` adds `mod check;` to `main.rs`, `FakePorts` and the second-run map to
`common.rs`; plan `08` adds the `load` row to the crate doc's module map.
