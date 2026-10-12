---
status: done
created: 2026-10-11
depends-on: [01]
---

# Errors Implementation Plan

**Goal:** The crate's error type names every new way a build can fail.

**Architecture:** Everything is in `error.rs`, the crate's one error file. `PortName` gains five variants, one per port a build reaches that a plan does not: the action cache, the content store, the event log, the execution strategy and the dispatcher. `Error` gains three variants and none is reshaped. `ForeignPlan` has no field: a plan handed over with a graph it was not made from. `OutputConflict` holds the path two outputs both need and the sites of the two targets, lower label first; one target declaring both outputs names itself twice. `NoProgram` holds the site of a command that names no program once its placeholders are expanded. Sites are boxed, as `MissingTool` and `MissingInput` box theirs, so the crate's `Result` stays small. `Refused` is unchanged: a program a command names only once it is expanded is refused with the same variant and the same sentence as any other program, and a test pins that sentence for this case. There is no outcome class for a refusal anywhere. Every `Display` text below is a requirement, pinned by a test. Nothing raises the new variants yet; they are variants of a public enum, so the build does not report them as unused.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D4, D5, D14, D20, D22), §7 (errors), §9 (the row for `error.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What plan `01` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `OutputPath` (`Display` is its text; built only by `OutputPath::of(&Directory, &Label, &OutputName)`) | `crates/buildl-core/src/types/output_path.rs`, re-exported from `crate::types` | the `path` field of `OutputConflict` |
| `DeclarationSite { label, provenance }` (public fields; `From<&Node>`) | `crates/buildl-core/src/error.rs` | the sites the new variants hold |
| `Error::MissingTool { site: Box<DeclarationSite>, program }`, rendered `{site.provenance}: {site.label}: program … was not found` | `crates/buildl-core/src/error.rs` | the model for a boxed site and for naming a field of it in `#[error(…)]` |
| `Error::Refused { want, site, ceiling }` and its `refusal` helper | `crates/buildl-core/src/error.rs` | unchanged; task 2 adds one test of it |
| `PortName` with `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, its `Display`, and the test `every_port_name_renders_a_human_phrase` | `crates/buildl-core/src/error.rs` | what task 1 extends |
| The test module's helpers `label(raw)`, `root_site(raw)` (a site declared in `build.lua`) and `lib_file()` (the provenance of `lib/build.lua`) | `crates/buildl-core/src/error.rs` | what the new tests build their values from |

Gate facts that bite here:

- `Error` derives `Debug` and `thiserror::Error` and nothing else, so a test compares
  `err.to_string()` with the expected text, never two errors with `==`.
- Inside `#[error(…)]`, a field of a boxed struct field is written `.first.label`: thiserror
  reads through the `Box`. The attributes below are the ones that compiled.
- `Error` and `PortName` are both `#[non_exhaustive]` and nothing in the workspace matches on
  either without a wildcard: the gate passed with `error.rs` as the only changed file.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not. Task 2's run selects every test of `error.rs`, the 32 it had and the five this task adds.

## File structure

```
crates/buildl-core/src/error.rs   — [modify] five PortName variants; ForeignPlan, OutputConflict, NoProgram; unit tests
```

### Task 1 — Name the five ports a build reaches

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing assertions. In `crates/buildl-core/src/error.rs`, in the test `every_port_name_renders_a_human_phrase`, add five cases after the `PortName::ToolResolver` case, so the test reads:

   ```rust
       #[test]
       fn every_port_name_renders_a_human_phrase() {
           let cases = [
               (PortName::Manifest, "manifest"),
               (PortName::Host, "host"),
               (PortName::Digester, "digester"),
               (PortName::StatCache, "stat cache"),
               (PortName::ToolResolver, "tool resolver"),
               (PortName::ActionCache, "action cache"),
               (PortName::ContentStore, "content store"),
               (PortName::EventLog, "event log"),
               (PortName::ExecStrategy, "exec strategy"),
               (PortName::Dispatcher, "dispatcher"),
           ];
           for (port, rendered) in cases {
               assert_eq!(port.to_string(), rendered);
           }
       }
   ```

2. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::every_port_name
   error[E0599]: no variant, associated function, or constant named `ActionCache` found for enum `PortName` in the current scope
   error[E0599]: no variant, associated function, or constant named `ContentStore` found for enum `PortName` in the current scope
   error[E0599]: no variant, associated function, or constant named `EventLog` found for enum `PortName` in the current scope
   error[E0599]: no variant, associated function, or constant named `ExecStrategy` found for enum `PortName` in the current scope
   error[E0599]: no variant, associated function, or constant named `Dispatcher` found for enum `PortName` in the current scope
   ```

3. Add the variants. In the same file, extend `enum PortName` after its last variant, so the end of the enum reads:

   ```rust
       /// The programs commands name.
       ToolResolver,
       /// The recorded results of earlier builds.
       ActionCache,
       /// The store of content by digest: outputs, and what actions printed.
       ContentStore,
       /// The build log.
       EventLog,
       /// What runs an action.
       ExecStrategy,
       /// What hands actions to workers.
       Dispatcher,
   }
   ```

4. Extend `impl fmt::Display for PortName`, so the end of its `match` reads:

   ```rust
               Self::ToolResolver => "tool resolver",
               Self::ActionCache => "action cache",
               Self::ContentStore => "content store",
               Self::EventLog => "event log",
               Self::ExecStrategy => "exec strategy",
               Self::Dispatcher => "dispatcher",
           })
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib error::tests::every_port_name
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 343 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): name the five ports a build reaches"
   ```

### Task 2 — Add the errors a build raises before anything runs

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/error.rs`, add `OutputPath` to the `use crate::types::{…}` list, so it reads:

   ```rust
       use crate::types::{
           Argument, Ceiling, Command, DeclarationOrder, Diagnostic, Directory, EnvName, FieldName,
           Freshness, Label, NetworkAccess, Node, OutputName, OutputPath, Program, Provenance,
           SettingName, SourcePath, TargetName, TargetRole, Want, Written,
       };
   ```

   Then add five tests directly above the test `adapter_names_the_port_before_the_adapters_own_message`. The last of the five passes already: it pins the sentence `Refused` renders for a program a command names only once it is expanded.

   ```rust
       #[test]
       fn a_foreign_plan_says_the_plan_does_not_belong_to_the_graph() {
           assert_eq!(
               Error::ForeignPlan.to_string(),
               "the plan was not made from this graph"
           );
       }

       #[test]
       fn an_output_conflict_names_the_path_and_both_targets_with_their_files() {
           let err = Error::OutputConflict {
               path: OutputPath::of(
                   &Directory::parse("out").unwrap(),
                   &label("//:lib"),
                   &OutputName::parse("lib").unwrap(),
               ),
               first: Box::new(root_site("//:lib")),
               second: Box::new(DeclarationSite {
                   label: label("//lib:text"),
                   provenance: lib_file(),
               }),
           };
           assert_eq!(
               err.to_string(),
               "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
           );
       }

       #[test]
       fn an_output_conflict_inside_one_target_names_that_target_twice() {
           let err = Error::OutputConflict {
               path: OutputPath::of(
                   &Directory::parse("out").unwrap(),
                   &label("//:app"),
                   &OutputName::parse("app").unwrap(),
               ),
               first: Box::new(root_site("//:app")),
               second: Box::new(root_site("//:app")),
           };
           assert_eq!(
               err.to_string(),
               "out/app: //:app (build.lua) and //:app (build.lua) cannot both write here"
           );
       }

       #[test]
       fn no_program_names_the_site_of_the_command() {
           let err = Error::NoProgram {
               site: Box::new(root_site("//:app")),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app: the command names no program once expanded"
           );
       }

       #[test]
       fn a_program_a_command_names_only_once_expanded_is_refused_like_any_other() {
           let err = Error::Refused {
               want: Want::Program(Program::parse("out/app").unwrap()),
               site: Some(Box::new(root_site("//:app_test"))),
               ceiling: Box::new(Ceiling {
                   run: BTreeSet::from([Program::parse("cc").unwrap()]),
                   env: BTreeSet::new(),
                   read: BTreeSet::new(),
                   write: BTreeSet::new(),
               }),
           };
           assert_eq!(
               err.to_string(),
               "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
           );
       }
   ```

2. Run the file's tests and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib error::tests
   error[E0599]: no variant, associated function, or constant named `ForeignPlan` found for enum `error::Error` in the current scope
   error[E0599]: no variant named `OutputConflict` found for enum `error::Error`
   error[E0599]: no variant named `OutputConflict` found for enum `error::Error`
   error[E0599]: no variant named `NoProgram` found for enum `error::Error`
   ```

3. Import the path type. At the top of the same file, add `OutputPath` to the `use crate::types::{…}` list, so it reads:

   ```rust
   use crate::types::{
       Ceiling, Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Node,
       OutputPath, Program, Provenance, SettingName, SourcePath, Want, Written,
   };
   ```

4. Add the variants. In `enum Error`, insert three variants directly above `Adapter`, so the end of the enum reads:

   ```rust
       /// A setting was given a value for this run, but nothing declares it.
       #[error("setting {name} is overridden but not declared")]
       UnknownOverride {
           /// The setting that was named.
           name: SettingName,
       },
       /// A plan was handed over with a graph it was not made from.
       #[error("the plan was not made from this graph")]
       ForeignPlan,
       /// Two outputs would be written to one place: the same path, or a path that another output
       /// needs as a directory.
       #[error(
           "{path}: {} ({}) and {} ({}) cannot both write here",
           .first.label,
           .first.provenance,
           .second.label,
           .second.provenance
       )]
       OutputConflict {
           /// The shorter of the two paths: the one both outputs need.
           path: OutputPath,
           /// The lower-labelled of the two targets, and its build file.
           first: Box<DeclarationSite>,
           /// The other target, and its build file; the same target when it declares both outputs.
           second: Box<DeclarationSite>,
       },
       /// A command names no program once its placeholders are expanded: it expanded to no
       /// argument, or its first argument is empty or still holds a `$`.
       #[error(
           "{}: {}: the command names no program once expanded",
           .site.provenance,
           .site.label
       )]
       NoProgram {
           /// The target whose command it is, and its build file.
           site: Box<DeclarationSite>,
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

5. Run `cargo fmt --all`, then run the file's tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib error::tests
   test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 312 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): add the errors a build raises before anything runs"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `PortName::{ActionCache, ContentStore, EventLog, ExecStrategy, Dispatcher}` render as
  `action cache`, `content store`, `event log`, `exec strategy` and `dispatcher`.
- `Error::ForeignPlan` renders `the plan was not made from this graph`.
- `Error::OutputConflict` renders
  `out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here`, and names
  one target twice when it declares both outputs.
- `Error::NoProgram` renders `build.lua: //:app: the command names no program once expanded`.
- `Error::Refused` with `Want::Program` renders
  `build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)`.
- `error.rs` is the only file changed; `crates/buildl-core/Cargo.toml` and
  `crates/expected-module-edges.txt` are unchanged, because `error` already imports `types`.

## Review findings

Reviewed once, 2026-10-12, over both tasks. The reviewer re-ran the gate: `cargo make dod`
exit 0 (`[cargo-make] INFO - Build Done in 5.06 seconds.`), `cargo deny check` exit 0
(`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan: the diff
of `error.rs` is the plan's code blocks in the plan's positions; no drift.

- nit — `DeclarationSite`'s doc, and its line in the module doc list, say a site names "the one
  holding a bad reference, or one target on a dependency cycle"; `OutputConflict` and
  `NoProgram` now hold sites that are neither — `error.rs`. Left as it is: the sentence was
  already incomplete before this plan (`Refused`, `MissingTool`, `MissingInput`), and the plan's
  code blocks do not touch it.

## Probe results

No separate probe was run. The facts the tasks assert about existing code (`PortName`, its
`Display` match, the test helpers, `DeclarationSite`, `Error::Refused`) are ones their own
compile and test runs depend on:

- Task 1 red: `cargo test -p buildl-core --lib error::tests::every_port_name` failed to build
  with five `error[E0599]` lines, one each for `ActionCache`, `ContentStore`, `EventLog`,
  `ExecStrategy`, `Dispatcher` — as the plan quotes.
- Task 1 green: `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 343 filtered out`
  — as the plan quotes.
- Task 2 red: `cargo test -p buildl-core --lib error::tests` failed to build with `E0599` for
  `ForeignPlan`, `OutputConflict` (twice) and `NoProgram` — as the plan quotes.
- Task 2 green: `test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 312 filtered out`
  — as the plan quotes.

## Deviations

- 2026-10-12 — the commit step of both tasks was skipped. The work is in the tree, uncommitted;
  committing is left to the author.
- 2026-10-12 — started while plan 01, which this plan depends on, was built and reviewed but not
  yet marked `done`: the author asked for plans 01–05 in one run.
