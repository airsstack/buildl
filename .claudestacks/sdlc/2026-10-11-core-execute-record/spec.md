---
status: approved
created: 2026-10-11
---

# Spec: Execute and Record — from `Plan` to a finished build

`buildl-core` can plan a workspace and cannot build one. This spec adds the last phase module,
`execute`: it lays out every target's outputs, expands every command, holds the expanded program to
the ceiling, and then runs the plan through a dispatching port — deciding per target whether its
cached result still stands, running the rest, and recording each result in the order **cas, then
cache row, then log**. It adds `Pipeline::build`, four ports, the grown `ActionCache`, and the types
an action, its result and its log event are made of. The decisions of §2 (D1–D22) answer the thirteen
questions the intent left open, close roadmap follow-ups #14, #15 and #16, and settle the five
contradictions between documents the intent lists.

---

## 1. Design premises, and where each one comes from

Every claim here was checked in this task against the artifact named. This spec asserts nothing about
the behaviour of a system outside this repository. The rust-guidelines rows cite the plugin at
version 0.1.4, under `~/.claude/plugins/cache/claudestacks/claudestacks-guideline-rust/0.1.4/`.

|Premise|Evidence|
|---|---|
|Execute is a ready queue, workers and per-target dependency counters; on failure new work stops by default, or continues with everything not downstream under `--keep-going`|`docs/design.md:240-254` (§8.4)|
|One action: expand placeholders, spawn with the granted env and no shell, outputs to temp, hash into the cas then link into `out/`, record row and log event|`docs/design.md:256-272` (§8.5)|
|A cache row is written only after its outputs exist in the cas; an interrupted build remembers finished work|`docs/design.md:274` (§8.5)|
|Three failure classes, told apart structurally; a failure never cancels a running action; downstream targets are skipped, reported apart from failed; failures are never cached|`docs/design.md:296-308` (§8.7)|
|Captured output goes into the cas by digest and is referenced from the log event, so a cache hit can replay it|`docs/design.md:294` (§8.7)|
|The log records one sorted-key JSON object per action: target, command, hashes, duration, cache hit or miss|`docs/design.md:198` (§7 rule 3)|
|`$in`, `$out`, `$deps` are substituted by the executor at run time; lists expand in written order|`docs/design.md:148`, `:153` (§5)|
|`$out` appears in eight commands. In seven it is where the target's own output goes: a file in five, a directory tree in two. In one it reaches a dependency's output|`grep -n '\$out' docs/design.md`: file `:128`, `:138`, `:321`, `:362`, `:404`; tree `:338` (`--outdir $out`), `:398` (`tar … -C $out`); a dependency's output `:142` (`$out/app`)|
|A directory-shaped output is represented by a stamp file|`docs/design.md:333`, `:342` (§9.2)|
|`out/` holds named outputs, linked from the cas; outputs default to the target's name|`docs/design.md:76` (§3), `:150` (§5)|
|`[execution]` holds `workers`, `timeout` and `strategy`|`docs/design.md:106-109` (§4)|
|Each tier runs an action in its own exec dir: tier 1 the workspace, tier 2 a temp dir holding the declared inputs|`docs/design.md:597-602` (§12.2); `docs/architecture.md:239`|
|`$dep://` placeholders belong to toolchain targets|`docs/design.md:404` (§9.6)|
|`ActionOutcome` is drawn as `{ node, class, outputs, stdout, stderr }`|`docs/architecture.md:134-140` (§2)|
|The target state machine: `Clean` only from the plan, `Pending`, `Ready`, `Running`, `Built`, `Failed`, `Skipped`|`docs/architecture.md:166-180` (§2.1)|
|`Schedule` is a pure single-writer state machine; parallelism is the `Dispatcher` port, whose adapter uses threads and `mpsc`|`docs/architecture.md:224` (§3.4); `docs/architecture-building-blocks.md:415`|
|`ExecStrategy` is drawn with `type Dir`, `prepare` and `run`|`docs/architecture.md:228-237`; `docs/architecture-building-blocks.md:210-215`|
|The action cache is drawn as rewritten once per build|`docs/architecture.md:245` (§3.5)|
|`OutcomeClass` is drawn with `Built`, `Cached`, `ActionFailure`, `InfraFailure`, `Refusal` (plan-time only) and `Skipped { because: NodeId }`; classification is made at the site that knows|`docs/architecture.md:254-264` (§4)|
|Parallelism is never observable in an artifact; the clock is read only for event timestamps and durations|`docs/architecture.md:281-282` (§5 rules 3, 4)|
|`ContentStore` is drawn with `put` and `contains`, `ActionCache` with `row`, `upsert` and `commit`, `EventLog` with `append`|`docs/architecture-building-blocks.md:193-208` (§5.2)|
|The bundle is drawn with `Cas`, `Log`, `Strategy` and `Clock`, and names `Dispatcher` only in a comment|`docs/architecture-building-blocks.md:232-245` (§5.3)|
|Three flows are Execute's to pass in `buildl-core`: a failure under keep-going, a cache write failing after a cas write, a byte-identical rebuilt output|`docs/architecture-building-blocks.md:371-373` (§8)|
|Follow-ups #14, #15 and #16 are assigned to this slice|`docs/roadmap.md:105-107` (§5)|
|The crate's five module rules|`crates/buildl-core/src/lib.rs:36-50`|
|`Pipeline` holds one `PortSet`; `plan` loads, resolves and plans, taking `&mut self`|`crates/buildl-core/src/pipeline/driver.rs:23-25`, `:97-104`|
|`Ports` names seven port types and `PortSet` holds seven fields|`crates/buildl-core/src/ports/bundle.rs:114-129`; `ports/port_set.rs:20-35`|
|`ActionCache` has one method, `row`; `Clock` has one, `now`|`crates/buildl-core/src/ports/action_cache.rs:16-19`; `ports/clock.rs:19-22`|
|Every verdict holds the target's `OwnComponents`: inside its key, or inside its `Unfinished`|`crates/buildl-core/src/types/plan.rs:66-72`, `:84-101`; `types/action_key.rs:89-95`|
|`OwnComponents` holds the command unexpanded, each env name with its value, and each referenced setting with its effective value|`crates/buildl-core/src/types/action_key.rs:68-86`|
|`ActionKey::new` is a key's only constructor; `CacheRow::new` takes a key and one digest per declared output|`crates/buildl-core/src/types/action_key.rs:144-150`; `types/cache_row.rs:27-29`|
|A `Node` holds its command, inputs, outputs, env names, network access and freshness; ids are in label order; `deps` and `dependents` are the graph's edges|`crates/buildl-core/src/types/target_graph.rs:26-49`, `:53`, `:132`, `:138`|
|An `Argument` leaves placeholders as text; a `$opt:` reference is `$opt:` and the longest run of ASCII letters, digits, `-` and `_`; a `Command` holds at least one argument|`crates/buildl-core/src/types/argument.rs:9`, `:22`, `:58-59`, `:134`|
|A `Program` holds no `$`; `Program::of` yields none for an empty argument or one holding `$`|`crates/buildl-core/src/types/program.rs:27-28`, `:72`; probe PI|
|An `OutputName` is a path inside a target's output directory, and where that directory is was left to the executor|`crates/buildl-core/src/types/output_name.rs:1-8`, `:18-21`|
|A `$opt:` reference naming no declared setting fails Resolve|`crates/buildl-core/src/error.rs:178`|
|`Error::Refused` holds a `Want`, an optional boxed site and the ceiling; `Error::Adapter` names a port; `PortName` has five variants; a `DeclarationSite` is built from a `&Node`|`crates/buildl-core/src/error.rs:201`, `:242`, `:375`, `:390-401`|
|`buildl-core` may not spawn or scope a thread, spawn a process, or read `Instant`|`crates/buildl-core/clippy.toml:64`, `:67`, `:118`, `:133`|
|`Timestamp::duration_since` yields a `Duration`; a `Timestamp` serializes as a bare number|`crates/buildl-core/src/types/timestamp.rs:50`, `:89-91`|
|A `Provenance` displays as its build file's path; a `Label` serializes as its text; a `Command` as an array of text|`crates/buildl-core/src/types/provenance.rs:50-53`; `types/label.rs:192-193`; `types/argument.rs:257-258`|
|A `Diagnostic` derives no `Serialize`|`crates/buildl-core/src/types/diagnostic.rs:17-18`|
|An enum variant holding a value serializes as an object of one key|`crates/buildl-core/tests/flows/golden/plan.json`: `"reason":{"Changed":[{"Input":"main.c"}]}`|
|The fakes are `FakePorts` over seven fake ports, built by `ports()`; a build is simulated by hand in the Plan flows|`crates/buildl-core/tests/flows/common.rs:266-276`, `:297`; `tests/flows/plan.rs:116-175`|
|The graph flow's fixture runs `$out/app` as its test target's command|`crates/buildl-core/tests/flows/graph.rs:53`|
|`CacheRow::new` is called at nine sites; `ActionCache` and `Ports` are each implemented at three; a `PortSet` is built at two|`grep -rn --include='*.rs'` over `crates`: `tests/flows/plan.rs:144`, `src/types/cache_row.rs:95-99`, `:104`, `:112`, `src/plan/verdict.rs:184`; `tests/flows/common.rs:258`, `:268`, `:298`, `src/ports/port_set.rs:144`, `:154`, `:166`, `src/ports/bundle.rs:95`, `:104`|
|The module-edge guard diffs `crates/expected-module-edges.txt`, which holds 20 edges|`Makefile.toml:60`, `:146-162`; `wc -l` printed `20`|
|No `bool` parameter where it expresses a decision; a duration is a `Duration`; generics before trait objects, and callbacks as `Fn` generics; one concept, one type; a file holding only a trait or only type definitions carries an exemption note|rust-guidelines `strong-types.md:32`, `:61-67`; `static-dispatch.md:7-14`, `:78-86`; `modularity.md:32-51`; `unit-test-mandate.md:15-32`|

### 1.1 Facts established by running something

Every probe was a temporary file, run and deleted afterwards; `git status --short` then listed only
this chain's directory and `docs/roadmap.md`.

**PF — a dispatching port that takes the work and the bookkeeping as two closures runs inline on one
thread.** `crates/buildl-core/tests/flows/probe.rs` declared

```rust
pub trait Dispatcher {
    fn dispatch<T, R, W, S>(&mut self, work: W, step: S)
    where T: Send, R: Send, W: Fn(T) -> R + Sync, S: FnMut(Option<R>) -> Vec<T>;
}
```

with an inline implementation — a queue seeded by `step(None)`, each task run through `work` and its
result handed to `step(Some(result))` — and drove a diamond of four nodes through it. `work` borrowed
two fields of one struct shared; `step` borrowed two others mutably.
`cargo test -p buildl-core --test flows probe -- --nocapture --test-threads=1`:

```
PF cache order [0, 1, 2, 3]
PF log ["built 0", "built 1", "built 2", "built 3"]
test result: ok. 1 passed; 0 failed
```

**PG — the same trait is implementable over std threads and `mpsc`, borrowing its ports.**
`crates/buildl/tests/probe_dispatch.rs` implemented it with `std::thread::scope`, three workers
taking tasks from one `mpsc` channel and sending results on another, and ran the same diamond with
`work` borrowing a strategy and a store by reference. `cargo test -p buildl --test probe_dispatch -- --nocapture`:

```
PG cache first=Some(0) last=Some(3) len=4
PG ran on the calling thread: false
test result: ok. 1 passed; 0 failed
```

Consequence of PF and PG: the shape of D7 needs no thread in `buildl-core`, no `Arc` around a port
and no trait object.

**PH — the port shapes of §4 compile together and draw no lint of their own.** A probe file declared
`ContentStore`, `ExecStrategy` with its generic `collect`, `EventLog`, a sub-trait of `ActionCache`
adding `record`, `Dispatcher` returning `Result<()>`, a bundle with `type Clock: Clock + Sync`, a
port set, a worker function calling `prepare`, `run`, `put` and `collect`, and an `execute` that
destructures the set and passes two closures to `dispatch`; then ran two actions through in-memory
implementations. `cargo test -p buildl-core --test flows probe`: `1 passed; 0 failed`.
`cargo clippy -p buildl-core --all-targets -- -D warnings` reported three errors, all `dead-code` for
items the probe declared and did not use (`Exit::TimedOut`, `ContentStore::place`, `record`).

**PI — what expanded text parses as.**

```
PI argument "out/app" ok=true              PI program of "out/app" = Some("out/app")
PI argument "out/lib/text" ok=true         PI program of "cc" = Some("cc")
PI argument "lib/a.c lib/b.c" ok=true      PI program of "" = None
PI argument "" ok=true                     PI program of "$dep://toolchains:go/bin/go" = None
PI argument "--o=out/x" ok=true            PI program of "$unknown" = None
PI setting value 'a b' ok=true             PI setting value '$in' ok=true
PI setting value with NUL ok=false         PI empty command ok=false
PI Error is Send: compiled                 PI size_of Error = 112
```

Consequences: every expansion of §5.3 is a valid `Argument`; an expanded output path is a valid
`Program`, so it can be compared with `proc.run` (D5); a setting's value may itself hold `$in`, so
substituted text must not be scanned again (§5.3); a command may expand to nothing, which
`Command::new` refuses (§5.4); an `Error` may cross a thread, so a worker's result may hold one.

**PL — a key finished again from the cache rows equals the key the plan holds.** A temporary test in
`tests/flows/plan.rs` built the four-target chain with that file's `build` helper, planned it, and for
each target called `ActionKey::new` over the verdict's `OwnComponents` and its dependencies'
`CacheRow::produced()`:

```
PL //:app clean=true refinished==planned:true refinished==row:true
PL //:docs clean=true refinished==planned:true refinished==row:true
PL //:gen clean=true refinished==planned:true refinished==row:true
PL //:lib clean=true refinished==planned:true refinished==row:true
```

Consequence: one rule — finish the key from the rows, compare it with the target's row — reaches the
same answer Plan reached for a clean target, so §5.6 needs no case per verdict.

**N1 — Resolve compares no target's outputs with another's.**
`grep -rn -i output crates/buildl-core/src/resolve/*.rs`, comment lines dropped, printed eight lines:
two in `fixtures.rs`, and `assembly.rs:151` (`outputs: without_repeats(&target.outputs)`) with the
five lines of its unit test — repeats inside one target only. Control: `grep -rn -c DuplicateLabel`
over the same directory printed `4` for `assembly.rs` and `11` for `index.rs`.

**N2 — an atomic is not among `buildl-core`'s banned APIs.**
`grep -c -i atomic crates/buildl-core/clippy.toml` printed `0`. Control: `grep -c 'std::thread::spawn'`
over the same file printed `1`.

## 2. Decisions

|#|Question|Decision|Because|
|---|---|---|---|
|D1|Where placeholders become paths|In `buildl-core`, as workspace-relative text. A strategy makes those paths true in the directory it runs the action in|Chosen by the author. The command is the same in every tier, is known before anything runs, and is tested against fakes. `docs/design.md:597-602` lays each tier's directory out differently, so a path must not depend on the tier|
|D2|What `$in`, `$out` and `$deps` expand to|Each is a list of paths: the target's inputs, its own outputs, its dependencies' outputs. Alone as an argument a list splices one argument per path; inside other text it joins with one space (§5.3)|`$out` chosen by the author. Seven of the design's eight uses read it as where the target's own output goes (§1); `docs/design.md:142` alone reaches a dependency's output through it, and is amended (§10). The two uses whose output is a directory tree keep their meaning and wait on directory outputs (§11)|
|D3|How the modules reflect Execute and Record|One phase module, `execute`, with recording as a file inside it|Chosen by the author. Record is the last step of one action (`docs/design.md:256-272`), and a dependent's key needs its dependency's row first, so the two cannot be two passes. `Pipeline::build` stays a sequence|
|D4|Where an output lives|At `<out>/<directory of the label>/<output name>`. Two targets claiming one path, or one claiming a path another needs as a directory, is an error before anything runs|`docs/design.md:76` puts named outputs in `out/`; `output_name.rs:8` left the directory to the executor; nothing upstream compares outputs (N1)|
|D5|A program first named by expansion (follow-up #14), and `OutcomeClass::Refusal`|Every command is expanded before anything runs, and its first argument must be a program in `proc.run`; otherwise the build is `Error::Refused`. No outcome class for a refusal|D1 makes every expansion known up front, so the check keeps design §6's rule that a refusal precedes execution. It is the error Plan already raises, which closes the intent's `Refusal` contradiction|
|D6|What is cached at run time (follow-up #15, and a conditional verdict)|Once a target's dependencies have finished, its key is finished from their current rows. It is cached when it is not declared `always`, that key equals its row's key, and every digest the row holds is in the cas. Otherwise it runs|PL: the rule agrees with Plan for a clean target. It settles a conditional one, and it sends a target whose output left the cas back to run, with no case per verdict|
|D7|The dispatching port|`dispatch(work, step)`: `work` runs a task on any thread; `step` runs on the calling thread, first with `None`, then with each result, and returns the tasks that became ready|PF, PG, PH. Generic over the task and the result, so the port knows nothing of builds; rust-guidelines `static-dispatch.md:78-86` prefers callbacks as `Fn` generics|
|D8|What runs where|A worker prepares, runs, stores captured output and stores the outputs. The calling thread places outputs, writes the row, appends the event and releases dependents|`docs/architecture-building-blocks.md:193` has workers write blobs; the row and the log have a single writer (`:199`). The split is the invariant's order: cas, then row, then log|
|D9|How a row is written (follow-up #16)|`ActionCache::record(label, row) -> Result<()>`: one call, durable when it returns `Ok`, absent when it returns `Err`. `CacheRow` gains `stdout` and `stderr`|Assumed in the intent and approved: design §8.5 binds, so a row is durable before its dependents start. `upsert` then `commit` cannot say that one row failed: a later commit would persist it. `docs/architecture.md:245` is amended|
|D10|How a stored output reaches `out/`|`ContentStore::place(digest, path)`, called for a built and for a cached target|`docs/design.md:265`. Linking needs to know where a blob lives, which only the store's adapter does|
|D11|How a produced output reaches the cas, and what a run returns|`ExecStrategy::collect(dir, action, cas)` returns one optional digest per declared output. `run` returns `RawOutcome { exit, stdout, stderr }` with `Exit::Exited(status)` or `Exit::TimedOut`|The strategy alone knows its exec dir. An output never passes through `buildl-core` as bytes. A spawn that cannot happen is the strategy's `Err`|
|D12|The classes an outcome can carry|`Cached`, `Built`, `ActionFailure { status }`, `OutputMissing { name }`, `InfraFailure { kind }`, `Skipped { because }` (§3.4)|`docs/architecture.md:254-264`, less `Refusal` (D5), plus an action that exits zero without an output it declared: the build revealing a fault, not a sick builder|
|D13|What a failure does to the rest|`OnFailure::Stop`: a target is skipped when it is still waiting on a dependency, and when it was handed to the dispatcher but no worker had begun it. An action a worker has begun finishes and is recorded. `OnFailure::KeepGoing`: only the failure's dependents, direct and indirect, are skipped|`docs/design.md:254`, `:306`. The dispatcher runs every task it is given (D7), so the stop is a flag the worker's own code reads before it begins an action (§5.7); N2|
|D14|What a port's own failure does|A strategy or cas failure, or `ActionCache::record` failing, while handling one target makes that target `InfraFailure`. A manifest, log or dispatcher failure fails the build with that error, and so does a breach of a port's contract: a dependency that succeeded and has no row (§5.6)|The first kind is the builder failing one action (`docs/design.md:301`); after the second nothing more can be recorded or trusted|
|D15|What a build returns|`Vec<ActionOutcome>`, one per target in id order. `Summary::of` counts them. A failed action is an outcome, not an error|`docs/architecture.md:134-140`; the handoff the documents already name. Rendering is the CLI's|
|D16|Which invocation and manifest values reach a build|`OnFailure` only. The timeout and the worker count configure the strategy and dispatcher adapters|A timeout is detected where the process is waited on; `buildl-core` only classifies `Exit::TimedOut`|
|D17|`always` targets, and test targets|An `always` target runs on every build and records a row like any other. A test target is built like a build target|Dependents key on what it produced. Choosing targets by role is target selection, a non-goal|
|D18|What a log event holds|§3.5. `because` and every other target reference is a `Label`|`docs/design.md:198`, `:306`. No `NodeId` reaches a file, and one enum serves the outcome and the event (rust-guidelines `modularity.md:32-51`)|
|D19|Where the clock is read|On the worker, before `prepare` and after the last store, for `started` and `finished`; on the calling thread once per event, for `at`. The bundle asks `Clock + Sync`|`docs/architecture.md:282`. A duration measured on the calling thread would include time spent queued|
|D20|How many failures are reported before anything runs|The first, in the fixed order of §5.1|Same as Load, Resolve and Plan|
|D21|How Execute learns the output directory and the ceiling|It asks `Manifest::workspace` again|`Plan` does not carry the workspace, and reshaping it for one field is not earned|
|D22|A plan that was not made from the graph given|`Error::ForeignPlan`, checked first|`execute` is public; without the check a missing verdict has no error to be|

## 3. Data model

Six new files under `types/`, and one edit. Every type derives `Debug, Clone, PartialEq, Eq` unless
stated.

### 3.1 Output paths

`types/output_path.rs`.

```rust
pub struct OutputPath(/* String */);
```

The workspace-relative path one output of one target lives at. Built only by
`OutputPath::of(out: &Directory, label: &Label, name: &OutputName) -> Self`: the three texts joined
by `/`, a root directory contributing nothing. `as_str`, `Display`, `Ord`, `Hash`; serializes as its
text. It has no `parse` and no `Deserialize`: nothing reads one back.

|`out`|label|output|path|
|---|---|---|---|
|`out`|`//:app`|`app`|`out/app`|
|`out`|`//lib:text`|`text`|`out/lib/text`|
|`out`|`//:z`|`bin/z`|`out/bin/z`|
|root|`//lib:text`|`text`|`lib/text`|

### 3.2 The action handed to a strategy

`types/ready_action.rs`. Fields are public.

```rust
pub struct ReadyAction {
    pub label: Label,
    pub command: Command,
    pub env: BTreeMap<EnvName, EnvValue>,
    pub inputs: Vec<SourcePath>,
    pub deps: Vec<OutputPath>,
    pub outputs: Vec<OutputPath>,
    pub network: NetworkAccess,
}
```

|Field|Holds|
|---|---|
|`label`|the target, for the adapter's own diagnostics|
|`command`|the node's command, expanded (§5.3)|
|`env`|each name of the key's `env` that has a value, with it|
|`inputs`|the node's inputs, in written order|
|`deps`|every output of every dependency, in edge order then the dependency's written order|
|`outputs`|the node's own outputs, in written order|
|`network`|the node's `network`|

### 3.3 What a strategy returns

`types/raw_outcome.rs`. Fields are public.

```rust
#[non_exhaustive]
pub enum Exit { Exited(i32), TimedOut }

pub struct RawOutcome { pub exit: Exit, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
```

`Exit` is `Copy`.

### 3.4 Outcomes

`types/outcome.rs`.

```rust
#[non_exhaustive]
pub enum InfraKind { Prepare, Run, TimedOut, Store }

#[non_exhaustive]
pub enum OutcomeClass {
    Cached,
    Built,
    ActionFailure { status: i32 },
    OutputMissing { name: OutputName },
    InfraFailure { kind: InfraKind },
    Skipped { because: Label },
}

pub struct ActionOutcome {
    pub label: Label,
    pub class: OutcomeClass,
    pub diagnostic: Option<Diagnostic>,
    pub outputs: Vec<Produced>,
    pub stdout: Option<Digest>,
    pub stderr: Option<Digest>,
}

pub struct Summary { pub failed: usize, pub skipped: usize, pub built: usize, pub cached: usize }
```

|Class|Meaning|Counted as|
|---|---|---|
|`Cached`|not run: its recorded result still stands (D6)|cached|
|`Built`|ran, exited zero, every output stored, row written|built|
|`ActionFailure`|ran and exited with a non-zero `status`|failed|
|`OutputMissing`|ran and exited zero, and left nothing at the output `name`: the first such, in written order|failed|
|`InfraFailure`|`Prepare`: the exec dir could not be made. `Run`: the action could not be run. `TimedOut`. `Store`: an output, a blob or the row could not be stored or placed|failed|
|`Skipped`|never begun, because the target `because` failed|skipped|

`InfraKind` is `Copy`. `OutcomeClass` and `InfraKind` implement `Serialize`: a variant without fields
as its name, the others as an object of one key (§3.5). `OutcomeClass::succeeded(&self) -> bool` is
true for `Cached` and `Built`.

`ActionOutcome::diagnostic` is a message for display, set for an `InfraFailure` other than `TimedOut`
and `None` otherwise: the `Display` text of the error a port returned, or one of the two texts §5.7
and §5.8 fix. `outputs` is non-empty only for `Cached` and `Built`. `stdout` and `stderr` are set for
`Cached` and `Built`; for a failure on a worker whose captured output was stored; and, from the row,
for a cached target whose output could not be placed.

`Summary` is `Copy`. `Summary::of(outcomes: &[ActionOutcome]) -> Self` counts by the table's last
column.

### 3.5 The log event

`types/outcome_event.rs`. Fields are public.

```rust
pub struct OutcomeEvent {
    pub at: Timestamp,
    pub provenance: Provenance,
    pub command: Command,
    pub key: Option<Digest>,
    pub started: Option<Timestamp>,
    pub finished: Option<Timestamp>,
    pub outcome: ActionOutcome,
}
```

|Field|Holds|
|---|---|
|`at`|when the outcome was recorded|
|`provenance`|the build file that declared the target|
|`command`|the expanded command, for every class|
|`key`|the digest of the target's finished key; `None` for `Skipped`|
|`started`, `finished`|the worker's two clock reads, whose difference is how long the action took. Both are set for every target a worker began, whatever its class, and both are `None` for every other: `Cached`, `Skipped`, and a cached target whose output could not be placed|

It implements `Serialize` by hand, through a private view as `Plan` does, and not `Deserialize`. The
shape, with the keys in the order the canonical serializer writes them:

```
{
  "at":         <nanoseconds>,
  "class":      "Cached" | "Built"
              | { "ActionFailure": { "status": <n> } }
              | { "OutputMissing": { "name": "<name>" } }
              | { "InfraFailure":  { "kind": "Prepare" | "Run" | "TimedOut" | "Store" } }
              | { "Skipped":       { "because": "<label>" } },
  "command":    ["<argument>", ...],
  "diagnostic": "<text>" | null,
  "file":       "<the build file, as Provenance displays it>",
  "finished":   <nanoseconds> | null,
  "key":        "<hex>" | null,
  "label":      "<label>",
  "outputs":    [ { "digest": "<hex>", "name": "<name>" }, ... ],
  "started":    <nanoseconds> | null,
  "stderr":     "<hex>" | null,
  "stdout":     "<hex>" | null
}
```

A requirement, pinned by §9's golden file.

### 3.6 The failure policy

`types/on_failure.rs`: `pub enum OnFailure { Stop, KeepGoing }`, `Copy`. An enum and not a `bool`
(rust-guidelines `strong-types.md:61-67`).

### 3.7 The cache row

`types/cache_row.rs`, edited.

```rust
pub struct CacheRow { /* key: ActionKey, outputs: Vec<Digest>, stdout: Digest, stderr: Digest */ }
```

`CacheRow::new(key, outputs, stdout: Digest, stderr: Digest) -> Option<Self>`, `None` as today when
`outputs` does not hold one digest per declared output. New accessors: `stdout() -> Digest`,
`stderr() -> Digest`, and `digests() -> impl Iterator<Item = Digest>` over every digest the row
holds: the outputs in order, then `stdout`, then `stderr`. The nine call sites of `new` (§1) gain two
arguments.

## 4. Ports

Four new traits under `ports/`, one file each; `ActionCache` grows; the bundle and the set grow.
Every fallible method returns the crate's `Result`.

```rust
pub trait ContentStore: Send + Sync {
    fn put(&self, bytes: &[u8]) -> Result<Digest>;
    fn contains(&self, digest: &Digest) -> bool;
    fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()>;
}

pub trait ActionCache {
    fn row(&self, label: &Label) -> Option<&CacheRow>;
    fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
}

pub trait EventLog {
    fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
}

pub trait ExecStrategy: Send + Sync {
    type Dir;
    fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
    fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
    fn collect<C: ContentStore>(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Option<Digest>>>;
}

pub trait Dispatcher {
    fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
    where
        T: Send,
        R: Send,
        W: Fn(T) -> R + Sync,
        S: FnMut(Option<R>) -> Vec<T>;
}
```

|Method|Contract|
|---|---|
|`ContentStore::put`|stores the bytes and returns their digest; storing bytes already held is not an error|
|`ContentStore::contains`|whether content with that digest is held|
|`ContentStore::place`|makes the content with that digest the file at `at`, replacing what is there|
|`ActionCache::row`|the row for the label: the previous build's, or the one `record` last stored|
|`ActionCache::record`|stores the row durably. After `Ok`, `row` returns it. After `Err`, `row` returns what it returned before the call|
|`EventLog::append`|appends one event|
|`ExecStrategy::prepare`|makes a directory to run the action in, where every path of `inputs` and `deps` is readable at that relative path|
|`ExecStrategy::run`|runs `command` there as an argument vector, with no shell, the variables of `env` and none of the host's own, waits, and returns how it ended with what it printed. An action that cannot be started is `Err`|
|`ExecStrategy::collect`|stores into `cas` what the action left at each path of `outputs`, and returns one entry per path in order: its digest, or `None` when nothing is there|
|`Dispatcher::dispatch`|calls `step(None)` once and runs each task it returns through `work`; hands each result to `step(Some(result))` on the calling thread, one at a time, and runs the tasks that returns; returns when no task is running and none is waiting. `work` may run on any thread, several at once. Every task returned by `step` is run, and every result reaches `step`. `Err` is the adapter's own failure|

`ports/bundle.rs`'s `Ports` gains five associated types and `ports/port_set.rs`'s `PortSet` five
fields:

|Associated type|Bound|Field|
|---|---|---|
|`Cas`|`ContentStore`|`cas`|
|`Log`|`EventLog`|`log`|
|`Strategy`|`ExecStrategy`|`strategy`|
|`Dispatcher`|`Dispatcher`|`dispatcher`|
|`Clock`|`Clock + Sync`|`clock`|

`Clock` itself is unchanged; the bound is the bundle's because a worker reads the clock (D19). The
three implementations of `ActionCache`, the three of `Ports` and the two `PortSet` values of §1 grow
with them; `PortSet`'s `Debug` asks that the twelve port types be `Debug`. The crate gains no
production implementation of any port.

## 5. Execute

### 5.1 Signature, placement and order

```rust
pub fn execute<P: Ports>(
    graph: &TargetGraph,
    plan: &Plan,
    on_failure: OnFailure,
    ports: &mut PortSet<P>,
) -> Result<Vec<ActionOutcome>>
```

A free function in a new phase module `execute/`, re-exported from the crate root as `load`,
`resolve` and `plan` are. It names no other phase module. It reads `manifest`, `cache`, `cas`, `log`,
`strategy`, `dispatcher` and `clock`.

```
 graph, plan, on_failure, ports
      │
  1. plan        the plan was made from this graph                ─ Err: ForeignPlan
      │
  2. manifest    ports.manifest.workspace()                       ─ Err: Adapter
      │
  3. layout      one OutputPath per output, no conflict, §5.2     ─ Err: OutputConflict
      │
  4. commands    per node in id order: expand §5.3, admit §5.4    ─ Err: NoProgram, Refused
      │
  5. dispatch    ports.dispatcher.dispatch(work, step), §5.5–§5.9 ─ Err: Adapter, CanonicalJson
      │
  6. outcomes    one per node, in id order
```

Steps 1 to 4 ask nothing of `cache`, `cas`, `log`, `strategy`, `dispatcher` or `clock`, and return
the first error they meet (D20). Step 1 holds when `plan.len()` equals `graph.len()` and every
`plan.label(id)` equals the node's label.

### 5.2 Layout

Every output of every node gets `OutputPath::of(&workspace.out, &node.label, name)`. Two paths
conflict when they are equal, or when one, followed by `/`, is a prefix of the other. The outputs
are visited in `OutputPath` order, then label order; the first that conflicts with one visited
before it is `OutputConflict`, holding the shorter of the two paths and the sites of the two targets,
lower label first. One target declaring both conflicting outputs names itself twice.

### 5.3 Expansion

Each argument of a node's command is scanned once, left to right. At each `$`:

|Text at the `$`|Is|Becomes|
|---|---|---|
|`$opt:` and a name, by the rule of `argument.rs:58-59`|a setting reference|the value `own.settings` holds for the name|
|`$` and a run of ASCII letters that is exactly `in`|the inputs|the node's inputs, in written order|
|`$` and a run of ASCII letters that is exactly `out`|the outputs|the node's own output paths, in written order|
|`$` and a run of ASCII letters that is exactly `deps`|the dependencies' outputs|every output path of every dependency, in edge order then the dependency's written order|
|anything else|text|itself|

The run of letters is the longest one, so `$input` is text and `$out/app` is the outputs followed by
`/app`. `own` is the node's `OwnComponents`, taken from its verdict. A reference to a setting `own`
does not hold stays text; Resolve and Plan leave none (`error.rs:178`; `action_key.rs:80-81`).
Substituted text is never scanned again (PI).

An argument that is exactly one of the three lists and nothing else is replaced by one argument per
path, and by none when the list is empty. In any other argument a list becomes its paths joined by
one space. The result is the node's expanded command.

|Target|Declared|Expanded|
|---|---|---|
|`//lib:text`, input `lib/a.c`|`cc -c $in -o $out`|`cc -c lib/a.c -o out/lib/text`|
|`//:app`, deps `//:main.o`, `//lib:text`|`cc $deps -o $out`|`cc out/main.o out/lib/text -o out/app`|
|`//:app_test`, dep `//:app`, `test_filter` = `Login`|`$deps --filter=$opt:test_filter`|`out/app --filter=Login`|
|`//:gen`, no input|`gen $in`|`gen`|

A `desc` is not expanded: nothing in this slice shows one.

### 5.4 Admission

A command that expanded to no argument, or whose first argument is one `Program::of` yields nothing
for — empty, or still holding `$` — is `NoProgram` with the node's site. Otherwise the first argument
is the action's program, and one outside `workspace.ceiling.run` is `Refused`, with
`Want::Program`, the node's site and the ceiling (D5, follow-up #14). Every node is checked, whatever
its verdict.

### 5.5 The schedule

`execute/schedule.rs`: a state per node, a count per node of dependencies not yet succeeded, the
policy, and whether the build has stopped. It asks no port.

```
 Pending ──► Started ──► Succeeded
    │            ├─────► Failed
    │            └─────► Skipped      abandoned: the build stopped before a worker began it
    └──────────────────► Skipped
```

|Operation|Effect|
|---|---|
|`Schedule::new(graph, on_failure)`|every node `Pending`, counting its dependencies: the length of `graph.deps(id)`, which the graph yields for every id of its own|
|`start() -> Vec<NodeId>`|every node counting zero becomes `Started`; returned in id order|
|`complete(graph, node, Finish::Succeeded) -> Effects`|`node` becomes `Succeeded`. Unless the build has stopped, each of its dependents counts one less, and a `Pending` one reaching zero becomes `Started`|
|`complete(graph, node, Finish::Failed) -> Effects`|`node` becomes `Failed`. Under `Stop` the build stops, remembering `node`, and every `Pending` node becomes `Skipped`. Under `KeepGoing` every `Pending` node reachable from `node` through `dependents` becomes `Skipped`|
|`abandon(node)`|a `Started` node becomes `Skipped`|
|`stopped() -> Option<NodeId>`|the failure that stopped the build, if it has stopped|

`Effects { started: Vec<NodeId>, skipped: Vec<NodeId> }`, both in id order. The walk through
`dependents` uses an explicit stack; nothing recurses over the graph. Its caller is the only writer,
so the counts are plain integers. A build stops only under `Stop`, at its first failure.

### 5.6 Settling a started target

For a `Started` node, with `own` its `OwnComponents`, `row` the cache's row for its label, and every
dependency `Succeeded`:

1. The key is `ActionKey::new(KeyComponents { own, deps })`, `deps` holding each dependency's label
   and `produced()` from its current row, in edge order. A dependency that succeeded and has no row
   is `Error::Adapter` naming `ActionCache`.
2. The target is **cached** when its freshness is not `Always`, `row` is present, the key's digest
   equals the row's key's, and `cas.contains` every digest of `row.digests()` (D6, follow-up #15).
3. Otherwise it **runs**: a task holding the node, the key and the `ReadyAction` of §3.2.

A cached target is recorded at once (§5.8) and finished (§5.9), which may start further targets;
those are settled in turn, in a queue, before `step` returns. A started node reached after the build
has stopped is not settled: it is abandoned (§5.9).

### 5.7 One action, on a worker

`execute/perform.rs`. `work` is this function over `&strategy`, `&cas`, `&clock` and a stop flag, an
`AtomicBool` that `step` sets and `work` only reads (N2):

```
 the stop flag is set                   ─► not begun: no clock read, no port call
 started = clock.now()
 prepare(action)                    Err ─► InfraFailure(Prepare)
 run(dir, action)                   Err ─► InfraFailure(Run)
 put(stdout), put(stderr)           Err ─► InfraFailure(Store)
 exit is TimedOut                       ─► InfraFailure(TimedOut)
 exit is Exited(n), n not zero          ─► ActionFailure { status: n }
 collect(dir, action, cas)          Err ─► InfraFailure(Store)
   not one entry per output             ─► InfraFailure(Store)
   an entry is None                     ─► OutputMissing { name }
   otherwise                            ─► the outputs' digests
 finished = clock.now()
```

The rows are tried top to bottom and the first that applies decides how the action ended. Whichever
row that is, `finished` is read last, so every action a worker began carries both instants. An `Err`
becomes the outcome's diagnostic, as the error's `Display` text; a reply of the wrong length gets
`the strategy answered for <n> outputs of <m>`. The result carries the node, the key, the two
instants, the digests of `stdout` and `stderr` when both were stored, and either the output digests
or the failure's class. A task that was not begun carries its node and nothing else.

### 5.8 Recording, on the calling thread

`execute/record.rs`. For each result of a worker, and for each cached target:

```
 a target that ran and produced its outputs
   place(digest, path) per output, in written order     Err ─► InfraFailure(Store)
   record(label, CacheRow::new(key, outputs, stdout, stderr))
                                                        Err ─► InfraFailure(Store)
                                                        Ok  ─► Built
 a cached target
   place(digest, path) per output, in written order     Err ─► InfraFailure(Store)
                                                        Ok  ─► Cached
 a target that failed on its worker                         ─► its class, unchanged
 every target, skipped ones included
   log.append(event), at = clock.now()                  Err ─► the build's error
```

Nothing is written to the cache for a target that failed, was cached or was skipped. A row therefore
exists only for a target whose outputs a worker stored first: **cas first, cache row after, log
always**. `CacheRow::new` returning `None` cannot happen after §5.7's length check and is
`InfraFailure(Store)` with the diagnostic `the row does not match its key` if it does.

### 5.9 The loop

`step` owns the schedule, one outcome slot per node, a queue of started nodes waiting to be settled,
and the first error that must fail the build. Every target reaches its outcome through one
procedure:

```
 finish(node, outcome)
   store the outcome in the node's slot; append its event (§5.8)
   effects = schedule.complete(node, Succeeded when outcome.class.succeeded(), else Failed)
   per node of effects.skipped, in id order:
     outcome Skipped { because: node }, stored and its event appended
   when the build has just stopped: set the stop flag
   queue every node of effects.started

 drain
   until the queue is empty, take its oldest node:
     the build has stopped ─► schedule.abandon(node);
                              outcome Skipped { because: schedule.stopped() }, stored, event appended
     it is cached (§5.6)    ─► record it (§5.8); finish(node, its outcome)
     otherwise              ─► add its task to the tasks to return

 step(None)                 queue every node of schedule.start(); drain; return the tasks
 step(Some(a result))       record it (§5.8); finish(node, its outcome); drain; return the tasks
 step(Some(not begun))      schedule.abandon(node);
                            outcome Skipped { because: schedule.stopped() }, stored, event appended;
                            return no task
```

So a failure is handled the same way wherever it lands — on a worker, or while a cached target is
placed during `step(None)` or a drain — and under `Stop` nothing begins after it: targets still
`Pending` are skipped by the schedule, started ones waiting in the queue are abandoned by `drain`,
and tasks already handed to the dispatcher come back not begun (§5.7).

Once an error that fails the build is held — `log.append`, `ActionKey::new`, or a contract breach of
§5.6, an `Error::Adapter` naming `ActionCache` with the diagnostic
`<label> succeeded and has no row` — `step` sets the stop flag, makes no further port call and
returns no task; results still arriving are dropped. When `dispatch` returns, `execute` returns that
error, or `dispatch`'s own, or else the outcomes. Every node has one: a node is started only by the
schedule, every started node is settled or abandoned, and every task's result reaches `step` (§4).

Rows and cas contents do not depend on the order results arrive in: a row is a function of its key
and of what the action produced. Which actions a worker had already begun when a failure lands does
depend on it, so under `Stop` the set of skipped targets is fixed only for a dispatcher that runs one
task at a time. This narrows the intent's wording, which has a failing build fail the same way on
every run: that holds for the errors of steps 1 to 4, for every row and for the cas, and not for
which targets a parallel build had reached.

## 6. The pipeline

```rust
impl<P: Ports> Pipeline<P> {
    pub fn build(
        &mut self,
        entry: &EntryName,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
    ) -> Result<Vec<ActionOutcome>>;
}
```

`build` runs `load` once, `resolve`, `plan`, then `execute`, and returns whatever any of them
returns. `check`, `graph` and `plan` keep their signatures. `pipeline/` stays the only module that
names more than one phase.

## 7. Errors

Three variants join `Error`; none is reshaped. `PortName` gains five variants.

|Variant|Fields|
|---|---|
|`ForeignPlan`|none|
|`OutputConflict`|`path: OutputPath`, `first: Box<DeclarationSite>`, `second: Box<DeclarationSite>`|
|`NoProgram`|`site: Box<DeclarationSite>`|

`Refused` and `Adapter` are raised as they are. Sites are boxed as the existing variants box theirs
(`error.rs:201-215`).

|`PortName`|`Display`|
|---|---|
|`ActionCache`|`action cache`|
|`ContentStore`|`content store`|
|`EventLog`|`event log`|
|`ExecStrategy`|`exec strategy`|
|`Dispatcher`|`dispatcher`|

Required renderings, one line each:

|Variant|`Display`|
|---|---|
|`ForeignPlan`|`the plan was not made from this graph`|
|`OutputConflict`|`out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here`|
|`NoProgram`|`build.lua: //:app: the command names no program once expanded`|
|`Refused`, from §5.4|`build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)`|

These are the texts the implementation must produce for those inputs; they are requirements, not
captured output. The last is `Refused`'s existing rendering.

## 8. Module layout

```
crates/buildl-core/src/
  types/output_path.rs      new   OutputPath
  types/ready_action.rs     new   ReadyAction
  types/raw_outcome.rs      new   Exit, RawOutcome
  types/outcome.rs          new   InfraKind, OutcomeClass, ActionOutcome, Summary
  types/outcome_event.rs    new   OutcomeEvent
  types/on_failure.rs       new   OnFailure
  types/cache_row.rs        edit  stdout, stderr, digests
  types/mod.rs              edit  modules, re-exports, doc index
  ports/content_store.rs    new   ContentStore
  ports/event_log.rs        new   EventLog
  ports/exec_strategy.rs    new   ExecStrategy
  ports/dispatcher.rs       new   Dispatcher
  ports/action_cache.rs     edit  record
  ports/bundle.rs           edit  five associated types; the doctest
  ports/port_set.rs         edit  five fields; Debug; the test stub
  ports/mod.rs              edit  modules, re-exports, doc index
  execute/mod.rs            new   docs, mod, pub use — nothing else
  execute/fixtures.rs       new   test-only: graphs and plans built by hand
  execute/assembly.rs       new   execute: the six steps, and the two closures
  execute/layout.rs         new   §5.2
  execute/expand.rs         new   §5.3
  execute/admission.rs      new   §5.4
  execute/schedule.rs       new   §5.5
  execute/settle.rs         new   §5.6
  execute/perform.rs        new   §5.7
  execute/record.rs         new   §5.8, and the event
  plan/verdict.rs           edit  one CacheRow::new call in its tests
  error.rs                  edit  three variants, five PortName variants
  pipeline/driver.rs        edit  build
  lib.rs                    edit  mod execute, re-exports, crate doc
crates/buildl-core/tests/flows/
  build.rs                  new   §9's flows
  golden/log.jsonl          new   the pinned events of one build
  common.rs                 edit  five fakes, FakeCache::record, FakePorts, ports()
  plan.rs                   edit  one CacheRow::new call
  main.rs                   edit  mod build
crates/expected-module-edges.txt   edit
```

Only `assembly` is a public module of `execute/`, as in `plan/`. `execute/fixtures.rs` holds no
implementation of a port. Each file keeps its decisions in functions over plain values, and the
functions that call a port stay thin around them: `settle` decides from the key, the row and whether
each digest is held; `perform` decides the class from an exit and a list of collected entries;
`record` builds the row, the outcome and the event from values. The first kind is unit-tested in the
file, the second by the flows, which is the crate's fifth module rule (`lib.rs:47-50`) and how
`plan/` is tested today. `assembly.rs` owns the stop flag and the two closures that share it. The golden edge file gains exactly four lines — `execute -> error`,
`execute -> ports`, `execute -> types`, `pipeline -> execute` — and must hold no edge between
`execute` and `load`, `resolve` or `plan` in either direction.

## 9. Testing

Unit tests sit in the file under test. `execute/mod.rs`, `ports/mod.rs` and the four new port files
hold no logic and carry the exemption note `ports/action_cache.rs:11` carries; so do
`types/ready_action.rs`, `types/raw_outcome.rs` and `types/on_failure.rs`, which hold only type
definitions (rust-guidelines `unit-test-mandate.md:15-32`).

|File|Covers|
|---|---|
|`types/output_path.rs`|the four rows of §3.1; ordering; serialization as text|
|`types/outcome.rs`|`succeeded` for every class; `Summary::of` over one outcome of each class; each class's JSON|
|`types/outcome_event.rs`|the exact JSON of §3.5 for a built, a failed and a skipped event|
|`types/cache_row.rs`|`new` with too few, too many and the right number of digests; `digests` in order|
|`error.rs`|§7's renderings, one per row; the five `PortName` texts|
|`execute/layout.rs`|equal paths; a path under another; `lib` against `lib2`; one target naming itself twice; the lowest path reported; no conflict|
|`execute/expand.rs`|every row of §5.3's two tables; an empty list alone and inside text; a setting value holding `$in` left alone; `$input`, `$`, `$opt:` with no name; two placeholders in one argument|
|`execute/admission.rs`|no argument; an empty first argument; one still holding `$`; a program inside and outside the ceiling|
|`execute/schedule.rs`|a diamond in order; a failure under each policy; a second failure under `KeepGoing`; a success after the build stopped starts nothing; `abandon`; `stopped` before and after a failure under each policy; a chain of 100 000 nodes skipped without recursion|
|`execute/settle.rs`|each condition of §5.6 step 2 failing in turn; `always`; a dependency with no row|
|`execute/perform.rs`|the class for each exit; collected entries of the wrong length, with one missing, with two missing, and complete; an error's text as the diagnostic|
|`execute/record.rs`|the row, the outcome and the event built for each class of §5.8; a cached target's outcome taken from its row|
|`execute/assembly.rs`|drives the ports, so the flows below cover it; any pure helper it keeps is tested in the file|

Flows in `tests/flows/build.rs`, through `Pipeline::<FakePorts>::build`. The new fakes, all in
memory and built from the public API alone:

|Fake|Holds|
|---|---|
|`FakeCas`|blobs by digest and placed paths, behind a `Mutex`; digests it is told to forget; paths and content it is told to fail on|
|`FakeCache`|gains `record`, and labels it is told to fail on|
|`FakeLog`|the events appended; whether to fail|
|`FakeStrategy`|per label, how the action exits, what it prints and what each output holds or that it is missing; behind a `Mutex`, a count of runs per label and every `ReadyAction` it was handed|
|`FakeDispatcher`|runs inline, on the calling thread, taking the oldest or the newest waiting task first|
|`FakeClock`|a counter behind an atomic, one tick per read|

`FakeCas`, `FakeStrategy` and `FakeClock` are shared with a worker, so what they record sits behind a
`Mutex` or an atomic, not the `Cell` and `RefCell` the existing fakes use
(`tests/flows/common.rs:5`), which the `Sync` bounds of §4 refuse.

An output's content is scripted, defaulting to text derived from the label, the output's name and the
digests of the action's inputs as the fake digester holds them, so an edited input changes what its
target produces unless the script says otherwise.

|Scenario|Assertion|
|---|---|
|a workspace never built|every target `Built`; a row, the outputs in the cas and a placed path for each; one event per target; each target started after its dependencies|
|built again, nothing changed|every target `Cached`; no action ran; no row written; every output placed again|
|one input changes and its target's output does too|that target and its dependents `Built`; an unrelated target `Cached`|
|one input changes and its target's output does not|that target `Built`; its dependents `Cached` and not run — early cutoff|
|`//:a` fails under `KeepGoing`, with two targets downstream and two apart|`Summary { failed: 1, skipped: 2, built: 2, cached: 0 }`; both skipped name `//:a`; no row for the three|
|the same workspace under `Stop`, oldest task first, `//:a` taken first|`//:a` failed; the two apart, already handed to the dispatcher, come back not begun and are `Skipped` naming `//:a`, as are the two downstream; the strategy ran one action|
|three targets with no dependency under `Stop`, the first cached with a `place` that fails|the other two are abandoned in the queue and `Skipped`; the strategy ran nothing|
|an action's outputs reach the cas and `record` fails|`InfraFailure { kind: Store }`; `row` yields nothing for it; its dependents skipped; the next build runs it again|
|a non-zero exit; a timeout; `prepare` failing; `run` failing; the cas refusing captured output; `collect` failing; `collect` answering for too few outputs; a declared output not produced|`ActionFailure`, `InfraFailure` of each kind, `OutputMissing`; no row; the event holds the class and, where stored, the digests of what the action printed|
|a built workspace whose cas forgets one output|that target runs and is `Built`; the others `Cached` (follow-up #15)|
|a built workspace, one `place` failing for a cached target|that target `InfraFailure { kind: Store }`; its row unchanged|
|a test target whose command is `$deps`, with and without `out/app` in `proc.run`|with: built, handed the command `out/app`; without: `Refused` naming `out/app` and the target, and no action ran (follow-up #14)|
|a command that expands to nothing; two targets claiming one path; `//:lib` beside `//lib:text`|`NoProgram`; `OutputConflict` for each; no action ran|
|a target with `always = true`, built twice|it runs both times; its dependent is `Cached` the second time|
|two targets with no dependency; the log fails on the first append|`build` returns that `Adapter` error; the strategy ran one action, the second task coming back not begun; the row of the target being logged is kept — finished work survives a build that dies|
|the two-directory workspace of the graph flow, with its test target's command as `$deps`, built once|each action was handed the command and paths §5.3's rules give it — `cc -c main.c -o out/main.o`, `cc out/main.o out/lib/text -o out/app`, `out/app --filter=Login`; that workspace's `//lib:text` runs plain `cc`, so the first row of §5.3's table is not one of them; the events, one canonical JSON object per line, equal `golden/log.jsonl`|
|the same workspace built with the newest task first|the same rows and the same cas contents as with the oldest first|
|a build file that fails to evaluate; a program outside the ceiling|the Load error and Plan's `Refused`, unchanged; no action ran|

`tests/flows/graph.rs`'s fixture keeps `$out/app` (§1): the Plan golden file pins it, and Plan does
not expand. The build flows declare their own workspaces.

## 10. Documentation deliverables and amendments

|File|Change|
|---|---|
|`crates/buildl-core/src/lib.rs`, `crates/buildl-core/README.md`|the module map gains `execute`; the status paragraph names Execute and Record as implemented, recording as the last step of each action|
|`docs/design.md` §3 (`:76`)|an output lives at `out/<directory>/<name>`; two targets may not claim one path (D4)|
|`docs/design.md` §5 (`:142`, `:148`)|the test example runs `$deps`; the placeholder rule of §5.3 (D2)|
|`docs/design.md` §8.5 (`:262`)|placeholders expand to workspace-relative paths, the same in every tier (D1)|
|`docs/design.md` §8.7 (`:298-302`)|a refusal precedes execution in every case, a program first named by expansion included (D5)|
|`docs/architecture.md` §2 (`:134-140`)|`ActionOutcome` as §3.4|
|`docs/architecture.md` §2.1 (`:164-182`)|the states of §5.5; a target becomes cached when it is settled, not only from the plan (D6). The diagram stays Mermaid|
|`docs/architecture.md` §3 (`:193-194`), §3.4 (`:224-241`)|one row for `execute` and its ports; `dispatch(work, step)` (D7); `ExecStrategy::collect` (D11); what runs where (D8)|
|`docs/architecture.md` §3.5 (`:245`)|a row is durable when it is recorded, one write per target (D9)|
|`docs/architecture.md` §4 (`:254-264`)|`OutcomeClass` as §3.4; no `Refusal` (D5, D12)|
|`docs/architecture.md` §7 (`:309`)|"a failing cache write leaves no row"|
|`docs/architecture-building-blocks.md` §5.1 (`:147-150`), §5.2 (`:193-215`), §5.3 (`:232-245`, `:265-278`)|the new handoff and classification types; the five signatures of §4; the bundle with `Dispatcher` and `Clock + Sync`|
|`docs/architecture-building-blocks.md` §5.4 (`:300-301`), §6 (`:314-318`), §8 (`:362-365`, `:372`, `:374`)|one `execute` row; the catalog's `ContentStore` and `ActionCache` rows; the fakes table; "cache write fails"; a wanted program outside the ceiling is a `Refused` error, not a `Refusal` class (D5)|
|`README.md`, `CLAUDE.md`|only where they repeat a sentence amended above|
|`docs/roadmap.md`|the I6 row and §4 on completion; §5 as §11 says|

Every section cross-reference survives.

## 11. Definition of Done

- `cargo make dod` passes, including `guard-core-purity`, `guard-crate-edges` and
  `guard-module-edges`.
- `cargo deny check` passes; `crates/buildl-core/Cargo.toml`'s dependencies are unchanged.
- Every row of §9 has a passing test.
- The documents of §10 are amended.
- `docs/roadmap.md` §5 no longer lists follow-ups #14, #15 and #16, and carries these:

|Follow-up|Target|
|---|---|
|A program named through `$opt:` in a command's first argument is held to `proc.run` at build time but has no fingerprint in the key, so replacing that tool leaves the target clean|the next slice that changes Plan|
|A rule's `desc` is not expanded|the slice that adds the `Reporter` port|
|An output that is a directory tree cannot be stored: `collect` answers one digest per output. `docs/design.md:338` and `:398` declare such outputs without the stamp file `:342` prescribes|the `buildl` adapters slice|
|`[execution]`'s `timeout`, `workers` and `strategy` are not in `Workspace`, so the adapters that need them have no port to read them from|the `buildl` adapters slice|
|Reading a cache row back now also covers `stdout` and `stderr` (extends #17)|the `buildl` adapters slice|

## 12. Non-goals

- The `Reporter` port, output blocks, the status line, `--stream`, expanding `desc`, and replaying a
  cached target's output — the adapters and CLI slices.
- Selecting the subgraph a requested target needs, choosing targets by role, and demand-driven
  loading (`docs/design.md` §11.1).
- The `Approver` port — milestone 4.
- Every real adapter: the cas on disk, `.buildl/cache.json`, `.buildl/log.jsonl`, the thread-pool
  dispatcher, the `Bare` and `InputSandbox` strategies, the system clock; how a strategy keeps an
  action from writing through a link into the cas; a cheaper durable write than one per row.
- Reading the timeout, the worker count and the strategy from the manifest.
- Parsing `--keep-going`, mapping a class to an exit code, and rendering a summary — the CLI slice.
- An output that is a directory, and storing an output without holding it in memory.
- Fingerprinting a program first named by expansion (§11).
- `$dep://` placeholders, hermetic toolchains and `b.fetch` — milestone 5.
- Enforcing `network`, the input sandbox, the OS sandbox and containers.
- A second failure's cause for a target downstream of two; reporting more than one error before
  anything runs.
- Serializing an `ActionOutcome` or a `Summary`, and reading an event back.
- `buildl test`, `clean`, `gc`, `log`, `doctor`, `verify`, and writing `graph.json` during a build.
- Watch mode, `query` and `rdeps`, the remote cache and remote execution, discovery targets.
