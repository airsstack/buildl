---
status: approved
created: 2026-10-11
depends-on: [01]
---

# Action Key Implementation Plan

**Goal:** An action key is the digest of its components, kept beside them.

**Architecture:** Two new files under `types/`. `types/action_key.rs` holds the six component types and `ActionKey`; the components derive `Serialize`, and `ActionKey::new` is the only constructor: it serializes the components through the crate's canonical JSON serializer and hashes the bytes, so a key's digest always matches its components. A key holds, for each dependency, the names and digests of what it produced, never the dependency's own key, which is what leaves a target clean when a dependency is rebuilt to the same bytes. `OwnComponents` is a type of its own because a target downstream of one that is about to run has exactly that much of its key before the build. `types/cache_row.rs` holds `CacheRow`, the whole key a target was last built under with one digest per declared output. Nothing here implements `Deserialize`: nothing in this chain reads a key back.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D1, D5, D7), §3.2 (the key), §3.3 (the cache row), §9 (the rows for `types/action_key.rs` and `types/cache_row.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What plan `01` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Program`, `EnvValue`, `Platform` | `crates/buildl-core/src/types/` | fields of the key's components |
| `Command`, `SourcePath`, `OutputName`, `EnvName`, `SettingName`, `SettingValue`, `NetworkAccess`, `Label` | `crates/buildl-core/src/types/` | fields of the key's components; each already serializes as its text |
| `Digest` (`Digest::of(&[u8])`, `Copy`, serializes as 64 hexadecimal characters) | `crates/buildl-core/src/types/digest.rs` | every content identity, and the key's own digest |
| `json::canonical::to_vec` and `to_string` (object keys sorted at every level) | `crates/buildl-core/src/json/canonical.rs` | the bytes a key is the digest of |

Gate facts that bite here. Each names a lint seen as a failing run while this code was prototyped:

- clippy's `type_complexity` rejects an array of `(&str, fn(&mut KeyComponents))` pairs written
  inline, so the test module names the function type `Change`.
- clippy's `similar_names` rejects a binding named `changed` beside one named `changes`, so the
  loop over the cases binds `altered`.
- `types -> json` and `types -> error` are already in `crates/expected-module-edges.txt`, so this
  plan changes no module edge.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/action_key.rs  — [create] the key's components, ActionKey, unit tests
crates/buildl-core/src/types/cache_row.rs   — [create] CacheRow, unit tests
crates/buildl-core/src/types/mod.rs         — [modify] declare and re-export the two modules
crates/buildl-core/src/lib.rs               — [modify] re-export the eight types
```

### Task 1 — Add the action key and its components

**Files:**
- Create `crates/buildl-core/src/types/action_key.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod action_key;` to the `pub mod` lines and `pub use action_key::{ActionKey, DepProducts, InputDigest, KeyComponents, OwnComponents, Produced, ToolIdentity};` to the `pub use` lines, and add to the module doc, directly below the `TargetGraph` bullet:

   ```rust
   //! - [`ActionKey`], [`KeyComponents`] and their parts — what decides a target's output, and the
   //!   digest that stands for it.
   ```

2. Re-export them. In `crates/buildl-core/src/lib.rs`, add `ActionKey`, `DepProducts`, `InputDigest`, `KeyComponents`, `OwnComponents`, `Produced`, `ToolIdentity` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/action_key.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use super::{
           ActionKey, DepProducts, InputDigest, KeyComponents, OwnComponents, Produced, ToolIdentity,
       };
       use crate::json::canonical;
       use crate::types::{
           Argument, Command, Digest, EnvName, EnvValue, Label, NetworkAccess, OutputName, Platform,
           Program, SettingName, SettingValue, SourcePath,
       };

       fn command(arguments: &[&str]) -> Command {
           Command::new(
               arguments
                   .iter()
                   .map(|argument| Argument::parse(*argument).unwrap())
                   .collect(),
           )
           .unwrap()
       }

       fn input(path: &str, content: &str) -> InputDigest {
           InputDigest {
               path: SourcePath::parse(path).unwrap(),
               digest: Digest::of(content.as_bytes()),
           }
       }

       fn produced(name: &str, content: &str) -> Produced {
           Produced {
               name: OutputName::parse(name).unwrap(),
               digest: Digest::of(content.as_bytes()),
           }
       }

       fn env(pairs: &[(&str, Option<&str>)]) -> BTreeMap<EnvName, Option<EnvValue>> {
           pairs
               .iter()
               .map(|(name, value)| {
                   (
                       EnvName::parse(*name).unwrap(),
                       value.map(|text| EnvValue::parse(text).unwrap()),
                   )
               })
               .collect()
       }

       fn settings(pairs: &[(&str, &str)]) -> BTreeMap<SettingName, SettingValue> {
           pairs
               .iter()
               .map(|(name, value)| {
                   (
                       SettingName::parse(*name).unwrap(),
                       SettingValue::parse(*value).unwrap(),
                   )
               })
               .collect()
       }

       /// One edit to a key's components.
       type Change = fn(&mut KeyComponents);

       /// A key with something in every field, and inputs written out of path order.
       fn components() -> KeyComponents {
           KeyComponents {
               own: OwnComponents {
                   command: command(&["cc", "$in", "--filter=$opt:test_filter"]),
                   inputs: vec![input("src/z.c", "z"), input("src/a.c", "a")],
                   outputs: vec![
                       OutputName::parse("app").unwrap(),
                       OutputName::parse("app.map").unwrap(),
                   ],
                   env: env(&[("PATH", Some("/bin")), ("HOME", None)]),
                   tool: Some(ToolIdentity {
                       program: Program::parse("cc").unwrap(),
                       fingerprint: Digest::of(b"cc 14.2"),
                   }),
                   settings: settings(&[("test_filter", "TestLogin")]),
                   network: NetworkAccess::Sealed,
                   platform: Platform::parse("linux/x86_64").unwrap(),
               },
               deps: vec![DepProducts {
                   label: Label::parse("//lib:text").unwrap(),
                   outputs: vec![produced("text", "text v1")],
               }],
           }
       }

       #[test]
       fn components_serialize_to_the_exact_canonical_bytes() {
           let json = canonical::to_string(&components()).unwrap();
           let expected = [
               r#"{"deps":[{"label":"//lib:text","outputs":[{"digest":""#,
               &Digest::of(b"text v1").to_string(),
               r#"","name":"text"}]}],"own":{"command":["cc","$in","--filter=$opt:test_filter"],"#,
               r#""env":{"HOME":null,"PATH":"/bin"},"inputs":[{"digest":""#,
               &Digest::of(b"z").to_string(),
               r#"","path":"src/z.c"},{"digest":""#,
               &Digest::of(b"a").to_string(),
               r#"","path":"src/a.c"}],"network":"Sealed","outputs":["app","app.map"],"#,
               r#""platform":"linux/x86_64","settings":{"test_filter":"TestLogin"},"#,
               r#""tool":{"fingerprint":""#,
               &Digest::of(b"cc 14.2").to_string(),
               r#"","program":"cc"}}}"#,
           ]
           .concat();
           assert_eq!(json, expected);
       }

       #[test]
       fn a_command_naming_no_tool_serializes_the_tool_as_null() {
           let mut components = components();
           components.own.tool = None;
           components.deps.clear();
           let json = canonical::to_string(&components).unwrap();
           assert!(json.starts_with(r#"{"deps":[],"own":{"#), "{json}");
           assert!(json.ends_with(r#""tool":null}}"#), "{json}");
       }

       #[test]
       fn a_key_is_the_digest_of_its_components_canonical_bytes() {
           let components = components();
           let bytes = canonical::to_vec(&components).unwrap();
           let key = ActionKey::new(components.clone()).unwrap();
           assert_eq!(key.digest(), Digest::of(&bytes));
           assert_eq!(key.components(), &components);
       }

       #[test]
       fn a_key_serializes_as_its_components_and_its_digest() {
           let key = ActionKey::new(components()).unwrap();
           let json = canonical::to_string(&key).unwrap();
           let expected = format!(
               r#"{{"components":{},"digest":"{}"}}"#,
               canonical::to_string(key.components()).unwrap(),
               key.digest()
           );
           assert_eq!(json, expected);
       }

       #[test]
       fn the_order_maps_were_filled_in_does_not_change_a_key() {
           let mut other = components();
           other.own.env = env(&[("HOME", None), ("PATH", Some("/bin"))]);
           let mut settings_reversed = components();
           settings_reversed.own.settings = settings(&[("b", "2"), ("a", "1")]);
           let mut settings_forward = components();
           settings_forward.own.settings = settings(&[("a", "1"), ("b", "2")]);
           assert_eq!(
               ActionKey::new(other).unwrap().digest(),
               ActionKey::new(components()).unwrap().digest()
           );
           assert_eq!(
               ActionKey::new(settings_reversed).unwrap().digest(),
               ActionKey::new(settings_forward).unwrap().digest()
           );
       }

       #[test]
       fn changing_any_one_component_changes_the_key() {
           let cases: [(&str, Change); 12] = [
               ("command", |c| c.own.command = command(&["cc", "$in"])),
               ("input digest", |c| {
                   c.own.inputs = vec![input("src/z.c", "z2"), input("src/a.c", "a")];
               }),
               ("input order", |c| c.own.inputs.reverse()),
               ("outputs", |c| {
                   c.own.outputs = vec![OutputName::parse("app").unwrap()];
               }),
               ("env value", |c| {
                   c.own.env = env(&[("PATH", Some("/usr/bin")), ("HOME", None)]);
               }),
               ("env unset or empty", |c| {
                   c.own.env = env(&[("PATH", Some("/bin")), ("HOME", Some(""))]);
               }),
               ("tool", |c| {
                   c.own.tool = Some(ToolIdentity {
                       program: Program::parse("cc").unwrap(),
                       fingerprint: Digest::of(b"cc 15.1"),
                   });
               }),
               ("setting", |c| {
                   c.own.settings = settings(&[("test_filter", "TestLogout")]);
               }),
               ("network", |c| c.own.network = NetworkAccess::Declared),
               ("platform", |c| {
                   c.own.platform = Platform::parse("macos/aarch64").unwrap();
               }),
               ("dep output", |c| {
                   c.deps = vec![DepProducts {
                       label: Label::parse("//lib:text").unwrap(),
                       outputs: vec![produced("text", "text v2")],
                   }];
               }),
               ("dep label", |c| {
                   c.deps = vec![DepProducts {
                       label: Label::parse("//lib:other").unwrap(),
                       outputs: vec![produced("text", "text v1")],
                   }];
               }),
           ];
           let original = ActionKey::new(components()).unwrap().digest();
           for (what, change) in cases {
               let mut altered = components();
               change(&mut altered);
               assert_ne!(
                   ActionKey::new(altered).unwrap().digest(),
                   original,
                   "changing the {what} should change the key"
               );
           }
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::action_key
   error[E0432]: unresolved imports `super::ActionKey`, `super::DepProducts`, `super::InputDigest`, `super::KeyComponents`, `super::OwnComponents`, `super::Produced`, `super::ToolIdentity`
   error[E0432]: unresolved imports `action_key::ActionKey`, `action_key::DepProducts`, `action_key::InputDigest`, `action_key::KeyComponents`, `action_key::OwnComponents`, `action_key::Produced`, `action_key::ToolIdentity`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/action_key.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ````rust
   //! The action key: everything that decides what a target's action produces, and its digest.
   //!
   //! Its own file because the key is one value built from many: a target is rebuilt exactly when
   //! its key differs from the one its last build recorded, so what a key holds, in what order, and
   //! how it is hashed are decided in one place.
   //!
   //! Responsibilities: the components of a key — [`InputDigest`], [`Produced`], [`ToolIdentity`],
   //! [`DepProducts`], [`OwnComponents`], [`KeyComponents`] — and [`ActionKey`], which pairs them
   //! with their digest.
   //!
   //! Non-responsibilities: gathering the components, and comparing two keys. Both belong to the
   //! phase that plans; this file only says what a key is.

   use std::collections::BTreeMap;

   use serde::Serialize;

   use crate::error::Result;
   use crate::json::canonical;
   use crate::types::{
       Command, Digest, EnvName, EnvValue, Label, NetworkAccess, OutputName, Platform, Program,
       SettingName, SettingValue, SourcePath,
   };

   /// One workspace file a target reads, with the digest of its content.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct InputDigest {
       /// The file.
       pub path: SourcePath,
       /// The digest of its content.
       pub digest: Digest,
   }

   /// One file a target produced, with the digest of its content.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct Produced {
       /// The output's name.
       pub name: OutputName,
       /// The digest of its content.
       pub digest: Digest,
   }

   /// The program a command runs, with what identifies the program itself.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct ToolIdentity {
       /// The program, as the command names it.
       pub program: Program,
       /// What identifies the program found under that name: replacing the tool changes it.
       pub fingerprint: Digest,
   }

   /// What one dependency produced.
   ///
   /// A key holds a dependency's outputs, not the dependency's own key: a dependency rebuilt to
   /// the same bytes leaves the keys of the targets that depend on it unchanged.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct DepProducts {
       /// The dependency.
       pub label: Label,
       /// Its outputs, in the order it declares them.
       pub outputs: Vec<Produced>,
   }

   /// Every part of a key that does not wait on another target.
   ///
   /// Its own type because a target downstream of one that is about to run has exactly this much
   /// of its key before the build: its dependency's outputs do not exist yet.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct OwnComponents {
       /// The command, placeholders unexpanded.
       pub command: Command,
       /// The files the target reads, in written order.
       pub inputs: Vec<InputDigest>,
       /// The names of the files it produces, in written order.
       pub outputs: Vec<OutputName>,
       /// Each environment variable it reads, with its value; `None` when the variable is unset.
       pub env: BTreeMap<EnvName, Option<EnvValue>>,
       /// The program the command names, and its fingerprint; `None` when it names none.
       pub tool: Option<ToolIdentity>,
       /// Each build setting the command refers to, with the value in effect.
       pub settings: BTreeMap<SettingName, SettingValue>,
       /// Whether the action may reach the network.
       pub network: NetworkAccess,
       /// The platform the action runs on.
       pub platform: Platform,
   }

   /// Everything an action key is the digest of.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct KeyComponents {
       /// The parts that do not wait on another target.
       pub own: OwnComponents,
       /// What each dependency produced, in the order the dependencies were written.
       pub deps: Vec<DepProducts>,
   }

   /// An action key: the components of a target's action, and the digest that stands for them.
   ///
   /// The components are kept beside the digest so that two keys can be compared field by field,
   /// which is how a rebuild is explained. A key is built only by [`ActionKey::new`], so its digest
   /// always matches its components.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::collections::BTreeMap;
   ///
   /// use buildl_core::{
   ///     ActionKey, Argument, Command, KeyComponents, NetworkAccess, OutputName, OwnComponents,
   ///     Platform,
   /// };
   ///
   /// let components = KeyComponents {
   ///     own: OwnComponents {
   ///         command: Command::new(vec![Argument::parse("cc")?])?,
   ///         inputs: Vec::new(),
   ///         outputs: vec![OutputName::parse("app")?],
   ///         env: BTreeMap::new(),
   ///         tool: None,
   ///         settings: BTreeMap::new(),
   ///         network: NetworkAccess::Sealed,
   ///         platform: Platform::parse("linux/x86_64")?,
   ///     },
   ///     deps: Vec::new(),
   /// };
   /// let key = ActionKey::new(components.clone())?;
   /// assert_eq!(key.components(), &components);
   /// assert_eq!(key.digest(), ActionKey::new(components)?.digest());
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct ActionKey {
       digest: Digest,
       components: KeyComponents,
   }

   impl ActionKey {
       /// The key of `components`: their canonical JSON, hashed.
       ///
       /// # Errors
       ///
       /// Returns [`Error::CanonicalJson`](crate::Error::CanonicalJson) when the components cannot
       /// be serialized.
       pub fn new(components: KeyComponents) -> Result<Self> {
           let bytes = canonical::to_vec(&components)?;
           Ok(Self {
               digest: Digest::of(&bytes),
               components,
           })
       }

       /// The digest of the components.
       #[must_use]
       pub const fn digest(&self) -> Digest {
           self.digest
       }

       /// What the digest was taken of.
       #[must_use]
       pub const fn components(&self) -> &KeyComponents {
           &self.components
       }
   }
   ````

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::action_key
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 234 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

9. Commit:

   ```
   $ git add crates/buildl-core/src/types/action_key.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the action key and its components"
   ```

### Task 2 — Add the cache row

**Files:**
- Create `crates/buildl-core/src/types/cache_row.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod cache_row;` to the `pub mod` lines and `pub use cache_row::CacheRow;` to the `pub use` lines, and add to the module doc, directly below the `ActionKey` bullet:

   ```rust
   //! - [`CacheRow`] — what the last successful build of one target recorded.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `CacheRow` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/cache_row.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use super::CacheRow;
       use crate::types::{
           ActionKey, Argument, Command, Digest, KeyComponents, NetworkAccess, OutputName,
           OwnComponents, Platform,
       };

       /// A key declaring `outputs`, with nothing else of note.
       fn key(outputs: &[&str]) -> ActionKey {
           ActionKey::new(KeyComponents {
               own: OwnComponents {
                   command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                   inputs: Vec::new(),
                   outputs: outputs
                       .iter()
                       .map(|name| OutputName::parse(*name).unwrap())
                       .collect(),
                   env: BTreeMap::new(),
                   tool: None,
                   settings: BTreeMap::new(),
                   network: NetworkAccess::Sealed,
                   platform: Platform::parse("linux/x86_64").unwrap(),
               },
               deps: Vec::new(),
           })
           .unwrap()
       }

       #[test]
       fn a_row_needs_exactly_one_digest_per_declared_output() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           assert!(CacheRow::new(key(&["app", "app.map"]), vec![app]).is_none());
           assert!(CacheRow::new(key(&["app", "app.map"]), vec![app, map, app]).is_none());
           assert!(CacheRow::new(key(&["app", "app.map"]), vec![app, map]).is_some());
           assert!(CacheRow::new(key(&[]), Vec::new()).is_some());
           assert!(CacheRow::new(key(&[]), vec![app]).is_none());
       }

       #[test]
       fn a_row_keeps_the_key_it_was_built_under() {
           let row = CacheRow::new(key(&["app"]), vec![Digest::of(b"app")]).unwrap();
           assert_eq!(row.key(), &key(&["app"]));
       }

       #[test]
       fn produced_pairs_each_output_name_with_its_digest_in_declared_order() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           let row = CacheRow::new(key(&["z.out", "a.out"]), vec![app, map]).unwrap();
           let produced: Vec<(String, Digest)> = row
               .produced()
               .into_iter()
               .map(|output| (output.name.to_string(), output.digest))
               .collect();
           assert_eq!(
               produced,
               [("z.out".to_owned(), app), ("a.out".to_owned(), map)]
           );
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::cache_row
   error[E0432]: unresolved import `super::CacheRow`
   error[E0432]: unresolved import `cache_row::CacheRow`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/cache_row.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! A cache row: what the last successful build of one target recorded.
   //!
   //! Its own file because a row is what a later plan compares against: it holds the key the
   //! target was built under, to say whether anything changed, and the digests of what the build
   //! produced, which the targets depending on it key on.
   //!
   //! Responsibilities: [`CacheRow`], its constructor, and pairing its outputs with their names.
   //!
   //! Non-responsibilities: storing rows, and deciding what a row means for the next build.

   use crate::types::{ActionKey, Digest, Produced};

   /// What the last successful build of one target recorded: its key, and what it produced.
   ///
   /// A row holds one digest per output the key declares, in the same order. That is checked once,
   /// by [`CacheRow::new`], so every reader may pair names with digests without checking again.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct CacheRow {
       key: ActionKey,
       outputs: Vec<Digest>,
   }

   impl CacheRow {
       /// A row for a build under `key` that produced `outputs`, one digest per declared output in
       /// declared order; `None` when `outputs` does not hold exactly one digest per output.
       #[must_use]
       pub fn new(key: ActionKey, outputs: Vec<Digest>) -> Option<Self> {
           (outputs.len() == key.components().own.outputs.len()).then_some(Self { key, outputs })
       }

       /// The key the target was built under.
       #[must_use]
       pub const fn key(&self) -> &ActionKey {
           &self.key
       }

       /// Each declared output with the digest of what was produced under that name, in declared
       /// order.
       #[must_use]
       pub fn produced(&self) -> Vec<Produced> {
           self.key
               .components()
               .own
               .outputs
               .iter()
               .zip(&self.outputs)
               .map(|(name, digest)| Produced {
                   name: name.clone(),
                   digest: *digest,
               })
               .collect()
       }
   }
   ```

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::cache_row
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 240 filtered out
   ```

8. After `cargo fmt --all`, the list in `crates/buildl-core/src/lib.rs` reads:

   ```rust
   pub use types::{
       Action, ActionKey, Alias, Argument, BuildFile, CacheRow, Command, Declaration,
       DeclarationOrder, Declared, DepProducts, Description, Diagnostic, Digest, Directory, EntryName,
       EnvName, EnvValue, Evaluated, FieldName, FileStat, Freshness, InputDigest, KeyComponents,
       Label, NetworkAccess, Node, NodeId, OutputName, OwnComponents, Platform, Produced, Program,
       Provenance, Rule, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, Target, TargetGraph, TargetName, TargetRole, Timestamp, ToolIdentity,
       Written,
   };
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

10. Commit:

   ```
   $ git add crates/buildl-core/src/types/cache_row.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the cache row"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- The components of one key serialize to exact, pinned bytes: an object with `deps` and `own`,
  keys sorted, an unset variable as `null`, a command naming no tool as `"tool":null`, inputs in
  written order.
- A key's digest is the SHA-256 of those bytes, is the same whatever order its maps were filled
  in, and changes when any one component changes: twelve single edits are each checked.
- A `CacheRow` exists only with exactly one digest per declared output, and pairs names with
  digests in declared order.
- No type in either file implements `Deserialize`.
