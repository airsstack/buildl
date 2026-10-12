---
status: approved
created: 2026-10-11
depends-on: [05, 06, 07, 08]
---

# Pipeline Build Implementation Plan

**Goal:** The pipeline builds a workspace to one outcome per target.

**Architecture:** `execute/assembly.rs` holds `execute`, a free function generic over the `Ports` bundle, re-exported from the crate root as `load`, `resolve` and `plan` are. Everything that can refuse a build is checked before any of it runs, in a fixed order: the plan was made from the graph, the manifest answers, no two outputs collide, and every command — expanded first — names a program the workspace allows. Only then does it call the dispatcher with two closures. `work` runs one task on whatever thread the dispatcher chooses and reads one `AtomicBool`, the stop flag, before beginning anything. `step` runs on the calling thread and is a method of a private `Build` value that owns the schedule, one outcome slot per target, the queue of started targets waiting to be settled, and the one error that fails the whole build; it alone writes the cache and the log, and it alone sets the stop flag. The three steps that ask a port live beside the decisions they wrap, as thin functions added to `execute/settle.rs`, `execute/perform.rs` and `execute/record.rs`; they are written here, with the flows that are their tests, because nothing inside the crate implements a port. `Pipeline::build` runs Load once, Resolve, Plan, then Execute. The flows drive it through the fakes, one flow or a small group per row of the spec's flow table, and one flow pins the log of a fixture workspace in a golden file.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D5, D6, D7, D8, D9, D13, D14, D15, D19, D20, D21, D22), §3.2 (the action handed to a strategy), §4 (the contracts of the ports), §5.1 (signature, placement, order), §5.6 to §5.9 (settling, one action on a worker, recording, the loop), §6 (the pipeline), §8 (the module layout and the edge file), §9 (the row for `execute/assembly.rs`, and every row of the flow table).

**Checkpoints:** review twice: once when task 1 is done, and once when tasks 2 to 5 are done.

---

## Context an implementer needs

What plans `01` to `08` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Layout::of(graph, out) -> Result<Layout>`, `Layout::outputs(id)`, `Layout::deps(graph, id)` | `crates/buildl-core/src/execute/layout.rs:34`, `:75`, `:81` | step 3 of `execute`, and where each output is put |
| `Lists { inputs, outputs, deps }`, `expand(command, settings, lists) -> Vec<String>` | `crates/buildl-core/src/execute/expand.rs:22`, `:61` | step 4: a command's placeholders replaced |
| `admit(expanded, node, ceiling) -> Result<Command>` | `crates/buildl-core/src/execute/admission.rs:30` | step 4: `NoProgram`, `Refused` |
| `Schedule::{new, start, complete, abandon, stopped}`, `Finish { Succeeded, Failed }`, `Effects { started, skipped }` | `crates/buildl-core/src/execute/schedule.rs:54`, `:32`, `:41` | which targets may start, which never will |
| `finished_key(own, deps) -> Result<ActionKey>`, `is_cached(freshness, key, row, held) -> bool` | `crates/buildl-core/src/execute/settle.rs:30`, `:59` | the two decisions `settle` asks the ports for |
| `Task { node, key, action }`, `Worked { NotBegun(NodeId), Performed(Box<Performed>) }`, `Performed { node, key, span, ended }`, `Span`, `Printed`, `Ended { Produced { outputs, printed }, Failed { class, diagnostic, printed } }`, `failed_to`, `exited`, `collected` | `crates/buildl-core/src/execute/perform.rs:22`, `:33`, `:43`, `:59`, `:68`, `:77`, `:100`, `:112`, `:134` | what crosses between the calling thread and a worker, and how `perform` decides how an action ended |
| `Progress { Began { key, span }, Keyed { key }, Skipped }`, `row`, `built`, `cached`, `failed`, `unstored`, `unplaced`, `mismatched`, `skipped`, `event` | `crates/buildl-core/src/execute/record.rs:28`, `:48` to `:126` | the row, the outcome and the event that `store`, `restore` and `append` hand to the ports |
| `execute/mod.rs` declaring `admission`, `expand`, `layout`, `perform`, `record`, `schedule` and `settle`, each under a `cfg_attr(not(test), expect(dead_code, …))`, then `#[cfg(test)] mod fixtures;` | `crates/buildl-core/src/execute/mod.rs` | the file that becomes the phase's table of contents |
| `command`, `graph`, `key`, `label`, `node`, `output_path`, `own`, `path` | `crates/buildl-core/src/execute/fixtures.rs` | the builders the unit tests of `assembly.rs` use; none is added |
| `ContentStore::{put, contains, place}` (`Send + Sync`), `ActionCache::{row, record}`, `EventLog::append`, `ExecStrategy::{prepare, run, collect}` (`Send + Sync`), `Dispatcher::dispatch(work, step)`, `Clock::now`, `Manifest::workspace` | `crates/buildl-core/src/ports/` | everything `execute` reaches outside the crate |
| `Ports` with `Cas`, `Log`, `Strategy`, `Dispatcher` and `Clock: Clock + Sync`; `PortSet<P>` with one public field per port | `crates/buildl-core/src/ports/bundle.rs:198`, `crates/buildl-core/src/ports/port_set.rs:20` | what `execute` borrows, field by field |
| `Error::ForeignPlan`, `Error::OutputConflict`, `Error::NoProgram`, `Error::Refused`, `Error::Adapter` | `crates/buildl-core/src/error.rs:242`, `:252`, `:267`, `:201`, `:273` | the errors a build can end in |
| `ReadyAction`, `OnFailure { Stop, KeepGoing }`, `ActionOutcome`, `OutcomeClass::succeeded`, `Summary::of`, `OutcomeEvent`, `OutputPath::of` | `crates/buildl-core/src/types/ready_action.rs:24`, `on_failure.rs:15`, `outcome.rs:79`, `:72`, `:112`, `outcome_event.rs:24`, `output_path.rs:35` | what a strategy is handed, the policy, and what a build returns and logs |
| `Plan::len`, `Plan::label`, `Plan::verdicts`; `Verdict`, `PlannedKey`, `Unfinished` | `crates/buildl-core/src/types/plan.rs` | where a target's own key components come from |
| `Pipeline::ports`, `ports_mut`, `graph`, `plan` | `crates/buildl-core/src/pipeline/driver.rs` | the methods `build` joins, and what the flows change between two builds |
| `FakeCas`, `FakeCache` (`failing`, `recorded()`), `FakeLog` (`broken`), `FakeStrategy` (`files`, `scripts`, `handed()`, `ran()`, `runs()`), `Script`, `Ends`, `Content`, `FakeDispatcher` with `Order`, `FakeClock` (`reads()`), `ports`, `open_workspace`, `program`, `dir`, `file`, `target`, `target_with`, `rule`, `alias`, `setting`, `workspace` | `crates/buildl-core/tests/flows/common.rs` | what the flows are built from; this plan changes none of it |
| 22 lines, with `execute -> error` followed by `execute -> types` | `crates/expected-module-edges.txt` | the golden list that gains two edges |

Gate facts that bite here. A bullet that names a lint or a warning reports one seen in a run while
this code was prototyped:

- Once `execute` calls every file of the phase, an `expect(dead_code)` left on a `mod` line is
  unfulfilled and fails the gate. Task 1 removes all seven in the same commit that adds the caller.
- `pipeline/driver.rs` gains `crate::execute`, and four files of `execute/` gain `crate::ports`, so
  `crates/expected-module-edges.txt` gains `pipeline -> execute` and `execute -> ports` in task 1.
  The file must hold no edge between `execute` and `load`, `resolve` or `plan`.
- The edge guard reads `crate::<module>` on lines that are not comments and refuses an item named
  through the crate root, so every import is written `crate::<module>::<item>`. Doc comments are
  not read by it.
- `buildl-core` starts no thread and holds no `Arc`. An atomic is not among its banned APIs:
  `grep -c -i atomic crates/buildl-core/clippy.toml` prints `0`, where the same search for
  `std::thread::spawn` prints `1`. The stop flag is a `core::sync::atomic::AtomicBool` that lives on
  the stack of `execute` and is borrowed by both closures.
- A flow helper that no flow calls is dead code in the test binary: the test build warns
  `function … is never used` and the gate refuses it. Each task therefore adds only the helpers its
  own flows use, and each task widens the file's imports by what its flows name.
- clippy's `redundant_closure_for_method_calls` rejects `.map(|argument| argument.as_str())`; the
  flows render arguments and paths with `.map(ToString::to_string)`.
- The digester and the strategy each hold the workspace's files: the first hashes them for the
  plan, the second derives what an unscripted action leaves from the digests of the action's own
  inputs. The flows' `put` writes a file to both. Because an unscripted output depends on the
  action's own inputs alone, a target that runs again only because a dependency changed leaves
  the bytes it left before, and what depends on it stays cached; the flow that pins a rebuild
  reaching a dependent therefore edits the input of the dependent's own dependency.
- `FakeClock` answers 1, 2, 3 … and `FakeDispatcher` runs one task at a time, so every instant
  in the golden log is fixed. A worker reads the clock twice for an action it begins and the
  calling thread once per event.
- The golden file holds one line per event and ends with a newline; the flow compares the lines
  joined by `\n` against `GOLDEN.trim_end()`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/execute/assembly.rs         — [create] execute: the six steps, the stop flag, the two closures, unit tests of its helpers
crates/buildl-core/src/execute/settle.rs           — [modify] Settled, settle: ask the cache and the store
crates/buildl-core/src/execute/perform.rs          — [modify] perform: take one task through the strategy and the store
crates/buildl-core/src/execute/record.rs           — [modify] store, restore, append: place, record, log
crates/buildl-core/src/execute/mod.rs              — [modify] declare assembly, export execute, drop the dead-code expectations
crates/buildl-core/src/lib.rs                      — [modify] re-export execute
crates/buildl-core/src/pipeline/driver.rs          — [modify] Pipeline::build
crates/buildl-core/tests/flows/build.rs            — [create] the flows
crates/buildl-core/tests/flows/golden/log.jsonl    — [create] the pinned events of one build
crates/buildl-core/tests/flows/main.rs             — [modify] declare the flow module
crates/expected-module-edges.txt                   — [modify] two edges
```

### Task 1 — Build a workspace through the ports

**Files:**
- Create `crates/buildl-core/src/execute/assembly.rs`
- Create `crates/buildl-core/tests/flows/build.rs`
- Modify `crates/buildl-core/src/execute/settle.rs`
- Modify `crates/buildl-core/src/execute/perform.rs`
- Modify `crates/buildl-core/src/execute/record.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/main.rs`
- Modify `crates/expected-module-edges.txt`

**Steps:**

1. Declare the flow module. `crates/buildl-core/tests/flows/main.rs` becomes:

   ```rust
   //! Pipeline flows through the ports, driven by fakes built from the public API alone.
   //!
   //! One test binary, so the fakes are shared by every flow without being compiled once per file.
   //! That the fakes compile at all is itself a check: an adapter outside this crate can build staged
   //! targets, settings and subdirs and implement every port the same way.

   pub mod common;

   mod build;
   mod check;
   mod graph;
   mod load;
   mod plan;
   ```

2. Write the failing flows. Create `crates/buildl-core/tests/flows/build.rs`:

   ```rust
   //! `Pipeline::build`, driven through every port by the fakes of [`crate::common`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, FileStat, Label, OnFailure, OutcomeClass, OutcomeEvent,
       OutputPath, Pipeline, SettingName, SettingValue, SourcePath, Summary, Timestamp, Written,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, dir, file, ports, program, target_with, workspace,
   };

   /// The values given to settings for one run.
   type Overrides = BTreeMap<SettingName, SettingValue>;

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   fn path(raw: &str) -> SourcePath {
       SourcePath::parse(raw).unwrap()
   }

   /// Writes or replaces a workspace file holding `text`, last modified at `modified`: for the
   /// digester that hashes it, and for the actions that read it.
   fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str, text: &str, modified: u64) {
       let content = FakeContent {
           bytes: text.as_bytes().to_vec(),
           stat: FileStat {
               modified: Timestamp::from_unix_nanos(modified),
               size: u64::try_from(text.len()).unwrap(),
           },
       };
       let ports = pipeline.ports_mut();
       ports.digester.files.insert(path(raw), content);
       ports
           .strategy
           .files
           .insert(path(raw), text.as_bytes().to_vec());
   }

   /// A build of the workspace with no setting overridden.
   fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
       pipeline
           .build(&entry(), &Overrides::new(), on_failure)
           .unwrap()
   }

   /// The last event logged for the target `raw` names.
   fn event<'p>(pipeline: &'p Pipeline<FakePorts>, raw: &str) -> &'p OutcomeEvent {
       let wanted = label(raw);
       pipeline
           .ports()
           .log
           .events
           .iter()
           .rev()
           .find(|event| event.outcome.label == wanted)
           .unwrap()
   }

   /// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone. Each reads one file and
   /// runs `cc`, which the fake resolver can find.
   fn chain() -> Pipeline<FakePorts> {
       let reading = |name: &str, input: &str, deps: &[&str]| {
           target_with(name, |target| {
               target.inputs = vec![Written::new(input)];
               target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
           })
       };
       let source = FakeSource::new(workspace(vec![(
           "",
           file(
               vec![
                   reading("gen", "gen.c", &[]),
                   reading("lib", "lib.c", &["gen"]),
                   reading("app", "main.c", &["lib"]),
                   reading("docs", "README", &[]),
               ],
               &[],
           ),
       )]));
       let mut pipeline = Pipeline::new(ports(source));
       for name in ["gen.c", "lib.c", "main.c", "README"] {
           put(&mut pipeline, name, name, 100);
       }
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       pipeline
   }

   #[test]
   fn a_workspace_never_built_builds_every_target_after_what_it_depends_on() {
       let mut pipeline = chain();

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       let labels: Vec<String> = outcomes
           .iter()
           .map(|outcome| outcome.label.to_string())
           .collect();
       assert_eq!(
           labels,
           ["//:app", "//:docs", "//:gen", "//:lib"],
           "one outcome per target, in id order"
       );
       assert_eq!(
           Summary::of(&outcomes),
           Summary {
               failed: 0,
               skipped: 0,
               built: 4,
               cached: 0
           }
       );

       let ports = pipeline.ports();
       let placed = ports.cas.placed();
       for outcome in &outcomes {
           assert_eq!(outcome.class, OutcomeClass::Built, "{}", outcome.label);
           let row = ports.cache.rows.get(&outcome.label).unwrap();
           assert_eq!(row.produced(), outcome.outputs, "{}", outcome.label);
           assert_eq!(outcome.outputs.len(), 1, "{}", outcome.label);
           for produced in &outcome.outputs {
               assert!(ports.cas.blob(&produced.digest).is_some());
               let at = OutputPath::of(&dir("out"), &outcome.label, &produced.name);
               assert_eq!(placed.get(&at), Some(&produced.digest), "{at}");
           }
           let printed = format!("{}\n", outcome.label).into_bytes();
           assert_eq!(ports.cas.blob(&outcome.stdout.unwrap()), Some(printed));
           assert_eq!(outcome.stderr, Some(Digest::of(b"")));
       }
       assert_eq!(placed.len(), 4);
       assert_eq!(ports.log.events.len(), 4, "one event per target");
       assert_eq!(ports.cache.recorded().len(), 4);

       for (target, dependency) in [("//:lib", "//:gen"), ("//:app", "//:lib")] {
           let began = event(&pipeline, target).started.unwrap();
           let done = event(&pipeline, dependency).finished.unwrap();
           assert!(began > done, "{target} began after {dependency} was done");
       }
       assert_eq!(
           ports.strategy.ran(),
           [
               label("//:docs"),
               label("//:gen"),
               label("//:lib"),
               label("//:app")
           ],
           "the oldest waiting task is taken first"
       );
   }

   #[test]
   fn a_second_build_with_nothing_changed_runs_nothing_and_places_every_output_again() {
       let mut pipeline = chain();
       build(&mut pipeline, OnFailure::Stop);
       let rows = pipeline.ports().cache.rows.clone();
       let placed = pipeline.ports().cas.placed();
       let reads = pipeline.ports().clock.reads();

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           Summary::of(&outcomes),
           Summary {
               failed: 0,
               skipped: 0,
               built: 0,
               cached: 4
           }
       );
       let ports = pipeline.ports();
       for outcome in &outcomes {
           assert_eq!(outcome.class, OutcomeClass::Cached, "{}", outcome.label);
           let row = rows.get(&outcome.label).unwrap();
           assert_eq!(outcome.outputs, row.produced(), "{}", outcome.label);
           assert_eq!(outcome.stdout, Some(row.stdout()));
           let logged = event(&pipeline, &outcome.label.to_string());
           assert_eq!(logged.key, Some(row.key().digest()));
           assert_eq!((logged.started, logged.finished), (None, None));
       }
       assert_eq!(ports.strategy.ran().len(), 4, "no action ran again");
       assert_eq!(ports.cache.recorded().len(), 4, "no row was written again");
       assert_eq!(ports.cache.rows, rows);
       assert_eq!(ports.cas.placed(), placed);
       assert_eq!(
           ports.cas.placements().len(),
           8,
           "every output was placed again"
       );
       assert_eq!(ports.log.events.len(), 8);
       assert_eq!(
           ports.clock.reads() - reads,
           4,
           "the clock was read once per event and by no worker"
       );
   }
   ```

   `chain` is the workspace most flows of this plan build: `//:app` needs `//:lib`, which needs `//:gen`, and `//:docs` stands alone. Targets are numbered in label order, so the outcomes come back as `//:app`, `//:docs`, `//:gen`, `//:lib`, while the actions run as `//:docs`, `//:gen`, `//:lib`, `//:app`: the two targets that wait on nothing start first, in id order, and the fake dispatcher takes the oldest waiting task.

3. Run them and confirm they fail to build, because `Pipeline` has no `build`:

   ```
   $ cargo test -p buildl-core --test flows build::
   error[E0599]: no method named `build` found for mutable reference `&mut Pipeline<FakePorts>` in the current scope
   ```

4. Take one task through the strategy and the store. In `crates/buildl-core/src/execute/perform.rs`, update the module doc and the imports, and add `perform` with its two private helpers directly above `#[cfg(test)]`. As a diff:

   ```diff
   --- a/crates/buildl-core/src/execute/perform.rs
   +++ b/crates/buildl-core/src/execute/perform.rs
   @@ -6,15 +6,19 @@
    //! same exit and the same collected outputs always end the same way.
    //!
    //! Responsibilities: [`Task`], [`Worked`], [`Performed`], [`Span`], [`Printed`] and [`Ended`];
   -//! and the decisions [`exited`], [`collected`] and [`failed_to`].
   +//! the decisions [`exited`], [`collected`] and [`failed_to`]; and [`perform`], which takes one
   +//! task through the ports in that fixed order.
    //!
   -//! Non-responsibilities: running the action and storing what it left, which arrive through
   -//! ports; and recording the result, which happens on the thread that schedules the build.
   +//! Non-responsibilities: how an action is run and where what it left is stored, which are the
   +//! ports'; and recording the result, which happens on the thread that schedules the build.

   -use crate::error::Error;
   +use core::sync::atomic::{AtomicBool, Ordering};
   +
   +use crate::error::{Error, Result};
   +use crate::ports::{Clock, ContentStore, ExecStrategy};
    use crate::types::{
   -    ActionKey, Diagnostic, Digest, Exit, InfraKind, NodeId, OutcomeClass, OutputName, ReadyAction,
   -    Timestamp,
   +    ActionKey, Diagnostic, Digest, Exit, InfraKind, NodeId, OutcomeClass, OutputName, RawOutcome,
   +    ReadyAction, Timestamp,
    };

    /// One action to run, as it is handed to a worker.
   @@ -168,6 +172,72 @@ pub(crate) fn collected(
            )
    }

   +/// Runs one task on the thread it is called from, and returns what came of it.
   +///
   +/// A task handed over after `stop` was set is not begun: the clock is not read and no port is
   +/// asked anything. Otherwise the clock is read, the action is taken through `strategy` and
   +/// `cas`, and the clock is read again, however the action ended: an action a worker began
   +/// always carries both instants.
   +pub(crate) fn perform<S, C, K>(
   +    task: Task,
   +    strategy: &S,
   +    cas: &C,
   +    clock: &K,
   +    stop: &AtomicBool,
   +) -> Worked
   +where
   +    S: ExecStrategy,
   +    C: ContentStore,
   +    K: Clock,
   +{
   +    if stop.load(Ordering::SeqCst) {
   +        return Worked::NotBegun(task.node);
   +    }
   +    let started = clock.now();
   +    let ended = run(&task, strategy, cas);
   +    let finished = clock.now();
   +    Worked::Performed(Box::new(Performed {
   +        node: task.node,
   +        key: task.key,
   +        span: Span { started, finished },
   +        ended,
   +    }))
   +}
   +
   +/// Takes the action of `task` through its steps, and says how it ended at the first step that
   +/// decides it: a directory to run in, the run, storing what it printed, its exit, and storing
   +/// what it left.
   +fn run<S: ExecStrategy, C: ContentStore>(task: &Task, strategy: &S, cas: &C) -> Ended {
   +    let action = &task.action;
   +    let dir = match strategy.prepare(action) {
   +        Ok(dir) => dir,
   +        Err(error) => return failed_to(InfraKind::Prepare, &error, None),
   +    };
   +    let raw = match strategy.run(&dir, action) {
   +        Ok(raw) => raw,
   +        Err(error) => return failed_to(InfraKind::Run, &error, None),
   +    };
   +    let printed = match stored(&raw, cas) {
   +        Ok(printed) => printed,
   +        Err(error) => return failed_to(InfraKind::Store, &error, None),
   +    };
   +    if let Some(ended) = exited(raw.exit, printed) {
   +        return ended;
   +    }
   +    match strategy.collect(&dir, action, cas) {
   +        Ok(entries) => collected(&task.key.components().own.outputs, &entries, printed),
   +        Err(error) => failed_to(InfraKind::Store, &error, Some(printed)),
   +    }
   +}
   +
   +/// Stores what an action printed, and returns the two digests.
   +fn stored<C: ContentStore>(raw: &RawOutcome, cas: &C) -> Result<Printed> {
   +    Ok(Printed {
   +        stdout: cas.put(&raw.stdout)?,
   +        stderr: cas.put(&raw.stderr)?,
   +    })
   +}
   +
    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;
   ```

   `perform` is what the dispatcher runs on a worker. The stop flag is read first, so a task handed over before the build stopped and reached after it comes back `NotBegun`, with no clock read and no port asked. For a task that is begun, `run` tries the steps top to bottom and the first that decides ends it; `finished` is read after `run` returns, whichever step that was, so every action a worker began carries both instants. The digests of what the action printed exist only once both are stored, which is why a failure to store them carries none.

5. Ask the cache and the store. In `crates/buildl-core/src/execute/settle.rs`, update the module doc and the imports, and add `Settled` and `settle` directly above `#[cfg(test)]`. As a diff:

   ```diff
   --- a/crates/buildl-core/src/execute/settle.rs
   +++ b/crates/buildl-core/src/execute/settle.rs
   @@ -5,12 +5,14 @@
    //! finished: a key holds what each dependency produced, and that is known only then. So the
    //! key is finished here, during the build, and compared with the recorded one.
    //!
   -//! Responsibilities: [`finished_key`] and [`is_cached`].
   +//! Responsibilities: [`finished_key`], [`is_cached`], and [`settle`] with [`Settled`], which
   +//! asks the ports what the two decisions need.
    //!
   -//! Non-responsibilities: reading a row, and asking whether content is still stored. Both
   -//! arrive through ports; the decisions here are over the values those ports returned.
   +//! Non-responsibilities: keeping rows and content, which are the ports'; and what happens to a
   +//! target once it is settled.

    use crate::error::{Error, PortName, Result};
   +use crate::ports::{ActionCache, ContentStore};
    use crate::types::{
        ActionKey, CacheRow, DepProducts, Diagnostic, Digest, Freshness, KeyComponents, Label,
        OwnComponents,
   @@ -68,6 +70,42 @@ pub(crate) fn is_cached(
            })
    }

   +/// What settling a target that may start found.
   +#[derive(Debug)]
   +pub(crate) enum Settled {
   +    /// It need not run: what its last build recorded, which is this row, still stands.
   +    Cached(CacheRow),
   +    /// It must run, under this key.
   +    Runs(ActionKey),
   +}
   +
   +/// Settles the target `label`, whose dependencies `deps` have all succeeded.
   +///
   +/// Its key is finished from `own` and the row `cache` holds for each dependency, in the order
   +/// the dependencies were written. It is cached when [`is_cached`] says so of that key, the
   +/// row `cache` holds for the target itself, and what `cas` still holds.
   +///
   +/// # Errors
   +///
   +/// Returns whatever [`finished_key`] returns.
   +pub(crate) fn settle<A: ActionCache, C: ContentStore>(
   +    label: &Label,
   +    freshness: Freshness,
   +    own: &OwnComponents,
   +    deps: &[&Label],
   +    cache: &A,
   +    cas: &C,
   +) -> Result<Settled> {
   +    let rows: Vec<(&Label, Option<&CacheRow>)> =
   +        deps.iter().map(|dep| (*dep, cache.row(dep))).collect();
   +    let key = finished_key(own, &rows)?;
   +    let standing = cache
   +        .row(label)
   +        .filter(|row| is_cached(freshness, &key, Some(row), |digest| cas.contains(digest)))
   +        .cloned();
   +    Ok(standing.map_or_else(|| Settled::Runs(key), Settled::Cached))
   +}
   +
    #[cfg(test)]
    mod tests {
        #![expect(
   ```

   A cached target hands back a copy of its row: everything its outcome reports and every digest to put in place is in it, and the caller needs the cache again before the row would be let go.

6. Place, record and log. In `crates/buildl-core/src/execute/record.rs`, update the module doc and the imports, and add `store`, `restore`, `append` and the private `place` directly above the private function `succeeded`, the last one before `#[cfg(test)]`. As a diff:

   ```diff
   --- a/crates/buildl-core/src/execute/record.rs
   +++ b/crates/buildl-core/src/execute/record.rs
   @@ -6,16 +6,20 @@
    //! place, so that what a class of outcome carries is decided once.
    //!
    //! Responsibilities: [`row`]; the outcomes [`built`], [`cached`], [`failed`], [`unstored`],
   -//! [`unplaced`], [`mismatched`] and [`skipped`]; and [`event`] with [`Progress`].
   +//! [`unplaced`], [`mismatched`] and [`skipped`]; [`event`] with [`Progress`]; and the three
   +//! steps that reach a port, in the order that keeps a crash harmless: [`store`] puts a built
   +//! target's outputs in place and only then records its row, [`restore`] puts a cached
   +//! target's outputs in place, and [`append`] logs an outcome whatever it is.
    //!
   -//! Non-responsibilities: putting an output in place, storing a row and appending an event,
   -//! which arrive through ports; and the order those happen in.
   +//! Non-responsibilities: how content is put in place, how a row is kept and how the log is
   +//! written, which are the ports'.

   -use crate::error::Error;
   +use crate::error::{Error, Result};
    use crate::execute::perform::{Printed, Span};
   +use crate::ports::{ActionCache, Clock, ContentStore, EventLog};
    use crate::types::{
        ActionKey, ActionOutcome, CacheRow, Command, Diagnostic, Digest, InfraKind, Label,
   -    OutcomeClass, OutcomeEvent, Provenance, Timestamp,
   +    OutcomeClass, OutcomeEvent, OutputPath, Provenance, Timestamp,
    };

    /// The class of a target whose outputs, or whose row, could not be stored or put in place.
   @@ -146,6 +150,71 @@ pub(crate) fn event(
        }
    }

   +/// Records a target that ran under `key`, left `outputs` and printed `printed`, and returns
   +/// its outcome.
   +///
   +/// Each output is put at its path of `paths`, in the order the outputs were written, and only
   +/// then is the row stored: a row therefore exists only for content the store already holds.
   +/// A port that fails either step makes the outcome a failure to store, and no row is kept.
   +pub(crate) fn store<A: ActionCache, C: ContentStore>(
   +    label: Label,
   +    key: ActionKey,
   +    outputs: Vec<Digest>,
   +    printed: Printed,
   +    paths: &[OutputPath],
   +    cas: &C,
   +    cache: &mut A,
   +) -> ActionOutcome {
   +    let Some(row) = row(key, outputs, printed) else {
   +        return mismatched(label, printed);
   +    };
   +    let outcome = built(label.clone(), &row);
   +    let stored = place(&row, paths, cas).and_then(|()| cache.record(label.clone(), row));
   +    match stored {
   +        Ok(()) => outcome,
   +        Err(error) => unstored(label, &error, printed),
   +    }
   +}
   +
   +/// Puts the outputs of a target that need not run back in place, from what `row` recorded,
   +/// and returns its outcome. Nothing is written to the cache.
   +pub(crate) fn restore<C: ContentStore>(
   +    label: Label,
   +    row: &CacheRow,
   +    paths: &[OutputPath],
   +    cas: &C,
   +) -> ActionOutcome {
   +    match place(row, paths, cas) {
   +        Ok(()) => cached(label, row),
   +        Err(error) => unplaced(label, &error, row),
   +    }
   +}
   +
   +/// Appends the event for `outcome` to `log`, reading `clock` once for when it was recorded.
   +///
   +/// # Errors
   +///
   +/// Returns whatever `log` returns.
   +pub(crate) fn append<L: EventLog, K: Clock>(
   +    log: &mut L,
   +    clock: &K,
   +    provenance: &Provenance,
   +    command: &Command,
   +    progress: Progress,
   +    outcome: ActionOutcome,
   +) -> Result<()> {
   +    log.append(&event(clock.now(), provenance, command, progress, outcome))
   +}
   +
   +/// Puts each output `row` holds at its path of `paths`, pairwise, in the order the outputs
   +/// were written.
   +fn place<C: ContentStore>(row: &CacheRow, paths: &[OutputPath], cas: &C) -> Result<()> {
   +    row.produced()
   +        .iter()
   +        .zip(paths)
   +        .try_for_each(|(produced, at)| cas.place(&produced.digest, at))
   +}
   +
    /// The outcome of a target whose outputs are in place: `class`, with what `row` holds.
    fn succeeded(label: Label, class: OutcomeClass, row: &CacheRow) -> ActionOutcome {
        ActionOutcome {
   ```

   `store` is the one place the order "content first, row after" is written down: each output is placed, and only then is the row recorded. The outcome is built from the row before the row is given away. A port failing either step makes the target a failure to store and leaves no row; nothing here touches the cache for a target that was cached, failed or was skipped.

7. Declare the assembly, export the function, and drop every dead-code expectation. `crates/buildl-core/src/execute/mod.rs` becomes:

   ```rust
   //! Execute, the fourth phase: from a plan to a finished build.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //! Recording what an action produced is the last step of running it, so it lives here too.
   //!
   //! Responsibilities: [`execute`].
   //!
   //! Non-responsibilities: planning the build, which the phase before this one does; and running
   //! a command, storing content, keeping rows or writing the log, which are each a port's.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod admission;
   pub mod assembly;
   mod expand;
   mod layout;
   mod perform;
   mod record;
   mod schedule;
   mod settle;

   #[cfg(test)]
   mod fixtures;

   pub use assembly::execute;
   ```

8. Write the failing unit tests of the assembly's three helpers. Create `crates/buildl-core/src/execute/assembly.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::{BTreeMap, BTreeSet};

       use super::{made_from, own_of, ready};
       use crate::execute::fixtures::{command, graph, key, label, node, output_path, own, path};
       use crate::types::{
           DirtyReason, EnvName, EnvValue, NetworkAccess, NodeId, Plan, PlannedKey, Unfinished,
           Verdict, WantedSet,
       };

       /// A plan holding one clean verdict per label, in the order given.
       fn plan(labels: &[&str]) -> Plan {
           let verdicts = labels
               .iter()
               .map(|_| Verdict::Clean { key: key(&["o"]) })
               .collect();
           let wanted = WantedSet {
               programs: BTreeSet::new(),
               env: BTreeSet::new(),
               write: BTreeSet::new(),
               network: BTreeSet::new(),
           };
           Plan::new(
               labels.iter().map(|raw| label(raw)).collect(),
               verdicts,
               wanted,
           )
       }

       #[test]
       fn a_plan_is_made_from_a_graph_when_it_holds_its_targets_under_the_same_ids() {
           let graph = graph(
               vec![node("//:a", |_| {}), node("//:b", |_| {})],
               &[&[], &[]],
           );
           assert!(made_from(&plan(&["//:a", "//:b"]), &graph));
           assert!(!made_from(&plan(&["//:a"]), &graph), "a target short");
           assert!(
               !made_from(&plan(&["//:a", "//:b", "//:c"]), &graph),
               "a target more"
           );
           assert!(
               !made_from(&plan(&["//:a", "//:c"]), &graph),
               "another target"
           );
           assert!(
               !made_from(&plan(&["//:b", "//:a"]), &graph),
               "the same targets under other ids"
           );
       }

       #[test]
       fn every_verdict_yields_the_components_of_its_target() {
           let finished = key(&["app"]);
           let unfinished = Unfinished {
               own: own(&["lib"]),
               awaiting: vec![NodeId::new(0)],
           };
           let clean = Verdict::Clean {
               key: finished.clone(),
           };
           let dirty = Verdict::Dirty {
               reason: DirtyReason::NeverBuilt,
               key: PlannedKey::Finished(finished),
           };
           let waiting = Verdict::Dirty {
               reason: DirtyReason::NeverBuilt,
               key: PlannedKey::Unfinished(unfinished.clone()),
           };
           let conditional = Verdict::Conditional(unfinished);

           assert_eq!(own_of(&clean), &own(&["app"]));
           assert_eq!(own_of(&dirty), &own(&["app"]));
           assert_eq!(own_of(&waiting), &own(&["lib"]));
           assert_eq!(own_of(&conditional), &own(&["lib"]));
       }

       #[test]
       fn an_action_is_handed_its_paths_in_written_order_and_only_the_variables_that_are_set() {
           let node = node("//:app", |node| {
               node.inputs = vec![path("z.c"), path("a.c")];
               node.network = NetworkAccess::Declared;
           });
           let cc = EnvName::parse("CC").unwrap();
           let unset = EnvName::parse("CFLAGS").unwrap();
           let mut own = own(&["app"]);
           own.env = BTreeMap::from([
               (cc.clone(), Some(EnvValue::parse("clang").unwrap())),
               (unset, None),
           ]);
           let outputs = [output_path("//:app", "app")];
           let deps = vec![output_path("//:z", "z"), output_path("//:a", "a")];

           let action = ready(
               &node,
               &own,
               command(&["cc", "-o", "out/app"]),
               &outputs,
               deps.clone(),
           );

           assert_eq!(action.label, label("//:app"));
           assert_eq!(action.command, command(&["cc", "-o", "out/app"]));
           assert_eq!(
               action.env,
               BTreeMap::from([(cc, EnvValue::parse("clang").unwrap())])
           );
           assert_eq!(action.inputs, [path("z.c"), path("a.c")]);
           assert_eq!(action.deps, deps);
           assert_eq!(action.outputs, outputs);
           assert_eq!(action.network, NetworkAccess::Declared);
       }
   }
   ```

9. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::assembly
   error[E0432]: unresolved imports `super::made_from`, `super::own_of`, `super::ready`
   error[E0432]: unresolved import `assembly::execute`
   ```

10. Sequence the steps. Replace the placeholder line at the top of `crates/buildl-core/src/execute/assembly.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

    ```rust
    //! The assembly: from a plan, through the ports, to one outcome per target.
    //!
    //! Its own file because it is where this phase's steps meet and where their order is fixed.
    //! Everything that can refuse a build is checked before any of it runs: the plan belongs to the
    //! graph, no two outputs collide, and every command, once expanded, names a program the
    //! workspace allows. Only then are targets handed out to be built, each one settled when it may
    //! start and recorded when it is done.
    //!
    //! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
    //! build has stopped; and the bookkeeping between two results — which targets wait to be
    //! settled, which have their outcome, and the one error that fails the whole build.
    //!
    //! Non-responsibilities: what each step decides. Each has its own sibling file.

    use core::sync::atomic::{AtomicBool, Ordering};
    use std::collections::{BTreeMap, VecDeque};

    use crate::error::{Error, Result};
    use crate::execute::admission::admit;
    use crate::execute::expand::{Lists, expand};
    use crate::execute::layout::Layout;
    use crate::execute::perform::{Ended, Performed, Task, Worked, perform};
    use crate::execute::record::{Progress, append, failed, restore, skipped, store};
    use crate::execute::schedule::{Finish, Schedule};
    use crate::execute::settle::{Settled, settle};
    use crate::ports::{Dispatcher, Manifest, PortSet, Ports};
    use crate::types::{
        ActionOutcome, Ceiling, Command, EnvName, EnvValue, Label, Node, NodeId, OnFailure, OutputPath,
        OwnComponents, Plan, PlannedKey, ReadyAction, TargetGraph, Verdict,
    };

    /// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
    /// order.
    ///
    /// `plan` must be the plan made from `graph`. A target starts once everything it depends on
    /// has succeeded. Its key is then finished from what those dependencies produced: when its
    /// last build recorded that key and everything it produced is still stored, the target is not
    /// run and its outputs are put back in place; otherwise its action runs. What an action
    /// produced is stored first, its row recorded after, and its outcome logged whatever it is, so
    /// a row never names content that is not held and a failure is never recorded as a result.
    ///
    /// An action that fails is an outcome, not an error. What a failure does to the rest of the
    /// build is `on_failure`'s to say: under [`OnFailure::Stop`] nothing more is begun, and under
    /// [`OnFailure::KeepGoing`] only the targets that depend on the failed one are skipped.
    ///
    /// # Errors
    ///
    /// Before anything runs, returns the first failure met, in this order:
    ///
    /// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
    /// 2. whatever the manifest port returns;
    /// 3. [`Error::OutputConflict`] when two outputs would be written to one place;
    /// 4. for the first target, in id order, whose expanded command cannot run:
    ///    [`Error::NoProgram`] when it names no program, and [`Error::Refused`] when the program
    ///    is outside the workspace's ceiling.
    ///
    /// Once the build is under way, returns the first of: the log port's failure to append an
    /// event; [`Error::CanonicalJson`] for a key that cannot be serialized; and
    /// [`Error::Adapter`], naming the action cache, for a dependency that succeeded and has no
    /// row. After such an error no port is asked anything more. Otherwise returns whatever the
    /// dispatcher port itself returns.
    pub fn execute<P: Ports>(
        graph: &TargetGraph,
        plan: &Plan,
        on_failure: OnFailure,
        ports: &mut PortSet<P>,
    ) -> Result<Vec<ActionOutcome>> {
        if !made_from(plan, graph) {
            return Err(Error::ForeignPlan);
        }
        let workspace = ports.manifest.workspace()?;
        let layout = Layout::of(graph, &workspace.out)?;
        let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;

        let PortSet {
            cache,
            cas,
            log,
            strategy,
            dispatcher,
            clock,
            ..
        } = ports;
        let (cas, strategy, clock) = (&*cas, &*strategy, &*clock);
        let stop = AtomicBool::new(false);
        let mut build = Build::<P> {
            graph,
            targets,
            layout: &layout,
            schedule: Schedule::new(graph, on_failure),
            outcomes: vec![None; graph.len()],
            queue: VecDeque::new(),
            fatal: None,
            cache,
            cas,
            log,
            clock,
            stop: &stop,
        };
        let dispatched = dispatcher.dispatch(
            |task| perform(task, strategy, cas, clock, &stop),
            |worked| build.step(worked),
        );
        build.outcomes(dispatched)
    }

    /// Whether `plan` holds exactly the targets of `graph`, under the same ids.
    fn made_from(plan: &Plan, graph: &TargetGraph) -> bool {
        plan.len() == graph.len()
            && graph
                .nodes()
                .iter()
                .zip((0_u32..).map(NodeId::new))
                .all(|(node, id)| plan.label(id) == Some(&node.label))
    }

    /// The components a verdict holds for its target, whatever the verdict says.
    const fn own_of(verdict: &Verdict) -> &OwnComponents {
        match verdict {
            Verdict::Clean { key }
            | Verdict::Dirty {
                key: PlannedKey::Finished(key),
                ..
            } => &key.components().own,
            Verdict::Dirty {
                key: PlannedKey::Unfinished(unfinished),
                ..
            }
            | Verdict::Conditional(unfinished) => &unfinished.own,
        }
    }

    /// The action of `node` as a strategy is handed it: `command` is its expanded command,
    /// `outputs` where its own outputs live and `deps` where its dependencies' do.
    ///
    /// The environment holds each variable the target reads that has a value; one that is not set
    /// is left out, as it is absent where the action runs.
    fn ready(
        node: &Node,
        own: &OwnComponents,
        command: Command,
        outputs: &[OutputPath],
        deps: Vec<OutputPath>,
    ) -> ReadyAction {
        let env: BTreeMap<EnvName, EnvValue> = own
            .env
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), value.clone()?)))
            .collect();
        ReadyAction {
            label: node.label.clone(),
            command,
            env,
            inputs: node.inputs.clone(),
            deps,
            outputs: outputs.to_vec(),
            network: node.network,
        }
    }

    /// One target, with everything about it that is settled before anything runs.
    struct Prepared<'a> {
        /// The target as the graph holds it.
        node: &'a Node,
        /// Its own key components, from its verdict.
        own: &'a OwnComponents,
        /// The labels of the targets it depends on, in the order they were written.
        deps: Vec<&'a Label>,
        /// Its action, ready to hand to a strategy.
        action: ReadyAction,
    }

    /// Every target of `graph` prepared, in id order: its command expanded and admitted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoProgram`] or [`Error::Refused`] for the first target, in id order, whose
    /// expanded command cannot run under `ceiling`.
    fn prepared<'a>(
        graph: &'a TargetGraph,
        plan: &'a Plan,
        layout: &Layout,
        ceiling: &Ceiling,
    ) -> Result<Vec<Prepared<'a>>> {
        graph
            .nodes()
            .iter()
            .zip(plan.verdicts())
            .zip((0_u32..).map(NodeId::new))
            .map(|((node, verdict), id)| {
                let own = own_of(verdict);
                let outputs = layout.outputs(id);
                let dep_paths = layout.deps(graph, id);
                let lists = Lists {
                    inputs: &node.inputs,
                    outputs,
                    deps: &dep_paths,
                };
                let command = admit(expand(&node.run, &own.settings, &lists), node, ceiling)?;
                let deps = graph
                    .deps(id)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|dep| graph.node(*dep))
                    .map(|dep| &dep.label)
                    .collect();
                Ok(Prepared {
                    node,
                    own,
                    deps,
                    action: ready(node, own, command, outputs, dep_paths),
                })
            })
            .collect()
    }

    /// One build under way: everything the calling thread keeps between two results.
    struct Build<'a, P: Ports> {
        /// The graph being built.
        graph: &'a TargetGraph,
        /// Every target, in id order.
        targets: Vec<Prepared<'a>>,
        /// Where every output lives.
        layout: &'a Layout,
        /// Which targets may start, and which never will.
        schedule: Schedule,
        /// One slot per target, in id order, filled when the target has its outcome.
        outcomes: Vec<Option<ActionOutcome>>,
        /// The started targets waiting to be settled, oldest first.
        queue: VecDeque<NodeId>,
        /// The error that fails the build, once there is one.
        fatal: Option<Error>,
        /// The rows of the last build and of this one.
        cache: &'a mut P::Cache,
        /// The content store, shared with the workers.
        cas: &'a P::Cas,
        /// The build log.
        log: &'a mut P::Log,
        /// The clock, shared with the workers.
        clock: &'a P::Clock,
        /// Set once nothing more may begin; the workers read it.
        stop: &'a AtomicBool,
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
            };
            match stepped {
                Ok(()) => tasks,
                Err(error) => {
                    self.fatal = Some(error);
                    self.stop.store(true, Ordering::SeqCst);
                    Vec::new()
                }
            }
        }

        /// Settles every started target waiting in the queue, oldest first: one whose recorded
        /// result still stands is finished at once, which may start others that join the queue;
        /// one that must run becomes a task. Once the build has stopped, what waits is abandoned.
        fn drain(&mut self, tasks: &mut Vec<Task>) -> Result<()> {
            while let Some(node) = self.queue.pop_front() {
                if self.schedule.stopped().is_some() {
                    self.abandon(node)?;
                    continue;
                }
                let Some(target) = self.targets.get(node.index()) else {
                    continue;
                };
                let settled = settle(
                    &target.node.label,
                    target.node.freshness,
                    target.own,
                    &target.deps,
                    &*self.cache,
                    self.cas,
                )?;
                match settled {
                    Settled::Runs(key) => tasks.push(Task {
                        node,
                        key,
                        action: target.action.clone(),
                    }),
                    Settled::Cached(row) => {
                        let outcome = restore(
                            target.node.label.clone(),
                            &row,
                            self.layout.outputs(node),
                            self.cas,
                        );
                        let progress = Progress::Keyed {
                            key: row.key().digest(),
                        };
                        self.finish(node, progress, outcome)?;
                    }
                }
            }
            Ok(())
        }

        /// Records what a worker did for one target, and finishes the target.
        fn record(&mut self, performed: Performed) -> Result<()> {
            let node = performed.node;
            let Some(target) = self.targets.get(node.index()) else {
                return Ok(());
            };
            let label = target.node.label.clone();
            let progress = Progress::Began {
                key: performed.key.digest(),
                span: performed.span,
            };
            let outcome = match performed.ended {
                Ended::Produced { outputs, printed } => store(
                    label,
                    performed.key,
                    outputs,
                    printed,
                    self.layout.outputs(node),
                    self.cas,
                    self.cache,
                ),
                Ended::Failed {
                    class,
                    diagnostic,
                    printed,
                } => failed(label, class, diagnostic, printed),
            };
            self.finish(node, progress, outcome)
        }

        /// Gives `node` its outcome, and tells the schedule how it finished: every target that
        /// can no longer be built is given its outcome too, and every target that may start now
        /// joins the queue. When the build has stopped, the workers are told to begin nothing.
        fn finish(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
            let finish = if outcome.class.succeeded() {
                Finish::Succeeded
            } else {
                Finish::Failed
            };
            self.conclude(node, progress, outcome)?;
            let effects = self.schedule.complete(self.graph, node, finish);
            for skipped_node in effects.skipped {
                self.skip(skipped_node, node)?;
            }
            if self.schedule.stopped().is_some() {
                self.stop.store(true, Ordering::SeqCst);
            }
            self.queue.extend(effects.started);
            Ok(())
        }

        /// Gives up on the started target `node` because the build has stopped: it is skipped,
        /// naming the failure that stopped the build. A build that has not stopped abandons
        /// nothing.
        fn abandon(&mut self, node: NodeId) -> Result<()> {
            let Some(failed) = self.schedule.stopped() else {
                return Ok(());
            };
            self.schedule.abandon(node);
            self.skip(node, failed)
        }

        /// Gives `node` the outcome of a target never begun because the target `because` failed.
        fn skip(&mut self, node: NodeId, because: NodeId) -> Result<()> {
            let (Some(target), Some(cause)) = (
                self.targets.get(node.index()),
                self.targets.get(because.index()),
            ) else {
                return Ok(());
            };
            let outcome = skipped(target.node.label.clone(), cause.node.label.clone());
            self.conclude(node, Progress::Skipped, outcome)
        }

        /// Keeps `outcome` as the outcome of `node`, and appends its event to the log.
        fn conclude(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
            let (Some(target), Some(slot)) = (
                self.targets.get(node.index()),
                self.outcomes.get_mut(node.index()),
            ) else {
                return Ok(());
            };
            *slot = Some(outcome.clone());
            append(
                self.log,
                self.clock,
                &target.node.provenance,
                &target.action.command,
                progress,
                outcome,
            )
        }

        /// What the build comes to, once the dispatcher has returned with `dispatched`: the error
        /// that failed it, or the dispatcher's own, or else every target's outcome in id order.
        fn outcomes(self, dispatched: Result<()>) -> Result<Vec<ActionOutcome>> {
            if let Some(error) = self.fatal {
                return Err(error);
            }
            dispatched?;
            Ok(self.outcomes.into_iter().flatten().collect())
        }
    }
    ```

    How the pieces meet:

    - **Before anything runs.** `made_from` is the first check, so a plan that does not belong to the graph is `ForeignPlan` before a port is asked. Then the manifest, then `Layout::of`, then `prepared`, which expands and admits every target's command in id order, whatever its verdict, and builds its `ReadyAction`. None of these four asks the cache, the store, the log, the strategy, the dispatcher or the clock.
    - **Borrowing the ports.** `PortSet` is taken apart field by field. The store, the strategy and the clock are reborrowed as shared references, because `work` uses them from other threads; the cache and the log stay exclusive and are reached only by `step`. The stop flag is a local `AtomicBool` both closures borrow.
    - **`step`.** `None` starts the build: every target that waits on nothing joins the queue, and `drain` settles the queue. A worker's result is recorded, its target finished, and the queue drained again. A task that came back not begun is abandoned. Whatever `step` returns `Err` is kept as the build's error, the stop flag is set, and from then on a turn does nothing.
    - **`drain`.** Takes the oldest started target. Once the build has stopped it is abandoned. Otherwise `settle` decides: a target that must run becomes a task; a cached one has its outputs put back by `restore` and is finished at once, which may start further targets that join the same queue.
    - **`finish`.** Keeps the outcome and logs it, tells the schedule, gives every target the schedule skipped its outcome naming the failed one, sets the stop flag when the build has stopped, and queues what may start now. Every target reaches its outcome through here or through `skip`, and every outcome through `conclude`, which is the only caller of `append`.
    - **The end.** When `dispatch` returns, the error that failed the build wins; then the dispatcher's own; otherwise the outcomes, one per target in id order.

11. Re-export the function. In `crates/buildl-core/src/lib.rs`:

    ```diff
    --- a/crates/buildl-core/src/lib.rs
    +++ b/crates/buildl-core/src/lib.rs
    @@ -80,6 +80,7 @@ pub use error::{
         ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
         EvaluationLimit, NameKind, PortName, Result,
     };
    +pub use execute::execute;
     pub use load::load;
     pub use pipeline::Pipeline;
     pub use plan::plan;
    ```

12. Add the command. In `crates/buildl-core/src/pipeline/driver.rs`, update the module doc and the imports, and add `build` to `impl<P: Ports> Pipeline<P>`, directly below `plan`. As a diff:

    ```diff
    --- a/crates/buildl-core/src/pipeline/driver.rs
    +++ b/crates/buildl-core/src/pipeline/driver.rs
    @@ -1,10 +1,11 @@
     //! The pipeline's driver: one bundle of ports, and one method per command.
     //!
    -//! Its own file because the driver owns the sequence of phases, and the sequence grows by one
    -//! method per command as later phases arrive. Today it runs Load, Resolve and Plan.
    +//! Its own file because the driver owns the sequence of phases: one method per command, each
    +//! running the phases up to the one the command stops at. It runs Load, Resolve, Plan and
    +//! Execute.
     //!
    -//! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`] and
    -//! [`Pipeline::plan`].
    +//! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`],
    +//! [`Pipeline::plan`] and [`Pipeline::build`].
     //!
     //! Non-responsibilities: the phases' own logic, which each phase's module holds.

    @@ -13,11 +14,14 @@ use core::fmt;
     use std::collections::BTreeMap;

     use crate::error::{Error, Result};
    +use crate::execute::execute;
     use crate::load::{declaration_order, load};
     use crate::plan::plan;
     use crate::ports::{PortSet, Ports};
     use crate::resolve::resolve;
    -use crate::types::{Declaration, EntryName, Plan, SettingName, SettingValue, TargetGraph};
    +use crate::types::{
    +    ActionOutcome, Declaration, EntryName, OnFailure, Plan, SettingName, SettingValue, TargetGraph,
    +};

     /// The pipeline over one chosen implementation of each port.
     pub struct Pipeline<P: Ports> {
    @@ -102,6 +106,29 @@ impl<P: Ports> Pipeline<P> {
             let graph = resolve(load(&self.ports.source, entry)?)?;
             plan(&graph, overrides, &mut self.ports)
         }
    +
    +    /// Loads the workspace once, resolves it, plans a build of the graph, and builds it.
    +    ///
    +    /// `overrides` gives values to build settings for this run, and `on_failure` says what a
    +    /// failed action does to the rest of the build. The outcomes are one per target, in id
    +    /// order; a target that failed or was skipped is an outcome, not an error.
    +    ///
    +    /// # Errors
    +    ///
    +    /// Returns whatever the load returns, then whatever [`resolve`] returns, then whatever
    +    /// [`plan`](fn@plan) returns, and otherwise whatever [`execute`](fn@execute) returns: two
    +    /// outputs that would be written to one place, a command that names no program or one
    +    /// outside the workspace's ceiling once it is expanded, or a port's own failure.
    +    pub fn build(
    +        &mut self,
    +        entry: &EntryName,
    +        overrides: &BTreeMap<SettingName, SettingValue>,
    +        on_failure: OnFailure,
    +    ) -> Result<Vec<ActionOutcome>> {
    +        let graph = resolve(load(&self.ports.source, entry)?)?;
    +        let plan = plan(&graph, overrides, &mut self.ports)?;
    +        execute(&graph, &plan, on_failure, &mut self.ports)
    +    }
     }

     /// The error naming the first place two sorted declaration lists differ, if they do.
    ```

13. Record the two new module edges. `crates/expected-module-edges.txt` becomes:

    ```
    error -> types
    execute -> error
    execute -> ports
    execute -> types
    json -> error
    load -> error
    load -> ports
    load -> types
    pipeline -> error
    pipeline -> execute
    pipeline -> load
    pipeline -> plan
    pipeline -> ports
    pipeline -> resolve
    pipeline -> types
    plan -> error
    plan -> ports
    plan -> types
    ports -> error
    ports -> types
    resolve -> error
    resolve -> types
    types -> error
    types -> json
    ```

14. Run the unit tests and the flows and confirm they pass:

    ```
    $ cargo test -p buildl-core --lib execute::assembly
    test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 419 filtered out; finished in 0.00s
    $ cargo test -p buildl-core --test flows build::
    running 2 tests
    test build::a_workspace_never_built_builds_every_target_after_what_it_depends_on ... ok
    test build::a_second_build_with_nothing_changed_runs_nothing_and_places_every_output_again ... ok

    test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
    ```

15. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

16. Commit:

    ```
    $ git add crates/buildl-core/src/execute crates/buildl-core/src/lib.rs crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/build.rs crates/buildl-core/tests/flows/main.rs crates/expected-module-edges.txt
    $ git commit -m "feat(buildl-core): build a workspace through the ports"
    ```

### Task 2 — Pin what is reused from the last build

**Files:**
- Modify `crates/buildl-core/tests/flows/build.rs`

**Steps:**

1. Widen the imports of `crates/buildl-core/tests/flows/build.rs`. Replace:

   ```rust
   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, FileStat, Label, OnFailure, OutcomeClass, OutcomeEvent,
       OutputPath, Pipeline, SettingName, SettingValue, SourcePath, Summary, Timestamp, Written,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, dir, file, ports, program, target_with, workspace,
   };
   ```

   With:

   ```rust
   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, FileStat, Freshness, InfraKind, Label, OnFailure,
       OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, SettingName, SettingValue,
       SourcePath, StagedItem, Summary, Timestamp, Written,
   };

   use crate::common::{
       Content, FakeContent, FakePorts, FakeSource, Script, dir, file, ports, program, target,
       target_with, workspace,
   };
   ```

2. Add five helpers and five flows at the end of the file:

   ```rust
   /// Where the output `name` of the target `raw` lives in a workspace whose outputs go under `out`.
   fn out(raw: &str, name: &str) -> OutputPath {
       OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
   }

   /// The outcome of the target `raw` names.
   fn outcome<'o>(outcomes: &'o [ActionOutcome], raw: &str) -> &'o ActionOutcome {
       let wanted = label(raw);
       outcomes
           .iter()
           .find(|outcome| outcome.label == wanted)
           .unwrap()
   }

   /// Every target's class, by label.
   fn classes(outcomes: &[ActionOutcome]) -> BTreeMap<String, OutcomeClass> {
       outcomes
           .iter()
           .map(|outcome| (outcome.label.to_string(), outcome.class.clone()))
           .collect()
   }

   /// The classes a test expects, by label.
   fn expected(pairs: &[(&str, OutcomeClass)]) -> BTreeMap<String, OutcomeClass> {
       pairs
           .iter()
           .map(|(raw, class)| ((*raw).to_owned(), class.clone()))
           .collect()
   }

   /// A pipeline over one root build file making `items`, with `cc` findable.
   fn root_only(items: Vec<StagedItem>) -> Pipeline<FakePorts> {
       let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
       let mut pipeline = Pipeline::new(ports(source));
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       pipeline
   }

   #[test]
   fn an_edited_input_rebuilds_its_target_and_what_depends_on_it_and_nothing_else() {
       let mut pipeline = chain();
       build(&mut pipeline, OnFailure::Stop);
       put(&mut pipeline, "lib.c", "lib.c, edited", 200);

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Built),
               ("//:docs", OutcomeClass::Cached),
               ("//:gen", OutcomeClass::Cached),
               ("//:lib", OutcomeClass::Built),
           ])
       );
       let ran = pipeline.ports().strategy.ran();
       assert_eq!(
           ran.get(4..),
           Some(&[label("//:lib"), label("//:app")][..]),
           "the second build ran the edited target, then the one that depends on it"
       );
   }

   #[test]
   fn a_target_rebuilt_to_the_same_output_leaves_what_depends_on_it_cached() {
       let mut pipeline = chain();
       let same = Script {
           outputs: BTreeMap::from([(out("//:gen", "gen"), Content::Holds(b"generated".to_vec()))]),
           ..Script::default()
       };
       pipeline
           .ports_mut()
           .strategy
           .scripts
           .insert(label("//:gen"), same);
       build(&mut pipeline, OnFailure::Stop);
       put(&mut pipeline, "gen.c", "gen.c, edited", 200);

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Cached),
               ("//:docs", OutcomeClass::Cached),
               ("//:gen", OutcomeClass::Built),
               ("//:lib", OutcomeClass::Cached),
           ])
       );
       let strategy = &pipeline.ports().strategy;
       assert_eq!(strategy.runs(&label("//:gen")), 2);
       assert_eq!(strategy.runs(&label("//:lib")), 1, "early cutoff");
       assert_eq!(strategy.runs(&label("//:app")), 1, "early cutoff");
   }

   #[test]
   fn a_target_whose_output_left_the_store_runs_again_and_the_others_stay_cached() {
       let mut pipeline = chain();
       let first = build(&mut pipeline, OnFailure::Stop);
       let lost = outcome(&first, "//:lib").outputs.first().unwrap().digest;
       pipeline.ports().cas.forget(&lost);

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Cached),
               ("//:docs", OutcomeClass::Cached),
               ("//:gen", OutcomeClass::Cached),
               ("//:lib", OutcomeClass::Built),
           ])
       );
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.runs(&label("//:lib")), 2);
       assert_eq!(ports.strategy.ran().len(), 5, "no other action ran again");
       assert!(
           ports.cas.blob(&lost).is_some(),
           "the output is stored again"
       );
   }

   #[test]
   fn a_cached_target_whose_output_cannot_be_placed_fails_and_keeps_its_row() {
       let mut pipeline = chain();
       build(&mut pipeline, OnFailure::KeepGoing);
       let rows = pipeline.ports().cache.rows.clone();
       pipeline.ports().cas.fail_place(out("//:docs", "docs"));

       let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

       let store = OutcomeClass::InfraFailure {
           kind: InfraKind::Store,
       };
       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Cached),
               ("//:docs", store),
               ("//:gen", OutcomeClass::Cached),
               ("//:lib", OutcomeClass::Cached),
           ])
       );
       let row = rows.get(&label("//:docs")).unwrap();
       let docs = outcome(&outcomes, "//:docs");
       assert_eq!(
           docs.diagnostic.as_ref().map(ToString::to_string).as_deref(),
           Some("content store: out/docs cannot be written")
       );
       assert_eq!(docs.outputs, []);
       assert_eq!(
           (docs.stdout, docs.stderr),
           (Some(row.stdout()), Some(row.stderr())),
           "what it printed is taken from its row"
       );
       let logged = event(&pipeline, "//:docs");
       assert_eq!(logged.key, Some(row.key().digest()));
       assert_eq!((logged.started, logged.finished), (None, None));
       let ports = pipeline.ports();
       assert_eq!(ports.cache.rows, rows, "its row is unchanged");
       assert_eq!(ports.strategy.runs(&label("//:docs")), 1, "it did not run");
   }

   #[test]
   fn a_target_that_always_runs_runs_on_every_build_and_its_dependent_stays_cached() {
       let mut pipeline = root_only(vec![
           target_with("gen", |target| target.freshness = Freshness::Always),
           target("lib", &["gen"]),
       ]);

       let first = build(&mut pipeline, OnFailure::Stop);
       let second = build(&mut pipeline, OnFailure::Stop);

       let both = [
           ("//:gen", OutcomeClass::Built),
           ("//:lib", OutcomeClass::Built),
       ];
       assert_eq!(classes(&first), expected(&both));
       assert_eq!(
           classes(&second),
           expected(&[
               ("//:gen", OutcomeClass::Built),
               ("//:lib", OutcomeClass::Cached)
           ])
       );
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.runs(&label("//:gen")), 2);
       assert_eq!(ports.strategy.runs(&label("//:lib")), 1);
       assert_eq!(
           ports.cache.recorded(),
           [label("//:gen"), label("//:lib"), label("//:gen")],
           "a target that always runs records a row like any other"
       );
   }
   ```

   The five flows, by what they pin:

   - An edited input rebuilds its target and the target that depends on it; the target it depends on and an unrelated one are cached. The edit is to `lib.c`, in the middle of the chain, for the reason given under the gate facts.
   - Early cutoff: `//:gen` is scripted to leave the same bytes whatever it reads, so after its input is edited it runs and everything downstream stays cached, without running.
   - A target whose output the store no longer holds runs again although its key is unchanged, and the others stay cached.
   - A cached target whose output cannot be put in place is a failure to store; what it printed is taken from its row, its row is unchanged, and it did not run.
   - A target declared to run always runs on both builds and records a row each time; its dependent is cached the second time.

3. Run the flows. The five new ones pass at once, because task 1 built the behaviour they pin:

   ```
   $ cargo test -p buildl-core --test flows build::
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
   ```

4. Confirm such a flow can fail. In `crates/buildl-core/src/execute/settle.rs`, in `settle`, replace `|digest| cas.contains(digest)` with `|_| true`, so that a row counts as standing whether or not its content is still stored, and run the flow that pins the rule:

   ```
   $ cargo test -p buildl-core --test flows build::a_target_whose_output_left
   test build::a_target_whose_output_left_the_store_runs_again_and_the_others_stay_cached ... FAILED

   assertion `left == right` failed
     left: {"//:app": Skipped { because: Label { directory: Directory(""), name: TargetName("lib") } }, "//:docs": Cached, "//:gen": Cached, "//:lib": InfraFailure { kind: Store }}
    right: {"//:app": Cached, "//:docs": Cached, "//:gen": Cached, "//:lib": Built}

   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 62 filtered out; finished in 0.01s
   ```

   The target is taken as cached, its output cannot be put in place, and its dependent is skipped. Put `|digest| cas.contains(digest)` back.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/build.rs
   $ git commit -m "test(buildl-core): pin what a build reuses from the last one"
   ```

### Task 3 — Pin what a failure does

**Files:**
- Modify `crates/buildl-core/tests/flows/build.rs`

**Steps:**

1. Widen the imports of `crates/buildl-core/tests/flows/build.rs`. Replace:

   ```rust
   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, FileStat, Freshness, InfraKind, Label, OnFailure,
       OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, SettingName, SettingValue,
       SourcePath, StagedItem, Summary, Timestamp, Written,
   };

   use crate::common::{
       Content, FakeContent, FakePorts, FakeSource, Script, dir, file, ports, program, target,
       target_with, workspace,
   };
   ```

   With:

   ```rust
   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, FileStat, Freshness, InfraKind, Label, OnFailure,
       OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName, SettingName,
       SettingValue, SourcePath, StagedItem, Summary, Timestamp, Written,
   };

   use crate::common::{
       Content, Ends, FakeContent, FakePorts, FakeSource, Script, dir, file, ports, program, target,
       target_with, workspace,
   };
   ```

2. Add six helpers and six flows at the end of the file:

   ```rust
   /// The class of a target skipped because the target `raw` names failed.
   fn skipped(raw: &str) -> OutcomeClass {
       OutcomeClass::Skipped {
           because: label(raw),
       }
   }

   /// The class of a target the builder itself failed, at `kind`.
   const fn infra(kind: InfraKind) -> OutcomeClass {
       OutcomeClass::InfraFailure { kind }
   }

   /// A script in which the action goes as `ends` says and everything else is unscripted.
   fn ending(ends: Ends) -> Script {
       Script {
           ends,
           ..Script::default()
       }
   }

   /// Scripts the action of the target `raw` names.
   fn script(pipeline: &mut Pipeline<FakePorts>, raw: &str, script: Script) {
       pipeline
           .ports_mut()
           .strategy
           .scripts
           .insert(label(raw), script);
   }

   /// The message of an outcome, as text.
   fn diagnostic(outcome: &ActionOutcome) -> Option<String> {
       outcome.diagnostic.as_ref().map(ToString::to_string)
   }

   /// `//:a` exits with status 1. `//:b` needs it and `//:c` needs `//:b`; `//:x` and `//:y` stand
   /// apart.
   fn failing() -> Pipeline<FakePorts> {
       let mut pipeline = root_only(vec![
           target("a", &[]),
           target("b", &["a"]),
           target("c", &["b"]),
           target("x", &[]),
           target("y", &[]),
       ]);
       script(&mut pipeline, "//:a", ending(Ends::Exits(1)));
       pipeline
   }

   #[test]
   fn under_keep_going_a_failure_skips_only_what_depends_on_it() {
       let mut pipeline = failing();

       let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:a", OutcomeClass::ActionFailure { status: 1 }),
               ("//:b", skipped("//:a")),
               ("//:c", skipped("//:a")),
               ("//:x", OutcomeClass::Built),
               ("//:y", OutcomeClass::Built),
           ])
       );
       assert_eq!(
           Summary::of(&outcomes),
           Summary {
               failed: 1,
               skipped: 2,
               built: 2,
               cached: 0
           }
       );
       let ports = pipeline.ports();
       let recorded: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
       assert_eq!(recorded, ["//:x", "//:y"], "no row for the other three");
       assert_eq!(ports.log.events.len(), 5, "one event per target");
       assert_eq!(
           event(&pipeline, "//:b").key,
           None,
           "a skipped target has no key"
       );
   }

   #[test]
   fn under_stop_nothing_begins_after_the_first_failure() {
       let mut pipeline = failing();

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:a", OutcomeClass::ActionFailure { status: 1 }),
               ("//:b", skipped("//:a")),
               ("//:c", skipped("//:a")),
               ("//:x", skipped("//:a")),
               ("//:y", skipped("//:a")),
           ])
       );
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.ran(), [label("//:a")]);
       assert_eq!(
           ports.strategy.handed().len(),
           1,
           "the two already handed over came back not begun"
       );
       assert_eq!(
           ports.clock.reads(),
           7,
           "two reads by the worker of //:a and one per event"
       );
       assert_eq!(ports.cache.rows.len(), 0);
       assert_eq!(ports.log.events.len(), 5, "one event per target");
   }

   #[test]
   fn under_stop_a_cached_target_that_cannot_be_placed_stops_what_waits_behind_it() {
       let mut pipeline = root_only(vec![target("a", &[]), target("b", &[]), target("c", &[])]);
       build(&mut pipeline, OnFailure::Stop);
       pipeline.ports().cas.fail_place(out("//:a", "a"));

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:a", infra(InfraKind::Store)),
               ("//:b", skipped("//:a")),
               ("//:c", skipped("//:a")),
           ])
       );
       let ports = pipeline.ports();
       assert_eq!(
           ports.strategy.ran().len(),
           3,
           "the second build ran nothing"
       );
       assert_eq!(ports.strategy.handed().len(), 3);
   }

   #[test]
   fn a_row_that_cannot_be_recorded_fails_its_target_and_the_next_build_runs_it_again() {
       let mut pipeline = chain();
       pipeline.ports_mut().cache.failing.insert(label("//:gen"));

       let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", skipped("//:gen")),
               ("//:docs", OutcomeClass::Built),
               ("//:gen", infra(InfraKind::Store)),
               ("//:lib", skipped("//:gen")),
           ])
       );
       let gen_outcome = outcome(&outcomes, "//:gen");
       assert_eq!(
           diagnostic(gen_outcome).as_deref(),
           Some("action cache: //:gen cannot be recorded")
       );
       assert_eq!(gen_outcome.outputs, []);
       assert!(gen_outcome.stdout.is_some(), "what it printed was stored");
       {
           let ports = pipeline.ports();
           assert!(
               ports.cas.placed().contains_key(&out("//:gen", "gen")),
               "its output reached the store and its place before the row was tried"
           );
           assert!(!ports.cache.rows.contains_key(&label("//:gen")));
       }

       pipeline.ports_mut().cache.failing.clear();
       let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Built),
               ("//:docs", OutcomeClass::Cached),
               ("//:gen", OutcomeClass::Built),
               ("//:lib", OutcomeClass::Built),
           ])
       );
       assert_eq!(pipeline.ports().strategy.runs(&label("//:gen")), 2);
   }

   #[test]
   fn every_way_an_action_can_fail_is_a_class_of_its_own_and_none_is_recorded() {
       let names = [
           "collect", "exit", "missing", "prepare", "refused", "run", "short", "timeout",
       ];
       let mut pipeline = root_only(names.iter().map(|name| target(name, &[])).collect());
       script(&mut pipeline, "//:collect", ending(Ends::CollectFails));
       script(&mut pipeline, "//:exit", ending(Ends::Exits(3)));
       script(&mut pipeline, "//:prepare", ending(Ends::PrepareFails));
       script(&mut pipeline, "//:run", ending(Ends::RunFails));
       script(&mut pipeline, "//:short", ending(Ends::CollectShort));
       script(&mut pipeline, "//:timeout", ending(Ends::TimesOut));
       let leaving_nothing = Script {
           outputs: BTreeMap::from([(out("//:missing", "missing"), Content::Missing)]),
           ..Script::default()
       };
       script(&mut pipeline, "//:missing", leaving_nothing);
       let printing = Script {
           stdout: Some(b"cannot be kept".to_vec()),
           ..Script::default()
       };
       script(&mut pipeline, "//:refused", printing);
       pipeline.ports().cas.fail_put(b"cannot be kept".to_vec());

       let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

       let missing = OutcomeClass::OutputMissing {
           name: OutputName::parse("missing").unwrap(),
       };
       // The class, the message, and whether what the action printed was stored.
       let table = [
           (
               "//:collect",
               infra(InfraKind::Store),
               Some("exec strategy: //:collect: the outputs cannot be read"),
               true,
           ),
           (
               "//:exit",
               OutcomeClass::ActionFailure { status: 3 },
               None,
               true,
           ),
           ("//:missing", missing, None, true),
           (
               "//:prepare",
               infra(InfraKind::Prepare),
               Some("exec strategy: //:prepare: no directory to run in"),
               false,
           ),
           (
               "//:refused",
               infra(InfraKind::Store),
               Some("content store: the content cannot be stored"),
               false,
           ),
           (
               "//:run",
               infra(InfraKind::Run),
               Some("exec strategy: //:run: the command cannot be started"),
               false,
           ),
           (
               "//:short",
               infra(InfraKind::Store),
               Some("the strategy answered for 0 outputs of 1"),
               true,
           ),
           ("//:timeout", infra(InfraKind::TimedOut), None, true),
       ];
       assert_eq!(
           Summary::of(&outcomes),
           Summary {
               failed: 8,
               skipped: 0,
               built: 0,
               cached: 0
           }
       );
       let ports = pipeline.ports();
       assert_eq!(ports.cache.rows.len(), 0, "a failure is never recorded");
       for (raw, class, message, stored) in table {
           let outcome = outcome(&outcomes, raw);
           assert_eq!(outcome.class, class, "{raw}");
           assert_eq!(diagnostic(outcome).as_deref(), message, "{raw}");
           assert_eq!(outcome.outputs, [], "{raw}");
           assert_eq!(outcome.stdout.is_some(), stored, "{raw}");
           assert_eq!(outcome.stderr.is_some(), stored, "{raw}");
           if let Some(stdout) = outcome.stdout {
               let printed = format!("{raw}\n").into_bytes();
               assert_eq!(ports.cas.blob(&stdout), Some(printed), "{raw}");
           }
           let logged = event(&pipeline, raw);
           assert_eq!(&logged.outcome, outcome, "{raw}");
           assert!(logged.key.is_some(), "{raw}");
           assert!(
               logged.started.is_some() && logged.finished.is_some(),
               "a worker began {raw}"
           );
       }
   }

   #[test]
   fn a_log_that_cannot_be_written_fails_the_build_and_keeps_what_was_already_recorded() {
       let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
       pipeline.ports_mut().log.broken = true;

       let error = pipeline
           .build(&entry(), &Overrides::new(), OnFailure::KeepGoing)
           .unwrap_err();

       assert_eq!(error.to_string(), "event log: the log cannot be written");
       assert!(matches!(error, Error::Adapter { port, .. } if port == PortName::EventLog));
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.ran(), [label("//:a")]);
       assert_eq!(
           ports.strategy.handed().len(),
           1,
           "the second task came back not begun"
       );
       assert_eq!(
           ports.clock.reads(),
           3,
           "two reads by the worker of //:a, one for its event, none for //:b"
       );
       let recorded: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
       assert_eq!(
           recorded,
           ["//:a"],
           "finished work survives a build that dies"
       );
   }
   ```

   The six flows, by what they pin:

   - Under `KeepGoing`, `//:a` failing skips the two targets downstream of it, both naming `//:a`, and the two apart are built: one failed, two skipped, two built, and no row for the first three.
   - Under `Stop`, with the oldest task taken first, `//:a` runs and fails; the two apart had already been handed to the dispatcher and come back not begun. All four others are skipped naming `//:a`, the strategy was asked for one directory, and the clock was read seven times: twice by the worker of `//:a`, once per event.
   - Under `Stop`, a cached target whose output cannot be put in place stops the two started targets waiting behind it in the queue; the strategy runs nothing.
   - A row that cannot be recorded makes its target a failure to store although its output reached the store and its place; no row exists, its dependents are skipped, and the next build runs it again.
   - Each of the eight ways an action can fail on a worker is its own class, with the message a port gave where there is one, what the action printed where that was stored, both instants, and no row.
   - A log that cannot be written fails the build with the log's own error. The first target's row is kept, the second task comes back not begun, and the clock was read three times.

3. Run the flows. The six new ones pass at once, because task 1 built the behaviour they pin:

   ```
   $ cargo test -p buildl-core --test flows build::
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
   ```

4. Confirm such a flow can fail. In `crates/buildl-core/src/execute/assembly.rs`, in `finish`, in the line `self.stop.store(true, Ordering::SeqCst);` directly above `self.queue.extend(effects.started);`, write `false` for `true`, so that workers are never told the build has stopped, and run the flow that pins the stop:

   ```
   $ cargo test -p buildl-core --test flows build::under_stop_nothing
   test build::under_stop_nothing_begins_after_the_first_failure ... FAILED

   assertion `left == right` failed
     left: {"//:a": ActionFailure { status: 1 }, "//:b": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }, "//:c": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }, "//:x": Built, "//:y": Built}
    right: {"//:a": ActionFailure { status: 1 }, "//:b": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }, "//:c": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }, "//:x": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }, "//:y": Skipped { because: Label { directory: Directory(""), name: TargetName("a") } }}

   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 68 filtered out; finished in 0.01s
   ```

   The two targets already handed to the dispatcher are built where they should be skipped. Write `true` back.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/build.rs
   $ git commit -m "test(buildl-core): pin what a failure does to a build"
   ```

### Task 4 — Pin what is refused before anything runs

**Files:**
- Modify `crates/buildl-core/tests/flows/build.rs`

**Steps:**

1. Widen the imports of `crates/buildl-core/tests/flows/build.rs`. Replace:

   ```rust
   use std::collections::BTreeMap;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, FileStat, Freshness, InfraKind, Label, OnFailure,
       OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName, SettingName,
       SettingValue, SourcePath, StagedItem, Summary, Timestamp, Written,
   };

   use crate::common::{
       Content, Ends, FakeContent, FakePorts, FakeSource, Script, dir, file, ports, program, target,
       target_with, workspace,
   };
   ```

   With:

   ```rust
   use std::collections::BTreeMap;
   use std::path::Path;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileStat, Freshness, InfraKind,
       Label, OnFailure, OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName,
       SettingName, SettingValue, SourcePath, StagedItem, Summary, TargetRole, Timestamp, Want,
       Written,
   };

   use crate::common::{
       Content, Ends, FakeContent, FakeFile, FakePorts, FakeSource, Script, dir, file, ports, program,
       target, target_with, workspace,
   };
   ```

2. Add three helpers and five flows at the end of the file:

   ```rust
   /// Asserts that the build was refused before any of it ran: no action, nothing stored or placed,
   /// no row, no event, and the clock never read.
   fn assert_nothing_ran(pipeline: &Pipeline<FakePorts>) {
       let ports = pipeline.ports();
       assert_eq!(ports.strategy.handed(), []);
       assert_eq!(ports.cas.digests().len(), 0);
       assert_eq!(ports.cas.placements(), []);
       assert_eq!(ports.cache.recorded(), []);
       assert_eq!(ports.log.events, []);
       assert_eq!(ports.clock.reads(), 0);
   }

   /// The error a build of the workspace ends in.
   fn refused(pipeline: &mut Pipeline<FakePorts>) -> Error {
       pipeline
           .build(&entry(), &Overrides::new(), OnFailure::Stop)
           .unwrap_err()
   }

   /// `//:app`, and a test target that runs what `//:app` produces.
   fn tested() -> Pipeline<FakePorts> {
       root_only(vec![
           target("app", &[]),
           target_with("app_test", |target| {
               target.role = TargetRole::Test;
               target.run = Some(vec![Written::new("$deps")]);
               target.deps = vec![Written::new("app")];
           }),
       ])
   }

   #[test]
   fn a_test_that_runs_what_its_dependency_built_is_built_when_the_workspace_allows_that_program() {
       let mut pipeline = tested();
       let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
       ceiling.run.insert(program("out/app"));

       let outcomes = build(&mut pipeline, OnFailure::Stop);

       assert_eq!(
           classes(&outcomes),
           expected(&[
               ("//:app", OutcomeClass::Built),
               ("//:app_test", OutcomeClass::Built),
           ])
       );
       let handed = pipeline.ports().strategy.handed();
       let test = handed
           .iter()
           .find(|action| action.label == label("//:app_test"))
           .unwrap();
       let command: Vec<String> = test
           .command
           .arguments()
           .iter()
           .map(ToString::to_string)
           .collect();
       assert_eq!(command, ["out/app"]);
       assert_eq!(test.deps, [out("//:app", "app")]);
   }

   #[test]
   fn a_program_first_named_by_expansion_is_refused_before_anything_runs() {
       let mut pipeline = tested();

       let error = refused(&mut pipeline);

       assert_eq!(
           error.to_string(),
           "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
       );
       match &error {
           Error::Refused { want, site, .. } => {
               assert_eq!(want, &Want::Program(program("out/app")));
               let site = site.as_deref().unwrap();
               assert_eq!(site.label, label("//:app_test"));
           }
           other => unreachable!("expected Refused, got {other:?}"),
       }
       assert_nothing_ran(&pipeline);
   }

   #[test]
   fn a_command_that_expands_to_nothing_names_no_program() {
       let mut pipeline = root_only(vec![target_with("gen", |target| {
           target.run = Some(vec![Written::new("$in")]);
       })]);

       let error = refused(&mut pipeline);

       assert_eq!(
           error.to_string(),
           "build.lua: //:gen: the command names no program once expanded"
       );
       assert!(matches!(error, Error::NoProgram { .. }));
       assert_nothing_ran(&pipeline);
   }

   #[test]
   fn two_outputs_that_would_be_written_to_one_place_are_refused() {
       let writing_x = |name: &str| {
           target_with(name, |target| {
               target.outputs = Some(vec![Written::new("x")]);
           })
       };
       let mut same_path = root_only(vec![writing_x("a"), writing_x("b")]);

       let error = refused(&mut same_path);

       assert_eq!(
           error.to_string(),
           "out/x: //:a (build.lua) and //:b (build.lua) cannot both write here"
       );
       assert!(matches!(error, Error::OutputConflict { .. }));
       assert_nothing_ran(&same_path);

       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("lib", &[])], &["lib"])),
           ("lib", file(vec![target("text", &[])], &[])),
       ]));
       let mut file_and_directory = Pipeline::new(ports(source));
       let tools = &mut file_and_directory.ports_mut().tools;
       tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));

       let error = refused(&mut file_and_directory);

       assert_eq!(
           error.to_string(),
           "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
       );
       assert!(matches!(error, Error::OutputConflict { .. }));
       assert_nothing_ran(&file_and_directory);
   }

   #[test]
   fn a_load_error_and_a_refusal_of_the_plan_reach_the_caller_unchanged() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("app", &[])], &["lib"])),
           ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
       ]));
       let mut unreadable = Pipeline::<FakePorts>::new(ports(source));

       match refused(&mut unreadable) {
           Error::Evaluation {
               provenance,
               failure,
               ..
           } => {
               assert_eq!(provenance.file(), Path::new("lib/build.lua"));
               assert_eq!(failure, EvaluationFailure::Syntax);
           }
           other => unreachable!("expected Evaluation, got {other:?}"),
       }
       assert_nothing_ran(&unreadable);

       let mut disallowed = chain();
       let ceiling = &mut disallowed.ports_mut().manifest.workspace.ceiling;
       ceiling.run.clear();

       let error = refused(&mut disallowed);

       assert_eq!(
           error.to_string(),
           "build.lua: //:app: program cc is outside the ceiling (proc.run allows: nothing)"
       );
       assert!(matches!(error, Error::Refused { .. }));
       assert_nothing_ran(&disallowed);
   }
   ```

   The five flows, by what they pin:

   - A test target whose command is `$deps` is built when the workspace allows `out/app` to run, and its action is handed the command `out/app`.
   - Without `out/app` in the ceiling the build is `Refused`, naming `out/app` and the test target, and nothing ran: a program first named by expansion is held to the ceiling before execution like any other.
   - A command that expands to no argument is `NoProgram`.
   - Two targets naming the same output, and `//:lib` beside `//lib:text`, are each `OutputConflict`, with the texts the spec fixes.
   - A build file that fails to evaluate and a program the plan refuses reach the caller as the errors Load and Plan raise, and nothing ran.

   `assert_nothing_ran` is what "before anything runs" means here: no directory asked of the strategy, nothing stored or placed, no row, no event, and the clock never read.

3. Run the flows. The five new ones pass at once, because task 1 built the behaviour they pin:

   ```
   $ cargo test -p buildl-core --test flows build::
   test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
   ```

4. Confirm such a flow can fail. In `crates/buildl-core/src/execute/admission.rs`, in `admit`, replace `if !ceiling.run.contains(&program) {` with `if ceiling.run.is_empty() {`, so that any program may run once the ceiling allows one, and run the flow that pins the refusal:

   ```
   $ cargo test -p buildl-core --test flows build::a_program_first_named
   test build::a_program_first_named_by_expansion_is_refused_before_anything_runs ... FAILED

   called `Result::unwrap_err()` on an `Ok` value: [ActionOutcome { label: Label { directory: Directory(""), name: TargetName("app") }, class: Built, …

   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 73 filtered out; finished in 0.00s
   ```

   The build that should be refused runs to its end. Put `if !ceiling.run.contains(&program) {` back.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/build.rs
   $ git commit -m "test(buildl-core): pin what a build refuses before anything runs"
   ```

### Task 5 — Pin the log of a fixture workspace

**Files:**
- Create `crates/buildl-core/tests/flows/golden/log.jsonl`
- Modify `crates/buildl-core/tests/flows/build.rs`

**Steps:**

1. Widen the imports of `crates/buildl-core/tests/flows/build.rs`, and add the golden constant below them. Replace:

   ```rust
   use std::collections::BTreeMap;
   use std::path::Path;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileStat, Freshness, InfraKind,
       Label, OnFailure, OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName,
       SettingName, SettingValue, SourcePath, StagedItem, Summary, TargetRole, Timestamp, Want,
       Written,
   };

   use crate::common::{
       Content, Ends, FakeContent, FakeFile, FakePorts, FakeSource, Script, dir, file, ports, program,
       target, target_with, workspace,
   };
   ```

   With:

   ```rust
   use std::collections::BTreeMap;
   use std::path::Path;

   use buildl_core::{
       ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileStat, Freshness, InfraKind,
       Label, OnFailure, OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName,
       SettingName, SettingValue, SourcePath, StagedItem, Summary, TargetRole, Timestamp, Want,
       Written, json,
   };

   use crate::common::{
       Content, Ends, FakeContent, FakeFile, FakePorts, FakeSource, Order, Script, alias, dir, file,
       ports, program, rule, setting, target, target_with, workspace,
   };

   /// The events of [`two_directories`] built once, one canonical JSON object per line, pinned.
   const GOLDEN: &str = include_str!("golden/log.jsonl");
   ```

2. Write the failing flow, and the flow that builds the same workspace in another order. Add four helpers and two flows at the end of the file:

   ```rust
   /// The workspace of the graph flows, with one change: its test target runs what it depends on,
   /// `$deps`, where the graph flows' runs `$out/app`. The root holds a rule, a target using it, a
   /// target with dependencies in two directories, an alias, the test target and a setting; `lib`
   /// holds one target. `cc` can be found, and the workspace allows `out/app` to run.
   fn two_directories(order: Order) -> Pipeline<FakePorts> {
       let root = vec![
           rule(
               "cc",
               &["cc", "-c", "$in", "-o", "$out"],
               Some("compile $in"),
           ),
           target_with("main.o", |target| {
               target.run = None;
               target.rule = Some(Written::new("cc"));
               target.inputs = vec![Written::new("main.c")];
           }),
           target_with("app", |target| {
               target.run = Some(["cc", "$deps", "-o", "$out"].map(Written::new).to_vec());
               target.deps = vec![Written::new("main.o"), Written::new("//lib:text")];
           }),
           alias("default", "app"),
           target_with("app_test", |target| {
               target.role = TargetRole::Test;
               target.run = Some(
                   ["$deps", "--filter=$opt:test_filter"]
                       .map(Written::new)
                       .to_vec(),
               );
               target.deps = vec![Written::new("default")];
           }),
           setting("test_filter"),
       ];
       let lib = vec![target_with("text", |target| {
           target.inputs = vec![Written::new("text.c")];
       })];
       let source = FakeSource::new(workspace(vec![
           ("", file(root, &["lib"])),
           ("lib", file(lib, &[])),
       ]));
       let mut pipeline = Pipeline::new(ports(source));
       put(&mut pipeline, "main.c", "main.c", 100);
       put(&mut pipeline, "lib/text.c", "lib/text.c", 100);
       let ports = pipeline.ports_mut();
       ports
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       ports
           .manifest
           .workspace
           .ceiling
           .run
           .insert(program("out/app"));
       ports.dispatcher.order = order;
       pipeline
   }

   /// A build of [`two_directories`] with `test_filter` set to `Login`.
   fn build_filtered(pipeline: &mut Pipeline<FakePorts>) -> Vec<ActionOutcome> {
       let overrides = Overrides::from([(
           SettingName::parse("test_filter").unwrap(),
           SettingValue::parse("Login").unwrap(),
       )]);
       pipeline
           .build(&entry(), &overrides, OnFailure::Stop)
           .unwrap()
   }

   /// The command and the three lists of paths an action was handed, as text.
   fn handed(pipeline: &Pipeline<FakePorts>, raw: &str) -> [Vec<String>; 4] {
       let wanted = label(raw);
       let actions = pipeline.ports().strategy.handed();
       let action = actions
           .iter()
           .find(|action| action.label == wanted)
           .unwrap();
       [
           action
               .command
               .arguments()
               .iter()
               .map(ToString::to_string)
               .collect(),
           action.inputs.iter().map(ToString::to_string).collect(),
           action.deps.iter().map(ToString::to_string).collect(),
           action.outputs.iter().map(ToString::to_string).collect(),
       ]
   }

   /// Texts as owned strings.
   fn texts(raw: &[&str]) -> Vec<String> {
       raw.iter().map(|text| (*text).to_owned()).collect()
   }

   #[test]
   fn a_two_directory_workspace_hands_each_action_its_expanded_command_and_logs_the_pinned_events() {
       let mut pipeline = two_directories(Order::OldestFirst);

       let outcomes = build_filtered(&mut pipeline);

       assert_eq!(
           Summary::of(&outcomes),
           Summary {
               failed: 0,
               skipped: 0,
               built: 4,
               cached: 0
           }
       );
       assert_eq!(
           handed(&pipeline, "//:main.o"),
           [
               texts(&["cc", "-c", "main.c", "-o", "out/main.o"]),
               texts(&["main.c"]),
               texts(&[]),
               texts(&["out/main.o"]),
           ]
       );
       assert_eq!(
           handed(&pipeline, "//lib:text"),
           [
               texts(&["cc"]),
               texts(&["lib/text.c"]),
               texts(&[]),
               texts(&["out/lib/text"]),
           ]
       );
       assert_eq!(
           handed(&pipeline, "//:app"),
           [
               texts(&["cc", "out/main.o", "out/lib/text", "-o", "out/app"]),
               texts(&[]),
               texts(&["out/main.o", "out/lib/text"]),
               texts(&["out/app"]),
           ]
       );
       assert_eq!(
           handed(&pipeline, "//:app_test"),
           [
               texts(&["out/app", "--filter=Login"]),
               texts(&[]),
               texts(&["out/app"]),
               texts(&["out/app_test"]),
           ]
       );

       let lines: Vec<String> = pipeline
           .ports()
           .log
           .events
           .iter()
           .map(|event| json::canonical::to_string(event).unwrap())
           .collect();
       assert_eq!(lines.join("\n"), GOLDEN.trim_end());
   }

   #[test]
   fn the_order_tasks_are_taken_in_changes_neither_a_row_nor_what_is_stored() {
       let mut oldest = two_directories(Order::OldestFirst);
       let mut newest = two_directories(Order::NewestFirst);

       build_filtered(&mut oldest);
       build_filtered(&mut newest);

       assert_ne!(
           oldest.ports().strategy.ran(),
           newest.ports().strategy.ran(),
           "the two builds ran their actions in different orders"
       );
       assert_eq!(oldest.ports().cache.rows, newest.ports().cache.rows);
       assert_eq!(oldest.ports().cas.digests(), newest.ports().cas.digests());
       assert_eq!(oldest.ports().cas.placed(), newest.ports().cas.placed());
   }
   ```

   `two_directories` declares the workspace the graph flows resolve, in `crates/buildl-core/tests/flows/graph.rs`, with one change: the test target's command is `$deps --filter=$opt:test_filter`, where the graph flows' is `$out/app --filter=$opt:test_filter`. That file is left as it is, because the golden plan pins its `$out/app`. The first flow checks the command and the three lists of paths each action was handed — `cc -c main.c -o out/main.o`, `cc out/main.o out/lib/text -o out/app`, `out/app --filter=Login` — and then the log.

3. Create `crates/buildl-core/tests/flows/golden/log.jsonl` holding an empty object, so the flow builds and fails:

   ```json
   {}
   ```

4. Run the flows and confirm the first fails, the events on the left and the empty object on the right. The second passes at once: task 1 built the behaviour it pins, and it reads no golden file.

   ```
   $ cargo test -p buildl-core --test flows build::
   test build::a_two_directory_workspace_hands_each_action_its_expanded_command_and_logs_the_pinned_events ... FAILED

   assertion `left == right` failed
     left: "{\"at\":3,\"class\":\"Built\",\"command\":[\"cc\",\"-c\",\"main.c\",\"-o\",\"out/main.o\"],\"diagnostic\":null,\"file\":\"build.lua\",\"finished\":2,…
    right: "{}"

   test result: FAILED. 19 passed; 1 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
   ```

5. Replace the content of `crates/buildl-core/tests/flows/golden/log.jsonl` with the four events, one per line:

   ```json
   {"at":3,"class":"Built","command":["cc","-c","main.c","-o","out/main.o"],"diagnostic":null,"file":"build.lua","finished":2,"key":"9f9c805d4238f7a270a3729ecb9a6884c63f7ca55d82ae4b00c525aec70051dd","label":"//:main.o","outputs":[{"digest":"eac6a7accc2a1c89f062d3f99550f6c0fb0837d918e090cad952e0d8b891c892","name":"main.o"}],"started":1,"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","stdout":"6068e75dbabc3612b447b8b3b256072fcbf6857e823efe66bf0d7e8bb0659169"}
   {"at":6,"class":"Built","command":["cc"],"diagnostic":null,"file":"lib/build.lua","finished":5,"key":"904d61e88efb81acaab476b70ef0efbdd7f5c2c5d7764f825d4c1d242c2c2f06","label":"//lib:text","outputs":[{"digest":"1bcf032e11bb242ddfdea780a380fb592ea236b63f564706629a2e5aedb7b31f","name":"text"}],"started":4,"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","stdout":"93dd56cf982cbc94c88d5ccceb34df77d33bc409e891e917559e1f32a90ca07b"}
   {"at":9,"class":"Built","command":["cc","out/main.o","out/lib/text","-o","out/app"],"diagnostic":null,"file":"build.lua","finished":8,"key":"61707f4b29c2300190c4d744773654fb606f8d721fdf3a31a480d3ee8be45ac5","label":"//:app","outputs":[{"digest":"89d1784aed872ae1e7885e42dbc669ba4dcf92df477135a73ca4c8372da7148f","name":"app"}],"started":7,"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","stdout":"182da3cc83dbbaa070364eb579ff097542f0ce0daf79cba25127f01fd120c811"}
   {"at":12,"class":"Built","command":["out/app","--filter=Login"],"diagnostic":null,"file":"build.lua","finished":11,"key":"30c502bacba3c9ac14133e396426e23dec3aeb0cab2cfad111436ef06f435f5a","label":"//:app_test","outputs":[{"digest":"db255e04f971798c36596af747c21f915240a73a0393e2f3c578003c6c73c7c9","name":"app_test"}],"started":10,"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","stdout":"9b4cb18784f8842a14a39e0fa859c35864b81a22831877145f34e286fb323ea6"}
   ```

   Each line holds the twelve keys the spec fixes, in the order the canonical serializer writes them: `at`, `class`, `command`, `diagnostic`, `file`, `finished`, `key`, `label`, `outputs`, `started`, `stderr`, `stdout`. The instants are the fake clock's: a worker's two reads, then the calling thread's one, per target.

6. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows build::
   test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.01s
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

8. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/golden/log.jsonl crates/buildl-core/tests/flows/build.rs
   $ git commit -m "test(buildl-core): pin the log of a fixture workspace"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`; `crates/buildl-core/Cargo.toml` is unchanged.
- Every row of spec §9's flow table has a passing flow in `crates/buildl-core/tests/flows/build.rs`:

  | Row of the flow table | Flow |
  |---|---|
  | a workspace never built | `a_workspace_never_built_builds_every_target_after_what_it_depends_on` |
  | built again, nothing changed | `a_second_build_with_nothing_changed_runs_nothing_and_places_every_output_again` |
  | one input changes and its target's output does too | `an_edited_input_rebuilds_its_target_and_what_depends_on_it_and_nothing_else` |
  | one input changes and its target's output does not | `a_target_rebuilt_to_the_same_output_leaves_what_depends_on_it_cached` |
  | `//:a` fails under `KeepGoing` | `under_keep_going_a_failure_skips_only_what_depends_on_it` |
  | the same workspace under `Stop` | `under_stop_nothing_begins_after_the_first_failure` |
  | three targets under `Stop`, the first cached with a `place` that fails | `under_stop_a_cached_target_that_cannot_be_placed_stops_what_waits_behind_it` |
  | an action's outputs reach the cas and `record` fails | `a_row_that_cannot_be_recorded_fails_its_target_and_the_next_build_runs_it_again` |
  | a non-zero exit, a timeout, and the six other failures on a worker | `every_way_an_action_can_fail_is_a_class_of_its_own_and_none_is_recorded` |
  | a built workspace whose cas forgets one output | `a_target_whose_output_left_the_store_runs_again_and_the_others_stay_cached` |
  | a built workspace, one `place` failing for a cached target | `a_cached_target_whose_output_cannot_be_placed_fails_and_keeps_its_row` |
  | a test target whose command is `$deps`, with and without `out/app` in `proc.run` | `a_test_that_runs_what_its_dependency_built_is_built_when_the_workspace_allows_that_program`, `a_program_first_named_by_expansion_is_refused_before_anything_runs` |
  | a command that expands to nothing; two targets claiming one path; `//:lib` beside `//lib:text` | `a_command_that_expands_to_nothing_names_no_program`, `two_outputs_that_would_be_written_to_one_place_are_refused` |
  | a target with `always = true`, built twice | `a_target_that_always_runs_runs_on_every_build_and_its_dependent_stays_cached` |
  | two targets with no dependency; the log fails on the first append | `a_log_that_cannot_be_written_fails_the_build_and_keeps_what_was_already_recorded` |
  | the two-directory workspace, built once | `a_two_directory_workspace_hands_each_action_its_expanded_command_and_logs_the_pinned_events` |
  | the same workspace built with the newest task first | `the_order_tasks_are_taken_in_changes_neither_a_row_nor_what_is_stored` |
  | a build file that fails to evaluate; a program outside the ceiling | `a_load_error_and_a_refusal_of_the_plan_reach_the_caller_unchanged` |

- `execute` has the signature of spec §5.1 and checks in its order; `Pipeline::build` has the signature of spec §6, and `check`, `graph` and `plan` keep theirs.
- `crates/expected-module-edges.txt` holds 24 lines: the 20 it had before this chain, and `execute -> error`, `execute -> ports`, `execute -> types`, `pipeline -> execute`. It holds no edge between `execute` and `load`, `resolve` or `plan`.
- No `expect(dead_code)` remains under `crates/buildl-core/src/execute/`: `grep -rn 'dead_code' crates/buildl-core/src/execute` prints nothing.
- `execute/` implements no port: `grep -rn 'impl .* for ' crates/buildl-core/src/execute` prints nothing.
