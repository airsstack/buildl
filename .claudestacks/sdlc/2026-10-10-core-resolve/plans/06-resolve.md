---
status: approved
created: 2026-10-10
depends-on: [01, 02, 03, 04, 05]
---

# Resolve Implementation Plan

**Goal:** `resolve` turns a workspace's declarations into one `TargetGraph`, or into the first error the spec's order names.

**Architecture:** Three files join `resolve/`, one responsibility each. `index.rs` groups declarations by name and refuses a duplicate; `references.rs` answers what one reference names, one function per kind of reference; `assembly.rs` holds `resolve`, which fixes the order: index, number the targets in label order, walk the index once checking references while building nodes and edges, then search the edges for a cycle. A dependency through an alias whose own target is wrong yields no edge and no error at the dependency: the walk reports it when it reaches the alias. `Declaration::into_parts` lets the index move a declaration's parts instead of cloning them. A test-only `fixtures.rs` holds the hand-built declarations the three test modules share.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §4.1–§4.4 (Resolve), §8 (module layout), §9 (the `resolve/index.rs`, `resolve/references.rs` and `resolve/assembly.rs` rows); decisions D1–D6, D9, D12.

**Checkpoints:** review after task 3 (the index and the reference checks), and again when task 4 is done.

---

## Context an implementer needs

What plans `01`–`05` leave that this plan builds on:

| From | Item | Used here as |
|---|---|---|
| 01 | `Node` (public fields), `TargetGraph::new(nodes, edges, aliases, settings)` (`pub(crate)`, under a temporary `expect(dead_code)`), the accessors `id_of`, `deps`, `dependents`, `node`, `nodes`, `settings` | what `resolve` builds, and what its tests read |
| 02 | `Argument::setting_references(&self) -> Vec<&str>` | the `$opt:` names of a command |
| 03 | `Error::{DuplicateLabel, DuplicateSetting, UnknownReference, WrongReferenceKind, UnknownSetting, DependencyCycle, TooManyTargets}`, `DeclaredKind`, `DeclarationSite { label, provenance }` | every failure |
| 04 | `resolve::suggest::nearest(wanted, candidates) -> Option<&T>` | the suggestion in two errors |
| 05 | `resolve::cycle::find(edges) -> Option<Vec<NodeId>>` | step 5 |
| 04, 05 | `resolve/mod.rs` declaring `cycle` and `suggest`, each under `cfg_attr(not(test), expect(dead_code, …))` | the file tasks 2–4 rewrite |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- Tasks 2 and 3 add files nothing outside the tests calls yet, so their `mod` lines carry the
  same `cfg_attr(not(test), expect(dead_code, …))` as `cycle` and `suggest`. Task 4 gives every
  one of them a caller and removes all four attributes, together with the one on
  `TargetGraph::new`; an attribute left behind fails the gate as an unfulfilled expectation.
- `Holder::site` returns a `DeclarationSite` and each error boxes it: a function returning
  `Box<DeclarationSite>` fails clippy's `unnecessary_box_returns`.
- A doc link to the function is written `[`resolve`](fn@crate::resolve)`: `crate::resolve` alone
  is ambiguous between the module and the function, and rustdoc fails the gate on it.
- `node` uses a let-chain (`if let Some(id) = … && seen.insert(*id)`), which the 2024 edition
  allows.
- The test for a position past `u32::MAX` is compiled on 64-bit targets only.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/declaration.rs   — [modify] Declaration::into_parts (task 1)
crates/buildl-core/src/resolve/fixtures.rs      — [create] test-only declaration builders (task 2)
crates/buildl-core/src/resolve/index.rs         — [create] Index, Entry, Item, index, unit tests (task 2)
crates/buildl-core/src/resolve/references.rs    — [create] Holder, dependency, rule, alias_target, settings, unit tests (task 3)
crates/buildl-core/src/resolve/assembly.rs      — [create] resolve, number, node_id, node, without_repeats, unit tests (task 4)
crates/buildl-core/src/resolve/mod.rs           — [modify] declare each file (tasks 2, 3); final form with the re-export (task 4)
crates/buildl-core/src/types/target_graph.rs   — [modify] drop the temporary attribute, link to resolve (task 4)
crates/buildl-core/src/lib.rs                   — [modify] re-export resolve (task 4)
```

### Task 1 — Take a declaration apart by value

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`

**Steps:**

1. Write the failing assertion, in the test `exposes_its_provenance_and_item` of `crates/buildl-core/src/types/declaration.rs`. Replace:

   ```rust
           assert_eq!(declaration.provenance(), &provenance);
           assert_eq!(declaration.item(), &Declared::Target(target()));
       }
   ```

   with:

   ```rust
           assert_eq!(declaration.provenance(), &provenance);
           assert_eq!(declaration.item(), &Declared::Target(target()));
           assert_eq!(
               declaration.into_parts(),
               (provenance, Declared::Target(target()))
           );
       }
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib types::declaration
   error[E0599]: no method named `into_parts` found for struct `Declaration` in the current scope
   ```

3. Add the method to `impl Declaration`. Insert:

   ```rust

       /// The build file and what it declared, by value.
       #[must_use]
       pub fn into_parts(self) -> (Provenance, Declared) {
           (self.provenance, self.item)
       }
   ```

   immediately after:

   ```rust
       pub const fn item(&self) -> &Declared {
           &self.item
       }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::declaration
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 169 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

6. Commit:

   ```
   $ git add crates/buildl-core/src/types/declaration.rs
   $ git commit -m "feat(buildl-core): take a declaration apart by value"
   ```

### Task 2 — Index declarations by name and refuse a duplicate

**Files:**
- Create `crates/buildl-core/src/resolve/fixtures.rs`
- Create `crates/buildl-core/src/resolve/index.rs`
- Modify `crates/buildl-core/src/resolve/mod.rs`

**Steps:**

1. Declare the two modules. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod cycle;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod index;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod suggest;

   #[cfg(test)]
   mod fixtures;
   ```

2. Create the shared test fixtures, `crates/buildl-core/src/resolve/fixtures.rs`:

   ```rust
   //! Declarations built by hand, shared by this phase's unit tests.
   //!
   //! Its own file because three test modules need the same builders. Compiled for tests only, so
   //! it carries no test module of its own.

   #![expect(
       clippy::unwrap_used,
       reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
   )]

   use std::path::PathBuf;

   use crate::types::{
       Action, Alias, Argument, Command, Declaration, Declared, Directory, Freshness, Label,
       NetworkAccess, OutputName, Provenance, Rule, Setting, SettingName, SettingValue, Target,
       TargetRole,
   };

   /// A label from known-valid text.
   pub(crate) fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   /// The build file of `directory`.
   pub(crate) fn file(directory: &str) -> Provenance {
       let directory = Directory::parse(directory).unwrap();
       let path = if directory.is_root() {
           PathBuf::from("build.lua")
       } else {
           PathBuf::from(format!("{directory}/build.lua"))
       };
       Provenance::new(path, directory)
   }

   /// A command from known-valid arguments.
   pub(crate) fn command(arguments: &[&str]) -> Command {
       Command::new(
           arguments
               .iter()
               .map(|argument| Argument::parse(*argument).unwrap())
               .collect(),
       )
       .unwrap()
   }

   /// `item`, declared by the build file of `directory`.
   pub(crate) fn declared_in(directory: &str, item: Declared) -> Declaration {
       Declaration::new(file(directory), item)
   }

   /// `item`, declared by the build file of its label's own directory.
   fn declared(label: &Label, item: Declared) -> Declaration {
       Declaration::new(file(label.directory().as_str()), item)
   }

   /// A build target running `cc`, with one output named after it, before `change` is applied.
   pub(crate) fn target_item(raw: &str, change: impl FnOnce(&mut Target)) -> Declared {
       let label = label(raw);
       let mut target = Target {
           outputs: vec![OutputName::parse(label.name().as_str()).unwrap()],
           label,
           role: TargetRole::Build,
           action: Action::Run(command(&["cc"])),
           inputs: Vec::new(),
           deps: Vec::new(),
           env: Vec::new(),
           network: NetworkAccess::Sealed,
           freshness: Freshness::Cached,
       };
       change(&mut target);
       Declared::Target(target)
   }

   /// A build target running `cc` and depending on `deps`, in that order.
   pub(crate) fn target(raw: &str, deps: &[&str]) -> Declaration {
       target_with(raw, |target| {
           target.deps = deps.iter().map(|dep| label(dep)).collect();
       })
   }

   /// A build target running `cc`, after `change` is applied.
   pub(crate) fn target_with(raw: &str, change: impl FnOnce(&mut Target)) -> Declaration {
       declared(&label(raw), target_item(raw, change))
   }

   /// A rule running `run`.
   pub(crate) fn rule(raw: &str, run: &[&str]) -> Declaration {
       let label = label(raw);
       declared(
           &label.clone(),
           Declared::Rule(Rule {
               label,
               run: command(run),
               description: None,
           }),
       )
   }

   /// An alias for `target`.
   pub(crate) fn alias(raw: &str, target: &str) -> Declaration {
       let alias_label = label(raw);
       declared(
           &alias_label.clone(),
           Declared::Alias(Alias {
               label: alias_label,
               target: label(target),
           }),
       )
   }

   /// A build setting with an empty default, declared by the root build file.
   pub(crate) fn setting(name: &str) -> Declaration {
       declared_in("", setting_item(name))
   }

   /// A build setting with an empty default.
   pub(crate) fn setting_item(name: &str) -> Declared {
       Declared::Setting(Setting {
           name: SettingName::parse(name).unwrap(),
           default: SettingValue::parse("").unwrap(),
       })
   }
   ```

3. Write the failing tests. Create `crates/buildl-core/src/resolve/index.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use super::{Item, index};
       use crate::error::{DeclaredKind, Error};
       use crate::resolve::fixtures::{
           alias, declared_in, label, rule, setting, setting_item, target, target_item,
       };
       use crate::types::{Provenance, SettingName};

       fn files(sites: &[Provenance]) -> Vec<&Path> {
           sites.iter().map(Provenance::file).collect()
       }

       #[test]
       fn every_declaration_is_found_under_its_name() {
           let found = index(vec![
               setting("test_filter"),
               alias("//:default", "//:app"),
               rule("//tools:cc", &["cc"]),
               target("//:app", &[]),
           ])
           .unwrap();
           let kinds: Vec<(String, DeclaredKind)> = found
               .labels
               .iter()
               .map(|(label, entry)| (label.to_string(), entry.item.kind()))
               .collect();
           assert_eq!(
               kinds,
               [
                   ("//:app".to_owned(), DeclaredKind::Target),
                   ("//:default".to_owned(), DeclaredKind::Alias),
                   ("//tools:cc".to_owned(), DeclaredKind::Rule),
               ]
           );
           assert_eq!(
               found.labels[&label("//tools:cc")].provenance.file(),
               Path::new("tools/build.lua")
           );
           assert!(matches!(
               found.labels[&label("//:app")].item,
               Item::Target(_)
           ));
           let default = &found.settings[&SettingName::parse("test_filter").unwrap()];
           assert_eq!(default.as_str(), "");
       }

       #[test]
       fn nothing_declared_indexes_to_nothing() {
           let found = index(Vec::new()).unwrap();
           assert!(found.labels.is_empty());
           assert!(found.settings.is_empty());
       }

       #[test]
       fn a_label_declared_twice_in_one_file_names_that_file_twice() {
           let err = index(vec![target("//:a", &[]), target("//:a", &[])]).unwrap_err();
           match err {
               Error::DuplicateLabel {
                   label: found,
                   sites,
               } => {
                   assert_eq!(found, label("//:a"));
                   assert_eq!(
                       files(&sites),
                       [Path::new("build.lua"), Path::new("build.lua")]
                   );
               }
               other => unreachable!("expected DuplicateLabel, got {other:?}"),
           }
       }

       #[test]
       fn a_label_declared_as_two_kinds_is_a_duplicate() {
           let err = index(vec![target("//:cc", &[]), rule("//:cc", &["cc"])]).unwrap_err();
           assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
           let err = index(vec![alias("//:x", "//:y"), rule("//:x", &["cc"])]).unwrap_err();
           assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
       }

       #[test]
       fn every_site_of_a_duplicate_is_named_in_the_declarations_own_order() {
           // Hand-built: Load cannot deliver one label from two files, but the index takes any
           // declarations and reports whatever sites they hold.
           let err = index(vec![
               declared_in("lib", target_item("//:a", |_| {})),
               declared_in("", target_item("//:a", |_| {})),
               declared_in("app", target_item("//:a", |_| {})),
           ])
           .unwrap_err();
           match err {
               Error::DuplicateLabel { sites, .. } => assert_eq!(
                   files(&sites),
                   [
                       Path::new("app/build.lua"),
                       Path::new("build.lua"),
                       Path::new("lib/build.lua"),
                   ]
               ),
               other => unreachable!("expected DuplicateLabel, got {other:?}"),
           }
       }

       #[test]
       fn the_lowest_duplicated_label_is_the_one_reported() {
           let declarations = vec![
               target("//:z", &[]),
               target("//:z", &[]),
               target("//:b", &[]),
               target("//:b", &[]),
               target("//:a", &[]),
           ];
           match index(declarations).unwrap_err() {
               Error::DuplicateLabel { label: found, .. } => assert_eq!(found, label("//:b")),
               other => unreachable!("expected DuplicateLabel, got {other:?}"),
           }
       }

       #[test]
       fn a_setting_declared_in_two_files_names_both() {
           let err = index(vec![
               declared_in("lib", setting_item("test_filter")),
               declared_in("", setting_item("test_filter")),
           ])
           .unwrap_err();
           match err {
               Error::DuplicateSetting { name, sites } => {
                   assert_eq!(name.as_str(), "test_filter");
                   assert_eq!(
                       files(&sites),
                       [Path::new("build.lua"), Path::new("lib/build.lua")]
                   );
               }
               other => unreachable!("expected DuplicateSetting, got {other:?}"),
           }
       }

       #[test]
       fn a_duplicated_label_is_reported_before_a_duplicated_setting() {
           let err = index(vec![
               setting("s"),
               setting("s"),
               target("//:a", &[]),
               target("//:a", &[]),
           ])
           .unwrap_err();
           assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
       }

       #[test]
       fn a_setting_and_a_label_may_share_a_name() {
           let found = index(vec![setting("app"), target("//:app", &[])]).unwrap();
           assert_eq!(found.labels.len(), 1);
           assert_eq!(found.settings.len(), 1);
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib resolve::index
   error[E0432]: unresolved imports `super::Item`, `super::index`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/resolve/index.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The index: every declaration found by the name it was declared under.
   //!
   //! Its own file because it is the first thing this phase does and the only place a name can
   //! collide: targets, rules and aliases share one namespace of labels, and build settings have
   //! their own.
   //!
   //! Responsibilities: [`Index`], [`Entry`], [`Item`], and [`index`], which refuses a name
   //! declared more than once.
   //!
   //! Non-responsibilities: what a declaration refers to. An index says what each name is, not
   //! whether the names it mentions exist.

   use std::collections::BTreeMap;

   use crate::error::{DeclaredKind, Error, Result};
   use crate::types::{
       Alias, Declaration, Declared, Label, Provenance, Rule, SettingName, SettingValue, Target,
   };

   /// What a label was declared as, holding the declaration itself.
   ///
   /// The four kinds a build file can declare, less the setting: a setting is named in its own
   /// namespace, so no label ever holds one. `DeclaredKind` is the kind alone, for an error to
   /// report.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) enum Item {
       /// A build or test target.
       Target(Target),
       /// A rule.
       Rule(Rule),
       /// An alias.
       Alias(Alias),
   }

   impl Item {
       /// Which kind of declaration this is.
       pub(crate) const fn kind(&self) -> DeclaredKind {
           match self {
               Self::Target(_) => DeclaredKind::Target,
               Self::Rule(_) => DeclaredKind::Rule,
               Self::Alias(_) => DeclaredKind::Alias,
           }
       }
   }

   /// One labelled declaration and the build file that made it.
   ///
   /// A `Declaration` narrowed to what a label can name, with its parts open to this phase.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) struct Entry {
       /// The build file.
       pub(crate) provenance: Provenance,
       /// What was declared.
       pub(crate) item: Item,
   }

   /// Every declaration of a workspace, each under the one name it was declared with.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) struct Index {
       /// Targets, rules and aliases, by label.
       pub(crate) labels: BTreeMap<Label, Entry>,
       /// Build settings and their defaults, by name.
       pub(crate) settings: BTreeMap<SettingName, SettingValue>,
   }

   /// Indexes `declarations` by name.
   ///
   /// The result does not depend on the order of `declarations`: they are sorted first, and both
   /// maps iterate in the order of their keys.
   ///
   /// # Errors
   ///
   /// Returns [`Error::DuplicateLabel`] for the lowest label declared more than once, in any mix
   /// of target, rule and alias. With no such label, returns [`Error::DuplicateSetting`] for the
   /// lowest setting name declared more than once. Either names the build file of every
   /// declaration of that name.
   pub(crate) fn index(mut declarations: Vec<Declaration>) -> Result<Index> {
       declarations.sort();

       let mut labels: BTreeMap<Label, Vec<Entry>> = BTreeMap::new();
       let mut settings: BTreeMap<SettingName, Vec<(Provenance, SettingValue)>> = BTreeMap::new();
       for declaration in declarations {
           let (provenance, declared) = declaration.into_parts();
           let (label, item) = match declared {
               Declared::Target(target) => (target.label.clone(), Item::Target(target)),
               Declared::Rule(rule) => (rule.label.clone(), Item::Rule(rule)),
               Declared::Alias(alias) => (alias.label.clone(), Item::Alias(alias)),
               Declared::Setting(setting) => {
                   settings
                       .entry(setting.name)
                       .or_default()
                       .push((provenance, setting.default));
                   continue;
               }
           };
           labels
               .entry(label)
               .or_default()
               .push(Entry { provenance, item });
       }

       Ok(Index {
           labels: unique(
               labels,
               |entry| &entry.provenance,
               |label, sites| Error::DuplicateLabel { label, sites },
           )?,
           settings: unique(
               settings,
               |(provenance, _)| provenance,
               |name, sites| Error::DuplicateSetting { name, sites },
           )?
           .into_iter()
           .map(|(name, (_, default))| (name, default))
           .collect(),
       })
   }

   /// Keeps the one declaration of each name, or fails on the lowest name declared more than once.
   fn unique<K: Ord, V>(
       grouped: BTreeMap<K, Vec<V>>,
       site: impl Fn(&V) -> &Provenance,
       duplicate: impl FnOnce(K, Vec<Provenance>) -> Error,
   ) -> Result<BTreeMap<K, V>> {
       let mut single = BTreeMap::new();
       for (name, mut declared) in grouped {
           if declared.len() > 1 {
               let sites = declared.iter().map(|entry| site(entry).clone()).collect();
               return Err(duplicate(name, sites));
           }
           if let Some(only) = declared.pop() {
               single.insert(name, only);
           }
       }
       Ok(single)
   }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::index
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 173 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

8. Commit:

   ```
   $ git add crates/buildl-core/src/resolve/fixtures.rs crates/buildl-core/src/resolve/index.rs crates/buildl-core/src/resolve/mod.rs
   $ git commit -m "feat(buildl-core): index declarations by name and refuse a duplicate"
   ```

### Task 3 — Resolve what each reference names

**Files:**
- Create `crates/buildl-core/src/resolve/references.rs`
- Modify `crates/buildl-core/src/resolve/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod cycle;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod index;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod references;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod suggest;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/resolve/references.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use super::{Holder, alias_target, dependency, rule, settings};
       use crate::error::{DeclarationField, DeclaredKind, Error};
       use crate::resolve::fixtures::{alias, command, file, label, rule as a_rule, setting, target};
       use crate::resolve::index::{Index, index};
       use crate::types::{Label, Provenance};

       /// `//:app` and `//lib:text` are targets, `//:default` an alias for `//:app`,
       /// `//tools:cc` a rule, and `test_filter` a setting.
       fn workspace() -> Index {
           index(vec![
               target("//:app", &[]),
               target("//lib:text", &[]),
               alias("//:default", "//:app"),
               a_rule("//tools:cc", &["cc"]),
               setting("test_filter"),
           ])
           .unwrap()
       }

       fn held_by<'a>(label: &'a Label, provenance: &'a Provenance) -> Holder<'a> {
           Holder { label, provenance }
       }

       /// Asserts `error` is a wrong-kind error for `reference` in `field`, and returns what was
       /// found and the file that declared it.
       fn wrong_kind(
           error: Error,
           field: DeclarationField,
           reference: &str,
       ) -> (DeclaredKind, String) {
           match error {
               Error::WrongReferenceKind {
                   site,
                   field: in_field,
                   reference: named,
                   found,
                   declared_in,
               } => {
                   assert_eq!(site.label, label("//:holder"));
                   assert_eq!(site.provenance.file(), Path::new("build.lua"));
                   assert_eq!(in_field, field);
                   assert_eq!(named, label(reference));
                   (found, declared_in.to_string())
               }
               other => unreachable!("expected WrongReferenceKind, got {other:?}"),
           }
       }

       /// Asserts `error` is an unknown-reference error in `field`, and returns its suggestion.
       fn unknown(error: Error, field: DeclarationField) -> Option<Label> {
           match error {
               Error::UnknownReference {
                   site,
                   field: in_field,
                   suggestion,
                   ..
               } => {
                   assert_eq!(site.label, label("//:holder"));
                   assert_eq!(in_field, field);
                   suggestion
               }
               other => unreachable!("expected UnknownReference, got {other:?}"),
           }
       }

       #[test]
       fn a_dependency_names_a_target_or_the_target_of_an_alias() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           assert_eq!(
               dependency(&index, holder, &label("//lib:text")).unwrap(),
               &label("//lib:text")
           );
           assert_eq!(
               dependency(&index, holder, &label("//:default")).unwrap(),
               &label("//:app")
           );
       }

       #[test]
       fn a_dependency_on_a_rule_is_the_wrong_kind() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let error = dependency(&index, held_by(&holder, &root), &label("//tools:cc")).unwrap_err();
           assert_eq!(
               wrong_kind(error, DeclarationField::Dep, "//tools:cc"),
               (DeclaredKind::Rule, "tools/build.lua".to_owned())
           );
       }

       #[test]
       fn an_unknown_dependency_is_offered_the_nearest_target_or_alias() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let error = dependency(&index, holder, &label("//:ap")).unwrap_err();
           assert_eq!(unknown(error, DeclarationField::Dep), Some(label("//:app")));
           let error = dependency(&index, holder, &label("//:defualt")).unwrap_err();
           assert_eq!(
               unknown(error, DeclarationField::Dep),
               Some(label("//:default"))
           );
           // A rule is near, but a dependency cannot name one, so it is not offered.
           let error = dependency(&index, holder, &label("//tools:c")).unwrap_err();
           assert_eq!(unknown(error, DeclarationField::Dep), None);
       }

       #[test]
       fn a_rule_reference_names_a_rule() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let found = rule(&index, held_by(&holder, &root), &label("//tools:cc")).unwrap();
           assert_eq!(found.run, command(&["cc"]));
       }

       #[test]
       fn a_rule_reference_to_a_target_or_an_alias_is_the_wrong_kind() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let error = rule(&index, holder, &label("//lib:text")).unwrap_err();
           assert_eq!(
               wrong_kind(error, DeclarationField::Rule, "//lib:text"),
               (DeclaredKind::Target, "lib/build.lua".to_owned())
           );
           let error = rule(&index, holder, &label("//:default")).unwrap_err();
           assert_eq!(
               wrong_kind(error, DeclarationField::Rule, "//:default"),
               (DeclaredKind::Alias, "build.lua".to_owned())
           );
       }

       #[test]
       fn an_unknown_rule_is_offered_the_nearest_rule_only() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let error = rule(&index, holder, &label("//tools:c")).unwrap_err();
           assert_eq!(
               unknown(error, DeclarationField::Rule),
               Some(label("//tools:cc"))
           );
           let error = rule(&index, holder, &label("//:ap")).unwrap_err();
           assert_eq!(unknown(error, DeclarationField::Rule), None);
       }

       #[test]
       fn an_alias_names_a_target() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           assert_eq!(
               alias_target(&index, held_by(&holder, &root), &label("//:app")).unwrap(),
               &label("//:app")
           );
       }

       #[test]
       fn an_alias_for_an_alias_or_a_rule_is_the_wrong_kind() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let error = alias_target(&index, holder, &label("//:default")).unwrap_err();
           assert_eq!(
               wrong_kind(error, DeclarationField::Target, "//:default"),
               (DeclaredKind::Alias, "build.lua".to_owned())
           );
           let error = alias_target(&index, holder, &label("//tools:cc")).unwrap_err();
           assert_eq!(
               wrong_kind(error, DeclarationField::Target, "//tools:cc"),
               (DeclaredKind::Rule, "tools/build.lua".to_owned())
           );
       }

       #[test]
       fn an_alias_for_nothing_is_offered_the_nearest_target_only() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let error = alias_target(&index, holder, &label("//:ap")).unwrap_err();
           assert_eq!(
               unknown(error, DeclarationField::Target),
               Some(label("//:app"))
           );
           let error = alias_target(&index, holder, &label("//:defualt")).unwrap_err();
           assert_eq!(unknown(error, DeclarationField::Target), None);
       }

       #[test]
       fn a_command_naming_only_declared_settings_passes() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           assert!(settings(&index, holder, &command(&["go", "-run=$opt:test_filter"])).is_ok());
           assert!(settings(&index, holder, &command(&["cc", "$in", "$opt:"])).is_ok());
       }

       #[test]
       fn the_first_undeclared_setting_is_reported_with_the_nearest_declared_one() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let holder = held_by(&holder, &root);
           let run = command(&[
               "go",
               "$opt:test_filter",
               "$opt:test_fliter.$opt:zzz",
               "$opt:yyy",
           ]);
           match settings(&index, holder, &run).unwrap_err() {
               Error::UnknownSetting {
                   site,
                   name,
                   suggestion,
               } => {
                   assert_eq!(site.label, label("//:holder"));
                   assert_eq!(site.provenance.file(), Path::new("build.lua"));
                   assert_eq!(name, "test_fliter");
                   assert_eq!(suggestion.unwrap().as_str(), "test_filter");
               }
               other => unreachable!("expected UnknownSetting, got {other:?}"),
           }
       }

       #[test]
       fn a_reference_too_long_to_be_a_setting_name_is_undeclared() {
           let index = workspace();
           let (holder, root) = (label("//:holder"), file(""));
           let long = "a".repeat(257);
           let run = command(&[&format!("$opt:{long}")]);
           match settings(&index, held_by(&holder, &root), &run).unwrap_err() {
               Error::UnknownSetting {
                   name, suggestion, ..
               } => {
                   assert_eq!(name, long);
                   assert!(suggestion.is_none());
               }
               other => unreachable!("expected UnknownSetting, got {other:?}"),
           }
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib resolve::references
   error[E0432]: unresolved imports `super::Holder`, `super::alias_target`, `super::dependency`, `super::rule`, `super::settings`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/resolve/references.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! What each reference in a declaration names.
   //!
   //! Its own file because every reference is answered the same way: look the name up in the index,
   //! accept the kinds the field may name, and otherwise say whether the name is missing or names
   //! the wrong kind of thing.
   //!
   //! Responsibilities: [`Holder`], and one function per kind of reference — [`dependency`],
   //! [`rule`], [`alias_target`] and [`settings`].
   //!
   //! Non-responsibilities: the order references are checked in, and what is built from the
   //! answers. Both belong to the function that walks the index.

   use crate::error::{DeclarationField, DeclarationSite, DeclaredKind, Error, Result};
   use crate::resolve::index::{Entry, Index, Item};
   use crate::resolve::suggest::nearest;
   use crate::types::{Command, Label, Provenance, Rule, SettingName};

   /// The declaration holding a reference: the one an error about that reference names.
   ///
   /// Borrowed from the index for the length of one check. `DeclarationSite` is the owned form an
   /// error carries.
   #[derive(Debug, Clone, Copy)]
   pub(crate) struct Holder<'a> {
       /// The declaration's label.
       pub(crate) label: &'a Label,
       /// The build file that declared it.
       pub(crate) provenance: &'a Provenance,
   }

   impl Holder<'_> {
       /// This declaration as an error names it.
       fn site(&self) -> DeclarationSite {
           DeclarationSite {
               label: self.label.clone(),
               provenance: self.provenance.clone(),
           }
       }
   }

   /// The label a `deps` entry resolves to: the target it names, or the one its alias names.
   ///
   /// The label an alias names is returned as the alias wrote it. Whether that label is a target
   /// is the alias's own reference to check, not the dependency's.
   ///
   /// # Errors
   ///
   /// Returns [`Error::WrongReferenceKind`] when `reference` names a rule, and
   /// [`Error::UnknownReference`] when it names nothing.
   pub(crate) fn dependency<'a>(
       index: &'a Index,
       holder: Holder<'_>,
       reference: &Label,
   ) -> Result<&'a Label> {
       const FIELD: DeclarationField = DeclarationField::Dep;
       match index.labels.get_key_value(reference) {
           Some((
               label,
               Entry {
                   item: Item::Target(_),
                   ..
               },
           )) => Ok(label),
           Some((
               _,
               Entry {
                   item: Item::Alias(alias),
                   ..
               },
           )) => Ok(&alias.target),
           Some((_, entry)) => Err(wrong_kind(holder, FIELD, reference, entry)),
           None => Err(unknown(
               index,
               holder,
               FIELD,
               reference,
               &[DeclaredKind::Target, DeclaredKind::Alias],
           )),
       }
   }

   /// The rule a target's `rule` reference names.
   ///
   /// # Errors
   ///
   /// Returns [`Error::WrongReferenceKind`] when `reference` names a target or an alias, and
   /// [`Error::UnknownReference`] when it names nothing.
   pub(crate) fn rule<'a>(
       index: &'a Index,
       holder: Holder<'_>,
       reference: &Label,
   ) -> Result<&'a Rule> {
       const FIELD: DeclarationField = DeclarationField::Rule;
       match index.labels.get(reference) {
           Some(Entry {
               item: Item::Rule(rule),
               ..
           }) => Ok(rule),
           Some(entry) => Err(wrong_kind(holder, FIELD, reference, entry)),
           None => Err(unknown(
               index,
               holder,
               FIELD,
               reference,
               &[DeclaredKind::Rule],
           )),
       }
   }

   /// The target an alias names.
   ///
   /// # Errors
   ///
   /// Returns [`Error::WrongReferenceKind`] when `reference` names another alias or a rule, and
   /// [`Error::UnknownReference`] when it names nothing.
   pub(crate) fn alias_target<'a>(
       index: &'a Index,
       holder: Holder<'_>,
       reference: &Label,
   ) -> Result<&'a Label> {
       const FIELD: DeclarationField = DeclarationField::Target;
       match index.labels.get_key_value(reference) {
           Some((
               label,
               Entry {
                   item: Item::Target(_),
                   ..
               },
           )) => Ok(label),
           Some((_, entry)) => Err(wrong_kind(holder, FIELD, reference, entry)),
           None => Err(unknown(
               index,
               holder,
               FIELD,
               reference,
               &[DeclaredKind::Target],
           )),
       }
   }

   /// Checks that every `$opt:name` in `command` names a declared build setting.
   ///
   /// References are checked in argument order, then in the order they appear in an argument.
   ///
   /// # Errors
   ///
   /// Returns [`Error::UnknownSetting`] for the first reference that names no declared setting.
   pub(crate) fn settings(index: &Index, holder: Holder<'_>, command: &Command) -> Result<()> {
       for argument in command.arguments() {
           for name in argument.setting_references() {
               let declared =
                   SettingName::parse(name).is_ok_and(|setting| index.settings.contains_key(&setting));
               if !declared {
                   return Err(Error::UnknownSetting {
                       site: Box::new(holder.site()),
                       name: name.to_owned(),
                       suggestion: nearest(name, index.settings.keys()).cloned(),
                   });
               }
           }
       }
       Ok(())
   }

   /// The error for a reference to a declaration its field cannot name.
   fn wrong_kind(
       holder: Holder<'_>,
       field: DeclarationField,
       reference: &Label,
       entry: &Entry,
   ) -> Error {
       Error::WrongReferenceKind {
           site: Box::new(holder.site()),
           field,
           reference: reference.clone(),
           found: entry.item.kind(),
           declared_in: entry.provenance.clone(),
       }
   }

   /// The error for a reference to nothing, offering the nearest label of a kind the field may
   /// name.
   fn unknown(
       index: &Index,
       holder: Holder<'_>,
       field: DeclarationField,
       reference: &Label,
       may_name: &[DeclaredKind],
   ) -> Error {
       let candidates = index
           .labels
           .iter()
           .filter(|(_, entry)| may_name.contains(&entry.item.kind()))
           .map(|(label, _)| label);
       Error::UnknownReference {
           site: Box::new(holder.site()),
           field,
           reference: reference.clone(),
           suggestion: nearest(&reference.to_string(), candidates).cloned(),
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::references
   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 182 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

7. Commit:

   ```
   $ git add crates/buildl-core/src/resolve/references.rs crates/buildl-core/src/resolve/mod.rs
   $ git commit -m "feat(buildl-core): resolve what each reference names"
   ```

8. **Checkpoint.** Stop here for review before task 4.

### Task 4 — Assemble the target graph

**Files:**
- Create `crates/buildl-core/src/resolve/assembly.rs`
- Modify `crates/buildl-core/src/resolve/mod.rs`
- Modify `crates/buildl-core/src/types/target_graph.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Give the module its final form: every file declared without the temporary attribute, and `resolve` re-exported. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: [`resolve`].
   //!
   //! Non-responsibilities: evaluating build files and validating one declaration on its own,
   //! which the phase before this one does; and anything about whether a target needs to run.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod assembly;
   mod cycle;
   mod index;
   mod references;
   mod suggest;

   #[cfg(test)]
   mod fixtures;

   pub use assembly::resolve;
   ```

2. Re-export the function from the crate root, in `crates/buildl-core/src/lib.rs`. Insert:

   ```rust
   pub use resolve::resolve;
   ```

   immediately after:

   ```rust
   pub use ports::{Clock, DeclarationSource, Ports};
   ```

3. `TargetGraph::new` is about to have a caller. In `crates/buildl-core/src/types/target_graph.rs`, remove its temporary attribute. Replace:

   ```rust
       #[cfg_attr(
           not(test),
           expect(
               dead_code,
               reason = "called by the phase that builds the graph, which is not written yet"
           )
       )]
       pub(crate) fn new(
   ```

   with:

   ```rust
       pub(crate) fn new(
   ```

4. In the same file, link the type's doc to the function. Replace:

   ```rust
   /// files were evaluated in. A graph is built by the phase that resolves declarations, which
   ```

   with:

   ```rust
   /// files were evaluated in. A graph is obtained from [`resolve`](fn@crate::resolve), which
   ```

5. Write the failing tests. Create `crates/buildl-core/src/resolve/assembly.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use super::{node_id, resolve, without_repeats};
       use crate::error::{DeclarationField, Error};
       use crate::resolve::fixtures::{
           alias, command, declared_in, label, rule, setting, setting_item, target, target_item,
           target_with,
       };
       use crate::types::{
           Action, Declaration, Declared, Description, EnvName, NodeId, OutputName, Rule, SourcePath,
           TargetGraph,
       };

       fn id(index: u32) -> NodeId {
           NodeId::new(index)
       }

       /// A rule running `run`, shown as `description` while it runs.
       fn described_rule(raw: &str, run: &[&str], description: &str) -> Declaration {
           let label = label(raw);
           declared_in(
               label.directory().as_str(),
               Declared::Rule(Rule {
                   label: label.clone(),
                   run: command(run),
                   description: Some(Description::parse(description).unwrap()),
               }),
           )
       }

       /// The labels `label` depends on in `graph`, in edge order.
       fn deps_of(graph: &TargetGraph, raw: &str) -> Vec<String> {
           let id = graph.id_of(&label(raw)).unwrap();
           graph
               .deps(id)
               .unwrap()
               .iter()
               .map(|dep| graph.node(*dep).unwrap().label.to_string())
               .collect()
       }

       fn labels(graph: &TargetGraph) -> Vec<String> {
           graph
               .nodes()
               .iter()
               .map(|node| node.label.to_string())
               .collect()
       }

       #[test]
       fn nothing_declared_resolves_to_an_empty_graph() {
           assert!(resolve(Vec::new()).unwrap().is_empty());
       }

       #[test]
       fn targets_are_numbered_in_label_order_and_rules_and_aliases_are_not_nodes() {
           let graph = resolve(vec![
               target("//lib:text", &[]),
               rule("//:cc", &["cc"]),
               alias("//:default", "//:app"),
               target("//:app", &["//lib:text"]),
               setting("test_filter"),
           ])
           .unwrap();
           assert_eq!(labels(&graph), ["//:app", "//lib:text"]);
           assert_eq!(graph.id_of(&label("//:app")), Some(id(0)));
           assert_eq!(graph.id_of(&label("//:default")), Some(id(0)));
           assert_eq!(graph.id_of(&label("//:cc")), None);
           assert_eq!(graph.dependents(id(1)), Some(&[id(0)][..]));
           assert_eq!(graph.settings().len(), 1);
           assert_eq!(
               graph.node(id(1)).unwrap().provenance.file(),
               Path::new("lib/build.lua")
           );
       }

       #[test]
       fn a_target_naming_a_rule_runs_the_rules_command_under_its_description() {
           let graph = resolve(vec![
               described_rule("//tools:cc", &["cc", "-c", "$in"], "compile $in"),
               target_with("//:main.o", |target| {
                   target.action = Action::UseRule(label("//tools:cc"));
               }),
               target("//:plain", &[]),
           ])
           .unwrap();
           let main = graph
               .node(graph.id_of(&label("//:main.o")).unwrap())
               .unwrap();
           assert_eq!(main.run, command(&["cc", "-c", "$in"]));
           assert_eq!(
               main.description.as_ref().map(Description::as_str),
               Some("compile $in")
           );
           let plain = graph
               .node(graph.id_of(&label("//:plain")).unwrap())
               .unwrap();
           assert_eq!(plain.run, command(&["cc"]));
           assert!(plain.description.is_none());
       }

       #[test]
       fn dependencies_keep_their_written_order_and_lose_their_repeats() {
           let graph = resolve(vec![
               target("//:a", &[]),
               target("//:b", &[]),
               target("//:c", &[]),
               target("//:app", &["//:c", "//:a", "//:c", "//:b", "//:a"]),
           ])
           .unwrap();
           assert_eq!(deps_of(&graph, "//:app"), ["//:c", "//:a", "//:b"]);
       }

       #[test]
       fn a_dependency_on_an_alias_is_an_edge_to_the_target_it_names() {
           let graph = resolve(vec![
               target("//:app", &[]),
               alias("//:default", "//:app"),
               target("//:test", &["//:default", "//:app"]),
           ])
           .unwrap();
           assert_eq!(deps_of(&graph, "//:test"), ["//:app"]);
       }

       #[test]
       fn a_nodes_lists_keep_their_written_order_and_lose_their_repeats() {
           let graph = resolve(vec![target_with("//:app", |target| {
               target.inputs = ["z.c", "a.c", "z.c"]
                   .map(|path| SourcePath::parse(path).unwrap())
                   .to_vec();
               target.outputs = ["out.b", "out.a", "out.b"]
                   .map(|name| OutputName::parse(name).unwrap())
                   .to_vec();
               target.env = ["PATH", "HOME", "PATH"]
                   .map(|name| EnvName::parse(name).unwrap())
                   .to_vec();
           })])
           .unwrap();
           let node = &graph.nodes()[0];
           let inputs: Vec<&str> = node.inputs.iter().map(SourcePath::as_str).collect();
           let outputs: Vec<&str> = node.outputs.iter().map(OutputName::as_str).collect();
           let env: Vec<&str> = node.env.iter().map(EnvName::as_str).collect();
           assert_eq!(inputs, ["z.c", "a.c"]);
           assert_eq!(outputs, ["out.b", "out.a"]);
           assert_eq!(env, ["PATH", "HOME"]);
       }

       #[test]
       fn without_repeats_keeps_the_first_of_each() {
           assert_eq!(without_repeats(&[3, 1, 3, 2, 1]), [3, 1, 2]);
           assert_eq!(without_repeats::<u8>(&[]), [0_u8; 0]);
       }

       #[test]
       fn the_order_of_the_declarations_does_not_change_the_graph() {
           let declarations = || {
               vec![
                   target("//:app", &["//lib:text", "//:main.o"]),
                   target("//:main.o", &[]),
                   target("//lib:text", &[]),
                   alias("//:default", "//:app"),
                   rule("//:cc", &["cc", "$opt:mode"]),
                   setting("mode"),
               ]
           };
           let forward = resolve(declarations()).unwrap();
           let mut reversed = declarations();
           reversed.reverse();
           assert_eq!(resolve(reversed).unwrap(), forward);
       }

       #[test]
       fn a_duplicate_is_reported_before_a_bad_reference() {
           let err = resolve(vec![
               target("//:app", &["//:nope"]),
               target("//:z", &[]),
               target("//:z", &[]),
           ])
           .unwrap_err();
           assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
       }

       #[test]
       fn a_bad_reference_is_reported_before_a_cycle() {
           let err = resolve(vec![
               target("//:a", &["//:b"]),
               target("//:b", &["//:a"]),
               target("//:z", &["//:nope"]),
           ])
           .unwrap_err();
           assert!(matches!(err, Error::UnknownReference { .. }), "{err:?}");
       }

       #[test]
       fn the_bad_reference_of_the_lowest_label_is_the_one_reported() {
           let err = resolve(vec![
               target("//:z", &["//:nope"]),
               alias("//:m", "//:nope"),
               target("//:b", &["//:missing"]),
           ])
           .unwrap_err();
           match err {
               Error::UnknownReference {
                   site, reference, ..
               } => {
                   assert_eq!(site.label, label("//:b"));
                   assert_eq!(reference, label("//:missing"));
               }
               other => unreachable!("expected UnknownReference, got {other:?}"),
           }
       }

       #[test]
       fn within_a_target_the_rule_then_the_deps_then_its_own_settings_are_checked() {
           // A wrong rule reference wins over a wrong dependency.
           let err = resolve(vec![target_with("//:app", |target| {
               target.action = Action::UseRule(label("//:no-rule"));
               target.deps = vec![label("//:no-dep")];
           })])
           .unwrap_err();
           match err {
               Error::UnknownReference { field, .. } => assert_eq!(field, DeclarationField::Rule),
               other => unreachable!("expected UnknownReference, got {other:?}"),
           }
           // A wrong dependency wins over an undeclared setting, and the first written dependency
           // over a later one.
           let err = resolve(vec![target_with("//:app", |target| {
               target.action = Action::Run(command(&["go", "$opt:nope"]));
               target.deps = vec![label("//:second"), label("//:first")];
           })])
           .unwrap_err();
           match err {
               Error::UnknownReference { reference, .. } => assert_eq!(reference, label("//:second")),
               other => unreachable!("expected UnknownReference, got {other:?}"),
           }
           // With its dependencies right, the target's own command is checked.
           let err = resolve(vec![target_with("//:app", |target| {
               target.action = Action::Run(command(&["go", "$opt:nope"]));
           })])
           .unwrap_err();
           assert!(matches!(err, Error::UnknownSetting { .. }), "{err:?}");
       }

       #[test]
       fn a_rules_command_is_checked_once_at_the_rule_even_with_no_user() {
           let err = resolve(vec![rule("//tools:go", &["go", "$opt:nope"])]).unwrap_err();
           match err {
               Error::UnknownSetting { site, name, .. } => {
                   assert_eq!(site.label, label("//tools:go"));
                   assert_eq!(site.provenance.file(), Path::new("tools/build.lua"));
                   assert_eq!(name, "nope");
               }
               other => unreachable!("expected UnknownSetting, got {other:?}"),
           }
       }

       #[test]
       fn a_target_using_a_rule_adds_no_settings_check_of_its_own() {
           // `//:a` sorts before the rule: were the rule's command checked at its user, the error
           // would name `//:a`.
           let err = resolve(vec![
               target_with("//:a", |target| {
                   target.action = Action::UseRule(label("//:go"));
               }),
               rule("//:go", &["go", "$opt:nope"]),
           ])
           .unwrap_err();
           match err {
               Error::UnknownSetting { site, .. } => assert_eq!(site.label, label("//:go")),
               other => unreachable!("expected UnknownSetting, got {other:?}"),
           }
       }

       #[test]
       fn an_alias_for_something_that_is_not_a_target_is_the_aliases_own_error() {
           let err = resolve(vec![target("//:app", &["//:x"]), alias("//:x", "//:nope")]).unwrap_err();
           match err {
               Error::UnknownReference { site, field, .. } => {
                   assert_eq!(site.label, label("//:x"));
                   assert_eq!(field, DeclarationField::Target);
               }
               other => unreachable!("expected UnknownReference, got {other:?}"),
           }
       }

       fn cycle_path(err: Error) -> Vec<(String, String)> {
           match err {
               Error::DependencyCycle { path } => path
                   .iter()
                   .map(|site| (site.label.to_string(), site.provenance.to_string()))
                   .collect(),
               other => unreachable!("expected DependencyCycle, got {other:?}"),
           }
       }

       fn step(label: &str, file: &str) -> (String, String) {
           (label.to_owned(), file.to_owned())
       }

       #[test]
       fn a_cycle_names_every_target_on_it_and_its_build_file_lowest_label_first() {
           let err = resolve(vec![
               target("//lib:b", &["//:c"]),
               target("//:c", &["//:a"]),
               target("//:a", &["//lib:b"]),
               target("//:free", &["//:a"]),
           ])
           .unwrap_err();
           assert_eq!(
               cycle_path(err),
               [
                   step("//:a", "build.lua"),
                   step("//lib:b", "lib/build.lua"),
                   step("//:c", "build.lua"),
               ]
           );
       }

       #[test]
       fn a_target_depending_on_itself_is_a_cycle_of_one() {
           let err = resolve(vec![target("//:a", &["//:a"])]).unwrap_err();
           assert_eq!(cycle_path(err), [step("//:a", "build.lua")]);
       }

       #[test]
       fn a_cycle_closed_through_an_alias_names_the_targets_only() {
           let err = resolve(vec![target("//:a", &["//:x"]), alias("//:x", "//:a")]).unwrap_err();
           assert_eq!(cycle_path(err), [step("//:a", "build.lua")]);
       }

       #[test]
       fn a_duplicate_across_hand_built_files_names_each_file() {
           let err = resolve(vec![
               declared_in("lib", target_item("//:a", |_| {})),
               declared_in("", target_item("//:a", |_| {})),
               declared_in("", setting_item("s")),
           ])
           .unwrap_err();
           match err {
               Error::DuplicateLabel {
                   label: found,
                   sites,
               } => {
                   assert_eq!(found, label("//:a"));
                   assert_eq!(sites.len(), 2);
               }
               other => unreachable!("expected DuplicateLabel, got {other:?}"),
           }
       }

       #[test]
       fn a_position_within_u32_is_an_id() {
           assert_eq!(node_id(0, 1).unwrap(), id(0));
           let last = usize::try_from(u32::MAX).unwrap();
           assert_eq!(node_id(last, last + 1).unwrap(), id(u32::MAX));
       }

       #[cfg(target_pointer_width = "64")]
       #[test]
       fn a_position_past_u32_is_too_many_targets() {
           let past = usize::try_from(u32::MAX).unwrap() + 1;
           match node_id(past, past + 1).unwrap_err() {
               Error::TooManyTargets { count } => assert_eq!(count, 4_294_967_297),
               other => unreachable!("expected TooManyTargets, got {other:?}"),
           }
       }
   }
   ```

6. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib resolve::assembly
   error[E0432]: unresolved imports `super::node_id`, `super::resolve`, `super::without_repeats`
   error[E0432]: unresolved import `assembly::resolve`
   ```

7. Replace the placeholder line at the top of `crates/buildl-core/src/resolve/assembly.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ````rust
   //! The assembly: from every declaration to one target graph.
   //!
   //! Its own file because it is where this phase's steps meet and where their order is fixed:
   //! index the names, number the targets, check every reference while building nodes and edges,
   //! then search the edges for a cycle.
   //!
   //! Responsibilities: [`resolve`], and the private steps that number targets and build one node.
   //!
   //! Non-responsibilities: what a name collision is, what a reference may name, and how a cycle
   //! is found. Each has its own sibling file.

   use std::collections::{BTreeMap, BTreeSet};

   use crate::error::{DeclarationSite, Error, Result};
   use crate::resolve::cycle;
   use crate::resolve::index::{Index, Item, index};
   use crate::resolve::references::{self, Holder};
   use crate::types::{Action, Declaration, Label, Node, NodeId, Target, TargetGraph};

   /// Builds the target graph of a workspace from the declarations of its build files.
   ///
   /// Every build or test target becomes a node, numbered in label order, holding the command it
   /// runs: its own, or a copy of the rule it names. Every dependency becomes an edge, in the order
   /// it was written; a dependency on an alias becomes an edge to the target the alias names.
   /// Aliases and build settings are kept beside the nodes. The result does not depend on the order
   /// of `declarations`.
   ///
   /// # Errors
   ///
   /// Returns the first failure met, in this order:
   ///
   /// 1. [`Error::DuplicateLabel`], then [`Error::DuplicateSetting`], for a name declared more
   ///    than once;
   /// 2. [`Error::TooManyTargets`] when the targets outnumber the ids a graph has;
   /// 3. walking the declarations in label order, the first reference that is wrong:
   ///    [`Error::UnknownReference`], [`Error::WrongReferenceKind`] or [`Error::UnknownSetting`].
   ///    Within a target the rule reference is checked first, then its dependencies in written
   ///    order, then the settings its own command names;
   /// 4. [`Error::DependencyCycle`] when the dependencies loop.
   ///
   /// # Examples
   ///
   /// ```
   /// use buildl_core::{Label, resolve};
   ///
   /// let graph = resolve(Vec::new())?;
   /// assert!(graph.is_empty());
   /// assert_eq!(graph.id_of(&Label::parse("//:app")?), None);
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   pub fn resolve(declarations: Vec<Declaration>) -> Result<TargetGraph> {
       let index = index(declarations)?;
       let ids = number(&index)?;

       let mut nodes = Vec::with_capacity(ids.len());
       let mut edges = Vec::with_capacity(ids.len());
       let mut aliases = BTreeMap::new();
       for (label, entry) in &index.labels {
           let holder = Holder {
               label,
               provenance: &entry.provenance,
           };
           match &entry.item {
               Item::Target(target) => {
                   let (node, deps) = node(&index, &ids, holder, target)?;
                   nodes.push(node);
                   edges.push(deps);
               }
               Item::Rule(rule) => references::settings(&index, holder, &rule.run)?,
               Item::Alias(alias) => {
                   let target = references::alias_target(&index, holder, &alias.target)?;
                   if let Some(id) = ids.get(target) {
                       aliases.insert(label.clone(), *id);
                   }
               }
           }
       }

       if let Some(cycle) = cycle::find(&edges) {
           let path = cycle
               .iter()
               .filter_map(|id| nodes.get(id.index()))
               .map(|node| DeclarationSite {
                   label: node.label.clone(),
                   provenance: node.provenance.clone(),
               })
               .collect();
           return Err(Error::DependencyCycle { path });
       }
       Ok(TargetGraph::new(nodes, edges, aliases, index.settings))
   }

   /// Numbers the targets of `index` in label order.
   fn number(index: &Index) -> Result<BTreeMap<&Label, NodeId>> {
       let targets = index
           .labels
           .iter()
           .filter(|(_, entry)| matches!(entry.item, Item::Target(_)))
           .map(|(label, _)| label);
       let count = targets.clone().count();
       targets
           .enumerate()
           .map(|(position, label)| Ok((label, node_id(position, count)?)))
           .collect()
   }

   /// The id of the target at `position` among `count` targets.
   fn node_id(position: usize, count: usize) -> Result<NodeId> {
       u32::try_from(position)
           .map(NodeId::new)
           .map_err(|_| Error::TooManyTargets { count })
   }

   /// Builds the node of `target` and the ids it depends on, checking each of its references.
   fn node(
       index: &Index,
       ids: &BTreeMap<&Label, NodeId>,
       holder: Holder<'_>,
       target: &Target,
   ) -> Result<(Node, Vec<NodeId>)> {
       let (run, description, own_command) = match &target.action {
           Action::UseRule(reference) => {
               let rule = references::rule(index, holder, reference)?;
               (&rule.run, rule.description.as_ref(), None)
           }
           Action::Run(command) => (command, None, Some(command)),
       };

       let mut deps = Vec::new();
       let mut seen = BTreeSet::new();
       for reference in &target.deps {
           let resolved = references::dependency(index, holder, reference)?;
           if let Some(id) = ids.get(resolved)
               && seen.insert(*id)
           {
               deps.push(*id);
           }
       }

       if let Some(command) = own_command {
           references::settings(index, holder, command)?;
       }

       let node = Node {
           label: target.label.clone(),
           provenance: holder.provenance.clone(),
           role: target.role,
           run: run.clone(),
           description: description.cloned(),
           inputs: without_repeats(&target.inputs),
           outputs: without_repeats(&target.outputs),
           env: without_repeats(&target.env),
           network: target.network,
           freshness: target.freshness,
       };
       Ok((node, deps))
   }

   /// `items` in the order given, keeping the first occurrence of each.
   fn without_repeats<T: Ord + Clone>(items: &[T]) -> Vec<T> {
       let mut seen = BTreeSet::new();
       items
           .iter()
           .filter(|item| seen.insert(*item))
           .cloned()
           .collect()
   }
   ````

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::assembly
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 194 filtered out
   ```

9. Run the whole phase's tests:

   ```
   $ cargo test -p buildl-core --lib resolve
   test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 152 filtered out
   ```

10. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

11. Commit:

   ```
   $ git add crates/buildl-core/src/resolve/assembly.rs crates/buildl-core/src/resolve/mod.rs crates/buildl-core/src/types/target_graph.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): resolve declarations into a target graph"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- No `expect(dead_code)` remains under `crates/buildl-core/src`:
  `grep -rn 'dead_code' crates/buildl-core/src` prints nothing.
- Every cell of spec §4.3's reference table has a test in `resolve/references.rs`; the order of
  failure across steps and within one target has tests in `resolve/assembly.rs`. Spec §9 lists
  two of those under `resolve/references.rs` — the order within a declaration, and a rule's
  `$opt:` checked without a user. Both are tested in `resolve/assembly.rs`, where that order is
  decided.
- The same declarations in reverse order resolve to an equal graph.
