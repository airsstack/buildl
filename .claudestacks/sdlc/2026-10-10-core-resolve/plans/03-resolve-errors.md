---
status: done
created: 2026-10-10
---

# Resolve Errors Implementation Plan

**Goal:** The seven Resolve errors render the one-line messages the spec fixes.

**Architecture:** Seven variants join the crate's one `#[non_exhaustive]` `Error` enum; none is reshaped. Two helper types join `error.rs`: `DeclaredKind` (what a label was declared as) and `DeclarationSite` (a declared label and its build file). The three reference errors hold their holder as `site: Box<DeclarationSite>` rather than as separate `provenance` and `label` fields: unboxed, the variants are 193, 194 and 144 bytes and clippy's `result_large_err` fails every function in the crate that returns `Result`. Rendering goes through three private helper functions so each `#[error]` attribute stays one format string.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §5 (errors, as amended 2026-10-10), §9 (the `error.rs` row); decisions D5, D12.

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What already exists and is used here:

| Item | Where | Used as |
|---|---|---|
| `Error` (`#[non_exhaustive]`, `thiserror`) | `crates/buildl-core/src/error.rs:36` | the enum the variants join |
| `DeclarationField::{Dep, Rule, Target}`, rendering `dep`, `rule`, `target` | `crates/buildl-core/src/error.rs:198` | the `field` of a reference error |
| `Label`, `Provenance`, `SettingName`, each with `Display` | `crates/buildl-core/src/types/` | the errors' fields |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- A test helper returning `Box<DeclarationSite>` fails clippy's `unnecessary_box_returns`; the
  helper below returns the value and each use boxes it.
- `DeclaredKind::with_article` exists because the sentence needs "an alias" beside "a rule"; it is
  private and `const`.
- The `TooManyTargets` test uses a small count on purpose: a literal above `u32::MAX` does not fit
  a `usize` on a 32-bit target.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/error.rs   — [modify] DuplicateLabel, DuplicateSetting, TooManyTargets (task 1)
                                    [modify] DeclaredKind, DeclarationSite, the four reference and cycle variants (task 2)
crates/buildl-core/src/lib.rs     — [modify] re-export DeclaredKind and DeclarationSite (task 2)
```

### Task 1 — Add the errors for a repeated name and for too many targets

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, widen the import. Replace:

   ```rust
       use crate::types::{
           DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, TargetName, Written,
       };
   ```

   with:

   ```rust
       use crate::types::{
           DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance, SettingName,
           TargetName, Written,
       };
   ```

2. Add two fixtures and three tests in the same module. Insert:

   ```rust
       fn root_file() -> Provenance {
           Provenance::new(PathBuf::from("build.lua"), Directory::root())
       }

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       #[test]
       fn duplicate_label_names_every_site() {
           let err = Error::DuplicateLabel {
               label: label("//:a"),
               sites: vec![root_file(), root_file()],
           };
           assert_eq!(
               err.to_string(),
               "//:a is declared more than once: build.lua, build.lua"
           );
       }

       #[test]
       fn duplicate_setting_names_every_site() {
           let err = Error::DuplicateSetting {
               name: SettingName::parse("test_filter").unwrap(),
               sites: vec![root_file(), lib_file()],
           };
           assert_eq!(
               err.to_string(),
               "setting test_filter is declared more than once: build.lua, lib/build.lua"
           );
       }

       #[test]
       fn too_many_targets_states_the_count() {
           let err = Error::TooManyTargets { count: 7 };
           assert_eq!(
               err.to_string(),
               "7 targets are more than a graph can number"
           );
       }
   ```

   immediately after:

   ```rust
               Directory::parse("lib").unwrap(),
           )
       }
   ```

3. Run them and confirm they fail to build, because the variants do not exist:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0599]: no variant named `DuplicateLabel` found for enum `error::Error`
   error[E0599]: no variant named `DuplicateSetting` found for enum `error::Error`
   error[E0599]: no variant named `TooManyTargets` found for enum `error::Error`
   ```

4. Widen the file's own import. Replace:

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, Written,
   };
   ```

   with:

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance,
       SettingName, Written,
   };
   ```

5. Add the three variants at the end of `Error`. Insert:

   ```rust
       /// A label was declared more than once, as any mix of target, rule and alias.
       #[error("{label} is declared more than once: {}", sites_list(.sites))]
       DuplicateLabel {
           /// The label.
           label: Label,
           /// The build file of every declaration of it, in the declarations' own order.
           sites: Vec<Provenance>,
       },
       /// A build setting was declared more than once.
       #[error("setting {name} is declared more than once: {}", sites_list(.sites))]
       DuplicateSetting {
           /// The setting.
           name: SettingName,
           /// The build file of every declaration of it, in the declarations' own order.
           sites: Vec<Provenance>,
       },
       /// The workspace declares more targets than the graph has ids for.
       #[error("{count} targets are more than a graph can number")]
       TooManyTargets {
           /// How many targets were declared.
           count: usize,
       },
   ```

   immediately after:

   ```rust
           /// The second evaluation's declaration at the first difference; `None` past its end.
           second_run: Option<Box<Declaration>>,
       },
   ```

6. Add the helper that renders a list of build files. Insert:

   ```rust
   /// The build files of a duplicated name, separated by commas.
   fn sites_list(sites: &[Provenance]) -> String {
       let rendered: Vec<String> = sites.iter().map(ToString::to_string).collect();
       rendered.join(", ")
   }
   ```

   immediately before:

   ```rust
   /// A ceiling an evaluation can reach.
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

9. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): add the errors for a repeated name and too many targets"
   ```

### Task 2 — Add the errors for a bad reference and for a cycle

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, widen the import. Replace:

   ```rust
       use super::{
           ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind,
       };
   ```

   with:

   ```rust
       use super::{
           ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
           EvaluationLimit, NameKind,
       };
   ```

2. Add a fixture. Insert:

   ```rust
       fn tools_file() -> Provenance {
           Provenance::new(
               PathBuf::from("tools/build.lua"),
               Directory::parse("tools").unwrap(),
           )
       }
   ```

   immediately before:

   ```rust
       fn label(raw: &str) -> Label {
   ```

3. Add a second fixture. Insert:

   ```rust
       /// The declaration `raw`, declared in the root build file.
       fn root_site(raw: &str) -> DeclarationSite {
           DeclarationSite {
               label: label(raw),
               provenance: root_file(),
           }
       }
   ```

   immediately before:

   ```rust
       #[test]
       fn duplicate_label_names_every_site() {
   ```

4. Add seven tests. Insert:

   ```rust
       #[test]
       fn unknown_reference_names_the_site_the_field_and_the_nearest_label() {
           let err = Error::UnknownReference {
               site: Box::new(root_site("//:app")),
               field: DeclarationField::Dep,
               reference: label("//:mian.o"),
               suggestion: Some(label("//:main.o")),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: dep //:mian.o is not declared (did you mean //:main.o?)"
           );
       }

       #[test]
       fn unknown_reference_without_a_near_label_offers_nothing() {
           let err = Error::UnknownReference {
               site: Box::new(root_site("//:default")),
               field: DeclarationField::Target,
               reference: label("//:nope"),
               suggestion: None,
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:default: target //:nope is not declared"
           );
       }

       #[test]
       fn wrong_reference_kind_names_what_was_found_and_where() {
           let err = Error::WrongReferenceKind {
               site: Box::new(root_site("//:app")),
               field: DeclarationField::Dep,
               reference: label("//:cc"),
               found: DeclaredKind::Rule,
               declared_in: tools_file(),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: dep //:cc names a rule, declared in tools/build.lua"
           );
       }

       #[test]
       fn every_declared_kind_renders_alone_and_in_a_sentence() {
           let cases = [
               (DeclaredKind::Target, "target", "names a target,"),
               (DeclaredKind::Rule, "rule", "names a rule,"),
               (DeclaredKind::Alias, "alias", "names an alias,"),
           ];
           for (found, alone, in_a_sentence) in cases {
               assert_eq!(found.to_string(), alone);
               let err = Error::WrongReferenceKind {
                   site: Box::new(root_site("//:app")),
                   field: DeclarationField::Rule,
                   reference: label("//:x"),
                   found,
                   declared_in: root_file(),
               };
               assert!(err.to_string().contains(in_a_sentence), "{err}");
           }
       }

       #[test]
       fn unknown_setting_names_the_site_and_the_nearest_setting() {
           let err = Error::UnknownSetting {
               site: Box::new(root_site("//:go_test")),
               name: "test_fliter".to_owned(),
               suggestion: Some(SettingName::parse("test_filter").unwrap()),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:go_test: setting test_fliter is not declared \
                (did you mean test_filter?)"
           );
       }

       #[test]
       fn dependency_cycle_lists_every_target_and_closes_on_the_first() {
           let err = Error::DependencyCycle {
               path: vec![
                   DeclarationSite {
                       label: label("//:a"),
                       provenance: root_file(),
                   },
                   DeclarationSite {
                       label: label("//lib:b"),
                       provenance: lib_file(),
                   },
               ],
           };
           assert_eq!(
               err.to_string(),
               "dependency cycle: //:a (build.lua) -> //lib:b (lib/build.lua) -> //:a"
           );
       }

       #[test]
       fn a_cycle_of_one_names_its_target_twice() {
           let err = Error::DependencyCycle {
               path: vec![DeclarationSite {
                   label: label("//:a"),
                   provenance: root_file(),
               }],
           };
           assert_eq!(
               err.to_string(),
               "dependency cycle: //:a (build.lua) -> //:a"
           );
       }
   ```

   immediately before:

   ```rust
       #[test]
       fn too_many_targets_states_the_count() {
   ```

5. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0432]: unresolved imports `super::DeclarationSite`, `super::DeclaredKind`
   error[E0599]: no variant named `UnknownReference` found for enum `error::Error`
   error[E0599]: no variant named `WrongReferenceKind` found for enum `error::Error`
   error[E0599]: no variant named `UnknownSetting` found for enum `error::Error`
   error[E0599]: no variant named `DependencyCycle` found for enum `error::Error`
   ```

6. Index the two new types in the module doc. Insert:

   ```rust
   //! - [`DeclaredKind`] — what a label an [`Error::WrongReferenceKind`] is about was declared as.
   //! - [`DeclarationSite`] — a declared label and its build file: the declaration holding a bad
   //!   reference, or one target on a dependency cycle.
   ```

   immediately after:

   ```rust
   //! - [`ActionFound`] — what an [`Error::ActionConflict`] found instead of one action.
   ```

7. Add the four variants. Insert:

   ```rust
       /// A reference names a label nothing declares.
       #[error(
           "{}: {}: {field} {reference} is not declared{}",
           .site.provenance,
           .site.label,
           suggestion_suffix(.suggestion.as_ref())
       )]
       UnknownReference {
           /// The declaration holding the reference, and its build file.
           site: Box<DeclarationSite>,
           /// Which field holds it: a dependency, a rule reference or an alias's target.
           field: DeclarationField,
           /// The label that was named.
           reference: Label,
           /// The nearest declared label the field could have named, if one is near enough.
           suggestion: Option<Label>,
       },
       /// A reference names a declaration of a kind its field cannot name.
       #[error(
           "{}: {}: {field} {reference} names {}, declared in {declared_in}",
           .site.provenance,
           .site.label,
           .found.with_article()
       )]
       WrongReferenceKind {
           /// The declaration holding the reference, and its build file.
           site: Box<DeclarationSite>,
           /// Which field holds it: a dependency, a rule reference or an alias's target.
           field: DeclarationField,
           /// The label that was named.
           reference: Label,
           /// What that label was declared as.
           found: DeclaredKind,
           /// The build file that declared it.
           declared_in: Provenance,
       },
       /// A command references a build setting nothing declares.
       #[error(
           "{}: {}: setting {name} is not declared{}",
           .site.provenance,
           .site.label,
           suggestion_suffix(.suggestion.as_ref())
       )]
       UnknownSetting {
           /// The target or rule whose command holds the reference, and its build file.
           site: Box<DeclarationSite>,
           /// The name after `$opt:`, as written.
           name: String,
           /// The nearest declared setting, if one is near enough.
           suggestion: Option<SettingName>,
       },
       /// The targets' dependencies form a cycle.
       #[error("dependency cycle: {}", cycle_path(.path))]
       DependencyCycle {
           /// The targets on the cycle, lowest label first; each depends on the next, and the last
           /// on the first.
           path: Vec<DeclarationSite>,
       },
   ```

   immediately before:

   ```rust
       /// The workspace declares more targets than the graph has ids for.
   ```

8. Add the two rendering helpers and the two types. Insert:

   ```rust
   /// The suffix offering the nearest declared name, when there is one.
   fn suggestion_suffix<T: fmt::Display>(suggestion: Option<&T>) -> String {
       suggestion.map_or_else(String::new, |nearest| format!(" (did you mean {nearest}?)"))
   }

   /// A cycle as its labels and build files, closed by repeating the first label.
   fn cycle_path(path: &[DeclarationSite]) -> String {
       let mut rendered: Vec<String> = path
           .iter()
           .map(|site| format!("{} ({})", site.label, site.provenance))
           .collect();
       if let Some(first) = path.first() {
           rendered.push(first.label.to_string());
       }
       rendered.join(" -> ")
   }

   /// What a label was declared as.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum DeclaredKind {
       /// A build or test target.
       Target,
       /// A rule.
       Rule,
       /// An alias.
       Alias,
   }

   impl DeclaredKind {
       /// The kind with its indefinite article, for a sentence.
       const fn with_article(self) -> &'static str {
           match self {
               Self::Target => "a target",
               Self::Rule => "a rule",
               Self::Alias => "an alias",
           }
       }
   }

   impl fmt::Display for DeclaredKind {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Target => "target",
               Self::Rule => "rule",
               Self::Alias => "alias",
           })
       }
   }

   /// A declared label and the build file that declared it.
   ///
   /// Names the declaration an error is about: the one holding a bad reference, or one target on
   /// a dependency cycle.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct DeclarationSite {
       /// The declared label.
       pub label: Label,
       /// The build file that declared it.
       pub provenance: Provenance,
   }
   ```

   immediately before:

   ```rust
   /// A ceiling an evaluation can reach.
   ```

9. Re-export the two types from the crate root, in `crates/buildl-core/src/lib.rs`. Replace:

   ```rust
   pub use error::{
       ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
   };
   ```

   with:

   ```rust
   pub use error::{
       ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
       EvaluationLimit, NameKind, Result,
   };
   ```

10. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out
   ```

11. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

12. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the errors for a bad reference and a cycle"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- Each rendering in spec §5's table is asserted character for character by one test, except
  `TooManyTargets`: its test uses a count of 7, because the spec row's count does not fit a 32-bit
  `usize`. The format string is the same.
- `cargo clippy` raises no `result_large_err`: the three reference errors box their site.

## Review findings

One reviewer pass over both tasks' files, 2026-10-10. Verdict: spec compliant, blocking set empty. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0; `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). Every code block of both tasks is in the tree as written, and each message matches spec §5 as amended.

- nit — the `field: DeclarationField` doc names three cases ("a dependency, a rule reference or an alias's target") while the type admits all twelve variants; only the future constructor holds the limit. Spec §5 fixes the field's type. Not fixed — `crates/buildl-core/src/error.rs:145`, `:163`
- nit — `cycle_path` over an empty `path` renders `dependency cycle: ` with a trailing space; `DependencyCycle::path` is a public `Vec` whose doc does not say non-empty, and no test pins the case. Unreachable until Resolve constructs the error. Not fixed — `crates/buildl-core/src/error.rs:220`
- nit — nothing fails today if the `DeclarationSite`/`DeclaredKind` re-export is reverted: both stay reachable as `buildl_core::error::…`, and no test or doc link names the root path. The flow tests of plan `07` are what will guard it. Not fixed — `crates/buildl-core/src/lib.rs:67`

## Probe results

No separate probe was run. Every fact the tasks assert about existing code is put under test by their own cycles, and all four runs matched the plan:

- task 1 red — `cargo test -p buildl-core --lib error::tests` — ``error[E0599]: no variant named `DuplicateLabel` found for enum `error::Error` ``, the same for `DuplicateSetting` and `TooManyTargets`; `could not compile buildl-core (lib test) due to 3 previous errors`
- task 1 green — `cargo test -p buildl-core --lib error::tests` — `test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out`
- task 2 red — `cargo test -p buildl-core --lib error::tests` — E0432 unresolved imports `super::DeclarationSite`, `super::DeclaredKind`; E0599 no variant `UnknownReference`, `WrongReferenceKind`, `UnknownSetting`, `DependencyCycle`; `could not compile buildl-core (lib test) due to 8 previous errors`
- task 2 green — `cargo test -p buildl-core --lib error::tests` — `test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 138 filtered out`

## Deviations

- 2026-10-10 — neither task's commit step was run during execution. The commit is the author's to make; each task's files were left in the working tree.
