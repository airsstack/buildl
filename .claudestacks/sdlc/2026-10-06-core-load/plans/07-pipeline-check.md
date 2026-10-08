---
status: done
created: 2026-10-06
depends-on: [06]
---

# Pipeline Check Implementation Plan

**Goal:** `Pipeline::check` returns a workspace's sorted declarations only when two loads through the same port agree.

**Architecture:** A new export-only `pipeline/` module in `crates/buildl-core/src/` whose one logic
file is `driver.rs` (not `pipeline/pipeline.rs`: clippy's `module_inception` fails the gate on a
module named after its parent). `Pipeline<P: Ports>` holds one value per port — today only
`P::Source` — and gains one method per command; `check` calls `load::load` twice and compares the two
sorted lists by value. The comparison lives in a private `first_difference`, unit-tested in the file,
which names the lesser of the first differing pair, because both lists are sorted and agree before
that index, so the lesser is the declaration the other run lacks. The flows through the port live in
the one integration-test binary `tests/flows/`, against public-API fakes; this plan adds the fakes
only `check.rs` needs (`FakePorts`, and `FakeSource`'s second-run mode).

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `thiserror` 2 (existing), `cargo-make`. No new
dependency.

**Content authority:** spec §5.2 (`Pipeline` and `check`), §5.3 (fakes), §7 (module layout), §8
(the two `check` scenarios), §10 (Definition of Done); decision D5.

---

## Context an implementer needs

Plans `01`–`06` are done. What they leave that this plan builds on:

| From | Item | Used here as |
|---|---|---|
| 03 | `Declaration` (derives `Ord`), `Declaration::provenance()`, `Declared::name()`, `Setting`, `SettingName`, `SettingValue`, `EntryName` | the compared values and the unit-test fixtures |
| 04 | `Error::Nondeterministic { provenance: Provenance, first_run: Option<Box<Declaration>>, second_run: Option<Box<Declaration>> }` | `check`'s failure |
| 05 | `ports::Ports` (`type Source: DeclarationSource;`), re-exported from the crate root | `Pipeline`'s type parameter, and `FakePorts` |
| 06 | `load::load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>>`, re-exported as `buildl_core::load` | the phase `check` runs twice |
| 06 | `tests/flows/main.rs`, `tests/flows/common.rs`, `tests/flows/load.rs` | the test binary `check.rs` joins |

`crates/buildl-core/src/lib.rs` code section as plan `06` leaves it (the crate doc above it is not
touched by this plan):

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

`crates/buildl-core/tests/flows/main.rs` as plan `06` leaves it ends with:

```rust
pub mod common;

mod load;
```

`crates/buildl-core/tests/flows/common.rs` as plan `06` leaves it. This plan assumes plan `06` ships
exactly this, without any item only `check.rs` uses — the single-map `FakeSource` and no `FakePorts`.
Its builder helpers (`dir`, `file`, `target`, `setting`, `workspace`) are already final and this plan
does not change them:

```rust
use std::cell::RefCell;
use std::collections::BTreeMap;

use buildl_core::{
    BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
    EvaluationFailure, Freshness, NetworkAccess, Result, StagedDeclaration, StagedFile, StagedItem,
    StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
};

// … FakeFile, unchanged …

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

// … dir, file, target, setting, workspace, unchanged …
```

The `common.rs` items **this plan adds**, all used only by `check.rs`:

| Item | Task |
|---|---|
| `Ports` in the `use buildl_core::{…}` list | 2 |
| `pub struct FakePorts` and `impl Ports for FakePorts { type Source = FakeSource; }` | 2 |
| `Cell` in the `use std::cell::{…}` list | 3 |
| `FakeSource` field `files` renamed `first`; new fields `second: Option<BTreeMap<Directory, FakeFile>>` and `root_evaluations: Cell<u32>`, initialised in `new` | 3 |
| `pub const fn FakeSource::with_second_run(first, second) -> Self` | 3 |
| `evaluate`'s root-evaluation count and its choice between `first` and `second` | 3 |

Gate facts that bite in this plan (from the contract; all verified on this exact code):

- `Pipeline::new` must be `const fn` and `#[must_use]` (`missing_const_for_fn`, `must_use_candidate`);
  so must `FakeSource::with_second_run`.
- `check` returns `Result`, so its doc carries `# Errors`; it names
  [`Error::Nondeterministic`] as an intra-doc link, which `RUSTDOCFLAGS=-D warnings` resolves.
- The test modules unwrap, so each opens with
  `#![expect(clippy::unwrap_used, reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal")]`.
  `unreachable!` in a match arm is not `clippy::panic` and passes.
- `check`'s last line is `first_difference(&first, &second).map_or(Ok(first), Err)`, the form the
  gate was verified with; keep it rather than re-spelling it as a `match`.
- `main.rs` keeps `mod check;` before `mod load;` (alphabetical); both stay private, their `#[test]`
  fns private, and `common.rs` items `pub` and documented.
- `lib.rs`: this plan adds only `pub mod pipeline;` and `pub use pipeline::Pipeline;`. The crate
  doc's module-map row for `pipeline` and the status paragraph belong to plan `08`; the gate does not
  need them.

Run `cargo fmt --all` before every `cargo make dod`; every Rust block below is in rustfmt's form.
All work happens in the worktree, on its branch. Commits follow Conventional Commits with scope
`buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/pipeline/mod.rs     — [create] export-only index (task 1)
crates/buildl-core/src/pipeline/driver.rs  — [create] Pipeline, new, check, first_difference + unit tests (task 1)
crates/buildl-core/src/lib.rs              — [modify] pub mod pipeline; pub use pipeline::Pipeline; (task 1)
crates/buildl-core/tests/flows/main.rs     — [modify] mod check; (task 2)
crates/buildl-core/tests/flows/check.rs    — [create] the deterministic-workspace flow (task 2)
                                             [modify] the two second-run flows (task 3)
crates/buildl-core/tests/flows/common.rs   — [modify] FakePorts (task 2); second-run mode (task 3)
```

---

## Task 1 — Add `Pipeline` with its `check` command

**Files:**
- Create `crates/buildl-core/src/pipeline/mod.rs`
- Create `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/pipeline/mod.rs`:

   ```rust
   //! The pipeline: the phases run in sequence, each command a prefix of that sequence.
   //!
   //! Its own directory because it is the only module that names more than one phase: the phases
   //! communicate through values, and the pipeline is what passes each one's output to the next.
   //!
   //! Responsibilities: [`Pipeline`].
   //!
   //! Non-responsibilities: any phase's logic, and any port's implementation.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod driver;

   pub use driver::Pipeline;
   ```

2. In `crates/buildl-core/src/lib.rs`, insert `pub mod pipeline;` between `pub mod load;` and
   `pub mod ports;`, and `pub use pipeline::Pipeline;` between `pub use load::load;` and
   `pub use ports::{…};`. The code section then reads:

   ```rust
   pub mod error;
   pub mod json;
   pub mod load;
   pub mod pipeline;
   pub mod ports;
   pub mod types;

   pub use error::{
       ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
   };
   pub use load::load;
   pub use pipeline::Pipeline;
   pub use ports::{Clock, DeclarationSource, Ports};
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
       Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
       SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

3. Write the failing tests. Create `crates/buildl-core/src/pipeline/driver.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 5.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use super::first_difference;
       use crate::error::Error;
       use crate::types::{
           Declaration, Declared, Directory, Provenance, Setting, SettingName, SettingValue,
       };

       fn setting_in(directory: &str, name: &str) -> Declaration {
           let directory = Directory::parse(directory).unwrap();
           let file = if directory.is_root() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(format!("{directory}/build.lua"))
           };
           Declaration::new(
               Provenance::new(file, directory),
               Declared::Setting(Setting {
                   name: SettingName::parse(name).unwrap(),
                   default: SettingValue::parse("").unwrap(),
               }),
           )
       }

       fn named_file(error: &Error) -> String {
           match error {
               Error::Nondeterministic { provenance, .. } => provenance.to_string(),
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }

       #[test]
       fn equal_lists_have_no_difference() {
           let list = [setting_in("", "a"), setting_in("lib", "b")];
           assert!(first_difference(&list, &list).is_none());
           assert!(first_difference(&[], &[]).is_none());
       }

       #[test]
       fn an_extra_declaration_names_its_own_file() {
           let first = [setting_in("y", "y1")];
           let second = [setting_in("x", "x1"), setting_in("y", "y1")];
           let error = first_difference(&first, &second).unwrap();
           assert_eq!(named_file(&error), "x/build.lua");
           match error {
               Error::Nondeterministic {
                   first_run,
                   second_run,
                   ..
               } => {
                   assert_eq!(first_run.as_deref(), Some(&setting_in("y", "y1")));
                   assert_eq!(second_run.as_deref(), Some(&setting_in("x", "x1")));
               }
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }

       #[test]
       fn a_declaration_past_the_end_of_the_other_list_is_named() {
           let first = [setting_in("", "a"), setting_in("lib", "b")];
           let second = [setting_in("", "a")];
           let error = first_difference(&first, &second).unwrap();
           assert_eq!(named_file(&error), "lib/build.lua");
           match error {
               Error::Nondeterministic { second_run, .. } => assert!(second_run.is_none()),
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }
   }
   ```

4. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --lib pipeline
   error[E0432]: unresolved import `super::first_difference`
   error[E0432]: unresolved import `driver::Pipeline`
   ```

5. Replace the whole of `crates/buildl-core/src/pipeline/driver.rs` with the implementation above
   the same test module:

   ```rust
   //! The pipeline's driver: one bundle of ports, and one method per command.
   //!
   //! Its own file because the driver owns the sequence of phases, and the sequence grows by one
   //! method per command as later phases arrive. Today it runs Load.
   //!
   //! Responsibilities: [`Pipeline`] and [`Pipeline::check`].
   //!
   //! Non-responsibilities: the phases' own logic, which each phase's module holds.

   use crate::error::{Error, Result};
   use crate::load::load;
   use crate::ports::Ports;
   use crate::types::{Declaration, EntryName};

   /// The pipeline over one chosen implementation of each port.
   pub struct Pipeline<P: Ports> {
       source: P::Source,
   }

   impl<P: Ports> Pipeline<P> {
       /// Assembles a pipeline from its ports.
       #[must_use]
       pub const fn new(source: P::Source) -> Self {
           Self { source }
       }

       /// Loads the workspace twice and returns its declarations when both loads agree.
       ///
       /// Declaration is deterministic by construction only if every build file is; evaluating twice
       /// and comparing is how a nondeterministic one is caught. A `pairs` loop is not
       /// nondeterminism here: the declarations are sorted before they are compared.
       ///
       /// # Errors
       ///
       /// Returns whatever either load returns, and [`Error::Nondeterministic`] when the two loads
       /// declare different things.
       pub fn check(&self, entry: &EntryName) -> Result<Vec<Declaration>> {
           let first = load(&self.source, entry)?;
           let second = load(&self.source, entry)?;
           first_difference(&first, &second).map_or(Ok(first), Err)
       }
   }

   /// The error naming the first place two sorted declaration lists differ, if they do.
   ///
   /// Both lists agree before the first differing index, so the lesser of the two declarations there
   /// is the one the other list lacks, and its build file is the one to fix.
   fn first_difference(first: &[Declaration], second: &[Declaration]) -> Option<Error> {
       let len = first.len().max(second.len());
       (0..len).find_map(|index| {
           let a = first.get(index);
           let b = second.get(index);
           if a == b {
               return None;
           }
           let lacking = match (a, b) {
               (Some(a), Some(b)) => a.min(b),
               (Some(only), None) | (None, Some(only)) => only,
               (None, None) => return None,
           };
           Some(Error::Nondeterministic {
               provenance: lacking.provenance().clone(),
               first_run: a.cloned().map(Box::new),
               second_run: b.cloned().map(Box::new),
           })
       })
   }

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use super::first_difference;
       use crate::error::Error;
       use crate::types::{
           Declaration, Declared, Directory, Provenance, Setting, SettingName, SettingValue,
       };

       fn setting_in(directory: &str, name: &str) -> Declaration {
           let directory = Directory::parse(directory).unwrap();
           let file = if directory.is_root() {
               PathBuf::from("build.lua")
           } else {
               PathBuf::from(format!("{directory}/build.lua"))
           };
           Declaration::new(
               Provenance::new(file, directory),
               Declared::Setting(Setting {
                   name: SettingName::parse(name).unwrap(),
                   default: SettingValue::parse("").unwrap(),
               }),
           )
       }

       fn named_file(error: &Error) -> String {
           match error {
               Error::Nondeterministic { provenance, .. } => provenance.to_string(),
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }

       #[test]
       fn equal_lists_have_no_difference() {
           let list = [setting_in("", "a"), setting_in("lib", "b")];
           assert!(first_difference(&list, &list).is_none());
           assert!(first_difference(&[], &[]).is_none());
       }

       #[test]
       fn an_extra_declaration_names_its_own_file() {
           let first = [setting_in("y", "y1")];
           let second = [setting_in("x", "x1"), setting_in("y", "y1")];
           let error = first_difference(&first, &second).unwrap();
           assert_eq!(named_file(&error), "x/build.lua");
           match error {
               Error::Nondeterministic {
                   first_run,
                   second_run,
                   ..
               } => {
                   assert_eq!(first_run.as_deref(), Some(&setting_in("y", "y1")));
                   assert_eq!(second_run.as_deref(), Some(&setting_in("x", "x1")));
               }
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }

       #[test]
       fn a_declaration_past_the_end_of_the_other_list_is_named() {
           let first = [setting_in("", "a"), setting_in("lib", "b")];
           let second = [setting_in("", "a")];
           let error = first_difference(&first, &second).unwrap();
           assert_eq!(named_file(&error), "lib/build.lua");
           match error {
               Error::Nondeterministic { second_run, .. } => assert!(second_run.is_none()),
               other => unreachable!("expected Nondeterministic, got {other:?}"),
           }
       }
   }
   ```

6. Run and confirm green — the three new tests are `equal_lists_have_no_difference`,
   `an_extra_declaration_names_its_own_file` and
   `a_declaration_past_the_end_of_the_other_list_is_named`:

   ```
   $ cargo test -p buildl-core --lib pipeline
   test pipeline::driver::tests::equal_lists_have_no_difference ... ok
   test pipeline::driver::tests::an_extra_declaration_names_its_own_file ... ok
   test pipeline::driver::tests::a_declaration_past_the_end_of_the_other_list_is_named ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 131 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   ```

   Expected: exit 0; the lib unit-test run reports `134 passed`.

8. Commit `feat(buildl-core): add the pipeline driver and its check command`.

---

## Task 2 — Drive `check` through `FakePorts`

**Files:**
- Modify `crates/buildl-core/tests/flows/main.rs`
- Create `crates/buildl-core/tests/flows/check.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. In `crates/buildl-core/tests/flows/main.rs`, insert `mod check;` above `mod load;`. The file's
   end then reads:

   ```rust
   pub mod common;

   mod check;
   mod load;
   ```

2. Write the failing test. Create `crates/buildl-core/tests/flows/check.rs`:

   ```rust
   //! `Pipeline::check`, driven through the build-file port by [`FakeSource`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use buildl_core::{EntryName, Pipeline};

   use crate::common::{FakePorts, FakeSource, file, target, workspace};

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   #[test]
   fn check_returns_the_sorted_declarations_of_a_deterministic_workspace() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
           ("lib", file(vec![target("text", &[])], &[])),
       ]));
       let pipeline = Pipeline::<FakePorts>::new(source);
       let declarations = pipeline.check(&entry()).unwrap();
       let names: Vec<&str> = declarations.iter().map(|d| d.item().name()).collect();
       assert_eq!(names, ["a", "b", "text"]);
   }
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --test flows check
   error[E0432]: unresolved import `crate::common::FakePorts`
   ```

4. In `crates/buildl-core/tests/flows/common.rs`, add `Ports` to the `buildl_core` import, which then
   reads:

   ```rust
   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedDeclaration, StagedFile,
       StagedItem, StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
   };
   ```

   and insert, between the closing brace of `impl DeclarationSource for FakeSource` and the
   `/// A directory from known-valid text.` doc comment of `dir`:

   ```rust
   /// The bundle choosing [`FakeSource`].
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
   }
   ```

5. Run and confirm green — the new test is
   `check_returns_the_sorted_declarations_of_a_deterministic_workspace`:

   ```
   $ cargo test -p buildl-core --test flows check
   test check::check_returns_the_sorted_declarations_of_a_deterministic_workspace ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   ```

   Expected: exit 0; the `flows` binary reports `10 passed`.

7. Commit `test(buildl-core): drive Pipeline::check through the fake ports`.

---

## Task 3 — Catch a workspace that declares differently on its second load

**Files:**
- Modify `crates/buildl-core/tests/flows/check.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing tests. Replace the whole of `crates/buildl-core/tests/flows/check.rs` with:

   ```rust
   //! `Pipeline::check`, driven through the build-file port by [`FakeSource`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::path::Path;

   use buildl_core::{EntryName, Error, Pipeline};

   use crate::common::{FakePorts, FakeSource, file, setting, target, workspace};

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   #[test]
   fn check_returns_the_sorted_declarations_of_a_deterministic_workspace() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("b", &[]), target("a", &[])], &["lib"])),
           ("lib", file(vec![target("text", &[])], &[])),
       ]));
       let pipeline = Pipeline::<FakePorts>::new(source);
       let declarations = pipeline.check(&entry()).unwrap();
       let names: Vec<&str> = declarations.iter().map(|d| d.item().name()).collect();
       assert_eq!(names, ["a", "b", "text"]);
   }

   #[test]
   fn a_pairs_shuffled_second_evaluation_passes_check() {
       let first = workspace(vec![(
           "",
           file(
               vec![target("a", &[]), setting("s"), target("b", &["a"])],
               &[],
           ),
       )]);
       let second = workspace(vec![(
           "",
           file(
               vec![target("b", &["a"]), target("a", &[]), setting("s")],
               &[],
           ),
       )]);
       let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
       assert_eq!(pipeline.check(&entry()).unwrap().len(), 3);
   }

   #[test]
   fn a_second_evaluation_that_declares_more_fails_check_naming_the_file() {
       let first = workspace(vec![
           ("", file(vec![target("app", &[])], &["lib"])),
           ("lib", file(vec![], &[])),
       ]);
       let second = workspace(vec![
           ("", file(vec![target("app", &[])], &["lib"])),
           ("lib", file(vec![target("extra", &[])], &[])),
       ]);
       let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
       match pipeline.check(&entry()).unwrap_err() {
           Error::Nondeterministic {
               provenance,
               first_run,
               second_run,
           } => {
               assert_eq!(provenance.file(), Path::new("lib/build.lua"));
               assert!(first_run.is_none());
               assert_eq!(second_run.unwrap().item().name(), "extra");
           }
           other => unreachable!("expected Nondeterministic, got {other:?}"),
       }
   }
   ```

2. Run and confirm failure (one error per call site):

   ```
   $ cargo test -p buildl-core --test flows check
   error[E0599]: no associated function or constant named `with_second_run` found for struct `FakeSource` in the current scope
   error[E0599]: no associated function or constant named `with_second_run` found for struct `FakeSource` in the current scope
   ```

3. Replace the whole of `crates/buildl-core/tests/flows/common.rs` with its final form. Against
   Task 2's state this adds `Cell` to the `std::cell` import, renames the field `files` to `first`,
   adds the fields `second` and `root_evaluations` (initialised in `new`), adds `with_second_run`,
   and makes `evaluate` count root evaluations and read `second` from the second one on; the
   builders are unchanged:

   ```rust
   //! In-memory implementations of the ports, and builders for the staged values they return.
   //!
   //! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

   use std::cell::{Cell, RefCell};
   use std::collections::BTreeMap;

   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedDeclaration, StagedFile,
       StagedItem, StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
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
       first: BTreeMap<Directory, FakeFile>,
       second: Option<BTreeMap<Directory, FakeFile>>,
       root_evaluations: Cell<u32>,
       evaluated: RefCell<Vec<Directory>>,
   }

   impl FakeSource {
       /// A source returning the same files on every evaluation.
       #[must_use]
       pub const fn new(files: BTreeMap<Directory, FakeFile>) -> Self {
           Self {
               first: files,
               second: None,
               root_evaluations: Cell::new(0),
               evaluated: RefCell::new(Vec::new()),
           }
       }

       /// A source returning `first` until the root is evaluated a second time, and `second` from
       /// then on — a workspace that declares differently on its second load.
       #[must_use]
       pub const fn with_second_run(
           first: BTreeMap<Directory, FakeFile>,
           second: BTreeMap<Directory, FakeFile>,
       ) -> Self {
           Self {
               first,
               second: Some(second),
               root_evaluations: Cell::new(0),
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
           if directory.is_root() {
               self.root_evaluations.set(self.root_evaluations.get() + 1);
           }
           self.evaluated.borrow_mut().push(directory.clone());
           let files = match &self.second {
               Some(second) if self.root_evaluations.get() > 1 => second,
               _ => &self.first,
           };
           match files.get(directory) {
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

   /// The bundle choosing [`FakeSource`].
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
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

4. Run and confirm green — the new tests are `a_pairs_shuffled_second_evaluation_passes_check` and
   `a_second_evaluation_that_declares_more_fails_check_naming_the_file`:

   ```
   $ cargo test -p buildl-core --test flows check
   test check::a_pairs_shuffled_second_evaluation_passes_check ... ok
   test check::a_second_evaluation_that_declares_more_fails_check_naming_the_file ... ok
   test check::check_returns_the_sorted_declarations_of_a_deterministic_workspace ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   ```

   Expected: exit 0; the `flows` binary reports `12 passed` (the `load` flows are unaffected by the
   `FakeSource` change: `new` sets `second` to `None`, so `evaluate` always reads `first`).

6. Commit `test(buildl-core): catch a workspace that declares differently on its second load`.

---

## Verification summary (plan-level)

- `cargo make dod` exits 0: fmt-check, clippy `--workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings` doc, test `--all-targets`, test `--doc`.
- `cargo deny check`, `cargo make guard-crate-edges` and `cargo make guard-core-purity` exit 0;
  `crates/buildl-core/Cargo.toml` and `crates/expected-edges.txt` are unchanged.
- Test totals: `buildl-core` lib unit tests `134 passed` (3 new in `pipeline::driver`); the `flows`
  binary `12 passed` (3 new in `check`).
- Spec §8's two `check` rows each have a test: "a fake whose second evaluation differs" →
  `a_second_evaluation_that_declares_more_fails_check_naming_the_file`; "a `pairs`-shuffled fake
  across the two runs" → `a_pairs_shuffled_second_evaluation_passes_check`. Spec §10's "through
  `Pipeline::check`" → `check_returns_the_sorted_declarations_of_a_deterministic_workspace`.
- `grep -rn 'pipeline' crates/buildl-core/src/load` returns nothing: `load/` names no other phase,
  and `pipeline/` is the only module that names `load/`.
- `src/pipeline/`, `tests/flows/check.rs`, `tests/flows/common.rs` and `tests/flows/main.rs` match
  the final state byte for byte; `src/lib.rs` differs from it only in the crate doc's module-map row
  for `pipeline` and the status paragraph, which plan `08` writes.

## Review findings

- correctness 🔴 — `first_difference` picked the lesser declaration with `a.min(b)`, `Declaration`'s derived `Ord` (provenance file path first), not Load's sort order (directory, declared name, whole declaration); across directories the two disagree (`app/build.lua` < `build.lua`, but root < `app`), so the error named a file that did not change — `src/pipeline/driver.rs:57`. Fixed: Load's comparator extracted as crate-private `load::declaration_order`, used by both `sort` and `first_difference` (`min_by`). Verified: `cargo make dod` → lib `137 passed`, flows `13 passed`, doctests `2 passed`, 0 failed.
- doc 🔴 — `first_difference` doc stated the invariant the code broke — `src/pipeline/driver.rs:46`. Fixed: doc now says the lesser in the order Load sorts by.
- test-coverage 🟡 — no test where the first differing pair spans root vs subdirectory, or `a-b` vs `a/b` — `src/pipeline/driver.rs:115`. Fixed: unit tests `a_root_declaration_is_named_over_a_subdirectory_one` (red: left "app/build.lua", right "build.lua") and `a_dash_directory_is_named_over_a_nested_one_it_sorts_before` (red: left "a/b/build.lua", right "a-b/build.lua"); green: `cargo test -p buildl-core --lib pipeline` → 5 passed.
- spec drift 🟡 — spec §8 "a fake whose second evaluation differs" requires both differing declarations; the flow asserted a one-sided pair only — `tests/flows/check.rs`. Fixed: flow `a_root_declaration_is_named_over_a_subdirectory_one_it_sorts_before` through `FakeSource::with_second_run`, both sides `Some` (red: left "app/build.lua", right "build.lua"); green: `cargo test -p buildl-core --test flows check` → 4 passed.
- guideline 🟡 — public `Pipeline<P>` lacked `Debug` (M-PUBLIC-DEBUG) — `src/pipeline/driver.rs:16`. Fixed: hand-written `impl<P: Ports> fmt::Debug for Pipeline<P> where P::Source: fmt::Debug`.
- doc 🔵 — `FakeSource` doc did not mention its second-run mode — `tests/flows/common.rs:22`. Fixed.
- spec 🔵 — spec §7 lists `mod load; mod check;`; delivery is alphabetical. No change.

## Probe results

- Claim: buildl-core lib has 131 unit tests before Task 1. `cargo test -p buildl-core --lib` → `test result: ok. 132 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. **Against the plan** (plan 06's fix round added one test); every lib count shifted +1, no behavioural impact.
- Claim (reviewer probe, scratch crate over the public API): with run 1 root empty, run 2 root declaring setting `r`, both runs `subdir("app")` with identical `app`, `check` names the changed file. Output before the fix: `Nondeterministic`, named file `app/build.lua`. **Against the plan's Task 1 code.**

## Deviations

- 2026-10-08 — `first_difference` no longer uses `Declaration`'s derived `Ord`. The plan's Task 1 code picked `a.min(b)`, which orders by file path first and so disagrees with Load's sort across directories, breaking spec §5.2's "its file is the one to fix". Load's sort comparator moved into `pub(crate) fn declaration_order` in `load/traversal.rs` (re-exported `pub(crate)` from `load/mod.rs`); `sort` and `first_difference` share it. Rejected: a hand-written `Ord` on `Declaration` — it would put Load's sort key on a public type's trait. Spec §5.2 wording is correct and unchanged.
- 2026-10-08 — added two unit tests, one flow and a `Debug` impl beyond the plan (review findings above). Final counts: lib 137 (plan: 134), flows 13 (plan: 12).
