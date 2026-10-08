---
status: approved
created: 2026-10-08
depends-on: [01]
---

# Staging Buffer Implementation Plan

**Goal:** One build file's staged declarations are held in a buffer that numbers every call, enforces a byte budget, and keeps the first refusal.

**Architecture:** One file, `staging.rs`. `Staging` holds the staged records, a single `DeclarationOrder` counter shared by declarations and `subdir` requests, a running byte charge, and the first recorded refusal. A record's charge is the in-memory size of its record type plus the byte length of every string it holds. A record that would pass the budget, or a call past `u32::MAX`, is refused with `Exceeded` and not stored. `finish` returns the staged file, or the first recorded refusal if there is one, so a refusal the build file caught still fails the file. The buffer knows nothing about Lua; the `buildl` module feeds it.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §7 (order, budget, sticky failure), D3 (byte budget), D6 (sticky).

---

## Context an implementer needs

**Conventions every task follows.** The workspace lints are strict. `unwrap`, `expect` and `panic!`
are denied outside tests, pedantic and nursery clippy run at `-D warnings`, and `missing_docs` and
`unreachable_pub` are on. Crate-internal items are therefore `pub(crate)` inside private modules. A
test module opens with `#![expect(clippy::unwrap_used, reason = …)]`, adding `clippy::panic` where
it panics. `lib.rs` holds only module docs, `mod` declarations and `pub use` re-exports. Every file
opens with `//!` docs naming its responsibilities and non-responsibilities, and carries no internal
planning vocabulary. Every commit is Conventional Commits with scope `buildl-lua`, or `repo` for
workspace files. Every task ends green on `cargo make dod` before it commits.

**Where the code comes from.** Every code block below was compiled, linted and tested in a scratch
copy of the workspace at every task boundary of this plan set, on macOS with rustc 1.98 and clippy
1.98. The expected outputs are the outputs those runs printed.

| Item from `buildl-core` (all re-exported at the crate root) | Used for |
|---|---|
| `StagedFile { declarations, subdirs }`, `StagedDeclaration { order, item }`, `StagedSubdir { path, order }`, all with `pub` fields | the buffer's contents |
| `StagedItem::{Target, Rule, Alias, Setting}` and `StagedTarget`/`StagedRule`/`StagedAlias`/`StagedSetting` (`pub` fields) | measuring a record's strings |
| `DeclarationOrder::new(u32)`, `.get()` | numbering calls |
| `Written<T>::new`, `.as_written()` | strings |
| `EvaluationFailure`, `Diagnostic` | the recorded refusal |

Plan `01` must be done: without its lint allowance, every `pub(crate)` item here fails clippy.

## File structure

```text
crates/buildl-lua/src/staging.rs — [create] Staging, Exceeded, record charging, and unit tests
crates/buildl-lua/src/lib.rs     — [modify] declare `mod staging;`
```

### Task 1 — The staging buffer

**Files:**
- Create `crates/buildl-lua/src/staging.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod staging;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/staging.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! The buffer one build file's declarations are staged into while it runs.
   //!
   //! Its own file because the buffer lives outside the Lua heap, so the Lua memory ceiling does not
   //! bound it. It enforces its own byte budget, numbers every staged call, and keeps the first
   //! refusal of the evaluation even if the build file catches the error and carries on.
   //!
   //! Responsibilities: [`Staging`] (order, byte budget, sticky refusal) and [`Exceeded`].
   //!
   //! Non-responsibilities: reading Lua values and rendering diagnostics, which happen before a record
   //! reaches the buffer.

   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until `LuaSource` evaluates a build file"
       )
   )]

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use buildl_core::{
           DeclarationOrder, Diagnostic, EvaluationFailure, EvaluationLimit, Freshness, NetworkAccess,
           StagedAlias, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole,
           Written,
       };

       use super::{Exceeded, Staging, item_bytes, record_size};

       fn alias(name: &str, target: &str) -> StagedItem {
           StagedItem::Alias(StagedAlias {
               name: Written::new(name),
               target: Written::new(target),
           })
       }

       fn staging_failure() -> EvaluationFailure {
           EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Staging,
           }
       }

       #[test]
       fn numbers_declarations_and_subdirs_from_one_counter() {
           let mut staging = Staging::new(u64::MAX);
           staging.stage(alias("default", "app")).unwrap();
           staging.stage_subdir(Written::new("lib")).unwrap();
           staging.stage(alias("all", "app")).unwrap();
           let file = staging.finish().unwrap();
           let orders: Vec<u32> = file
               .declarations
               .iter()
               .map(|declaration| declaration.order.get())
               .collect();
           assert_eq!(orders, [0, 2]);
           assert_eq!(
               file.subdirs,
               [StagedSubdir {
                   path: Written::new("lib"),
                   order: DeclarationOrder::new(1),
               }]
           );
       }

       #[test]
       fn admits_a_record_that_fills_the_budget_exactly() {
           let charge = record_size::<StagedSubdir>() + 3;
           let mut staging = Staging::new(charge);
           assert_eq!(staging.stage_subdir(Written::new("lib")), Ok(()));
           assert_eq!(
               staging.stage_subdir(Written::new("")),
               Err(Exceeded::Budget(charge))
           );
       }

       #[test]
       fn refuses_a_record_one_byte_over_the_budget() {
           let charge = record_size::<StagedSubdir>() + 3;
           let mut staging = Staging::new(charge - 1);
           assert_eq!(
               staging.stage_subdir(Written::new("lib")),
               Err(Exceeded::Budget(charge - 1))
           );
           assert!(staging.finish().unwrap().subdirs.is_empty());
       }

       #[test]
       fn refuses_once_every_order_is_used() {
           let mut staging = Staging::new(u64::MAX);
           staging.next = u32::MAX;
           assert_eq!(staging.stage(alias("default", "app")), Err(Exceeded::Order));
       }

       #[test]
       fn charges_every_string_a_target_holds() {
           let target = StagedItem::Target(StagedTarget {
               name: Written::new("app"),
               role: TargetRole::Build,
               rule: Some(Written::new("cc")),
               run: Some(vec![Written::new("cc"), Written::new("-o")]),
               inputs: vec![Written::new("a.c")],
               deps: vec![Written::new("//lib:text")],
               outputs: Some(vec![Written::new("app")]),
               env: vec![Written::new("HOME")],
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           });
           assert_eq!(item_bytes(&target), 3 + 2 + 2 + 2 + 3 + 10 + 3 + 4);
       }

       #[test]
       fn charges_every_string_a_rule_and_a_setting_hold() {
           let rule = StagedItem::Rule(StagedRule {
               name: Written::new("cc"),
               run: vec![Written::new("cc")],
               description: Some(Written::new("compile")),
           });
           let setting = StagedItem::Setting(StagedSetting {
               name: Written::new("mode"),
               default: Written::new("debug"),
           });
           assert_eq!(item_bytes(&rule), 2 + 2 + 7);
           assert_eq!(item_bytes(&setting), 4 + 5);
           assert_eq!(item_bytes(&alias("default", "app")), 7 + 3);
       }

       #[test]
       fn keeps_the_first_refusal_only() {
           let mut staging = Staging::new(u64::MAX);
           staging.record(staging_failure(), Diagnostic::new("first"));
           staging.record(EvaluationFailure::Runtime, Diagnostic::new("second"));
           assert_eq!(staging.recorded(), Some(&Diagnostic::new("first")));
           assert_eq!(
               staging.finish().unwrap_err(),
               (staging_failure(), Diagnostic::new("first"))
           );
       }

       #[test]
       fn a_recorded_refusal_wins_over_staged_records() {
           let mut staging = Staging::new(u64::MAX);
           staging.stage(alias("default", "app")).unwrap();
           staging.record(EvaluationFailure::Runtime, Diagnostic::new("boom"));
           assert!(staging.finish().is_err());
       }

       #[test]
       fn names_what_was_exceeded() {
           assert_eq!(
               Exceeded::Budget(16).to_string(),
               "staging budget of 16 bytes exceeded"
           );
           assert_eq!(
               Exceeded::Order.to_string(),
               "too many staged calls to number"
           );
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib staging::tests
   ```

   ```text
   error[E0432]: unresolved imports `super::Exceeded`, `super::Staging`, `super::item_bytes`, `super::record_size`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use core::fmt;
   use core::mem::size_of;

   use buildl_core::{
       DeclarationOrder, Diagnostic, Directory, EvaluationFailure, StagedDeclaration, StagedFile,
       StagedItem, StagedSubdir, Written,
   };

   /// Why the buffer refused a record.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) enum Exceeded {
       /// Admitting the record would pass the byte budget, which this carries.
       Budget(u64),
       /// The file already staged as many calls as a [`DeclarationOrder`] can number.
       Order,
   }

   impl fmt::Display for Exceeded {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           match self {
               Self::Budget(budget) => write!(f, "staging budget of {budget} bytes exceeded"),
               Self::Order => f.write_str("too many staged calls to number"),
           }
       }
   }

   /// One build file's staged declarations and `subdir` requests, in call order.
   #[derive(Debug)]
   pub(crate) struct Staging {
       budget: u64,
       charged: u64,
       next: u32,
       declarations: Vec<StagedDeclaration>,
       subdirs: Vec<StagedSubdir>,
       recorded: Option<(EvaluationFailure, Diagnostic)>,
   }

   impl Staging {
       /// An empty buffer that admits at most `budget` bytes.
       pub(crate) const fn new(budget: u64) -> Self {
           Self {
               budget,
               charged: 0,
               next: 0,
               declarations: Vec::new(),
               subdirs: Vec::new(),
               recorded: None,
           }
       }

       /// Stages a declaration, charging its record size plus the bytes of its text.
       pub(crate) fn stage(&mut self, item: StagedItem) -> Result<(), Exceeded> {
           let charge = record_size::<StagedDeclaration>().saturating_add(item_bytes(&item));
           let order = self.admit(charge)?;
           self.declarations.push(StagedDeclaration { order, item });
           Ok(())
       }

       /// Stages a `subdir` request, charged like a declaration.
       pub(crate) fn stage_subdir(&mut self, path: Written<Directory>) -> Result<(), Exceeded> {
           let charge = record_size::<StagedSubdir>().saturating_add(bytes(path.as_written()));
           let order = self.admit(charge)?;
           self.subdirs.push(StagedSubdir { path, order });
           Ok(())
       }

       /// Records a refusal. Only the first one is kept: it is the cause, and anything after it ran in
       /// a file that had already failed.
       pub(crate) fn record(&mut self, failure: EvaluationFailure, diagnostic: Diagnostic) {
           if self.recorded.is_none() {
               self.recorded = Some((failure, diagnostic));
           }
       }

       /// The recorded refusal's diagnostic, if a refusal was recorded.
       pub(crate) fn recorded(&self) -> Option<&Diagnostic> {
           self.recorded.as_ref().map(|(_, diagnostic)| diagnostic)
       }

       /// The staged file, or the recorded refusal if there is one.
       pub(crate) fn finish(self) -> Result<StagedFile, (EvaluationFailure, Diagnostic)> {
           match self.recorded {
               Some(recorded) => Err(recorded),
               None => Ok(StagedFile {
                   declarations: self.declarations,
                   subdirs: self.subdirs,
               }),
           }
       }

       /// Charges `charge` against the budget and numbers the call.
       fn admit(&mut self, charge: u64) -> Result<DeclarationOrder, Exceeded> {
           let charged = self.charged.saturating_add(charge);
           if charged > self.budget {
               return Err(Exceeded::Budget(self.budget));
           }
           let next = self.next.checked_add(1).ok_or(Exceeded::Order)?;
           let order = DeclarationOrder::new(self.next);
           self.charged = charged;
           self.next = next;
           Ok(order)
       }
   }

   /// The in-memory size of one record of type `T`.
   fn record_size<T>() -> u64 {
       u64::try_from(size_of::<T>()).unwrap_or(u64::MAX)
   }

   fn bytes(text: &str) -> u64 {
       u64::try_from(text.len()).unwrap_or(u64::MAX)
   }

   fn all_bytes<T>(texts: &[Written<T>]) -> u64 {
       texts
           .iter()
           .fold(0, |sum, text| sum.saturating_add(bytes(text.as_written())))
   }

   /// The bytes of every string `item` holds.
   fn item_bytes(item: &StagedItem) -> u64 {
       match item {
           StagedItem::Target(target) => [
               bytes(target.name.as_written()),
               target
                   .rule
                   .as_ref()
                   .map_or(0, |rule| bytes(rule.as_written())),
               target.run.as_deref().map_or(0, all_bytes),
               all_bytes(&target.inputs),
               all_bytes(&target.deps),
               target.outputs.as_deref().map_or(0, all_bytes),
               all_bytes(&target.env),
           ]
           .into_iter()
           .fold(0, u64::saturating_add),
           StagedItem::Rule(rule) => bytes(rule.name.as_written())
               .saturating_add(all_bytes(&rule.run))
               .saturating_add(
                   rule.description
                       .as_ref()
                       .map_or(0, |description| bytes(description.as_written())),
               ),
           StagedItem::Alias(alias) => {
               bytes(alias.name.as_written()).saturating_add(bytes(alias.target.as_written()))
           }
           StagedItem::Setting(setting) => {
               bytes(setting.name.as_written()).saturating_add(bytes(setting.default.as_written()))
           }
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib staging::tests
   ```

   ```text
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): stage declarations under an order counter and a byte budget`.

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --lib staging::tests` reports 9 passed.
- `cargo make dod` exits `0`; the only `buildl-lua` change is `staging.rs` and its `mod` line.
