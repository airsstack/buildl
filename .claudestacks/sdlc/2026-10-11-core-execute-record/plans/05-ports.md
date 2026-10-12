---
status: done
created: 2026-10-11
depends-on: [01, 02, 03, 04]
---

# Ports Implementation Plan

**Goal:** A pipeline is built from one set holding every port a build uses.

**Architecture:** Four new traits under `ports/`, one file each, with the signatures the design fixes. `ContentStore` stores bytes by digest, says whether a digest is held, and makes held content a file of the workspace. `EventLog` appends one event. `ExecStrategy` runs one action in three steps, `prepare`, `run`, `collect`, with an associated `Dir` type because only a strategy knows the directory it runs an action in; `collect` is generic over the store, so an output never passes through this crate as bytes. `Dispatcher::dispatch(work, step)` is generic over the task and the result, so the port knows nothing of builds: `work` may run on any thread, `step` always runs on the calling thread. `ContentStore` and `ExecStrategy` are `Send + Sync` because actions use them from the threads they run on. `ActionCache` gains `record`, one durable write per row, which is all a build needs to say that a row exists once its target's outputs are stored. The `Ports` bundle gains five associated types — `Cas`, `Log`, `Strategy`, `Dispatcher` and `Clock`, the last bounded `Clock + Sync` — and `PortSet` gains the five fields `cas`, `log`, `strategy`, `dispatcher` and `clock`, twelve ports in all. Every fake lives in the flows' shared file and is built from the public API alone; what a fake shares with a worker sits behind a `Mutex` or an atomic. Each fake is held to its port's contract by a small test beside it, because the flows that will run a build against these fakes are only as good as the fakes are. The crate gains no production implementation of any port, and nothing calls the new ports yet.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D7, D8, D9, D10, D11, D19), §4 (ports), §8 (the files under `ports/`, `tests/flows/common.rs`), §9 (the fakes).

**Checkpoints:** review once, when all seven tasks are done.

---

## Context an implementer needs

What plans `01` to `04` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `OutputPath` (`Ord`, `Display` is its text; built only by `OutputPath::of(&Directory, &Label, &OutputName)`) | `crates/buildl-core/src/types/output_path.rs` | where `ContentStore::place` puts content; a key of the fakes' maps |
| `ReadyAction` (public fields `label`, `command`, `env`, `inputs`, `deps`, `outputs`, `network`; `Clone`) | `crates/buildl-core/src/types/ready_action.rs` | what every `ExecStrategy` method is handed |
| `RawOutcome { exit, stdout, stderr }`, `Exit::{Exited(i32), TimedOut}` | `crates/buildl-core/src/types/raw_outcome.rs` | what `ExecStrategy::run` returns |
| `OutcomeEvent` (public fields; `Clone`, `PartialEq`), `ActionOutcome`, `OutcomeClass` | `crates/buildl-core/src/types/outcome_event.rs`, `outcome.rs` | what `EventLog::append` is handed |
| `CacheRow::new(key, outputs, stdout, stderr) -> Option<CacheRow>` | `crates/buildl-core/src/types/cache_row.rs` | the rows the fake cache's tests record |
| `PortName::{ActionCache, ContentStore, EventLog, ExecStrategy, Dispatcher}`; `Error::Adapter { port, diagnostic }`, rendered `{port}: {diagnostic}` | `crates/buildl-core/src/error.rs` | named in the traits' docs; what every fake's own failure is |
| `Clock` with `now(&self) -> Timestamp`, and its unit-test fake | `crates/buildl-core/src/ports/clock.rs` | unchanged; the bundle starts naming it |
| `Timestamp::from_unix_nanos`, `Timestamp::UNIX_EPOCH` | `crates/buildl-core/src/types/timestamp.rs` | what the fake clocks return |
| `ActionCache` with `row` alone | `crates/buildl-core/src/ports/action_cache.rs` | the trait that gains `record` |
| The three implementations of `ActionCache` | `crates/buildl-core/tests/flows/common.rs:258` (`FakeCache`), `crates/buildl-core/src/ports/bundle.rs:95` (the doc example), `crates/buildl-core/src/ports/port_set.rs:144` (the test stub) | what task 3 changes together |
| The three implementations of `Ports` | `crates/buildl-core/tests/flows/common.rs:268` (`FakePorts`), `crates/buildl-core/src/ports/bundle.rs:104` (the doc example), `crates/buildl-core/src/ports/port_set.rs:154` (the test bundle) | what task 6 changes together |
| The two `PortSet` values | `crates/buildl-core/tests/flows/common.rs:298` (`ports`), `crates/buildl-core/src/ports/port_set.rs:166` (the unit test) | what task 7 changes together |
| `pub mod common;` and the fakes of the seven ports a plan reads, with `dir`, `program`, `open_workspace`, `ports` | `crates/buildl-core/tests/flows/main.rs`, `crates/buildl-core/tests/flows/common.rs` | the file the new fakes join |

Those line numbers are the files as plan `04` leaves them. `grep -rn 'impl.*ActionCache for\|impl.*Ports for\|PortSet {\|PortSet::<' --include='*.rs' crates` found those eight places and no other: `crates/buildl`, `crates/buildl-lua` and `crates/buildl-cli` implement none of these traits and build no `PortSet`. The same search for `impl DeclarationSource for` finds `crates/buildl-lua/src/source.rs`, so the method does find what is there.

Gate facts that bite here:

- The four new trait files hold no logic, so tasks 1 and 2 have no failing-test step; the gate's
  build and rustdoc steps check them, and the fakes of tasks 4 and 5 are what prove each trait
  can be implemented from outside the crate.
- `ports -> error` and `ports -> types` are already in `crates/expected-module-edges.txt`, so this
  plan changes no module edge.
- `crates/buildl-core/src/ports/mod.rs` and `crates/buildl-core/src/lib.rs` keep their `mod`
  lines and `pub use` lists sorted. A task adds a name to the right list and lets
  `cargo fmt --all` place it.
- `tests/flows/main.rs` declares `pub mod common;`, so a fake no flow uses yet is not reported
  as dead code: the gate passed after each of tasks 3 to 7 with the new fakes used only by their
  own tests.
- The fakes' tests live in a `tests` module at the bottom of `tests/flows/common.rs` and run as
  `common::tests::…` in the flows binary. `clippy::unwrap_used` is denied there as everywhere, so
  the module expects it, and the fakes themselves take a lock with
  `unwrap_or_else(PoisonError::into_inner)`.
- Clippy's `significant_drop_tightening` refuses a lock guard held to the end of a function when
  it could be dropped earlier. `FakeCas::put` and `FakeCas::place` therefore `drop(state)` before
  returning.
- Extending `ActionCache`, `Ports` or `PortSet` breaks every implementation or value at once, so
  each of tasks 3, 6 and 7 changes all of them in one commit.
- The doc example of `ports/bundle.rs` holds a fenced block, so the file is shown below inside
  four backticks.
- A script's `stdout` and `stderr` are `Option`s: `None` means what an unscripted action prints,
  which depends on the action's label and so cannot be a `Default` value.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure of a `common::` run does not, and the `56 passed` of a whole flows run is the 40 flows the crate had and the 16 tests this plan adds.

## File structure

```
crates/buildl-core/src/ports/content_store.rs   — [create] ContentStore
crates/buildl-core/src/ports/event_log.rs       — [create] EventLog
crates/buildl-core/src/ports/exec_strategy.rs   — [create] ExecStrategy
crates/buildl-core/src/ports/dispatcher.rs      — [create] Dispatcher
crates/buildl-core/src/ports/action_cache.rs    — [modify] record
crates/buildl-core/src/ports/bundle.rs          — [modify] five associated types; the doc example
crates/buildl-core/src/ports/port_set.rs        — [modify] five fields; Debug; the test stub
crates/buildl-core/src/ports/mod.rs             — [modify] declare and re-export the four modules
crates/buildl-core/src/lib.rs                   — [modify] re-export the four traits
crates/buildl-core/tests/flows/common.rs        — [modify] five fakes, FakeCache::record, FakePorts, ports(), the fakes' tests
```

### Task 1 — Add the content-store and event-log ports

**Files:**
- Create `crates/buildl-core/src/ports/content_store.rs`
- Create `crates/buildl-core/src/ports/event_log.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/ports/content_store.rs`:

   ```rust
   //! The port through which a build stores what actions produce, by content.
   //!
   //! Its own file because a store keeps bytes somewhere and puts files where a workspace expects
   //! them, and this crate does neither: it hands over bytes, asks whether a digest is held, and
   //! names the path an output belongs at.
   //!
   //! Responsibilities: [`ContentStore`].
   //!
   //! Non-responsibilities: implementing it, and deciding what is stored or placed. The first
   //! belongs to an adapter crate; the second to the phase that runs actions.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{Digest, OutputPath};

   /// Holds content by its digest, and makes held content a file of the workspace.
   ///
   /// `Send + Sync` because several actions store what they produced at once, each from the thread
   /// that ran it.
   pub trait ContentStore: Send + Sync {
       /// Stores `bytes` and returns their digest. Storing bytes already held is not an error.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the bytes cannot be stored.
       fn put(&self, bytes: &[u8]) -> Result<Digest>;

       /// Whether content with that digest is held.
       fn contains(&self, digest: &Digest) -> bool;

       /// Makes the content with that digest the file at `at`, replacing what is there.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the content is not held or
       /// the file cannot be made.
       fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()>;
   }
   ```

2. Create `crates/buildl-core/src/ports/event_log.rs`:

   ```rust
   //! The port through which a build writes down how each target ended.
   //!
   //! Its own file because a log is kept somewhere a later reader can find it, and this crate keeps
   //! nothing: it hands over one event per target and lets an adapter decide where events go.
   //!
   //! Responsibilities: [`EventLog`].
   //!
   //! Non-responsibilities: implementing it, and building the events. The first belongs to an
   //! adapter crate; the second to the phase that runs actions.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::OutcomeEvent;

   /// Receives the record of every target a build handled, one event at a time.
   pub trait EventLog {
       /// Appends one event.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::EventLog`](crate::PortName::EventLog), when the event cannot be written.
       fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
   }
   ```

3. Declare both modules. `crates/buildl-core/src/ports/mod.rs` becomes:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API. Every
   //! trait of the crate lives here, the bundle included, so no logic module declares one.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //! - [`Manifest`] — what a workspace's manifest says.
   //! - [`Host`] — the platform actions run on, and the values of environment variables.
   //! - [`Digester`] — a file's metadata and content digest.
   //! - [`StatCache`] — the digests of files already hashed.
   //! - [`ToolResolver`] — what identifies the program a command names.
   //! - [`ActionCache`] — what the previous build recorded for a target.
   //! - [`ContentStore`] — what actions produce, held by content and placed in the workspace.
   //! - [`EventLog`] — the record of how each target of a build ended.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   //!   parameter instead of one per port.
   //! - [`PortSet`] — one instance of each port a bundle names, so a pipeline is built from one
   //!   value.
   //!
   //! Non-responsibilities: production implementations. A real implementation does I/O, so it lives
   //! in an adapter crate; the only implementations in this crate are test fakes.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod action_cache;
   pub mod bundle;
   pub mod clock;
   pub mod content_store;
   pub mod declaration_source;
   pub mod digester;
   pub mod event_log;
   pub mod host;
   pub mod manifest;
   pub mod port_set;
   pub mod stat_cache;
   pub mod tool_resolver;

   pub use action_cache::ActionCache;
   pub use bundle::Ports;
   pub use clock::Clock;
   pub use content_store::ContentStore;
   pub use declaration_source::DeclarationSource;
   pub use digester::Digester;
   pub use event_log::EventLog;
   pub use host::Host;
   pub use manifest::Manifest;
   pub use port_set::PortSet;
   pub use stat_cache::StatCache;
   pub use tool_resolver::ToolResolver;
   ```

4. Re-export both. In `crates/buildl-core/src/lib.rs`, add `ContentStore` and `EventLog` to the `pub use ports::{…}` list, then run `cargo fmt --all`, which sorts the additions and rewraps the list. The list reads:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, EventLog, Host, Manifest,
       PortSet, Ports, StatCache, ToolResolver,
   };
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/src/ports/content_store.rs crates/buildl-core/src/ports/event_log.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the content-store and event-log ports"
   ```

### Task 2 — Add the exec-strategy and dispatcher ports

**Files:**
- Create `crates/buildl-core/src/ports/exec_strategy.rs`
- Create `crates/buildl-core/src/ports/dispatcher.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/ports/exec_strategy.rs`:

   ```rust
   //! The port through which a build runs one action.
   //!
   //! Its own file because running a command means making a directory, starting a process and
   //! reading what it left behind, and this crate does none of those: it hands over an action whose
   //! command and paths are already settled, and learns how the run ended.
   //!
   //! Responsibilities: [`ExecStrategy`].
   //!
   //! Non-responsibilities: implementing it, and deciding what an ending means. The first belongs
   //! to an adapter crate, which also chooses how strongly an action is isolated; the second to the
   //! phase that runs actions.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::ports::ContentStore;
   use crate::types::{Digest, RawOutcome, ReadyAction};

   /// Runs one action in a directory of its own, and stores what the action produced.
   ///
   /// The three methods are called in order for one action, by the same caller: [`prepare`], then
   /// [`run`], then [`collect`]. `Send + Sync` because several actions run at once, each on its own
   /// thread.
   ///
   /// [`prepare`]: ExecStrategy::prepare
   /// [`run`]: ExecStrategy::run
   /// [`collect`]: ExecStrategy::collect
   pub trait ExecStrategy: Send + Sync {
       /// The directory one action runs in, as this strategy knows it.
       type Dir;

       /// Makes a directory to run `action` in, where every path of its `inputs` and `deps` is
       /// readable at that relative path.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the directory cannot be
       /// made.
       fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;

       /// Runs the action's `command` in `dir` as an argument vector, with no shell, with the
       /// variables of its `env` and none of the host's own; waits, and returns how the command
       /// ended with what it printed.
       ///
       /// A command that ran and failed is not an error: it is an outcome with a non-zero status.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the command cannot be
       /// started.
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;

       /// Stores into `cas` what the action left at each path of its `outputs`, and returns one
       /// entry per path, in order: the digest of what was there, or `None` when nothing was.
       ///
       /// # Errors
       ///
       /// Returns whatever `cas` returns, and [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when an output cannot be read.
       fn collect<C: ContentStore>(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           cas: &C,
       ) -> Result<Vec<Option<Digest>>>;
   }
   ```

2. Create `crates/buildl-core/src/ports/dispatcher.rs`:

   ```rust
   //! The port through which a build has several things done at once.
   //!
   //! Its own file because doing work in parallel means starting threads, and this crate starts
   //! none: it hands over what to do with one task and what to do with one result, and an adapter
   //! decides how many tasks are worked on at a time.
   //!
   //! Responsibilities: [`Dispatcher`].
   //!
   //! Non-responsibilities: implementing it, and knowing what a task is. The first belongs to an
   //! adapter crate; the port is generic over the task and the result, so it knows nothing of
   //! targets or actions.
   //!
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
       /// calls at once. Every task `step` returns is run, and every result reaches `step`.
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

3. Declare both modules. `crates/buildl-core/src/ports/mod.rs` becomes:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API. Every
   //! trait of the crate lives here, the bundle included, so no logic module declares one.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //! - [`Manifest`] — what a workspace's manifest says.
   //! - [`Host`] — the platform actions run on, and the values of environment variables.
   //! - [`Digester`] — a file's metadata and content digest.
   //! - [`StatCache`] — the digests of files already hashed.
   //! - [`ToolResolver`] — what identifies the program a command names.
   //! - [`ActionCache`] — what the previous build recorded for a target.
   //! - [`ContentStore`] — what actions produce, held by content and placed in the workspace.
   //! - [`EventLog`] — the record of how each target of a build ended.
   //! - [`ExecStrategy`] — running one action in a directory of its own.
   //! - [`Dispatcher`] — having several tasks worked on at once.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   //!   parameter instead of one per port.
   //! - [`PortSet`] — one instance of each port a bundle names, so a pipeline is built from one
   //!   value.
   //!
   //! Non-responsibilities: production implementations. A real implementation does I/O, so it lives
   //! in an adapter crate; the only implementations in this crate are test fakes.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod action_cache;
   pub mod bundle;
   pub mod clock;
   pub mod content_store;
   pub mod declaration_source;
   pub mod digester;
   pub mod dispatcher;
   pub mod event_log;
   pub mod exec_strategy;
   pub mod host;
   pub mod manifest;
   pub mod port_set;
   pub mod stat_cache;
   pub mod tool_resolver;

   pub use action_cache::ActionCache;
   pub use bundle::Ports;
   pub use clock::Clock;
   pub use content_store::ContentStore;
   pub use declaration_source::DeclarationSource;
   pub use digester::Digester;
   pub use dispatcher::Dispatcher;
   pub use event_log::EventLog;
   pub use exec_strategy::ExecStrategy;
   pub use host::Host;
   pub use manifest::Manifest;
   pub use port_set::PortSet;
   pub use stat_cache::StatCache;
   pub use tool_resolver::ToolResolver;
   ```

4. Re-export both. In `crates/buildl-core/src/lib.rs`, add `ExecStrategy` and `Dispatcher` to the `pub use ports::{…}` list, then run `cargo fmt --all`. The list reads:

   ```rust
   pub use ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, PortSet, Ports, StatCache, ToolResolver,
   };
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/ports/exec_strategy.rs crates/buildl-core/src/ports/dispatcher.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the exec-strategy and dispatcher ports"
   ```

### Task 3 — Let the action cache record a row

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/src/ports/action_cache.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`

**Steps:**

1. Write the failing implementation. In `crates/buildl-core/tests/flows/common.rs`, give the fake cache the method, the targets it fails for and the list of what it recorded. Replace:

   ```rust
   /// The rows of a previous build, held in memory.
   #[derive(Debug, Default)]
   pub struct FakeCache {
       /// The rows.
       pub rows: BTreeMap<Label, CacheRow>,
   }

   impl ActionCache for FakeCache {
       fn row(&self, label: &Label) -> Option<&CacheRow> {
           self.rows.get(label)
       }
   }
   ```

   With:

   ```rust
   /// The rows of a previous build and of this one, held in memory.
   #[derive(Debug, Default)]
   pub struct FakeCache {
       /// The rows.
       pub rows: BTreeMap<Label, CacheRow>,
       /// The targets whose row it fails to record.
       pub failing: BTreeSet<Label>,
       recorded: Vec<Label>,
   }

   impl FakeCache {
       /// Every target a row was recorded for, in the order recorded.
       #[must_use]
       pub fn recorded(&self) -> Vec<Label> {
           self.recorded.clone()
       }
   }

   impl ActionCache for FakeCache {
       fn row(&self, label: &Label) -> Option<&CacheRow> {
           self.rows.get(label)
       }

       fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
           if self.failing.contains(&label) {
               return Err(Error::Adapter {
                   port: PortName::ActionCache,
                   diagnostic: Diagnostic::new(format!("{label} cannot be recorded")),
               });
           }
           self.recorded.push(label.clone());
           self.rows.insert(label, row);
           Ok(())
       }
   }
   ```

2. Write the tests that hold it to the port's contract. Append to the end of the same file, after the `workspace` function:

   ```rust
   /// The fakes held to the contracts of the ports they stand in for: a flow is only as good as
   /// the fake it runs against.
   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;

       use buildl_core::{
           ActionCache, ActionKey, Argument, CacheRow, Command, Digest, Error, KeyComponents, Label,
           NetworkAccess, OutputName, OwnComponents, Platform, PortName,
       };

       use super::FakeCache;

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       /// A row for a target that ran `cc` and produced one output, `app`, holding `content`.
       fn row(content: &str) -> CacheRow {
           let key = ActionKey::new(KeyComponents {
               own: OwnComponents {
                   command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                   inputs: Vec::new(),
                   outputs: vec![OutputName::parse("app").unwrap()],
                   env: BTreeMap::new(),
                   tool: None,
                   settings: BTreeMap::new(),
                   network: NetworkAccess::Sealed,
                   platform: Platform::parse("linux/x86_64").unwrap(),
               },
               deps: Vec::new(),
           })
           .unwrap();
           let nothing = Digest::of(b"");
           CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
       }

       #[test]
       fn a_recorded_row_is_the_one_read_back() {
           let mut cache = FakeCache::default();
           assert_eq!(cache.row(&label("//:app")), None);

           cache.record(label("//:app"), row("first")).unwrap();
           cache.record(label("//:app"), row("second")).unwrap();

           assert_eq!(cache.row(&label("//:app")), Some(&row("second")));
           assert_eq!(cache.recorded(), [label("//:app"), label("//:app")]);
       }

       #[test]
       fn a_row_that_cannot_be_recorded_leaves_what_was_there() {
           let mut cache = FakeCache::default();
           cache.record(label("//:app"), row("first")).unwrap();
           cache.failing.insert(label("//:app"));

           let error = cache.record(label("//:app"), row("second")).unwrap_err();

           assert_eq!(error.to_string(), "action cache: //:app cannot be recorded");
           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::ActionCache),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
           assert_eq!(cache.row(&label("//:app")), Some(&row("first")));
           assert_eq!(cache.recorded(), [label("//:app")]);
       }
   }
   ```

3. Run them and confirm they fail to build, because the port has no such method:

   ```
   $ cargo test -p buildl-core --test flows common::
   error[E0407]: method `record` is not a member of trait `ActionCache`
   ```

4. Add the method. Replace the whole of `crates/buildl-core/src/ports/action_cache.rs` with:

   ```rust
   //! The port through which the pipeline reads and writes what a build recorded for a target.
   //!
   //! Its own file because the rows outlive a run, so they are stored somewhere, and this crate
   //! stores nothing. A plan reads the row a target's last build left; a build writes a new one
   //! once the target has been built again.
   //!
   //! Responsibilities: [`ActionCache`].
   //!
   //! Non-responsibilities: implementing it, and deciding when a row is written. The first belongs
   //! to an adapter crate; a row is written only for a target whose outputs are already stored.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{CacheRow, Label};

   /// Holds the row the last successful build of each target recorded.
   pub trait ActionCache {
       /// The row for the target `label` names, if it has one: the one a previous build left, or
       /// the one [`record`](ActionCache::record) last stored.
       fn row(&self, label: &Label) -> Option<&CacheRow>;

       /// Stores `row` durably as the row of the target `label` names.
       ///
       /// One call writes one row, and the row is kept once the call has returned `Ok`: a build
       /// that dies afterwards does not lose it. After `Ok`, [`row`](ActionCache::row) returns it.
       /// After `Err`, [`row`](ActionCache::row) returns what it returned before the call.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ActionCache`](crate::PortName::ActionCache), when the row cannot be stored.
       fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
   }
   ```

5. Give the bundle's doc example the method. In `crates/buildl-core/src/ports/bundle.rs`, replace:

   ```rust
   /// impl ActionCache for Nothing {
   ///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
   ///         None
   ///     }
   /// }
   ```

   With:

   ```rust
   /// impl ActionCache for Nothing {
   ///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
   ///         None
   ///     }
   ///
   ///     fn record(&mut self, _label: Label, _row: CacheRow) -> Result<()> {
   ///         Ok(())
   ///     }
   /// }
   ```

6. Give the port set's test stub the method. In the test module of `crates/buildl-core/src/ports/port_set.rs`, replace:

   ```rust
       impl ActionCache for Stub {
           fn row(&self, _label: &Label) -> Option<&CacheRow> {
               None
           }
       }
   ```

   With:

   ```rust
       impl ActionCache for Stub {
           fn row(&self, _label: &Label) -> Option<&CacheRow> {
               None
           }

           fn record(&mut self, _label: Label, _row: CacheRow) -> Result<()> {
               Ok(())
           }
       }
   ```

7. Say in the module doc that the port now writes. In `crates/buildl-core/src/ports/mod.rs`, replace:

   ```rust
   //! - [`ActionCache`] — what the previous build recorded for a target.
   ```

   With:

   ```rust
   //! - [`ActionCache`] — what a build recorded for a target, read and written.
   ```

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::
   test common::tests::a_row_that_cannot_be_recorded_leaves_what_was_there ... ok
   test common::tests::a_recorded_row_is_the_one_read_back ... ok
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out
   ```

9. Run the bundle's doc example and confirm it passes:

   ```
   $ cargo test -p buildl-core --doc ports::bundle
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out
   ```

10. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

11. Commit:

    ```
    $ git add crates/buildl-core/tests/flows/common.rs crates/buildl-core/src/ports/action_cache.rs crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/port_set.rs crates/buildl-core/src/ports/mod.rs
    $ git commit -m "feat(buildl-core): let the action cache record a row"
    ```

### Task 4 — Add fakes for the content store, the event log and the clock

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing tests. In the `tests` module at the bottom of `crates/buildl-core/tests/flows/common.rs`, widen the imports. Replace:

   ```rust
       use std::collections::BTreeMap;

       use buildl_core::{
           ActionCache, ActionKey, Argument, CacheRow, Command, Digest, Error, KeyComponents, Label,
           NetworkAccess, OutputName, OwnComponents, Platform, PortName,
       };

       use super::FakeCache;
   ```

   With:

   ```rust
       use std::collections::{BTreeMap, BTreeSet};
       use std::path::PathBuf;

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Clock, Command, ContentStore,
           Digest, Directory, Error, EventLog, KeyComponents, Label, NetworkAccess, OutcomeClass,
           OutcomeEvent, OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance,
           Timestamp,
       };

       use super::{FakeCache, FakeCas, FakeClock, FakeLog, dir};
   ```

2. In the same module, add two helpers directly below the `label` helper:

   ```rust
       /// Where the output `name` of the target `raw` lives, under `out`.
       fn out_path(raw: &str, name: &str) -> OutputPath {
           OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
       }

       /// The event of a target declared in the root build file that was found cached.
       fn event(raw: &str) -> OutcomeEvent {
           OutcomeEvent {
               at: Timestamp::from_unix_nanos(1),
               provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
               command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
               key: None,
               started: None,
               finished: None,
               outcome: ActionOutcome {
                   label: label(raw),
                   class: OutcomeClass::Cached,
                   diagnostic: None,
                   outputs: Vec::new(),
                   stdout: None,
                   stderr: None,
               },
           }
       }
   ```

3. In the same module, add the tests after the last one, `a_row_that_cannot_be_recorded_leaves_what_was_there`:

   ```rust
       #[test]
       fn stored_content_is_held_under_its_digest() {
           let cas = FakeCas::default();

           let digest = cas.put(b"app").unwrap();

           assert_eq!(digest, Digest::of(b"app"));
           assert_eq!(cas.put(b"app").unwrap(), digest, "storing it again");
           assert!(cas.contains(&digest));
           assert!(!cas.contains(&Digest::of(b"other")));
           assert_eq!(cas.blob(&digest), Some(b"app".to_vec()));
           assert_eq!(cas.digests(), BTreeSet::from([digest]));
       }

       #[test]
       fn content_it_is_told_to_refuse_is_not_stored() {
           let cas = FakeCas::default();
           cas.fail_put(b"app".to_vec());

           let error = cas.put(b"app").unwrap_err();

           assert_eq!(
               error.to_string(),
               "content store: the content cannot be stored"
           );
           assert!(cas.digests().is_empty());
           assert!(cas.put(b"other").is_ok());
       }

       #[test]
       fn placed_content_becomes_the_file_at_its_path() {
           let cas = FakeCas::default();
           let first = cas.put(b"first").unwrap();
           let second = cas.put(b"second").unwrap();
           let app = out_path("//:app", "app");

           cas.place(&first, &app).unwrap();
           cas.place(&second, &app).unwrap();

           assert_eq!(cas.placed(), BTreeMap::from([(app.clone(), second)]));
           assert_eq!(cas.placements(), [(app.clone(), first), (app, second)]);
       }

       #[test]
       fn content_that_is_not_held_cannot_be_placed() {
           let cas = FakeCas::default();
           let app = out_path("//:app", "app");
           let digest = cas.put(b"app").unwrap();
           cas.forget(&digest);

           let error = cas.place(&digest, &app).unwrap_err();

           assert!(!cas.contains(&digest));
           assert_eq!(
               error.to_string(),
               format!("content store: {digest} is not held")
           );
           assert!(cas.placed().is_empty());
       }

       #[test]
       fn a_path_it_is_told_to_fail_on_is_not_written() {
           let cas = FakeCas::default();
           let app = out_path("//:app", "app");
           let map = out_path("//:app", "app.map");
           let digest = cas.put(b"app").unwrap();
           cas.fail_place(app.clone());

           let error = cas.place(&digest, &app).unwrap_err();
           cas.place(&digest, &map).unwrap();

           assert_eq!(
               error.to_string(),
               "content store: out/app cannot be written"
           );
           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::ContentStore),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
           assert_eq!(cas.placements(), [(map, digest)]);
       }

       #[test]
       fn a_log_keeps_every_event_until_it_is_broken() {
           let mut log = FakeLog::default();
           log.append(&event("//:a")).unwrap();
           log.append(&event("//:b")).unwrap();
           assert_eq!(log.events, [event("//:a"), event("//:b")]);

           log.broken = true;
           let error = log.append(&event("//:c")).unwrap_err();

           assert_eq!(error.to_string(), "event log: the log cannot be written");
           assert_eq!(log.events.len(), 2);
       }

       #[test]
       fn every_read_of_the_clock_is_one_tick_later() {
           let clock = FakeClock::default();
           assert_eq!(clock.reads(), 0);

           assert_eq!(clock.now(), Timestamp::from_unix_nanos(1));
           assert_eq!(clock.now(), Timestamp::from_unix_nanos(2));

           assert_eq!(clock.reads(), 2);
       }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --test flows common::
   error[E0432]: unresolved imports `super::FakeCas`, `super::FakeClock`, `super::FakeLog`
   ```

5. Add the fakes. At the top of the same file, widen the imports. Replace:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, DeclarationOrder, DeclarationSource, Diagnostic,
       Digest, Digester, Directory, EnvName, EnvValue, Error, Evaluated, EvaluationFailure, FileStat,
       Freshness, Host, Label, Manifest, NetworkAccess, Platform, PortName, PortSet, Ports, Program,
       Result, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, ToolResolver,
       Workspace, Written,
   };
   ```

   With:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::{BTreeMap, BTreeSet};
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
       DeclarationSource, Diagnostic, Digest, Digester, Directory, EnvName, EnvValue, Error,
       Evaluated, EvaluationFailure, EventLog, FileStat, Freshness, Host, Label, Manifest,
       NetworkAccess, OutcomeEvent, OutputPath, Platform, PortName, PortSet, Ports, Program, Result,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver,
       Workspace, Written,
   };
   ```

6. Put the three fakes directly above the doc comment `/// The bundle choosing the fakes of this file.` Insert:

   ```rust
   /// What a [`FakeCas`] holds, behind its lock.
   #[derive(Debug, Default)]
   struct CasState {
       blobs: BTreeMap<Digest, Vec<u8>>,
       placed: BTreeMap<OutputPath, Digest>,
       placements: Vec<(OutputPath, Digest)>,
       unstorable: BTreeSet<Vec<u8>>,
       unplaceable: BTreeSet<OutputPath>,
   }

   /// A content store held in memory, with the workspace paths content was placed at.
   ///
   /// Actions store what they produce from the threads they run on, so everything it holds sits
   /// behind one lock and every method takes `&self`.
   #[derive(Debug, Default)]
   pub struct FakeCas {
       state: Mutex<CasState>,
   }

   impl FakeCas {
       fn state(&self) -> MutexGuard<'_, CasState> {
           self.state.lock().unwrap_or_else(PoisonError::into_inner)
       }

       /// The content held under `digest`, if any.
       #[must_use]
       pub fn blob(&self, digest: &Digest) -> Option<Vec<u8>> {
           self.state().blobs.get(digest).cloned()
       }

       /// The digest of everything held.
       #[must_use]
       pub fn digests(&self) -> BTreeSet<Digest> {
           self.state().blobs.keys().copied().collect()
       }

       /// What is at each workspace path now: the digest last placed there.
       #[must_use]
       pub fn placed(&self) -> BTreeMap<OutputPath, Digest> {
           self.state().placed.clone()
       }

       /// Every placement that succeeded, in the order made.
       #[must_use]
       pub fn placements(&self) -> Vec<(OutputPath, Digest)> {
           self.state().placements.clone()
       }

       /// Drops the content held under `digest`, as a store that was cleaned would.
       pub fn forget(&self, digest: &Digest) {
           self.state().blobs.remove(digest);
       }

       /// Makes every later attempt to place content at `path` fail.
       pub fn fail_place(&self, path: OutputPath) {
           self.state().unplaceable.insert(path);
       }

       /// Makes every later attempt to store exactly `bytes` fail.
       pub fn fail_put(&self, bytes: Vec<u8>) {
           self.state().unstorable.insert(bytes);
       }
   }

   impl ContentStore for FakeCas {
       fn put(&self, bytes: &[u8]) -> Result<Digest> {
           let mut state = self.state();
           if state.unstorable.contains(bytes) {
               return Err(Error::Adapter {
                   port: PortName::ContentStore,
                   diagnostic: Diagnostic::new("the content cannot be stored"),
               });
           }
           let digest = Digest::of(bytes);
           state.blobs.insert(digest, bytes.to_vec());
           drop(state);
           Ok(digest)
       }

       fn contains(&self, digest: &Digest) -> bool {
           self.state().blobs.contains_key(digest)
       }

       fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()> {
           let mut state = self.state();
           if !state.blobs.contains_key(digest) {
               return Err(Error::Adapter {
                   port: PortName::ContentStore,
                   diagnostic: Diagnostic::new(format!("{digest} is not held")),
               });
           }
           if state.unplaceable.contains(at) {
               return Err(Error::Adapter {
                   port: PortName::ContentStore,
                   diagnostic: Diagnostic::new(format!("{at} cannot be written")),
               });
           }
           state.placed.insert(at.clone(), *digest);
           state.placements.push((at.clone(), *digest));
           drop(state);
           Ok(())
       }
   }

   /// A build log held in memory.
   #[derive(Debug, Default)]
   pub struct FakeLog {
       /// Every event appended, in order.
       pub events: Vec<OutcomeEvent>,
       /// Whether appending fails.
       pub broken: bool,
   }

   impl EventLog for FakeLog {
       fn append(&mut self, event: &OutcomeEvent) -> Result<()> {
           if self.broken {
               return Err(Error::Adapter {
                   port: PortName::EventLog,
                   diagnostic: Diagnostic::new("the log cannot be written"),
               });
           }
           self.events.push(event.clone());
           Ok(())
       }
   }

   /// A clock that moves one nanosecond each time it is read: the first read is `1`, the next `2`.
   ///
   /// Actions read it from the threads they run on, so the count is atomic.
   #[derive(Debug, Default)]
   pub struct FakeClock {
       reads: AtomicU64,
   }

   impl FakeClock {
       /// How many times the clock was read.
       #[must_use]
       pub fn reads(&self) -> u64 {
           self.reads.load(Ordering::SeqCst)
       }
   }

   impl Clock for FakeClock {
       fn now(&self) -> Timestamp {
           Timestamp::from_unix_nanos(self.reads.fetch_add(1, Ordering::SeqCst) + 1)
       }
   }
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs
   $ git commit -m "test(buildl-core): add fakes for the content store, the event log and the clock"
   ```

### Task 5 — Add fakes for the exec strategy and the dispatcher

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing tests. In the `tests` module at the bottom of `crates/buildl-core/tests/flows/common.rs`, widen the imports. Replace:

   ```rust
       use std::collections::{BTreeMap, BTreeSet};
       use std::path::PathBuf;

       use buildl_core::{
           ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Clock, Command, ContentStore,
           Digest, Directory, Error, EventLog, KeyComponents, Label, NetworkAccess, OutcomeClass,
           OutcomeEvent, OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance,
           Timestamp,
       };

       use super::{FakeCache, FakeCas, FakeClock, FakeLog, dir};
   ```

   With:

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
           Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeLog, FakeStrategy, Order,
           Script, dir,
       };
   ```

2. In the same module, add three helpers directly below the `label` helper:

   ```rust
       fn path(raw: &str) -> SourcePath {
           SourcePath::parse(raw).unwrap()
       }

       /// The action of a target that runs `cc` over `inputs` and declares `outputs`.
       fn action(raw: &str, inputs: &[&str], outputs: &[&str]) -> ReadyAction {
           ReadyAction {
               label: label(raw),
               command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
               env: BTreeMap::new(),
               inputs: inputs.iter().map(|input| path(input)).collect(),
               deps: Vec::new(),
               outputs: outputs.iter().map(|name| out_path(raw, name)).collect(),
               network: NetworkAccess::Sealed,
           }
       }

       /// A strategy holding one script, for `//:app`.
       fn scripted(script: Script) -> FakeStrategy {
           FakeStrategy {
               scripts: BTreeMap::from([(label("//:app"), script)]),
               ..FakeStrategy::default()
           }
       }
   ```

3. In the same module, add the tests after the last one, `every_read_of_the_clock_is_one_tick_later`. The helper `dispatched` hands the dispatcher two closures that borrow locals, as the closures of a build do:

   ```rust
       #[test]
       fn an_unscripted_action_succeeds_prints_its_label_and_leaves_every_output() {
           let strategy = FakeStrategy::default();
           let cas = FakeCas::default();
           let app = action("//:app", &[], &["app", "app.map"]);

           let dir = strategy.prepare(&app).unwrap();
           let ran = strategy.run(&dir, &app).unwrap();
           let collected = strategy.collect(&dir, &app, &cas).unwrap();

           assert_eq!(ran.exit, Exit::Exited(0));
           assert_eq!(ran.stdout, b"//:app\n");
           assert_eq!(ran.stderr, b"");
           let binary = Digest::of(b"//:app out/app");
           let map = Digest::of(b"//:app out/app.map");
           assert_eq!(collected, [Some(binary), Some(map)]);
           assert_eq!(cas.digests(), BTreeSet::from([binary, map]));
           assert_eq!(strategy.handed(), [app]);
           assert_eq!(strategy.ran(), [label("//:app")]);
           assert_eq!(strategy.runs(&label("//:app")), 1);
           assert_eq!(strategy.runs(&label("//:other")), 0);
       }

       #[test]
       fn an_unscripted_output_changes_exactly_when_an_input_does() {
           let cas = FakeCas::default();
           let app = action("//:app", &["src/a.c", "src/b.c"], &["app"]);
           let produce = |a: &str| {
               let strategy = FakeStrategy {
                   files: BTreeMap::from([(path("src/a.c"), a.as_bytes().to_vec())]),
                   ..FakeStrategy::default()
               };
               let dir = strategy.prepare(&app).unwrap();
               strategy.collect(&dir, &app, &cas).unwrap()
           };

           let first = produce("one");

           assert_eq!(produce("one"), first);
           assert_ne!(produce("two"), first);
           let text = format!(
               "//:app out/app src/a.c={} src/b.c={}",
               Digest::of(b"one"),
               Digest::of(b"")
           );
           assert_eq!(first, [Some(Digest::of(text.as_bytes()))]);
       }

       #[test]
       fn a_scripted_action_ends_and_prints_as_its_script_says() {
           let app = action("//:app", &[], &["app"]);
           let failing = scripted(Script {
               ends: Ends::Exits(3),
               stdout: Some(b"compiling".to_vec()),
               stderr: Some(b"no such file".to_vec()),
               ..Script::default()
           });
           let slow = scripted(Script {
               ends: Ends::TimesOut,
               ..Script::default()
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
       fn a_strategy_fails_at_the_step_its_script_names() {
           let cas = FakeCas::default();
           let app = action("//:app", &[], &["app"]);
           let with = |ends: Ends| {
               scripted(Script {
                   ends,
                   ..Script::default()
               })
           };

           let no_dir = with(Ends::PrepareFails);
           let error = no_dir.prepare(&app).unwrap_err();
           assert_eq!(
               error.to_string(),
               "exec strategy: //:app: no directory to run in"
           );
           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::ExecStrategy),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
           assert_eq!(no_dir.handed(), std::slice::from_ref(&app));
           assert_eq!(no_dir.runs(&label("//:app")), 0);

           let no_start = with(Ends::RunFails);
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
                   .collect(&dir, &app, &cas)
                   .unwrap_err()
                   .to_string(),
               "exec strategy: //:app: the outputs cannot be read"
           );
           assert!(cas.digests().is_empty());
       }

       #[test]
       fn scripted_outputs_are_stored_a_missing_one_is_not_and_a_short_reply_drops_the_last() {
           let cas = FakeCas::default();
           let app = action("//:app", &[], &["app", "app.map"]);
           let outputs = BTreeMap::from([
               (
                   out_path("//:app", "app"),
                   Content::Holds(b"binary".to_vec()),
               ),
               (out_path("//:app", "app.map"), Content::Missing),
           ]);
           let partial = scripted(Script {
               outputs: outputs.clone(),
               ..Script::default()
           });
           let short = scripted(Script {
               ends: Ends::CollectShort,
               outputs,
               ..Script::default()
           });

           let dir = partial.prepare(&app).unwrap();
           assert_eq!(
               partial.collect(&dir, &app, &cas).unwrap(),
               [Some(Digest::of(b"binary")), None]
           );
           assert_eq!(cas.digests(), BTreeSet::from([Digest::of(b"binary")]));

           let dir = short.prepare(&app).unwrap();
           assert_eq!(short.run(&dir, &app).unwrap().exit, Exit::Exited(0));
           assert_eq!(
               short.collect(&dir, &app, &cas).unwrap(),
               [Some(Digest::of(b"binary"))]
           );
       }

       #[test]
       fn an_output_the_store_refuses_fails_the_collection_as_the_stores_error() {
           let cas = FakeCas::default();
           cas.fail_put(b"//:app out/app".to_vec());
           let strategy = FakeStrategy::default();
           let app = action("//:app", &[], &["app"]);

           let dir = strategy.prepare(&app).unwrap();
           let error = strategy.collect(&dir, &app, &cas).unwrap_err();

           match error {
               Error::Adapter { port, .. } => assert_eq!(port, PortName::ContentStore),
               other => unreachable!("expected Adapter, got {other:?}"),
           }
       }

       /// Dispatches three tasks, the first of which yields a fourth, and returns the results in
       /// the order `step` received them. Both closures borrow locals, as a build's closures do.
       fn dispatched(order: Order) -> Vec<u32> {
           let worked = AtomicU32::new(0);
           let mut results = Vec::new();
           let mut dispatcher = FakeDispatcher { order };

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
               .unwrap();

           assert_eq!(worked.load(Ordering::SeqCst), 4, "every task is run");
           results
       }

       #[test]
       fn tasks_run_one_at_a_time_oldest_first_unless_told_otherwise() {
           assert_eq!(FakeDispatcher::default().order, Order::OldestFirst);
           assert_eq!(dispatched(Order::OldestFirst), [10, 20, 30, 40]);
           assert_eq!(dispatched(Order::NewestFirst), [30, 20, 10, 40]);
       }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --test flows common::
   error[E0432]: unresolved imports `super::Content`, `super::Ends`, `super::FakeDispatcher`, `super::FakeStrategy`, `super::Order`, `super::Script`
   ```

5. Add the fakes. At the top of the same file, widen the imports. Replace:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::{BTreeMap, BTreeSet};
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
       DeclarationSource, Diagnostic, Digest, Digester, Directory, EnvName, EnvValue, Error,
       Evaluated, EvaluationFailure, EventLog, FileStat, Freshness, Host, Label, Manifest,
       NetworkAccess, OutcomeEvent, OutputPath, Platform, PortName, PortSet, Ports, Program, Result,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, Timestamp, ToolResolver,
       Workspace, Written,
   };
   ```

   With:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::{BTreeMap, BTreeSet, VecDeque};
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
       DeclarationSource, Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue,
       Error, Evaluated, EvaluationFailure, EventLog, ExecStrategy, Exit, FileStat, Freshness, Host,
       Label, Manifest, NetworkAccess, OutcomeEvent, OutputPath, Platform, PortName, PortSet, Ports,
       Program, RawOutcome, ReadyAction, Result, SourcePath, StagedAlias, StagedDeclaration,
       StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatCache,
       StatEntry, TargetRole, Timestamp, ToolResolver, Workspace, Written,
   };
   ```

6. Put the fakes directly above the doc comment `/// The bundle choosing the fakes of this file.`, below `FakeClock`. Insert:

   ```rust
   /// How a scripted action goes.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Ends {
       /// The command runs and exits with this status.
       Exits(i32),
       /// The command runs longer than it is allowed to.
       TimesOut,
       /// No directory can be made to run it in.
       PrepareFails,
       /// The command cannot be started.
       RunFails,
       /// The command exits with zero, and then its outputs cannot be read.
       CollectFails,
       /// The command exits with zero, and the strategy then answers for one output too few.
       CollectShort,
   }

   impl Default for Ends {
       fn default() -> Self {
           Self::Exits(0)
       }
   }

   /// What a scripted action leaves at one of its output paths.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Content {
       /// A file holding these bytes.
       Holds(Vec<u8>),
       /// Nothing.
       Missing,
   }

   /// How the action of one target behaves, where that differs from an unscripted one.
   ///
   /// An unscripted action exits with zero, prints its label and a newline to standard output and
   /// nothing to standard error, and leaves at each output path text made of its label, the path
   /// and the digest of each of its inputs — so the output changes exactly when an input does.
   #[derive(Debug, Clone, Default)]
   pub struct Script {
       /// How the action goes.
       pub ends: Ends,
       /// What it writes to standard output; `None` for what an unscripted action writes.
       pub stdout: Option<Vec<u8>>,
       /// What it writes to standard error; `None` for nothing.
       pub stderr: Option<Vec<u8>>,
       /// What it leaves at an output path; a path not named holds what an unscripted action
       /// leaves there.
       pub outputs: BTreeMap<OutputPath, Content>,
   }

   /// The directory a [`FakeStrategy`] runs an action in: nothing, since nothing is run.
   #[derive(Debug)]
   pub struct FakeDir;

   /// A strategy that runs nothing: each action ends, prints and produces what its script says.
   ///
   /// Actions are run from several threads, so what it records sits behind locks.
   #[derive(Debug, Default)]
   pub struct FakeStrategy {
       /// The workspace files an action can read, as the digester holds them.
       pub files: BTreeMap<SourcePath, Vec<u8>>,
       /// The targets whose action is scripted.
       pub scripts: BTreeMap<Label, Script>,
       handed: Mutex<Vec<ReadyAction>>,
       ran: Mutex<Vec<Label>>,
   }

   impl FakeStrategy {
       /// Every action a directory was asked for, in the order asked.
       #[must_use]
       pub fn handed(&self) -> Vec<ReadyAction> {
           self.handed
               .lock()
               .unwrap_or_else(PoisonError::into_inner)
               .clone()
       }

       /// The target of every action that was run, in the order run.
       #[must_use]
       pub fn ran(&self) -> Vec<Label> {
           self.ran
               .lock()
               .unwrap_or_else(PoisonError::into_inner)
               .clone()
       }

       /// How many times the action of `label` was run.
       #[must_use]
       pub fn runs(&self, label: &Label) -> usize {
           self.ran().iter().filter(|ran| *ran == label).count()
       }

       fn script(&self, label: &Label) -> Script {
           self.scripts.get(label).cloned().unwrap_or_default()
       }

       /// What an unscripted action leaves at `path`.
       fn unscripted(&self, action: &ReadyAction, path: &OutputPath) -> Vec<u8> {
           let inputs: Vec<String> = action
               .inputs
               .iter()
               .map(|input| {
                   let bytes = self.files.get(input).map_or(&[][..], Vec::as_slice);
                   format!(" {input}={}", Digest::of(bytes))
               })
               .collect();
           format!("{} {path}{}", action.label, inputs.concat()).into_bytes()
       }
   }

   /// The failure of a [`FakeStrategy`] for the action of `label`.
   fn strategy_failure(label: &Label, what: &str) -> Error {
       Error::Adapter {
           port: PortName::ExecStrategy,
           diagnostic: Diagnostic::new(format!("{label}: {what}")),
       }
   }

   impl ExecStrategy for FakeStrategy {
       type Dir = FakeDir;

       fn prepare(&self, action: &ReadyAction) -> Result<FakeDir> {
           self.handed
               .lock()
               .unwrap_or_else(PoisonError::into_inner)
               .push(action.clone());
           if self.script(&action.label).ends == Ends::PrepareFails {
               return Err(strategy_failure(&action.label, "no directory to run in"));
           }
           Ok(FakeDir)
       }

       fn run(&self, _dir: &FakeDir, action: &ReadyAction) -> Result<RawOutcome> {
           self.ran
               .lock()
               .unwrap_or_else(PoisonError::into_inner)
               .push(action.label.clone());
           let script = self.script(&action.label);
           let exit = match script.ends {
               Ends::RunFails => {
                   return Err(strategy_failure(
                       &action.label,
                       "the command cannot be started",
                   ));
               }
               Ends::TimesOut => Exit::TimedOut,
               Ends::Exits(status) => Exit::Exited(status),
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

       fn collect<C: ContentStore>(
           &self,
           _dir: &FakeDir,
           action: &ReadyAction,
           cas: &C,
       ) -> Result<Vec<Option<Digest>>> {
           let script = self.script(&action.label);
           if script.ends == Ends::CollectFails {
               return Err(strategy_failure(
                   &action.label,
                   "the outputs cannot be read",
               ));
           }
           let mut entries = Vec::new();
           for path in &action.outputs {
               let content = script
                   .outputs
                   .get(path)
                   .cloned()
                   .unwrap_or_else(|| Content::Holds(self.unscripted(action, path)));
               entries.push(match content {
                   Content::Holds(bytes) => Some(cas.put(&bytes)?),
                   Content::Missing => None,
               });
           }
           if script.ends == Ends::CollectShort {
               entries.pop();
           }
           Ok(entries)
       }
   }

   /// Which waiting task a [`FakeDispatcher`] takes next.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum Order {
       /// The one that has waited longest.
       #[default]
       OldestFirst,
       /// The one handed over last.
       NewestFirst,
   }

   /// A dispatcher that runs every task on the calling thread, one at a time.
   #[derive(Debug, Default)]
   pub struct FakeDispatcher {
       /// Which waiting task is taken next.
       pub order: Order,
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
           loop {
               let next = match self.order {
                   Order::OldestFirst => waiting.pop_front(),
                   Order::NewestFirst => waiting.pop_back(),
               };
               let Some(task) = next else {
                   return Ok(());
               };
               waiting.extend(step(Some(work(task))));
           }
       }
   }
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows common::
   test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs
   $ git commit -m "test(buildl-core): add fakes for the exec strategy and the dispatcher"
   ```

### Task 6 — Name every port a build uses in the bundle

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/port_set.rs`

**Steps:**

1. Write the failing implementation. In `crates/buildl-core/tests/flows/common.rs`, make the bundle choose the five new fakes. Replace:

   ```rust
   /// The bundle choosing the fakes of this file.
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
       type Manifest = FakeManifest;
       type Host = FakeHost;
       type Digester = FakeDigester;
       type StatCache = FakeStatCache;
       type Tools = FakeTools;
       type Cache = FakeCache;
   }
   ```

   With:

   ```rust
   /// The bundle choosing the fakes of this file.
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
       type Manifest = FakeManifest;
       type Host = FakeHost;
       type Digester = FakeDigester;
       type StatCache = FakeStatCache;
       type Tools = FakeTools;
       type Cache = FakeCache;
       type Cas = FakeCas;
       type Log = FakeLog;
       type Strategy = FakeStrategy;
       type Dispatcher = FakeDispatcher;
       type Clock = FakeClock;
   }
   ```

2. Run the flows and confirm they fail to build, because `Ports` names seven ports:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0437]: type `Cas` is not a member of trait `Ports`
   error[E0437]: type `Log` is not a member of trait `Ports`
   error[E0437]: type `Strategy` is not a member of trait `Ports`
   error[E0437]: type `Dispatcher` is not a member of trait `Ports`
   error[E0437]: type `Clock` is not a member of trait `Ports`
   ```

3. Extend the bundle. Replace the whole of `crates/buildl-core/src/ports/bundle.rs` with the following. The doc example's `Nothing` now fails wherever it would have to keep something, `record` included, through one helper:

   ````rust
   //! The bundle that names one implementation of every port a pipeline uses.
   //!
   //! Its own file because it is a different kind of trait from the ports themselves: it has no
   //! methods, only associated types, and it exists so that a pipeline takes one type parameter
   //! instead of one per port.
   //!
   //! Responsibilities: [`Ports`].
   //!
   //! Non-responsibilities: choosing the implementations. A composition root does that, by
   //! implementing this trait.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::ports::{
       ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
       ExecStrategy, Host, Manifest, StatCache, ToolResolver,
   };

   /// One implementation of each port, chosen together.
   ///
   /// # Examples
   ///
   /// A bundle for a workspace with no build files, on a machine with nothing in it:
   ///
   /// ```
   /// use std::collections::BTreeSet;
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationSource,
   ///     Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue, Error, Evaluated,
   ///     EventLog, ExecStrategy, FileStat, Host, Label, Manifest, OutcomeEvent, OutputPath,
   ///     Platform, PortName, Ports, Program, RawOutcome, ReadyAction, Result, SourcePath,
   ///     StatCache, StatEntry, Timestamp, ToolResolver, Workspace,
   /// };
   ///
   /// /// Every port at once, each answering that there is nothing.
   /// struct Nothing;
   ///
   /// /// The failure of a port that has nothing to work with.
   /// fn nothing(port: PortName) -> Error {
   ///     Error::Adapter {
   ///         port,
   ///         diagnostic: Diagnostic::new("there is nothing here"),
   ///     }
   /// }
   ///
   /// impl DeclarationSource for Nothing {
   ///     fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
   ///         Ok(Evaluated::Absent)
   ///     }
   /// }
   ///
   /// impl Manifest for Nothing {
   ///     fn workspace(&self) -> Result<Workspace> {
   ///         Ok(Workspace {
   ///             out: Directory::parse("out")?,
   ///             state: Directory::parse(".buildl")?,
   ///             ceiling: Ceiling {
   ///                 run: BTreeSet::new(),
   ///                 env: BTreeSet::new(),
   ///                 read: BTreeSet::new(),
   ///                 write: BTreeSet::new(),
   ///             },
   ///         })
   ///     }
   /// }
   ///
   /// impl Host for Nothing {
   ///     fn platform(&self) -> Platform {
   ///         Platform::parse("linux/x86_64").unwrap_or_else(|_| unreachable!("a valid platform"))
   ///     }
   ///
   ///     fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
   ///         Ok(None)
   ///     }
   /// }
   ///
   /// impl Digester for Nothing {
   ///     fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
   ///         Ok(None)
   ///     }
   ///
   ///     fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ///         Ok(Vec::new())
   ///     }
   /// }
   ///
   /// impl StatCache for Nothing {
   ///     fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
   ///         None
   ///     }
   ///
   ///     fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}
   ///
   ///     fn commit(&mut self) -> Result<()> {
   ///         Ok(())
   ///     }
   /// }
   ///
   /// impl ToolResolver for Nothing {
   ///     fn fingerprint(&self, _program: &Program) -> Result<Option<Digest>> {
   ///         Ok(None)
   ///     }
   /// }
   ///
   /// impl ActionCache for Nothing {
   ///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
   ///         None
   ///     }
   ///
   ///     fn record(&mut self, _label: Label, _row: CacheRow) -> Result<()> {
   ///         Err(nothing(PortName::ActionCache))
   ///     }
   /// }
   ///
   /// impl ContentStore for Nothing {
   ///     fn put(&self, _bytes: &[u8]) -> Result<Digest> {
   ///         Err(nothing(PortName::ContentStore))
   ///     }
   ///
   ///     fn contains(&self, _digest: &Digest) -> bool {
   ///         false
   ///     }
   ///
   ///     fn place(&self, _digest: &Digest, _at: &OutputPath) -> Result<()> {
   ///         Err(nothing(PortName::ContentStore))
   ///     }
   /// }
   ///
   /// impl EventLog for Nothing {
   ///     fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
   ///         Err(nothing(PortName::EventLog))
   ///     }
   /// }
   ///
   /// impl ExecStrategy for Nothing {
   ///     type Dir = ();
   ///
   ///     fn prepare(&self, _action: &ReadyAction) -> Result<()> {
   ///         Err(nothing(PortName::ExecStrategy))
   ///     }
   ///
   ///     fn run(&self, _dir: &(), _action: &ReadyAction) -> Result<RawOutcome> {
   ///         Err(nothing(PortName::ExecStrategy))
   ///     }
   ///
   ///     fn collect<C: ContentStore>(
   ///         &self,
   ///         _dir: &(),
   ///         _action: &ReadyAction,
   ///         _cas: &C,
   ///     ) -> Result<Vec<Option<Digest>>> {
   ///         Err(nothing(PortName::ExecStrategy))
   ///     }
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
   ///     }
   /// }
   ///
   /// impl Clock for Nothing {
   ///     fn now(&self) -> Timestamp {
   ///         Timestamp::UNIX_EPOCH
   ///     }
   /// }
   ///
   /// /// A bundle choosing it for every port.
   /// struct NothingPorts;
   ///
   /// impl Ports for NothingPorts {
   ///     type Source = Nothing;
   ///     type Manifest = Nothing;
   ///     type Host = Nothing;
   ///     type Digester = Nothing;
   ///     type StatCache = Nothing;
   ///     type Tools = Nothing;
   ///     type Cache = Nothing;
   ///     type Cas = Nothing;
   ///     type Log = Nothing;
   ///     type Strategy = Nothing;
   ///     type Dispatcher = Nothing;
   ///     type Clock = Nothing;
   /// }
   /// ```
   pub trait Ports {
       /// How build files are evaluated.
       type Source: DeclarationSource;
       /// How the workspace's manifest is read.
       type Manifest: Manifest;
       /// How the machine actions run on is asked about itself.
       type Host: Host;
       /// How a file's metadata and content digest are read.
       type Digester: Digester;
       /// Where the digests of files already hashed are kept.
       type StatCache: StatCache;
       /// How the programs commands name are found.
       type Tools: ToolResolver;
       /// Where each target's row is read from and written to.
       type Cache: ActionCache;
       /// Where what actions produce is stored, by content.
       type Cas: ContentStore;
       /// Where the record of how each target ended is written.
       type Log: EventLog;
       /// How one action is run.
       type Strategy: ExecStrategy;
       /// How several actions are run at once.
       type Dispatcher: Dispatcher;
       /// How the wall clock is read. `Sync` because an action's start and end are read from the
       /// thread that runs it.
       type Clock: Clock + Sync;
   }
   ````

4. Make the port set's test bundle name them too. Apply this change to the test module of `crates/buildl-core/src/ports/port_set.rs`: wider imports, five more implementations for `Stub`, five more associated types:

   ```diff
   --- a/crates/buildl-core/src/ports/port_set.rs
   +++ b/crates/buildl-core/src/ports/port_set.rs
   @@ -71,14 +71,16 @@ mod tests {
        use super::PortSet;
        use crate::error::Result;
        use crate::ports::{
   -        ActionCache, DeclarationSource, Digester, Host, Manifest, Ports, StatCache, ToolResolver,
   +        ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
   +        ExecStrategy, Host, Manifest, Ports, StatCache, ToolResolver,
        };
        use crate::types::{
   -        BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, FileStat,
   -        Label, Platform, Program, SourcePath, StatEntry, Workspace,
   +        BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
   +        FileStat, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome, ReadyAction,
   +        SourcePath, StatEntry, Timestamp, Workspace,
        };

   -    /// One port type answering that there is nothing, standing in for all seven ports.
   +    /// One port type answering that there is nothing, standing in for every port.
        #[derive(Debug)]
        struct Stub;

   @@ -151,6 +153,69 @@ mod tests {
            }
        }

   +    impl ContentStore for Stub {
   +        fn put(&self, bytes: &[u8]) -> Result<Digest> {
   +            Ok(Digest::of(bytes))
   +        }
   +
   +        fn contains(&self, _digest: &Digest) -> bool {
   +            false
   +        }
   +
   +        fn place(&self, _digest: &Digest, _at: &OutputPath) -> Result<()> {
   +            Ok(())
   +        }
   +    }
   +
   +    impl EventLog for Stub {
   +        fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
   +            Ok(())
   +        }
   +    }
   +
   +    impl ExecStrategy for Stub {
   +        type Dir = ();
   +
   +        fn prepare(&self, _action: &ReadyAction) -> Result<()> {
   +            Ok(())
   +        }
   +
   +        fn run(&self, _dir: &(), _action: &ReadyAction) -> Result<RawOutcome> {
   +            Ok(RawOutcome {
   +                exit: Exit::Exited(0),
   +                stdout: Vec::new(),
   +                stderr: Vec::new(),
   +            })
   +        }
   +
   +        fn collect<C: ContentStore>(
   +            &self,
   +            _dir: &(),
   +            _action: &ReadyAction,
   +            _cas: &C,
   +        ) -> Result<Vec<Option<Digest>>> {
   +            Ok(Vec::new())
   +        }
   +    }
   +
   +    impl Dispatcher for Stub {
   +        fn dispatch<T, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
   +        where
   +            T: Send,
   +            R: Send,
   +            W: Fn(T) -> R + Sync,
   +            S: FnMut(Option<R>) -> Vec<T>,
   +        {
   +            Ok(())
   +        }
   +    }
   +
   +    impl Clock for Stub {
   +        fn now(&self) -> Timestamp {
   +            Timestamp::UNIX_EPOCH
   +        }
   +    }
   +
        /// A bundle choosing [`Stub`] for every port. It deliberately does not implement `Debug`:
        /// a bundle is only a choice of port types, so rendering a set must not require it.
        struct UndebuggableBundle;
   @@ -163,6 +228,11 @@ mod tests {
            type StatCache = Stub;
            type Tools = Stub;
            type Cache = Stub;
   +        type Cas = Stub;
   +        type Log = Stub;
   +        type Strategy = Stub;
   +        type Dispatcher = Stub;
   +        type Clock = Stub;
        }

        #[test]
   ```

5. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows
   test result: ok. 56 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the bundle's doc example and confirm it passes:

   ```
   $ cargo test -p buildl-core --doc ports::bundle
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out
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
   $ git add crates/buildl-core/tests/flows/common.rs crates/buildl-core/src/ports/bundle.rs crates/buildl-core/src/ports/port_set.rs
   $ git commit -m "feat(buildl-core): name every port a build uses in the Ports bundle"
   ```

### Task 7 — Hold every port a build uses in the set

**Files:**
- Modify `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl-core/src/ports/port_set.rs`, the test builds a set of twelve ports and looks for twelve fields. It becomes:

   ```rust
       #[test]
       fn debug_renders_every_port_without_the_bundle_being_debug() {
           let set = PortSet::<UndebuggableBundle> {
               source: Stub,
               manifest: Stub,
               host: Stub,
               digester: Stub,
               stat_cache: Stub,
               tools: Stub,
               cache: Stub,
               cas: Stub,
               log: Stub,
               strategy: Stub,
               dispatcher: Stub,
               clock: Stub,
           };

           let text = format!("{set:?}");

           assert!(text.starts_with("PortSet"), "unexpected rendering: {text}");
           for field in [
               "source",
               "manifest",
               "host",
               "digester",
               "stat_cache",
               "tools",
               "cache",
               "cas",
               "log",
               "strategy",
               "dispatcher",
               "clock",
           ] {
               assert!(
                   text.contains(&format!("{field}: Stub")),
                   "field `{field}` missing from: {text}"
               );
           }
       }
   ```

2. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib ports::port_set
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `cas`
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `log`
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `strategy`
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `dispatcher`
   error[E0560]: struct `PortSet<UndebuggableBundle>` has no field named `clock`
   ```

3. Add the five fields. Above its test module, `crates/buildl-core/src/ports/port_set.rs` reads:

   ```rust
   //! One instance of every port, held together.
   //!
   //! Its own file because it is the value beside the [`Ports`] trait: the trait names one
   //! implementation of each port, and this struct holds one instance of each, so a pipeline is
   //! built from one argument and a phase that needs several ports borrows one value.
   //!
   //! Responsibilities: [`PortSet`], and its `Debug` rendering, which asks only that the twelve
   //! port types be `Debug`.
   //!
   //! Non-responsibilities: building the instances. A composition root does that.

   use std::fmt;

   use crate::ports::Ports;

   /// One instance of each port a bundle names.
   ///
   /// The fields are public: a set has no invariant of its own, and whoever built the adapters may
   /// reach them again between two commands.
   pub struct PortSet<P: Ports> {
       /// Evaluates build files.
       pub source: P::Source,
       /// Reads the workspace's manifest.
       pub manifest: P::Manifest,
       /// Answers for the machine actions run on.
       pub host: P::Host,
       /// Reads a file's metadata and content digest.
       pub digester: P::Digester,
       /// Keeps the digests of files already hashed.
       pub stat_cache: P::StatCache,
       /// Finds the programs commands name.
       pub tools: P::Tools,
       /// Holds each target's row: the previous build's, then this one's.
       pub cache: P::Cache,
       /// Stores what actions produce, by content.
       pub cas: P::Cas,
       /// Receives the record of how each target ended.
       pub log: P::Log,
       /// Runs one action.
       pub strategy: P::Strategy,
       /// Has several actions run at once.
       pub dispatcher: P::Dispatcher,
       /// Reads the wall clock.
       pub clock: P::Clock,
   }

   /// Written by hand because a derive would require the bundle type itself to be `Debug`, and a
   /// bundle is only a choice of port types. This asks only that the twelve port types be `Debug`.
   impl<P: Ports> fmt::Debug for PortSet<P>
   where
       P::Source: fmt::Debug,
       P::Manifest: fmt::Debug,
       P::Host: fmt::Debug,
       P::Digester: fmt::Debug,
       P::StatCache: fmt::Debug,
       P::Tools: fmt::Debug,
       P::Cache: fmt::Debug,
       P::Cas: fmt::Debug,
       P::Log: fmt::Debug,
       P::Strategy: fmt::Debug,
       P::Dispatcher: fmt::Debug,
       P::Clock: fmt::Debug,
   {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.debug_struct("PortSet")
               .field("source", &self.source)
               .field("manifest", &self.manifest)
               .field("host", &self.host)
               .field("digester", &self.digester)
               .field("stat_cache", &self.stat_cache)
               .field("tools", &self.tools)
               .field("cache", &self.cache)
               .field("cas", &self.cas)
               .field("log", &self.log)
               .field("strategy", &self.strategy)
               .field("dispatcher", &self.dispatcher)
               .field("clock", &self.clock)
               .finish()
       }
   }
   ```

4. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib ports::port_set
   test ports::port_set::tests::debug_renders_every_port_without_the_bundle_being_debug ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 348 filtered out
   ```

5. Run the flows and confirm they no longer build, because their set lacks the five ports:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0063]: missing fields `cas`, `clock`, `dispatcher` and 2 other fields in initializer of `PortSet<_>`
   ```

6. Fill the set the flows are built from. In `crates/buildl-core/tests/flows/common.rs`, replace:

   ```rust
   /// Every port around `source`: an [`open_workspace`], a `linux/x86_64` machine with no variable
   /// set, no workspace file, nothing recorded, no program and no row.
   #[must_use]
   pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
       PortSet {
           source,
           manifest: FakeManifest {
               workspace: open_workspace(),
           },
           host: FakeHost {
               platform: Platform::parse("linux/x86_64")
                   .unwrap_or_else(|error| unreachable!("fixture platform: {error}")),
               variables: BTreeMap::new(),
               unreportable: BTreeSet::new(),
           },
           digester: FakeDigester::default(),
           stat_cache: FakeStatCache::default(),
           tools: FakeTools::default(),
           cache: FakeCache::default(),
       }
   }
   ```

   With:

   ```rust
   /// Every port around `source`, each holding nothing.
   ///
   /// An [`open_workspace`]; a `linux/x86_64` machine with no variable set; no workspace file,
   /// nothing recorded, no program and no row; an empty content store and an empty log; a strategy
   /// with no script; a dispatcher taking the oldest task first; and a clock that has not been
   /// read.
   #[must_use]
   pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
       PortSet {
           source,
           manifest: FakeManifest {
               workspace: open_workspace(),
           },
           host: FakeHost {
               platform: Platform::parse("linux/x86_64")
                   .unwrap_or_else(|error| unreachable!("fixture platform: {error}")),
               variables: BTreeMap::new(),
               unreportable: BTreeSet::new(),
           },
           digester: FakeDigester::default(),
           stat_cache: FakeStatCache::default(),
           tools: FakeTools::default(),
           cache: FakeCache::default(),
           cas: FakeCas::default(),
           log: FakeLog::default(),
           strategy: FakeStrategy::default(),
           dispatcher: FakeDispatcher::default(),
           clock: FakeClock::default(),
       }
   }
   ```

7. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows
   test result: ok. 56 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

9. Commit:

   ```
   $ git add crates/buildl-core/src/ports/port_set.rs crates/buildl-core/tests/flows/common.rs
   $ git commit -m "feat(buildl-core): hold every port a build uses in the PortSet"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`; `crates/buildl-core/Cargo.toml` and
  `crates/expected-module-edges.txt` are unchanged.
- `ContentStore`, `EventLog`, `ExecStrategy` and `Dispatcher` have the signatures of spec §4, and
  every fallible method returns the crate's `Result`. `ActionCache` has `row` and `record`.
- `Ports` names twelve ports, `Clock` bounded `Clock + Sync`; `PortSet` holds one instance of
  each, and its `Debug` asks only that the twelve port types be `Debug`.
- Each fake is implemented from the public API alone and passes the tests that hold it to its
  port's contract: a row that cannot be recorded leaves what was there; content is held under its
  digest and placed at a path; a broken log keeps nothing more; the clock moves one tick per
  read; a strategy ends, prints and produces what its script says and fails at the step the
  script names; the dispatcher runs every task, oldest or newest first, with closures that borrow
  locals.
- The crate holds no production implementation of any port, and the 40 flows it had still pass.
- What the flows of a build are written against:

  | Fake | Public API |
  |---|---|
  | `FakeCas` (`Default`) | `blob(&self, &Digest) -> Option<Vec<u8>>`, `digests(&self) -> BTreeSet<Digest>`, `placed(&self) -> BTreeMap<OutputPath, Digest>`, `placements(&self) -> Vec<(OutputPath, Digest)>`, `forget(&self, &Digest)`, `fail_place(&self, OutputPath)`, `fail_put(&self, Vec<u8>)` |
  | `FakeCache` (`Default`) | `pub rows: BTreeMap<Label, CacheRow>`, `pub failing: BTreeSet<Label>`, `recorded(&self) -> Vec<Label>` |
  | `FakeLog` (`Default`) | `pub events: Vec<OutcomeEvent>`, `pub broken: bool` |
  | `FakeClock` (`Default`) | `reads(&self) -> u64`; `now` returns 1, 2, 3, … nanoseconds |
  | `FakeStrategy` (`Default`; `Dir = FakeDir`) | `pub files: BTreeMap<SourcePath, Vec<u8>>`, `pub scripts: BTreeMap<Label, Script>`, `handed(&self) -> Vec<ReadyAction>`, `ran(&self) -> Vec<Label>`, `runs(&self, &Label) -> usize` |
  | `Script` (`Default`) | `pub ends: Ends`, `pub stdout: Option<Vec<u8>>`, `pub stderr: Option<Vec<u8>>`, `pub outputs: BTreeMap<OutputPath, Content>` |
  | `Ends` (`Default` is `Exits(0)`) | `Exits(i32)`, `TimesOut`, `PrepareFails`, `RunFails`, `CollectFails`, `CollectShort` |
  | `Content` | `Holds(Vec<u8>)`, `Missing` |
  | `FakeDispatcher` (`Default`) | `pub order: Order`; `Order::{OldestFirst, NewestFirst}`, `OldestFirst` by default |

## Review findings

Reviewed once, 2026-10-12, over all seven tasks. The reviewer re-ran the gate: `cargo make dod`
exit 0 (`[cargo-make] INFO - Build Done in 6.13 seconds.`), `cargo deny check` exit 0
(`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan: every
final-state code block is in the delivered files verbatim, the six whole-file blocks
byte-identical; nothing missing, nothing extra.

- nit — the `Clock + Sync` bound on `Ports::Clock` and the `Send + Sync` supertraits of
  `ContentStore` and `ExecStrategy` are unguarded inside this plan: dropping one fails nothing
  today — `ports/bundle.rs`, `ports/content_store.rs:21`, `ports/exec_strategy.rs:28`. Left as it
  is: plan 09's worker closure borrows these ports inside `dispatch`, which needs the bounds.
- nit — "twelve port types" is a count the next port invalidates — `ports/port_set.rs:7,48`.
  Left as it is: the count is right today and spec §4 words it this way.
- nit — spec §9 says a fake output's default text comes from "the output's name" and input
  digests "as the fake digester holds them"; the plan and the code use the output's path, and
  `FakeStrategy.files` is its own map a flow fills beside `FakeDigester.files` —
  `tests/flows/common.rs`. Left as it is: the code equals the plan, and the property the spec
  wants (an output changes exactly when an input does) holds and is tested.

## Probe results

No separate probe was run. The facts the tasks assert about existing code are ones their own
compile and test runs depend on:

- Task 3 red: `cargo test -p buildl-core --test flows common::` failed to build with
  `error[E0407]: method `record` is not a member of trait `ActionCache``, as the plan quotes,
  plus four `E0599` lines the same missing method causes. Green: `2 passed; … 40 filtered out`;
  `cargo test -p buildl-core --doc ports::bundle`: `1 passed; … 5 filtered out`.
- Task 4 red: `error[E0432]: unresolved imports `super::FakeCas`, `super::FakeClock`,
  `super::FakeLog``. Green: `9 passed; … 40 filtered out`.
- Task 5 red: `error[E0432]: unresolved imports `super::Content`, `super::Ends`,
  `super::FakeDispatcher`, `super::FakeStrategy`, `super::Order`, `super::Script``. Green:
  `16 passed; … 40 filtered out`.
- Task 6 red: five `error[E0437]` lines, one each for `Cas`, `Log`, `Strategy`, `Dispatcher`,
  `Clock`. Green: `cargo test -p buildl-core --test flows`: `56 passed; … 0 filtered out`.
- Task 7 red: five `error[E0560]` lines for the lib test, then `error[E0063]: missing fields
  `cas`, `clock`, `dispatcher` and 2 other fields` for the flows. Green: `1 passed; …
  348 filtered out` and `56 passed; … 0 filtered out`.

Every one of these matches what the plan quotes.

## Deviations

- 2026-10-12 — the commit step of all seven tasks was skipped. The work is in the tree,
  uncommitted; committing is left to the author.
- 2026-10-12 — started while plans 01–04, which this plan depends on, were built and reviewed
  but not yet marked `done`: the author asked for plans 01–05 in one run.
