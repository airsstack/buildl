---
status: done
created: 2026-10-14
depends-on: [04]
---

# Run Adapters Implementation Plan

**Goal:** Each of the three ports that run a build's actions — `ExecStrategy`, `Dispatcher`, `Clock` — has a real adapter in `buildl`.

**Architecture:** Three adapter modules of the `buildl` crate, each implementing one port of `buildl-core` with std alone. `clock/` is `SystemClock`, the only place the wall clock is read. `dispatch/` is `ThreadPool`: one `std::thread::scope` per dispatch, tasks sent to the workers over one `mpsc` channel behind a `Mutex`, and what a worker reports — something a running task said, a result, or a task lost to a panic — sent back over another; a worker is started only when a task is handed out while every worker there is has one, so a build of cached targets starts no thread; a panic is caught on the worker, so the pool never unwinds and the pipeline, finding a target without an outcome, ends the build as `Error::DispatcherFault`. `exec/` is `Bare`, the tier 1 strategy, in three files: `outputs.rs` reports what stands at one output path, following no symbolic link and storing every regular file through whatever `ContentStore` it is handed; `capture.rs` runs one process with an empty standard input and both outputs piped, each read by a detached thread, while the calling thread — the only one that touches the child — waits with `try_wait` against a deadline, kills the child when the deadline passes, and gives an output something else still holds open one second to drain; `bare.rs` puts them behind the port: `prepare` makes each output's directory and unlinks what is at the path through the shared `state::clear_path`, `run` starts the argument vector with its program resolved through the shared `SearchPath`, the workspace root as working directory and exactly the action's environment, and `collect` reports each output in declared order. The three modules name `crate::state` and `crate::search_path` and no other module of the crate. Every process a test starts is a system tool run by argument vector; `sh -c` appears in tests only.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, std (`std::thread`, `std::sync::mpsc`, `std::process`, `std::os::unix`), `buildl-core` (the ports and their values), `tempfile` in tests, `cargo-make`. The tests run the system's `sh`, `pwd`, `env`, `cat`, `head`, `sleep` and `mkfifo`. No new dependency.

**Content authority:** spec §6.1 (`exec/`), §6.2 (the guarantees `Bare`'s documentation states), §6.3 (`dispatch/`), §6.4 (`clock/`), §6.5 (what `prepare` leaves after an interrupted build), §3 (what an adapter module may name), §11 (the rows raised by `dispatch/` and by a strategy), §12.1 and §12.2 (the tests of `exec/`, `dispatch/` and `clock/`), §14 (no dependency), §17 (the rows proved on Linux by these tests).

---

## Context an implementer needs

What the plan `04-buildl-foundation` leaves that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `pub mod clock;`, `pub mod dispatch;`, `pub mod exec;`, `pub mod search_path;`, `pub mod state;` | `crates/buildl/src/lib.rs` | the three modules are already declared; this plan never edits `lib.rs` |
| a `mod.rs` holding a `//!` doc comment and nothing else | `crates/buildl/src/clock/mod.rs`, `dispatch/mod.rs`, `exec/mod.rs` | tasks 1, 2 and 10 replace the whole file; tasks 5, 6 and 9 add lines below what is there |
| `pub(crate) fn clear_path(path: &Path) -> io::Result<()>`: removes whatever is at `path`, following no symbolic link; nothing being there is `Ok` | `crates/buildl/src/state/`, reached as `crate::state::clear_path` | how `Bare::prepare` clears an output path |
| `pub struct SearchPath` with `pub fn new(directories: impl IntoIterator<Item = PathBuf>) -> Self` (a directory that is not absolute is left out) and `pub fn locate(&self, program: &Program, base: &Path) -> Option<PathBuf>` | `crates/buildl/src/search_path.rs`, reached as `crate::search_path::SearchPath` | how `Bare::run` finds the program; tests build one over `/bin` and `/usr/bin`, over two temp directories, and over none |
| `tempfile` among the dependencies of `buildl` | `crates/buildl/Cargo.toml` | `tempfile::tempdir()` in tests |
| the `guard-adapter-isolation` task, run by `cargo make dod` | `Makefile.toml` | checks the imports of every file this plan writes |

What `buildl-core` gives, as the plans `01` to `03` leave it:

| Item | Used as |
|---|---|
| `trait Clock { fn now(&self) -> Timestamp; }`; `Timestamp::from_unix_nanos(u64)`, `duration_since(self, earlier) -> Option<Duration>` | task 1 |
| `trait Dispatcher { fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()> where T: Send, M: Send, R: Send, W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync, S: FnMut(Turn<M, R>) -> Vec<T>; }`; `enum Turn<M, R> { Begin, Told(M), Done(R) }`; `Workers::{PerCore, Fixed}`, `Workers::of(usize)`, `Workers::on(self, cores: NonZeroUsize) -> NonZeroUsize` | tasks 2 to 4 |
| `trait ContentStore: Send + Sync` with `put(&self, &[u8]) -> Result<Digest>`, `contains`, `get(&self, &Digest) -> Result<Option<Vec<u8>>>`, `clear(&self, &OutputPath) -> Result<()>`, `place(&self, &Digest, FileMode, &OutputPath) -> Result<()>` | task 5, and what `collect` stores through |
| `Found::{Nothing, File(StoredFile), Tree(Vec<FoundEntry>), Symlink, Special}`; `FoundEntry { path: String, kind: FoundKind }` (`Ord`); `FoundKind::{File(StoredFile), EmptyDirectory, Symlink, Special, NotText}`; `StoredFile { digest, mode }`; `FileMode::{Plain, Executable}` | tasks 6 to 8 |
| `trait ExecStrategy: Send + Sync` with `type Dir`, `prepare(&self, &ReadyAction) -> Result<Self::Dir>`, `run(&self, &Self::Dir, &ReadyAction, Option<LiveSink<'_>>) -> Result<RawOutcome>`, `collect<C: ContentStore>(&self, &Self::Dir, &ReadyAction, &C) -> Result<Vec<Found>>` | task 10 |
| `ReadyAction { label, command, description, env, inputs, deps, outputs, network }`, every field public; `Command::arguments() -> &[Argument]`; `Program::of(&Argument) -> Option<Program>` | tasks 10 to 12 |
| `RawOutcome { exit, stdout, stderr }`; `Exit::{Exited(i32), Signaled(i32), TimedOut}`; `Timeout::as_duration()`; `Channel::{Stdout, Stderr}`; `type LiveSink<'a> = &'a (dyn Fn(Channel, &[u8]) + Sync)` | tasks 9 to 15 |
| `Error::Adapter { port: PortName, diagnostic: Diagnostic }`, shown as `{port}: {diagnostic}`; `PortName::ExecStrategy` shows as `exec strategy`, `PortName::Dispatcher` as `dispatcher`, `PortName::ContentStore` as `content store` | every error these adapters raise |

Gate facts that bite here:

- **Imports.** `guard-adapter-isolation` reads every line of these files, tests included. A sibling
  file of the same module is named `crate::exec::capture::…`, never through `super::super`; a
  `mod.rs` never writes `super::`; nothing is imported as `crate::{…}` or as `crate::<Item>`. A
  test module names its own file's items through `use super::…;`, which the guard allows in a
  file that is not a `mod.rs`.
- **Dead code between tasks.** `exec/outputs.rs` (tasks 6 to 8) and `exec/capture.rs` (task 9) have
  no caller until `Bare` exists, so each opens with an `expect(dead_code)` attribute, and task 10
  removes both. `expect` rather than `allow`: the attribute becomes an error the moment the code
  is used, so it cannot be left behind. `outputs.rs` expects it in every build, because its error
  type's fields are read by nobody, tests included, until task 10; `capture.rs` expects it outside
  tests only, because its tests read everything it returns.
- **A trait is implemented whole.** `ExecStrategy` has three methods and an implementation must
  hold all of them, so task 10 brings `prepare`, `run` and `collect` together with one test or
  more for each. Tasks 11 to 15 then change `run` one behaviour at a time.
- **Signatures grow.** `Bare::new` gains its search path in task 11 and its timeout in task 13;
  `capture` gains its timeout in task 13 and its live sink in task 15. Each test file keeps one
  helper that builds the value, so a task that adds a parameter changes the helper and the two or
  three tests that build one directly, and those edits are spelled out.
- **Three failing runs are slow by design.** The failing run of task 4 takes 20 seconds, of
  task 14 about 30, and of task 15 about 20: each waits for something the code does not do yet.
  The passing runs take a second together.
- **One test runs on Linux alone.** `collect_reports_a_name_that_is_not_unicode` is gated with
  `#[cfg(target_os = "linux")]`: macOS refuses to create a file whose name is not Unicode
  (`Illegal byte sequence (os error 92)`). On macOS it is compiled out, and task 8 says what to
  expect on each system.
- **The `filtered out` figure.** Every quoted test result was captured in a copy of the crate
  that held this plan's tests alone. In the repository the `filtered out` figure is larger by
  the tests of the other modules; the `passed` figure is the one to compare.
- **Test processes.** A test's workspace root is always passed as a value; no test changes the
  process's directory or sets an environment variable. In a script, a tool is named by its
  absolute path, because the child has no `PATH`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was built task by task. A failing run is quoted by the lines that name the cause, without the line numbers of a panic, which move as the files grow.

## File structure

```
crates/buildl/src/clock/mod.rs             — [modify] declare and re-export SystemClock
crates/buildl/src/clock/system_clock.rs    — [create] SystemClock: Clock
crates/buildl/src/dispatch/mod.rs          — [modify] declare and re-export ThreadPool
crates/buildl/src/dispatch/thread_pool.rs  — [create] ThreadPool: Dispatcher
crates/buildl/src/exec/mod.rs              — [modify] declare the four files, re-export Bare
crates/buildl/src/exec/memory_store.rs     — [create] MemoryStore, the content store of this module's tests
crates/buildl/src/exec/outputs.rs          — [create] found, Unread: what stands at one output path
crates/buildl/src/exec/capture.rs          — [create] capture, Capture: one process run to its end
crates/buildl/src/exec/bare.rs             — [create] Bare: ExecStrategy
```

The gate is run the same way at the end of every task: `cargo fmt --all -- --check`, `cargo make dod` and `cargo deny check`, from the repository root, all three exiting `0`. `cargo fmt --all -- --check` prints nothing; `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`; `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

### Task 1 — Add the system clock

**Files:**
- Create `crates/buildl/src/clock/system_clock.rs`
- Modify `crates/buildl/src/clock/mod.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl/src/clock/system_clock.rs` with its module doc and its test:

   ```rust
   //! The system clock as a build reads it.
   //!
   //! Its own file because it is the only place in the crate where the wall clock is read.
   //!
   //! Responsibilities: [`SystemClock`].

   #[cfg(test)]
   mod tests {
       use buildl_core::{Clock, Timestamp};

       use super::SystemClock;

       #[test]
       fn the_clock_reads_after_the_epoch_and_does_not_run_backwards_between_two_reads() {
           let clock = SystemClock;
           let first = clock.now();
           let second = clock.now();
           assert!(first > Timestamp::from_unix_nanos(0));
           assert!(second.duration_since(first).is_some());
       }
   }
   ```

2. Declare the file. Replace the whole of `crates/buildl/src/clock/mod.rs` with:

   ```rust
   //! The host wall clock, behind the port a build reads the time through.
   //!
   //! Its own module because reading the time is the one thing here that no test can pin: a build
   //! reads it only to stamp when an action started and finished, and nothing a build stores or
   //! compares depends on the value. Keeping the read in one adapter is what lets every other
   //! module stay free of it.
   //!
   //! Responsibilities: [`SystemClock`], the implementation of `buildl_core::Clock`.
   //!
   //! Non-responsibilities: measuring how long an action may run. That is a monotonic reading the
   //! module that runs actions takes for itself; it reaches no key, file or log.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod system_clock;

   pub use system_clock::SystemClock;
   ```

3. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl --lib clock::
   error[E0432]: unresolved import `super::SystemClock`
   error[E0432]: unresolved import `system_clock::SystemClock`
   ```

4. Add the clock. In `crates/buildl/src/clock/system_clock.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   use std::time::{SystemTime, UNIX_EPOCH};

   use buildl_core::{Clock, Timestamp};

   /// Reads the host wall clock.
   ///
   /// A clock set before 1970 reads as the epoch, and one past the year 2554, where a `u64` of
   /// nanoseconds ends, reads as that last instant.
   #[derive(Debug, Clone, Copy, Default)]
   pub struct SystemClock;

   impl Clock for SystemClock {
       fn now(&self) -> Timestamp {
           let since = SystemTime::now()
               .duration_since(UNIX_EPOCH)
               .unwrap_or_default();
           Timestamp::from_unix_nanos(u64::try_from(since.as_nanos()).unwrap_or(u64::MAX))
       }
   }
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl --lib clock::
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/clock/mod.rs crates/buildl/src/clock/system_clock.rs
   $ git commit -m "feat(buildl): add the system clock adapter"
   ```

### Task 2 — Add the thread pool

The pool of this task runs every task and hands back every result, on a bounded number of threads started as tasks need them. What a running task says is dropped until task 3, and a task that panics takes its worker with it until task 4.

**Files:**
- Create `crates/buildl/src/dispatch/thread_pool.rs`
- Modify `crates/buildl/src/dispatch/mod.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl/src/dispatch/thread_pool.rs` with its module doc and its tests. Three of them call `serve`, the private function behind `dispatch`, because it returns how many threads it started, which the port does not:

   ```rust
   //! A pool of worker threads that lives for one dispatch.
   //!
   //! Its own file because it is the only place in the crate where a thread that runs an action is
   //! started. The threads are scoped: what a task borrows — the strategy, the store, the clock —
   //! only has to live as long as the dispatch, and every worker has ended when the dispatch
   //! returns.
   //!
   //! Responsibilities: [`ThreadPool`].

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::num::NonZeroUsize;
       use std::sync::Mutex;
       use std::sync::atomic::{AtomicUsize, Ordering};
       use std::thread;
       use std::time::Duration;

       use buildl_core::{Dispatcher, Turn, Workers};

       use super::{ThreadPool, serve};

       fn limit(workers: usize) -> NonZeroUsize {
           NonZeroUsize::new(workers).unwrap()
       }

       /// A task that says nothing and returns twice its number.
       fn doubled(task: u32, _tell: &(dyn Fn(()) + Sync)) -> u32 {
           task * 2
       }

       #[test]
       fn every_task_is_run_and_every_result_reaches_step() {
           let mut pool = ThreadPool::new(Workers::of(4));
           let mut results = Vec::new();

           // Each of the first five results hands out one more task, as a finished target releases
           // the targets that waited on it.
           pool.dispatch(doubled, |turn| match turn {
               Turn::Begin => (0..20).collect(),
               Turn::Told(()) => Vec::new(),
               Turn::Done(result) => {
                   results.push(result);
                   if result < 10 {
                       vec![1000 + result]
                   } else {
                       Vec::new()
                   }
               }
           })
           .unwrap();

           results.sort_unstable();
           let mut expected: Vec<u32> = (0..20).map(|task| task * 2).collect();
           expected.extend([2000, 2004, 2008, 2012, 2016]);
           assert_eq!(results, expected);
       }

       #[test]
       fn step_is_only_called_on_the_calling_thread() {
           let caller = thread::current().id();
           let mut pool = ThreadPool::new(Workers::of(4));
           let worked_on = Mutex::new(Vec::new());
           let mut stepped_on = Vec::new();

           pool.dispatch(
               |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                   worked_on.lock().unwrap().push(thread::current().id());
                   task
               },
               |turn| {
                   stepped_on.push(thread::current().id());
                   match turn {
                       Turn::Begin => (0..12).collect(),
                       Turn::Told(()) | Turn::Done(_) => Vec::new(),
                   }
               },
           )
           .unwrap();

           assert_eq!(stepped_on.len(), 13);
           assert!(stepped_on.iter().all(|thread| *thread == caller));
           let worked_on = worked_on.into_inner().unwrap();
           assert_eq!(worked_on.len(), 12);
           assert!(worked_on.iter().all(|thread| *thread != caller));
       }

       #[test]
       fn no_more_tasks_run_at_once_than_there_are_workers() {
           let running = AtomicUsize::new(0);
           let most = AtomicUsize::new(0);

           let started = serve(
               limit(2),
               &|task: u32, _tell: &(dyn Fn(()) + Sync)| {
                   let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                   most.fetch_max(now, Ordering::SeqCst);
                   thread::sleep(Duration::from_millis(2));
                   running.fetch_sub(1, Ordering::SeqCst);
                   task
               },
               |turn| match turn {
                   Turn::Begin => (0..16).collect(),
                   Turn::Told(()) | Turn::Done(_) => Vec::new(),
               },
           )
           .unwrap();

           // Sixteen tasks were handed out at once, so both workers were started and no third.
           assert_eq!(started, 2);
           assert!(most.load(Ordering::SeqCst) <= 2);
       }

       #[test]
       fn no_task_starts_no_thread() {
           let mut turns = 0;

           let started = serve(limit(4), &doubled, |_turn| {
               turns += 1;
               Vec::new()
           })
           .unwrap();

           assert_eq!(started, 0);
           assert_eq!(turns, 1);
       }

       #[test]
       fn one_task_at_a_time_uses_one_thread() {
           let mut results = Vec::new();

           // Each result hands out the next task, so no two tasks ever wait together.
           let started = serve(limit(8), &doubled, |turn| match turn {
               Turn::Begin => vec![0],
               Turn::Told(()) => Vec::new(),
               Turn::Done(result) => {
                   results.push(result);
                   if result < 18 {
                       vec![result / 2 + 1]
                   } else {
                       Vec::new()
                   }
               }
           })
           .unwrap();

           assert_eq!(started, 1);
           assert_eq!(results, [0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
       }

       #[test]
       fn one_worker_runs_tasks_in_the_order_they_were_handed_out() {
           let mut pool = ThreadPool::new(Workers::of(1));
           let order = Mutex::new(Vec::new());

           pool.dispatch(
               |task: u32, _tell: &(dyn Fn(()) + Sync)| order.lock().unwrap().push(task),
               |turn| match turn {
                   Turn::Begin => (0..50).collect(),
                   Turn::Told(()) | Turn::Done(()) => Vec::new(),
               },
           )
           .unwrap();

           assert_eq!(order.into_inner().unwrap(), (0..50).collect::<Vec<u32>>());
       }

       #[test]
       fn workers_per_core_is_the_machines_parallelism() {
           let cores = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

           assert_eq!(ThreadPool::new(Workers::PerCore).workers, cores);
           assert_eq!(ThreadPool::new(Workers::of(3)).workers, limit(3));
       }
   }
   ```

2. Declare the file. Replace the whole of `crates/buildl/src/dispatch/mod.rs` with:

   ```rust
   //! The worker threads a build runs its actions on.
   //!
   //! Its own module because starting a thread is something only this crate may do: the pipeline
   //! says what to do with one task and with one result, through `buildl_core::Dispatcher`, and
   //! this adapter decides how many tasks are worked on at a time.
   //!
   //! Responsibilities: [`ThreadPool`], the implementation of `buildl_core::Dispatcher`.
   //!
   //! Non-responsibilities: knowing what a task is, stopping a build at a failure, and cancelling
   //! anything that runs. The pool runs every task it is handed and hands back every result; what a
   //! result means for the rest of a build is the pipeline's to decide.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod thread_pool;

   pub use thread_pool::ThreadPool;
   ```

3. Run the tests and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib dispatch::
   error[E0432]: unresolved imports `super::ThreadPool`, `super::serve`
   error[E0432]: unresolved import `thread_pool::ThreadPool`
   ```

4. Add the pool. In `crates/buildl/src/dispatch/thread_pool.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   use std::num::NonZeroUsize;
   use std::sync::mpsc::{self, Receiver, Sender};
   use std::sync::{Mutex, PoisonError};
   use std::thread;

   use buildl_core::{Diagnostic, Dispatcher, Error, PortName, Result, Turn, Workers};

   /// Runs tasks on up to a fixed number of threads, started as tasks need them.
   ///
   /// A dispatch that is handed no task starts no thread, and one whose tasks come one at a time
   /// starts one. No thread outlives the dispatch that started it.
   #[derive(Debug)]
   pub struct ThreadPool {
       workers: NonZeroUsize,
   }

   impl ThreadPool {
       /// As many workers as `workers` says for this machine: one per core for
       /// [`Workers::PerCore`], and one where the number of cores cannot be read.
       #[must_use]
       pub fn new(workers: Workers) -> Self {
           let cores = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
           Self {
               workers: workers.on(cores),
           }
       }
   }

   impl Dispatcher for ThreadPool {
       fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>
       where
           T: Send,
           M: Send,
           R: Send,
           W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
           S: FnMut(Turn<M, R>) -> Vec<T>,
       {
           serve(self.workers, &work, step).map(|_started| ())
       }
   }

   /// Runs every task `step` yields through `work`, on up to `limit` threads, and returns how many
   /// threads it started.
   ///
   /// A thread is started only when a task is handed out while every thread there is has one, so
   /// the count is the most tasks that ever waited or ran together, up to `limit`. Tasks leave in
   /// the order `step` returned them.
   fn serve<T, M, R, W, S>(limit: NonZeroUsize, work: &W, mut step: S) -> Result<usize>
   where
       T: Send,
       M: Send,
       R: Send,
       W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
       S: FnMut(Turn<M, R>) -> Vec<T>,
   {
       let mut ready = step(Turn::Begin);
       if ready.is_empty() {
           return Ok(0);
       }
       let (tasks, queue) = mpsc::channel::<T>();
       let queue = Mutex::new(queue);
       let (back, results) = mpsc::channel::<R>();
       thread::scope(|scope| {
           // Owned by this closure, so it is dropped when the closure returns on any path: each
           // worker's `recv` then fails, the worker ends, and the scope joins it.
           let tasks = tasks;
           let mut started = 0_usize;
           let mut running = 0_usize;
           loop {
               for task in ready {
                   running += 1;
                   if started < limit.get() && started < running {
                       let (queue, back) = (&queue, back.clone());
                       let spawned = thread::Builder::new()
                           .name(format!("buildl-worker-{started}"))
                           .spawn_scoped(scope, move || worker(queue, work, &back));
                       match spawned {
                           Ok(_) => started += 1,
                           Err(error) if started == 0 => {
                               return Err(Error::Adapter {
                                   port: PortName::Dispatcher,
                                   diagnostic: Diagnostic::new(format!(
                                       "starting a worker thread: {error}"
                                   )),
                               });
                           }
                           // The workers there are take the task once one of them is free.
                           Err(_) => {}
                       }
                   }
                   // The queue outlives the scope, so this cannot fail.
                   let _ = tasks.send(task);
               }
               if running == 0 {
                   return Ok(started);
               }
               // This function keeps a sender of its own, so the channel cannot close.
               let Ok(result) = results.recv() else {
                   return Ok(started);
               };
               running -= 1;
               ready = step(Turn::Done(result));
           }
       })
   }

   /// One worker thread: takes tasks off `queue` until it closes, and hands back each result.
   fn worker<T, M, R, W>(queue: &Mutex<Receiver<T>>, work: &W, back: &Sender<R>)
   where
       W: Fn(T, &(dyn Fn(M) + Sync)) -> R,
   {
       loop {
           // The lock is held to take one task and released before the task runs.
           let next = queue.lock().unwrap_or_else(PoisonError::into_inner).recv();
           let Ok(task) = next else {
               return;
           };
           // What a task says goes nowhere.
           let result = work(task, &|_message: M| {});
           if back.send(result).is_err() {
               return;
           }
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib dispatch::
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/dispatch/mod.rs crates/buildl/src/dispatch/thread_pool.rs
   $ git commit -m "feat(buildl): add the thread pool dispatcher"
   ```

### Task 3 — Carry what a running task says to the calling thread

**Files:**
- Modify `crates/buildl/src/dispatch/thread_pool.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl/src/dispatch/thread_pool.rs`, after the last test, `workers_per_core_is_the_machines_parallelism`, with one blank line above, add:

   ```rust
       #[test]
       fn what_a_task_says_reaches_step_before_its_result_and_in_order() {
           let mut pool = ThreadPool::new(Workers::of(4));
           let mut log: Vec<(u32, u32)> = Vec::new();

           // Every task says three things, the last from a thread of its own, and its result is
           // logged as a fourth. The first thing tasks 0 and 1 say hands out one more task each.
           pool.dispatch(
               |task: u32, tell: &(dyn Fn((u32, u32)) + Sync)| {
                   tell((task, 0));
                   tell((task, 1));
                   thread::scope(|scope| {
                       scope.spawn(|| tell((task, 2)));
                   });
                   task
               },
               |turn| match turn {
                   Turn::Begin => (0..8).collect(),
                   Turn::Told(said) => {
                       log.push(said);
                       match said {
                           (task @ (0 | 1), 0) => vec![100 + task],
                           _ => Vec::new(),
                       }
                   }
                   Turn::Done(task) => {
                       log.push((task, 3));
                       Vec::new()
                   }
               },
           )
           .unwrap();

           for task in (0..8).chain([100, 101]) {
               let of_task: Vec<u32> = log
                   .iter()
                   .filter(|(from, _)| *from == task)
                   .map(|(_, what)| *what)
                   .collect();
               assert_eq!(of_task, [0, 1, 2, 3], "task {task}");
           }
           assert_eq!(log.len(), 40);
       }
   ```

2. Run it and confirm it fails, because what a task says goes nowhere and only its result arrives:

   ```
   $ cargo test -p buildl --lib dispatch::
   assertion `left == right` failed: task 0
     left: [3]
    right: [0, 1, 2, 3]
   test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out
   ```

3. Give a worker three things to hand back. In the same file, directly above the doc comment that starts ``/// Runs every task `step` yields through `work`, on up to `limit` threads``, with one blank line below, insert:

   ```rust
   /// What a worker hands back.
   enum Report<M, R> {
       /// A running task said this.
       Told(M),
       /// A task is done, with this result.
       Done(R),
   }
   ```

4. Send reports instead of results. In `serve`, replace:

   ```rust
       let (back, results) = mpsc::channel::<R>();
   ```

   with:

   ```rust
       let (back, reports) = mpsc::channel::<Report<M, R>>();
   ```

5. Hand each report to `step` as the turn it is. In `serve`, replace:

   ```rust
               // This function keeps a sender of its own, so the channel cannot close.
               let Ok(result) = results.recv() else {
                   return Ok(started);
               };
               running -= 1;
               ready = step(Turn::Done(result));
   ```

   with:

   ```rust
               // This function keeps a sender of its own, so the channel cannot close.
               let Ok(report) = reports.recv() else {
                   return Ok(started);
               };
               ready = match report {
                   Report::Told(message) => step(Turn::Told(message)),
                   Report::Done(result) => {
                       running -= 1;
                       step(Turn::Done(result))
                   }
               };
   ```

6. Let a worker send what its task says. Replace:

   ```rust
   /// One worker thread: takes tasks off `queue` until it closes, and hands back each result.
   fn worker<T, M, R, W>(queue: &Mutex<Receiver<T>>, work: &W, back: &Sender<R>)
   where
       W: Fn(T, &(dyn Fn(M) + Sync)) -> R,
   {
   ```

   with:

   ```rust
   /// One worker thread: takes tasks off `queue` until it closes, and hands back what each one
   /// says and then its result.
   fn worker<T, M, R, W>(queue: &Mutex<Receiver<T>>, work: &W, back: &Sender<Report<M, R>>)
   where
       M: Send,
       R: Send,
       W: Fn(T, &(dyn Fn(M) + Sync)) -> R,
   {
   ```

7. In `worker`, replace:

   ```rust
           // What a task says goes nowhere.
           let result = work(task, &|_message: M| {});
           if back.send(result).is_err() {
               return;
           }
   ```

   with:

   ```rust
           // A sender may be used from several threads at once, so a task may say things from
           // threads of its own. Each message is sent before the result, on the one channel.
           let tell = |message: M| {
               let _ = back.send(Report::Told(message));
           };
           let result = work(task, &tell);
           if back.send(Report::Done(result)).is_err() {
               return;
           }
   ```

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib dispatch::
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

10. Commit:

    ```
    $ git add crates/buildl/src/dispatch/thread_pool.rs
    $ git commit -m "feat(buildl): carry what a running task says to the calling thread"
    ```

### Task 4 — Report a task that panics as lost

**Files:**
- Modify `crates/buildl/src/dispatch/thread_pool.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl/src/dispatch/thread_pool.rs`, replace:

   ```rust
       use std::num::NonZeroUsize;
       use std::sync::Mutex;
       use std::sync::atomic::{AtomicUsize, Ordering};
       use std::thread;
       use std::time::Duration;
   ```

   with:

   ```rust
       use std::num::NonZeroUsize;
       use std::panic::resume_unwind;
       use std::sync::atomic::{AtomicUsize, Ordering};
       use std::sync::{Mutex, mpsc};
       use std::thread;
       use std::time::Duration;
   ```

2. In the same module, after the last test, `what_a_task_says_reaches_step_before_its_result_and_in_order`, with one blank line above, add:

   ```rust
       #[test]
       fn a_task_that_panics_is_lost_and_the_rest_are_reported() {
           // The dispatch runs on a thread of its own, so a pool that waited for the lost task's
           // result for ever fails this test instead of hanging it.
           let (done, waiting) = mpsc::channel();
           thread::spawn(move || {
               let mut pool = ThreadPool::new(Workers::of(2));
               let mut results = Vec::new();
               let ended = pool.dispatch(
                   |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                       if task == 3 {
                           // Unwinds as a panic does, without the message a panic prints.
                           resume_unwind(Box::new("the task panics"));
                       }
                       task
                   },
                   |turn| match turn {
                       Turn::Begin => (0..8).collect(),
                       Turn::Told(()) => Vec::new(),
                       Turn::Done(task) => {
                           results.push(task);
                           Vec::new()
                       }
                   },
               );
               let _ = done.send((ended.is_ok(), results));
           });

           let (returned_ok, mut results) = waiting.recv_timeout(Duration::from_secs(20)).unwrap();

           assert!(returned_ok);
           results.sort_unstable();
           assert_eq!(results, [0, 1, 2, 4, 5, 6, 7]);
       }
   ```

3. Run it and confirm it fails. The worker that took the panicking task is gone, the pool waits for a result that never comes, and the test gives up after its 20 seconds:

   ```
   $ cargo test -p buildl --lib dispatch::
   called `Result::unwrap()` on an `Err` value: Timeout
   test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out
   ```

4. Catch the panic on the worker. At the top of the file, replace:

   ```rust
   use std::num::NonZeroUsize;
   use std::sync::mpsc::{self, Receiver, Sender};
   ```

   with:

   ```rust
   use std::num::NonZeroUsize;
   use std::panic::{AssertUnwindSafe, catch_unwind};
   use std::sync::mpsc::{self, Receiver, Sender};
   ```

5. In `Report`, replace:

   ```rust
       /// A task is done, with this result.
       Done(R),
   }
   ```

   with:

   ```rust
       /// A task is done, with this result.
       Done(R),
       /// A task panicked, and has no result.
       Lost,
   }
   ```

6. In `serve`, replace:

   ```rust
                   Report::Done(result) => {
                       running -= 1;
                       step(Turn::Done(result))
                   }
               };
   ```

   with:

   ```rust
                   Report::Done(result) => {
                       running -= 1;
                       step(Turn::Done(result))
                   }
                   // Nothing is handed to `step`: whoever handed the task out finds it has no
                   // result.
                   Report::Lost => {
                       running -= 1;
                       Vec::new()
                   }
               };
   ```

7. Replace:

   ```rust
   /// One worker thread: takes tasks off `queue` until it closes, and hands back what each one
   /// says and then its result.
   ```

   with:

   ```rust
   /// One worker thread: takes tasks off `queue` until it closes, and hands back what each one
   /// says and then its result, or that it was lost.
   ```

8. In `worker`, replace:

   ```rust
           let result = work(task, &tell);
           if back.send(Report::Done(result)).is_err() {
               return;
           }
   ```

   with:

   ```rust
           // A task that panics is caught here, so the thread lives on, the pool keeps its size,
           // and the scope never sees a panic.
           let report =
               catch_unwind(AssertUnwindSafe(|| work(task, &tell))).map_or(Report::Lost, Report::Done);
           if back.send(report).is_err() {
               return;
           }
   ```

9. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib dispatch::
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
   ```

10. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`.

11. Commit:

    ```
    $ git add crates/buildl/src/dispatch/thread_pool.rs
    $ git commit -m "feat(buildl): report a task that panics as lost"
    ```

### Task 5 — Add the content store the exec tests store through

`Bare::collect` and the function below it store every regular file through a `ContentStore` they are handed. The store on disk belongs to another module, which `exec/` may not name, so its tests use this one. It is compiled for tests only.

**Files:**
- Create `crates/buildl/src/exec/memory_store.rs`
- Modify `crates/buildl/src/exec/mod.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl/src/exec/memory_store.rs` with its module doc and its test:

   ```rust
   //! A content store held in memory, for the tests of this module.
   //!
   //! Its own file because two files of the module need one: what stands at an output path is
   //! stored as it is found, and a strategy stores through whatever store it is handed. A store on
   //! disk belongs to another module, which this one may not name.
   //!
   //! Responsibilities: [`MemoryStore`].

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use buildl_core::{ContentStore, Digest, Error, PortName};

       use super::MemoryStore;

       #[test]
       fn the_store_holds_what_it_is_handed_and_a_broken_one_refuses() {
           let store = MemoryStore::new();
           let digest = store.put(b"some bytes").unwrap();

           assert_eq!(digest, Digest::of(b"some bytes"));
           assert!(store.contains(&digest));
           assert_eq!(store.get(&digest).unwrap(), Some(b"some bytes".to_vec()));
           assert_eq!(store.get(&Digest::of(b"other")).unwrap(), None);
           assert_eq!(store.len(), 1);

           let broken = MemoryStore::broken();
           let refused = broken.put(b"some bytes").unwrap_err();

           assert!(matches!(
               refused,
               Error::Adapter {
                   port: PortName::ContentStore,
                   ..
               }
           ));
           assert_eq!(broken.len(), 0);
       }
   }
   ```

2. Declare the file. At the end of `crates/buildl/src/exec/mod.rs`, below the doc comment that is there and one blank line, add:

   ```rust
   #[cfg(test)]
   mod memory_store;
   ```

3. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl --lib exec::memory_store
   error[E0432]: unresolved import `super::MemoryStore`
   ```

4. Add the store. In `crates/buildl/src/exec/memory_store.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   use std::collections::BTreeMap;
   use std::sync::{Mutex, MutexGuard, PoisonError};

   use buildl_core::{
       ContentStore, Diagnostic, Digest, Error, FileMode, OutputPath, PortName, Result,
   };

   /// Holds content by its digest, in memory. It places nothing: it has no workspace.
   #[derive(Debug, Default)]
   pub(crate) struct MemoryStore {
       blobs: Mutex<BTreeMap<Digest, Vec<u8>>>,
       broken: bool,
   }

   impl MemoryStore {
       /// A store that holds nothing yet.
       pub(crate) fn new() -> Self {
           Self::default()
       }

       /// A store that refuses everything it is handed.
       pub(crate) fn broken() -> Self {
           Self {
               broken: true,
               ..Self::default()
           }
       }

       /// How many different contents are held.
       pub(crate) fn len(&self) -> usize {
           self.blobs().len()
       }

       fn blobs(&self) -> MutexGuard<'_, BTreeMap<Digest, Vec<u8>>> {
           self.blobs.lock().unwrap_or_else(PoisonError::into_inner)
       }
   }

   impl ContentStore for MemoryStore {
       fn put(&self, bytes: &[u8]) -> Result<Digest> {
           if self.broken {
               return Err(Error::Adapter {
                   port: PortName::ContentStore,
                   diagnostic: Diagnostic::new("the store is broken"),
               });
           }
           let digest = Digest::of(bytes);
           self.blobs().insert(digest, bytes.to_vec());
           Ok(digest)
       }

       fn contains(&self, digest: &Digest) -> bool {
           self.blobs().contains_key(digest)
       }

       fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
           Ok(self.blobs().get(digest).cloned())
       }

       fn clear(&self, _at: &OutputPath) -> Result<()> {
           Ok(())
       }

       fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
           Ok(())
       }
   }
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl --lib exec::memory_store
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/exec/mod.rs crates/buildl/src/exec/memory_store.rs
   $ git commit -m "test(buildl): add an in-memory content store for the exec tests"
   ```

### Task 6 — Report the file, the link or nothing at an output path

A directory is reported as something else until task 7 walks it.

**Files:**
- Create `crates/buildl/src/exec/outputs.rs`
- Modify `crates/buildl/src/exec/mod.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl/src/exec/outputs.rs` with its module doc and its test:

   ```rust
   //! What stands at one output path of an action that ran.
   //!
   //! Its own file because looking is not judging: an output path is reported as it is found — a
   //! file, a directory with everything below it, a link, something else, or nothing — and what a
   //! report means for the build is decided by the pipeline. No symbolic link is followed, at the
   //! path or below it, so nothing outside the output is ever read.
   //!
   //! Responsibilities: `found` and `Unread`.
   //!
   //! Non-responsibilities: refusing anything. A link, a pipe and an empty directory are reported,
   //! not rejected.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, Permissions};
       use std::os::unix::fs::PermissionsExt;

       use buildl_core::{ContentStore, Digest, FileMode, Found, StoredFile};

       use super::found;
       use crate::exec::memory_store::MemoryStore;

       #[test]
       fn collect_reports_a_file_with_its_mode_and_stores_it() {
           let dir = tempfile::tempdir().unwrap();
           let store = MemoryStore::new();
           let plain = dir.path().join("plain");
           let tool = dir.path().join("tool");
           fs::write(&plain, "text").unwrap();
           fs::write(&tool, "#!/bin/sh\n").unwrap();
           fs::set_permissions(&plain, Permissions::from_mode(0o644)).unwrap();
           // One execute bit is enough, and it need not be the owner's.
           fs::set_permissions(&tool, Permissions::from_mode(0o610)).unwrap();

           assert_eq!(
               found(&plain, &store).unwrap(),
               Found::File(StoredFile {
                   digest: Digest::of(b"text"),
                   mode: FileMode::Plain,
               })
           );
           assert_eq!(
               found(&tool, &store).unwrap(),
               Found::File(StoredFile {
                   digest: Digest::of(b"#!/bin/sh\n"),
                   mode: FileMode::Executable,
               })
           );
           assert_eq!(
               store.get(&Digest::of(b"text")).unwrap(),
               Some(b"text".to_vec())
           );
           assert_eq!(store.len(), 2);
       }
   }
   ```

2. Declare the file. In `crates/buildl/src/exec/mod.rs`, directly below the line `mod memory_store;`, add as the last line of the file:

   ```rust
   mod outputs;
   ```

3. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl --lib exec::outputs
   error[E0432]: unresolved import `super::found`
   ```

4. Add the function. In `crates/buildl/src/exec/outputs.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   #![expect(
       dead_code,
       reason = "nothing reports an output, or reads why one could not be, until a strategy collects"
   )]

   use std::fs::{self, Metadata};
   use std::io;
   use std::os::unix::fs::PermissionsExt;
   use std::path::Path;

   use buildl_core::{ContentStore, Error, FileMode, Found, StoredFile};

   /// Why what stands at an output path could not be reported.
   #[derive(Debug)]
   pub(crate) enum Unread {
       /// The path, or something below it, could not be looked at, listed or read.
       Io(io::Error),
       /// The store refused a file's bytes. The error is the store's own.
       Store(Error),
   }

   impl From<io::Error> for Unread {
       fn from(error: io::Error) -> Self {
           Self::Io(error)
       }
   }

   /// What is at `path`, following no symbolic link; every regular file is stored through `cas`.
   pub(crate) fn found<C: ContentStore>(path: &Path, cas: &C) -> Result<Found, Unread> {
       let metadata = match fs::symlink_metadata(path) {
           Ok(metadata) => metadata,
           Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Found::Nothing),
           Err(error) => return Err(Unread::Io(error)),
       };
       let kind = metadata.file_type();
       if kind.is_symlink() {
           return Ok(Found::Symlink);
       }
       if kind.is_file() {
           return Ok(Found::File(stored(path, &metadata, cas)?));
       }
       Ok(Found::Special)
   }

   /// Stores the regular file at `path`, and says whether it may be run: it may when any of its
   /// three execute bits is set.
   fn stored<C: ContentStore>(
       path: &Path,
       metadata: &Metadata,
       cas: &C,
   ) -> Result<StoredFile, Unread> {
       let digest = cas.put(&fs::read(path)?).map_err(Unread::Store)?;
       let mode = if metadata.permissions().mode() & 0o111 == 0 {
           FileMode::Plain
       } else {
           FileMode::Executable
       };
       Ok(StoredFile { digest, mode })
   }
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl --lib exec::outputs
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/exec/mod.rs crates/buildl/src/exec/outputs.rs
   $ git commit -m "feat(buildl): report the file, the link or nothing at an output path"
   ```

### Task 7 — Report everything below a directory output

**Files:**
- Modify `crates/buildl/src/exec/outputs.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl/src/exec/outputs.rs`, one of the new tests ends in `panic!`, so the module expects that lint too. Replace:

   ```rust
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, Permissions};
   ```

   with:

   ```rust
           clippy::unwrap_used,
           clippy::panic,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, Permissions};
   ```

2. Widen the imports and add three helpers. Replace:

   ```rust
       use std::fs::{self, Permissions};
       use std::os::unix::fs::PermissionsExt;

       use buildl_core::{ContentStore, Digest, FileMode, Found, StoredFile};

       use super::found;
       use crate::exec::memory_store::MemoryStore;
   ```

   with:

   ```rust
       use std::fs::{self, Permissions};
       use std::os::unix::fs::{PermissionsExt, symlink};
       use std::path::Path;
       use std::process::Command;

       use buildl_core::{ContentStore, Digest, FileMode, Found, FoundEntry, FoundKind, StoredFile};

       use super::found;
       use crate::exec::memory_store::MemoryStore;

       /// Makes a named pipe at `path` with the system's `mkfifo`: something that is neither a
       /// file, a directory nor a link.
       fn pipe(path: &Path) {
           let made = Command::new("mkfifo").arg(path).status().unwrap();
           assert!(made.success());
       }

       fn entry(path: &str, kind: FoundKind) -> FoundEntry {
           FoundEntry {
               path: path.to_owned(),
               kind,
           }
       }

       fn file(bytes: &[u8], mode: FileMode) -> FoundKind {
           FoundKind::File(StoredFile {
               digest: Digest::of(bytes),
               mode,
           })
       }
   ```

3. After the last test, `collect_reports_a_file_with_its_mode_and_stores_it`, with one blank line above, add:

   ```rust
       #[test]
       fn collect_reports_nothing_a_link_and_a_special_file_at_the_path() {
           let dir = tempfile::tempdir().unwrap();
           let store = MemoryStore::new();
           fs::write(dir.path().join("file"), "text").unwrap();
           symlink("file", dir.path().join("link")).unwrap();
           symlink("nowhere", dir.path().join("dangling")).unwrap();
           pipe(&dir.path().join("pipe"));
           fs::create_dir(dir.path().join("hollow")).unwrap();

           let at = |name: &str| found(&dir.path().join(name), &store).unwrap();

           assert_eq!(at("absent"), Found::Nothing);
           assert_eq!(at("link"), Found::Symlink);
           assert_eq!(at("dangling"), Found::Symlink);
           assert_eq!(at("pipe"), Found::Special);
           assert_eq!(at("hollow"), Found::Tree(Vec::new()));
           // Nothing was read through the link, and a pipe was never opened.
           assert_eq!(store.len(), 0);
       }

       #[test]
       fn collect_reports_everything_below_a_directory() {
           let dir = tempfile::tempdir().unwrap();
           let store = MemoryStore::new();
           let top = dir.path().join("tree");
           fs::create_dir_all(top.join("sub/deeper")).unwrap();
           fs::create_dir_all(top.join("empty")).unwrap();
           fs::write(top.join("a"), "a\n").unwrap();
           fs::write(top.join("sub/b"), "b\n").unwrap();
           fs::set_permissions(top.join("sub/b"), Permissions::from_mode(0o755)).unwrap();
           fs::write(top.join("sub/deeper/c"), "c\n").unwrap();
           symlink("a", top.join("link")).unwrap();
           symlink("sub", top.join("linked-directory")).unwrap();
           pipe(&top.join("pipe"));

           let Found::Tree(mut entries) = found(&top, &store).unwrap() else {
               panic!("a directory is reported as a tree");
           };
           entries.sort();

           // A directory that holds something is not listed, and a link to one is not entered.
           assert_eq!(
               entries,
               [
                   entry("a", file(b"a\n", FileMode::Plain)),
                   entry("empty", FoundKind::EmptyDirectory),
                   entry("link", FoundKind::Symlink),
                   entry("linked-directory", FoundKind::Symlink),
                   entry("pipe", FoundKind::Special),
                   entry("sub/b", file(b"b\n", FileMode::Executable)),
                   entry("sub/deeper/c", file(b"c\n", FileMode::Plain)),
               ]
           );
           assert_eq!(store.len(), 3);
           assert_eq!(
               store.get(&Digest::of(b"c\n")).unwrap(),
               Some(b"c\n".to_vec())
           );
       }
   ```

4. Run them and confirm both fail, because a directory is reported as something that is neither a file nor a link:

   ```
   $ cargo test -p buildl --lib exec::outputs
   assertion `left == right` failed
     left: Special
    right: Tree([])
   a directory is reported as a tree
   test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 11 filtered out
   ```

5. Walk a directory. At the top of the file, replace:

   ```rust
   use std::path::Path;

   use buildl_core::{ContentStore, Error, FileMode, Found, StoredFile};
   ```

   with:

   ```rust
   use std::path::{Path, PathBuf};

   use buildl_core::{ContentStore, Error, FileMode, Found, FoundEntry, FoundKind, StoredFile};
   ```

6. In `found`, replace:

   ```rust
       if kind.is_file() {
           return Ok(Found::File(stored(path, &metadata, cas)?));
       }
       Ok(Found::Special)
   }
   ```

   with:

   ```rust
       if kind.is_file() {
           return Ok(Found::File(stored(path, &metadata, cas)?));
       }
       if kind.is_dir() {
           return Ok(Found::Tree(below(path, cas)?));
       }
       Ok(Found::Special)
   }

   /// One directory still to list.
   struct Level {
       /// Where it is.
       directory: PathBuf,
       /// What the path of everything it holds starts with: nothing for the output itself, and
       /// otherwise the directory's own path below the output, then a `/`.
       prefix: String,
   }

   /// Everything below the directory `top`, in the order the filesystem lists it.
   ///
   /// A directory that holds something is not listed: the paths of what it holds say it is there.
   /// The directories still to list are kept on a stack of this function's own, so a deep tree
   /// cannot exhaust the thread's.
   fn below<C: ContentStore>(top: &Path, cas: &C) -> Result<Vec<FoundEntry>, Unread> {
       let mut entries = Vec::new();
       let mut pending = vec![Level {
           directory: top.to_path_buf(),
           prefix: String::new(),
       }];
       while let Some(level) = pending.pop() {
           let mut holds_something = false;
           for entry in fs::read_dir(&level.directory)? {
               let entry = entry?;
               holds_something = true;
               let path = format!("{}{}", level.prefix, entry.file_name().to_string_lossy());
               // The entry's own metadata: a link is not followed.
               let metadata = entry.metadata()?;
               let kind = metadata.file_type();
               if kind.is_dir() {
                   pending.push(Level {
                       directory: entry.path(),
                       prefix: format!("{path}/"),
                   });
                   continue;
               }
               let kind = if kind.is_symlink() {
                   FoundKind::Symlink
               } else if kind.is_file() {
                   FoundKind::File(stored(&entry.path(), &metadata, cas)?)
               } else {
                   FoundKind::Special
               };
               entries.push(FoundEntry { path, kind });
           }
           if !holds_something && !level.prefix.is_empty() {
               entries.push(FoundEntry {
                   path: level.prefix.trim_end_matches('/').to_owned(),
                   kind: FoundKind::EmptyDirectory,
               });
           }
       }
       Ok(entries)
   }
   ```

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::outputs
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

9. Commit:

   ```
   $ git add crates/buildl/src/exec/outputs.rs
   $ git commit -m "feat(buildl): report everything below a directory output"
   ```

### Task 8 — Report a name that is not Unicode

The walk of task 7 shows a name that is not Unicode with its bytes replaced and reports what it names as a file like any other, stored. Such an entry has no path the pipeline can keep, so it is reported as `FoundKind::NotText` and not stored, with everything below a directory so named.

**Files:**
- Modify `crates/buildl/src/exec/outputs.rs`

**Steps:**

1. Write the test. In the test module of `crates/buildl/src/exec/outputs.rs`, after the last test, `collect_reports_everything_below_a_directory`, with one blank line above, add:

   ```rust
       #[cfg(target_os = "linux")]
       #[test]
       fn collect_reports_a_name_that_is_not_unicode() {
           use std::ffi::OsString;
           use std::os::unix::ffi::OsStringExt;

           let dir = tempfile::tempdir().unwrap();
           let store = MemoryStore::new();
           let top = dir.path().join("tree");
           let raw_file = OsString::from_vec(vec![b'f', 0xff]);
           let raw_directory = OsString::from_vec(vec![b'd', 0xff]);
           let raw_hollow = OsString::from_vec(vec![b'h', 0xff]);
           fs::create_dir_all(top.join(&raw_directory).join("deeper")).unwrap();
           fs::create_dir_all(top.join(&raw_hollow)).unwrap();
           fs::write(top.join(&raw_file), "x").unwrap();
           fs::write(top.join(&raw_directory).join("inside"), "y").unwrap();
           fs::write(top.join(&raw_directory).join("deeper/below"), "z").unwrap();
           fs::write(top.join("plain"), "p").unwrap();

           let Found::Tree(mut entries) = found(&top, &store).unwrap() else {
               panic!("a directory is reported as a tree");
           };
           entries.sort();

           // What is not text is shown with U+FFFD in its place. Below a directory whose name is
           // not text nothing is text, whatever its own name is, and nothing there is stored.
           assert_eq!(
               entries,
               [
                   entry("d\u{fffd}/deeper/below", FoundKind::NotText),
                   entry("d\u{fffd}/inside", FoundKind::NotText),
                   entry("f\u{fffd}", FoundKind::NotText),
                   entry("h\u{fffd}", FoundKind::NotText),
                   entry("plain", file(b"p", FileMode::Plain)),
               ]
           );
           assert_eq!(store.len(), 1);
       }
   ```

2. Run it. On Linux, confirm it fails: the first entry compared is reported as a stored file where the test expects `NotText`. On macOS the test is compiled out, so the run reports the three tests there were and no failure:

   ```
   $ cargo test -p buildl --lib exec::outputs
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out
   ```

   That line is the macOS run. The Linux failure was not seen while this plan was written: no Linux machine was at hand.

3. Remember, for each directory, whether every name down to it is text. In `Level`, replace:

   ```rust
       /// otherwise the directory's own path below the output, then a `/`.
       prefix: String,
   }
   ```

   with:

   ```rust
       /// otherwise the directory's own path below the output, then a `/`.
       prefix: String,
       /// Whether its own name, and the name of every directory between it and the output, is
       /// valid Unicode.
       text: bool,
   }
   ```

4. In the doc comment of `below`, replace:

   ```rust
   /// A directory that holds something is not listed: the paths of what it holds say it is there.
   /// The directories still to list are kept on a stack of this function's own, so a deep tree
   /// cannot exhaust the thread's.
   ```

   with:

   ```rust
   /// A directory that holds something is not listed: the paths of what it holds say it is there.
   /// A name that is not valid Unicode is reported as [`FoundKind::NotText`], with what is not
   /// text replaced in its path; a directory with such a name is still listed, and everything
   /// below it is reported the same way and not stored. The directories still to list are kept on
   /// a stack of this function's own, so a deep tree cannot exhaust the thread's.
   ```

5. In `below`, replace:

   ```rust
           directory: top.to_path_buf(),
           prefix: String::new(),
       }];
   ```

   with:

   ```rust
           directory: top.to_path_buf(),
           prefix: String::new(),
           text: true,
       }];
   ```

6. Replace:

   ```rust
               let path = format!("{}{}", level.prefix, entry.file_name().to_string_lossy());
   ```

   with:

   ```rust
               let name = entry.file_name();
               let (path, text) = match name.to_str() {
                   Some(name) if level.text => (format!("{}{name}", level.prefix), true),
                   _ => (format!("{}{}", level.prefix, name.to_string_lossy()), false),
               };
   ```

7. Replace:

   ```rust
                       directory: entry.path(),
                       prefix: format!("{path}/"),
                   });
                   continue;
               }
               let kind = if kind.is_symlink() {
   ```

   with:

   ```rust
                       directory: entry.path(),
                       prefix: format!("{path}/"),
                       text,
                   });
                   continue;
               }
               let kind = if !text {
                   FoundKind::NotText
               } else if kind.is_symlink() {
   ```

8. Replace:

   ```rust
               entries.push(FoundEntry {
                   path: level.prefix.trim_end_matches('/').to_owned(),
                   kind: FoundKind::EmptyDirectory,
               });
   ```

   with:

   ```rust
               entries.push(FoundEntry {
                   path: level.prefix.trim_end_matches('/').to_owned(),
                   kind: if level.text {
                       FoundKind::EmptyDirectory
                   } else {
                       FoundKind::NotText
                   },
               });
   ```

9. Run the tests and confirm they pass: four on Linux, and on macOS the three that are compiled:

   ```
   $ cargo test -p buildl --lib exec::outputs
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out
   ```

10. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`.

11. Commit:

    ```
    $ git add crates/buildl/src/exec/outputs.rs
    $ git commit -m "feat(buildl): report a name that is not Unicode below an output"
    ```

### Task 9 — Run one process to its end and keep what it printed

The function of this task waits for as long as the process runs and as long as anything holds its outputs open. Task 13 gives it a deadline, task 14 a limit on the outputs, and task 15 a live sink.

**Files:**
- Create `crates/buildl/src/exec/capture.rs`
- Modify `crates/buildl/src/exec/mod.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl/src/exec/capture.rs` with its module doc and its tests:

   ```rust
   //! Running one process to its end, and keeping what it printed.
   //!
   //! Its own file because waiting for a process is where a build can hang: a pipe that fills
   //! while nobody reads it stops the process for good, and a pipe something else still holds
   //! never reaches its end. Each of the two outputs is read on a thread of its own, and the
   //! thread that started the process is the only one that touches it.
   //!
   //! Responsibilities: `capture` and `Capture`.
   //!
   //! Non-responsibilities: choosing the program, its arguments, its directory and its
   //! environment, and saying what an ending means. The caller builds the command and reads the
   //! status.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::process::Command;

       use super::{Capture, capture};

       /// The command `sh -c script`, with an empty environment. A tool the script names is
       /// written by its path, because the child has no `PATH`.
       fn sh(script: &str) -> Command {
           let mut command = Command::new("/bin/sh");
           command.env_clear().args(["-c", script]);
           command
       }

       fn run(command: &mut Command) -> Capture {
           capture(command).unwrap()
       }

       #[test]
       fn standard_input_is_empty() {
           // `cat` copies its input until the input ends, so it returns at once, with nothing,
           // only when the input it was handed is already at its end.
           let ended = run(Command::new("/bin/cat").env_clear());

           assert!(ended.status.success());
           assert_eq!(ended.stdout, b"");
           assert_eq!(ended.stderr, b"");
       }

       #[test]
       fn ten_megabytes_on_each_stream_do_not_block() {
           // Far more than a pipe holds, on both at once: read one after the other, the process
           // would stop on the pipe nobody is reading.
           let ended = run(&mut sh(
               "/usr/bin/head -c 10000000 /dev/zero & /usr/bin/head -c 10000000 /dev/zero >&2; wait",
           ));

           assert!(ended.status.success());
           assert_eq!(ended.stdout.len(), 10_000_000);
           assert_eq!(ended.stderr.len(), 10_000_000);
       }
   }
   ```

2. Declare the file. In `crates/buildl/src/exec/mod.rs`, replace:

   ```rust
   #[cfg(test)]
   mod memory_store;
   ```

   with:

   ```rust
   mod capture;
   #[cfg(test)]
   mod memory_store;
   ```

3. Run the tests and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib exec::capture
   error[E0432]: unresolved imports `super::Capture`, `super::capture`
   ```

4. Add the function. In `crates/buildl/src/exec/capture.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing but this file's tests runs a process until a strategy runs an action"
       )
   )]

   use std::io::{self, Read};
   use std::process::{Child, Command, ExitStatus, Stdio};
   use std::sync::mpsc::{self, Sender};
   use std::thread;

   use buildl_core::Channel;

   /// How one process ended, and everything it printed.
   #[derive(Debug)]
   pub(crate) struct Capture {
       /// The status the process ended with.
       pub(crate) status: ExitStatus,
       /// Everything it wrote to standard output.
       pub(crate) stdout: Vec<u8>,
       /// Everything it wrote to standard error.
       pub(crate) stderr: Vec<u8>,
   }

   /// What a reader hands the thread that waits for the process.
   enum Piece {
       /// A run of bytes read from one of the two outputs.
       Chunk(Channel, Vec<u8>),
       /// That reader's output is at its end, or can no longer be read.
       Closed,
   }

   /// How much a reader asks for at a time.
   const CHUNK: usize = 64 * 1024;

   /// Spawns `command` with an empty standard input and both outputs piped, and waits for it.
   ///
   /// # Errors
   ///
   /// Returns the failure to start the process, to start a reader, or to wait. A process that was
   /// started is killed and reaped, as far as it can be, before an error is returned.
   pub(crate) fn capture(command: &mut Command) -> io::Result<Capture> {
       let mut child = command
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .spawn()?;
       let waited = waited(&mut child);
       if waited.is_err() {
           // The error says what went wrong; whether these two succeed changes nothing.
           let _ = child.kill();
           let _ = child.wait();
       }
       waited
   }

   /// Reads both outputs of `child` to their end, then waits for it.
   fn waited(child: &mut Child) -> io::Result<Capture> {
       let (to, from) = mpsc::channel();
       let mut open = 0_u8;
       if let Some(pipe) = child.stdout.take() {
           read(Channel::Stdout, pipe, to.clone())?;
           open += 1;
       }
       if let Some(pipe) = child.stderr.take() {
           read(Channel::Stderr, pipe, to.clone())?;
           open += 1;
       }
       // Only the readers send now, so the channel closes when the last of them has ended.
       drop(to);
       let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
       while open > 0 {
           match from.recv() {
               Ok(Piece::Chunk(Channel::Stdout, bytes)) => stdout.extend_from_slice(&bytes),
               Ok(Piece::Chunk(Channel::Stderr, bytes)) => stderr.extend_from_slice(&bytes),
               Ok(Piece::Closed) => open -= 1,
               Err(mpsc::RecvError) => open = 0,
           }
       }
       let status = child.wait()?;
       Ok(Capture {
           status,
           stdout,
           stderr,
       })
   }

   /// Starts a thread that reads `pipe` to its end and sends what it reads to `to`.
   ///
   /// The thread is not joined. It ends at the end of the stream, at a read error, or when what
   /// it sends has nobody left to take it.
   fn read<R: Read + Send + 'static>(
       channel: Channel,
       mut pipe: R,
       to: Sender<Piece>,
   ) -> io::Result<()> {
       thread::Builder::new()
           .name("buildl-capture".to_owned())
           .spawn(move || {
               let mut buffer = vec![0_u8; CHUNK];
               loop {
                   match pipe.read(&mut buffer) {
                       Ok(0) | Err(_) => break,
                       Ok(count) => {
                           let chunk = buffer[..count].to_vec();
                           if to.send(Piece::Chunk(channel, chunk)).is_err() {
                               return;
                           }
                       }
                   }
               }
               let _ = to.send(Piece::Closed);
           })?;
       Ok(())
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::capture
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/exec/mod.rs crates/buildl/src/exec/capture.rs
   $ git commit -m "feat(buildl): run one process to its end and keep what it printed"
   ```

### Task 10 — Add the tier 1 strategy

The strategy of this task clears an action's output paths, runs its command in the workspace root and reports what the command left. The program is still looked for by the system, in this process's own `PATH`, and the command still inherits this process's environment: tasks 11 and 12 change both.

**Files:**
- Create `crates/buildl/src/exec/bare.rs`
- Modify `crates/buildl/src/exec/mod.rs`
- Modify `crates/buildl/src/exec/outputs.rs`
- Modify `crates/buildl/src/exec/capture.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl/src/exec/bare.rs` with its module doc and its tests:

   ```rust
   //! The strategy that runs an action as a plain process in the workspace root.
   //!
   //! Its own file because it is the one place where the three steps of running an action meet:
   //! clearing the paths the action writes, running its command, and reporting what the command
   //! left. Each step is short here; waiting for a process and walking an output have files of
   //! their own.
   //!
   //! Responsibilities: [`Bare`].

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::fs::{self, Permissions};
       use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
       use std::path::{Path, PathBuf};

       use buildl_core::{
           Argument, Command, Digest, Directory, Error, ExecStrategy, Exit, FileMode, Found,
           FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath, PortName, RawOutcome,
           ReadyAction, StoredFile, TargetName,
       };

       use super::Bare;
       use crate::exec::memory_store::MemoryStore;

       /// A workspace root in a directory of its own, by its canonical path. The directory above
       /// it is the test's too, for what has to lie outside the workspace.
       fn workspace() -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap().join("ws");
           fs::create_dir(&root).unwrap();
           (dir, root)
       }

       fn bare(root: &Path) -> Bare {
           Bare::new(root.to_path_buf())
       }

       /// The action of `//lib:probe` that runs `arguments`, with no variable and no output.
       fn action(arguments: &[&str]) -> ReadyAction {
           let arguments = arguments
               .iter()
               .map(|argument| Argument::parse(*argument).unwrap())
               .collect();
           ReadyAction {
               label: Label::parse("//lib:probe").unwrap(),
               command: Command::new(arguments).unwrap(),
               description: None,
               env: BTreeMap::new(),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: Vec::new(),
               network: NetworkAccess::Sealed,
           }
       }

       /// The action that runs `script` through the system's `sh`. A tool the script names is
       /// written by its path.
       fn sh(script: &str) -> ReadyAction {
           action(&["sh", "-c", script])
       }

       /// The path `out/<directory>/<name>`.
       fn output(directory: &str, name: &str) -> OutputPath {
           let label = Label::new(
               Directory::parse(directory).unwrap(),
               TargetName::parse("probe").unwrap(),
           );
           OutputPath::of(
               &Directory::parse("out").unwrap(),
               &label,
               &OutputName::parse(name).unwrap(),
           )
       }

       fn plain(bytes: &[u8]) -> StoredFile {
           StoredFile {
               digest: Digest::of(bytes),
               mode: FileMode::Plain,
           }
       }

       #[test]
       fn prepare_makes_the_directory_and_leaves_nothing_at_the_path() {
           let (_dir, root) = workspace();
           let outside = root.parent().unwrap().join("outside");
           fs::create_dir(&outside).unwrap();
           fs::write(outside.join("keep"), "untouched").unwrap();
           // What an earlier build left: a file that is a link to the store's read-only blob, a
           // tree, and a link to a directory outside the workspace.
           let blob = outside.join("blob");
           fs::write(&blob, "held by the store").unwrap();
           fs::set_permissions(&blob, Permissions::from_mode(0o444)).unwrap();
           fs::create_dir_all(root.join("out/lib/tree/deep")).unwrap();
           fs::write(root.join("out/lib/tree/deep/old"), "old").unwrap();
           fs::hard_link(&blob, root.join("out/lib/file")).unwrap();
           symlink(&outside, root.join("out/lib/link")).unwrap();
           let mut action = sh("true");
           action.outputs = vec![
               output("lib", "file"),
               output("lib", "tree"),
               output("lib", "link"),
               output("fresh/deep", "made"),
           ];

           bare(&root).prepare(&action).unwrap();

           for output in &action.outputs {
               assert!(
                   fs::symlink_metadata(root.join(output.as_str())).is_err(),
                   "{output} still holds something"
               );
           }
           assert!(root.join("out/fresh/deep").is_dir());
           // The blob was unlinked from, never written through, and nothing was followed.
           let held = fs::metadata(&blob).unwrap();
           assert_eq!(fs::read(&blob).unwrap(), b"held by the store");
           assert_eq!(held.nlink(), 1);
           assert_eq!(held.permissions().mode() & 0o777, 0o444);
           assert_eq!(fs::read(outside.join("keep")).unwrap(), b"untouched");
       }

       #[test]
       fn an_output_whose_directory_cannot_be_made_fails_the_prepare() {
           let (_dir, root) = workspace();
           // A file stands where the directory of the output has to be.
           fs::create_dir(root.join("out")).unwrap();
           fs::write(root.join("out/lib"), "in the way").unwrap();
           let mut action = sh("true");
           action.outputs = vec![output("lib", "file")];

           let failed = bare(&root).prepare(&action).unwrap_err();

           assert!(matches!(
               failed,
               Error::Adapter {
                   port: PortName::ExecStrategy,
                   ..
               }
           ));
           let said = failed.to_string();
           assert!(
               said.starts_with("exec strategy: //lib:probe: making the directory of out/lib/file: "),
               "{said}"
           );
       }

       #[test]
       fn the_command_runs_in_the_workspace_root() {
           let (_dir, root) = workspace();

           let raw = bare(&root).run(&(), &action(&["pwd"]), None).unwrap();

           assert_eq!(raw.exit, Exit::Exited(0));
           assert_eq!(
               String::from_utf8(raw.stdout).unwrap(),
               format!("{}\n", root.display())
           );
       }

       #[test]
       fn an_exit_status_is_reported_as_it_is() {
           let (_dir, root) = workspace();

           let raw = bare(&root)
               .run(&(), &sh("echo out; echo err >&2; exit 3"), None)
               .unwrap();

           assert_eq!(
               raw,
               RawOutcome {
                   exit: Exit::Exited(3),
                   stdout: b"out\n".to_vec(),
                   stderr: b"err\n".to_vec(),
               }
           );
       }

       #[test]
       fn an_action_killed_by_a_signal_is_reported_signaled() {
           let (_dir, root) = workspace();

           let raw = bare(&root).run(&(), &sh("kill -SEGV $$"), None).unwrap();

           assert_eq!(raw.exit, Exit::Signaled(11));
       }

       #[test]
       fn a_command_that_cannot_be_started_is_an_adapter_error() {
           let (_dir, root) = workspace();
           // No process can be started in a directory that does not exist.
           let nowhere = bare(&root.join("gone"));

           let failed = nowhere.run(&(), &sh("true"), None).unwrap_err();

           assert!(matches!(
               failed,
               Error::Adapter {
                   port: PortName::ExecStrategy,
                   ..
               }
           ));
           let said = failed.to_string();
           assert!(
               said.starts_with("exec strategy: //lib:probe: running sh: "),
               "{said}"
           );
       }

       #[test]
       fn collect_reports_one_entry_per_output_in_order() {
           let (_dir, root) = workspace();
           let store = MemoryStore::new();
           fs::create_dir_all(root.join("out/lib/tree")).unwrap();
           fs::write(root.join("out/lib/second"), "2").unwrap();
           fs::write(root.join("out/lib/tree/inside"), "3").unwrap();
           let mut action = sh("true");
           action.outputs = vec![
               output("lib", "first"),
               output("lib", "second"),
               output("lib", "tree"),
           ];

           let found = bare(&root).collect(&(), &action, &store).unwrap();

           assert_eq!(
               found,
               [
                   Found::Nothing,
                   Found::File(plain(b"2")),
                   Found::Tree(vec![FoundEntry {
                       path: "inside".to_owned(),
                       kind: FoundKind::File(plain(b"3")),
                   }]),
               ]
           );
       }

       #[test]
       fn a_store_that_fails_fails_the_collect_with_its_own_error() {
           let (_dir, root) = workspace();
           fs::create_dir_all(root.join("out/lib")).unwrap();
           fs::write(root.join("out/lib/file"), "x").unwrap();
           let mut action = sh("true");
           action.outputs = vec![output("lib", "file")];

           let refused = bare(&root)
               .collect(&(), &action, &MemoryStore::broken())
               .unwrap_err();

           // The store's own error, not one this strategy made.
           assert_eq!(refused.to_string(), "content store: the store is broken");
       }
   }
   ```

2. Declare the file and export the strategy. Replace the whole of `crates/buildl/src/exec/mod.rs` with:

   ```rust
   //! Running one action as a plain process: the first tier of isolation.
   //!
   //! Its own module because starting a process, waiting for it and reading what it left behind
   //! are things only this crate may do: the pipeline hands over an action whose command and paths
   //! are settled, through `buildl_core::ExecStrategy`, and learns how the run ended and what
   //! stands at each output path.
   //!
   //! Responsibilities: [`Bare`], the implementation of `buildl_core::ExecStrategy` that runs an
   //! action in the workspace itself.
   //!
   //! Non-responsibilities: confining an action, and judging what it left. This tier settles how
   //! a command is started and nothing about what the command may reach. A missing output, a link
   //! and an empty directory are reported as they are found; what each means for the build is the
   //! pipeline's to decide. Files are stored through whatever `buildl_core::ContentStore` the
   //! module is handed.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod bare;
   mod capture;
   #[cfg(test)]
   mod memory_store;
   mod outputs;

   pub use bare::Bare;
   ```

3. Run the tests and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib exec::bare
   error[E0432]: unresolved import `super::Bare`
   error[E0432]: unresolved import `bare::Bare`
   ```

4. Add the strategy. In `crates/buildl/src/exec/bare.rs`, between the module doc comment and `#[cfg(test)]`, with one blank line above and one below, insert:

   ```rust
   use core::fmt;
   use std::fs;
   use std::os::unix::process::ExitStatusExt;
   use std::path::PathBuf;
   use std::process::{Command, ExitStatus};

   use buildl_core::{
       Argument, ContentStore, Diagnostic, Error, ExecStrategy, Exit, Found, Label, LiveSink,
       PortName, RawOutcome, ReadyAction, Result,
   };

   use crate::exec::capture::capture;
   use crate::exec::outputs::{Unread, found};
   use crate::state::clear_path;

   /// Tier 1: a plain process in the workspace root.
   ///
   /// The command is started as an argument vector, never through a shell, with an empty
   /// standard input. Nothing is confined: the process can read and write whatever the user can.
   #[derive(Debug)]
   pub struct Bare {
       root: PathBuf,
   }

   impl Bare {
       /// `root` is the absolute workspace root.
       #[must_use]
       pub const fn new(root: PathBuf) -> Self {
           Self { root }
       }
   }

   impl ExecStrategy for Bare {
       /// Nothing: an action runs in the workspace root, and its command already names its paths
       /// relative to it, so no directory travels from `prepare` to `run`.
       type Dir = ();

       fn prepare(&self, action: &ReadyAction) -> Result<()> {
           for output in &action.outputs {
               let path = self.root.join(output.as_str());
               if let Some(directory) = path.parent() {
                   fs::create_dir_all(directory).map_err(|error| {
                       fault(
                           &action.label,
                           &format!("making the directory of {output}"),
                           error,
                       )
                   })?;
               }
               // Unlinked, never written through: what an earlier build left at the path is a
               // link to a file the content store still holds.
               clear_path(&path)
                   .map_err(|error| fault(&action.label, &format!("clearing {output}"), error))?;
           }
           Ok(())
       }

       fn run(
           &self,
           (): &(),
           action: &ReadyAction,
           _live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome> {
           let Some((program, rest)) = action.command.arguments().split_first() else {
               return Err(fault(
                   &action.label,
                   "starting the command",
                   "it names no program",
               ));
           };
           let mut command = Command::new(program.as_str());
           command
               .args(rest.iter().map(Argument::as_str))
               .current_dir(&self.root);
           let captured = capture(&mut command)
               .map_err(|error| fault(&action.label, &format!("running {program}"), error))?;
           Ok(RawOutcome {
               exit: ended(captured.status),
               stdout: captured.stdout,
               stderr: captured.stderr,
           })
       }

       fn collect<C: ContentStore>(
           &self,
           (): &(),
           action: &ReadyAction,
           cas: &C,
       ) -> Result<Vec<Found>> {
           action
               .outputs
               .iter()
               .map(|output| {
                   found(&self.root.join(output.as_str()), cas).map_err(|unread| match unread {
                       Unread::Io(error) => fault(
                           &action.label,
                           &format!("reading the output {output}"),
                           error,
                       ),
                       Unread::Store(error) => error,
                   })
               })
               .collect()
       }
   }

   /// How a process that ended with `status` ended: with a status of its own, or by a signal.
   fn ended(status: ExitStatus) -> Exit {
       status.code().map_or_else(
           // Waiting for a process gives a status or a signal, never neither.
           || Exit::Signaled(status.signal().unwrap_or_default()),
           Exit::Exited,
       )
   }

   /// The error for the action of `label`: what was being done, and why it could not be.
   fn fault(label: &Label, doing: &str, why: impl fmt::Display) -> Error {
       Error::Adapter {
           port: PortName::ExecStrategy,
           diagnostic: Diagnostic::new(format!("{label}: {doing}: {why}")),
       }
   }
   ```

5. `found` and `Unread` have a caller now. In `crates/buildl/src/exec/outputs.rs`, delete these four lines and the blank line below them:

   ```rust
   #![expect(
       dead_code,
       reason = "nothing reports an output, or reads why one could not be, until a strategy collects"
   )]
   ```

6. So has `capture`. In `crates/buildl/src/exec/capture.rs`, delete these seven lines and the blank line below them:

   ```rust
   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing but this file's tests runs a process until a strategy runs an action"
       )
   )]
   ```

7. Run the tests of the module and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::
   test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. Leaving either attribute of steps 5 and 6 in place fails `cargo make dod` with `error: this lint expectation is unfulfilled`.

9. Commit:

   ```
   $ git add crates/buildl/src/exec/mod.rs crates/buildl/src/exec/bare.rs crates/buildl/src/exec/outputs.rs crates/buildl/src/exec/capture.rs
   $ git commit -m "feat(buildl): add the tier 1 execution strategy"
   ```

### Task 11 — Run the program the search path locates

**Files:**
- Modify `crates/buildl/src/exec/bare.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
       use super::Bare;
       use crate::exec::memory_store::MemoryStore;
   ```

   with:

   ```rust
       use super::Bare;
       use crate::exec::memory_store::MemoryStore;
       use crate::search_path::SearchPath;
   ```

2. Hand every strategy of these tests a search path. Replace:

   ```rust
       fn bare(root: &Path) -> Bare {
           Bare::new(root.to_path_buf())
       }
   ```

   with:

   ```rust
       /// Where the system keeps the tools these tests run.
       fn system() -> SearchPath {
           SearchPath::new(["/bin", "/usr/bin"].map(PathBuf::from))
       }

       fn bare(root: &Path) -> Bare {
           Bare::new(root.to_path_buf(), system())
       }
   ```

3. The diagnostic of a command that cannot be started names the located file from now on. In `a_command_that_cannot_be_started_is_an_adapter_error`, replace:

   ```rust
               said.starts_with("exec strategy: //lib:probe: running sh: "),
   ```

   with:

   ```rust
               said.starts_with("exec strategy: //lib:probe: running /bin/sh: "),
   ```

4. After the last test, `a_store_that_fails_fails_the_collect_with_its_own_error`, with one blank line above, add:

   ```rust
       #[test]
       fn the_located_file_is_the_one_run() {
           let (_dir, root) = workspace();
           // The same name in two directories of the search path. Each is the system's shell under
           // another name, and a shell handed only a script says the name it was started by.
           let first = root.parent().unwrap().join("first");
           let second = root.parent().unwrap().join("second");
           for directory in [&first, &second] {
               fs::create_dir(directory).unwrap();
               symlink("/bin/sh", directory.join("tool")).unwrap();
           }
           let search = SearchPath::new([first.clone(), second]);

           let raw = Bare::new(root, search)
               .run(&(), &action(&["tool", "-c", "echo $0"]), None)
               .unwrap();

           assert_eq!(raw.exit, Exit::Exited(0));
           assert_eq!(
               String::from_utf8(raw.stdout).unwrap(),
               format!("{}\n", first.join("tool").display())
           );
       }

       #[test]
       fn a_program_that_is_not_located_is_an_adapter_error() {
           let (_dir, root) = workspace();
           let nowhere_to_look = Bare::new(root, SearchPath::new([]));

           let failed = nowhere_to_look.run(&(), &sh("true"), None).unwrap_err();

           assert!(matches!(
               failed,
               Error::Adapter {
                   port: PortName::ExecStrategy,
                   ..
               }
           ));
           assert_eq!(
               failed.to_string(),
               "exec strategy: //lib:probe: starting the command: the program sh was not found"
           );
       }
   ```

5. Run them and confirm they fail to build, because a strategy is made from a root alone:

   ```
   $ cargo test -p buildl --lib exec::bare
   error[E0061]: this function takes 1 argument but 2 arguments were supplied
   ```

   The line is printed three times, once for each place a strategy is made.

6. Give the strategy its search path. Above the test module, replace:

   ```rust
   use buildl_core::{
       Argument, ContentStore, Diagnostic, Error, ExecStrategy, Exit, Found, Label, LiveSink,
       PortName, RawOutcome, ReadyAction, Result,
   };

   use crate::exec::capture::capture;
   use crate::exec::outputs::{Unread, found};
   use crate::state::clear_path;

   /// Tier 1: a plain process in the workspace root.
   ///
   /// The command is started as an argument vector, never through a shell, with an empty
   /// standard input. Nothing is confined: the process can read and write whatever the user can.
   #[derive(Debug)]
   pub struct Bare {
       root: PathBuf,
   }

   impl Bare {
       /// `root` is the absolute workspace root.
       #[must_use]
       pub const fn new(root: PathBuf) -> Self {
           Self { root }
       }
   }
   ```

   with:

   ```rust
   use buildl_core::{
       Argument, ContentStore, Diagnostic, Error, ExecStrategy, Exit, Found, Label, LiveSink,
       PortName, Program, RawOutcome, ReadyAction, Result,
   };

   use crate::exec::capture::capture;
   use crate::exec::outputs::{Unread, found};
   use crate::search_path::SearchPath;
   use crate::state::clear_path;

   /// Tier 1: a plain process in the workspace root.
   ///
   /// The command is started as an argument vector, never through a shell, with an empty
   /// standard input, and its program is the file the search path it was handed leads to.
   /// Nothing is confined: the process can read and write whatever the user can.
   #[derive(Debug)]
   pub struct Bare {
       root: PathBuf,
       search: SearchPath,
   }

   impl Bare {
       /// `root` is the absolute workspace root, and `search` the search path the programs of a
       /// plan were fingerprinted through, so that the file run is the file fingerprinted.
       #[must_use]
       pub const fn new(root: PathBuf, search: SearchPath) -> Self {
           Self { root, search }
       }
   }
   ```

7. In `run`, replace:

   ```rust
           let Some((program, rest)) = action.command.arguments().split_first() else {
               return Err(fault(
                   &action.label,
                   "starting the command",
                   "it names no program",
               ));
           };
           let mut command = Command::new(program.as_str());
           command
               .args(rest.iter().map(Argument::as_str))
               .current_dir(&self.root);
           let captured = capture(&mut command)
               .map_err(|error| fault(&action.label, &format!("running {program}"), error))?;
   ```

   with:

   ```rust
           let Some((named, rest)) = action.command.arguments().split_first() else {
               return Err(fault(
                   &action.label,
                   "starting the command",
                   "it names no program",
               ));
           };
           let located =
               Program::of(named).and_then(|program| self.search.locate(&program, &self.root));
           let Some(program) = located else {
               return Err(fault(
                   &action.label,
                   "starting the command",
                   format!("the program {named} was not found"),
               ));
           };
           // The located path is what the process is started by; every other argument is as
           // written.
           let mut command = Command::new(&program);
           command
               .args(rest.iter().map(Argument::as_str))
               .current_dir(&self.root);
           let captured = capture(&mut command).map_err(|error| {
               fault(
                   &action.label,
                   &format!("running {}", program.display()),
                   error,
               )
           })?;
   ```

8. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::bare
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

10. Commit:

    ```
    $ git add crates/buildl/src/exec/bare.rs
    $ git commit -m "feat(buildl): run the program the search path locates"
    ```

### Task 12 — Hand a command only its declared environment

**Files:**
- Modify `crates/buildl/src/exec/bare.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
       use buildl_core::{
           Argument, Command, Digest, Directory, Error, ExecStrategy, Exit, FileMode, Found,
           FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath, PortName, RawOutcome,
           ReadyAction, StoredFile, TargetName,
       };
   ```

   with:

   ```rust
       use buildl_core::{
           Argument, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy, Exit,
           FileMode, Found, FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath,
           PortName, RawOutcome, ReadyAction, StoredFile, TargetName,
       };
   ```

2. After the last test, `a_program_that_is_not_located_is_an_adapter_error`, with one blank line above, add:

   ```rust
       #[test]
       fn the_child_sees_only_the_declared_variables() {
           let (_dir, root) = workspace();
           let mut declared = action(&["env"]);
           declared.env.insert(
               EnvName::parse("FOO").unwrap(),
               EnvValue::parse("bar").unwrap(),
           );

           let raw = bare(&root).run(&(), &declared, None).unwrap();

           // `env` prints every variable it was started with: the one the action declares, and
           // none of this process's own.
           assert_eq!(raw.exit, Exit::Exited(0));
           assert_eq!(String::from_utf8(raw.stdout).unwrap(), "FOO=bar\n");
       }
   ```

3. Run it and confirm it fails, because the command is started with every variable of the test process:

   ```
   $ cargo test -p buildl --lib exec::bare
   assertion `left == right` failed
    right: "FOO=bar\n"
   test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 16 filtered out
   ```

   The `left:` line, not quoted, is the whole environment of the test process.

4. Clear the environment and put in what the action declares. In `run`, replace:

   ```rust
           command
               .args(rest.iter().map(Argument::as_str))
               .current_dir(&self.root);
   ```

   with:

   ```rust
           command
               .args(rest.iter().map(Argument::as_str))
               .current_dir(&self.root)
               .env_clear()
               .envs(
                   action
                       .env
                       .iter()
                       .map(|(name, value)| (name.as_str(), value.as_str())),
               );
   ```

5. Say so in the doc comment of `Bare`. Replace:

   ```rust
   /// The command is started as an argument vector, never through a shell, with an empty
   /// standard input, and its program is the file the search path it was handed leads to.
   /// Nothing is confined: the process can read and write whatever the user can.
   ```

   with:

   ```rust
   /// The command is started as an argument vector, never through a shell, with an empty
   /// standard input; its program is the file the search path it was handed leads to; and its
   /// environment holds exactly the variables of the action, with nothing of this process's own,
   /// not even `PATH`. Nothing is confined: the process can read and write whatever the user can.
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::bare
   test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

8. Commit:

   ```
   $ git add crates/buildl/src/exec/bare.rs
   $ git commit -m "feat(buildl): hand a command only its declared environment"
   ```

### Task 13 — Stop a command that runs past its timeout

**Files:**
- Modify `crates/buildl/src/exec/capture.rs`
- Modify `crates/buildl/src/exec/bare.rs`

**Steps:**

1. Write the failing test of the wait. In the test module of `crates/buildl/src/exec/capture.rs`, replace:

   ```rust
       use std::process::Command;

       use super::{Capture, capture};
   ```

   with:

   ```rust
       use std::process::Command;
       use std::time::{Duration, Instant};

       use super::{Capture, capture};

       /// Longer than any command of these tests runs when nothing is wrong.
       const PATIENT: Duration = Duration::from_secs(60);
   ```

2. Replace:

   ```rust
       fn run(command: &mut Command) -> Capture {
           capture(command).unwrap()
       }
   ```

   with:

   ```rust
       fn run(command: &mut Command) -> Capture {
           capture(command, PATIENT).unwrap()
       }
   ```

3. After the last test, `ten_megabytes_on_each_stream_do_not_block`, with one blank line above, add:

   ```rust
       #[test]
       fn a_child_that_closes_its_streams_is_still_timed_out() {
           let started = Instant::now();

           // Both outputs reach their end at once, and the process runs on for half a minute.
           let ended = capture(
               &mut sh("exec >&- 2>&-; exec /bin/sleep 30"),
               Duration::from_millis(300),
           )
           .unwrap();

           assert!(ended.timed_out);
           assert_eq!(ended.stdout, b"");
           // Stopped at the timeout, not when the process would have ended by itself.
           assert!(started.elapsed() < Duration::from_secs(10));
       }
   ```

4. Write the failing test of the strategy. In the test module of `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
       use std::path::{Path, PathBuf};

       use buildl_core::{
           Argument, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy, Exit,
   ```

   with:

   ```rust
       use std::path::{Path, PathBuf};
       use std::time::{Duration, Instant};

       use buildl_core::{
           Argument, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy, Exit,
   ```

5. Replace:

   ```rust
           PortName, RawOutcome, ReadyAction, StoredFile, TargetName,
       };

       use super::Bare;
   ```

   with:

   ```rust
           PortName, RawOutcome, ReadyAction, StoredFile, TargetName, Timeout,
       };

       use super::Bare;
   ```

6. Hand every strategy of these tests a timeout. Replace:

   ```rust
       fn bare(root: &Path) -> Bare {
           Bare::new(root.to_path_buf(), system())
       }
   ```

   with:

   ```rust
       /// Longer than any command of these tests runs when nothing is wrong.
       fn patient() -> Timeout {
           Timeout::parse("60s").unwrap()
       }

       fn bare(root: &Path) -> Bare {
           Bare::new(root.to_path_buf(), patient(), system())
       }
   ```

7. In `the_located_file_is_the_one_run`, replace:

   ```rust
           let raw = Bare::new(root, search)
   ```

   with:

   ```rust
           let raw = Bare::new(root, patient(), search)
   ```

8. In `a_program_that_is_not_located_is_an_adapter_error`, replace:

   ```rust
           let nowhere_to_look = Bare::new(root, SearchPath::new([]));
   ```

   with:

   ```rust
           let nowhere_to_look = Bare::new(root, patient(), SearchPath::new([]));
   ```

9. After the last test, `the_child_sees_only_the_declared_variables`, with one blank line above, add:

   ```rust
       #[test]
       fn an_action_past_its_timeout_is_stopped_and_reported_timed_out() {
           let (_dir, root) = workspace();
           let impatient = Bare::new(root, Timeout::parse("300ms").unwrap(), system());
           let started = Instant::now();

           let raw = impatient.run(&(), &sh("exec /bin/sleep 30"), None).unwrap();

           assert_eq!(raw.exit, Exit::TimedOut);
           // Stopped at the timeout, not when the command would have ended by itself.
           assert!(started.elapsed() < Duration::from_secs(10));
       }
   ```

10. Run the tests of the module and confirm they fail to build, because neither a strategy nor a wait is given a timeout yet:

    ```
    $ cargo test -p buildl --lib exec::
    error[E0061]: this function takes 2 arguments but 3 arguments were supplied
    error[E0061]: this function takes 1 argument but 2 arguments were supplied
    error[E0609]: no field `timed_out` on type `Capture`
    ```

    The first line is printed four times and the second twice, once for each call.

11. Give the wait a deadline. At the top of `crates/buildl/src/exec/capture.rs`, replace:

   ```rust
   use std::io::{self, Read};
   use std::process::{Child, Command, ExitStatus, Stdio};
   use std::sync::mpsc::{self, Sender};
   use std::thread;
   ```

    with:

   ```rust
   use std::io::{self, Read};
   use std::process::{Child, Command, ExitStatus, Stdio};
   use std::sync::mpsc::{self, RecvTimeoutError, Sender};
   use std::thread;
   use std::time::{Duration, Instant};
   ```

12. In `Capture`, replace:

   ```rust
       /// The status the process ended with.
       pub(crate) status: ExitStatus,
   ```

    with:

   ```rust
       /// The status the process ended with.
       pub(crate) status: ExitStatus,
       /// Whether it ran longer than it was allowed to and was stopped. Then `status` is that of a
       /// process ended by the signal that stopped it.
       pub(crate) timed_out: bool,
   ```

13. Replace:

   ```rust
   /// How much a reader asks for at a time.
   const CHUNK: usize = 64 * 1024;

   /// Spawns `command` with an empty standard input and both outputs piped, and waits for it.
   ///
   /// # Errors
   ///
   /// Returns the failure to start the process, to start a reader, or to wait. A process that was
   /// started is killed and reaped, as far as it can be, before an error is returned.
   pub(crate) fn capture(command: &mut Command) -> io::Result<Capture> {
       let mut child = command
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .spawn()?;
       let waited = waited(&mut child);
   ```

    with:

   ```rust
   /// How much a reader asks for at a time.
   const CHUNK: usize = 64 * 1024;

   /// The longest the waiting thread sleeps without looking at the process.
   const TICK: Duration = Duration::from_millis(20);

   /// The first pause of a wait in which nothing can arrive to end it. Each pause after it is
   /// twice as long, up to [`TICK`].
   const FIRST_PAUSE: Duration = Duration::from_micros(100);

   /// Spawns `command` with an empty standard input and both outputs piped, and waits for it at
   /// most `timeout`.
   ///
   /// A process still running after `timeout` is killed. Only that process is: what it started is
   /// not, and stays in this process's group, where a signal from the terminal reaches it.
   ///
   /// # Errors
   ///
   /// Returns the failure to start the process, to start a reader, to wait or to kill. A process
   /// that was started is killed and reaped, as far as it can be, before an error is returned.
   pub(crate) fn capture(command: &mut Command, timeout: Duration) -> io::Result<Capture> {
       let mut child = command
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .spawn()?;
       let waited = waited(&mut child, timeout);
   ```

14. Replace:

   ```rust
   /// Reads both outputs of `child` to their end, then waits for it.
   fn waited(child: &mut Child) -> io::Result<Capture> {
       let (to, from) = mpsc::channel();
   ```

    with:

   ```rust
   /// Reads both outputs of `child` and waits for it, at most `timeout`.
   ///
   /// The calling thread is the only one that touches `child`. It wakes when a reader hands it
   /// something, and otherwise at least every [`TICK`], to look at the process and at the time: a
   /// process that ends closes its outputs, so one that ends early is noticed at once.
   fn waited(child: &mut Child, timeout: Duration) -> io::Result<Capture> {
       // A monotonic reading, which a wall clock set back cannot stretch. A timeout too long to
       // add to it is no deadline at all.
       let deadline = Instant::now().checked_add(timeout);
       let (to, from) = mpsc::channel();
   ```

15. In `waited`, replace:

   ```rust
       let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
       while open > 0 {
           match from.recv() {
               Ok(Piece::Chunk(Channel::Stdout, bytes)) => stdout.extend_from_slice(&bytes),
               Ok(Piece::Chunk(Channel::Stderr, bytes)) => stderr.extend_from_slice(&bytes),
               Ok(Piece::Closed) => open -= 1,
               Err(mpsc::RecvError) => open = 0,
           }
       }
       let status = child.wait()?;
       Ok(Capture {
           status,
           stdout,
           stderr,
       })
   }
   ```

    with:

   ```rust
       let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
       let mut ended = None;
       let mut timed_out = false;
       let mut pause = FIRST_PAUSE;
       loop {
           let now = Instant::now();
           if ended.is_none() {
               // The process is looked at before the time is, so one that ends on the deadline
               // is not stopped.
               ended = child.try_wait()?;
               if ended.is_none() && deadline.is_some_and(|deadline| now >= deadline) {
                   child.kill()?;
                   ended = Some(child.wait()?);
                   timed_out = true;
               }
           }
           let wait = match ended {
               Some(status) if open == 0 => {
                   return Ok(Capture {
                       status,
                       timed_out,
                       stdout,
                       stderr,
                   });
               }
               Some(_) => TICK,
               None => deadline.map_or(TICK, |deadline| {
                   TICK.min(deadline.saturating_duration_since(now))
               }),
           };
           if open == 0 {
               // Both outputs are at their end and the process still runs: nothing can arrive to
               // wake this thread, so it sleeps, a little longer each time.
               thread::sleep(pause.min(wait));
               pause = pause.saturating_mul(2).min(TICK);
               continue;
           }
           match from.recv_timeout(wait) {
               Ok(Piece::Chunk(Channel::Stdout, bytes)) => stdout.extend_from_slice(&bytes),
               Ok(Piece::Chunk(Channel::Stderr, bytes)) => stderr.extend_from_slice(&bytes),
               Ok(Piece::Closed) => open = open.saturating_sub(1),
               Err(RecvTimeoutError::Timeout) => {}
               Err(RecvTimeoutError::Disconnected) => open = 0,
           }
       }
   }
   ```

16. Give the strategy its timeout. At the top of `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
   use std::path::PathBuf;
   use std::process::{Command, ExitStatus};

   use buildl_core::{
       Argument, ContentStore, Diagnostic, Error, ExecStrategy, Exit, Found, Label, LiveSink,
       PortName, Program, RawOutcome, ReadyAction, Result,
   };

   use crate::exec::capture::capture;
   ```

    with:

   ```rust
   use std::path::PathBuf;
   use std::process::Command;
   use std::time::Duration;

   use buildl_core::{
       Argument, ContentStore, Diagnostic, Error, ExecStrategy, Exit, Found, Label, LiveSink,
       PortName, Program, RawOutcome, ReadyAction, Result, Timeout,
   };

   use crate::exec::capture::{Capture, capture};
   ```

17. Replace:

   ```rust
   /// environment holds exactly the variables of the action, with nothing of this process's own,
   /// not even `PATH`. Nothing is confined: the process can read and write whatever the user can.
   #[derive(Debug)]
   pub struct Bare {
       root: PathBuf,
       search: SearchPath,
   }

   impl Bare {
       /// `root` is the absolute workspace root, and `search` the search path the programs of a
       /// plan were fingerprinted through, so that the file run is the file fingerprinted.
       #[must_use]
       pub const fn new(root: PathBuf, search: SearchPath) -> Self {
           Self { root, search }
       }
   }
   ```

    with:

   ```rust
   /// environment holds exactly the variables of the action, with nothing of this process's own,
   /// not even `PATH`. A command that runs longer than the timeout is stopped; what it started is
   /// not. Nothing is confined: the process can read and write whatever the user can.
   #[derive(Debug)]
   pub struct Bare {
       root: PathBuf,
       timeout: Duration,
       search: SearchPath,
   }

   impl Bare {
       /// `root` is the absolute workspace root; `timeout` the longest one action may run; and
       /// `search` the search path the programs of a plan were fingerprinted through, so that the
       /// file run is the file fingerprinted.
       #[must_use]
       pub const fn new(root: PathBuf, timeout: Timeout, search: SearchPath) -> Self {
           Self {
               root,
               timeout: timeout.as_duration(),
               search,
           }
       }
   }
   ```

18. In `run`, replace:

   ```rust
           let captured = capture(&mut command).map_err(|error| {
               fault(
                   &action.label,
                   &format!("running {}", program.display()),
                   error,
               )
           })?;
           Ok(RawOutcome {
               exit: ended(captured.status),
               stdout: captured.stdout,
               stderr: captured.stderr,
           })
   ```

    with:

   ```rust
           let captured = capture(&mut command, self.timeout).map_err(|error| {
               fault(
                   &action.label,
                   &format!("running {}", program.display()),
                   error,
               )
           })?;
           Ok(RawOutcome {
               exit: ended(&captured),
               stdout: captured.stdout,
               stderr: captured.stderr,
           })
   ```

19. Replace:

   ```rust
   /// How a process that ended with `status` ended: with a status of its own, or by a signal.
   fn ended(status: ExitStatus) -> Exit {
       status.code().map_or_else(
           // Waiting for a process gives a status or a signal, never neither.
           || Exit::Signaled(status.signal().unwrap_or_default()),
           Exit::Exited,
       )
   }
   ```

    with:

   ```rust
   /// How a process ended: stopped for running too long, with a status of its own, or by a
   /// signal.
   ///
   /// Running too long is read from what the wait recorded, never from the signal that stopped
   /// the process: anything can send that signal.
   fn ended(captured: &Capture) -> Exit {
       if captured.timed_out {
           return Exit::TimedOut;
       }
       captured.status.code().map_or_else(
           // Waiting for a process gives a status or a signal, never neither.
           || Exit::Signaled(captured.status.signal().unwrap_or_default()),
           Exit::Exited,
       )
   }
   ```

20. Run the tests of the module and confirm they pass:

    ```
    $ cargo test -p buildl --lib exec::
    test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out
    ```

21. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`.

22. Commit:

    ```
    $ git add crates/buildl/src/exec/capture.rs crates/buildl/src/exec/bare.rs
    $ git commit -m "feat(buildl): stop a command that runs past its timeout"
    ```

### Task 14 — Stop waiting for an output something else holds open

**Files:**
- Modify `crates/buildl/src/exec/capture.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl/src/exec/capture.rs`, after the last test, `a_child_that_closes_its_streams_is_still_timed_out`, with one blank line above, add:

   ```rust
       #[test]
       fn a_grandchild_holding_the_pipes_does_not_hang_the_run() {
           let started = Instant::now();

           // The shell ends at once. The `sleep` it started keeps both outputs open for half a
           // minute.
           let ended = run(&mut sh("/bin/sleep 30 & echo hi; exit 0"));

           assert!(ended.status.success());
           assert!(!ended.timed_out);
           // What was written before the process ended is kept.
           assert_eq!(ended.stdout, b"hi\n");
           // Returned once the outputs had their time to drain, not when their holder let go.
           assert!(started.elapsed() < Duration::from_secs(10));
       }
   ```

2. Run it and confirm it fails. The run takes about 30 seconds: the wait returns only when the `sleep` the shell started lets go of the two outputs.

   ```
   $ cargo test -p buildl --lib exec::capture
   assertion failed: started.elapsed() < Duration::from_secs(10)
   test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 26 filtered out
   ```

3. Give the outputs a second once the process is gone. In `crates/buildl/src/exec/capture.rs`, replace:

   ```rust
   /// The first pause of a wait in which nothing can arrive to end it. Each pause after it is
   /// twice as long, up to [`TICK`].
   const FIRST_PAUSE: Duration = Duration::from_micros(100);
   ```

   with:

   ```rust
   /// The first pause of a wait in which nothing can arrive to end it. Each pause after it is
   /// twice as long, up to [`TICK`].
   const FIRST_PAUSE: Duration = Duration::from_micros(100);

   /// How long, once the process is gone, its outputs get to reach their end.
   ///
   /// An output still open then is held by something the process started. What that writes
   /// within this time is kept; after it the readers are left behind, and each of them ends when
   /// whatever holds its output closes it or writes to it.
   const DRAIN: Duration = Duration::from_secs(1);
   ```

4. In the doc comment of `capture`, replace:

   ```rust
   /// most `timeout`.
   ///
   /// A process still running after `timeout` is killed. Only that process is: what it started is
   /// not, and stays in this process's group, where a signal from the terminal reaches it.
   ```

   with:

   ```rust
   /// most `timeout`.
   ///
   /// A process still running after `timeout` is killed. Only that process is: what it started is
   /// not, and stays in this process's group, where a signal from the terminal reaches it. Once
   /// the process is gone, an output something else still holds open is waited for [`DRAIN`] and
   /// no longer, so what the process started cannot hold the caller.
   ```

5. In `waited`, replace:

   ```rust
       let mut ended = None;
       let mut timed_out = false;
       let mut pause = FIRST_PAUSE;
       loop {
           let now = Instant::now();
           if ended.is_none() {
               // The process is looked at before the time is, so one that ends on the deadline
               // is not stopped.
               ended = child.try_wait()?;
               if ended.is_none() && deadline.is_some_and(|deadline| now >= deadline) {
                   child.kill()?;
                   ended = Some(child.wait()?);
                   timed_out = true;
               }
           }
           let wait = match ended {
               Some(status) if open == 0 => {
                   return Ok(Capture {
                       status,
                       timed_out,
                       stdout,
                       stderr,
                   });
               }
               Some(_) => TICK,
               None => deadline.map_or(TICK, |deadline| {
                   TICK.min(deadline.saturating_duration_since(now))
               }),
           };
   ```

   with:

   ```rust
       // The status the process ended with, and when to stop waiting for its outputs.
       let mut ended: Option<(ExitStatus, Instant)> = None;
       let mut timed_out = false;
       let mut pause = FIRST_PAUSE;
       loop {
           let now = Instant::now();
           if ended.is_none() {
               // The process is looked at before the time is, so one that ends on the deadline
               // is not stopped.
               let mut status = child.try_wait()?;
               if status.is_none() && deadline.is_some_and(|deadline| now >= deadline) {
                   child.kill()?;
                   status = Some(child.wait()?);
                   timed_out = true;
               }
               ended = status.map(|status| (status, now.checked_add(DRAIN).unwrap_or(now)));
           }
           let wait = match ended {
               Some((status, drained_by)) if open == 0 || now >= drained_by => {
                   // Returning drops the receiver, which is what ends a reader left behind the
                   // next time it has something to send.
                   return Ok(Capture {
                       status,
                       timed_out,
                       stdout,
                       stderr,
                   });
               }
               Some((_, drained_by)) => drained_by.saturating_duration_since(now),
               None => deadline.map_or(TICK, |deadline| {
                   TICK.min(deadline.saturating_duration_since(now))
               }),
           };
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib exec::capture
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`.

8. Commit:

   ```
   $ git add crates/buildl/src/exec/capture.rs
   $ git commit -m "feat(buildl): stop waiting for an output something else holds open"
   ```

### Task 15 — Hand what a command prints to a live sink

**Files:**
- Modify `crates/buildl/src/exec/bare.rs`
- Modify `crates/buildl/src/exec/capture.rs`

**Steps:**

1. Write the failing test of the strategy. In the test module of `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
       use std::path::{Path, PathBuf};
       use std::time::{Duration, Instant};

       use buildl_core::{
           Argument, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy, Exit,
           FileMode, Found, FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath,
           PortName, RawOutcome, ReadyAction, StoredFile, TargetName, Timeout,
       };
   ```

   with:

   ```rust
       use std::path::{Path, PathBuf};
       use std::sync::Mutex;
       use std::time::{Duration, Instant};

       use buildl_core::{
           Argument, Channel, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy,
           Exit, FileMode, Found, FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath,
           PortName, RawOutcome, ReadyAction, StoredFile, TargetName, Timeout,
       };
   ```

2. After the last test, `an_action_past_its_timeout_is_stopped_and_reported_timed_out`, with one blank line above, add:

   ```rust
       #[test]
       fn a_live_sink_is_handed_each_chunk_before_the_command_ends() {
           let (_dir, root) = workspace();
           let heard = root.join("heard");
           let seen = Mutex::new(Vec::new());
           // The command writes a line, then waits for the file `heard` before it writes more.
           // The sink makes that file when it is handed something, so the command can only end
           // when its first line reached the sink while it was still running.
           let live = |channel: Channel, bytes: &[u8]| {
               seen.lock().unwrap().push((channel, bytes.to_vec()));
               fs::write(&heard, "").unwrap();
           };
           let waits = sh(
               "echo one; while [ ! -e heard ]; do /bin/sleep 0.05; done; echo two; echo three >&2",
           );
           let strategy = Bare::new(root, Timeout::parse("20s").unwrap(), system());

           let raw = strategy.run(&(), &waits, Some(&live)).unwrap();

           assert_eq!(raw.exit, Exit::Exited(0));
           assert_eq!(raw.stdout, b"one\ntwo\n");
           assert_eq!(raw.stderr, b"three\n");
           let seen = seen.into_inner().unwrap();
           assert_eq!(seen.first(), Some(&(Channel::Stdout, b"one\n".to_vec())));
           // For each output, what the sink was handed, joined in order, is what the result holds.
           let joined = |wanted: Channel| -> Vec<u8> {
               seen.iter()
                   .filter(|(channel, _)| *channel == wanted)
                   .flat_map(|(_, bytes)| bytes.iter().copied())
                   .collect()
           };
           assert_eq!(joined(Channel::Stdout), raw.stdout);
           assert_eq!(joined(Channel::Stderr), raw.stderr);
       }
   ```

3. Run it and confirm it fails. The run takes about 20 seconds: the sink is never called, so the file the command waits for is never made, and the command is stopped at the test's timeout.

   ```
   $ cargo test -p buildl --lib exec::bare
   assertion `left == right` failed
     left: TimedOut
    right: Exited(0)
   test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 18 filtered out
   ```

4. Write the failing test of the wait. In the test module of `crates/buildl/src/exec/capture.rs`, replace:

   ```rust
       use std::process::Command;
       use std::time::{Duration, Instant};

       use super::{Capture, capture};
   ```

   with:

   ```rust
       use std::process::Command;
       use std::sync::Mutex;
       use std::thread;
       use std::time::{Duration, Instant};

       use buildl_core::Channel;

       use super::{Capture, capture};
   ```

5. In `run`, the helper of that module, replace:

   ```rust
           capture(command, PATIENT).unwrap()
   ```

   with:

   ```rust
           capture(command, PATIENT, None).unwrap()
   ```

6. In `a_child_that_closes_its_streams_is_still_timed_out`, replace:

   ```rust
               &mut sh("exec >&- 2>&-; exec /bin/sleep 30"),
               Duration::from_millis(300),
           )
   ```

   with:

   ```rust
               &mut sh("exec >&- 2>&-; exec /bin/sleep 30"),
               Duration::from_millis(300),
               None,
           )
   ```

7. After the last test, `a_grandchild_holding_the_pipes_does_not_hang_the_run`, with one blank line above, add:

   ```rust
       #[test]
       fn a_live_sink_is_called_on_the_waiting_thread_with_what_is_captured() {
           let caller = thread::current().id();
           let seen = Mutex::new(Vec::new());
           let live = |channel: Channel, bytes: &[u8]| {
               seen.lock()
                   .unwrap()
                   .push((thread::current().id(), channel, bytes.to_vec()));
           };

           let ended = capture(&mut sh("echo out; echo err >&2"), PATIENT, Some(&live)).unwrap();

           let seen = seen.into_inner().unwrap();
           // Never from a reader: whoever is handed the bytes needs no lock of its own.
           assert!(seen.iter().all(|(thread, _, _)| *thread == caller));
           let joined = |wanted: Channel| -> Vec<u8> {
               seen.iter()
                   .filter(|(_, channel, _)| *channel == wanted)
                   .flat_map(|(_, _, bytes)| bytes.iter().copied())
                   .collect()
           };
           assert_eq!(joined(Channel::Stdout), ended.stdout);
           assert_eq!(joined(Channel::Stderr), ended.stderr);
           assert_eq!(ended.stdout, b"out\n");
           assert_eq!(ended.stderr, b"err\n");
       }
   ```

8. Run the tests of the module and confirm they fail to build, because a wait takes no sink yet:

   ```
   $ cargo test -p buildl --lib exec::
   error[E0061]: this function takes 2 arguments but 3 arguments were supplied
   ```

   The line is printed three times, once for each call.

9. Hand each run of bytes to the sink before it is kept. In `crates/buildl/src/exec/capture.rs`, replace:

   ```rust
   use buildl_core::Channel;

   /// How one process ended, and everything it printed.
   ```

   with:

   ```rust
   use buildl_core::{Channel, LiveSink};

   /// How one process ended, and everything it printed.
   ```

10. Replace:

   ```rust
   /// the process is gone, an output something else still holds open is waited for [`DRAIN`] and
   /// no longer, so what the process started cannot hold the caller.
   ///
   /// # Errors
   ///
   /// Returns the failure to start the process, to start a reader, to wait or to kill. A process
   /// that was started is killed and reaped, as far as it can be, before an error is returned.
   pub(crate) fn capture(command: &mut Command, timeout: Duration) -> io::Result<Capture> {
       let mut child = command
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .spawn()?;
       let waited = waited(&mut child, timeout);
   ```

    with:

   ```rust
   /// the process is gone, an output something else still holds open is waited for [`DRAIN`] and
   /// no longer, so what the process started cannot hold the caller.
   ///
   /// With `live`, every run of bytes is handed to it as it arrives, with the output it was
   /// written to, and then kept: `live` is called on the calling thread, once per run of bytes,
   /// each output's in the order written, with exactly the bytes the result holds.
   ///
   /// # Errors
   ///
   /// Returns the failure to start the process, to start a reader, to wait or to kill. A process
   /// that was started is killed and reaped, as far as it can be, before an error is returned.
   pub(crate) fn capture(
       command: &mut Command,
       timeout: Duration,
       live: Option<LiveSink<'_>>,
   ) -> io::Result<Capture> {
       let mut child = command
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .spawn()?;
       let waited = waited(&mut child, timeout, live);
   ```

11. Replace:

   ```rust
   fn waited(child: &mut Child, timeout: Duration) -> io::Result<Capture> {
   ```

    with:

   ```rust
   fn waited(child: &mut Child, timeout: Duration, live: Option<LiveSink<'_>>) -> io::Result<Capture> {
   ```

12. In `waited`, replace:

   ```rust
               Ok(Piece::Chunk(Channel::Stdout, bytes)) => stdout.extend_from_slice(&bytes),
               Ok(Piece::Chunk(Channel::Stderr, bytes)) => stderr.extend_from_slice(&bytes),
               Ok(Piece::Closed) => open = open.saturating_sub(1),
   ```

    with:

   ```rust
               Ok(Piece::Chunk(channel, bytes)) => {
                   if let Some(live) = live {
                       live(channel, &bytes);
                   }
                   match channel {
                       Channel::Stdout => stdout.extend_from_slice(&bytes),
                       Channel::Stderr => stderr.extend_from_slice(&bytes),
                   }
               }
               Ok(Piece::Closed) => open = open.saturating_sub(1),
   ```

13. Pass the strategy's sink on. In `crates/buildl/src/exec/bare.rs`, replace:

   ```rust
       fn run(
           &self,
           (): &(),
           action: &ReadyAction,
           _live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome> {
   ```

    with:

   ```rust
       fn run(&self, (): &(), action: &ReadyAction, live: Option<LiveSink<'_>>) -> Result<RawOutcome> {
   ```

14. In `run`, replace:

   ```rust
           let captured = capture(&mut command, self.timeout).map_err(|error| {
   ```

    with:

   ```rust
           let captured = capture(&mut command, self.timeout, live).map_err(|error| {
   ```

15. Run the tests of the module and confirm they pass:

    ```
    $ cargo test -p buildl --lib exec::
    test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out
    ```

16. Run the gate:

    ```
    $ cargo fmt --all -- --check
    $ cargo make dod
    $ cargo deny check
    ```

    Expected: all three exit `0`.

17. Commit:

    ```
    $ git add crates/buildl/src/exec/bare.rs crates/buildl/src/exec/capture.rs
    $ git commit -m "feat(buildl): hand what a command prints to a live sink"
    ```

---

## Verification summary (plan-level)

- **The gate.** After task 15, `cargo fmt --all -- --check`, `cargo make dod` and `cargo deny check` exit `0`. Run over the finished code in a copy of the workspace that also held the foundation plan's manifests, golden file, `deny.toml` and guard: `cargo make dod` printed `guard-adapter-isolation: OK (2 edges)`, `test result: ok. 32 passed; 0 failed` for the `buildl` unit tests and `[cargo-make] INFO - Build Done in 30.65 seconds.`; `cargo deny --offline check -D unused-wrapper` printed `advisories ok, bans ok, licenses ok, sources ok`. The two edges are `exec -> search_path` and `exec -> state`.
- **Every commit is green.** The fifteen tasks were replayed in order from the three empty modules; after each one `cargo fmt -p buildl -- --check`, `cargo clippy -p buildl --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc -p buildl --all-features --no-deps`, `cargo test -p buildl --all-targets --all-features`, `cargo test -p buildl --all-features --doc` and the guard's script passed. The unit tests after each task, in order: 1, 8, 9, 10, 11, 12, 14, 14, 16, 24, 26, 27, 29, 30, 32 passed.
- **Only these nine files change.** `git status --short` after task 15 lists nothing outside `crates/buildl/src/clock/`, `crates/buildl/src/dispatch/` and `crates/buildl/src/exec/`. `grep -rn 'dead_code' crates/buildl/src/exec crates/buildl/src/dispatch crates/buildl/src/clock` prints nothing: task 10 removed both attributes.
- **Every test the spec names for these modules exists, under its name.** `clock/`: `the_clock_reads_after_the_epoch_and_does_not_run_backwards_between_two_reads` (task 1). `dispatch/`, nine: `every_task_is_run_and_every_result_reaches_step`, `step_is_only_called_on_the_calling_thread`, `no_more_tasks_run_at_once_than_there_are_workers`, `no_task_starts_no_thread`, `one_task_at_a_time_uses_one_thread`, `one_worker_runs_tasks_in_the_order_they_were_handed_out`, `workers_per_core_is_the_machines_parallelism` (task 2), `what_a_task_says_reaches_step_before_its_result_and_in_order` (task 3), `a_task_that_panics_is_lost_and_the_rest_are_reported` (task 4). `exec/`, twenty:

  | Test | File | Task |
  |---|---|---|
  | `collect_reports_a_file_with_its_mode_and_stores_it` | `outputs.rs` | 6 |
  | `collect_reports_nothing_a_link_and_a_special_file_at_the_path`, `collect_reports_everything_below_a_directory` | `outputs.rs` | 7 |
  | `collect_reports_a_name_that_is_not_unicode` (Linux) | `outputs.rs` | 8 |
  | `standard_input_is_empty`, `ten_megabytes_on_each_stream_do_not_block` | `capture.rs` | 9 |
  | `prepare_makes_the_directory_and_leaves_nothing_at_the_path`, `the_command_runs_in_the_workspace_root`, `an_exit_status_is_reported_as_it_is`, `an_action_killed_by_a_signal_is_reported_signaled`, `a_command_that_cannot_be_started_is_an_adapter_error`, `collect_reports_one_entry_per_output_in_order`, `a_store_that_fails_fails_the_collect_with_its_own_error` | `bare.rs` | 10 |
  | `the_located_file_is_the_one_run`, `a_program_that_is_not_located_is_an_adapter_error` | `bare.rs` | 11 |
  | `the_child_sees_only_the_declared_variables` | `bare.rs` | 12 |
  | `an_action_past_its_timeout_is_stopped_and_reported_timed_out` | `bare.rs` | 13 |
  | `a_child_that_closes_its_streams_is_still_timed_out` | `capture.rs` | 13 |
  | `a_grandchild_holding_the_pipes_does_not_hang_the_run` | `capture.rs` | 14 |
  | `a_live_sink_is_handed_each_chunk_before_the_command_ends` | `bare.rs` | 15 |

  Three more are this plan's own: `the_store_holds_what_it_is_handed_and_a_broken_one_refuses` (task 5, the test double held to the port), `an_output_whose_directory_cannot_be_made_fails_the_prepare` (task 10, the error `prepare` raises and the shape of its diagnostic) and `a_live_sink_is_called_on_the_waiting_thread_with_what_is_captured` (task 15, that no reader thread ever calls the sink).
- **The timing tests hold on a loaded machine.** The finished test binary was run 30 times on an idle machine and 30 times beside 32 busy loops on 8 cores, 16 test threads each time: no failure in 60 runs. No test asserts that something took at least some time; each asserts an ending (`TimedOut`, `Exited(0)`, the bytes kept) and a bound of 10 seconds on something that takes 0.3 or 1 second, against a process that would otherwise run for 30. The live-sink test of task 15 has no timing at all: the command cannot end before the sink was handed its first line.
- **Linux.** Everything above ran on macOS. The ubuntu leg of CI is the first Linux run of the `exec/` tests, of the paths `/bin/sh`, `/bin/cat`, `/bin/sleep`, `/usr/bin/head`, and of `collect_reports_a_name_that_is_not_unicode`. That last test was compiled and linted here by turning its gate to `target_os = "macos"` in a throwaway copy, where it fails at the first step, as it must: `Illegal byte sequence` (os error 92) on creating the name. The code it covers has never run.
- **The pool under the real pipeline.** The finished `ThreadPool` was put in a bundle with `buildl-core`'s own test fakes for every other port and run through `Pipeline::build` on 41 targets: 41 built and then 41 cached with 133 reporter calls; a strategy that panics for `//:leaf07` ends the build as `dispatcher: returned without an outcome for //:all, //:leaf07` with 39 rows kept; one worker runs the forty leaves in label order. That run used `buildl-core`'s test fakes by path, which a unit test of `dispatch/` may not do, so it is a proof of this plan and not a test in it; the end-to-end tests of `crates/buildl/tests/e2e` are where the pool runs under the pipeline in the repository.
- **What has no test.** The two branches of `serve` for a worker thread that cannot be started, and the branch of `capture` that kills and reaps the child when a reader cannot be started or a wait fails: std gives no way to make a thread fail to start or a wait fail. `collect`'s error for an output that cannot be read is raised through the same `fault` function the tested errors use.
