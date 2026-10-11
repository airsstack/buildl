---
status: done
created: 2026-10-11
depends-on: [03]
---

# Plan Errors Implementation Plan

**Goal:** Every way a plan can fail has an error variant with its required text.

**Architecture:** All in `error.rs`, the crate's one error file. Five variants join `Error` and none is reshaped: `UnknownOverride`, `Refused`, `MissingTool`, `MissingInput` and `Adapter`. `PortName` says which port an `Adapter` error came through. A site is boxed, as the existing variants box theirs, and so is the ceiling a refusal carries, which keeps `Error` no larger than it is today. `Refused` renders through one private function, because its text depends on the kind of want and on whether there is a site: a directory a build writes into is the workspace's, so no target declares it. One conversion is added beside the variants, `From<&Node> for DeclarationSite`: three files of the phase that plans name a graph node as the site of an error, and the conversion replaces three copies of one struct literal. The spec's §7 does not list it.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D15, D16, D20), §7 (errors, and every required rendering), §9 (the `error.rs` row).

**Checkpoints:** review once, when all three tasks are done.

---

## Context an implementer needs

What plans `01` and `03` and the existing file leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Error` (`#[non_exhaustive]`, `thiserror`), `DeclarationSite { label, provenance }`, the test helpers `root_site`, `root_file`, `lib_file`, `label` | `crates/buildl-core/src/error.rs` | the enum the variants join, and the fixtures the new tests reuse |
| `Program`, `SourcePath`, `SettingName`, `Diagnostic`, `Directory` (`is_root`, `as_str`) | `crates/buildl-core/src/types/` | fields of the new variants |
| `Want`, `Ceiling` | `crates/buildl-core/src/types/authority.rs` | what a refusal carries |
| `Node` (public fields `label`, `provenance`) | `crates/buildl-core/src/types/target_graph.rs` | what a site is made from |

Gate facts that bite here:

- `error -> types` is already in `crates/expected-module-edges.txt`, so this plan changes no
  module edge.
- The required texts of spec §7 are asserted character for character. Two of them are longer
  than rustfmt's line width; the tests split those string literals with a trailing `\`.
- A list in a refusal renders an empty ceiling list as `nothing` and the workspace root, whose
  text is empty, as `.`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/error.rs   — [modify] five variants, PortName, From<&Node> for DeclarationSite, three rendering helpers, unit tests
crates/buildl-core/src/lib.rs     — [modify] re-export PortName
```

### Task 1 — Add the override and adapter errors

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, widen the first import. Replace:

   ```rust
       use super::{
           ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
           EvaluationLimit, NameKind,
       };
   ```

   With:

   ```rust
       use super::{
           ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
           EvaluationLimit, NameKind, PortName,
       };
   ```

2. Add three tests directly above `fn name_kind_renders_a_human_phrase`. Insert:

   ```rust
       #[test]
       fn unknown_override_names_the_setting() {
           let err = Error::UnknownOverride {
               name: SettingName::parse("test_fliter").unwrap(),
           };
           assert_eq!(
               err.to_string(),
               "setting test_fliter is overridden but not declared"
           );
       }

       #[test]
       fn adapter_names_the_port_before_the_adapters_own_message() {
           let err = Error::Adapter {
               port: PortName::StatCache,
               diagnostic: Diagnostic::new("statcache.json: permission denied"),
           };
           assert_eq!(
               err.to_string(),
               "stat cache: statcache.json: permission denied"
           );
       }

       #[test]
       fn every_port_name_renders_a_human_phrase() {
           let cases = [
               (PortName::Manifest, "manifest"),
               (PortName::Host, "host"),
               (PortName::Digester, "digester"),
               (PortName::StatCache, "stat cache"),
               (PortName::ToolResolver, "tool resolver"),
           ];
           for (port, rendered) in cases {
               assert_eq!(port.to_string(), rendered);
           }
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0432]: unresolved import `super::PortName`
   error[E0599]: no variant named `UnknownOverride` found for enum `error::Error`
   error[E0599]: no variant named `Adapter` found for enum `error::Error`
   ```

4. Add the port names. In the same file, directly above the doc comment `/// A ceiling an evaluation can reach.`, insert:

   ```rust
   /// Which port an [`Error::Adapter`] came through.
   ///
   /// Non-exhaustive for the same reason [`Error`] is: a new port is not a breaking change.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum PortName {
       /// The workspace manifest.
       Manifest,
       /// The machine actions run on.
       Host,
       /// File metadata and content digests.
       Digester,
       /// The recorded digests of files already hashed.
       StatCache,
       /// The programs commands name.
       ToolResolver,
   }

   impl fmt::Display for PortName {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Manifest => "manifest",
               Self::Host => "host",
               Self::Digester => "digester",
               Self::StatCache => "stat cache",
               Self::ToolResolver => "tool resolver",
           })
       }
   }
   ```

5. Add the two variants at the end of `enum Error`, after `TooManyTargets`, so the end of the enum reads:

   ```rust
       /// The workspace declares more targets than the graph has ids for.
       #[error("{count} targets are more than a graph can number")]
       TooManyTargets {
           /// How many targets were declared.
           count: usize,
       },
       /// A setting was given a value for this run, but nothing declares it.
       #[error("setting {name} is overridden but not declared")]
       UnknownOverride {
           /// The setting that was named.
           name: SettingName,
       },
       /// An adapter could not do what a port asked of it.
       #[error("{port}: {diagnostic}")]
       Adapter {
           /// The port whose adapter failed.
           port: PortName,
           /// The adapter's message, for display only.
           diagnostic: Diagnostic,
       },
   }
   ```

6. Name the new type in the module doc. Directly below the two-line `DeclarationSite` bullet, insert:

   ```rust
   //! - [`PortName`] — which port an [`Error::Adapter`] came through.
   ```

7. Re-export it. In `crates/buildl-core/src/lib.rs`, add `PortName` to the `pub use error::{…}` list, then run `cargo fmt --all`. The list reads:

   ```rust
   pub use error::{
       ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
       EvaluationLimit, NameKind, PortName, Result,
   };
   ```

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 228 filtered out
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
   $ git add crates/buildl-core/src/error.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the unknown-override and adapter errors"
   ```

### Task 2 — Add the missing-tool and missing-input errors

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, widen the `crate::types` import. Replace:

   ```rust
       use crate::types::{
           DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance, SettingName,
           TargetName, Written,
       };
   ```

   With:

   ```rust
       use crate::types::{
           Argument, Command, DeclarationOrder, Diagnostic, Directory, FieldName, Freshness, Label,
           NetworkAccess, Node, OutputName, Program, Provenance, SettingName, SourcePath, TargetName,
           TargetRole, Written,
       };
   ```

2. Add three tests directly above `fn unknown_override_names_the_setting`. Insert:

   ```rust
       #[test]
       fn a_site_is_a_nodes_label_and_its_build_file() {
           let node = Node {
               label: label("//lib:text"),
               provenance: lib_file(),
               role: TargetRole::Build,
               run: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
               description: None,
               inputs: Vec::new(),
               outputs: vec![OutputName::parse("text").unwrap()],
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           };
           assert_eq!(
               DeclarationSite::from(&node),
               DeclarationSite {
                   label: label("//lib:text"),
                   provenance: lib_file(),
               }
           );
       }

       #[test]
       fn missing_tool_names_the_site_and_the_program() {
           let err = Error::MissingTool {
               site: Box::new(root_site("//:app")),
               program: Program::parse("cc").unwrap(),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: program cc was not found"
           );
       }

       #[test]
       fn missing_input_names_the_site_and_the_path() {
           let err = Error::MissingInput {
               site: Box::new(root_site("//:app")),
               path: SourcePath::parse("src/main.c").unwrap(),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: input src/main.c does not exist"
           );
       }
   ```

3. Run them and confirm they fail to build. The first error is the missing conversion:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0308]: mismatched types
   error[E0599]: no variant named `MissingTool` found for enum `error::Error`
   error[E0599]: no variant named `MissingInput` found for enum `error::Error`
   ```

4. Widen the import at the top of the file. Replace:

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance,
       SettingName, Written,
   };
   ```

   With:

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Node, Program,
       Provenance, SettingName, SourcePath, Written,
   };
   ```

5. Add the conversion directly above the doc comment ``/// Which port an [`Error::Adapter`] came through.`` Insert:

   ```rust
   impl From<&Node> for DeclarationSite {
       /// The site of a graph node: its label, and the build file that declared it.
       fn from(node: &Node) -> Self {
           Self {
               label: node.label.clone(),
               provenance: node.provenance.clone(),
           }
       }
   }
   ```

6. Add the two variants to `enum Error`, directly above `UnknownOverride`'s doc comment. Insert:

   ```rust
       /// A command names a program that cannot be found.
       #[error(
           "{}: {}: program {program} was not found",
           .site.provenance,
           .site.label
       )]
       MissingTool {
           /// The lowest-labelled target whose command names the program, and its build file.
           site: Box<DeclarationSite>,
           /// The program.
           program: Program,
       },
       /// A target reads a workspace file that does not exist.
       #[error(
           "{}: {}: input {path} does not exist",
           .site.provenance,
           .site.label
       )]
       MissingInput {
           /// The lowest-labelled target declaring the input, and its build file.
           site: Box<DeclarationSite>,
           /// The file.
           path: SourcePath,
       },
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 228 filtered out
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
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): add the missing-tool and missing-input errors"
   ```

### Task 3 — Add the refusal

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, widen the `crate::types` import again and import `BTreeSet`. Replace:

   ```rust
       use crate::types::{
           Argument, Command, DeclarationOrder, Diagnostic, Directory, FieldName, Freshness, Label,
           NetworkAccess, Node, OutputName, Program, Provenance, SettingName, SourcePath, TargetName,
           TargetRole, Written,
       };
   ```

   With:

   ```rust
       use std::collections::BTreeSet;

       use crate::types::{
           Argument, Ceiling, Command, DeclarationOrder, Diagnostic, Directory, EnvName, FieldName,
           Freshness, Label, NetworkAccess, Node, OutputName, Program, Provenance, SettingName,
           SourcePath, TargetName, TargetRole, Want, Written,
       };
   ```

2. Add a ceiling fixture and five tests directly above `fn unknown_override_names_the_setting`. Insert:

   ```rust
       /// A ceiling allowing `cc` and `ld`, `HOME` and `PATH`, reads under `src`, and no write.
       fn ceiling() -> Ceiling {
           Ceiling {
               run: ["cc", "ld"]
                   .into_iter()
                   .map(|raw| Program::parse(raw).unwrap())
                   .collect(),
               env: ["PATH", "HOME"]
                   .into_iter()
                   .map(|raw| EnvName::parse(raw).unwrap())
                   .collect(),
               read: BTreeSet::from([Directory::parse("src").unwrap()]),
               write: BTreeSet::new(),
           }
       }

       #[test]
       fn a_refused_program_names_the_site_and_what_may_run() {
           let err = Error::Refused {
               want: Want::Program(Program::parse("docker").unwrap()),
               site: Some(Box::new(root_site("//:image"))),
               ceiling: Box::new(ceiling()),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:image: program docker is outside the ceiling (proc.run allows: cc, ld)"
           );
       }

       #[test]
       fn a_refused_variable_names_the_site_and_what_may_be_read() {
           let err = Error::Refused {
               want: Want::Env(EnvName::parse("AWS_SECRET").unwrap()),
               site: Some(Box::new(root_site("//:app"))),
               ceiling: Box::new(ceiling()),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: environment variable AWS_SECRET is outside the ceiling \
                (env.read allows: HOME, PATH)"
           );
       }

       #[test]
       fn a_refused_input_names_the_site_and_where_inputs_may_lie() {
           let err = Error::Refused {
               want: Want::Read(SourcePath::parse("lib/key.pem").unwrap()),
               site: Some(Box::new(DeclarationSite {
                   label: label("//lib:text"),
                   provenance: lib_file(),
               })),
               ceiling: Box::new(ceiling()),
           };
           assert_eq!(
               err.to_string(),
               "lib/build.lua: //lib:text: input lib/key.pem is outside the ceiling \
                (fs.read allows: src)"
           );
       }

       #[test]
       fn a_refused_directory_has_no_site_and_an_empty_list_allows_nothing() {
           let err = Error::Refused {
               want: Want::Write(Directory::parse("out").unwrap()),
               site: None,
               ceiling: Box::new(ceiling()),
           };
           assert_eq!(
               err.to_string(),
               "directory out is outside the ceiling (fs.write allows: nothing)"
           );
       }

       #[test]
       fn the_workspace_root_renders_as_a_dot_in_a_refusal() {
           let mut allows_root = ceiling();
           allows_root.read = BTreeSet::from([Directory::root(), Directory::parse("src").unwrap()]);
           allows_root.write = BTreeSet::from([Directory::parse("out").unwrap()]);
           let read = Error::Refused {
               want: Want::Read(SourcePath::parse("lib/key.pem").unwrap()),
               site: Some(Box::new(root_site("//:app"))),
               ceiling: Box::new(allows_root.clone()),
           };
           assert_eq!(
               read.to_string(),
               "build.lua: //:app: input lib/key.pem is outside the ceiling (fs.read allows: ., src)"
           );
           let write = Error::Refused {
               want: Want::Write(Directory::root()),
               site: None,
               ceiling: Box::new(allows_root),
           };
           assert_eq!(
               write.to_string(),
               "directory . is outside the ceiling (fs.write allows: out)"
           );
       }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0599]: no variant named `Refused` found for enum `error::Error`
   ```

4. Widen the import at the top of the file. Replace:

   ```rust
   use crate::types::{
       Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Node, Program,
       Provenance, SettingName, SourcePath, Written,
   };
   ```

   With:

   ```rust
   use crate::types::{
       Ceiling, Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Node, Program,
       Provenance, SettingName, SourcePath, Want, Written,
   };
   ```

5. Add the variant to `enum Error`, directly above `MissingTool`'s doc comment. Insert:

   ```rust
       /// A build wants something its workspace's ceiling does not allow.
       #[error("{}", refusal(.want, .site.as_deref(), .ceiling))]
       Refused {
           /// What was wanted.
           want: Want,
           /// The lowest-labelled target wanting it, and its build file; `None` for a directory
           /// the build writes into, which the workspace names and no target declares.
           site: Option<Box<DeclarationSite>>,
           /// What the workspace allows.
           ceiling: Box<Ceiling>,
       },
   ```

6. Add the three functions that render it, directly above the doc comment `/// What a label was declared as.` Insert:

   ```rust
   /// A refusal as one sentence: who wanted what, and what the ceiling allows of that kind.
   fn refusal(want: &Want, site: Option<&DeclarationSite>, ceiling: &Ceiling) -> String {
       let (wanted, list, allowed) = match want {
           Want::Program(program) => (
               format!("program {program}"),
               "proc.run",
               allowed_list(&ceiling.run),
           ),
           Want::Env(name) => (
               format!("environment variable {name}"),
               "env.read",
               allowed_list(&ceiling.env),
           ),
           Want::Read(path) => (
               format!("input {path}"),
               "fs.read",
               allowed_list(ceiling.read.iter().map(directory_text)),
           ),
           Want::Write(directory) => (
               format!("directory {}", directory_text(directory)),
               "fs.write",
               allowed_list(ceiling.write.iter().map(directory_text)),
           ),
       };
       let holder = site.map_or_else(String::new, |site| {
           format!("{}: {}: ", site.provenance, site.label)
       });
       format!("{holder}{wanted} is outside the ceiling ({list} allows: {allowed})")
   }

   /// The entries of one ceiling list, separated by commas; `nothing` for an empty list.
   fn allowed_list<T: fmt::Display>(entries: impl IntoIterator<Item = T>) -> String {
       let rendered: Vec<String> = entries.into_iter().map(|entry| entry.to_string()).collect();
       if rendered.is_empty() {
           "nothing".to_owned()
       } else {
           rendered.join(", ")
       }
   }

   /// A directory as a refusal shows it: the workspace root, whose text is empty, as `.`.
   fn directory_text(directory: &Directory) -> &str {
       if directory.is_root() {
           "."
       } else {
           directory.as_str()
       }
   }
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 228 filtered out
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
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): add the refusal of a want outside the ceiling"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- Each rendering spec §7 requires is asserted character for character: the unknown override; a
  refused program, variable, input and directory; the missing tool; the missing input; the adapter
  failure.
- A refusal of a directory names no target and no build file; an empty ceiling list renders as
  `nothing`; the workspace root renders as `.`.
- `PortName` renders `manifest`, `host`, `digester`, `stat cache` and `tool resolver`.
- `DeclarationSite::from(&node)` is the node's label and build file.
- No existing variant changed shape.

## Review findings

One reviewer pass over the plan's whole diff, 2026-10-11. Verdict: compliant with the plan, blocking set empty, nothing at bug or risk tier. The reviewer's own gate run: `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 5.16 seconds.`), `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`).

- reversion-guard (nit) — nothing fails today if the `PortName` re-export is dropped; plan `06`'s doc links to `crate::PortName::…` close this under the gate's rustdoc step — `lib.rs:74`
- plan conformance (nit) — `use std::collections::BTreeSet;` sits beside `use std::path::PathBuf;` in the test module, not directly above the `crate::types` import as task 3 step 1's block shows; no behaviour change — `error.rs:627`

No finding was fixed: none is in the blocking set.

## Probe results

No throwaway test was written. Every claim the tasks make about existing code was settled by the task's own red-green cycle, each of which printed the failure and the pass the plan quotes.

- The three tasks leave the tree the plan was written from, but for one import line — `diff -r crates <prototype snapshot after plan 05>/crates` printed only the placement of `use std::collections::BTreeSet;` in `error.rs`'s test module (line 627 in the tree, line 633 in the prototype). Against the plan's text in that one line; see Deviations.

## Deviations

- 2026-10-11 — task 3 step 1: the coder placed `use std::collections::BTreeSet;` with the test module's other `std` import instead of between the `super` and `crate::types` imports, and reported no deviation. Kept: it groups the `std` imports the way the rest of the file does, the gate is green, and no later plan of the chain edits that import block (`grep -l 'Argument, Ceiling, Command, DeclarationOrder' plans/*.md` names this plan only).
- 2026-10-11 — the commit step of every task was not run. The work was left in the working tree, uncommitted, because committing is the author's act in this run.
