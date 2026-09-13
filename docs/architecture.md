# buildl — internal architecture

**Status: draft for review.** Companion to [`design.md`](./design.md): that document says _what buildl is and why_; this one says _how each part works in Rust_, and [`architecture-building-blocks.md`](./architecture-building-blocks.md) says _how the parts are put together_. Scope follows the §13 decision there — the build-system core first: Load → Resolve → Plan → Execute → Record, the action cache and cas, and the `check` / `graph` / `plan` / `build` commands. Plugins, remote, and watch are designed for but not implemented by anything here.

**Author:** rstlix0x0 · **Date:** 2026-08-23 · **Reviewers:** —

---

## 1. Workspace layout

Four crates, split so the logic of a build depends only on abstractions it owns. [`architecture-building-blocks.md`](./architecture-building-blocks.md) owns the layout — its C4 views, the port catalog (§6), and the dependency rules (§7) — and this document refers to crates and ports by those names.

```text
buildl/
  Cargo.toml        # workspace: resolver 3, shared lints (airsl's lint set)
  crates/
    buildl-core/    # domain data, ports (traits), pure logic of every phase: no I/O, no Lua
    buildl-lua/     # DeclarationSource over airsl: engine per file, the buildl HostModule
    buildl/         # every other adapter + LocalPorts: the composition root
    buildl-cli/     # the `buildl` binary: clap argument surface, exit codes
```

Inherited workspace policy, verbatim from airsl [1]: `unsafe_code = "forbid"`, `unwrap_used` and `panic` denied, pedantic + nursery clippy at warn, every dependency commented with its reason in the workspace `Cargo.toml`.

### 1.1 Crate dependency graph

Every dependency points at `buildl-core`; `buildl` is the one crate that names concrete adapters together (building blocks §7):

```mermaid
%% Crate dependency graph — arrows point at the dependency
graph TD
    CLI["buildl-cli<br/>argument surface"] --> LIB["buildl<br/>adapters + composition root"]
    LIB --> LUA["buildl-lua<br/>DeclarationSource via airsl"]
    LIB --> CORE["buildl-core<br/>domain data · ports · pure logic"]
    LUA --> CORE
    LUA --> AIRSL["airsl<br/>Engine, Policy, HostModule"]
```

Two structural rules the graph encodes:

- **airsl is a dependency of `buildl-lua` only.** No other crate knows Lua exists, and `buildl-core` cannot name airsl at all because airsl is not among its dependencies. The whole runtime boundary is one crate deep, which is what keeps the execution side testable without a Lua state and keeps an airsl upgrade's blast radius contained.
- **Phases communicate through values, not calls.** Load produces `Vec<Declaration>`, Resolve consumes it and produces `TargetGraph`, Plan produces `Plan`, Execute produces `Vec<ActionOutcome>`. The pipeline in `buildl-core` owns the sequence; no phase invokes the next. This is §8 of the design doc made literal: each handoff is a serializable value, so each phase is unit-testable alone and `buildl graph` / `buildl plan` are just serializations of a handoff.

## 2. Core types

All in `buildl-core`, all newtypes over primitives, following airsl's `types/` discipline (`ModuleName`, `RootTable`, `ChunkName` — validated at construction, impossible to construct invalid) [1].

```mermaid
%% Core type relationships
classDiagram
    class Label {
        directory: String
        name: String
        parse(s) Result~Label~
    }
    class Provenance {
        file: PathBuf
        directory: String
    }
    class Declaration {
        label: Label
        provenance: Provenance
        action: ActionTemplate
        inputs: Vec~InputSpec~
        deps: Vec~Label~
        flags: TargetFlags
    }
    class TargetGraph {
        nodes: Vec~Target~
        edges: Vec~Vec~NodeId~~
        reverse: Vec~Vec~NodeId~~
        by_label: BTreeMap~Label_NodeId~
    }
    class ActionKey {
        digest: Digest
        components: KeyComponents
    }
    class Digest {
        sha256: 32_bytes
    }
    class Plan {
        dirty: Vec~NodeId~
        reasons: BTreeMap~NodeId_DirtyReason~
        wanted: WantedSet
    }
    class ActionOutcome {
        node: NodeId
        class: OutcomeClass
        outputs: Vec~Digest~
        stdout: Digest
        stderr: Digest
    }
    Declaration --> Label
    Declaration --> Provenance
    TargetGraph --> Declaration : built from
    Plan --> ActionKey
    ActionOutcome --> Digest
```

The load-bearing decisions:

- **`Label` is the universal name** (`//dir:name`), parsed and validated once at Resolve; everywhere downstream a target is a `NodeId` (`u32` index into the graph's arenas). Strings appear only at the edges — parsing and reporting.
- **`ActionKey` keeps its components.** The key is a digest _of_ a `KeyComponents` struct (command, input digests, dep keys, env values, tool fingerprints, settings, os/arch — the §12.4 ledger as a Rust struct). Keeping the components alongside the digest is what makes `plan`'s "why dirty" reporting a struct diff instead of archaeology.
- **`Digest` is SHA-256 everywhere** — files, outputs, keys, log blobs — matching the Remote Execution API's digest model [4] so the remote-cache step later is a transport problem, not a re-keying.
- **`Provenance` is threaded, never reconstructed.** Attached in `declare`, carried through `Declaration` into `Target`, surfaced in every error. No phase ever asks "which file declared this?" — it always already knows.

### 2.1 The target state machine

Each node in an executing build moves through exactly these states — this enum _is_ the scheduler's bookkeeping:

```mermaid
%% Target states during execution
stateDiagram-v2
    [*] --> Clean : plan says key matches
    [*] --> Pending : dirty, deps not ready
    Pending --> Ready : dep counter hits zero
    Ready --> Running : worker picks up
    Running --> Built : exit 0, outputs landed
    Running --> Failed : action / infra failure
    Pending --> Skipped : upstream failed
    Clean --> [*]
    Built --> [*]
    Failed --> [*]
    Skipped --> [*]
```

`Clean` and `Skipped` are terminal without ever running — the two states a task runner cannot represent, and the reason the summary line can say `1 failed, 2 skipped, 2 built, 9 cached`.

## 3. Phase implementations

Each phase's logic lives in `buildl-core` and reaches I/O only through ports; the adapters live in `buildl-lua` and `buildl` (building blocks §6). The subsections keep the phase vocabulary:

|Section|Logic in `buildl-core`|Ports it uses|Adapter crate|
|---|---|---|---|
|§3.1 `declare`|directory queue, `subdir`, sorted merge|`DeclarationSource`|`buildl-lua`|
|§3.2 `graph`|resolve, validations, cycle detection|—|—|
|§3.3 `plan`|key assembly, dirty set, ceiling check|`Digester`, `StatCache`, `ToolResolver`, `ActionCache`, `Manifest`|`buildl`|
|§3.4 `exec`|`Schedule` state machine|`Dispatcher`, `ExecStrategy`, `ContentStore`, `Reporter`|`buildl`|
|§3.5 `store`|record ordering: cas, row, log|`ContentStore`, `ActionCache`, `EventLog`, `Clock`|`buildl`|

### 3.1 `declare` — the airsl boundary

The only crate that speaks Lua is `buildl-lua`, which implements the `DeclarationSource` port for one build file at a time; the directory queue and the merge described below are Load logic in `buildl-core`. Per build file: construct an `Engine` with the declaration policy (custom `LanguageSurface` without `os`, curated `ModuleSet` — design doc §12.1), evaluate, drop the engine (~136 µs; isolation by disposal [1]).

Installation: the engine keeps airsl's default `airsstack` root table, the `buildl` `HostModule` populates its own table, and inside `install` it binds that same table to the global `buildl` (design doc §5) — so every build file is evaluated with `buildl` loaded and `airsstack` beside it. `HostModule: Send + Sync` is part of airsl's contract, which is what rules the staging buffer's shape below.

The mechanics of collection: the `buildl` `HostModule`'s closures capture an `Arc<Mutex<Vec<Declaration>>>` staging buffer plus the current file's `Provenance`. `b.target(...)` validates its option table shape _immediately_ (unknown field → error naming file and field, airsl-refusal style) and pushes a `Declaration`. `b.subdir(dir)` pushes onto the directory queue owned by the Load driver — Lua never recurses.

Parallel load: the directory queue is processed by a small pool; each worker owns its engines, staging buffers merge at the end, and the merge sorts by (directory, declaration order) so parallel load yields the identical staging list as serial load. Determinism rule: parallelism must never be observable in any output (§6).

### 3.2 `graph` — arenas, not pointers

`TargetGraph` is struct-of-arrays with `u32` node ids: `nodes: Vec<Target>`, `edges: Vec<Vec<NodeId>>` (deps), `reverse: Vec<Vec<NodeId>>`, plus `by_label: BTreeMap<Label, NodeId>`. Petgraph [2] was considered and declined for the core structure — the graph is built once, never mutated, needs exactly toposort/cycle-detection/reachability, and owning the representation keeps it `serde`-serializable as the stable `graph.json` without an adapter layer. (Nothing prevents using petgraph algorithms _over_ this representation later if `query` grows complex.)

Cycle detection is iterative DFS (explicit stack — a deep graph must not overflow the call stack; airsl's require-loader made the same choice for the same reason [1]). The error carries the full cycle path as labels with provenance.

### 3.3 `plan` — keys and the stat cache

Bottom-up over a toposort. Per node: assemble `KeyComponents`, hash inputs through the stat cache, fold dep keys (already computed — toposort guarantees it), digest the canonical serialization of the components.

The stat cache is a `BTreeMap<PathBuf, StatEntry { mtime, size, digest }>` loaded from `.buildl/statcache.json`; a hit on (mtime, size) reuses the digest, a miss re-hashes and updates — ninja's mtime discipline with content-hash truth underneath [3]. File hashing fans out with rayon's parallel iterators [5] — files are independent, the work is chunkable, and rayon's pool is confined to the `Digester` adapter in `buildl` (decision record §6).

Output: `Plan { dirty, reasons, wanted }` — and the ceiling check runs here, so a `Plan` that would exceed the ceiling is an `Err`, not a plan.

### 3.4 `exec` — the scheduler

Two halves. The scheduling logic is `Schedule`, a pure state machine in `buildl-core`: it owns the state array (§2.1) and one dependency counter per node, hands out ready `NodeId`s, and on `complete(node, class)` marks dependents ready or skipped. Its caller is the only writer, so the counters are plain integers — single-writer bookkeeping, so no lock ordering to reason about. Parallelism is the `Dispatcher` port. Its v1 adapter in `buildl` uses plain threads, no async runtime (decision record §6): a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a `NodeId`, run the action through the strategy (below), and send an `ActionOutcome` back on a results channel. Tests drive `Schedule` through an inline dispatcher on one thread.

Execution strategies are one trait, the design doc's isolation ladder (§12.2) as code:

```rust
pub trait ExecStrategy: Send + Sync {
    /// The materialised exec dir; each isolation tier has its own shape.
    type Dir;
    /// Materialise the action's exec dir (inputs visible per this strategy's tier).
    fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
    /// Spawn and wait, capturing output. No shell, ever.
    fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
}
```

The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace cwd, granted env only) and `InputSandbox` (tier 2: temp dir, symlinked declared inputs). `OsSandbox` and `Container` are later impls of the same trait — the trait is in v1 so the ladder is a seam, not a refactor.

Output capture: piped stdout/stderr read by two threads per action into buffers, blobs written to the cas, digests recorded on the outcome. Printing is owned by a single reporter on the scheduler side (block per completed action; TTY status line) — workers never touch the terminal.

### 3.5 `store` — cas, action cache, log

The cas is `.buildl/cas/ab/cdef...` (two-char shard dirs). Every write is temp-file-then-rename within the same filesystem — atomic, the `fs.atomic_write` discipline [1]. `out/` entries are hard links into the cas (copy fallback for filesystems that refuse). The action cache is a `BTreeMap<Label, CacheRow { key: Digest, outputs: Vec<Digest>, stdout, stderr }>` serialized as sorted-key JSON; rewritten atomically once per build, rows updated in memory as outcomes land — write ordering enforces the §8.5 invariant: **cas first, row after, log always**. `log.jsonl` is append-only, one sorted-key JSON object per outcome, output blobs referenced by digest rather than inlined.

`gc` walks roots (current `graph.json` outputs + cache rows) and unlinks unreferenced cas entries — refcounting deferred; mark-and-sweep is enough at v1 scale.

## 4. Error architecture

One `thiserror` enum in `buildl-core`, airsl-style: structured fields, no message parsing anywhere [1]. The design doc's three failure classes (§8.7) are a `#[non_exhaustive]` enum carried _on_ errors and outcomes, not inferred from them:

```rust
pub enum OutcomeClass {
    Built,
    Cached,
    ActionFailure { status: i32 },          // the build did its job
    InfraFailure { kind: InfraKind },       // missing tool, sandbox, timeout
    Refusal { wanted: Wanted, ceiling: CeilingRef },  // plan-time only
    Skipped { because: NodeId },
}
```

Classification is structural at the site that knows (spawn error vs non-zero exit vs timeout flag), never downstream. The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim. Every error that can name a target carries `Label + Provenance`; the display impl renders airsl-refusal-shaped messages — what was wanted, what was granted or found, where it was declared.

## 5. Determinism engineering rules

House rules enforcing design-doc §12 at the code level — checkable in review, some in CI:

1. **No `HashMap` iteration reaches any output.** Anything serialized, displayed, or hashed iterates a `BTreeMap`/sorted `Vec`. (`HashMap` is fine as a pure lookup table.)
2. **All JSON leaves through one canonical serializer** — sorted keys, fixed float handling. `graph.json`, `cache.json`, `log.jsonl`, and the bytes hashed into an `ActionKey` all use it, so "the key of X" and "the file of X" can never disagree.
3. **Parallelism is never observable.** Parallel load merges sorted (§3.1); parallel hashing writes into pre-indexed slots; the executor's completion _order_ appears only in the log's timestamps, never in any artifact.
4. **Wall clock is quarantined.** `now()` is read in exactly two places — log event timestamps and durations — via the `Clock` port, whose only real adapter lives in `buildl`; `buildl-core` cannot name the system clock at all, so the crate boundary enforces the rule rather than grep.
5. **Double-run checks are CI, not doctrine.** `buildl check` runs declaration twice and diffs staging hashes; the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.json`, and cas contents.

## 6. Decision records

|Decision|Choice|Because|
|---|---|---|
|Async runtime|**None — threads**|The workload is process-spawning and file-hashing: worker count ≈ core count, no fan-out beyond it, no IO multiplexing need. An async runtime buys nothing and costs a dependency tree and an ecosystem split. Revisit only if remote execution's network fan-out demands it.|
|Graph library|**Own arenas; petgraph declined** [2]|Built once, immutable, three algorithms needed, must serialize stably (§3.2).|
|Parallel hashing|**rayon, confined to the `Digester` adapter** [5]|Chunkable CPU-bound work is its exact use case; confining it keeps the executor's threading model singular.|
|Hash|**SHA-256**|REAPI compatibility [4] outweighs blake3's speed; the stat cache makes hashing rare on warm builds anyway.|
|Cache files|**JSON via the canonical serializer**|Diff-stable and debuggable beats compact; airsl's sorted-key JSON is the precedent [1]. Binary formats only if profiling demands.|
|Worker/scheduler channel|**std mpsc**|One producer set, one consumer, no select needed; crossbeam only if the reporter grows a second consumer.|
|Lua exposure|**`buildl-lua` crate only**|An airsl (or mlua [6]) upgrade touches one crate; `buildl-core` compiles Lua-free by construction, since airsl is not among its dependencies.|
|Structure|**Four dependency-inverted crates**|Every flow testable against fake ports; recorded with its alternatives in building blocks §10.|

## 7. Testing strategy

The narrow handoffs are the test surface. Each flow runs first in `buildl-core` against fake ports — no Lua state, filesystem, process, or thread — and again against real adapters in the crate that owns them; the layers are in building blocks §8.

- **`declare`** (`buildl-lua`): fixture build files → assert exact `Vec<Declaration>`; hostile fixtures (huge loops, `pairs` tricks, bad option tables) → assert refusal shape and instruction-ceiling stops.
- **`graph`**: staging lists (no Lua) → golden `graph.json`; duplicate/unknown/cycle fixtures → assert full provenance in errors.
- **`plan`**: synthetic graphs + a tempdir tree → dirty sets and reasons; stat-cache hit/miss/mtime-rollback cases.
- **`exec`**: scripted fake strategies in `buildl-core` → state-machine transitions, keep-going semantics, crash-safety (a failing cache commit leaves no row); tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → output block integrity, a worker killed mid-action leaves no row.
- **End-to-end**: a fixture workspace built twice — second run all-cached and byte-identical state (§5.5); then one file touched — assert exactly the expected dirty closure rebuilds.
- **The airsl repo as dogfood** (design doc §13): buildl's own gate eventually runs under buildl.

## 8. Open implementation questions

- Whether `ExecDir` materialisation for `InputSandbox` uses symlinks (fast, but visible to tools that `readlink`) or hard links (opaque, but same-filesystem only) — likely per-strategy config with hard links default.
- Whether the stat cache should be keyed by `(dev, inode)` in addition to path, to survive renames without re-hashing.
- Where settings (`--set`) validation lives: `manifest` (they're workspace surface) vs `settings` (they're invocation surface) — leaning `settings`, validated against declarations at Resolve.
- Whether `graph.json` is written on every build or only on `buildl graph` — writing always makes `gc` roots simpler; measuring the cost first.

## References

1. airsstack. _airsl — source and docs (architecture.md, sandbox.md; workspace Cargo.toml lint and dependency policy)_. GitHub, 2026. URL: [https://github.com/airsstack/airsl](https://github.com/airsstack/airsl)
2. petgraph developers. _petgraph — graph data structures and algorithms (toposort)_. docs.rs. URL: [https://docs.rs/petgraph/](https://docs.rs/petgraph/)
3. Evan Martin et al. _The Ninja build system — manual_. ninja-build.org. URL: [https://ninja-build.org/manual.html](https://ninja-build.org/manual.html)
4. Bazel authors. _remote-apis — Digest and ActionCache model_. GitHub. URL: [https://github.com/bazelbuild/remote-apis](https://github.com/bazelbuild/remote-apis)
5. rayon developers. _rayon — data parallelism with work stealing_. crates.io. URL: [https://crates.io/crates/rayon](https://crates.io/crates/rayon)
6. khvzak et al. _mlua — Lua bindings for Rust_. crates.io. URL: [https://crates.io/crates/mlua](https://crates.io/crates/mlua)
