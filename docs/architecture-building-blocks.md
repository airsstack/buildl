# buildl — architecture building blocks

**Status: built for the core pipeline, for `verify`, which re-runs cached actions and compares what they produce with the cache, for grant negotiation: the `Approver` port, the `grants` logic and `doctor`, and for plugins and discovery targets: an origin on every declaration, the plugin check in Plan, attribution, the `plugins` module of `buildl`, and a second load of the build files that reads the results of the discovery targets, and for the remote cache: the `RemoteCache` port, the worker's lookup and upload steps in `execute`, and the `remote` module of `buildl`, which speaks the Remote Execution API.** What this document describes exists, except what arrives with a later item of design §13. There is no `Grants` value: failure is closed, so a wanted set inside the ceiling is the whole grant, and the approver answers yes or no. Companion to [`design.md`](./design.md) and [`architecture.md`](./architecture.md): `design.md` says _what buildl is and why_, `architecture.md` says _how each part works_ — algorithms, types, determinism rules, runtime decision records — and this document says _how the parts are put together_: the C4 views of the system [2], the four crates, the ports between them, and the dependency rules that keep the logic of a build testable without any concrete implementation. It is the building block view of the architecture [8] and owns the workspace layout; `architecture.md` §1 defers to it.

**Author:** rstlix0x0 · **Date:** 2026-09-13 · **Reviewers:** —

---

## 1. Principles

### 1.1 Dependency inversion

The logic of a build — loading, resolving, planning, scheduling, recording — depends only on abstractions it defines itself. Concrete implementations (the airsl engine, the filesystem, child processes, threads, the system clock) depend on those abstractions, and exactly one place chooses which implementations run together [3]. The abstractions are _ports_ and the implementations are _adapters_ [4].

```mermaid
%% Dependency direction — arrows point at the dependency
graph TD
    CLI["buildl-cli<br/>argument surface"] --> LIB["buildl<br/>adapters + composition root"]
    LIB --> LUA["buildl-lua<br/>DeclarationSource via airsl"]
    LIB --> CORE["buildl-core<br/>domain data · ports · pure logic"]
    LUA --> CORE
    LUA --> AIRSL["airsl"]
```

The consequence the structure exists for: **every flow of the pipeline runs in `buildl-core`'s tests against fake ports** — no Lua state, no filesystem, no process spawn, no thread (§8).

### 1.2 What `buildl-core` may depend on

Dependency inversion targets I/O and volatile subsystems. A stable library doing pure computation is not the implementation of a port, so the core may use one directly.

|`buildl-core` may depend on|`buildl-core` must not depend on|
|---|---|
|`std`, excluding its I/O, process, thread, environment, and clock APIs|`airsl`, `mlua`, `toml`, `clap`|
|`serde`, `serde_json`|`std::fs`, `std::process`, `std::thread`, `std::env`, `SystemTime`|
|`thiserror`|`rayon`, `tempfile`, `walkdir`, `globset`|
|`sha2`|any other buildl crate|

The table is the dependency rule. `crates/buildl-core/clippy.toml` is its enumeration: 130 entries
across `disallowed-methods`, `disallowed-types` and `disallowed-macros`, partitioned into nine
families — the filesystem (41, counting the nine `Path` methods that reach it), the environment
(23, including the compile-time reads and the two `PATH`-format helpers), threads (20), processes
(15), the platform filesystem extensions (13), the standard streams (10, including the print
macros), the clock (3), the network (3) — and floating-point numbers (2), which are not an I/O
surface at all. The nine counts sum to the 130, so the partition is the whole config. A float is
banned because a non-finite one serializes as JSON `null`, so a value could reach an action key
differing from the value it was computed from (`architecture.md` §5 rule 2). `cargo make
guard-core-purity` asserts that every entry still resolves, that no source file suppresses the
lints, and that the config is not switched off.

### 1.3 The name `buildl`

Three different things carry the name, and the crate structure keeps them apart:

|Name|What it is|Lives in|
|---|---|---|
|`buildl` binary|the command users run|`buildl-cli` (`[[bin]] name = "buildl"`)|
|`buildl` Lua global|the declaration framework build authors call (design §5)|`buildl-lua` (the `buildl` `HostModule`)|
|`buildl` Rust crate|the complete framework: every real adapter wired onto `buildl-core`|`crates/buildl`|

## 2. System context — C4 level 1

```mermaid
%% C4 level 1 — system context
graph TD
    AUTHOR["[Person]<br/>Build author<br/>writes build.lua, runs buildl"]
    CI["[Person]<br/>CI pipeline<br/>runs build and plan, reads the exit class"]
    BUILDL["[Software system]<br/>buildl<br/>sandboxed, deterministic build system:<br/>Lua declares, Rust executes"]
    WS["[External system]<br/>Workspace filesystem<br/>sources, out/"]
    TOOLS["[External system]<br/>Toolchains<br/>go, cargo, bun, docker"]
    REMOTE["[External system]<br/>Remote cache — REAPI over gRPC<br/>action results shared between machines"]
    GIT["[External system]<br/>Plugin repositories<br/>read by buildl plugins fetch"]
    AUTHOR -->|"writes build files, invokes commands"| BUILDL
    CI -->|"invokes build and plan"| BUILDL
    BUILDL -->|"reads, writes"| WS
    BUILDL -->|"spawns, no shell"| TOOLS
    BUILDL -->|"looks results up; stores them when asked"| REMOTE
    BUILDL -->|"fetches a pinned revision, through the system git"| GIT
```

## 3. Containers — C4 level 2

A C4 container is a separately running process or a data store [2]. The crates are not containers: they compile into one process and appear at level 3.

```mermaid
%% C4 level 2 — containers
graph TD
    subgraph SYS["buildl — system boundary"]
        CLIP["[Container: Rust binary]<br/>buildl CLI process<br/>Load → Resolve → Plan → Execute → Record"]
        DEFS["[Data]<br/>Build definitions<br/>buildl.toml, buildl.lock, build.lua, plugins"]
        STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, cache.journal,<br/>log.jsonl, statcache.json,<br/>graph.json, lock, plugins/"]
        OUT["[Data store]<br/>out/<br/>hard links into the cas"]
    end
    CHILD["[Container]<br/>Child processes<br/>one toolchain invocation per action"]
    CLIP -->|"reads"| DEFS
    CLIP -->|"cas first, row after, log always"| STATE
    CLIP -->|"links named outputs"| OUT
    CLIP -->|"spawns argv with granted env"| CHILD
```

## 4. Components — C4 level 3

Inside the CLI process. Arrows point at the dependency: every adapter depends on `buildl-core`, and the one edge between adapter crates is `buildl → buildl-lua`, which the composition root alone may hold.

```mermaid
%% C4 level 3 — components inside the CLI process
graph TD
    CLI["[Component: crate] buildl-cli<br/>clap argument surface<br/>maps outcome classes and<br/>error variants to exit codes"]
    LIB["[Component: crate] buildl — composition root<br/>adapters: manifest, host, digest, tools,<br/>cas, cache, log, statcache, graph,<br/>exec, dispatch, clock, report, approver, remote<br/>no port: plugins — lock, check, fetch<br/>shared: state, search_path<br/>assembly: LocalPorts, Workspace facade"]
    LUA["[Component: crate] buildl-lua<br/>implements DeclarationSource<br/>airsl Engine per build file<br/>buildl HostModule bound as a global<br/>buildl.use: one plugin instance per chain"]
    subgraph CORE["[Component: crate] buildl-core — no I/O, no Lua, no threads"]
        DATA["domain data<br/>Label, Digest, NodeId, Provenance<br/>Declaration, Origin, Discovered<br/>TargetGraph, Plan, Attribution<br/>KeyComponents, ActionKey<br/>RemoteAction, RemoteHit, RemoteUpload<br/>ActionOutcome, OutcomeClass"]
        PORTS["ports — traits<br/>DeclarationSource, Manifest, Host<br/>Digester, StatCache, ToolResolver<br/>ContentStore, ActionCache, EventLog<br/>RunHistory, Sweep, ExecStrategy, Dispatcher<br/>Clock, GraphRecord, Reporter<br/>Approver, RemoteCache"]
        LOGIC["pure logic<br/>load, resolve, placement, plan, grants<br/>discovery, Schedule, record, verify<br/>canonical JSON"]
    end
    AIRSL["[External crate] airsl<br/>Engine, Policy, HostModule"]
    CLI -->|"calls"| LIB
    LIB -->|"wires"| LUA
    LIB -->|"implements"| PORTS
    LIB -->|"wires into Pipeline"| LOGIC
    LUA -->|"implements"| PORTS
    LUA -->|"evaluates with"| AIRSL
    LOGIC --> PORTS
    LOGIC --> DATA
    PORTS --> DATA
```

|Crate|Role|Depends on|Must not depend on|
|---|---|---|---|
|`buildl-core`|domain data, ports, the pure logic of every phase|`serde`, `serde_json`, `thiserror`, `sha2`|any buildl crate, `airsl`, any I/O API|
|`buildl-lua`|the `DeclarationSource` adapter: evaluates one build file through airsl, and loads the plugins it uses|`buildl-core`, `airsl`|`buildl`|
|`buildl`|every other adapter; the `plugins` module; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core` as `buildl::buildl_core`|`buildl-core`, `buildl-lua`, `toml`, `toml_edit`, `rayon`, `serde`, `thiserror`, `tempfile`, `tonic`, `tonic-prost`, `prost`, `prost-types`, `tokio`, `uuid`|— (composition root, §7)|
|`buildl-cli`|argument surface and exit codes|`buildl`, `clap`|`buildl-core` and `buildl-lua` directly|

Each adapter dependency has its reason recorded in the workspace `Cargo.toml`: `toml` reads `buildl.toml`; `rayon` hashes files, in the `Digester` adapter and nowhere else; `serde` derives the manifest's tables and the two lines the state adapters write; `thiserror` derives the state lock's error; and `tempfile` gives the temp-file-then-rename that every state file and blob is written by. `toml_edit` places a refusal of a whole `[remote]` table, where `toml` keeps no position. The remote cache's gRPC client is `tonic`, with `tonic-prost`, `prost` and `prost-types` for the messages it sends, `tokio` for the runtime the client needs, and `uuid` for the names of ByteStream uploads. They are named by the `remote` module and by no other, and `deny.toml` lets only `buildl` (and tonic's and prost's own companion crates) depend on `tonic`, `prost` and `uuid` (§7). `tokio` is not banned by name: besides `buildl`, its direct dependents are the HTTP and gRPC stack under tonic (`tonic`, `tower`, `h2`, `hyper`, `hyper-util`, `hyper-timeout`, `tokio-rustls`, `tokio-stream`, `tokio-util`), and `guard-crate-edges` keeps every other member off it by diffing each member's direct dependencies against `crates/expected-edges.txt`.

Inside `buildl` there is one module per adapter, `plugins`, which serves no port but is isolated as an adapter is, two shared modules that implement no port, and two composition modules (§7 rule 2):

|Module|Holds|Implements|
|---|---|---|
|`manifest`|`buildl.toml`, finding the workspace root, and the `[plugins.<name>]` tables: where each plugin comes from, and the cap that goes in the `Workspace`|`Manifest`|
|`host`|the OS, the architecture, environment values|`Host`|
|`digest`|what stands at an input's path, and file hashing on rayon|`Digester`|
|`tools`|a program's lookup and its fingerprint|`ToolResolver`|
|`cas`|`.buildl/cas`, and the files placed under `out/`; listed and pruned through `Sweep`|`ContentStore` and `Sweep`|
|`cache`|`cache.json` and `cache.journal`|`ActionCache`|
|`log`|`log.jsonl`, appended to and read back|`EventLog` and `RunHistory`|
|`statcache`|`statcache.json`|`StatCache`|
|`graph`|`graph.json`|`GraphRecord`|
|`exec`|tier 1, `bare`; tier 2, `sandbox`, which clears the real output paths or, for `verify`, leaves them (`OutPaths`); and `Local`, which picks between them from the manifest|`ExecStrategy`|
|`dispatch`|worker threads for Load and Execute|`Dispatcher`|
|`clock`|the system clock|`Clock`|
|`report`|terminal output, and every rendering of a value into text|`Reporter`|
|`approver`|the three approvers a manifest can name and `LocalApprover`, which picks one; the `Console` an interactive one asks on; the approvals it remembers, outside the workspace|`Approver`|
|`remote`|the remote cache over the Remote Execution API [9]: the tokio runtime that drives a gRPC client, the channel, the login, the cache's capabilities, blob transfer, the input tree, the encoding of buildl's remote values as REAPI messages, `Reapi` and `Remote`, the cache a run is given, and `token_is_present`, which `doctor` asks; behind the `test-server` feature, an in-memory server for tests. The generated REAPI types are `include!`d from `generated/reapi/`|`RemoteCache`|
|`plugins`|`buildl.lock`; the digest of a plugin's files; the check that every plugin `buildl.toml` names is there and unchanged; fetching git plugins into `<state>/plugins` through the system `git`. It names `state` and `search_path` and no other module, and `manifest` and it share only values in `buildl-core`|— (`local_ports` runs its `check`, `workspace` its `fetch`)|
|`state`|shared: where each state file is, the state lock, writing a file whole, clearing a path, the scratch directory the sandbox runs actions in|—|
|`search_path`|shared: the lookup of a program name|—|
|`local_ports`|composition: the `Ports` bundle and its wiring|—|
|`workspace`|composition: the `Workspace` facade|—|

`DeclarationSource` is `buildl_lua::LuaSource`.

`buildl` and `buildl-cli` build on Unix targets only, and the supported platforms are Linux and macOS, the two the CI matrix runs. `buildl` uses `std::os::unix` for file modes and for the signal that ended a process.

## 5. Code — C4 level 4: `buildl-core`

The signatures below are the shape, not the final API.

### 5.1 Domain data

Every value is a validated newtype or a serializable handoff; field-level detail is in `architecture.md` §2.

|Kind|Types|
|---|---|
|Names and identity|`Directory`, `TargetName`, `Label`, `NodeId`, `Provenance`|
|Content|`Digest` — the single SHA-256 type for files, outputs, keys, and log blobs|
|Outputs|`OutputManifest` — what one output is, a file or a directory of files, and the value an output's digest is taken of — with `StoredFile` and `FileMode` (a file's digest and whether it may be run), `Tree`, `TreeEntry` and `TreePath` (a directory's files, in path order); `Found`, `FoundEntry` and `FoundKind` — what a strategy reports at an output path|
|Time|`Timestamp` — nanoseconds since the Unix epoch, the current one obtained only through the `Clock` port|
|Phase handoffs|`Declaration`, `TargetGraph`, `Plan`, `ActionOutcome`|
|Execution handoffs|`OutputPath` — where one output lives under `out/` — `ReadyAction` — one action with its command expanded, and its rule's description expanded with it as an `ExpandedDescription` — `RawOutcome` and `Exit` — how a run ended and what it printed — `OutcomeEvent` — one line of the log — `OnFailure`, `Summary`|
|Incrementality|`KeyComponents`, `ActionKey`|
|Authority|`Ceiling`, `WantedSet`, `Want`, `Workspace`; `Approval` and `DenialReason` — an approver's answer — `ApproverMode` — which approver the manifest names — `Attribution` — which plugins' targets are behind each want, shown to an approver and never part of the wanted set — and `Checkup` — what `doctor` reports, plugin violations included|
|Plugins|`PluginName`, `PluginVersion` and `PluginRef` — a plugin and what its own manifest allows — `PluginChain` — the plugins that made a declaration, outermost first, never empty — `Origin` — a build file, or a plugin chain, carried by every `Declaration` and `Node` and by no key — `PluginCap` and `PluginNetwork` — what a workspace lets one plugin have — `PluginViolation` — a want a plugin does not allow — `PluginSource`, `GitUrl`, `Rev`, `TagName` and `CommitId` — where a plugin's files come from|
|Discovery|`Discovered` — the last result of each discovery target, shared behind an `Arc` and handed to every evaluation — `Loaded` — a load's declarations and the discovery targets with no result — and `TargetRole::Discovery`|
|Remote cache|`RemoteSettings` — the manifest's `[remote]` table, held in `Workspace`, with `RemoteUrl`, `RemoteScheme`, `InstanceName`, `HostPath`, `TokenSource` and `HeaderName`, each validated at construction — `RemoteAction`, `RemoteFile` and `RemotePlatform` — a task as the cache is asked about it — `RemoteHit`, `RemoteOutputFile`, `RemoteOutputDirectory` and `RemoteTreeFile` — the result it answers with, unchecked — `RemoteUpload`, `RemoteOutput`, `RemoteContent`, `RemoteTree` and `Uploaded` — what a run stores — `Blob` — a digest with its size — `RemoteUse`, `RemoteFailure` and `RemotePolicy` — what a run does with the cache and what a failure does, enums and not flags — `RemoteSwitch` — whether a failed call has turned the cache off for the command — `RemoteCall` and `RemoteNotice` — what a build says about it|
|Execution settings|`Execution` — the manifest's `[execution]` table, held in `Workspace` — with `Workers`, `Timeout` and `Isolation`, each validated at construction; none is a key component (`architecture.md` §6)|
|A run and what it shows|`Scope` — what a run was asked for: the workspace, or named targets — `Purpose` — whether the run builds or tests — `Rerun` — whether a run runs its tests again whatever their keys say — `LiveOutput` — whether one action is shown as it prints — `Channel`, `LiveSink` and `Captured` — which stream an action wrote to, where its bytes go as they are read, and what it printed — `Turn` — what a dispatcher hands the scheduling thread|
|Inputs|`InputStat` and `InputKind` — what stands at an input's path: nothing, a regular file, or a link, a directory or a special file; `Hasher` — `Digest::of` for bytes handed over in pieces|
|Log, read back|`LogLine` — one line of the log, an event or a damaged line by its number — `LoggedRun` — one event as it reads back, the twelve keys `OutcomeEvent` writes|
|A verify and what it found|`Sample` — which cached targets re-run — `Check` — what a verify found for one target: verified, mismatched, failed, skipped or unplaced — `OutputDifference`, `FileChange` and `EntryChange` — the first way an output differed — `Verification` — every check in label order, with the counts of targets not cached and not sampled|
|Classification|`OutcomeClass`, `InfraKind` and `OutputRefusal` (`#[non_exhaustive]`, `architecture.md` §4); `TreeFault` — why a list of files is not a tree|

### 5.2 Ports

```rust
/// Evaluates one build file. The directory queue, `subdir` handling, and the
/// sorted merge across files are Load logic in `buildl-core`, not the adapter's.
/// A directory with no build file is `Evaluated::Absent`; whether that is an
/// error is Load's decision. `discovered` holds what discovery targets last produced:
/// a build file that asks for one gets its bytes, or nothing, and Load passes the same
/// map to every file it evaluates.
pub trait DeclarationSource: Sync {
    fn evaluate(&self, file: &BuildFile, discovered: &Discovered) -> Result<Evaluated>;
}

/// What the workspace's manifest says: its output and state directories, its ceiling,
/// and its execution settings. The phases read the directories and the ceiling; the
/// execution settings are for whoever assembles the dispatcher and the strategy.
pub trait Manifest {
    fn workspace(&self) -> Result<Workspace>;
}

/// The machine actions run on. `Ok(None)` is an unset variable.
pub trait Host {
    fn platform(&self) -> Platform;
    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>;
}

/// What stands at its path is asked of every input, following no symbolic link:
/// nothing, a regular file with its metadata, or something that is not a regular
/// file. Digests are asked only of files whose metadata changed, all in one call so
/// an adapter may hash in parallel.
pub trait Digester {
    fn stat(&self, path: &SourcePath) -> Result<InputStat>;
    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>>;
}

/// Stores entries. Whether one may be reused is Plan's decision, not the cache's.
pub trait StatCache {
    fn entry(&self, path: &SourcePath) -> Option<StatEntry>;
    fn record(&mut self, path: SourcePath, entry: StatEntry);
    fn commit(&mut self) -> Result<()>;
}

/// `Ok(None)` is a program that cannot be found.
pub trait ToolResolver {
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>>;
}

/// Workers write output blobs concurrently. What is held is bytes, by their digest
/// alone: whether a file may be run is said each time it is placed.
pub trait ContentStore: Send + Sync {
    fn put(&self, bytes: &[u8]) -> Result<Digest>;
    fn contains(&self, digest: &Digest) -> bool;
    /// The held content; `Ok(None)` when it is not held.
    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>>;
    /// Removes whatever is at `at`, a file or a directory with everything below it.
    /// Called before an output is placed, and again when it could not be placed whole.
    fn clear(&self, at: &OutputPath) -> Result<()>;
    /// Makes a stored blob the file at `at`, runnable or not as `mode` says, making
    /// every missing directory above it and replacing a file already there.
    fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()>;
}

/// The scheduler side is the single writer. A row is durable once `record` returns
/// `Ok`; after `Err` the cache holds what it held before the call.
pub trait ActionCache {
    fn row(&self, label: &Label) -> Option<&CacheRow>;
    fn labels(&self) -> Vec<Label>;
    fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
    fn forget(&mut self, label: &Label) -> Result<()>;
}

pub trait EventLog {
    fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
}

/// The reading side of the log: its lines, oldest first, each an event or a damaged line.
pub trait RunHistory {
    fn lines(&self) -> Result<Vec<LogLine>>;
}

/// Listing and removing what the cas holds, apart from `ContentStore` so a build cannot
/// remove what it stored; `Ports` does not carry it.
pub trait Sweep {
    fn held(&self) -> Result<BTreeSet<Digest>>;
    fn remove(&self, digest: &Digest) -> Result<u64>;
}

/// When the command starts, the directory that will contain each declared output
/// exists and nothing exists at an output path itself.
pub trait ExecStrategy: Send + Sync {
    /// Each isolation tier materialises a differently shaped exec dir.
    type Dir;
    fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
    /// With `live`, every run of bytes the command writes is also handed to it as it
    /// is read, with its channel. The outcome is the same with or without it.
    fn run(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome>;
    /// One entry per declared output, in order: what the action left there — nothing,
    /// a file, a directory with everything below it, or something that is neither.
    /// Stores the bytes of every regular file it reports, and follows no symbolic link.
    fn collect<C: ContentStore>(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>>;
}

/// `work` runs one task, on any thread, and may send messages through its second
/// argument while it runs. `step` runs on the calling thread: once with `Turn::Begin`,
/// then with each message (`Turn::Told`) and each result (`Turn::Done`), a task's
/// messages in the order sent and all before its result. It returns the tasks that
/// became ready. Every task `step` returns is run, and every message and every result
/// reaches `step`; a build over a dispatcher that loses a task or a result fails with
/// `Error::DispatcherFault` instead of reporting success.
pub trait Dispatcher {
    fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>;
}

pub trait Clock {
    fn now(&self) -> Timestamp;
}

/// Keeps the graph a build resolved. `record` replaces what is kept with `graph`, and
/// a reader finds the old record whole or the new one whole.
pub trait GraphRecord {
    fn record(&mut self, graph: &TargetGraph) -> Result<()>;
}

/// Shows a build as it runs. Every call is made on the thread that called `build`, and
/// none can fail: `begun` once, first; then `started`, `printed` and `finished`;
/// `summary` once, last, and never after an error.
pub trait Reporter {
    /// Every refusal check has passed, and `targets` targets are about to be built.
    fn begun(&mut self, targets: usize);
    /// A worker began `action`. Never called for a cached or a skipped target.
    fn started(&mut self, action: &ReadyAction);
    /// The action shown live wrote `bytes`.
    fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]);
    /// A target has its outcome. `printed` is what it wrote that was not shown live:
    /// for a cached or a fetched target, what it wrote when it last ran.
    fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>);
    /// A call to the remote cache failed and the build went on without it, or a target's
    /// result was not uploaded because an input changed meanwhile.
    fn remote(&mut self, notice: &RemoteNotice);
    /// Every target has its outcome.
    fn summary(&mut self, summary: Summary);
}

/// Settles whether a build may run once its wants are inside the ceiling.
pub trait Approver {
    /// `Approval::Granted`, or `Approval::Denied` with what was not approved and why.
    /// Never asked about a want outside the ceiling; an error is never read as a yes.
    /// `attribution` says which plugins' targets are behind the wants: it explains the set
    /// and never changes it, so what is remembered or refused is decided on `wanted`.
    fn approve(&mut self, wanted: &WantedSet, attribution: &Attribution) -> Result<Approval>;
}

/// A cache of results shared between machines, asked and filled by the action that made them.
/// Blocking, and called from several worker threads at once. A miss is `Ok(None)`; every other
/// failure is an `Error::Adapter` naming `PortName::RemoteCache`. What it answers is not yet
/// checked: the phase that executes checks every answer before it uses one.
pub trait RemoteCache: Send + Sync {
    /// The result stored for `action`, as stored; `None` when there is none.
    fn lookup(&self, action: &RemoteAction) -> Result<Option<RemoteHit>>;
    /// The bytes of each of `blobs`, in the same order.
    fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>>;
    /// The files of a directory tree the cache stores, in path order; `None` when it holds a
    /// symlink or an empty directory, which a build cannot place.
    fn decode_tree(&self, bytes: &[u8]) -> Result<Option<Vec<RemoteTreeFile>>>;
    /// Stores what a run produced as the result for `action`, with the input files and the
    /// blobs it names; the output and log blobs are read from `blobs`.
    fn upload(
        &self,
        action: &RemoteAction,
        produced: &RemoteUpload,
        blobs: &dyn BlobSource,
    ) -> Result<Uploaded>;
}

/// Where the bytes of the blobs an upload names are read from: the local content store, which
/// the remote adapter may not import. One of the two `dyn` in the ports, the other being the closure parameter of `Dispatcher::dispatch`.
pub trait BlobSource: Sync {
    fn read(&self, digest: &Digest) -> Result<Option<Vec<u8>>>;
}
```

Every fallible port method returns the crate's one `Result`, not an error type of its own:
`architecture.md` §4 settles the error model as a single structured enum, so `Result<Evaluated>`
above is `core::result::Result<Evaluated, buildl_core::Error>`. The enum is `#[non_exhaustive]`,
and each phase adds the variants it earns.

### 5.3 `Pipeline` and the `Ports` bundle

Static dispatch throughout [7]. One trait of associated types carries every port, so `Pipeline` takes a single type parameter instead of one per port. The bundle is declared in `buildl-core`'s `ports` module, beside the traits it names, so every trait of the crate lives in one module; a composition root implements it.

```rust
pub trait Ports {
    type Source: DeclarationSource;
    type Manifest: Manifest;
    type Host: Host;
    type Digester: Digester;
    type StatCache: StatCache;
    type Tools: ToolResolver;
    type Cache: ActionCache;
    type Cas: ContentStore;
    type Log: EventLog + RunHistory;   // plan, verify and build read discovery results from it
    type Strategy: ExecStrategy;
    type Dispatcher: Dispatcher;
    type Clock: Clock + Sync;   // a worker reads the clock
    type Graph: GraphRecord;
    type Reporter: Reporter;
    type Approver: Approver;
    type Remote: RemoteCache;   // lookup before an action runs, upload after; NoRemote when there is none
}

/// One instance of each port the bundle names, with a public field per port: `source`,
/// `manifest`, `host`, `digester`, `stat_cache`, `tools`, `cache`, `cas`, `log`, `strategy`,
/// `dispatcher`, `clock`, `graph`, `reporter`, `approver` and `remote`.
pub struct PortSet<P: Ports> { pub source: P::Source, pub manifest: P::Manifest, /* ... */ }

pub struct Pipeline<P: Ports> { /* ports: PortSet<P> */ }

// in `buildl`: the bundle is generic over the reporter and the console a build is lent
pub struct LocalPorts<R, C>(PhantomData<fn() -> (R, C)>);
pub type LocalPipeline<R, C> = Pipeline<LocalPorts<R, C>>;
```

`Ports::Remote` is bound by `RemoteCache`, which only `build` and `test` ever call: every other command, a workspace with no `[remote]` table and a run under `RemotePolicy::OFF` hold a cache that is never asked, `NoRemote` in the core's flows and `Remote::Off` in `LocalPorts`. `Pipeline::set_remote` chooses the policy of the `build` and `test` calls that follow it. `Ports::Log` is bound by `EventLog + RunHistory`: `Pipeline::plan`, `Pipeline::verify`, `Pipeline::build` and `Pipeline::test` read the last result of each discovery target from the log they append to, and the log adapter implements both traits. The two stay separate traits, since a writer and a reader never share a method, and a caller that only reads names `RunHistory` alone.

```mermaid
%% C4 level 4 — the Ports bundle and its two implementations
classDiagram
    class Pipeline~P~ {
        check()
        graph()
        query()
        plan()
        build()
        verify()
    }
    class Ports {
        <<trait>>
        Source: DeclarationSource
        Manifest: Manifest
        Host: Host
        Digester: Digester
        StatCache: StatCache
        Tools: ToolResolver
        Cas: ContentStore
        Cache: ActionCache
        Log: EventLog + RunHistory
        Strategy: ExecStrategy
        Dispatcher: Dispatcher
        Clock: Clock + Sync
        Graph: GraphRecord
        Reporter: Reporter
        Approver: Approver
        Remote: RemoteCache
    }
    class LocalPorts {
        crate buildl
        real adapters
        generic over R and C, the reporter and console of one build
    }
    class FakePorts {
        crate buildl-core, tests only
        in-memory fakes
    }
    Pipeline --> Ports : generic over
    LocalPorts ..|> Ports
    FakePorts ..|> Ports
```

### 5.4 Pure logic

|Logic|Input → output|Ports used|
|---|---|---|
|load|workspace root and a `Discovered` → `Vec<Declaration>` (queue, `subdir`, serial replay, sorted merge; every file evaluated with the same results)|`DeclarationSource`, `Dispatcher`|
|discovery|the build files, the log's lines and the cas → declarations loaded twice, the second with the last result of each discovery target (`results`, `load_twice`, `second_pass`); `Error::DiscoveryChanged` when the second load declares other discovery targets than the first. Crate-private, and named by `pipeline` only|`DeclarationSource`, `Dispatcher`, `RunHistory`, `ContentStore`|
|resolve|`Vec<Declaration>` → `TargetGraph` (validations, cycle detection)|—|
|narrow|`TargetGraph`, a `Scope` and a `Purpose` → the graph of the targets a run was asked for and everything they depend on; for `Scope::Workspace` that is every target but tests for a build and every test for a test run; a label nothing declares is `Error::UnknownTarget`, and a test run naming a target that is not a test is `Error::NotATest`|—|
|reach|`TargetGraph` and a `Query` → the targets the query reaches from its target, following dependencies or the reverse edges any number of steps, without that target and without aliases, as a `BTreeSet<Label>`; a label nothing declares is `Error::UnknownTarget`|—|
|plan|`TargetGraph`, setting overrides and the run's `Rerun` → `Plan` (the settings in force, every output placed and every command expanded, the plugin check before the ceiling check, the ceiling check with programs a dependency built exempt, attribution, key assembly, a verdict per target; the plan carries the settings it was made with, as `Plan::settings()`)|`Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ActionCache`|
|grants|a plan's `WantedSet` and `Attribution` → go on, or `Error::NotApproved` (`negotiate`, run by `build`, `test` and `verify` after Plan and before anything runs); what of a graph's wants its plugins do not allow (`plugin_violations`), which Plan stops at the first of and `doctor` lists whole; which plugins' targets are behind each want (`attribution`); the "built" rule Plan and Execute share; a graph's wanted set (`wanted`), which Plan and `doctor` share, and what of it lies outside the ceiling (`outside`), which `doctor` reports. The ceiling check stays Plan's: no `Grants` value is handed on|`Approver`|
|execute|the whole `TargetGraph`, the narrowed one, its `Plan`, the failure policy, `LiveOutput` and the run's `Rerun` → `Vec<ActionOutcome>` (output layout, checked over the whole graph and used for the narrowed one; placeholder expansion of commands and descriptions under the plan's settings, the program check, the `Schedule` state machine, cached or run, what an action left judged into one manifest per output, then cas, row, log, with each step shown through the reporter; with a remote cache, a lookup before an action runs and an upload after, each answer checked here and never by the adapter (`architecture.md` §3.4, §3.8); an outcome per target of the narrowed graph, or `Error::DispatcherFault`)|`Manifest`, `Dispatcher`, `ExecStrategy`, `ContentStore`, `ActionCache`, `EventLog`, `Clock`, `Reporter`, `RemoteCache`|
|last_run|`[LogLine]` and a `Label` → the newest run of the label that stored both its output digests, the class of each newer line that stored none, and the damaged lines; or `Error::NoRun`, suggesting the nearest label that has a run|—|
|verify|the whole `TargetGraph`, the narrowed one, its `Plan` and a `Sample` → `Verification` (each clean target's outputs put back under `out/` in graph order; the `Sample` of candidates, lowest key digest first; each chosen action re-run through the same `perform` as `execute` and its output manifests compared with the cached ones, in declared order, to the first difference; a target whose dependency is not cached or not placed is skipped; no row, no event and no placed output)|`Manifest`, `ActionCache`, `ContentStore`, `ExecStrategy`, `Dispatcher`, `Clock`|
|canonical JSON|value → sorted-key bytes; those bytes, and no others, → the value|—|

`Pipeline` strings these together, one method per command. `plan` and `build` take a `Scope` and narrow between Resolve and Plan, with `Purpose::Build`; `test` does the same with `Purpose::Test` and takes a `Rerun` too. `build` and `test` also record the whole graph through `GraphRecord` before they narrow. `build` and `test` begin with the discovery run: when the first load declares a discovery target they plan, put to the approver and execute just those targets, load again with the results that leaves, and go on as above; the outcomes are the discovery run's followed by the main run's (`architecture.md` §3.6). `check`, `graph` and `query` read only build files and the results of discovery targets, so each also exists as a free function, `pipeline::check`, `pipeline::graph` and `pipeline::query`, for a caller that holds a `DeclarationSource`, a `Dispatcher`, a `RunHistory` and a `ContentStore` and nothing else: a command that reads only those then needs no state directory, takes no lock and is not stopped by a state file, and a log or a blob it cannot read is no result, not an error. `gc` is a free function too, `pipeline::gc`, over a `DeclarationSource`, a `Dispatcher`, an `ActionCache`, a `ContentStore`, a `Sweep` and a `RunHistory`; it loads the build files as `plan` does, twice when they declare a discovery target, and builds nothing. `doctor` is a free function too, `pipeline::doctor`, over a `DeclarationSource`, a `Dispatcher`, a `Manifest`, a `RunHistory` and a `ContentStore`: it loads (twice, with a discovery target) and resolves, places the outputs, and returns a `Checkup` of the whole graph at each setting's default, with the plugins behind each want and every want a plugin does not allow, refusing nothing, taking no lock and asking no approver. `log` is a free function over its own two ports, `pipeline::log` over `RunHistory` and `ContentStore`: it runs `last_run` over the log's lines, reads the run's two blobs, and takes no lock and no `DeclarationSource`, so it reads no build file. `verify` loads as `plan` does, with the results of the discovery targets and no discovery run, resolves, narrows with `Purpose::Build` and plans as `build` does, then runs the `verify` logic; it records no graph, since it changes nothing that `gc` or `log` reads.

## 6. Port catalog

"Since" is the item of design §13 an adapter arrives with; item 1 is built, and so is each item-2 row listed here (today `RunHistory` and `Sweep`), the item-3 row, `InputSandbox`, the item-4 row, `Approver`, and the item-6 row, `RemoteCache`; a row with a later item describes what is designed. Item 5, plugins and discovery targets, adds no port: it gives two ports an argument (`DeclarationSource::evaluate`, `Approver::approve`), one bundle a bound (`Ports::Log`), and `buildl` the `plugins` module, which serves none (§4). Item 6, the remote cache, adds one: `RemoteCache`, with the `Remote` type of the bundle and a `remote` call on `Reporter`.

|Port|`buildl-core` uses it for|v1 adapter|Crate|Since|
|---|---|---|---|---|
|`DeclarationSource`|Load — one build file per call, several calls at once, each with the discovery results|airsl engine per file, `buildl` module table; the plugins a file uses with `buildl.use` load into the same evaluation, one instance per chain|`buildl-lua`|1|
|`Manifest`|the ceiling and the workspace's output and state directories; the `[execution]` settings, which no phase reads and the composition root hands to the dispatcher and the strategy|`buildl.toml` reader. The entry file's name, the declaration limits and where each plugin comes from do not go through the port: the adapter hands them to the composition root, which makes the declaration source from them. A plugin's cap does: it is in `Workspace::plugins`|`buildl`|1|
|`Host`|the platform in every key, and the values of the env names targets read|the running machine's OS, architecture and environment|`buildl`|1|
|`Digester`|what stands at each input's path, and input content hashes|filesystem + rayon, each file streamed in pieces|`buildl`|1|
|`StatCache`|reusing digests on unchanged mtime + size|`.buildl/statcache.json`, keyed by path; keeps an entry only for a file modified more than 2 s before the run began, and discards a file it cannot read back|`buildl`|1|
|`ToolResolver`|tool fingerprints in the key|`PATH` lookup + content hash|`buildl`|1|
|`ContentStore`|the cas: storing a blob, asking whether one is held, reading a held one back (an output's manifest), clearing an output's path under `out/` — before the output is placed, and again when it cannot be placed whole — and making a stored blob a file there, runnable or not|`.buildl/cas/ab/…`, temp file then rename, nothing synced; read-only blobs, with an executable twin `….x` beside a blob that was placed executable; hard links into `out/` where a write through a read-only file is refused, copies elsewhere|`buildl`|1|
|`ActionCache`|cache rows: reading the last build's, listing the labels that have one, recording one durably for each target that was built, and forgetting one durably when a failure under its key contradicts it|`.buildl/cache.json`, written by the canonical serializer and read back by the canonical reader, with `.buildl/cache.journal`: one appended line per recorded row, or per forgotten one, folded into the snapshot once per build|`buildl`|1|
|`EventLog`|one event per outcome|`.buildl/log.jsonl`, appended to across builds|`buildl`|1|
|`RunHistory`|the log's lines, oldest first, each an event or a damaged line; Load reads from them the last result of each discovery target|`.buildl/log.jsonl`, read only: never created, cut or written; a last line without a newline is left out|`buildl`|2|
|`Sweep`|the cas, listed and pruned: every digest it holds a blob or a twin for, and removing the blob and twin of one digest, answering the bytes they took. Kept apart from `ContentStore` and out of `Ports`, so a build cannot remove what it stored|the same `.buildl/cas` that `LocalCas` stores into: `held` lists the digests of its blobs and twins, `remove` unlinks both|`buildl`|2|
|`ExecStrategy`|running one action, started with each output's containing directory in place and nothing at the output's own path, handing over what it prints as it prints when asked to, then reporting what the action left at each output path — a file, a directory with everything below it, or neither — and storing every regular file of it|`Bare` (tier 1)|`buildl`|1|
|`ExecStrategy`|the same, in a directory of its own outside the workspace that holds copies of the action's declared inputs and its dependencies' outputs and nothing else of the workspace, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned|`InputSandbox` (tier 2), the default. `Local` is the one strategy `LocalPorts` names: `Bare`, `InputSandbox`, or `Refused` for `os-sandbox` and `container`, which `build` and `test` refuse before anything runs|`buildl`|3|
|`Dispatcher`|evaluating build files and running ready actions in parallel, and carrying what a running action says to the scheduling thread|std scoped threads + `mpsc`, started as tasks need them|`buildl`|1|
|`Approver`|whether a build whose wants are inside the ceiling may run, asked by `build`, `test` and `verify` after Plan, with the attribution of the wants|`LocalApprover`, the one the manifest's `[approver]` table names: `ManifestApprover` (yes), `DenyApprover` (no), or `InteractiveApprover`, which asks once on a `Console`, naming the plugins behind each want it asks about, and remembers a yes in the user's data directory, outside the workspace|`buildl`|4|
|`RemoteCache`|results shared between machines: the result stored for an action, the blobs it names, a directory tree read into files, and a run's result stored with everything it names. Blocking, called from the worker threads, and checked by the core, never trusted|`remote`: a gRPC client over the Remote Execution API [9], with a tokio runtime owned by the adapter and reached through `block_on`; `grpcs` for TLS, a token in a header. `NoRemote` for a run without one|`buildl`|6|
|`Clock`|log timestamps and durations|system clock|`buildl`|1|
|`GraphRecord`|keeping the graph a build resolved|`.buildl/graph.json`: the canonical bytes, written whole by every build|`buildl`|1|
|`Reporter`|output blocks, the status line, and the warnings about the remote cache|one terminal reporter with a live and a plain mode|`buildl`|1|

The `Clock` port is `architecture.md` §5 rule 4's quarantine of the wall clock. The crate boundary does not enforce it — `std::time` is in scope for every crate — so `crates/buildl-core/clippy.toml` bans `Instant`, `SystemTime` and `SystemTimeError` by name, and `cargo make guard-core-purity` fails the gate when one is used, when a ban stops resolving, when a source file suppresses the lint, and on three further ways the bans can be switched off that the task itself enumerates.

## 7. Dependency rules

1. **`buildl-core` depends on no buildl crate, no airsl, and no I/O API** (§1.2).
2. **Adapters depend on `buildl-core`.** `buildl-lua` never depends on `buildl`; inside `buildl`, adapter modules never import each other. What several adapters need lives in two shared modules, `state` and `search_path`, which implement no port and name no module of the crate; adapters meet only in the two composition modules, `local_ports` and `workspace`. `plugins`, which implements no port, is held to the same rule: it names `state` and `search_path`, `local_ports` calls its `check` and `workspace` its `fetch`, and the `manifest` adapter, which reads which plugins a workspace names, shares with it values in `buildl-core` and no code.
3. **`buildl` is the single composition root** — the only adapter crate that depends on another adapter crate (`buildl-lua`), and the only code that names more than one concrete adapter, through `LocalPorts`. This is dependency inversion applied, not violated: the principle constrains the logic, and a composition root is where concrete choices are made [3].
4. **`buildl-cli` reaches core types through `buildl`'s re-exports**, so the binary has one library dependency.
5. **The remote cache's gRPC client, its protobuf messages and its async runtime are the `remote` module's, in `buildl`.** `tonic`, `prost` and `uuid` have `buildl` as the one crate that may depend on them, beside their own companion crates; `tokio` is not banned by name, and `guard-crate-edges` keeps every other member off it. Nothing outside the module names tokio, and the runtime is reached through `block_on`, so `buildl-core`, the `Dispatcher` and every other adapter stay synchronous (`architecture.md` §6). `deny.toml` holds the crate edge; that no other module of `buildl` names them is the module layout's doing, as it is for `rayon` in `digest`.

|Rule|Enforced by|
|---|---|
|the dependency half of 1, rules 4 and 5, and the cross-crate half of 2|`deny.toml`'s `[bans].deny` wrapper lists, which name the only direct parents a crate may have, `tonic`, `prost` and `uuid` among them, plus `cargo make guard-crate-edges`, which diffs every member's direct dependencies against `crates/expected-edges.txt`. The compiler covers only imports: it stops a crate using what its `[dependencies]` omits, but not a dependency being added, and not one being removed.|
|the "no I/O API" half of 1|`crates/buildl-core/clippy.toml`, an enumerated ban on the filesystem, process, thread, network, environment, standard-stream and clock APIs, asserted by `cargo make guard-core-purity`|
|"no phase module names another" (`architecture.md` §1.1)|`crates/expected-module-edges.txt`, a committed list of `buildl-core`'s module-to-module imports, which also holds the edges of `history`, `suggest`, `verify`, `placement`, `grants` and `discovery`, six modules that are not phases, diffed by `cargo make guard-module-edges`. A grouped `crate::{…}` import, a `super::` that climbs to the crate root and an item named through the crate root's re-export (`crate::<Item>`) are refused outright, because the listing cannot read them|
|adapter isolation inside `buildl`|`cargo make guard-adapter-isolation`, which reads every file under `crates/buildl/src` but `lib.rs`, test code included: an adapter module may name `crate::state` and `crate::search_path` and no other module of the crate, the two shared modules name none, and only `local_ports` and `workspace` name any module they like. A grouped `crate::{…}` import, `super::super`, a `super::` in a `mod.rs` or in a module that is one file, and an item named through the crate root (`crate::<Item>`) are refused outright, because each would hide an edge|

## 8. Testing layers

```mermaid
%% Testing layers — the widest coverage at the bottom
graph BT
    CORE["buildl-core<br/>every flow against fake ports<br/>no Lua, no filesystem, no processes, no threads"]
    ADP["buildl adapters<br/>each module alone: a temp directory, system tools run by argv"]
    LUA["buildl-lua<br/>real airsl: build.lua fixtures to exact declarations"]
    E2E["buildl/tests/e2e<br/>real adapters through the Workspace facade:<br/>a fixture workspace built twice, byte-identical"]
    CLIT["buildl-cli<br/>argument parsing and exit mapping in process,<br/>then the built binary"]
    CORE --> ADP --> LUA --> E2E --> CLIT
```

The four layers that touch real I/O, in `buildl` and `buildl-cli`:

|Layer|Lives in|May touch|Must not|
|---|---|---|---|
|Adapter unit|`crates/buildl/src/<module>/`, beside the code|its own module, `state`, `search_path`, `buildl-core`, one temp directory; `exec` starts system tools|name another adapter module; read the process's current directory; set an environment variable|
|End to end|`crates/buildl/tests/e2e`, with fixtures in `crates/buildl/tests/fixtures`|`buildl`'s public API; a temp copy of a fixture|a private item; a path outside the package|
|CLI unit|`crates/buildl-cli/src`, beside the code|argument parsing, the exit mapping, terminal detection, a command's flow over byte buffers and a temp workspace|a process; a terminal|
|CLI binary|`crates/buildl-cli/tests/cli`|the built binary, with a chosen working directory and environment and its output piped|`buildl` internals; a terminal|

The tools these tests run are system tools — `sh`, `cp`, `sort`, `true`, `false`, `sleep` and their like — run by argv; `sh -c` appears in tests only, never in buildl. There is no compiled helper program and no shared kit of fakes (§10, §11). The one thing the layers share is a server: the remote cache's adapter tests, `buildl`'s end-to-end tests and `buildl-cli`'s binary tests all talk to the in-memory REAPI server that `buildl`'s `test-server` feature builds, on loopback, so no test reaches the internet and the shipped binary does not hold the server.

The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`. They implement the public traits through the public API alone, so whatever a fake does an adapter crate can do too, and the compiler checks that on every test build. A port no flow reaches yet keeps a unit-test fake beside its trait; `Clock` still has the one it had before a build read it:

|Port|Fake|What the test no longer needs|
|---|---|---|
|`DeclarationSource`|a fixed staged file, absence or evaluation failure per directory; other files once the root is evaluated again or once it is handed a discovery result; what the root build file was handed on each load|airsl, Lua|
|`Manifest`|one fixed workspace and ceiling|a `buildl.toml`|
|`Host`|a fixed platform and fixed variables, some of which it can be told to fail on|the real environment|
|`Digester`|files held as bytes with their metadata, counting every call|a filesystem|
|`StatCache`|in-memory, counting commits|a filesystem|
|`ToolResolver`|fixed fingerprints, recording every lookup|a `PATH`|
|`ContentStore`|blobs and placed paths held in memory, each placed path with its mode; digests it can be told to forget, paths and content it can be told to fail on, and reads it can be told to lose|a filesystem|
|`ActionCache`|rows held in memory, with labels it can be told to fail on; it also keeps the labels it forgot, and can be told to fail a forget|a filesystem|
|`EventLog`|the events appended, and whether to fail|a filesystem|
|`RunHistory`|the lines it was handed, and whether to fail|a filesystem|
|`Sweep`|the digests of the blobs an in-memory store holds (`FakeCas`, the same store that fakes `ContentStore`); `remove` drops one blob and answers its size, or 0 for a digest it does not hold. It records nothing and never fails|a filesystem|
|`ExecStrategy`|scripted per label: how the action exits, what it prints, and what is at each output — a file, one that may be run, a directory of files and of things that are not, a symbolic link, something special, or nothing|process spawning|
|`Dispatcher`|inline, on the test thread, taking the oldest or the newest waiting task first; tasks it can be told to lose, unrun or run with the result dropped|threads; results are deterministic|
|`Clock`|a counter, one tick per read|wall-clock time|
|`GraphRecord`|the graphs recorded, and whether to fail|a filesystem|
|`Reporter`|every call kept as an owned value, in order|a terminal|
|`Approver`|one answer, given every time; each set it was asked about, and the attribution given with it|a terminal, a data directory|
|`RemoteCache`|`FakeRemote`: results held in memory by the output path of the target, lookups, downloads and uploads kept in the order received, and it can be told to fail any call, serve tampered bytes or answer that an input changed|a network, a server|

Flows that run entirely in `buildl-core`'s tests:

|Scenario|Assertion|
|---|---|
|`//:a` fails under `--keep-going`|summary `1 failed, 2 skipped, 2 built`|
|a test run over a workspace of builds and tests|only the tests and what they depend on run; no other target does|
|a cached test run with `--rerun`|the test runs again and its dependency, whose key is unchanged, does not|
|a test fails with `--rerun`|its row is forgotten, so the next run runs it again and a failure records no pass|
|cas write succeeds, cache write fails|no row for the target — the crash-safety invariant (design §8.5)|
|rebuilt output byte-identical|downstream targets stay `Clean` — early cutoff|
|an action leaves a directory at an output|stored file by file under one manifest; the next build puts it back whole without running|
|a directory rebuilt to the same files, then with one file changed|its dependent is cached, then runs|
|one file of a recorded directory has left the cas|the target is built again, never half put back|
|one file of a directory cannot be placed, on a build or on a cache hit|the target fails and nothing of the directory is left at its path|
|wanted program outside the ceiling|an `Error::Refused` before any strategy runs — an error of the build, not an outcome class|
|one input changed|dirty reason is the `KeyComponents` diff|
|a tool named through a build setting is replaced|the target is dirty for the tool, and runs again|
|the `[execution]` settings all change after a build|every target stays `Clean`: none of them is in a key|
|only a rule's `desc` is edited|every target is cached, and no action runs|
|the dispatcher loses a task|`Error::DispatcherFault` naming every target left without an outcome; what ended before keeps its content, row and event|
|the dispatcher loses a build file|`Error::LostBuildFiles` naming every directory without a result; nothing is declared|
|a build file enqueues subdirectories out of order|merged declarations sorted by directory, then declared name|
|a plugin's target wants a program its manifest or its cap leaves out|`Error::PluginRefused` naming the plugin, before the ceiling is asked|
|the same declaration, from a build file and from a plugin|one key: the origin is no part of it|
|a build with an approver|it is handed the attribution of the plan beside the wanted set|
|a workspace that declares a discovery target, built|the discovery run first, then the targets declared from its result; a second build is all cached, discovery included|
|the second load declares another discovery target, or changes one|`Error::DiscoveryChanged`|
|build files evaluated twice|`build` loads each build file twice, never three times; without a discovery target, once|
|a discovery target fails|the build stops with its failure and loads nothing more|
|a discovery result the store cannot read after the target ran|an infrastructure error naming the log or the store, not a build with targets missing|
|`plan` over a workspace with a discovery target that is not clean|the plan lists that discovery target and not what it depends on|
|a target the fake remote holds|it ends `Fetched` and the strategy is never called; the row is written after the content; the target is not reported as started|
|a miss; the `Off` policy|a miss runs the action; under `Off` the port is never called|
|a dependency fetched, and a graph the remote holds entirely|its dependents get the keys a local build would give them; the graph finishes with no strategy call|
|the same plan with the remote off, and reading from an empty one|byte-identical keys and rows|
|a hit that is unusable, a blob that differs from its digest, a tree blob included|a remote failure: the result is not stored|
|a lookup, a download or an upload fails|lenient: one warning, the target runs or stays built, the remote is off for the rest of the command, also across the passes of a discovery build; strict: the build fails|
|a run with uploading on|the upload follows the run and precedes the row; a failed action, a restored target and a fetched target are not uploaded; `bare` is refused before the approver is asked|
|the fake answers that an input changed|a notice for that target and the remote stays on|


The same promises against the real adapters, through the `Workspace` facade (`crates/buildl/tests/e2e`):

|Scenario|Assertion|
|---|---|
|a fixture built in two directories, with four workers and with one|`out/`, `cache.json`, `graph.json` and the cas are byte-identical; no journal and no staging file is left|
|three targets produce the same bytes, one of them placed executable|one blob and its executable twin, in both copies|
|a second build|every target cached; `cache.json` and `graph.json` unchanged; the log grew|
|`out/` removed|every output put back without running, the executable bit kept|
|one input edited|its dependents rebuilt and nothing else|
|an edit that leaves an output's bytes unchanged|the rebuild stops there — early cutoff|
|a named target, or an alias|that target and what it needs, nothing else; `graph.json` is still the whole graph|
|a test that declares no outputs|it passes and is cached; the next `test` serves it from the cache|
|a build of the whole workspace|no test runs|
|a failure, without and with `--keep-going`|`1 failed, 4 skipped`, then `1 failed, 2 skipped, 2 built`; no row for the failed target, which runs again next time|
|the states an interrupted build leaves: blobs with no row, rows in the journal, staging files, a partial output|the next build finishes: it trusts no blob without a row, folds the journal, sweeps the staging and clears the partial output|
|a corrupt `cache.json`|`Error::CorruptState` naming the file, the file untouched, no action run; `check`, `graph` and `query` still work|
|a workspace another buildl holds|`plan` and `build` refused; `check`, `graph` and `query` succeed|
|a strategy other than `bare` or `sandbox`; a state directory outside `fs.write`|`build` refused and `plan` not; `plan` refused before `.buildl/` is made (the same refusal of `test` is a unit test in `src/workspace.rs`)|
|the sandbox fixtures: a manifest with no `strategy` line|the default manifest builds the whole chain|
|a target that reads a file it did not declare|it fails under `sandbox` and builds under `bare`|
|the same chain under `bare` and under `sandbox`|`out/` is byte-identical across the two|
|a target built under `bare`, then the `strategy` line removed|every target cached — `strategy` is no part of the key|
|an action that fails, and one that outlasts the timeout|nothing is left under `out/` for either|
|a leftover in the sandbox directory from an earlier build; a finished build|the leftover is removed, and nothing is left in the temporary directory after the build|
|test targets run under the default manifest|a test that reads an undeclared file fails under `sandbox` and passes under `bare`|
|a build the sandbox cannot prepare|the log line names the target and no sandbox path|
|`verify` after a build of the `verified` fixture, which has a deterministic chain, a tree output and a target that writes its process id|every target but the last verified; that one mismatched, naming its output and `File(Bytes)`|
|`verify`, then the cache file and the log|both byte-identical to before; no journal and no scratch directory left|
|`verify` with `out/` removed|the cached outputs put back, and no output of a re-run placed|
|`verify` with a sample of one, twice in a row|the same target checked both times|
|`verify` with a target never built; with the content of a dependency removed|the first counted not cached; the dependent of the second skipped, naming it|
|`verify` under `bare` and under `os-sandbox`|refused as `Error::Adapter` on the manifest before `.buildl/` or `out/` is made|
|`gc` after `verify`|the cas holds exactly the blobs it held before the `verify`|
|a fixture with a path plugin|it builds and caches; a plugin target outside the plugin's cap is `Error::PluginRefused`, with nothing in `out/`|
|a git plugin never fetched; a lock entry that is stale; a store with an edited file or an added link|`check` and `build` refuse before a build file opens, naming `buildl plugins fetch` for the first two and `PluginChanged` for the others; `log` and `clean` still run|
|`plugins fetch`, then again; a moved tag; a damaged store; a plugin the workspace stops naming|the first writes the lock and the store, the second changes nothing, a moved tag is refused as changed, a damaged store is restored without touching the lock, and what the workspace no longer names leaves the lock and the store|
|`plugins fetch` over a workspace with no git plugin|nothing is fetched and `git` is not looked for|
|a fixture with a discovery target: `graph`, `build`, `graph`|the first notes that the target has not run and makes no `.buildl/`; after the build the targets declared from its result are in the graph, `gc` keeps them, and the next build is all cached|
|a discovery target not up to date, then `plan`|the plan notes that the targets declared from it may change|
|`check` over a damaged log|it works: an unreadable result is no result|
|a second checkout of the same sources, against the in-test server a first build uploaded to|every target fetched, no action run, outputs byte-identical|
|a server that is down; a missing token; `verify`|one warning and every target built; the build refused before anything runs; the server sees no call|
|sixteen threads turning the remote off at once|one notice|

## 9. Publishing

`buildl-cli` is published to crates.io, and crates.io accepts a crate only when its dependencies are published too [6], so all four crates publish, dependency first:

```mermaid
%% crates.io publish order
graph LR
    P1["1. buildl-core"] --> P2["2. buildl-lua"] --> P3["3. buildl"] --> P4["4. buildl-cli"]
```

|Requirement|Form|
|---|---|
|internal dependencies|`buildl-core = { version = "…", path = "crates/buildl-core" }` in `[workspace.dependencies]`; packaging keeps the version and drops the path|
|version sync|each workspace dependency version matches the crate's own version|
|metadata|`description` and `readme` per crate; `license` and `repository` inherited from the workspace|
|crate `README.md`|written for crates.io readers of that crate|

Nothing is published yet. Four rules keep publishing possible, and every change to the workspace follows them:

1. Every dependency between members is `{ workspace = true }`, and no member depends, dev-dependencies included, on a member later in the publish order: `cargo package` fails on one, and a dev-dependency given by path alone is silently dropped from the packaged manifest.
2. The workspace has no fifth member (the REAPI code generator is a package outside the workspace, `architecture.md` §3.8), and `buildl` gains no `[[bin]]` or `[[example]]` for the sake of tests. Its one `[features]` entry, `test-server`, builds the in-memory server the remote cache's tests talk to: off by default, enabled by `buildl-cli` only as a dev-dependency, so the shipped binary does not hold the server.
3. Fixtures and golden files live inside the package whose tests read them, reached through `CARGO_MANIFEST_DIR` or `include_str!`; no test reads another package's directory.
4. No fixture path holds a component named `target` or `debug`.

All four names were unclaimed on crates.io on 2026-09-13; a name is claimed only by its first publish.

Once published, the ports are a semver contract:

|Concern|Rule|
|---|---|
|sealed or open|**open** — adapters live in other crates, and a sealed trait cannot be implemented outside its own crate [5]|
|growing a port|new methods get default bodies; a required method breaks every implementor|
|growing data|`#[non_exhaustive]` on enums and structs expected to gain variants or fields|
|version line|`0.x` until the port surface settles|

## 10. Decision records

|Decision|Choice|Because|
|---|---|---|
|Where the core lives|**A crate, `buildl-core`** — not a `core` module|A crate-root `mod core` shadows Rust's built-in `core` crate: `use core::fmt::Debug;` fails with `error[E0432]: unresolved import core::fmt` — "could not find `fmt` in `core`" (rustc 1.91.1). A crate boundary also turns the dependency rules into compiler errors.|
|Crate cut|**Four crates**|Only airsl brings a C build and a distinct upgrade risk, so only the Lua adapter earns its own crate; the other adapters share `buildl` until a concrete reason to split appears (§11). `cargo test -p buildl-core` never compiles Lua.|
|Composition root|**`buildl`, not `buildl-cli`**|Embedders — watch mode, editors, other tools — get a wired `Workspace` without copying the CLI's wiring; end-to-end tests live beside the real adapters; the scaffold's crate descriptions already call the CLI a thin shell over the library.|
|Port dispatch|**Generics with one `Ports` bundle**|Static dispatch [7] without a type parameter per port on `Pipeline`.|
|Port openness|**Open, unsealed traits**|Adapters are implemented in other crates [5]; ports grow through default methods.|
|Scheduler|**Pure `Schedule` + `Dispatcher` port**|Keep-going and skip semantics are testable without threads; the scheduler was already the single writer (`architecture.md` §3.4), so counters need no atomics.|
|Load granularity|**`DeclarationSource` evaluates one file**|The directory queue and the sorted merge are determinism-bearing logic, so they stay in `buildl-core` where fakes can test them.|
|Source globbing|**Host-side in `buildl-lua`**|`b.sources()` walks with `globset` and `walkdir`, so the declaration engine holds no grant and `airsstack.glob.walk` is refused; sorting, symlink policy and the containment check are buildl's, in one place. The directories the walk stays out of are a required argument of `LuaSource::new`, so a source cannot be built without them, and the composition root gives it the manifest's output and state directories: the skip list is the adapter's configuration, not part of the port, so `buildl-lua` reads no manifest.|
|Diagram notation|**Mermaid `graph` with C4 element labels**|Renders wherever the other documents' Mermaid renders; Mermaid's dedicated C4 syntax is marked experimental.|
|Re-exporting the core|**`pub use buildl_core;` — the crate under its own name, `buildl::buildl_core`**|`pub use buildl_core as core` is unusable: in a crate root that also names `core::fmt` it is `error[E0432]: unresolved import core::fmt`, and a downstream crate that glob-imports the re-export and names `core::fmt` gets `error[E0659]: core is ambiguous`. A third name would invent vocabulary. The facade `buildl::Workspace` and the manifest value `buildl::buildl_core::Workspace` stay apart.|
|When ports are wired|**Per `plan`, `build`, `test`, `verify` or `gc` call; a source of build files, the log and the cas for `check`, `graph` and `query`, and with the manifest for `doctor`; the log and the cas alone for `log`; none for `clean`**|`Pipeline` needs every port, and a command that reads only build files and the results of discovery targets must not need, lock or be stopped by a state file: a corrupt `cache.json` does not stop `check`, and a log that will not parse only means no result. The state lock is held for one call, so an embedder that keeps a `Workspace` open does not hold the directory while idle.|
|Who owns the reporter|**The caller: `Workspace::build` is lent `&mut R` for one build**|Only `build` reports, and an embedder supplies its own reporter. `LocalPorts<R, C>` is generic over it, and `&mut R` is a `Reporter` when `R` is, so the bundle needs no bound beyond that. The `Console` an interactive approver asks on is lent the same way, as `C`.|
|Tools in tests|**System tools by argv; no compiled helper, no published kit of fakes**|Cargo sets `CARGO_BIN_EXE_<name>` only for integration tests of the package that owns the binary, so a helper would cost the published `buildl` crate a `[[bin]]` or a feature, or the workspace a fifth member (§9).|
|Adapter isolation|**Checked by `cargo make guard-adapter-isolation`**|Sixteen modules — fifteen adapters, and `plugins`, which serves no port — in one crate is where a cross-import slips in. What they share lives in `state` and `search_path`, which implement no port (§7 rule 2).|
|Reaching discovery results|**A second argument of `DeclarationSource::evaluate`, a `Discovered` that Load passes unchanged to every evaluation**|An evaluation stays a function of its file and that one value, wherever and whenever it runs on the dispatcher's workers. Nothing is published (§9), so the port's signature changes in place, and the compiler names every implementor.|
|What an approver is told|**`Approver::approve(wanted, attribution)`; the attribution explains, the wanted set decides**|An approval is remembered on the wanted set alone, so an approver that ignores the attribution (`ManifestApprover` and `DenyApprover` do) is correct, and one that shows it (`InteractiveApprover`) cannot change what a yes covers.|
|The remote cache's port|**`RemoteCache`, blocking, with the checks in the core and `BlobSource` a `dyn` callback**|Workers on plain threads call it concurrently, and a blocking port keeps the scheduler and the core synchronous whatever the adapter needs underneath (`architecture.md` §6). The core judges every answer, so each check is tested against a fake port and the adapter only moves and encodes. The adapter may not import the content store (§7 rule 2), so an upload names the blobs it needs through a callback the core hands across the port, which with the `Dispatcher`'s progress closure is one of the two places a trait object is passed.|
|Where the test server lives|**A `test-server` feature of `buildl`, off by default**|The adapter's tests, the end-to-end tests and the binary's tests all need the same REAPI server, and an integration test or another crate can reach a library's code only through its public API. One feature builds it once for the three; the shipped binary does not enable it, and `buildl-cli` takes it as a dev-dependency only (§9 rule 2).|
|The plugin lock and store|**`plugins`, a module of `buildl` that serves no port**|Plan never reads the lock or the store, so no trait stands between the core and them: `local_ports` runs the check once and hands the directories to the declaration source, and `workspace` runs the fetch. The module is isolated like an adapter (§7 rule 2).|

## 11. Open questions

- Settled: no test kit of fake ports is published, and none is shared between crates. `buildl`'s adapter tests need another port in one place only — collecting an action's outputs into a content store — and write a small in-memory store there (§8).
- When an adapter inside `buildl` earns its own crate: an independent consumer, a heavy dependency, or a platform-specific build.

## References

1. airsstack. _airsl — Embeddable Lua runtime with a host standard library_ (source and docs). GitHub, 2026. URL: [https://github.com/airsstack/airsl](https://github.com/airsstack/airsl)
2. Simon Brown. _The C4 model for visualising software architecture_. c4model.com. URL: [https://c4model.com/](https://c4model.com/)
3. Robert C. Martin. _The Dependency Inversion Principle_. C++ Report, 1996.
4. Alistair Cockburn. _Hexagonal architecture (ports and adapters)_. URL: [https://alistair.cockburn.us/hexagonal-architecture/](https://alistair.cockburn.us/hexagonal-architecture/)
5. Rust library team. _Rust API Guidelines — Future proofing (C-SEALED)_. URL: [https://rust-lang.github.io/api-guidelines/future-proofing.html](https://rust-lang.github.io/api-guidelines/future-proofing.html)
6. The Cargo team. _The Cargo Book — Publishing on crates.io_. URL: [https://doc.rust-lang.org/cargo/reference/publishing.html](https://doc.rust-lang.org/cargo/reference/publishing.html)
7. Microsoft. _Pragmatic Rust Guidelines_. URL: [https://microsoft.github.io/rust-guidelines/](https://microsoft.github.io/rust-guidelines/)
8. arc42. _Building block view_. URL: [https://docs.arc42.org/section-5/](https://docs.arc42.org/section-5/)
9. Bazel authors. _remote-apis — `build/bazel/remote/execution/v2/remote_execution.proto`, at commit `c2c07e3d718d3d490a4958344d9c3e47de0bfaf3`_. GitHub. URL: [https://github.com/bazelbuild/remote-apis/tree/c2c07e3d718d3d490a4958344d9c3e47de0bfaf3](https://github.com/bazelbuild/remote-apis/tree/c2c07e3d718d3d490a4958344d9c3e47de0bfaf3)
