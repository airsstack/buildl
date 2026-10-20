---
status: approved
created: 2026-10-19
built-on: 6e1aa4d56501
---

# Spec: parallel load — evaluate build files across workers

Load evaluates one build file at a time. This spec runs Load's directory queue through the
`Dispatcher` port the executor already uses, so build files are evaluated on up to
`[execution] workers` threads, while the declarations Load returns, and the error it reports
when files fail, stay exactly those of serial load. The serial loop survives as a replay over
results that arrive in any order; it decides the result and the error, and the workers only make
the results arrive sooner.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-19 at commit `bc6bea6`.

|#|Premise|Evidence|
|---|---|---|
|P1|Load is one loop over a queue: evaluate the front directory's file; `Absent` is `MissingBuildFile { directory, requested_by }`; resolve its `subdir` requests and queue each unseen one with this file as requester; convert each staged declaration; at the end, sort.|`crates/buildl-core/src/load/traversal.rs:44-73`.|
|P2|A file's `subdir` requests resolve sorted and without repeats; the sort of the result orders by directory, name, then the whole declaration, so it does not depend on evaluation order.|`load/traversal.rs:120-134` (`resolve_subdirs`: `sort`, `dedup`), `:288-299` (`declaration_order`, `sort`).|
|P3|`LuaSource` evaluates identically from several threads at once.|Probe (temporary integration test in `crates/buildl-lua/tests`, run and deleted): fixture `crates/buildl-lua/tests/fixtures/workspace`, root and `lib` build files; 25 rounds × 8 scoped threads → `PROBE: 200 concurrent evaluations, all equal to serial`. `LuaSource` is asserted `Send + Sync` at `crates/buildl-lua/src/source.rs:249-250`.|
|P4|`Error`, `Declaration`, `Evaluated` and `Directory` are `Send`.|Same probe: `const fn send<T: Send>() {}` instantiated for each type compiled.|
|P5|`buildl-core` starts no thread; parallel work goes through the `Dispatcher` port, generic over task, message and result.|`grep -rn "std::thread\|thread::" crates/buildl-core/src` → no match; control: the same pattern finds `crates/buildl/src/dispatch/thread_pool.rs:15` (`use std::thread`). `crates/buildl-core/src/ports/dispatcher.rs:1-5`, `:46-53` (`dispatch<T, M, R, W, S>`; `W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync`; `S: FnMut(Turn<M, R>) -> Vec<T>`; every returned task is run; `step` runs on the calling thread).|
|P6|`ThreadPool` hands a task to a worker only when one is free, in the order `step` returned them, so a waiting task starts after `step` has seen every earlier result.|`crates/buildl/src/dispatch/thread_pool.rs:66-70`.|
|P7|The pipeline's dispatcher is `ThreadPool::new(execution.workers)`; `Workers::on(cores)` gives one per core for `0`.|`crates/buildl/src/local_ports.rs:241`; `thread_pool.rs:32-35` (`ThreadPool::new`: `workers.on(cores)`).|
|P8|The executor already stops work through an `AtomicBool` shared between `work` and `step`.|`crates/buildl-core/src/execute/perform.rs:16`, `:46` (`stop: &'a AtomicBool`).|
|P9|Every caller of `buildl_core::load`.|`grep -rn "load(" crates --include='*.rs'`, less `fn load`, atomic `.load(Ordering…)` and comments, counted per file → `buildl-core/src/pipeline/driver.rs` 6: `:124` (`plan`), `:159` (`build`), `:209` (`test`), `:236-237` (`check`, twice), `:248` (`graph`; `query` calls `graph`); `buildl-core/tests/flows/load.rs` 9; `buildl-core/tests/flows/common.rs` 1 (`:1411`); `buildl-lua/tests/flows/load.rs` 1 (`:59`). The other three matches, `buildl-lua/src/{primitives.rs:135, stable_text.rs:136, values.rs:201}`, are `lua.load(source)`, Lua's own.|
|P15|`buildl-lua`'s dev-dependencies hold no dispatcher, and it may not depend on `buildl`.|`crates/buildl-lua/Cargo.toml` dev-dependencies: `tempfile` only; `CLAUDE.md` structural rules ("`buildl-lua` never depends on `buildl`").|
|P16|One existing flow test asserts the order of evaluation.|`crates/buildl-core/tests/flows/load.rs:90-100` (`a_directory_requested_twice_is_evaluated_once`: `[dir(""), dir("a"), dir("a/x")]`).|
|P10|`check`, `graph` and `query` are free functions taking only a source, and the `Workspace` facade calls them with `wiring.source()`; `Pipeline::check/graph/query` take `&self`.|`pipeline/driver.rs:74`, `:88`, `:99`, `:235`, `:247`, `:260`; `crates/buildl/src/workspace.rs:95-120`.|
|P11|`ContentStore` requires `Send + Sync` on the trait, with a reason; `DeclarationSource` requires neither.|`crates/buildl-core/src/ports/content_store.rs:22-24`; `ports/declaration_source.rs:58`.|
|P12|The flow tests' `FakeSource` holds `Cell` and `RefCell`, so it is not `Sync`; `FakeDispatcher` can take tasks oldest or newest first, one at a time or all together.|`crates/buildl-core/tests/flows/common.rs:44-48`; `:791-817` (`Order::{OldestFirst, NewestFirst}`, `Pace::{OneAtATime, Together}`), `:834`.|
|P13|`DispatcherFault` names labels; the CLI maps it to an internal error.|`crates/buildl-core/src/error.rs:373-376`; `crates/buildl-cli/src/exit.rs:114`.|
|P14|The other `DeclarationSource` implementations hold no interior mutability.|`ports/bundle.rs:49` (`Nothing`), `ports/port_set.rs:116` (`Stub`), `ports/declaration_source.rs:36` (`RootOnly`), `crates/buildl/src/report/plan_text.rs:230` (`Unused`): each a unit struct.|

## 2. Behaviour

|Aspect|Behaviour|
|---|---|
|Workers|Build files are evaluated on the pipeline's dispatcher: up to `[execution] workers` at once, `0` one per core (P7). `workers = 1` evaluates one at a time in serial order (P6).|
|Result|The declarations are those serial load returns, in the same order (P2), for any worker count and any timing.|
|Error|The error is the one serial load returns: the first failure in serial queue order, with serial's `requested_by`.|
|Stop|Once the replay meets that error, no further build file is evaluated: a task a worker takes afterwards returns at once. Files already being evaluated finish and their results are dropped.|
|Commands|`check` (both loads), `graph`, `query`, `plan`, `build`, `test` and `gc` load in parallel. `log` and `clean` load nothing.|
|Limits|Each file keeps its own engine and byte budget; `buildl-lua`'s library code does not change (P3). Its one test that calls `load` gains a test dispatcher (§3.4).|

## 3. Components

### 3.1 `load` (`load/traversal.rs`, new `load/evaluation.rs` and `load/loading.rs`)

```rust
pub fn load<S, D>(source: &S, dispatcher: &mut D, entry: &EntryName) -> Result<Vec<Declaration>>
where S: DeclarationSource, D: Dispatcher;
```

**Work** (`evaluation.rs`, any thread): for a directory, unless the stop flag is set, evaluate
its build file and, in serial's order (P1), resolve its `subdir` requests, then convert each
declaration:

```rust
enum FileResult {
    Stopped,                                     // the stop flag was set before it began
    Absent,
    Failed(Error),                               // evaluation, subdir or conversion error
    Loaded { subdirs: Vec<Directory>, declarations: Vec<Declaration> },
}
```

**Step** (calling thread) is a struct of its own, `Loading` (new `load/loading.rs`), so its
decisions are unit-tested without a dispatcher; `load`'s `step` closure only forwards to it:

```text
 Loading::begin()        → [root]; dispatched = {root}
 Loading::done(dir, r)   → results[dir] = r
                           replay already met its error → return []        (r is dropped)
                           tasks = r is Loaded ? its subdirs not yet dispatched, in order : []
                           replay.advance(&results)
                           replay met an error now → stop = true; return []
                           return tasks
```

**Replay** — the serial loop of P1, reading `results` instead of calling the source: while the
front of its queue has a result, take it — `Absent` is `MissingBuildFile` with the requester the
replay recorded, `Failed(e)` is `e`, `Loaded` queues its unseen subdirs with this file as
requester and keeps its declarations. When the queue is empty the replay is done.

**After dispatch**: replay done → sort (P2) → `Ok`. Replay met an error → `Err` of it. Replay
still waiting on a directory with no result → `Error::LostBuildFiles { directories }`, every
dispatched directory without a result, in order.

|Rule|Reason|
|---|---|
|Subdirs are dispatched from every result as it arrives, not in replay order.|No barrier: every worker stays busy. An evaluation the serial loop would not have made is possible only past the error, and its result is dropped.|
|`requested_by` is the replay's, not whichever parent finished first.|`MissingBuildFile` names serial's requester.|
|A `Failed` file dispatches no children.|The replay stops at that file at the latest, and its children come after it in serial order.|
|The stop flag is an `AtomicBool` on `load`'s stack, read by `work` and set by `step`.|The executor's precedent (P8); core still starts no thread (P5).|

### 3.2 `DeclarationSource: Sync`

The trait gains the `Sync` supertrait, documented as `ContentStore` documents its own (P11):
several build files are evaluated at once, each from the thread that took it. `LuaSource` is
already `Sync` (P3); the four other implementations are unit structs (P14); `FakeSource` moves
from `Cell`/`RefCell` to `Mutex` (P12).

### 3.3 `Error::LostBuildFiles`

`LostBuildFiles { directories: Vec<Directory> }` — never empty, in directory order; displays as
`dispatcher: returned without evaluating the build files of {dirs}`. It is a broken dispatcher
contract, as `DispatcherFault` is, and maps to the same exit (P13). The `Dispatcher` port's doc
(`ports/dispatcher.rs:38-40`, "a build over it fails with `DispatcherFault`") names both: a
build's lost action is `DispatcherFault`, a load's lost build file `LostBuildFiles`.

### 3.4 Callers

|Caller|Change|
|---|---|
|`Pipeline::plan/build/test`|`load(&self.ports.source, &mut self.ports.dispatcher, entry)` (P9).|
|`pipeline::check/graph/query` (free)|gain `dispatcher: &mut D`; `check` hands it to both loads.|
|`Pipeline::check/graph/query`|take `&mut self` and pass `&mut self.ports.dispatcher` (P10).|
|`pipeline::gc` (the clean-gc chain)|gains `dispatcher: &mut D` for its load.|
|`Wiring::dispatcher()`|new: `ThreadPool::new(self.workspace.execution.workers)`, the same value `ports` uses (P7).|
|`Workspace::check/graph/query/gc`|pass `&mut self.wiring.dispatcher()`.|
|`buildl-core/tests/flows/load.rs`, `common.rs:1411`|pass a `FakeDispatcher` (P9, P12).|
|`buildl-lua/tests/flows/load.rs:59`|passes a one-thread test dispatcher defined in `buildl-lua/tests/flows/common.rs`: it runs each task on the calling thread, oldest first. Test code only; `buildl-lua` gains no dependency (P15).|
|doc comments of the free functions (`pipeline/driver.rs:225-229`, "a caller that holds a source and no other port")|say a source and a dispatcher.|

No module edge is added: `load -> ports` and `load -> types` exist (`crates/expected-module-edges.txt`).

## 4. Testing

The intent's "evaluations overlap" is proven in two halves: `buildl-core`, which starts no
thread (P5), proves it hands every known file out at once; `buildl` proves that a real
dispatcher then evaluates them at once.

|Layer|Proves|
|---|---|
|`Loading` unit (`load/loading.rs`)|**hand-out:** `done(root, Loaded{subdirs: [a, b, c]})` returns `[a, b, c]` before any of them is done; a subdir already dispatched is not returned again. **replay**, fed results in every arrival order: the declarations of serial; with two failing files, serial's error; two parents naming one missing directory → serial's `requested_by`; the error sets the stop flag and every later `done` returns `[]`; a waiting directory with no result → `LostBuildFiles`.|
|`evaluation` unit (`load/evaluation.rs`)|the work function: `Stopped` when the flag is set, without calling the source; `Absent`; `Failed` for an evaluation, a subdir and a conversion error, in serial's order within one file; `Loaded` otherwise.|
|`buildl-core` flows (`tests/flows`, existing `FakeDispatcher`, P12)|`load.rs`: for `OldestFirst`/`NewestFirst` × `OneAtATime`/`Together`, the same declarations and the same error as the default dispatcher, compared as values; a task taken after the error evaluates nothing (`FakeSource` records each evaluation); a lost task (`Loss::Unreported`) → `LostBuildFiles`. `a_directory_requested_twice_is_evaluated_once` (P16) keeps its exact order under the default dispatcher, whose order is serial's. `check.rs`, `graph.rs`, `query.rs`, `plan.rs`, `build.rs`, `test.rs`, and the clean-gc chain's `gc` flow, pass with `FakeSource` on a `Mutex`.|
|`buildl` adapter|**overlap:** `ThreadPool` at `workers = 2` over a test source whose two sibling files each wait, on a `Condvar` with a 10 s timeout, until both are being evaluated; the test fails on the timeout, so it passes only if the two evaluations overlap. `ThreadPool` + `LuaSource` over a fixture of many directories: equal declarations at `workers` 1, 2 and 8.|
|e2e via `Workspace`|`graph.json` byte-identical at `workers = 1` and `workers = 8`; a workspace with two broken build files reports the same error text at both counts.|
|CLI|nothing new: no flag, no output changes.|

## 5. Documents

|File|Change|
|---|---|
|`docs/architecture.md` §3.1|the "one at a time today … is its design" paragraph describes the built design: the dispatcher, eager dispatch, the serial replay, the stop flag. The §3 phase table lists `Dispatcher` among Load's ports.|
|`docs/architecture.md` §4 (`:320`), §5 rule 3 (`:352`), §6 (`:375`)|the error table gains `LostBuildFiles`; rule 3 drops "Load is sequential today, so this clause has nothing to hold yet"; the "A dispatcher that loses a task" record names Load's error beside the build's.|
|`docs/design.md` §4|the `workers` row: the number of build files evaluated and actions run at once.|
|`docs/design.md` §8.1, §11.1|"parallelises … if needed" / "parallelises across engines" describe parallel load as built, over the same workers.|
|`docs/architecture-building-blocks.md`|`DeclarationSource: Sync` in the trait listing (`:191`); the `dispatch` row (`:148`) and the `Dispatcher` port row (`:445`) name Load as a user; the `load` row (`:415`) lists `Dispatcher` beside `DeclarationSource`; `:425` says the free functions take a source and a dispatcher; the `DeclarationSource` row (`:433`) drops "one build file at a time"; the failure table (`:530`) gains the lost build file.|
|`README.md`, `CLAUDE.md`|the status lines say build files load in parallel.|

## 6. Non-goals

- Demand-driven loading (the chain after this one).
- Parallelism inside one build file, or one engine shared between files.
- Caching evaluation results between runs.
- A separate setting for load workers.
- Changes to `buildl-lua`'s library code, the declaration format or any state file.
