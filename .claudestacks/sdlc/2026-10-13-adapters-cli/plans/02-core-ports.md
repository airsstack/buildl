---
status: done
created: 2026-10-14
depends-on: [01]
---

# Core Ports Implementation Plan

**Goal:** Every port of `buildl-core` has the signature a real adapter is written against, with a fake that honours it.

**Architecture:** Three ports change shape and two are new. `Digester::stat` returns an `InputStat` instead of an `Option<FileStat>`, so a path that holds a directory, a link or a special file reaches Plan as an answer and not as an adapter's failure; Plan, which knows the declaring target, raises `Error::InputNotAFile` for it beside `Error::MissingInput`. `Dispatcher::dispatch` gains a message type: `work` is handed, with its task, a `&(dyn Fn(M) + Sync)` to tell the calling thread something while the task still runs, and `step` takes a `Turn<M, R>` instead of an `Option<R>`. `ExecStrategy::run` gains `live: Option<LiveSink<'_>>`, where a strategy hands what the command writes as it writes it. `Reporter` is told how a build goes, one infallible call at a time, and a `&mut` to a reporter is a reporter. `GraphRecord::record` replaces the kept graph of a workspace. `Ports` gains `Graph` and `Reporter`, and `PortSet` gains the fields `graph` and `reporter`: fourteen ports. Every fake lives in `tests/flows/common.rs`, is built from the public API alone, and is held to its port's contract by a test beside it: `FakeDigester` gains `others`, `FakeDispatcher` hands over what a task said before its result and gains `pace`, `FakeStrategy` hands a live sink the bytes it returns and can end a command by a signal, and `FakeGraphRecord` and `FakeReporter` are new. A changed signature changes its trait, its fake, the doc example of the bundle, the stub of the set's own test and every caller in one commit. Callers in the phases change only as far as they must to compile: a worker tells the scheduling thread nothing yet and runs every command with no live sink, and no phase calls the two new ports. Plan `03` does that.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`, `cargo-deny`. No new dependency.

**Content authority:** spec §2.2 (`Reporter`, its fake), §2.3 (`Dispatcher`, `Pace`), §2.4 (`ExecStrategy::run`), §2.7 (`GraphRecord`, the two associated types and fields), §2.9 (the sentences added to four contracts), §2.10 (the files), §2.11 (the fakes held to their contracts, the reporter's unit test, the flow of §2.12), §2.12 (`Digester::stat`, Plan's three answers).

---

## Context an implementer needs

What plan `01` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Turn<M, R>::{Begin, Told(M), Done(R)}` | `crates/buildl-core/src/types/turn.rs` | what `Dispatcher::dispatch` hands `step` |
| `Channel::{Stdout, Stderr}`, `LiveSink<'a> = &'a (dyn Fn(Channel, &[u8]) + Sync)`, `Captured<'a> { stdout, stderr }` | `crates/buildl-core/src/types/{channel,live_sink,captured}.rs` | the argument of `ExecStrategy::run`; what `Reporter::printed` and `Reporter::finished` are handed |
| `InputStat::{Missing, File(FileStat), Other(InputKind)}`, `InputKind::{Symlink, Directory, Special}` | `crates/buildl-core/src/types/input_stat.rs` | what `Digester::stat` returns |
| `Exit::Signaled(i32)`, `OutcomeClass::Killed { signal }` | `crates/buildl-core/src/types/{raw_outcome,outcome}.rs` | what a scripted signal ends a fake command with |
| `Error::InputNotAFile { site, path, found }`, `PortName::GraphRecord` | `crates/buildl-core/src/error.rs` | what Plan raises; what the fake graph record's failure names |
| `digests(inputs, digester, stat_cache)` | `crates/buildl-core/src/plan/inputs.rs` | the one caller of `Digester::stat` |
| `execute(graph, plan, on_failure, ports)` and `Build::step` | `crates/buildl-core/src/execute/assembly.rs` | the one caller of `Dispatcher::dispatch` |
| `run(task, strategy, cas)` | `crates/buildl-core/src/execute/perform.rs` | the one caller of `ExecStrategy::run` |
| The implementations of the three changed ports | `crates/buildl-core/tests/flows/common.rs` (`FakeDigester`, `FakeDispatcher`, `FakeStrategy`), the doc example of `crates/buildl-core/src/ports/bundle.rs` (`Nothing`), the `tests` module of `crates/buildl-core/src/ports/port_set.rs` (`Stub`) | what a signature change changes together |
| The implementations of `Ports` and the `PortSet` values | `FakePorts` and `ports()` in `common.rs`; `NothingPorts` in the doc example of `bundle.rs`; `UndebuggableBundle` and the set of the one test in `port_set.rs` | what tasks 10 and 11 change together |
| The fakes' tests | the `tests` module at the bottom of `crates/buildl-core/tests/flows/common.rs`, run as `common::tests::…` in the flows binary | where each fake is held to its port's contract |

`grep -rn 'impl.*Digester for\|impl.*Dispatcher for\|impl.*ExecStrategy for\|impl.*Ports for\|PortSet {\|PortSet::<' --include='*.rs' crates` finds fifteen lines: those places, and the `let PortSet {` with which `execute/assembly.rs` takes a set apart, which ends in `..` and needs no change for a new field. The same search for `impl.*DeclarationSource for` finds `crates/buildl-lua/src/source.rs`, the one implementation of a port outside this crate, so the search does find what is there; `crates/buildl` and `crates/buildl-cli` implement no port and build no set. So no crate but `buildl-core` changes, and `cargo make dod`, which builds the whole workspace, proves after every task that the other three still compile.

Facts of the gate that bite here:

- The doc example of `ports/bundle.rs` is compiled by `cargo test --doc`, and rustfmt does not format it: its `use buildl_core::{…}` list is kept sorted and wrapped by hand, and each step that changes it shows the whole list.
- `tests/flows/main.rs` declares `pub mod common;`, so a fake that no flow uses yet is not reported as dead code. The gate passed after each task with the new fakes used only by their own tests.
- Clippy denies `unwrap_used` in the fakes as everywhere: a fake takes a lock with `unwrap_or_else(PoisonError::into_inner)`. The fakes' `tests` module expects the lint, so its tests unwrap.
- `cargo fmt --all` sorts and rewraps a `use` list, and moves a new `use` statement to its place among its neighbours. A step adds a name and runs it; the blocks below show the result.
- `guard-module-edges` compares the crate's module-to-module imports with `crates/expected-module-edges.txt`. Every import this plan adds is `ports -> types`, `ports -> error`, `plan -> types` or `execute -> types`, all listed already, so that file does not change. A module named only in a doc comment is not an import.
- A flow's `pipeline.plan(…)` and `pipeline.build(…)` keep the signatures they have: this plan changes neither. Plan `03` changes both, and with them the one flow this plan adds to `tests/flows/plan.rs`.
- Two things belong to plan `03` by subject and are here by necessity. `crates/buildl-core/src/plan/inputs.rs` must name all three answers of `stat` in the commit that changes the trait, so the refusal of an input that is not a regular file, its flow and the line of `plan/assembly.rs` that lists it are in task 1. `crates/buildl-core/src/execute/assembly.rs` and `perform.rs` must call the dispatcher and the strategy by their new signatures, so tasks 2 and 4 change those calls, and nothing else there.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes, in a copy of the workspace walked through these tasks in order; a failing run is quoted by the error lines that name the cause, not in full. `cargo make dod` was run after every task and exited `0` with the counts each gate step gives. `cargo deny check` was run on the tree plan `01` starts from and on the tree plan `03` leaves, and printed `advisories ok, bans ok, licenses ok, sources ok` both times; no task of the three plans changes a manifest or the lockfile.

Before task 1 the three `buildl-core` results of `cargo make dod` read `543 passed` (unit tests), `108 passed` (flows) and `13 passed` (doctests).

## File structure

```
crates/buildl-core/src/ports/digester.rs        — [modify] stat returns InputStat
crates/buildl-core/src/ports/dispatcher.rs      — [modify] a message type; step takes a Turn
crates/buildl-core/src/ports/exec_strategy.rs   — [modify] run takes a live sink
crates/buildl-core/src/ports/manifest.rs        — [modify] one sentence of contract
crates/buildl-core/src/ports/content_store.rs   — [modify] two sentences of contract
crates/buildl-core/src/ports/stat_cache.rs      — [modify] one sentence of contract
crates/buildl-core/src/ports/reporter.rs        — [create] Reporter, its blanket implementation for &mut, its test
crates/buildl-core/src/ports/graph_record.rs    — [create] GraphRecord
crates/buildl-core/src/ports/bundle.rs          — [modify] two associated types; the doc example
crates/buildl-core/src/ports/port_set.rs        — [modify] two fields; Debug; the test stub
crates/buildl-core/src/ports/mod.rs             — [modify] declare, index and re-export the two new ports
crates/buildl-core/src/plan/inputs.rs           — [modify] the three answers of stat
crates/buildl-core/src/plan/assembly.rs         — [modify] the error order's fifth step
crates/buildl-core/src/execute/assembly.rs      — [modify] dispatch by its new signature
crates/buildl-core/src/execute/perform.rs       — [modify] run by its new signature
crates/buildl-core/src/lib.rs                   — [modify] re-export the two new ports
crates/buildl-core/tests/flows/common.rs        — [modify] the fakes' new signatures, FakeDigester's others, Pace, Ends::Signaled, FakeGraphRecord, FakeReporter, FakePorts, ports(), the fakes' tests
crates/buildl-core/tests/flows/plan.rs          — [modify] the flow of an input that is not a regular file
```

### Task 1 — Have the digester say what stands at a path

**Files:**
- Modify `crates/buildl-core/src/plan/assembly.rs`
- Modify `crates/buildl-core/src/plan/inputs.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/digester.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/tests/flows/plan.rs`

**Steps:**

1. Write the failing test of the fake. A path may be named in a new field, `others`, as holding something other than a regular file. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
           Order, Script, dir,
       };
       use super::{Leaf, tree};
       use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile, TreePath};

       fn label(raw: &str) -> Label {
   ```

   With:

   ```rust
           Order, Script, dir,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
       use buildl_core::{
           Digester, FileMode, FileStat, Found, FoundEntry, FoundKind, InputKind, InputStat,
           StoredFile, TreePath,
       };

       fn label(raw: &str) -> Label {
   ```

   Replace:

   ```rust
           let nothing = Digest::of(b"");
           CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
       }

       #[test]
   ```

   With:

   ```rust
           let nothing = Digest::of(b"");
           CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
       }

       #[test]
       fn a_digester_reports_a_file_nothing_and_what_is_not_a_file() {
           let stat = FileStat {
               modified: Timestamp::from_unix_nanos(7),
               size: 4,
           };
           let mut digester = FakeDigester::default();
           digester.files.insert(
               path("main.c"),
               FakeContent {
                   bytes: b"main".to_vec(),
                   stat,
               },
           );
           digester.others.insert(path("src"), InputKind::Directory);
           digester
               .others
               .insert(path("main.c.link"), InputKind::Symlink);

           assert_eq!(
               digester.stat(&path("main.c")).unwrap(),
               InputStat::File(stat)
           );
           assert_eq!(digester.stat(&path("gone.c")).unwrap(), InputStat::Missing);
           assert_eq!(
               digester.stat(&path("src")).unwrap(),
               InputStat::Other(InputKind::Directory)
           );
           assert_eq!(
               digester.stat(&path("main.c.link")).unwrap(),
               InputStat::Other(InputKind::Symlink)
           );
           assert_eq!(digester.stat_calls(), 4);
       }

       #[test]
   ```

2. Write the failing flow: a plan over an input that is a directory, a link or a special file ends in the error naming the target that declared it, and nothing is hashed. In `crates/buildl-core/tests/flows/plan.rs`:

   Replace:

   ```rust
   use buildl_core::{
       ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, EnvName,
       EnvValue, Error, EvaluationFailure, FileStat, Freshness, KeyComponents, Label, NodeId,
       OwnComponents, Pipeline, Plan, PlannedKey, Platform, PortName, SettingName, SettingValue,
       SourcePath, StagedItem, StagedSetting, TargetGraph, Timestamp, ToolIdentity, Verdict, Want,
       Written, json,
   };
   use buildl_core::{Execution, Isolation, Manifest, Timeout, Workers};
   ```

   With:

   ```rust
   use buildl_core::{
       ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, EnvName,
       EnvValue, Error, EvaluationFailure, FileStat, Freshness, InputKind, KeyComponents, Label,
       NodeId, OwnComponents, Pipeline, Plan, PlannedKey, Platform, PortName, SettingName,
       SettingValue, SourcePath, StagedItem, StagedSetting, TargetGraph, Timestamp, ToolIdentity,
       Verdict, Want, Written, json,
   };
   use buildl_core::{Execution, Isolation, Manifest, Timeout, Workers};
   ```

   Replace:

   ```rust
       let site = match error {
           Error::Refused { site, .. } => site.as_deref(),
           Error::MissingTool { site, .. } | Error::MissingInput { site, .. } => Some(&**site),
           _ => None,
       };
   ```

   With:

   ```rust
       let site = match error {
           Error::Refused { site, .. } => site.as_deref(),
           Error::MissingTool { site, .. }
           | Error::MissingInput { site, .. }
           | Error::InputNotAFile { site, .. } => Some(&**site),
           _ => None,
       };
   ```

   Replace:

   ```rust
           matches!(error, Error::MissingTool { program: missing, .. } if missing == program("cc"))
       );
   }

   #[test]
   ```

   With:

   ```rust
           matches!(error, Error::MissingTool { program: missing, .. } if missing == program("cc"))
       );
   }

   #[test]
   fn an_input_that_is_not_a_regular_file_names_the_target_declaring_it_and_what_it_is() {
       for (found, text) in [
           (
               InputKind::Directory,
               "build.lua: //:lib: input lib.c is a directory; an input must be a regular file",
           ),
           (
               InputKind::Symlink,
               "build.lua: //:lib: input lib.c is a symbolic link; an input must be a regular file",
           ),
           (
               InputKind::Special,
               "build.lua: //:lib: input lib.c is a pipe, a socket or a device; an input must be a regular file",
           ),
       ] {
           let mut pipeline = chain();
           pipeline
               .ports_mut()
               .digester
               .others
               .insert(path("lib.c"), found);

           let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

           assert_eq!(error.to_string(), text);
           assert_eq!(
               site_of(&error),
               Some(("//:lib".to_owned(), Path::new("build.lua")))
           );
           assert!(matches!(
               error,
               Error::InputNotAFile { path: named, found: kind, .. }
                   if named == path("lib.c") && kind == found
           ));
           assert_eq!(pipeline.ports().digester.digest_calls(), 0);
       }
   }

   #[test]
   ```

3. Run the flows and confirm they fail to build, because the fake has no `others` and `stat` still answers with an `Option`:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0609]: no field `others` on type `FakeDigester`
   error[E0308]: mismatched types
   error: could not compile `buildl-core` (test "flows") due to 7 previous errors
   ```

4. Change the port. `crates/buildl-core/src/ports/digester.rs`:

   Replace:

   ```rust
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{Digest, FileStat, SourcePath};

   /// Reads the metadata and the content digests of workspace files.
   pub trait Digester {
       /// The metadata of the file at `path`; `Ok(None)` when no file is there.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Digester`](crate::PortName::Digester), when the metadata cannot be read.
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>>;

       /// The content digest of each file in `paths`, in the order given.
   ```

   With:

   ```rust
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{Digest, InputStat, SourcePath};

   /// Reads the metadata and the content digests of workspace files.
   pub trait Digester {
       /// What stands at `path`, following no symbolic link: nothing, a regular file with its
       /// metadata, or something that is not a regular file, with what it is.
       ///
       /// Only a regular file is an input. The other two answers are not failures of this port:
       /// the caller turns each into the error naming the target that declared the input.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Digester`](crate::PortName::Digester), when the metadata cannot be read.
       fn stat(&self, path: &SourcePath) -> Result<InputStat>;

       /// The content digest of each file in `paths`, in the order given.
   ```

5. Have Plan read the three answers. A regular file goes on as it did; nothing is `Error::MissingInput` as before; anything else is `Error::InputNotAFile`, with the same site. In `crates/buildl-core/src/plan/inputs.rs`:

   Replace:

   ```rust
   use crate::error::{DeclarationSite, Error, PortName, Result};
   use crate::ports::{Digester, StatCache};
   use crate::types::{Diagnostic, Digest, FileStat, Node, SourcePath, StatEntry};

   /// The content digest of every input in `inputs`.
   ```

   With:

   ```rust
   use crate::error::{DeclarationSite, Error, PortName, Result};
   use crate::ports::{Digester, StatCache};
   use crate::types::{Diagnostic, Digest, FileStat, InputStat, Node, SourcePath, StatEntry};

   /// The content digest of every input in `inputs`.
   ```

   Replace:

   ```rust
   /// # Errors
   ///
   /// Returns [`Error::MissingInput`] for the first input that does not exist, naming the
   /// lowest-labelled target declaring it; [`Error::Adapter`] when the digester's reply does not
   /// hold one digest per file; and whatever either port returns.
   pub(crate) fn digests<'g, D: Digester, S: StatCache>(
   ```

   With:

   ```rust
   /// # Errors
   ///
   /// Returns, for the first input in path order that is not a regular file, naming the
   /// lowest-labelled target declaring it, [`Error::MissingInput`] when nothing is there and
   /// [`Error::InputNotAFile`] when something else is; [`Error::Adapter`] when the digester's reply does not
   /// hold one digest per file; and whatever either port returns.
   pub(crate) fn digests<'g, D: Digester, S: StatCache>(
   ```

   Replace:

   ```rust
       let mut read = Vec::with_capacity(inputs.len());
       for (path, node) in inputs {
           let Some(stat) = digester.stat(path)? else {
               return Err(Error::MissingInput {
                   site: Box::new(DeclarationSite::from(*node)),
                   path: (*path).clone(),
               });
           };
           read.push(Read {
   ```

   With:

   ```rust
       let mut read = Vec::with_capacity(inputs.len());
       for (path, node) in inputs {
           let stat = match digester.stat(path)? {
               InputStat::File(stat) => stat,
               InputStat::Missing => {
                   return Err(Error::MissingInput {
                       site: Box::new(DeclarationSite::from(*node)),
                       path: (*path).clone(),
                   });
               }
               InputStat::Other(found) => {
                   return Err(Error::InputNotAFile {
                       site: Box::new(DeclarationSite::from(*node)),
                       path: (*path).clone(),
                       found,
                   });
               }
           };
           read.push(Read {
   ```

6. Name the new refusal in the order Plan's errors are documented in. In `crates/buildl-core/src/plan/assembly.rs`:

   Replace:

   ```rust
   ///    nothing is asked of any other port before this check has passed;
   /// 4. [`Error::MissingTool`](crate::Error::MissingTool) for a program that cannot be found;
   /// 5. [`Error::MissingInput`](crate::Error::MissingInput) for an input that does not exist;
   /// 6. whatever the host port returns for an environment variable.
   ///
   ```

   With:

   ```rust
   ///    nothing is asked of any other port before this check has passed;
   /// 4. [`Error::MissingTool`](crate::Error::MissingTool) for a program that cannot be found;
   /// 5. [`Error::MissingInput`](crate::Error::MissingInput) for an input that does not exist, and
   ///    [`Error::InputNotAFile`](crate::Error::InputNotAFile) for one that is not a regular file;
   /// 6. whatever the host port returns for an environment variable.
   ///
   ```

7. Change the doc example of the bundle. In `crates/buildl-core/src/ports/bundle.rs`:

   Replace:

   ```rust
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, FileStat, Found, Host, Label, Manifest, OutcomeEvent,
   ///     OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result,
   ///     SourcePath, StatCache, StatEntry, Timestamp, ToolResolver, Workspace,
   /// };
   ///
   ```

   With:

   ```rust
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, Manifest, OutcomeEvent,
   ///     OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath,
   ///     StatCache, StatEntry, Timestamp, ToolResolver, Workspace,
   /// };
   ///
   ```

   Replace:

   ```rust
   ///
   /// impl Digester for Nothing {
   ///     fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
   ///         Ok(None)
   ///     }
   ///
   ```

   With:

   ```rust
   ///
   /// impl Digester for Nothing {
   ///     fn stat(&self, _path: &SourcePath) -> Result<InputStat> {
   ///         Ok(InputStat::Missing)
   ///     }
   ///
   ```

8. Change the stub of the set's own test. In the `tests` module of `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
       use crate::types::{
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, FileStat, Found, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
           ReadyAction, SourcePath, StatEntry, Timestamp, Workspace,
       };
   ```

   With:

   ```rust
       use crate::types::{
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
           ReadyAction, SourcePath, StatEntry, Timestamp, Workspace,
       };
   ```

   Replace:

   ```rust
       }

       impl Digester for Stub {
           fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
               Ok(None)
           }

           fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ```

   With:

   ```rust
       }

       impl Digester for Stub {
           fn stat(&self, _path: &SourcePath) -> Result<InputStat> {
               Ok(InputStat::Missing)
           }

           fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ```

9. Change the fake. A path named in `others` is reported as that, whatever `files` holds for it. In `crates/buildl-core/tests/flows/common.rs`, add `InputKind` and `InputStat` to the first `use buildl_core::{…}` list, run `cargo fmt --all`, and:

   Replace:

   ```rust
       DeclarationSource, Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue,
       Error, Evaluated, EvaluationFailure, EventLog, ExecStrategy, Exit, FileStat, Freshness, Host,
       Label, Manifest, NetworkAccess, OutcomeEvent, OutputPath, Platform, PortName, PortSet, Ports,
       Program, RawOutcome, ReadyAction, Result, SourcePath, StagedAlias, StagedDeclaration,
       StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatCache,
       StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};
   ```

   With:

   ```rust
       DeclarationSource, Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue,
       Error, Evaluated, EvaluationFailure, EventLog, ExecStrategy, Exit, FileStat, Freshness, Host,
       InputKind, InputStat, Label, Manifest, NetworkAccess, OutcomeEvent, OutputPath, Platform,
       PortName, PortSet, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};
   ```

   Replace:

   ```rust
       /// The files that exist.
       pub files: BTreeMap<SourcePath, FakeContent>,
       stat_calls: Cell<u32>,
       digest_calls: Cell<u32>,
   ```

   With:

   ```rust
       /// The files that exist.
       pub files: BTreeMap<SourcePath, FakeContent>,
       /// The paths that hold something other than a regular file. One named here is reported as
       /// that, whatever `files` holds for it.
       pub others: BTreeMap<SourcePath, InputKind>,
       stat_calls: Cell<u32>,
       digest_calls: Cell<u32>,
   ```

   Replace:

   ```rust
   }

   impl Digester for FakeDigester {
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>> {
           self.stat_calls.set(self.stat_calls.get() + 1);
           Ok(self.files.get(path).map(|file| file.stat))
       }

       fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ```

   With:

   ```rust
   }

   impl Digester for FakeDigester {
       fn stat(&self, path: &SourcePath) -> Result<InputStat> {
           self.stat_calls.set(self.stat_calls.get() + 1);
           if let Some(kind) = self.others.get(path) {
               return Ok(InputStat::Other(*kind));
           }
           Ok(self
               .files
               .get(path)
               .map_or(InputStat::Missing, |file| InputStat::File(file.stat)))
       }

       fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ```

10. Run the two new tests, then every flow, and confirm they pass:

    ```
    $ cargo test -p buildl-core --test flows not_a
    test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 107 filtered out
    $ cargo test -p buildl-core --test flows
    test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
    ```

    The filter `not_a` also matches a flow that was there before.

11. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `110 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

12. Commit:

   ```
   $ git add crates/buildl-core/src/plan/assembly.rs crates/buildl-core/src/plan/inputs.rs crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/digester.rs crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs crates/buildl-core/tests/flows/plan.rs
   $ git commit -m "feat(buildl-core): have the digester say what stands at a path"
   ```

### Task 2 — Let a running task tell the scheduling thread something

**Files:**
- Modify `crates/buildl-core/src/execute/assembly.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/dispatcher.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing tests. The two tests that drive the fake dispatcher take the new shape: `work` is handed something to tell with, `step` takes a turn, and what a task said must have reached `step` before its result. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       use buildl_core::{
           Digester, FileMode, FileStat, Found, FoundEntry, FoundKind, InputKind, InputStat,
           StoredFile, TreePath,
       };

       fn label(raw: &str) -> Label {
   ```

   With:

   ```rust
       use buildl_core::{
           Digester, FileMode, FileStat, Found, FoundEntry, FoundKind, InputKind, InputStat,
           StoredFile, TreePath, Turn,
       };

       fn label(raw: &str) -> Label {
   ```

   Replace:

   ```rust
           let worked = AtomicU32::new(0);
           let mut results = Vec::new();
           let mut dispatcher = FakeDispatcher {
               order,
   ```

   With:

   ```rust
           let worked = AtomicU32::new(0);
           let mut results = Vec::new();
           let mut said = Vec::new();
           let mut dispatcher = FakeDispatcher {
               order,
   ```

   Replace:

   ```rust
           dispatcher
               .dispatch(
                   |task: u32| {
                       worked.fetch_add(1, Ordering::SeqCst);
                       task * 10
                   },
                   |result| {
                       result.map_or_else(
                           || vec![1, 2, 3],
                           |result| {
                               results.push(result);
                               if result == 10 { vec![4] } else { Vec::new() }
                           },
                       )
                   },
               )
   ```

   With:

   ```rust
           dispatcher
               .dispatch(
                   |task: u32, tell: &(dyn Fn(u32) + Sync)| {
                       worked.fetch_add(1, Ordering::SeqCst);
                       tell(task);
                       task * 10
                   },
                   |turn| match turn {
                       Turn::Begin => vec![1, 2, 3],
                       Turn::Told(task) => {
                           said.push(task);
                           Vec::new()
                       }
                       Turn::Done(result) => {
                           results.push(result);
                           assert_eq!(
                               said.last().map(|task| task * 10),
                               Some(result),
                               "what a task said comes before its result"
                           );
                           if result == 10 { vec![4] } else { Vec::new() }
                       }
                   },
               )
   ```

   Replace:

   ```rust
               dispatcher
                   .dispatch(
                       |task: u32| {
                           ran.fetch_add(1, Ordering::SeqCst);
                           task * 10
                       },
                       |result| {
                           result.map_or_else(
                               || vec![1, 2, 3],
                               |result| {
                                   results.push(result);
                                   Vec::new()
                               },
                           )
                       },
                   )
   ```

   With:

   ```rust
               dispatcher
                   .dispatch(
                       |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                           ran.fetch_add(1, Ordering::SeqCst);
                           task * 10
                       },
                       |turn| match turn {
                           Turn::Begin => vec![1, 2, 3],
                           Turn::Told(()) => Vec::new(),
                           Turn::Done(result) => {
                               results.push(result);
                               Vec::new()
                           }
                       },
                   )
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::tasks
   error[E0593]: closure is expected to take 1 argument, but it takes 2 arguments
   error: could not compile `buildl-core` (test "flows") due to 2 previous errors
   ```

3. Change the port. `crates/buildl-core/src/ports/dispatcher.rs`:

   Replace:

   ```rust
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;

   /// Runs tasks, possibly several at once, and hands each result back one at a time.
   pub trait Dispatcher {
       /// Runs every task `step` yields through `work`, until none is left.
       ///
       /// `step(None)` is called once, first, and each task it returns is run through `work`. Each
       /// result is handed to `step(Some(result))`, one at a time, and the tasks that returns are run
       /// in turn. The call returns when no task is running and none is waiting.
       ///
       /// `step` is always called on the calling thread. `work` may be called on any thread, several
       /// calls at once. Every task `step` returns is run, and every result reaches `step`. Neither
       /// can happen twice, and no result can be made up: a task and a result are each handed over
       /// by value, and the port asks for no way to copy or to build one. A dispatcher that loses a
       /// task or its result breaks the contract: a build over it fails with
       /// [`Error::DispatcherFault`](crate::Error::DispatcherFault) instead of reporting success.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Dispatcher`](crate::PortName::Dispatcher), for the adapter's own failure.
       /// Neither `work` nor `step` can fail here: a task that fails says so in its result.
       fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
       where
           T: Send,
           R: Send,
           W: Fn(T) -> R + Sync,
           S: FnMut(Option<R>) -> Vec<T>;
   }
   ```

   With:

   ```rust
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::Turn;

   /// Runs tasks, possibly several at once, and hands each result back one at a time.
   pub trait Dispatcher {
       /// Runs every task `step` yields through `work`, until none is left.
       ///
       /// `step(Turn::Begin)` is called once, first, and each task it returns is run through
       /// `work`. Each result is handed to `step(Turn::Done(result))`, one at a time, and the
       /// tasks that returns are run in turn. The call returns when no task is running and none is
       /// waiting.
       ///
       /// `work` is handed, with its task, a way to tell `step` something while the task is still
       /// running. Each message it is given reaches `step` as `Turn::Told(message)`: the messages
       /// of one task in the order that task sent them, and all of them before that task's result.
       /// Messages of different tasks reach `step` in any order. The tasks `step` returns for a
       /// message are run like any other.
       ///
       /// `step` is always called on the calling thread. `work` may be called on any thread, several
       /// calls at once, and what it is handed to tell with may be called from any thread until
       /// `work` returns. Every task `step` returns is run, and every message and every result
       /// reaches `step`. None can happen twice, and none can be made up: a task, a message and a
       /// result are each handed over by value, and the port asks for no way to copy or to build
       /// one. A dispatcher that loses a task or its result breaks the contract: a build over it
       /// fails with [`Error::DispatcherFault`](crate::Error::DispatcherFault) instead of reporting
       /// success.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Dispatcher`](crate::PortName::Dispatcher), for the adapter's own failure.
       /// Neither `work` nor `step` can fail here: a task that fails says so in its result.
       fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>
       where
           T: Send,
           M: Send,
           R: Send,
           W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
           S: FnMut(Turn<M, R>) -> Vec<T>;
   }
   ```

4. Change its one caller, and no more than the call needs: a turn instead of an optional result, and a worker that says nothing, so the message type is `()`. In `crates/buildl-core/src/execute/assembly.rs`:

   Replace:

   ```rust
   use crate::types::{
       ActionOutcome, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label, Node, NodeId,
       OnFailure, OutputPath, OwnComponents, Plan, PlannedKey, ReadyAction, TargetGraph, Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   With:

   ```rust
   use crate::types::{
       ActionOutcome, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Label, Node, NodeId,
       OnFailure, OutputPath, OwnComponents, Plan, PlannedKey, ReadyAction, TargetGraph, Turn,
       Verdict,
   };

   /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
   ```

   Replace:

   ```rust
       };
       let dispatched = dispatcher.dispatch(
           |task| perform(task, strategy, cas, clock, &stop),
           |worked| build.step(worked),
       );
       build.outcomes(dispatched)
   ```

   With:

   ```rust
       };
       let dispatched = dispatcher.dispatch(
           |task, _tell| perform(task, strategy, cas, clock, &stop),
           |turn| build.step(turn),
       );
       build.outcomes(dispatched)
   ```

   Replace:

   ```rust
   }

   impl<P: Ports> Build<'_, P> {
       /// Takes one turn: `None` to begin the build, then the result of each task. Returns the
       /// tasks that became ready.
       ///
       /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
       /// result is dropped, and no task is returned.
       fn step(&mut self, worked: Option<Worked>) -> Vec<Task> {
           if self.fatal.is_some() {
               return Vec::new();
           }
           let mut tasks = Vec::new();
           let stepped = match worked {
               None => {
                   let started = self.schedule.start();
                   self.queue.extend(started);
                   self.drain(&mut tasks)
               }
               Some(Worked::NotBegun(node)) => self.abandon(node),
               Some(Worked::Performed(performed)) => self
                   .record(*performed)
                   .and_then(|()| self.drain(&mut tasks)),
   ```

   With:

   ```rust
   }

   impl<P: Ports> Build<'_, P> {
       /// Takes one turn: the beginning of the build, then the result of each task. Returns the
       /// tasks that became ready. A worker says nothing while it runs a task.
       ///
       /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
       /// result is dropped, and no task is returned.
       fn step(&mut self, turn: Turn<(), Worked>) -> Vec<Task> {
           if self.fatal.is_some() {
               return Vec::new();
           }
           let mut tasks = Vec::new();
           let stepped = match turn {
               Turn::Begin => {
                   let started = self.schedule.start();
                   self.queue.extend(started);
                   self.drain(&mut tasks)
               }
               Turn::Told(()) => Ok(()),
               Turn::Done(Worked::NotBegun(node)) => self.abandon(node),
               Turn::Done(Worked::Performed(performed)) => self
                   .record(*performed)
                   .and_then(|()| self.drain(&mut tasks)),
   ```

5. Change the doc example of the bundle: its dispatcher keeps what a task said and hands it over once the task is done. In `crates/buildl-core/src/ports/bundle.rs`:

   Replace:

   ````rust
   /// ```
   /// use std::collections::BTreeSet;
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, Manifest, OutcomeEvent,
   ///     OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath,
   ///     StatCache, StatEntry, Timestamp, ToolResolver, Workspace,
   /// };
   ///
   ````

   With:

   ````rust
   /// ```
   /// use std::collections::BTreeSet;
   /// use std::sync::{Mutex, PoisonError};
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, Manifest, OutcomeEvent,
   ///     OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath,
   ///     StatCache, StatEntry, Timestamp, ToolResolver, Turn, Workspace,
   /// };
   ///
   ````

   Replace:

   ```rust
   /// }
   ///
   /// /// Runs every task on the calling thread, one at a time.
   /// impl Dispatcher for Nothing {
   ///     fn dispatch<T, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
   ///     where
   ///         T: Send,
   ///         R: Send,
   ///         W: Fn(T) -> R + Sync,
   ///         S: FnMut(Option<R>) -> Vec<T>,
   ///     {
   ///         let mut waiting = step(None);
   ///         while let Some(task) = waiting.pop() {
   ///             waiting.extend(step(Some(work(task))));
   ///         }
   ///         Ok(())
   ```

   With:

   ```rust
   /// }
   ///
   /// /// Runs every task on the calling thread, one at a time, and hands over what a task said
   /// /// once the task is done.
   /// impl Dispatcher for Nothing {
   ///     fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
   ///     where
   ///         T: Send,
   ///         M: Send,
   ///         R: Send,
   ///         W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
   ///         S: FnMut(Turn<M, R>) -> Vec<T>,
   ///     {
   ///         let mut waiting = step(Turn::Begin);
   ///         while let Some(task) = waiting.pop() {
   ///             let said = Mutex::new(Vec::new());
   ///             let result = work(task, &|message| {
   ///                 said.lock().unwrap_or_else(PoisonError::into_inner).push(message);
   ///             });
   ///             for message in said.into_inner().unwrap_or_else(PoisonError::into_inner) {
   ///                 waiting.extend(step(Turn::Told(message)));
   ///             }
   ///             waiting.extend(step(Turn::Done(result)));
   ///         }
   ///         Ok(())
   ```

6. Change the stub of the set's own test. In the `tests` module of `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
           ReadyAction, SourcePath, StatEntry, Timestamp, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   With:

   ```rust
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
           ReadyAction, SourcePath, StatEntry, Timestamp, Turn, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   Replace:

   ```rust
       }

       impl Dispatcher for Stub {
           fn dispatch<T, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
           where
               T: Send,
               R: Send,
               W: Fn(T) -> R + Sync,
               S: FnMut(Option<R>) -> Vec<T>,
           {
               Ok(())
   ```

   With:

   ```rust
       }

       impl Dispatcher for Stub {
           fn dispatch<T, M, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
           where
               T: Send,
               M: Send,
               R: Send,
               W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
               S: FnMut(Turn<M, R>) -> Vec<T>,
           {
               Ok(())
   ```

7. Change the fake. It keeps what a task said in a `Mutex`, because what `work` is handed must be `Sync`, and hands it over before the result. A lost task says nothing either. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
   ```

   With:

   ```rust
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::Turn;
   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
   ```

   Replace:

   ```rust
   }

   /// A dispatcher that runs every task on the calling thread, one at a time.
   ///
   /// It breaks the port's contract only where told to: `lose` names tasks by the position at which
   ```

   With:

   ```rust
   }

   /// A dispatcher that runs every task on the calling thread, one at a time: what a task said
   /// while it ran is handed over once it is done, right before its result.
   ///
   /// It breaks the port's contract only where told to: `lose` names tasks by the position at which
   ```

   Replace:

   ```rust
   }

   impl Dispatcher for FakeDispatcher {
       fn dispatch<T, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
       where
           T: Send,
           R: Send,
           W: Fn(T) -> R + Sync,
           S: FnMut(Option<R>) -> Vec<T>,
       {
           let mut waiting: VecDeque<T> = step(None).into();
           let mut taken = 0_usize;
           loop {
   ```

   With:

   ```rust
   }

   impl Dispatcher for FakeDispatcher {
       fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
       where
           T: Send,
           M: Send,
           R: Send,
           W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
           S: FnMut(Turn<M, R>) -> Vec<T>,
       {
           let mut waiting: VecDeque<T> = step(Turn::Begin).into();
           let mut taken = 0_usize;
           loop {
   ```

   Replace:

   ```rust
               let loss = self.lose.get(&taken).copied();
               taken += 1;
               match loss {
                   Some(Loss::Unrun) => {}
                   Some(Loss::Unreported) => drop(work(task)),
                   None => waiting.extend(step(Some(work(task)))),
               }
           }
       }
   ```

   With:

   ```rust
               let loss = self.lose.get(&taken).copied();
               taken += 1;
               if loss == Some(Loss::Unrun) {
                   continue;
               }
               let said = Mutex::new(Vec::new());
               let result = work(task, &|message| {
                   said.lock()
                       .unwrap_or_else(PoisonError::into_inner)
                       .push(message);
               });
               if loss == Some(Loss::Unreported) {
                   continue;
               }
               for message in said.into_inner().unwrap_or_else(PoisonError::into_inner) {
                   waiting.extend(step(Turn::Told(message)));
               }
               waiting.extend(step(Turn::Done(result)));
           }
       }
   ```

8. Run the fakes' tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::tests
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `110 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

10. Commit:

   ```
   $ git add crates/buildl-core/src/execute/assembly.rs crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/dispatcher.rs crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): let a running task tell the scheduling thread something"
   ```

### Task 3 — Let the fake dispatcher have several tasks under way

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test. With every waiting task taken together, what each said arrives in the order they were taken, and their results the last taken first: what a build sees when the first action to begin is the last to end. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeLog, FakeStrategy, Loss,
           Order, Script, dir,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   With:

   ```rust
       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeLog, FakeStrategy, Loss,
           Order, Pace, Script, dir,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   Replace:

   ```rust
           assert_eq!(dispatched(Order::NewestFirst), [30, 20, 10, 40]);
       }
   }
   ```

   With:

   ```rust
           assert_eq!(dispatched(Order::NewestFirst), [30, 20, 10, 40]);
       }

       #[test]
       fn tasks_run_together_say_everything_first_and_end_last_taken_first() {
           let mut turns = Vec::new();
           let mut dispatcher = FakeDispatcher {
               pace: Pace::Together,
               ..FakeDispatcher::default()
           };

           dispatcher
               .dispatch(
                   |task: u32, tell: &(dyn Fn(String) + Sync)| {
                       tell(format!("{task} began"));
                       tell(format!("{task} printed"));
                       task * 10
                   },
                   |turn| match turn {
                       Turn::Begin => vec![1, 2, 3],
                       Turn::Told(said) => {
                           turns.push(said);
                           Vec::new()
                       }
                       Turn::Done(result) => {
                           turns.push(format!("{result} done"));
                           if result == 10 { vec![4] } else { Vec::new() }
                       }
                   },
               )
               .unwrap();

           assert_eq!(FakeDispatcher::default().pace, Pace::OneAtATime);
           assert_eq!(
               turns,
               [
                   "1 began",
                   "1 printed",
                   "2 began",
                   "2 printed",
                   "3 began",
                   "3 printed",
                   "30 done",
                   "20 done",
                   "10 done",
                   "4 began",
                   "4 printed",
                   "40 done",
               ]
           );
       }
   }
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::tasks
   error[E0432]: unresolved import `super::Pace`
   error[E0560]: struct `FakeDispatcher` has no field named `pace`
   error: could not compile `buildl-core` (test "flows") due to 3 previous errors
   ```

3. Give the fake its pace. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
   }

   /// A dispatcher that runs every task on the calling thread, one at a time: what a task said
   /// while it ran is handed over once it is done, right before its result.
   ///
   /// It breaks the port's contract only where told to: `lose` names tasks by the position at which
   ```

   With:

   ```rust
   }

   /// How many tasks a [`FakeDispatcher`] has under way before it hands anything back.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum Pace {
       /// One: what a task said and its result are handed over before the next task is taken.
       #[default]
       OneAtATime,
       /// Every waiting task: all are run, then what each said is handed over in the order they
       /// were taken, then their results, the task taken last first. It is what a build sees when
       /// several actions run at once and the first to begin is the last to end.
       Together,
   }

   /// A dispatcher that runs every task on the calling thread.
   ///
   /// It breaks the port's contract only where told to: `lose` names tasks by the position at which
   ```

   Replace:

   ```rust
       /// Which waiting task is taken next.
       pub order: Order,
       /// The tasks to lose, by the position at which they are taken, and how.
       pub lose: BTreeMap<usize, Loss>,
   ```

   With:

   ```rust
       /// Which waiting task is taken next.
       pub order: Order,
       /// How many tasks are under way at once.
       pub pace: Pace,
       /// The tasks to lose, by the position at which they are taken, and how.
       pub lose: BTreeMap<usize, Loss>,
   ```

   Replace:

   ```rust
           let mut taken = 0_usize;
           loop {
               let next = match self.order {
                   Order::OldestFirst => waiting.pop_front(),
                   Order::NewestFirst => waiting.pop_back(),
               };
               let Some(task) = next else {
                   return Ok(());
               };
               let loss = self.lose.get(&taken).copied();
               taken += 1;
               if loss == Some(Loss::Unrun) {
                   continue;
               }
               let said = Mutex::new(Vec::new());
               let result = work(task, &|message| {
                   said.lock()
                       .unwrap_or_else(PoisonError::into_inner)
                       .push(message);
               });
               if loss == Some(Loss::Unreported) {
                   continue;
               }
               for message in said.into_inner().unwrap_or_else(PoisonError::into_inner) {
                   waiting.extend(step(Turn::Told(message)));
               }
               waiting.extend(step(Turn::Done(result)));
           }
       }
   ```

   With:

   ```rust
           let mut taken = 0_usize;
           loop {
               let mut ran = Vec::new();
               loop {
                   let next = match self.order {
                       Order::OldestFirst => waiting.pop_front(),
                       Order::NewestFirst => waiting.pop_back(),
                   };
                   let Some(task) = next else {
                       break;
                   };
                   let loss = self.lose.get(&taken).copied();
                   taken += 1;
                   if loss == Some(Loss::Unrun) {
                       continue;
                   }
                   let said = Mutex::new(Vec::new());
                   let result = work(task, &|message| {
                       said.lock()
                           .unwrap_or_else(PoisonError::into_inner)
                           .push(message);
                   });
                   if loss == Some(Loss::Unreported) {
                       continue;
                   }
                   ran.push((
                       said.into_inner().unwrap_or_else(PoisonError::into_inner),
                       result,
                   ));
                   if self.pace == Pace::OneAtATime {
                       break;
                   }
               }
               if ran.is_empty() {
                   return Ok(());
               }
               let mut results = Vec::new();
               for (said, result) in ran {
                   for message in said {
                       waiting.extend(step(Turn::Told(message)));
                   }
                   results.push(result);
               }
               if self.pace == Pace::Together {
                   results.reverse();
               }
               for result in results {
                   waiting.extend(step(Turn::Done(result)));
               }
           }
       }
   ```

4. Run the fakes' tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::tests
   test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `111 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs
   $ git commit -m "test(buildl-core): let the fake dispatcher have several tasks under way"
   ```

### Task 4 — Hand a strategy somewhere to send what a command writes while it runs

**Files:**
- Modify `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/exec_strategy.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing tests. Every call of `run` in the fakes' tests gains its third argument, and a new test holds the fake to the contract: for each output, what was handed over live, joined, is exactly what the run returns, and the returned value is the same with or without a sink. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       use std::collections::{BTreeMap, BTreeSet};
       use std::path::PathBuf;
       use std::sync::atomic::{AtomicU32, Ordering};

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Clock, Command, ContentStore,
           Digest, Directory, Dispatcher, Error, EventLog, ExecStrategy, Exit, KeyComponents, Label,
           NetworkAccess, OutcomeClass, OutcomeEvent, OutputName, OutputPath, OwnComponents, Platform,
           PortName, Provenance, ReadyAction, SourcePath, Timestamp,
       };

       use super::{
   ```

   With:

   ```rust
       use std::collections::{BTreeMap, BTreeSet};
       use std::path::PathBuf;
       use std::sync::Mutex;
       use std::sync::atomic::{AtomicU32, Ordering};

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Channel, Clock, Command,
           ContentStore, Digest, Directory, Dispatcher, Error, EventLog, ExecStrategy, Exit,
           KeyComponents, Label, NetworkAccess, OutcomeClass, OutcomeEvent, OutputName, OutputPath,
           OwnComponents, Platform, PortName, Provenance, ReadyAction, SourcePath, Timestamp,
       };

       use super::{
   ```

   Replace:

   ```rust
           let app = action("//:app", &[], &["app", "app.map"]);

           let dir = strategy.prepare(&app).unwrap();
           let ran = strategy.run(&dir, &app).unwrap();
           let collected = strategy.collect(&dir, &app, &cas).unwrap();

           assert_eq!(ran.exit, Exit::Exited(0));
   ```

   With:

   ```rust
           let app = action("//:app", &[], &["app", "app.map"]);

           let dir = strategy.prepare(&app).unwrap();
           let ran = strategy.run(&dir, &app, None).unwrap();
           let collected = strategy.collect(&dir, &app, &cas).unwrap();

           assert_eq!(ran.exit, Exit::Exited(0));
   ```

   Replace:

   ```rust
           });

           let failed = failing.run(&failing.prepare(&app).unwrap(), &app).unwrap();
           let timed_out = slow.run(&slow.prepare(&app).unwrap(), &app).unwrap();

           assert_eq!(failed.exit, Exit::Exited(3));
           assert_eq!(failed.stdout, b"compiling");
           assert_eq!(failed.stderr, b"no such file");
           assert_eq!(timed_out.exit, Exit::TimedOut);
           assert_eq!(timed_out.stdout, b"//:app\n");
       }

       #[test]
   ```

   With:

   ```rust
           });

           let failed = failing
               .run(&failing.prepare(&app).unwrap(), &app, None)
               .unwrap();
           let timed_out = slow.run(&slow.prepare(&app).unwrap(), &app, None).unwrap();

           assert_eq!(failed.exit, Exit::Exited(3));
           assert_eq!(failed.stdout, b"compiling");
           assert_eq!(failed.stderr, b"no such file");
           assert_eq!(timed_out.exit, Exit::TimedOut);
           assert_eq!(timed_out.stdout, b"//:app\n");
       }

       #[test]
       fn what_a_run_hands_over_live_joins_to_exactly_what_it_returns() {
           let app = action("//:app", &[], &["app"]);
           let strategy = scripted(Script {
               stdout: Some(b"one\ntwo\n".to_vec()),
               stderr: Some(b"warn\n".to_vec()),
               ..Script::default()
           });
           let handed = Mutex::new(Vec::new());
           let live = |channel: Channel, bytes: &[u8]| {
               handed.lock().unwrap().push((channel, bytes.to_vec()));
           };

           let dir = strategy.prepare(&app).unwrap();
           let shown = strategy.run(&dir, &app, Some(&live)).unwrap();
           let quiet = strategy.run(&dir, &app, None).unwrap();

           let handed = handed.into_inner().unwrap();
           assert_eq!(
               handed,
               [
                   (Channel::Stdout, b"o".to_vec()),
                   (Channel::Stderr, b"warn\n".to_vec()),
                   (Channel::Stdout, b"ne\ntwo\n".to_vec()),
               ],
               "a run of bytes at a time, each with the output it was written to"
           );
           let joined = |wanted: Channel| -> Vec<u8> {
               handed
                   .iter()
                   .filter(|(channel, _)| *channel == wanted)
                   .flat_map(|(_, bytes)| bytes.clone())
                   .collect()
           };
           assert_eq!(joined(Channel::Stdout), shown.stdout);
           assert_eq!(joined(Channel::Stderr), shown.stderr);
           assert_eq!(
               shown, quiet,
               "what is returned is the same with or without it"
           );
       }

       #[test]
   ```

   Replace:

   ```rust
           let dir = no_start.prepare(&app).unwrap();
           assert_eq!(
               no_start.run(&dir, &app).unwrap_err().to_string(),
               "exec strategy: //:app: the command cannot be started"
           );
           assert_eq!(no_start.runs(&label("//:app")), 1);

           let unreadable = with(Ends::CollectFails);
           let dir = unreadable.prepare(&app).unwrap();
           assert_eq!(unreadable.run(&dir, &app).unwrap().exit, Exit::Exited(0));
           assert_eq!(
               unreadable
   ```

   With:

   ```rust
           let dir = no_start.prepare(&app).unwrap();
           assert_eq!(
               no_start.run(&dir, &app, None).unwrap_err().to_string(),
               "exec strategy: //:app: the command cannot be started"
           );
           assert_eq!(no_start.runs(&label("//:app")), 1);

           let unreadable = with(Ends::CollectFails);
           let dir = unreadable.prepare(&app).unwrap();
           assert_eq!(
               unreadable.run(&dir, &app, None).unwrap().exit,
               Exit::Exited(0)
           );
           assert_eq!(
               unreadable
   ```

   Replace:

   ```rust
           assert_eq!(cas.digests(), BTreeSet::from([Digest::of(b"binary")]));

           let dir = short.prepare(&app).unwrap();
           assert_eq!(short.run(&dir, &app).unwrap().exit, Exit::Exited(0));
           assert_eq!(
               short.collect(&dir, &app, &cas).unwrap(),
   ```

   With:

   ```rust
           assert_eq!(cas.digests(), BTreeSet::from([Digest::of(b"binary")]));

           let dir = short.prepare(&app).unwrap();
           assert_eq!(short.run(&dir, &app, None).unwrap().exit, Exit::Exited(0));
           assert_eq!(
               short.collect(&dir, &app, &cas).unwrap(),
   ```

2. Run the new test and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::what_a_run
   error[E0061]: this method takes 2 arguments but 3 arguments were supplied
   error: could not compile `buildl-core` (test "flows") due to 8 previous errors
   ```

3. Change the port. `crates/buildl-core/src/ports/exec_strategy.rs`:

   Replace:

   ```rust
   use crate::error::Result;
   use crate::ports::ContentStore;
   use crate::types::{Found, RawOutcome, ReadyAction};

   /// Runs one action in a directory of its own, says what the action left at its output paths,
   ```

   With:

   ```rust
   use crate::error::Result;
   use crate::ports::ContentStore;
   use crate::types::{Found, LiveSink, RawOutcome, ReadyAction};

   /// Runs one action in a directory of its own, says what the action left at its output paths,
   ```

   Replace:

   ```rust
       /// [`collect`](ExecStrategy::collect) looks.
       ///
       /// A command that ran and failed is not an error: it is an outcome with a non-zero status.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the command cannot be
       /// started.
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;

       /// Says what the action left at each path of its `outputs`, one entry per path, in order,
   ```

   With:

   ```rust
       /// [`collect`](ExecStrategy::collect) looks.
       ///
       /// A command that ran and failed is not an error: it is an outcome with a non-zero status,
       /// or with the signal that ended it.
       ///
       /// With `live`, every run of bytes the command writes is also handed to it as it is read,
       /// with the output it was written to, before the command has ended. `live` may be called
       /// from any thread, until this call returns. What is returned is the same with or without
       /// it: for each of the two outputs, the bytes handed to `live`, joined in the order they
       /// were handed over, are exactly the bytes of that output in the result.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the command cannot be
       /// started.
       fn run(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome>;

       /// Says what the action left at each path of its `outputs`, one entry per path, in order,
   ```

4. Change its one caller: a worker runs every command with no live sink. In `crates/buildl-core/src/execute/perform.rs`:

   Replace:

   ```rust
           Err(error) => return failed_to(InfraKind::Prepare, &error, None),
       };
       let raw = match strategy.run(&dir, action) {
           Ok(raw) => raw,
           Err(error) => return failed_to(InfraKind::Run, &error, None),
   ```

   With:

   ```rust
           Err(error) => return failed_to(InfraKind::Prepare, &error, None),
       };
       let raw = match strategy.run(&dir, action, None) {
           Ok(raw) => raw,
           Err(error) => return failed_to(InfraKind::Run, &error, None),
   ```

5. Change the doc example of the bundle. In `crates/buildl-core/src/ports/bundle.rs`:

   Replace:

   ```rust
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, Manifest, OutcomeEvent,
   ///     OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath,
   ///     StatCache, StatEntry, Timestamp, ToolResolver, Turn, Workspace,
   /// };
   ///
   ```

   With:

   ```rust
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, LiveSink, Manifest,
   ///     OutcomeEvent, OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction,
   ///     Result, SourcePath, StatCache, StatEntry, Timestamp, ToolResolver, Turn, Workspace,
   /// };
   ///
   ```

   Replace:

   ```rust
   ///     }
   ///
   ///     fn run(&self, _dir: &(), _action: &ReadyAction) -> Result<RawOutcome> {
   ///         Err(nothing(PortName::ExecStrategy))
   ///     }
   ```

   With:

   ```rust
   ///     }
   ///
   ///     fn run(
   ///         &self,
   ///         _dir: &(),
   ///         _action: &ReadyAction,
   ///         _live: Option<LiveSink<'_>>,
   ///     ) -> Result<RawOutcome> {
   ///         Err(nothing(PortName::ExecStrategy))
   ///     }
   ```

6. Change the stub of the set's own test. In the `tests` module of `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
       use crate::types::{
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
           ReadyAction, SourcePath, StatEntry, Timestamp, Turn, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   With:

   ```rust
       use crate::types::{
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, LiveSink, OutcomeEvent, OutputPath, Platform, Program,
           RawOutcome, ReadyAction, SourcePath, StatEntry, Timestamp, Turn, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   Replace:

   ```rust
           }

           fn run(&self, _dir: &(), _action: &ReadyAction) -> Result<RawOutcome> {
               Ok(RawOutcome {
                   exit: Exit::Exited(0),
   ```

   With:

   ```rust
           }

           fn run(
               &self,
               _dir: &(),
               _action: &ReadyAction,
               _live: Option<LiveSink<'_>>,
           ) -> Result<RawOutcome> {
               Ok(RawOutcome {
                   exit: Exit::Exited(0),
   ```

7. Change the fake. It hands the sink the first byte of standard output, then standard error, then the rest of standard output: uneven runs, to show that only the joined bytes are promised. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::Turn;
   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
   ```

   With:

   ```rust
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
   ```

   Replace:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   With:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{Channel, LiveSink, Turn};
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   Replace:

   ```rust
       }

       fn run(&self, _dir: &FakeDir, action: &ReadyAction) -> Result<RawOutcome> {
           self.ran
               .lock()
   ```

   With:

   ```rust
       }

       fn run(
           &self,
           _dir: &FakeDir,
           action: &ReadyAction,
           live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome> {
           self.ran
               .lock()
   ```

   Replace:

   ```rust
               Ends::PrepareFails | Ends::CollectFails | Ends::CollectShort => Exit::Exited(0),
           };
           Ok(RawOutcome {
               exit,
               stdout: script
                   .stdout
                   .unwrap_or_else(|| format!("{}\n", action.label).into_bytes()),
               stderr: script.stderr.unwrap_or_default(),
           })
       }
   ```

   With:

   ```rust
               Ends::PrepareFails | Ends::CollectFails | Ends::CollectShort => Exit::Exited(0),
           };
           let stdout = script
               .stdout
               .unwrap_or_else(|| format!("{}\n", action.label).into_bytes());
           let stderr = script.stderr.unwrap_or_default();
           if let Some(live) = live {
               // Written a byte at a time and a line at a time, to show that only the joined
               // bytes are promised.
               let (first, rest) = stdout.split_at(stdout.len().min(1));
               for (channel, bytes) in [
                   (Channel::Stdout, first),
                   (Channel::Stderr, stderr.as_slice()),
                   (Channel::Stdout, rest),
               ] {
                   if !bytes.is_empty() {
                       live(channel, bytes);
                   }
               }
           }
           Ok(RawOutcome {
               exit,
               stdout,
               stderr,
           })
       }
   ```

8. Run the fakes' tests and the bundle's doc example and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::tests
   test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out
   $ cargo test -p buildl-core --doc ports::bundle
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `112 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

10. Commit:

   ```
   $ git add crates/buildl-core/src/execute/perform.rs crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/exec_strategy.rs crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): hand a strategy somewhere to send what a command writes"
   ```

### Task 5 — Let a scripted command be ended by a signal

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test. The test of scripted endings gains a third script. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
               ..Script::default()
           });

           let failed = failing
               .run(&failing.prepare(&app).unwrap(), &app, None)
               .unwrap();
           let timed_out = slow.run(&slow.prepare(&app).unwrap(), &app, None).unwrap();

           assert_eq!(failed.exit, Exit::Exited(3));
           assert_eq!(failed.stdout, b"compiling");
           assert_eq!(failed.stderr, b"no such file");
           assert_eq!(timed_out.exit, Exit::TimedOut);
           assert_eq!(timed_out.stdout, b"//:app\n");
       }

       #[test]
   ```

   With:

   ```rust
               ..Script::default()
           });
           let killed = scripted(Script {
               ends: Ends::Signaled(11),
               ..Script::default()
           });

           let failed = failing
               .run(&failing.prepare(&app).unwrap(), &app, None)
               .unwrap();
           let timed_out = slow.run(&slow.prepare(&app).unwrap(), &app, None).unwrap();
           let signaled = killed
               .run(&killed.prepare(&app).unwrap(), &app, None)
               .unwrap();

           assert_eq!(failed.exit, Exit::Exited(3));
           assert_eq!(failed.stdout, b"compiling");
           assert_eq!(failed.stderr, b"no such file");
           assert_eq!(timed_out.exit, Exit::TimedOut);
           assert_eq!(timed_out.stdout, b"//:app\n");
           assert_eq!(signaled.exit, Exit::Signaled(11));
           assert_eq!(signaled.stdout, b"//:app\n");
       }

       #[test]
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_scripted_action
   error[E0599]: no variant, associated function, or constant named `Signaled` found for enum `Ends` in the current scope
   error: could not compile `buildl-core` (test "flows") due to 1 previous error
   ```

3. Add the ending. A command a signal ended is a result, never an error. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       /// The command runs and exits with this status.
       Exits(i32),
       /// The command runs longer than it is allowed to.
       TimesOut,
   ```

   With:

   ```rust
       /// The command runs and exits with this status.
       Exits(i32),
       /// The command runs and this signal ends it.
       Signaled(i32),
       /// The command runs longer than it is allowed to.
       TimesOut,
   ```

   Replace:

   ```rust
               Ends::TimesOut => Exit::TimedOut,
               Ends::Exits(status) => Exit::Exited(status),
               Ends::PrepareFails | Ends::CollectFails | Ends::CollectShort => Exit::Exited(0),
           };
   ```

   With:

   ```rust
               Ends::TimesOut => Exit::TimedOut,
               Ends::Exits(status) => Exit::Exited(status),
               Ends::Signaled(signal) => Exit::Signaled(signal),
               Ends::PrepareFails | Ends::CollectFails | Ends::CollectShort => Exit::Exited(0),
           };
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_scripted_action
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `112 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs
   $ git commit -m "test(buildl-core): let a scripted command be ended by a signal"
   ```

### Task 6 — Write down what four port contracts left unsaid

**Files:**
- Modify `crates/buildl-core/src/ports/content_store.rs`
- Modify `crates/buildl-core/src/ports/manifest.rs`
- Modify `crates/buildl-core/src/ports/stat_cache.rs`

These are sentences of contract, with no signature and no behaviour of this crate behind them, so the task has no failing test: each says what an adapter may or must do where the core already relies on it.

**Steps:**

1. Every call of `Manifest::workspace` on one instance returns the same value: Plan and Execute each ask. In `crates/buildl-core/src/ports/manifest.rs`:

   Replace:

   ```rust
       /// out its type's default. Text a type rejects makes the manifest not valid.
       ///
       /// # Errors
       ///
   ```

   With:

   ```rust
       /// out its type's default. Text a type rejects makes the manifest not valid.
       ///
       /// Every call on one instance returns the same value: Plan and Execute each ask, and the
       /// directories a build writes under are the ones its ceiling was checked for.
       ///
       /// # Errors
       ///
   ```

2. A store that cannot tell whether it holds content answers `false`, and content that no longer has its digest is not held. In `crates/buildl-core/src/ports/content_store.rs`:

   Replace:

   ```rust
       fn put(&self, bytes: &[u8]) -> Result<Digest>;

       /// Whether content with that digest is held.
       fn contains(&self, digest: &Digest) -> bool;

       /// The content held under that digest; `None` when it is not held.
       ///
       /// # Errors
   ```

   With:

   ```rust
       fn put(&self, bytes: &[u8]) -> Result<Digest>;

       /// Whether content with that digest is held. An implementation that cannot tell answers
       /// `false`, which makes the target that needs the content run again.
       fn contains(&self, digest: &Digest) -> bool;

       /// The content held under that digest; `None` when it is not held. Content that no longer
       /// has that digest is not held.
       ///
       /// # Errors
   ```

3. A stat cache may decline an entry it cannot vouch for. In `crates/buildl-core/src/ports/stat_cache.rs`:

   Replace:

   ```rust
       fn entry(&self, path: &SourcePath) -> Option<StatEntry>;

       /// Replaces the entry for `path`, in memory.
       fn record(&mut self, path: SourcePath, entry: StatEntry);

       /// Persists every entry.
   ```

   With:

   ```rust
       fn entry(&self, path: &SourcePath) -> Option<StatEntry>;

       /// Replaces the entry for `path`, in memory.
       ///
       /// An implementation may decline to keep an entry it cannot vouch for;
       /// [`entry`](StatCache::entry) then returns `None` for that path.
       fn record(&mut self, path: SourcePath, entry: StatEntry);

       /// Persists every entry.
   ```

4. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `543 passed` (unit tests), `112 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

5. Commit:

   ```
   $ git add crates/buildl-core/src/ports/content_store.rs crates/buildl-core/src/ports/manifest.rs crates/buildl-core/src/ports/stat_cache.rs
   $ git commit -m "docs(buildl-core): write down what four port contracts left unsaid"
   ```

### Task 7 — Add the reporter port

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Create `crates/buildl-core/src/ports/reporter.rs`

**Steps:**

1. Write the trait with its failing test: a reporter that is only borrowed receives every call made through the borrow. The implementation that makes a borrow a reporter is not written yet. Create `crates/buildl-core/src/ports/reporter.rs`:

   ```rust
   //! The port through which a build shows a person what it is doing.
   //!
   //! Its own file because showing means writing to a terminal, and this crate writes nowhere: it
   //! says what began, what was printed and how each target ended, and an adapter decides what a
   //! person sees of it.
   //!
   //! Responsibilities: [`Reporter`].
   //!
   //! Non-responsibilities: implementing it, and deciding what is reported and when. The first
   //! belongs to an adapter crate; the second to the phase that runs actions. Showing an error that
   //! fails a whole command is not here either: an error is returned to the caller.
   //!
   //! A reporter borrowed mutably is a reporter too, so a caller that keeps its reporter can lend
   //! it to one build.

   use crate::types::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Summary};

   /// Is told how a build goes, one call at a time.
   ///
   /// Every call is made on the thread that called the build, never on one that runs an action, so
   /// an implementation needs no lock and is the only writer of what it writes to. No call can
   /// fail: what a reporter cannot show is its own to deal with, and changes nothing a build
   /// stores or returns.
   ///
   /// One build makes its calls in this order: one [`begun`], any number of [`started`],
   /// [`printed`] and [`finished`], then one [`summary`]. A build that is refused before it runs
   /// anything makes no call, and one that ends in an error after that calls [`summary`] never.
   ///
   /// From the first [`printed`] for a target until that target's [`finished`], no [`printed`] and
   /// no [`finished`] is about another target: what one action prints is never interleaved with
   /// anything another printed.
   ///
   /// [`begun`]: Reporter::begun
   /// [`started`]: Reporter::started
   /// [`printed`]: Reporter::printed
   /// [`finished`]: Reporter::finished
   /// [`summary`]: Reporter::summary
   pub trait Reporter {
       /// A build of `targets` targets is about to hand out its first action: everything that can
       /// refuse a build has passed. Called once, first. Each of those targets is reported by one
       /// [`finished`](Reporter::finished).
       fn begun(&mut self, targets: usize);

       /// A worker began `action`. Called for a target that runs, and never for one that is cached
       /// or skipped.
       fn started(&mut self, action: &ReadyAction);

       /// The running action of the target `label` wrote `bytes` to `channel`. Called only under
       /// [`LiveOutput::OneAction`](crate::LiveOutput::OneAction), and only for the one action
       /// shown live at the time.
       fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]);

       /// A target has its outcome: `event` is what the build log holds for it. Called once for
       /// every target of the build, whatever its class.
       ///
       /// `printed` is what its action wrote that [`printed`](Reporter::printed) was not handed:
       /// everything it wrote when it last ran, for a target that is cached as for one that ran
       /// now; and nothing for a target whose output was shown live, or that has none stored.
       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>);

       /// Every target has its outcome, counted in `summary`. Called once, last.
       fn summary(&mut self, summary: Summary);
   }

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::path::PathBuf;

       use super::Reporter;
       use crate::types::{
           ActionOutcome, Argument, Captured, Channel, Command, Directory, Label, NetworkAccess,
           OutcomeClass, OutcomeEvent, Provenance, ReadyAction, Summary, Timestamp,
       };

       /// Keeps the name of each call it receives, in order.
       #[derive(Default)]
       struct Names(Vec<&'static str>);

       impl Reporter for Names {
           fn begun(&mut self, _targets: usize) {
               self.0.push("begun");
           }

           fn started(&mut self, _action: &ReadyAction) {
               self.0.push("started");
           }

           fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {
               self.0.push("printed");
           }

           fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {
               self.0.push("finished");
           }

           fn summary(&mut self, _summary: Summary) {
               self.0.push("summary");
           }
       }

       /// Makes one call of each kind on a reporter it owns.
       fn report_once<R: Reporter>(mut reporter: R) {
           let label = Label::parse("//:app").unwrap();
           let command = Command::new(vec![Argument::parse("cc").unwrap()]).unwrap();
           reporter.begun(1);
           reporter.started(&ReadyAction {
               label: label.clone(),
               command: command.clone(),
               description: None,
               env: BTreeMap::new(),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: Vec::new(),
               network: NetworkAccess::Sealed,
           });
           reporter.printed(&label, Channel::Stdout, b"hi");
           reporter.finished(
               &OutcomeEvent {
                   at: Timestamp::UNIX_EPOCH,
                   provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                   command,
                   key: None,
                   started: None,
                   finished: None,
                   outcome: ActionOutcome {
                       label,
                       class: OutcomeClass::Built,
                       diagnostic: None,
                       outputs: Vec::new(),
                       stdout: None,
                       stderr: None,
                   },
               },
               Captured::default(),
           );
           reporter.summary(Summary {
               failed: 0,
               skipped: 0,
               built: 1,
               cached: 0,
           });
       }

       #[test]
       fn a_borrowed_reporter_receives_every_call_made_through_the_borrow() {
           let mut names = Names::default();
           report_once(&mut names);
           assert_eq!(
               names.0,
               ["begun", "started", "printed", "finished", "summary"]
           );
       }
   }
   ```

2. Declare the module, name the port in the index and re-export it. In `crates/buildl-core/src/ports/mod.rs`:

   Replace:

   ```rust
   //! - [`ExecStrategy`] — running one action in a directory of its own.
   //! - [`Dispatcher`] — having several tasks worked on at once.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   //!   parameter instead of one per port.
   ```

   With:

   ```rust
   //! - [`ExecStrategy`] — running one action in a directory of its own.
   //! - [`Dispatcher`] — having several tasks worked on at once.
   //! - [`Reporter`] — showing a person what a build is doing.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   //!   parameter instead of one per port.
   ```

   Replace:

   ```rust
   pub mod manifest;
   pub mod port_set;
   pub mod stat_cache;
   pub mod tool_resolver;
   ```

   With:

   ```rust
   pub mod manifest;
   pub mod port_set;
   pub mod reporter;
   pub mod stat_cache;
   pub mod tool_resolver;
   ```

   Replace:

   ```rust
   pub use manifest::Manifest;
   pub use port_set::PortSet;
   pub use stat_cache::StatCache;
   pub use tool_resolver::ToolResolver;
   ```

   With:

   ```rust
   pub use manifest::Manifest;
   pub use port_set::PortSet;
   pub use reporter::Reporter;
   pub use stat_cache::StatCache;
   pub use tool_resolver::ToolResolver;
   ```

3. Re-export it from the crate root. In `crates/buildl-core/src/lib.rs`, add `Reporter` to the `pub use ports::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, PortSet, Ports, StatCache, ToolResolver,
   };
   pub use resolve::resolve;
   ```

   With:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
   };
   pub use resolve::resolve;
   ```

4. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib ports::reporter
   error[E0277]: the trait bound `&mut Names: Reporter` is not satisfied
   error: could not compile `buildl-core` (lib test) due to 1 previous error
   ```

5. Make a borrowed reporter a reporter. `?Sized` lets the borrow be of a trait object too. In `crates/buildl-core/src/ports/reporter.rs`:

   Replace:

   ```rust
       /// Every target has its outcome, counted in `summary`. Called once, last.
       fn summary(&mut self, summary: Summary);
   }

   #[cfg(test)]
   ```

   With:

   ```rust
       /// Every target has its outcome, counted in `summary`. Called once, last.
       fn summary(&mut self, summary: Summary);
   }

   impl<R: Reporter + ?Sized> Reporter for &mut R {
       fn begun(&mut self, targets: usize) {
           (**self).begun(targets);
       }

       fn started(&mut self, action: &ReadyAction) {
           (**self).started(action);
       }

       fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]) {
           (**self).printed(label, channel, bytes);
       }

       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
           (**self).finished(event, printed);
       }

       fn summary(&mut self, summary: Summary) {
           (**self).summary(summary);
       }
   }

   #[cfg(test)]
   ```

6. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib ports::reporter
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 543 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `544 passed` (unit tests), `112 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/ports/reporter.rs
   $ git commit -m "feat(buildl-core): add the reporter port"
   ```

### Task 8 — Add the graph-record port and its fake

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Create `crates/buildl-core/src/ports/graph_record.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test of the fake: it keeps the canonical bytes of every graph handed to it, in order, and once broken it fails as the port says and keeps nothing more. The test builds its graphs through `load` and `resolve`, the public way to one. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Channel, Clock, Command,
           ContentStore, Digest, Directory, Dispatcher, Error, EventLog, ExecStrategy, Exit,
           KeyComponents, Label, NetworkAccess, OutcomeClass, OutcomeEvent, OutputName, OutputPath,
           OwnComponents, Platform, PortName, Provenance, ReadyAction, SourcePath, Timestamp,
       };

       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeLog, FakeStrategy, Loss,
           Order, Pace, Script, dir,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   With:

   ```rust
       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Channel, Clock, Command,
           ContentStore, Digest, Directory, Dispatcher, EntryName, Error, EventLog, ExecStrategy,
           Exit, GraphRecord, KeyComponents, Label, NetworkAccess, OutcomeClass, OutcomeEvent,
           OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance, ReadyAction,
           SourcePath, TargetGraph, Timestamp, json, load, resolve,
       };

       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeGraphRecord, FakeLog,
           FakeSource, FakeStrategy, Loss, Order, Pace, Script, dir, file, target, workspace,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   Replace:

   ```rust
           );
           assert_eq!(digester.stat_calls(), 4);
       }

       #[test]
   ```

   With:

   ```rust
           );
           assert_eq!(digester.stat_calls(), 4);
       }

       /// The graph of a workspace whose root build file declares a target for each of `names`.
       fn graph_of(names: &[&str]) -> TargetGraph {
           let targets = names.iter().map(|name| target(name, &[])).collect();
           let source = FakeSource::new(workspace(vec![("", file(targets, &[]))]));
           let entry = EntryName::parse("build.lua").unwrap();
           resolve(load(&source, &entry).unwrap()).unwrap()
       }

       #[test]
   ```

   Replace:

   ```rust
           assert_eq!(error.to_string(), "event log: the log cannot be written");
           assert_eq!(log.events.len(), 2);
       }

       #[test]
   ```

   With:

   ```rust
           assert_eq!(error.to_string(), "event log: the log cannot be written");
           assert_eq!(log.events.len(), 2);
       }

       #[test]
       fn a_graph_record_keeps_the_bytes_of_every_graph_until_it_is_broken() {
           let one = graph_of(&["app"]);
           let two = graph_of(&["app", "lib"]);
           let mut record = FakeGraphRecord::default();

           record.record(&one).unwrap();
           record.record(&two).unwrap();

           assert_eq!(
               record.recorded,
               [
                   json::canonical::to_vec(&one).unwrap(),
                   json::canonical::to_vec(&two).unwrap()
               ],
               "the canonical bytes of each graph, in the order recorded"
           );

           record.broken = true;
           let error = record.record(&one).unwrap_err();

           assert_eq!(
               error.to_string(),
               "graph record: the graph cannot be written"
           );
           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::GraphRecord),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
           assert_eq!(record.recorded.len(), 2, "a broken record keeps nothing");
       }

       #[test]
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_graph_record
   error[E0432]: unresolved import `buildl_core::GraphRecord`
   error[E0432]: unresolved import `super::FakeGraphRecord`
   error: could not compile `buildl-core` (test "flows") due to 2 previous errors
   ```

3. Create `crates/buildl-core/src/ports/graph_record.rs`. The trait holds no logic, so the file has no test of its own:

   ```rust
   //! The port through which a build writes down the graph it resolved.
   //!
   //! Its own file because the graph a build worked from is kept where a later reader can find it,
   //! and this crate keeps nothing: it hands over the graph and lets an adapter store it.
   //!
   //! Responsibilities: [`GraphRecord`].
   //!
   //! Non-responsibilities: implementing it, and deciding when a graph is recorded. The first
   //! belongs to an adapter crate; the second to the pipeline.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::TargetGraph;

   /// Keeps the graph the last build resolved.
   pub trait GraphRecord {
       /// Replaces what is kept with `graph`.
       ///
       /// An implementation stores the bytes
       /// [`json::canonical::to_vec`](crate::json::canonical::to_vec) writes for `graph`, and no
       /// other byte, so the same workspace always leaves the same record. A reader finds the old
       /// record whole or the new one whole, never part of either.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::GraphRecord`](crate::PortName::GraphRecord), when the graph cannot be stored.
       fn record(&mut self, graph: &TargetGraph) -> Result<()>;
   }
   ```

4. Declare the module, name the port in the index and re-export it. In `crates/buildl-core/src/ports/mod.rs`:

   Replace:

   ```rust
   //! - [`ExecStrategy`] — running one action in a directory of its own.
   //! - [`Dispatcher`] — having several tasks worked on at once.
   //! - [`Reporter`] — showing a person what a build is doing.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   ```

   With:

   ```rust
   //! - [`ExecStrategy`] — running one action in a directory of its own.
   //! - [`Dispatcher`] — having several tasks worked on at once.
   //! - [`GraphRecord`] — the graph a build resolved, kept where a later reader finds it.
   //! - [`Reporter`] — showing a person what a build is doing.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   ```

   Replace:

   ```rust
   pub mod event_log;
   pub mod exec_strategy;
   pub mod host;
   pub mod manifest;
   ```

   With:

   ```rust
   pub mod event_log;
   pub mod exec_strategy;
   pub mod graph_record;
   pub mod host;
   pub mod manifest;
   ```

   Replace:

   ```rust
   pub use event_log::EventLog;
   pub use exec_strategy::ExecStrategy;
   pub use host::Host;
   pub use manifest::Manifest;
   ```

   With:

   ```rust
   pub use event_log::EventLog;
   pub use exec_strategy::ExecStrategy;
   pub use graph_record::GraphRecord;
   pub use host::Host;
   pub use manifest::Manifest;
   ```

5. Re-export it from the crate root. In `crates/buildl-core/src/lib.rs`, add `GraphRecord` to the `pub use ports::{…}` list and run `cargo fmt --all`:

   Replace:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
   };
   pub use resolve::resolve;
   ```

   With:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, GraphRecord, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
   };
   pub use resolve::resolve;
   ```

6. Write the fake. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{Channel, LiveSink, Turn};
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   With:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{Channel, GraphRecord, LiveSink, TargetGraph, Turn, json};
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   Replace:

   ```rust
               }
           }
       }
   }
   ```

   With:

   ```rust
               }
           }
       }
   }

   /// A graph record held in memory: the bytes of every graph handed to it, in order.
   #[derive(Debug, Default)]
   pub struct FakeGraphRecord {
       /// The canonical bytes of each graph recorded.
       pub recorded: Vec<Vec<u8>>,
       /// Whether recording fails.
       pub broken: bool,
   }

   impl GraphRecord for FakeGraphRecord {
       fn record(&mut self, graph: &TargetGraph) -> Result<()> {
           if self.broken {
               return Err(Error::Adapter {
                   port: PortName::GraphRecord,
                   diagnostic: Diagnostic::new("the graph cannot be written"),
               });
           }
           self.recorded.push(json::canonical::to_vec(graph)?);
           Ok(())
       }
   }
   ```

7. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_graph_record
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 112 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `544 passed` (unit tests), `113 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/src/ports/graph_record.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): add the graph-record port"
   ```

### Task 9 — Add a fake reporter

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test: the fake keeps every call as an owned value, in the order received. In the `tests` module of `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       use std::sync::atomic::{AtomicU32, Ordering};

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Channel, Clock, Command,
           ContentStore, Digest, Directory, Dispatcher, EntryName, Error, EventLog, ExecStrategy,
           Exit, GraphRecord, KeyComponents, Label, NetworkAccess, OutcomeClass, OutcomeEvent,
           OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance, ReadyAction,
           SourcePath, TargetGraph, Timestamp, json, load, resolve,
       };

       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeGraphRecord, FakeLog,
           FakeSource, FakeStrategy, Loss, Order, Pace, Script, dir, file, target, workspace,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   With:

   ```rust
       use std::sync::atomic::{AtomicU32, Ordering};

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Captured, Channel, Clock,
           Command, ContentStore, Digest, Directory, Dispatcher, EntryName, Error, EventLog,
           ExecStrategy, Exit, GraphRecord, KeyComponents, Label, NetworkAccess, OutcomeClass,
           OutcomeEvent, OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance,
           ReadyAction, Reporter, SourcePath, Summary, TargetGraph, Timestamp, json, load, resolve,
       };

       use super::{
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeGraphRecord, FakeLog,
           FakeReporter, FakeSource, FakeStrategy, Loss, Order, Pace, Reported, Script, dir, file,
           target, workspace,
       };
       use super::{FakeContent, FakeDigester, Leaf, tree};
   ```

   Replace:

   ```rust
           }
           assert_eq!(record.recorded.len(), 2, "a broken record keeps nothing");
       }

       #[test]
   ```

   With:

   ```rust
           }
           assert_eq!(record.recorded.len(), 2, "a broken record keeps nothing");
       }

       #[test]
       fn a_reporter_keeps_every_call_it_receives_in_order() {
           let counts = Summary {
               failed: 0,
               skipped: 0,
               built: 1,
               cached: 1,
           };
           let mut reporter = FakeReporter::default();

           reporter.begun(2);
           reporter.started(&action("//:app", &[], &["app"]));
           reporter.printed(&label("//:app"), Channel::Stderr, b"warn\n");
           reporter.finished(
               &event("//:lib"),
               Captured {
                   stdout: b"out",
                   stderr: b"err",
               },
           );
           reporter.summary(counts);

           assert_eq!(
               reporter.calls,
               [
                   Reported::Begun(2),
                   Reported::Started(label("//:app"), None),
                   Reported::Printed(label("//:app"), Channel::Stderr, b"warn\n".to_vec()),
                   Reported::Finished {
                       label: label("//:lib"),
                       class: OutcomeClass::Cached,
                       stdout: b"out".to_vec(),
                       stderr: b"err".to_vec(),
                   },
                   Reported::Summary(counts),
               ]
           );
       }

       #[test]
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_reporter
   error[E0432]: unresolved imports `super::FakeReporter`, `super::Reported`
   error: could not compile `buildl-core` (test "flows") due to 1 previous error
   ```

3. Write the fake. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{Channel, GraphRecord, LiveSink, TargetGraph, Turn, json};
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   With:

   ```rust
       StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   use buildl_core::{
       Captured, Channel, GraphRecord, LiveSink, OutcomeClass, Reporter, Summary, TargetGraph, Turn,
       json,
   };
   use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

   /// What the fake holds for one directory.
   ```

   Replace:

   ```rust
           self.recorded.push(json::canonical::to_vec(graph)?);
           Ok(())
       }
   }
   ```

   With:

   ```rust
           self.recorded.push(json::canonical::to_vec(graph)?);
           Ok(())
       }
   }

   /// One call a [`FakeReporter`] received.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Reported {
       /// `begun`, with the number of targets.
       Begun(usize),
       /// `started`, with the action's label and its description.
       Started(Label, Option<String>),
       /// `printed`.
       Printed(Label, Channel, Vec<u8>),
       /// `finished`, with the outcome's label and class and what was left to show.
       Finished {
           /// The target.
           label: Label,
           /// How it ended.
           class: OutcomeClass,
           /// What it wrote to standard output, as handed over.
           stdout: Vec<u8>,
           /// What it wrote to standard error, as handed over.
           stderr: Vec<u8>,
       },
       /// `summary`.
       Summary(Summary),
   }

   /// A reporter that shows nothing and keeps every call it received, in order.
   #[derive(Debug, Default)]
   pub struct FakeReporter {
       /// The calls.
       pub calls: Vec<Reported>,
   }

   impl Reporter for FakeReporter {
       fn begun(&mut self, targets: usize) {
           self.calls.push(Reported::Begun(targets));
       }

       fn started(&mut self, action: &ReadyAction) {
           self.calls.push(Reported::Started(
               action.label.clone(),
               action
                   .description
                   .as_ref()
                   .map(|description| description.as_str().to_owned()),
           ));
       }

       fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]) {
           self.calls
               .push(Reported::Printed(label.clone(), channel, bytes.to_vec()));
       }

       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
           self.calls.push(Reported::Finished {
               label: event.outcome.label.clone(),
               class: event.outcome.class.clone(),
               stdout: printed.stdout.to_vec(),
               stderr: printed.stderr.to_vec(),
           });
       }

       fn summary(&mut self, summary: Summary) {
           self.calls.push(Reported::Summary(summary));
       }
   }
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --test flows common::tests::a_reporter
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `544 passed` (unit tests), `114 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs
   $ git commit -m "test(buildl-core): add a fake reporter"
   ```

### Task 10 — Name the graph record and the reporter in the bundle

**Files:**
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing implementation. In `crates/buildl-core/tests/flows/common.rs`, make the bundle choose the two new fakes:

   Replace:

   ```rust
       type Dispatcher = FakeDispatcher;
       type Clock = FakeClock;
   }

   /// A workspace that writes to `out` and `.buildl`, under a ceiling allowing `cc` to run, any
   ```

   With:

   ```rust
       type Dispatcher = FakeDispatcher;
       type Clock = FakeClock;
       type Graph = FakeGraphRecord;
       type Reporter = FakeReporter;
   }

   /// A workspace that writes to `out` and `.buildl`, under a ceiling allowing `cc` to run, any
   ```

2. Run the flows and confirm they fail to build, because `Ports` names twelve ports:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0437]: type `Graph` is not a member of trait `Ports`
   error[E0437]: type `Reporter` is not a member of trait `Ports`
   error: could not compile `buildl-core` (test "flows") due to 2 previous errors
   ```

3. Add the two associated types, and make the doc example choose an implementation of each. In `crates/buildl-core/src/ports/bundle.rs`:

   Replace:

   ```rust
   use crate::ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, StatCache, ToolResolver,
   };

   /// One implementation of each port, chosen together.
   ```

   With:

   ```rust
   use crate::ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, GraphRecord, Host, Manifest, Reporter, StatCache, ToolResolver,
   };

   /// One implementation of each port, chosen together.
   ```

   Replace:

   ```rust
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileMode, Found, Host, InputStat, Label, LiveSink, Manifest,
   ///     OutcomeEvent, OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction,
   ///     Result, SourcePath, StatCache, StatEntry, Timestamp, ToolResolver, Turn, Workspace,
   /// };
   ///
   ```

   With:

   ```rust
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Captured, Ceiling, Channel, Clock, ContentStore,
   ///     DeclarationSource, Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue,
   ///     Error, Evaluated, EventLog, ExecStrategy, FileMode, Found, GraphRecord, Host, InputStat,
   ///     Label, LiveSink, Manifest, OutcomeEvent, OutputPath, Platform, PortName, Ports, Program,
   ///     RawOutcome, ReadyAction, Reporter, Result, SourcePath, StatCache, StatEntry, Summary,
   ///     TargetGraph, Timestamp, ToolResolver, Turn, Workspace,
   /// };
   ///
   ```

   Replace:

   ```rust
   /// }
   ///
   /// impl Clock for Nothing {
   ///     fn now(&self) -> Timestamp {
   ```

   With:

   ```rust
   /// }
   ///
   /// impl GraphRecord for Nothing {
   ///     fn record(&mut self, _graph: &TargetGraph) -> Result<()> {
   ///         Err(nothing(PortName::GraphRecord))
   ///     }
   /// }
   ///
   /// impl Reporter for Nothing {
   ///     fn begun(&mut self, _targets: usize) {}
   ///
   ///     fn started(&mut self, _action: &ReadyAction) {}
   ///
   ///     fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}
   ///
   ///     fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}
   ///
   ///     fn summary(&mut self, _summary: Summary) {}
   /// }
   ///
   /// impl Clock for Nothing {
   ///     fn now(&self) -> Timestamp {
   ```

   Replace:

   ````rust
   ///     type Dispatcher = Nothing;
   ///     type Clock = Nothing;
   /// }
   /// ```
   ````

   With:

   ````rust
   ///     type Dispatcher = Nothing;
   ///     type Clock = Nothing;
   ///     type Graph = Nothing;
   ///     type Reporter = Nothing;
   /// }
   /// ```
   ````

   Replace:

   ```rust
       /// thread that runs it.
       type Clock: Clock + Sync;
   }
   ```

   With:

   ```rust
       /// thread that runs it.
       type Clock: Clock + Sync;
       /// Where the graph a build resolved is kept.
       type Graph: GraphRecord;
       /// How a build shows what it is doing. Called only from the thread that called the build.
       type Reporter: Reporter;
   }
   ```

4. Make the bundle of the set's own test choose its stub for both. In the `tests` module of `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
       use crate::ports::{
           ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
           ExecStrategy, Host, Manifest, Ports, StatCache, ToolResolver,
       };
       use crate::types::{
           BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
           FileMode, Found, InputStat, Label, LiveSink, OutcomeEvent, OutputPath, Platform, Program,
           RawOutcome, ReadyAction, SourcePath, StatEntry, Timestamp, Turn, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   With:

   ```rust
       use crate::ports::{
           ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
           ExecStrategy, GraphRecord, Host, Manifest, Ports, Reporter, StatCache, ToolResolver,
       };
       use crate::types::{
           BuildFile, CacheRow, Captured, Ceiling, Channel, Digest, Directory, EnvName, EnvValue,
           Evaluated, Exit, FileMode, Found, InputStat, Label, LiveSink, OutcomeEvent, OutputPath,
           Platform, Program, RawOutcome, ReadyAction, SourcePath, StatEntry, Summary, TargetGraph,
           Timestamp, Turn, Workspace,
       };

       /// One port type answering that there is nothing, standing in for every port.
   ```

   Replace:

   ```rust
       }

       impl Clock for Stub {
           fn now(&self) -> Timestamp {
   ```

   With:

   ```rust
       }

       impl GraphRecord for Stub {
           fn record(&mut self, _graph: &TargetGraph) -> Result<()> {
               Ok(())
           }
       }

       impl Reporter for Stub {
           fn begun(&mut self, _targets: usize) {}

           fn started(&mut self, _action: &ReadyAction) {}

           fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

           fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}

           fn summary(&mut self, _summary: Summary) {}
       }

       impl Clock for Stub {
           fn now(&self) -> Timestamp {
   ```

   Replace:

   ```rust
           type Dispatcher = Stub;
           type Clock = Stub;
       }

       #[test]
   ```

   With:

   ```rust
           type Dispatcher = Stub;
           type Clock = Stub;
           type Graph = Stub;
           type Reporter = Stub;
       }

       #[test]
   ```

5. Run the flows and the bundle's doc example and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows
   test result: ok. 114 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo test -p buildl-core --doc ports::bundle
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `544 passed` (unit tests), `114 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): name the graph record and the reporter in the Ports bundle"
   ```

### Task 11 — Hold the graph record and the reporter in the set

**Files:**
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test. The test of the set's `Debug` builds a set of fourteen ports and looks for fourteen fields. In the `tests` module of `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
               dispatcher: Stub,
               clock: Stub,
           };

           let text = format!("{set:?}");
   ```

   With:

   ```rust
               dispatcher: Stub,
               clock: Stub,
               graph: Stub,
               reporter: Stub,
           };

           let text = format!("{set:?}");
   ```

   Replace:

   ```rust
               "dispatcher",
               "clock",
           ] {
               assert!(
   ```

   With:

   ```rust
               "dispatcher",
               "clock",
               "graph",
               "reporter",
           ] {
               assert!(
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib ports::port_set
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `graph`
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `reporter`
   error: could not compile `buildl-core` (lib test) due to 2 previous errors
   ```

3. Add the two fields, ask their types to be `Debug` and render them. The two sentences that count the ports say fourteen. In `crates/buildl-core/src/ports/port_set.rs`:

   Replace:

   ```rust
   //! built from one argument and a phase that needs several ports borrows one value.
   //!
   //! Responsibilities: [`PortSet`], and its `Debug` rendering, which asks only that the twelve
   //! port types be `Debug`.
   //!
   ```

   With:

   ```rust
   //! built from one argument and a phase that needs several ports borrows one value.
   //!
   //! Responsibilities: [`PortSet`], and its `Debug` rendering, which asks only that the fourteen
   //! port types be `Debug`.
   //!
   ```

   Replace:

   ```rust
       /// Reads the wall clock.
       pub clock: P::Clock,
   }

   /// Written by hand because a derive would require the bundle type itself to be `Debug`, and a
   /// bundle is only a choice of port types. This asks only that the twelve port types be `Debug`.
   impl<P: Ports> fmt::Debug for PortSet<P>
   where
   ```

   With:

   ```rust
       /// Reads the wall clock.
       pub clock: P::Clock,
       /// Keeps the graph a build resolved.
       pub graph: P::Graph,
       /// Shows what a build is doing.
       pub reporter: P::Reporter,
   }

   /// Written by hand because a derive would require the bundle type itself to be `Debug`, and a
   /// bundle is only a choice of port types. This asks only that the fourteen port types be `Debug`.
   impl<P: Ports> fmt::Debug for PortSet<P>
   where
   ```

   Replace:

   ```rust
       P::Dispatcher: fmt::Debug,
       P::Clock: fmt::Debug,
   {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
   ```

   With:

   ```rust
       P::Dispatcher: fmt::Debug,
       P::Clock: fmt::Debug,
       P::Graph: fmt::Debug,
       P::Reporter: fmt::Debug,
   {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
   ```

   Replace:

   ```rust
               .field("dispatcher", &self.dispatcher)
               .field("clock", &self.clock)
               .finish()
       }
   ```

   With:

   ```rust
               .field("dispatcher", &self.dispatcher)
               .field("clock", &self.clock)
               .field("graph", &self.graph)
               .field("reporter", &self.reporter)
               .finish()
       }
   ```

4. Give the flows' set its two new ports. In `crates/buildl-core/tests/flows/common.rs`:

   Replace:

   ```rust
   /// An [`open_workspace`]; a `linux/x86_64` machine with no variable set; no workspace file,
   /// nothing recorded, no program and no row; an empty content store and an empty log; a strategy
   /// with no script; a dispatcher taking the oldest task first; and a clock that has not been
   /// read.
   #[must_use]
   pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
   ```

   With:

   ```rust
   /// An [`open_workspace`]; a `linux/x86_64` machine with no variable set; no workspace file,
   /// nothing recorded, no program and no row; an empty content store and an empty log; a strategy
   /// with no script; a dispatcher taking the oldest task first, one at a time; a clock that has
   /// not been read; no graph recorded; and a reporter that has received no call.
   #[must_use]
   pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
   ```

   Replace:

   ```rust
           dispatcher: FakeDispatcher::default(),
           clock: FakeClock::default(),
       }
   }
   ```

   With:

   ```rust
           dispatcher: FakeDispatcher::default(),
           clock: FakeClock::default(),
           graph: FakeGraphRecord::default(),
           reporter: FakeReporter::default(),
       }
   }
   ```

5. Run the test, then every flow, and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib ports::port_set
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 543 filtered out
   $ cargo test -p buildl-core --test flows
   test result: ok. 114 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and its three `buildl-core` results read `544 passed` (unit tests), `114 passed` (flows) and `13 passed` (doctests), each with `0 failed`. `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): hold the graph record and the reporter in the PortSet"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0` after every task; `crates/buildl-core/Cargo.toml`, `Cargo.lock`, `crates/expected-edges.txt` and `crates/expected-module-edges.txt` are unchanged, and no file outside `crates/buildl-core` changes.
- `Digester::stat`, `Dispatcher::dispatch` and `ExecStrategy::run` have the signatures of spec §2.12, §2.3 and §2.4; `Reporter` and `GraphRecord` those of §2.2 and §2.7; `Manifest::workspace`, `ContentStore::contains`, `ContentStore::get` and `StatCache::record` carry the sentences of §2.9.
- `Ports` names fourteen ports and `PortSet` holds one instance of each; its `Debug` asks only that the fourteen port types be `Debug`.
- Plan refuses an input that is a directory, a link or a special file with `Error::InputNotAFile`, naming the lowest-labelled target that declares it, before anything is hashed. The three texts: `build.lua: //:lib: input lib.c is a directory; an input must be a regular file`, `… is a symbolic link; …`, `… is a pipe, a socket or a device; …`.
- Each fake passes the test that holds it to its contract: a digester reports a file, nothing, and what is not a file; what a task said reaches `step` before its result; tasks run together say everything first and end last-taken first; what a run hands over live joins to exactly what it returns; a scripted signal is how the run ended; a broken graph record keeps nothing; a reporter keeps every call in order.
- Unit tests of `buildl-core`: 544 (the 543 plan `01` left and the reporter's); flows: 114 (the 108 there were, one flow and five tests of fakes); doctests: 13.
- No phase reports anything, records a graph or shows an action live yet: a worker says nothing through the dispatcher and runs every command with no live sink. That is plan `03`.
- What plan `03` and the adapter plans are written against:

  | Item | Public API |
  |---|---|
  | `Digester` | `stat(&self, &SourcePath) -> Result<InputStat>`, `digest` unchanged |
  | `Dispatcher` | `dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>` with `T: Send, M: Send, R: Send, W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync, S: FnMut(Turn<M, R>) -> Vec<T>` |
  | `ExecStrategy` | `run(&self, &Self::Dir, &ReadyAction, Option<LiveSink<'_>>) -> Result<RawOutcome>`; `prepare` and `collect` unchanged |
  | `Reporter` | `begun(&mut self, usize)`, `started(&mut self, &ReadyAction)`, `printed(&mut self, &Label, Channel, &[u8])`, `finished(&mut self, &OutcomeEvent, Captured<'_>)`, `summary(&mut self, Summary)`; implemented for `&mut R` where `R: Reporter + ?Sized` |
  | `GraphRecord` | `record(&mut self, &TargetGraph) -> Result<()>` |
  | `Ports` | the twelve associated types it had, `type Graph: GraphRecord` and `type Reporter: Reporter` |
  | `PortSet<P>` | the twelve public fields it had, `pub graph: P::Graph` and `pub reporter: P::Reporter` |
  | `FakeDigester` | `pub files`, `pub others: BTreeMap<SourcePath, InputKind>`, `stat_calls()`, `digest_calls()`, `digested()` |
  | `FakeDispatcher` (`Default`) | `pub order: Order`, `pub pace: Pace`, `pub lose: BTreeMap<usize, Loss>`; `Pace::{OneAtATime, Together}`, `OneAtATime` by default |
  | `Ends` | `Exits(i32)`, `Signaled(i32)`, `TimesOut`, `PrepareFails`, `RunFails`, `CollectFails`, `CollectShort` |
  | `FakeGraphRecord` (`Default`) | `pub recorded: Vec<Vec<u8>>`, `pub broken: bool` |
  | `FakeReporter` (`Default`) | `pub calls: Vec<Reported>`; `Reported::{Begun(usize), Started(Label, Option<String>), Printed(Label, Channel, Vec<u8>), Finished { label, class, stdout, stderr }, Summary(Summary)}` |
