---
status: approved
created: 2026-10-11
depends-on: [02, 07]
---

# Plan Gather Implementation Plan

**Goal:** What the ports report for a graph becomes one set of own key components per target.

**Architecture:** Two more logic files in `plan/`, both over plain values. `plan/inputs.rs` holds the stat-cache rule: a recorded digest stands exactly when the file's metadata equals what the digest was recorded under — equality, not order, so an earlier modification time is a change too. It sorts inputs, once their metadata is read, into those that keep their digest and those to hash, and pairs a digester's reply with the files that were sent, failing as the digester's own error when the reply has the wrong length. `plan/components.rs` holds `Gathered`, everything fetched once for a graph, and `own`, which builds one target's `OwnComponents` from its node and that: inputs and outputs in written order, the environment keyed by name, only the settings the command names. The functions that call a port are thin wrappers around these and arrive with the flows that test them, in plan `10`.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D5, D6, D11), §5.4 (inputs, components), §9 (the rows for `plan/inputs.rs` and `plan/components.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What plans `01`, `02`, `05` and `07` leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `FileStat`, `StatEntry` (both `Copy`, compared with `==`) | `crates/buildl-core/src/types/file_stat.rs` | the values the stat-cache rule decides over |
| `Error::Adapter { port, diagnostic }`, `PortName::Digester`, `Diagnostic::new` | `crates/buildl-core/src/` | the failure of a reply of the wrong length |
| `OwnComponents`, `InputDigest`, `ToolIdentity` | `crates/buildl-core/src/types/action_key.rs` | what `own` builds |
| `referenced(&Command, &BTreeMap<SettingName, SettingValue>)` | `crates/buildl-core/src/plan/settings.rs` | the settings a command names |
| `program_of(&Command) -> Option<Program>` | `crates/buildl-core/src/plan/tools.rs` | the program a command names |
| `command`, `settings`, `node`, `path`, `env_name`, `program` | `crates/buildl-core/src/plan/fixtures.rs` | the builders this plan's tests use; none is added |
| `plan/mod.rs` declaring `authority`, `settings` and `tools`, each under a `cfg_attr(not(test), expect(dead_code, …))` | `crates/buildl-core/src/plan/mod.rs` | the file each task adds one `mod` line to |

Gate facts that bite here:

- Nothing outside a file's own tests calls its functions until plan `10` writes the function
  that makes a plan, so the library build reports them as dead code and the gate fails. Each
  `mod` line in `plan/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It
  must be `cfg_attr(not(test), …)`: in the test build the functions are used, and a plain
  `expect` would fail there as unfulfilled. Plan `10` removes the attributes.
- `plan/fixtures.rs` is compiled for tests only, and a fixture no test calls is dead code in the
  test build. Each task therefore adds only the builders its own tests use.
- Both files import only `crate::error`, `crate::types` and siblings under `crate::plan`, so
  `crates/expected-module-edges.txt` does not change.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/plan/inputs.rs       — [create] Read, Split, reusable, split, hashed, unit tests
crates/buildl-core/src/plan/components.rs   — [create] Gathered, own, unit tests
crates/buildl-core/src/plan/mod.rs          — [modify] declare the two modules
```

### Task 1 — Decide which recorded digests still stand

**Files:**
- Create `crates/buildl-core/src/plan/inputs.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
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
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod authority;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod inputs;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod settings;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod tools;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/plan/inputs.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{Read, Split, hashed, reusable, split};
       use crate::error::{Error, PortName};
       use crate::plan::fixtures::path;
       use crate::types::{Digest, FileStat, StatEntry, Timestamp};

       fn stat(modified: u64, size: u64) -> FileStat {
           FileStat {
               modified: Timestamp::from_unix_nanos(modified),
               size,
           }
       }

       fn entry(modified: u64, size: u64, content: &str) -> StatEntry {
           StatEntry {
               stat: stat(modified, size),
               digest: Digest::of(content.as_bytes()),
           }
       }

       #[test]
       fn a_recorded_digest_stands_while_the_metadata_is_unchanged() {
           assert_eq!(
               reusable(Some(entry(100, 7, "main")), stat(100, 7)),
               Some(Digest::of(b"main"))
           );
       }

       #[test]
       fn a_file_never_hashed_has_no_digest_to_reuse() {
           assert_eq!(reusable(None, stat(100, 7)), None);
       }

       #[test]
       fn any_change_of_metadata_discards_the_recorded_digest() {
           let recorded = Some(entry(100, 7, "main"));
           assert_eq!(reusable(recorded, stat(101, 7)), None, "a later time");
           assert_eq!(reusable(recorded, stat(99, 7)), None, "an earlier time");
           assert_eq!(reusable(recorded, stat(100, 8)), None, "another size");
       }

       #[test]
       fn inputs_are_sorted_without_changing_their_order() {
           let a = path("src/a.c");
           let b = path("src/b.c");
           let c = path("src/c.c");
           let d = path("src/d.c");
           let read = [
               Read {
                   path: &a,
                   stat: stat(1, 10),
                   recorded: None,
               },
               Read {
                   path: &b,
                   stat: stat(2, 20),
                   recorded: Some(entry(2, 20, "b")),
               },
               Read {
                   path: &c,
                   stat: stat(3, 30),
                   recorded: Some(entry(9, 30, "c")),
               },
               Read {
                   path: &d,
                   stat: stat(4, 40),
                   recorded: Some(entry(4, 40, "d")),
               },
           ];
           assert_eq!(
               split(&read),
               Split {
                   reused: vec![(&b, Digest::of(b"b")), (&d, Digest::of(b"d"))],
                   pending: vec![(&a, stat(1, 10)), (&c, stat(3, 30))],
               }
           );
       }

       #[test]
       fn each_hashed_input_is_recorded_under_the_metadata_it_was_read_with() {
           let a = path("src/a.c");
           let c = path("src/c.c");
           let pending = [(&a, stat(1, 10)), (&c, stat(3, 30))];
           let reply = [Digest::of(b"a"), Digest::of(b"c")];
           assert_eq!(
               hashed(&pending, &reply).unwrap(),
               [(&a, entry(1, 10, "a")), (&c, entry(3, 30, "c"))]
           );
           assert!(hashed(&[], &[]).unwrap().is_empty());
       }

       #[test]
       fn a_reply_of_the_wrong_length_is_the_digesters_failure() {
           let a = path("src/a.c");
           let c = path("src/c.c");
           let pending = [(&a, stat(1, 10)), (&c, stat(3, 30))];
           let short = [Digest::of(b"a")];
           let long = [Digest::of(b"a"), Digest::of(b"c"), Digest::of(b"x")];
           for (reply, text) in [
               (&short[..], "digester: 1 digests were returned for 2 files"),
               (&long[..], "digester: 3 digests were returned for 2 files"),
           ] {
               let error = hashed(&pending, reply).unwrap_err();
               assert_eq!(error.to_string(), text);
               match error {
                   Error::Adapter { port, .. } => assert_eq!(port, PortName::Digester),
                   other => unreachable!("expected Adapter, got {other:?}"),
               }
           }
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::inputs
   error[E0432]: unresolved imports `super::Read`, `super::Split`, `super::hashed`, `super::reusable`, `super::split`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/plan/inputs.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Input digests: which recorded digests still stand, and which files must be hashed again.
   //!
   //! Its own file because hashing is what a warm plan must avoid: a file whose metadata has not
   //! changed since it was last hashed keeps its recorded digest, and only the others are read.
   //!
   //! Responsibilities: [`Read`], [`reusable`], [`split`], [`Split`] and [`hashed`].
   //!
   //! Non-responsibilities: reading metadata, hashing, and storing what was hashed. All three
   //! arrive through ports; the decisions here are over the values those ports returned.

   use crate::error::{Error, PortName, Result};
   use crate::types::{Diagnostic, Digest, FileStat, SourcePath, StatEntry};

   /// One input once its metadata has been read.
   #[derive(Debug, Clone, Copy)]
   pub(crate) struct Read<'g> {
       /// The file.
       pub(crate) path: &'g SourcePath,
       /// Its metadata now.
       pub(crate) stat: FileStat,
       /// What was recorded for it when it was last hashed, if anything.
       pub(crate) recorded: Option<StatEntry>,
   }

   /// Inputs sorted by whether they must be hashed, each list in the order the inputs were given.
   #[derive(Debug, PartialEq, Eq)]
   pub(crate) struct Split<'g> {
       /// The inputs whose recorded digest stands, with that digest.
       pub(crate) reused: Vec<(&'g SourcePath, Digest)>,
       /// The inputs to hash, with the metadata each will be recorded under.
       pub(crate) pending: Vec<(&'g SourcePath, FileStat)>,
   }

   /// The digest recorded for a file, when its metadata is still what it was recorded under.
   ///
   /// The two are compared for equality: a modification time earlier than the recorded one is a
   /// change like any other.
   pub(crate) fn reusable(recorded: Option<StatEntry>, stat: FileStat) -> Option<Digest> {
       recorded
           .filter(|entry| entry.stat == stat)
           .map(|entry| entry.digest)
   }

   /// Sorts `read` into the inputs that keep their recorded digest and the inputs to hash.
   pub(crate) fn split<'g>(read: &[Read<'g>]) -> Split<'g> {
       let mut reused = Vec::new();
       let mut pending = Vec::new();
       for input in read {
           match reusable(input.recorded, input.stat) {
               Some(digest) => reused.push((input.path, digest)),
               None => pending.push((input.path, input.stat)),
           }
       }
       Split { reused, pending }
   }

   /// Pairs each input that was sent to be hashed with its digest from `reply`, as the entry to
   /// record for it.
   ///
   /// # Errors
   ///
   /// Returns [`Error::Adapter`], naming [`PortName::Digester`], when `reply` does not hold exactly
   /// one digest per input.
   pub(crate) fn hashed<'g>(
       pending: &[(&'g SourcePath, FileStat)],
       reply: &[Digest],
   ) -> Result<Vec<(&'g SourcePath, StatEntry)>> {
       if reply.len() != pending.len() {
           return Err(Error::Adapter {
               port: PortName::Digester,
               diagnostic: Diagnostic::new(format!(
                   "{} digests were returned for {} files",
                   reply.len(),
                   pending.len()
               )),
           });
       }
       Ok(pending
           .iter()
           .zip(reply)
           .map(|((path, stat), digest)| {
               (
                   *path,
                   StatEntry {
                       stat: *stat,
                       digest: *digest,
                   },
               )
           })
           .collect())
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::inputs
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 283 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/plan/inputs.rs crates/buildl-core/src/plan/mod.rs
   $ git commit -m "feat(buildl-core): decide which recorded input digests still stand"
   ```

### Task 2 — Build a target's own key components

**Files:**
- Create `crates/buildl-core/src/plan/components.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
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
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod authority;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod components;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod inputs;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod settings;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that makes a plan, which is not written yet"
       )
   )]
   mod tools;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/plan/components.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use super::{Gathered, own};
       use crate::plan::fixtures::{command, env_name, node, path, program, settings};
       use crate::types::{
           Digest, EnvName, EnvValue, InputDigest, NetworkAccess, OutputName, Platform, SourcePath,
           ToolIdentity,
       };

       /// What was gathered for a graph reading `src/a.c` and `src/z.c`, `HOME` (unset) and `PATH`,
       /// running `cc`, with the settings `mode` and `test_filter`.
       fn gathered<'g>(
           paths: &'g [SourcePath],
           names: &'g [EnvName],
           values: &[Option<&str>],
       ) -> Gathered<'g> {
           Gathered {
               settings: settings(&[("mode", "debug"), ("test_filter", "TestLogin")]),
               fingerprints: BTreeMap::from([(program("cc"), Digest::of(b"cc 14.2"))]),
               digests: paths
                   .iter()
                   .map(|path| (path, Digest::of(path.as_str().as_bytes())))
                   .collect(),
               env: names
                   .iter()
                   .zip(values)
                   .map(|(name, value)| (name, value.map(|text| EnvValue::parse(text).unwrap())))
                   .collect(),
               platform: Platform::parse("linux/x86_64").unwrap(),
           }
       }

       #[test]
       fn a_nodes_components_are_built_from_the_node_and_what_was_gathered() {
           let paths = [path("src/a.c"), path("src/z.c")];
           let names = [env_name("HOME"), env_name("PATH")];
           let gathered = gathered(&paths, &names, &[None, Some("/bin")]);
           let node = node("//:app", |node| {
               node.run = command(&["cc", "$in", "--filter=$opt:test_filter"]);
               node.inputs = vec![path("src/z.c"), path("src/a.c")];
               node.outputs = vec![
                   OutputName::parse("app").unwrap(),
                   OutputName::parse("app.map").unwrap(),
               ];
               node.env = vec![env_name("PATH"), env_name("HOME")];
               node.network = NetworkAccess::Declared;
           });

           let components = own(&node, &gathered);

           assert_eq!(components.command, node.run);
           assert_eq!(
               components.inputs,
               [
                   InputDigest {
                       path: path("src/z.c"),
                       digest: Digest::of(b"src/z.c"),
                   },
                   InputDigest {
                       path: path("src/a.c"),
                       digest: Digest::of(b"src/a.c"),
                   },
               ],
               "inputs keep their written order"
           );
           assert_eq!(components.outputs, node.outputs);
           assert_eq!(
               components.env,
               BTreeMap::from([
                   (env_name("HOME"), None),
                   (env_name("PATH"), Some(EnvValue::parse("/bin").unwrap())),
               ])
           );
           assert_eq!(
               components.tool,
               Some(ToolIdentity {
                   program: program("cc"),
                   fingerprint: Digest::of(b"cc 14.2"),
               })
           );
           assert_eq!(
               components.settings,
               settings(&[("test_filter", "TestLogin")]),
               "only the setting the command names"
           );
           assert_eq!(components.network, NetworkAccess::Declared);
           assert_eq!(components.platform, gathered.platform);
       }

       #[test]
       fn a_command_naming_no_program_has_no_tool() {
           let gathered = gathered(&[], &[], &[]);
           let node = node("//:app_test", |node| node.run = command(&["$out/app"]));
           assert_eq!(own(&node, &gathered).tool, None);
       }

       #[test]
       fn a_target_reading_nothing_has_empty_inputs_environment_and_settings() {
           let paths = [path("src/a.c")];
           let names = [env_name("PATH")];
           let gathered = gathered(&paths, &names, &[Some("/bin")]);
           let components = own(&node("//:app", |_| {}), &gathered);
           assert!(components.inputs.is_empty());
           assert!(components.env.is_empty());
           assert!(components.settings.is_empty());
           assert_eq!(components.network, NetworkAccess::Sealed);
       }

       #[test]
       fn the_order_a_node_lists_its_variables_in_does_not_reach_its_components() {
           let names = [env_name("HOME"), env_name("PATH")];
           let gathered = gathered(&[], &names, &[Some("/home/u"), Some("/bin")]);
           let forward = node("//:app", |node| {
               node.env = vec![env_name("HOME"), env_name("PATH")];
           });
           let backward = node("//:app", |node| {
               node.env = vec![env_name("PATH"), env_name("HOME")];
           });
           assert_eq!(own(&forward, &gathered), own(&backward, &gathered));
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib plan::components
   error[E0432]: unresolved imports `super::Gathered`, `super::own`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/plan/components.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! A target's own key components, built from what was gathered for the whole graph.
   //!
   //! Its own file because this is where the answers of five ports meet one target: by the time a
   //! node's components are built, every value they need has been fetched once for the graph, and
   //! building them asks nothing more.
   //!
   //! Responsibilities: [`Gathered`] and [`own`].
   //!
   //! Non-responsibilities: fetching the values, and a target's dependencies, which join its key
   //! only once it is known what they produced.

   use std::collections::BTreeMap;

   use crate::plan::settings::referenced;
   use crate::plan::tools::program_of;
   use crate::types::{
       Digest, EnvName, EnvValue, InputDigest, Node, OwnComponents, Platform, Program, SettingName,
       SettingValue, SourcePath, ToolIdentity,
   };

   /// Everything gathered for a graph before any target's components are built.
   #[derive(Debug)]
   pub(crate) struct Gathered<'g> {
       /// The value every declared setting has for this run.
       pub(crate) settings: BTreeMap<SettingName, SettingValue>,
       /// The fingerprint of every program a command names.
       pub(crate) fingerprints: BTreeMap<Program, Digest>,
       /// The content digest of every input.
       pub(crate) digests: BTreeMap<&'g SourcePath, Digest>,
       /// The value of every environment variable a target reads; `None` for one that is unset.
       pub(crate) env: BTreeMap<&'g EnvName, Option<EnvValue>>,
       /// The platform actions run on.
       pub(crate) platform: Platform,
   }

   /// The components of `node`'s key that do not wait on another target.
   ///
   /// Inputs and outputs keep the order they were written in. The environment is keyed by name, so
   /// the order `node` lists its variables in does not reach the key. Only the settings the command
   /// names are kept. An input or a program `gathered` holds nothing for is left out, and a variable
   /// it holds nothing for is taken as unset; neither happens when `gathered` was built for the
   /// graph `node` belongs to.
   pub(crate) fn own(node: &Node, gathered: &Gathered<'_>) -> OwnComponents {
       let inputs = node
           .inputs
           .iter()
           .filter_map(|path| {
               gathered.digests.get(path).map(|digest| InputDigest {
                   path: path.clone(),
                   digest: *digest,
               })
           })
           .collect();
       let env = node
           .env
           .iter()
           .map(|name| (name.clone(), gathered.env.get(name).cloned().flatten()))
           .collect();
       let tool = program_of(&node.run).and_then(|program| {
           let fingerprint = *gathered.fingerprints.get(&program)?;
           Some(ToolIdentity {
               program,
               fingerprint,
           })
       });
       OwnComponents {
           command: node.run.clone(),
           inputs,
           outputs: node.outputs.clone(),
           env,
           tool,
           settings: referenced(&node.run, &gathered.settings),
           network: node.network,
           platform: gathered.platform.clone(),
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib plan::components
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 289 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/plan/components.rs crates/buildl-core/src/plan/mod.rs
   $ git commit -m "feat(buildl-core): build a target's own key components"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- A recorded digest is reused for equal metadata and discarded for a later time, an earlier time
  and another size; a file with no entry is hashed.
- Inputs are sorted into reused and pending without changing their order; a reply is paired with
  the files that were sent, and a reply of the wrong length is `Error::Adapter` naming the
  digester.
- A node's components hold its inputs in written order, an unset variable as `None`, no tool for
  a command naming no program, and only the settings the command names; the order a node lists
  its variables in does not change them.
- `crates/expected-module-edges.txt` is unchanged.
