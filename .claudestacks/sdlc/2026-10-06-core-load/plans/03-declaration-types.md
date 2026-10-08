---
status: done
created: 2026-10-06
depends-on: [01, 02]
---

# Declaration Types Implementation Plan

**Goal:** Every value crossing Load's two boundaries — staged in from the port, declared out to the next phase — exists as a type in `buildl-core`.

**Architecture:** Two files in `crates/buildl-core/src/types/`, one per side of Load.

```text
  DeclarationSource ──► Evaluated ──► (Load, plan 06) ──► Vec<Declaration>
     (plan 05)          build_file.rs                      declaration.rs
                        Written<T> text, DeclarationOrder  validated domain values, no order
                        no serde, no Ord                   serde + total order
```

`declaration.rs` is Load's output. `Declaration` is a private-field header (`Provenance`) over the
closed sum `Declared` of four pub-field records; everything derives `Serialize`, `Deserialize` and
`Ord`, so the handoff serializes through the canonical serializer and sorts. It carries no
`DeclarationOrder`: a `pairs` loop reorders calls between evaluations, so an order field would make
two correct runs compare unequal. `build_file.rs` is the port's vocabulary: `BuildFile` wraps a
`Provenance`, `Evaluated` is `Staged(StagedFile) | Absent`, and the staged mirror keeps every textual
value as `Written<T>` plus the call's `DeclarationOrder`, which exists only to locate Load's errors.
The three two-state flag enums live in `declaration.rs`; `build_file.rs` imports them through
`crate::types`, so both sides share one type per flag. The only logic in either file is
`Declared::name`, `DeclarationOrder`'s `Display`, and accessors.

| Type | File | Derives | Task |
|---|---|---|---|
| `TargetRole`, `NetworkAccess`, `Freshness` | `declaration.rs` | `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize` | 1 |
| `Action`, `Target` | `declaration.rs` | same, minus `Copy` | 2 |
| `Rule`, `Alias` | `declaration.rs` | same, minus `Copy` | 3 |
| `Setting`, `Declared` + `name()` | `declaration.rs` | same, minus `Copy` | 4 |
| `Declaration` | `declaration.rs` | same, minus `Copy` | 5 |
| `DeclarationOrder` | `build_file.rs` | `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash` + `Display` | 6 |
| `BuildFile` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 7 |
| `StagedTarget`, `StagedRule` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 8 |
| `StagedAlias`, `StagedSetting` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 9 |
| `StagedItem`, `StagedDeclaration` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 10 |
| `StagedSubdir`, `StagedFile` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 11 |
| `Evaluated` | `build_file.rs` | `Debug, Clone, PartialEq, Eq` | 12 |

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` 1.0 with `derive`, `serde_json` (tests
only here), `cargo-make`.

**Content authority:** spec §3.2 (`DeclarationOrder`, the flag enums), §3.3 (`Declaration`), §3.4
(the port's values), §7 (which file holds which type), §8 (the testing rows for new types).

---

## Context an implementer needs

**What plans 01 and 02 leave.** `crates/buildl-core/src/types/` holds `SourcePath`, `OutputName`,
`EnvName`, `Argument` and `Command` (`argument.rs`), `Description`, `SettingName` and
`SettingValue` (`setting.rs`), `EntryName`, `FieldName`, `Diagnostic`, the private `grammar`
module, and `Written<T>` (`written.rs`), all re-exported from `types/mod.rs` and `lib.rs`.
`Command` serializes as a JSON array of its arguments; `Label` as its `//dir:name` string;
`Provenance` as `{"directory": …, "file": …}`. `Written<T>` derives `Debug, Clone, PartialEq, Eq,
PartialOrd, Ord, Hash` and has no serde. In `types/mod.rs` the re-export block opens
`pub use argument::{Argument, Command};` and the next line is `pub use description::Description;`
— every task below replaces what sits between those two lines. `crates/buildl-core/src/lib.rs`
ends with this block, which task 1 replaces first:

```rust
pub use types::{
    Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName,
    Label, NodeId, OutputName, Provenance, SettingName, SettingValue, SourcePath, TargetName,
    Timestamp, Written,
};
```

`crates/buildl-core/Cargo.toml` already depends on `serde` and `serde_json`, and
`crates/expected-edges.txt` already records both — **this plan changes neither file**.

**Gate facts that bite here** (`cargo make dod`: fmt-check, clippy `-D warnings` with the two
architecture guards, rustdoc `-D warnings`, tests, doctests):

| Fact | Consequence in this plan |
|---|---|
| `missing_docs` warns | every `pub` type, field and variant below carries a doc comment |
| `must_use_candidate` (pedantic) | every value-returning `pub fn` is `#[must_use]` |
| `missing_const_for_fn` (nursery) | `Declaration::new`/`provenance`/`item`, `DeclarationOrder::new`/`get`, `BuildFile::new`/`provenance`/`directory` are `const fn`; `BuildFile::file` and `Declared::name` cannot be and are not |
| `doc_markdown` | bare identifiers in docs are backticked (`b.target`, `pairs`) |
| `unwrap_used` denied | a test module opens with the `#![expect(clippy::unwrap_used, …)]` header **only** once it unwraps; an unfulfilled `expect` warns. Tasks 1 and 6 create test modules that do not unwrap, so they carry no header; tasks 4 and 7 add it |
| rustdoc `-D warnings` fails broken intra-doc links | each module doc's `Responsibilities:` paragraph names only items already in the file, so every task rewrites it; task 5 and task 12 land the final wording |
| `guard-core-purity` greps for `clippy::(disallowed_|all\b|style\b)` | none of those strings appears; no `#[allow]` anywhere |

**How each task goes red.** Types with behaviour — the flag ordering, `Declared::name`,
`Declaration`'s accessors and JSON shape, `DeclarationOrder`, `BuildFile` — go red on a test.
`Action`, `Target`, `Rule`, `Alias` and the staged records are derive-only data: their only
behaviour is generated, so under the unit-test mandate they need no test of their own (`Target`,
`Rule`, `Alias` and `Action` are exercised by tasks 4 and 5's tests; the staged records by plan 06's
flows). Their red step is the re-export: a `pub use` of an item that does not exist yet fails with
`error[E0432]`.

**Not in this plan.** The `types/mod.rs` and `lib.rs` doc prose is unchanged — plan 08 adds the
`Declaration`/`BuildFile` bullets to the `types/mod.rs` index. Error variants that hold a
`DeclarationOrder` or a `Declaration` are plan 04's; the port trait that returns `Evaluated` is plan
05's.

**Canonical JSON.** The serializer is `crate::json::canonical::to_string`; `crate::json` re-exports
nothing, so `crate::json::to_string` does not resolve.

**Test counts.** The `running N tests` lines below assume plans 01 and 02 landed as written, which
leaves `buildl-core`'s unit-test binary at 105. This plan adds 6 (→ 111) and no doctest. If the
baseline differs, the delta per task is what to check: +1, +0, +0, +1, +2, +1, +1, +0, +0, +0, +0, +0.

Run `cargo fmt --all` before every `cargo make dod`. Every Rust block below is in rustfmt's form,
but retyping can drift, and fmt-check is the gate's first step. One commit per task, Conventional
Commits, scope `buildl-core`.

### File map

```
crates/buildl-core/src/types/declaration.rs — [create] flags, Action, Target, Rule, Alias, Setting,
                                              Declared, Declaration and their tests (tasks 1–5)
crates/buildl-core/src/types/build_file.rs  — [create] DeclarationOrder, BuildFile, the staged
                                              mirror, Evaluated and their tests (tasks 6–12)
crates/buildl-core/src/types/mod.rs         — [modify] `pub mod` + `pub use` lines (every task)
crates/buildl-core/src/lib.rs               — [modify] the `pub use types::{…}` block (every task)
```

---

## Task 1 — Add the three two-state flags

**Files:**
- Create `crates/buildl-core/src/types/declaration.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/types/declaration.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       use super::{Freshness, NetworkAccess, TargetRole};

       #[test]
       fn flags_order_their_default_first() {
           assert!(TargetRole::Build < TargetRole::Test);
           assert!(NetworkAccess::Sealed < NetworkAccess::Declared);
           assert!(Freshness::Cached < Freshness::Always);
       }
   }
   ```

   The test pins the variant order: derived `Ord` follows declaration order, and each flag's
   default is declared first, so the default sorts first.

2. Wire the module. In `crates/buildl-core/src/types/mod.rs` add `pub mod declaration;` between
   `pub mod argument;` and `pub mod description;`, and between `pub use argument::{Argument, Command};`
   and `pub use description::Description;` add:

   ```rust
   pub use declaration::{Freshness, NetworkAccess, TargetRole};
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName,
       Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, SettingName, SettingValue,
       SourcePath, TargetName, TargetRole, Timestamp, Written,
   };
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `declaration::Freshness`, `declaration::NetworkAccess`, `declaration::TargetRole`
     --> crates/buildl-core/src/types/mod.rs:…
   error[E0432]: unresolved imports `super::Freshness`, `super::NetworkAccess`, `super::TargetRole`
    --> crates/buildl-core/src/types/declaration.rs:5:17
   ```

4. Replace the whole file with the module doc, the three enums and the same test module:

   ```rust
   //! What a build file declares, once every value in it has been validated.
   //!
   //! Its own file because a declaration is the value Load hands to the next phase: everything in it
   //! is already a domain type, and nothing in it depends on the order the build file made its calls
   //! in.
   //!
   //! Responsibilities: the three two-state flags [`TargetRole`], [`NetworkAccess`] and
   //! [`Freshness`].
   //!
   //! Non-responsibilities: relationships between declarations. Whether a label is declared twice,
   //! whether a dependency exists, and whether a rule reference names a rule all need every build
   //! file, so they belong to the phase that builds the graph.

   use serde::{Deserialize, Serialize};

   /// Whether a target is built for its outputs or run as a test.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub enum TargetRole {
       /// Declared with `b.target`.
       Build,
       /// Declared with `b.test`.
       Test,
   }

   /// Whether a target's action may reach the network.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub enum NetworkAccess {
       /// The default: no network.
       Sealed,
       /// `network = true`: a declared, plan-visible exception.
       Declared,
   }

   /// Whether a target's result may be reused from the cache.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub enum Freshness {
       /// The default: reused while its action key is unchanged.
       Cached,
       /// `always = true`: run on every build.
       Always,
   }

   #[cfg(test)]
   mod tests {
       use super::{Freshness, NetworkAccess, TargetRole};

       #[test]
       fn flags_order_their_default_first() {
           assert!(TargetRole::Build < TargetRole::Test);
           assert!(NetworkAccess::Sealed < NetworkAccess::Declared);
           assert!(Freshness::Cached < Freshness::Always);
       }
   }
   ```

5. Run and confirm green. Adds `types::declaration::tests::flags_order_their_default_first`:

   ```
   $ cargo test -p buildl-core
   test types::declaration::tests::flags_order_their_default_first ... ok
   test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): add the target role, network and freshness flags`.

---

## Task 2 — Add a target and its action

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the
   `pub use declaration::…` line with:

   ```rust
   pub use declaration::{Action, Freshness, NetworkAccess, Target, TargetRole};
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Argument, Command, Description, Diagnostic, Digest, Directory, EntryName, EnvName,
       FieldName, Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, SettingName,
       SettingValue, SourcePath, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `declaration::Action`, `declaration::Target`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/declaration.rs` in three places.

   Replace the module doc's `Responsibilities:` paragraph (two `//!` lines) with:

   ```rust
   //! Responsibilities: [`Target`], a target's [`Action`], and the three two-state flags
   //! [`TargetRole`], [`NetworkAccess`] and [`Freshness`].
   ```

   Replace `use serde::{Deserialize, Serialize};` with:

   ```rust
   use serde::{Deserialize, Serialize};

   use crate::types::{Command, EnvName, Label, OutputName, SourcePath};
   ```

   Insert above the `#[cfg(test)]` line, after the `Freshness` enum:

   ```rust
   /// How a target produces its outputs.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub enum Action {
       /// `rule = "..."`: run the named rule's command.
       UseRule(Label),
       /// `run = { ... }`: run this command.
       Run(Command),
   }

   /// A buildable or testable target, declared with `b.target` or `b.test`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Target {
       /// The target's name, in the directory that declared it.
       pub label: Label,
       /// Whether it builds or tests.
       pub role: TargetRole,
       /// How it produces its outputs.
       pub action: Action,
       /// The workspace files it reads.
       pub inputs: Vec<SourcePath>,
       /// The targets it depends on.
       pub deps: Vec<Label>,
       /// The files it produces, relative to its output directory.
       pub outputs: Vec<OutputName>,
       /// The environment variables it reads.
       pub env: Vec<EnvName>,
       /// Whether it may reach the network.
       pub network: NetworkAccess,
       /// Whether its result may be reused.
       pub freshness: Freshness,
   }
   ```

   `Target`'s fields are `pub`: it is a data record with nine fields, and a constructor taking
   them would trip `too_many_arguments`.

4. Run and confirm green. No test is added — both types are derive-only; tasks 4 and 5 exercise
   them:

   ```
   $ cargo test -p buildl-core
   test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add a declared target and its action`.

---

## Task 3 — Add the rule and alias declarations

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the
   `pub use declaration::…` line with:

   ```rust
   pub use declaration::{Action, Alias, Freshness, NetworkAccess, Rule, Target, TargetRole};
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, Command, Description, Diagnostic, Digest, Directory, EntryName,
       EnvName, FieldName, Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule,
       SettingName, SettingValue, SourcePath, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `declaration::Alias`, `declaration::Rule`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/declaration.rs` in three places.

   Replace the `Responsibilities:` paragraph with:

   ```rust
   //! Responsibilities: [`Target`], [`Rule`] and [`Alias`], a target's [`Action`], and the three
   //! two-state flags [`TargetRole`], [`NetworkAccess`] and [`Freshness`].
   ```

   Replace the `use crate::types::{…};` import with:

   ```rust
   use crate::types::{Command, Description, EnvName, Label, OutputName, SourcePath};
   ```

   Insert above the `#[cfg(test)]` line, after `Target`:

   ```rust
   /// A reusable command, declared with `b.rule`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Rule {
       /// The rule's name, in the directory that declared it.
       pub label: Label,
       /// The command a target using the rule runs.
       pub run: Command,
       /// The status-line text shown while it runs.
       pub description: Option<Description>,
   }

   /// A second name for a target, declared with `b.alias`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Alias {
       /// The alias's name, in the directory that declared it.
       pub label: Label,
       /// The target it names.
       pub target: Label,
   }
   ```

4. Run and confirm green; no test is added (derive-only, exercised in task 4):

   ```
   $ cargo test -p buildl-core
   test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add the rule and alias declarations`.

---

## Task 4 — Add the setting declaration and the `Declared` sum

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. In `crates/buildl-core/src/types/declaration.rs` replace the whole
   `#[cfg(test)] mod tests { … }` block with the one below. It now unwraps, so it gains the
   `expect` header, and its `label`, `command` and `target` fixtures are the ones task 5 reuses:

   ```rust
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{
           Action, Alias, Declared, Freshness, NetworkAccess, Rule, Setting, Target, TargetRole,
       };
       use crate::types::{Argument, Command, Label, OutputName, SettingName, SettingValue};

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       fn command(raw: &[&str]) -> Command {
           Command::new(raw.iter().map(|a| Argument::parse(*a).unwrap()).collect()).unwrap()
       }

       fn target() -> Target {
           Target {
               label: label("//:app"),
               role: TargetRole::Build,
               action: Action::Run(command(&["cc", "$deps", "-o", "$out"])),
               inputs: Vec::new(),
               deps: vec![label("//:main.o")],
               outputs: vec![OutputName::parse("app").unwrap()],
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           }
       }

       #[test]
       fn name_is_the_target_name_or_the_setting_name() {
           assert_eq!(Declared::Target(target()).name(), "app");
           let rule = Rule {
               label: label("//:cc"),
               run: command(&["cc"]),
               description: None,
           };
           assert_eq!(Declared::Rule(rule).name(), "cc");
           let alias = Alias {
               label: label("//:default"),
               target: label("//:app"),
           };
           assert_eq!(Declared::Alias(alias).name(), "default");
           let setting = Setting {
               name: SettingName::parse("test_filter").unwrap(),
               default: SettingValue::parse("").unwrap(),
           };
           assert_eq!(Declared::Setting(setting).name(), "test_filter");
       }

       #[test]
       fn flags_order_their_default_first() {
           assert!(TargetRole::Build < TargetRole::Test);
           assert!(NetworkAccess::Sealed < NetworkAccess::Declared);
           assert!(Freshness::Cached < Freshness::Always);
       }
   }
   ```

   In `crates/buildl-core/src/types/mod.rs` replace the `pub use declaration::…` line with:

   ```rust
   pub use declaration::{
       Action, Alias, Declared, Freshness, NetworkAccess, Rule, Setting, Target, TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, Command, Declared, Description, Diagnostic, Digest, Directory,
       EntryName, EnvName, FieldName, Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance,
       Rule, Setting, SettingName, SettingValue, SourcePath, Target, TargetName, TargetRole,
       Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `declaration::Declared`, `declaration::Setting`
     --> crates/buildl-core/src/types/mod.rs:…
   error[E0432]: unresolved imports `super::Declared`, `super::Setting`
      --> crates/buildl-core/src/types/declaration.rs:105:24
   ```

3. Edit the rest of `crates/buildl-core/src/types/declaration.rs`.

   Replace the `Responsibilities:` paragraph with:

   ```rust
   //! Responsibilities: the four kinds in [`Declared`] — [`Target`], [`Rule`], [`Alias`] and
   //! [`Setting`] — a target's [`Action`], and the three two-state flags [`TargetRole`],
   //! [`NetworkAccess`] and [`Freshness`].
   ```

   Replace the `use crate::types::{…};` import with:

   ```rust
   use crate::types::{
       Command, Description, EnvName, Label, OutputName, SettingName, SettingValue, SourcePath,
   };
   ```

   Insert above the `#[cfg(test)]` line, after `Alias`:

   ```rust
   /// A build setting, declared with `b.option`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Setting {
       /// The setting's workspace-wide name.
       pub name: SettingName,
       /// The value used when no `--set` overrides it.
       pub default: SettingValue,
   }

   /// The four things a build file can declare.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub enum Declared {
       /// A target.
       Target(Target),
       /// A rule.
       Rule(Rule),
       /// An alias.
       Alias(Alias),
       /// A build setting.
       Setting(Setting),
   }

   impl Declared {
       /// The declared name: the target name of a target, rule or alias, or the setting's name.
       #[must_use]
       pub fn name(&self) -> &str {
           match self {
               Self::Target(target) => target.label.name().as_str(),
               Self::Rule(rule) => rule.label.name().as_str(),
               Self::Alias(alias) => alias.label.name().as_str(),
               Self::Setting(setting) => setting.name.as_str(),
           }
       }
   }
   ```

   `name` is the second component of plan 06's sort key (directory, declared name, whole value).
   It returns `&str` so a target's `TargetName` and a setting's `SettingName` compare as one kind.

4. Run and confirm green. Adds `types::declaration::tests::name_is_the_target_name_or_the_setting_name`:

   ```
   $ cargo test -p buildl-core
   test types::declaration::tests::name_is_the_target_name_or_the_setting_name ... ok
   test result: ok. 107 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add the setting declaration and the declared-kind sum`.

---

## Task 5 — Add `Declaration`, Load's output record

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl-core/src/types/declaration.rs` replace the whole
   `#[cfg(test)] mod tests { … }` block with its final form:

   ```rust
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::PathBuf;

       use super::{
           Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
           TargetRole,
       };
       use crate::types::{
           Argument, Command, Description, Directory, Label, OutputName, Provenance, SettingName,
           SettingValue,
       };

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       fn command(raw: &[&str]) -> Command {
           Command::new(raw.iter().map(|a| Argument::parse(*a).unwrap()).collect()).unwrap()
       }

       fn target() -> Target {
           Target {
               label: label("//:app"),
               role: TargetRole::Build,
               action: Action::Run(command(&["cc", "$deps", "-o", "$out"])),
               inputs: Vec::new(),
               deps: vec![label("//:main.o")],
               outputs: vec![OutputName::parse("app").unwrap()],
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           }
       }

       #[test]
       fn name_is_the_target_name_or_the_setting_name() {
           assert_eq!(Declared::Target(target()).name(), "app");
           let rule = Rule {
               label: label("//:cc"),
               run: command(&["cc"]),
               description: None,
           };
           assert_eq!(Declared::Rule(rule).name(), "cc");
           let alias = Alias {
               label: label("//:default"),
               target: label("//:app"),
           };
           assert_eq!(Declared::Alias(alias).name(), "default");
           let setting = Setting {
               name: SettingName::parse("test_filter").unwrap(),
               default: SettingValue::parse("").unwrap(),
           };
           assert_eq!(Declared::Setting(setting).name(), "test_filter");
       }

       #[test]
       fn exposes_its_provenance_and_item() {
           let provenance = Provenance::new(PathBuf::from("build.lua"), Directory::root());
           let declaration = Declaration::new(provenance.clone(), Declared::Target(target()));
           assert_eq!(declaration.provenance(), &provenance);
           assert_eq!(declaration.item(), &Declared::Target(target()));
       }

       #[test]
       fn serializes_as_exact_canonical_json_and_round_trips() {
           let declaration = Declaration::new(
               Provenance::new(PathBuf::from("build.lua"), Directory::root()),
               Declared::Target(target()),
           );
           let json = crate::json::canonical::to_string(&declaration).unwrap();
           assert_eq!(
               json,
               concat!(
                   r#"{"item":{"Target":{"action":{"Run":["cc","$deps","-o","$out"]},"#,
                   r#""deps":["//:main.o"],"env":[],"freshness":"Cached","inputs":[],"#,
                   r#""label":"//:app","network":"Sealed","outputs":["app"],"role":"Build"}},"#,
                   r#""provenance":{"directory":"","file":"build.lua"}}"#
               )
           );
           assert_eq!(
               serde_json::from_str::<Declaration>(&json).unwrap(),
               declaration
           );

           let mut variant = target();
           variant.role = TargetRole::Test;
           variant.action = Action::UseRule(label("//:cc"));
           variant.network = NetworkAccess::Declared;
           variant.freshness = Freshness::Always;
           let items = [
               Declared::Target(variant),
               Declared::Rule(Rule {
                   label: label("//:cc"),
                   run: command(&["cc"]),
                   description: Some(Description::parse("compile").unwrap()),
               }),
               Declared::Alias(Alias {
                   label: label("//:default"),
                   target: label("//:app"),
               }),
               Declared::Setting(Setting {
                   name: SettingName::parse("test_filter").unwrap(),
                   default: SettingValue::parse("").unwrap(),
               }),
           ];
           for item in items {
               let declaration = Declaration::new(
                   Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                   item,
               );
               let json = crate::json::canonical::to_string(&declaration).unwrap();
               assert_eq!(
                   serde_json::from_str::<Declaration>(&json).unwrap(),
                   declaration
               );
           }
       }

       #[test]
       fn flags_order_their_default_first() {
           assert!(TargetRole::Build < TargetRole::Test);
           assert!(NetworkAccess::Sealed < NetworkAccess::Declared);
           assert!(Freshness::Cached < Freshness::Always);
       }
   }
   ```

   The JSON assertion is exact on purpose: the canonical serializer sorts object keys, a unit
   variant renders as its name, `Command` as an array, `Label` as its string, and the root
   directory as `""`. A field rename or a derive change shows up here as a changed string, which is
   a cache-format change for every later phase. The loop that follows round-trips the other three
   kinds and a target whose every flag and its action take the non-default variant, so every
   serialized shape in this file is read back at least once.

   In `crates/buildl-core/src/types/mod.rs` replace the `pub use declaration::…` block with:

   ```rust
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, Command, Declaration, Declared, Description, Diagnostic, Digest,
       Directory, EntryName, EnvName, FieldName, Freshness, Label, NetworkAccess, NodeId, OutputName,
       Provenance, Rule, Setting, SettingName, SettingValue, SourcePath, Target, TargetName,
       TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `declaration::Declaration`
     --> crates/buildl-core/src/types/mod.rs:…
   error[E0432]: unresolved import `super::Declaration`
      --> crates/buildl-core/src/types/declaration.rs:145:24
   ```

3. Edit the rest of `crates/buildl-core/src/types/declaration.rs`.

   Replace the `Responsibilities:` paragraph with its final wording:

   ```rust
   //! Responsibilities: [`Declaration`], the four kinds in [`Declared`] — [`Target`], [`Rule`],
   //! [`Alias`] and [`Setting`] — a target's [`Action`], and the three two-state flags
   //! [`TargetRole`], [`NetworkAccess`] and [`Freshness`].
   ```

   Replace the `use crate::types::{…};` import with:

   ```rust
   use crate::types::{
       Command, Description, EnvName, Label, OutputName, Provenance, SettingName, SettingValue,
       SourcePath,
   };
   ```

   Insert above the `#[cfg(test)]` line, after `impl Declared`:

   ```rust
   /// One validated declaration and the build file that made it.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Declaration {
       provenance: Provenance,
       item: Declared,
   }

   impl Declaration {
       /// Records `item` as declared by the build file `provenance` names.
       #[must_use]
       pub const fn new(provenance: Provenance, item: Declared) -> Self {
           Self { provenance, item }
       }

       /// The build file that made the declaration.
       #[must_use]
       pub const fn provenance(&self) -> &Provenance {
           &self.provenance
       }

       /// What was declared.
       #[must_use]
       pub const fn item(&self) -> &Declared {
           &self.item
       }
   }
   ```

   The fields are private and the accessors `const`: a `Declaration` is built once by Load and
   read thereafter.

4. Run and confirm green. Adds `exposes_its_provenance_and_item` and
   `serializes_as_exact_canonical_json_and_round_trips`:

   ```
   $ cargo test -p buildl-core
   test types::declaration::tests::exposes_its_provenance_and_item ... ok
   test types::declaration::tests::serializes_as_exact_canonical_json_and_round_trips ... ok
   test result: ok. 109 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   `crates/buildl-core/src/types/declaration.rs` is now in its final form.

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add the declaration record Load hands on`.

---

## Task 6 — Add `DeclarationOrder`

**Files:**
- Create `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/types/build_file.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       use super::DeclarationOrder;

       #[test]
       fn declaration_order_renders_and_orders_by_index() {
           assert_eq!(DeclarationOrder::new(3).to_string(), "#3");
           assert_eq!(DeclarationOrder::new(3).get(), 3);
           assert!(DeclarationOrder::new(2) < DeclarationOrder::new(10));
       }
   }
   ```

   `2 < 10` pins numeric rather than textual ordering.

2. Wire the module. In `crates/buildl-core/src/types/mod.rs` add `pub mod build_file;` between
   `pub mod argument;` and `pub mod declaration;`, and replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::DeclarationOrder;
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, Command, Declaration, DeclarationOrder, Declared, Description,
       Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label, NetworkAccess,
       NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue, SourcePath, Target,
       TargetName, TargetRole, Timestamp, Written,
   };
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `build_file::DeclarationOrder`
     --> crates/buildl-core/src/types/mod.rs:…
   error[E0432]: unresolved import `super::DeclarationOrder`
    --> crates/buildl-core/src/types/build_file.rs:5:9
   ```

4. Replace the whole file with:

   ```rust
   //! The values that cross the build-file evaluation port: the file asked for, and what came back.
   //!
   //! Its own file because these are the port's vocabulary rather than Load's output. What comes back
   //! is staged — every textual value still exactly as the build file wrote it — so that every
   //! validation and every decision about it happens on this side of the port.
   //!
   //! Responsibilities: [`DeclarationOrder`].
   //!
   //! Non-responsibilities: validation. A staged value is recorded, not checked; Load turns it into a
   //! [`Declaration`](crate::types::Declaration).

   use core::fmt;

   /// The position of a call among the calls one build file made, counting from zero.
   ///
   /// It locates an error inside its file. It is not part of any declaration: a `pairs` loop issues
   /// the same calls in a different order on each evaluation.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
   pub struct DeclarationOrder(u32);

   impl DeclarationOrder {
       /// Wraps a call's index.
       #[must_use]
       pub const fn new(index: u32) -> Self {
           Self(index)
       }

       /// The index.
       #[must_use]
       pub const fn get(self) -> u32 {
           self.0
       }
   }

   impl fmt::Display for DeclarationOrder {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           write!(f, "#{}", self.0)
       }
   }

   #[cfg(test)]
   mod tests {
       use super::DeclarationOrder;

       #[test]
       fn declaration_order_renders_and_orders_by_index() {
           assert_eq!(DeclarationOrder::new(3).to_string(), "#3");
           assert_eq!(DeclarationOrder::new(3).get(), 3);
           assert!(DeclarationOrder::new(2) < DeclarationOrder::new(10));
       }
   }
   ```

   No serde: a `DeclarationOrder` never leaves the staged side, and `Declaration` deliberately has
   no field for it.

5. Run and confirm green. Adds `types::build_file::tests::declaration_order_renders_and_orders_by_index`:

   ```
   $ cargo test -p buildl-core
   test types::build_file::tests::declaration_order_renders_and_orders_by_index ... ok
   test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): add a call's position within its build file`.

---

## Task 7 — Add `BuildFile`

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. In `crates/buildl-core/src/types/build_file.rs` replace the whole
   `#[cfg(test)] mod tests { … }` block with its final form; it unwraps, so it gains the `expect`
   header:

   ```rust
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::{Path, PathBuf};

       use super::{BuildFile, DeclarationOrder};
       use crate::types::{Directory, Provenance};

       #[test]
       fn declaration_order_renders_and_orders_by_index() {
           assert_eq!(DeclarationOrder::new(3).to_string(), "#3");
           assert_eq!(DeclarationOrder::new(3).get(), 3);
           assert!(DeclarationOrder::new(2) < DeclarationOrder::new(10));
       }

       #[test]
       fn build_file_exposes_its_provenance() {
           let lib = Directory::parse("lib").unwrap();
           let provenance = Provenance::new(PathBuf::from("lib/build.lua"), lib.clone());
           let file = BuildFile::new(provenance.clone());
           assert_eq!(file.provenance(), &provenance);
           assert_eq!(file.directory(), &lib);
           assert_eq!(file.file(), Path::new("lib/build.lua"));
       }
   }
   ```

   In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::{BuildFile, DeclarationOrder};
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label,
       NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue,
       SourcePath, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `build_file::BuildFile`
     --> crates/buildl-core/src/types/mod.rs:…
   error[E0432]: unresolved import `super::BuildFile`
     --> crates/buildl-core/src/types/build_file.rs:50:17
   ```

3. Edit the rest of `crates/buildl-core/src/types/build_file.rs`.

   Replace the `Responsibilities:` line with:

   ```rust
   //! Responsibilities: [`BuildFile`] and [`DeclarationOrder`].
   ```

   Replace `use core::fmt;` with:

   ```rust
   use core::fmt;
   use std::path::Path;

   use crate::types::{Directory, Provenance};
   ```

   Insert above the `#[cfg(test)]` line, after `impl fmt::Display for DeclarationOrder`:

   ```rust
   /// One build file to evaluate: its workspace-relative path and the directory it is evaluated in.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct BuildFile(Provenance);

   impl BuildFile {
       /// Names the build file `provenance` describes.
       #[must_use]
       pub const fn new(provenance: Provenance) -> Self {
           Self(provenance)
       }

       /// The file and directory, as every declaration it makes will carry them.
       #[must_use]
       pub const fn provenance(&self) -> &Provenance {
           &self.0
       }

       /// The directory the file is evaluated in.
       #[must_use]
       pub const fn directory(&self) -> &Directory {
           self.0.directory()
       }

       /// The file's workspace-relative path.
       #[must_use]
       pub fn file(&self) -> &Path {
           self.0.file()
       }
   }
   ```

   `BuildFile` is a `Provenance` rather than a second struct with the same two fields: one concept,
   one type. `file()` is not `const` because `Provenance::file` is not.

4. Run and confirm green. Adds `types::build_file::tests::build_file_exposes_its_provenance`:

   ```
   $ cargo test -p buildl-core
   test types::build_file::tests::build_file_exposes_its_provenance ... ok
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): name the build file a port evaluates`.

---

## Task 8 — Stage target and rule calls as written

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::{BuildFile, DeclarationOrder, StagedRule, StagedTarget};
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label,
       NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedRule, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `build_file::StagedRule`, `build_file::StagedTarget`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/build_file.rs` in three places.

   Replace the `Responsibilities:` line with:

   ```rust
   //! Responsibilities: [`BuildFile`], [`StagedTarget`], [`StagedRule`] and [`DeclarationOrder`].
   ```

   Replace the `use crate::types::{Directory, Provenance};` import with:

   ```rust
   use crate::types::{
       Argument, Description, Directory, EnvName, Freshness, Label, NetworkAccess, OutputName,
       Provenance, SourcePath, TargetName, TargetRole, Written,
   };
   ```

   Insert above the `#[cfg(test)]` line, after `impl BuildFile`:

   ```rust
   /// A `b.target` or `b.test` call.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedTarget {
       /// The target's name, never a label: a declaration stays in its own directory.
       pub name: Written<TargetName>,
       /// Whether it was declared with `b.target` or `b.test`.
       pub role: TargetRole,
       /// The `rule` field, if given.
       pub rule: Option<Written<Label>>,
       /// The `run` field, if given.
       pub run: Option<Vec<Written<Argument>>>,
       /// The `inputs` field, relative to the declaring directory.
       pub inputs: Vec<Written<SourcePath>>,
       /// The `deps` field.
       pub deps: Vec<Written<Label>>,
       /// The `outputs` field, if given.
       pub outputs: Option<Vec<Written<OutputName>>>,
       /// The `env` field.
       pub env: Vec<Written<EnvName>>,
       /// The `network` field.
       pub network: NetworkAccess,
       /// The `always` field.
       pub freshness: Freshness,
   }

   /// A `b.rule` call.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedRule {
       /// The rule's name.
       pub name: Written<TargetName>,
       /// The `run` field.
       pub run: Vec<Written<Argument>>,
       /// The `desc` field, if given.
       pub description: Option<Written<Description>>,
   }
   ```

   A declared name is a `Written<TargetName>`, never a `Written<Label>`: Load joins it with the
   declaring directory, so a build file cannot declare into another directory. `rule`, `run` and
   `outputs` are `Option`s because Load, not the adapter, decides exactly-one and the default.

4. Run and confirm green; no test is added (derive-only; plan 06's flows build these):

   ```
   $ cargo test -p buildl-core
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): stage target and rule calls as written`.

---

## Task 9 — Stage alias and option calls as written

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::{
       BuildFile, DeclarationOrder, StagedAlias, StagedRule, StagedSetting, StagedTarget,
   };
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label,
       NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedAlias, StagedRule, StagedSetting, StagedTarget, Target, TargetName,
       TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `build_file::StagedAlias`, `build_file::StagedSetting`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/build_file.rs` in three places.

   Replace the `Responsibilities:` line with:

   ```rust
   //! Responsibilities: [`BuildFile`], the four staged kinds — [`StagedTarget`], [`StagedRule`],
   //! [`StagedAlias`] and [`StagedSetting`] — and [`DeclarationOrder`].
   ```

   Replace the `use crate::types::{…};` import with its final form:

   ```rust
   use crate::types::{
       Argument, Description, Directory, EnvName, Freshness, Label, NetworkAccess, OutputName,
       Provenance, SettingName, SettingValue, SourcePath, TargetName, TargetRole, Written,
   };
   ```

   Insert above the `#[cfg(test)]` line, after `StagedRule`:

   ```rust
   /// A `b.alias` call.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedAlias {
       /// The alias's name.
       pub name: Written<TargetName>,
       /// The target it names.
       pub target: Written<Label>,
   }

   /// A `b.option` call.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedSetting {
       /// The setting's name.
       pub name: Written<SettingName>,
       /// The `default` field.
       pub default: Written<SettingValue>,
   }
   ```

4. Run and confirm green; no test is added:

   ```
   $ cargo test -p buildl-core
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): stage alias and option calls as written`.

---

## Task 10 — Add `StagedItem` and `StagedDeclaration`

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::{
       BuildFile, DeclarationOrder, StagedAlias, StagedDeclaration, StagedItem, StagedRule,
       StagedSetting, StagedTarget,
   };
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label,
       NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedAlias, StagedDeclaration, StagedItem, StagedRule, StagedSetting,
       StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `build_file::StagedDeclaration`, `build_file::StagedItem`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/build_file.rs` in two places.

   Replace the `Responsibilities:` paragraph with:

   ```rust
   //! Responsibilities: [`BuildFile`], [`StagedDeclaration`], [`StagedItem`] and its four kinds,
   //! and [`DeclarationOrder`].
   ```

   Insert directly above the line ``/// A `b.target` or `b.test` call.`` (after `impl BuildFile`):

   ```rust
   /// One declaration call.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedDeclaration {
       /// The call's position among the file's calls.
       pub order: DeclarationOrder,
       /// What the call declared.
       pub item: StagedItem,
   }

   /// The four kinds of declaration call, each exactly as written.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum StagedItem {
       /// `b.target` or `b.test`.
       Target(StagedTarget),
       /// `b.rule`.
       Rule(StagedRule),
       /// `b.alias`.
       Alias(StagedAlias),
       /// `b.option`.
       Setting(StagedSetting),
   }
   ```

4. Run and confirm green; no test is added:

   ```
   $ cargo test -p buildl-core
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): stage one declaration call with its position`.

---

## Task 11 — Add `StagedSubdir` and `StagedFile`

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with:

   ```rust
   pub use build_file::{
       BuildFile, DeclarationOrder, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget,
   };
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, FieldName, Freshness, Label,
       NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName, SettingValue,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `build_file::StagedFile`, `build_file::StagedSubdir`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/build_file.rs` in two places.

   Replace the `Responsibilities:` paragraph with:

   ```rust
   //! Responsibilities: [`BuildFile`], [`StagedFile`], [`StagedSubdir`], [`StagedDeclaration`],
   //! [`StagedItem`] and its four kinds, and [`DeclarationOrder`].
   ```

   Insert directly above the line `/// One declaration call.` (after `impl BuildFile`):

   ```rust
   /// Everything one build file declared and requested, exactly as written.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedFile {
       /// Its declarations, in call order.
       pub declarations: Vec<StagedDeclaration>,
       /// Its `subdir` requests, in call order.
       pub subdirs: Vec<StagedSubdir>,
   }

   /// One `subdir` request.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StagedSubdir {
       /// The directory, relative to the requesting file's directory.
       pub path: Written<Directory>,
       /// The request's position among the file's calls.
       pub order: DeclarationOrder,
   }
   ```

   `StagedFile` holds no `Provenance`: core built the `BuildFile`, so it already knows which file
   the result belongs to.

4. Run and confirm green; no test is added:

   ```
   $ cargo test -p buildl-core
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): stage a build file's declarations and subdir requests`.

---

## Task 12 — Add `Evaluated`

**Files:**
- Modify `crates/buildl-core/src/types/build_file.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Go red on the re-export. In `crates/buildl-core/src/types/mod.rs` replace the lines between
   `pub use argument::{Argument, Command};` and `pub use description::Description;` with their
   final form:

   ```rust
   pub use build_file::{
       BuildFile, DeclarationOrder, Evaluated, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget,
   };
   pub use declaration::{
       Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
       TargetRole,
   };
   ```

   In `crates/buildl-core/src/lib.rs` replace the `pub use types::{…};` block with its final form:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
       Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
       SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
   };
   ```

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `build_file::Evaluated`
     --> crates/buildl-core/src/types/mod.rs:…
   ```

3. Edit `crates/buildl-core/src/types/build_file.rs` in two places.

   Replace the `Responsibilities:` paragraph with its final wording:

   ```rust
   //! Responsibilities: [`BuildFile`], [`Evaluated`], [`StagedFile`], [`StagedSubdir`],
   //! [`StagedDeclaration`], [`StagedItem`] and its four kinds, and [`DeclarationOrder`].
   ```

   Insert directly above the line `/// Everything one build file declared and requested, exactly as written.`
   (after `impl BuildFile`):

   ```rust
   /// The outcome of evaluating one build file.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Evaluated {
       /// The file exists and was evaluated.
       Staged(StagedFile),
       /// No build file exists at that path.
       Absent,
   }
   ```

   `Absent` is a value, not an error: whether a missing file is fatal, and whom to blame, is
   Load's decision (plan 06), made with the requesting call's provenance in hand.

4. Run and confirm green; no test is added:

   ```
   $ cargo test -p buildl-core
   test result: ok. 111 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

   `crates/buildl-core/src/types/build_file.rs` is now in its final form.

5. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

6. Commit `feat(buildl-core): add the outcome of evaluating one build file`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all
$ cargo make dod
$ cargo deny check
```

All exit 0. `cargo make dod` runs `guard-core-purity` and `guard-crate-edges` ahead of clippy;
`crates/buildl-core/Cargo.toml` and `crates/expected-edges.txt` are untouched, so `cargo deny check`
is unchanged from plan 02.

End state:

| Check | Expected |
|---|---|
| `cargo test -p buildl-core` | 111 unit tests pass (105 + 6) |
| `types/declaration.rs` | `TargetRole`, `NetworkAccess`, `Freshness`, `Action`, `Target`, `Rule`, `Alias`, `Setting`, `Declared` (+ `name`), `Declaration`; 4 tests |
| `types/build_file.rs` | `DeclarationOrder`, `BuildFile`, `Evaluated`, `StagedFile`, `StagedSubdir`, `StagedDeclaration`, `StagedItem`, `StagedTarget`, `StagedRule`, `StagedAlias`, `StagedSetting`; 2 tests |
| `types/mod.rs`, `lib.rs` | all 21 types re-exported; doc prose untouched |

This is the first review checkpoint of the chain: plans 04 (error variants holding
`DeclarationOrder` and `Declaration`) and 05 (`DeclarationSource` returning `Evaluated`) build on
these types.

## Review findings

- doc accuracy — 🔵 `DeclarationOrder` doc said a `pairs` loop issues calls in a different order "on each evaluation"; reworded to "may issue … from one evaluation to the next" — `crates/buildl-core/src/types/build_file.rs:22-23` — fixed; verified by `cargo make dod` (Build Done, `test result: ok. 111 passed`).
- test coverage — 🔵 the round-trip loop never serialized `Rule { description: None }`; added a fifth item `//:ld` with `description: None` — `crates/buildl-core/src/types/declaration.rs` `serializes_as_exact_canonical_json_and_round_trips` — fixed; verified by `cargo make dod` (Build Done, `test result: ok. 111 passed`).
- reviewer verdict: spec compliant, no drift, blocking set empty; re-ran `cargo make dod`, `guard-core-purity`, `guard-crate-edges` (Build Done), `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`), `cargo test -p buildl-core --lib` (`111 passed`).

## Probe results

- baseline test count is 105 — `cargo test -p buildl-core --lib` — `test result: ok. 105 passed; 0 failed` — matches plan.
- `crate::json::canonical::to_string` exists — `grep -n "pub fn to_string" json/canonical.rs` — `json/canonical.rs:80:pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String>` — matches plan.
- `Provenance::file` exists and is not `const` — `types/provenance.rs:39:    pub fn file(&self) -> &Path {` — matches plan.
- `Directory::root` is `const` — `types/directory.rs:33:    pub const fn root() -> Self {` — matches plan.
- all other asserted facts settled by each task's own red-green step (red E0432 then green, counts 106, 106, 106, 107, 109, 110, 111 ×6 as the plan states).

## Deviations

- 2026-10-08 — one coder ran all 12 tasks (user's one-subagent-per-plan decision) and `cargo make dod` ran once at plan end rather than per task; per-task commits not made — the user holds the commit gate.
