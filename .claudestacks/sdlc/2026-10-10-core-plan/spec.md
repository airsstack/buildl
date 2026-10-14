---
status: approved
created: 2026-10-10
---

# Spec: Plan — from `TargetGraph` to `Plan`

`buildl-core` can resolve a workspace into a `TargetGraph` but cannot say whether any target needs to
run. This spec adds the third phase. Plan reads the workspace manifest, refuses a build whose wants
exceed the ceiling, gives every target an action key built from what six ports report, compares it
with the previous build's, and hands back one `Plan`: a verdict per target and the wanted set. It adds
`Pipeline::plan`, six ports, and the types the key, the verdicts and the ceiling are made of. The
decisions of §2 (D1–D20) answer the eleven questions the intent left open and record the further
choices the work forced.

---

## 1. Design premises, and where each one comes from

Every claim here was checked in this task against the artifact named. This spec asserts nothing about
the behaviour of a system outside this repository. The rust-guidelines rows cite the plugin at
version 0.1.4, under `~/.claude/plugins/cache/claudestacks/claudestacks-guideline-rust/0.1.4/`.

|Premise|Evidence|
|---|---|
|The wanted set is derived from the graph, intersected with the ceiling, and a want outside it refuses the plan before anything runs|`docs/design.md:157`, `:167` (§6)|
|The ceiling has four lists: `proc.run`, `env.read`, `fs.read`, `fs.write`; `fs` entries are workspace-relative|`docs/design.md:92-96` (§4)|
|`[workspace]` has the keys `name`, `entry`, `out`, `config`; the state directory `.buildl` is fixed by the layout, not configured|`docs/design.md:86-90` (§4), `:71` (§3)|
|`plan` explains itself: dirtiness has a mechanical answer|`docs/design.md:196` (§7 rule 1)|
|The key folds dep action keys|`docs/design.md:230`, `:237` (§8.3), `:622` (§12.4); `docs/architecture.md:151`, `:207`; `README.md:91`, `:98`; `CLAUDE.md:28`|
|A byte-identical rebuilt output leaves downstream targets clean|`docs/design.md:273` (§8.5); `docs/fundamental-walkthrough.md:87`, `:97`; `docs/architecture-building-blocks.md:324`|
|Build files are inputs whose hashes invalidate their own targets|`docs/design.md:197` (§7 rule 2), `:628` (§12.4)|
|A missing tool is reported at plan time, naming the target and its declaring file; the tool fingerprint is a content hash or a version string|`docs/design.md:387` (§9.6)|
|A setting's value folds into the key; an override is `--set name=value`|`docs/design.md:151` (§5), `:498` (§10.2)|
|The stat cache reuses a recorded digest when mtime and size are unchanged|`docs/design.md:237` (§8.3), `docs/architecture.md:209` (§3.3)|
|The approver is milestone 4|`docs/design.md:643` (§13)|
|`Plan` is drawn as `{ dirty, reasons, wanted }`; `CacheRow` as `{ key: Digest, outputs: Vec<Digest>, stdout, stderr }`|`docs/architecture.md:123-127` (§2), `:236` (§3.5)|
|Plan uses `Digester`, `StatCache`, `ToolResolver`, `ActionCache`, `Manifest`|`docs/architecture.md:185` (§3)|
|Where a `--set` value is validated is open|`docs/architecture.md:308` (§8)|
|`ActionCache` is drawn with `row`, `upsert` and `commit`|`docs/architecture-building-blocks.md:170-174` (§5.2)|
|Two flows are Plan's to pass in `buildl-core`: a wanted program outside the ceiling, and one changed input|`docs/architecture-building-blocks.md:325-326` (§8)|
|The crate's five module rules|`crates/buildl-core/src/lib.rs:35-49`|
|`load` and `resolve` are free functions re-exported from the crate root|`crates/buildl-core/src/load/traversal.rs:44`, `crates/buildl-core/src/resolve/assembly.rs:51`, `crates/buildl-core/src/lib.rs:76`, `:79`|
|`Pipeline` holds one field, `source`; `new` takes it; `check` and `graph` take `&self`|`crates/buildl-core/src/pipeline/driver.rs:20-21`, `:38`, `:52`, `:68`|
|`Ports` has one associated type, `Source`|`crates/buildl-core/src/ports/bundle.rs:39-41`|
|`Pipeline::new` is called at five test sites and `Ports` is implemented once outside a doctest|`grep -rn --include='*.rs' -e 'Pipeline::' -e 'impl Ports' crates`: `crates/buildl-core/tests/flows/check.rs:24`, `:46`, `:60`, `:85`; `tests/flows/graph.rs:27`; `tests/flows/common.rs:100`; the doctest at `src/ports/bundle.rs:35`|
|A graph's nodes are numbered in `Label` order; `deps`, `dependents`, `settings` are its accessors|`crates/buildl-core/src/types/target_graph.rs:53`, `:132`, `:138`, `:159`|
|A `Command` holds at least one argument; an `Argument` may be empty|`crates/buildl-core/src/types/argument.rs:26-27`, `:134-139`; probe PC|
|A `Directory` is workspace-relative and the empty string is the root|`crates/buildl-core/src/types/directory.rs:21-25`, `:33`, `:67`|
|`Timestamp` is a nanosecond count and is never a key component|`crates/buildl-core/src/types/timestamp.rs:17-27`|
|`Digest::of` hashes bytes; a `Digest` serializes as its hexadecimal text|`crates/buildl-core/src/types/digest.rs:26-28`, `:42`; probe PA|
|The canonical serializer sorts object keys and is reached as `json::canonical::to_vec` / `to_string`|`crates/buildl-core/src/json/canonical.rs:68`, `:80`|
|`DeclarationSite` is a label and its build file; error sites are boxed|`crates/buildl-core/src/error.rs:143`, `:160`, `:179`|
|`buildl-core` may not read the environment|`crates/buildl-core/clippy.toml:48`, `:50`|
|The module-edge guard hangs off `clippy` and diffs `crates/expected-module-edges.txt`, which holds 16 edges|`Makefile.toml:60`, `:146-162`; `wc -l` printed `16`|
|Public fields and parameters are domain types; static dispatch through generics; one concept, one type|rust-guidelines `strong-types.md:15-20`, `static-dispatch.md:9-14`, `modularity.md:32-51`|

### 1.1 Facts established by running something

Probes PA–PD were a temporary file `crates/buildl-core/tests/flows/probe.rs`, run with
`cargo test -p buildl-core --test flows probe -- --nocapture --test-threads=1` and deleted afterwards
(`4 passed; 0 failed`).

**PA — a key-shaped value serializes to the same bytes whatever order its maps were filled in.** A
struct with a `BTreeMap<EnvName, Option<String>>`, a list of `(SourcePath, Digest)` pairs, a
`BTreeMap<SettingName, SettingValue>`, an absent tool and one dependency, filled in two insertion
orders:

```
PA {"deps":[{"label":"//lib:text","outputs":[["text","e3b9…3dd8"]]}],"env":{"HOME":null,"PATH":"/bin"},"inputs":[["src/z.c","594e…7b06"],["src/a.c","ca97…48bb"]],"settings":{"test_filter":"TestLogin"},"tool":null}
PA same bytes either insertion order: true
PA digest b226cd6ab07d26bc3ee06cbab3ecfd38a2f19b8bd04c5643b7f3e17e63547ff5
```

Consequences: `EnvName` and `SettingName` work as map keys; an unset value is `null`; a list keeps the
order it is given (`src/z.c` before `src/a.c`); the key of §3.2 needs no change to `json::canonical`.

**PB — an input is workspace-relative, outputs default to the target's name, and ids are not
topological.** A root file with `a` depending on `z`, `z` declaring two outputs, and `lib/build.lua`
with `text` reading `a.c` and `sub/b.c`:

```
PB id=0 //:a deps=[1] inputs=[] outputs=["a"]
PB id=1 //:z deps=[] inputs=["main.c"] outputs=["bin/z", "z.map"]
PB id=2 //lib:text deps=[] inputs=["lib/a.c", "lib/sub/b.c"] outputs=["text"]
```

Consequences: a `Node`'s inputs can be handed to a port and compared with a ceiling directory as they
are (§5.3); node `0` depends on node `1`, so walking ids in order is not bottom-up and Plan needs an
order of its own (D19).

**PC — what a command's first argument may be.**

```
PC empty command ok=false
PC argument "" ok=true                 PC argument "bin/$opt:tool" ok=true
PC argument "cc" ok=true               PC argument "/usr/bin/cc" ok=true
PC argument "$out/app" ok=true         PC argument "scripts/gen.sh" ok=true
```

Consequence: every node has a first argument, and it may be empty or hold a placeholder, so "the
program a command names" needs its own rule (D6).

**PD — clippy refuses a function of eight parameters.** `fn seven` with seven parameters and
`fn eight` with eight, linted with `cargo clippy -p buildl-core --all-targets -- -D warnings`:

```
error: this function has too many arguments (8/7)
   --> crates/buildl-core/tests/flows/probe.rs:116:1
    = note: `-D clippy::too-many-arguments` implied by `-D warnings`
```

`seven` drew no diagnostic. Consequence: a `plan` function taking the graph, the overrides and six
ports one by one cannot pass the gate (D13).

**N1 — the port catalog has no port for environment values or the host platform.**
`sed -n 263,277p docs/architecture-building-blocks.md | grep -n -i -e environment -e platform -e host -e os/arch`
printed nothing and exited `1`. Control: the same lines with `grep -c ToolResolver` printed `1`.

**N2 — nothing in `buildl-core` computes a topological order.**
`grep -rn -i -e toposort -e topolog crates/buildl-core/src` printed nothing and exited `1`. Control:
`grep -c OnStack crates/buildl-core/src/resolve/cycle.rs` printed `4`.

**PE — the planned shapes need no boxing beyond the error sites.** A second temporary probe file,
run the same way and deleted, declared the types of §3.2, §3.4 and §3.5 and the five variants of §7
field for field, with `String` newtypes standing in for `Program`, `EnvValue` and `Platform`:

```
PE Error today        = 112        PE Unfinished         = 232
PE five new variants  = 48         PE PlannedKey         = 264
PE OwnComponents      = 208        PE DirtyReason        = 24
PE ActionKey          = 264        PE Verdict            = 288
```

`cargo clippy -p buildl-core --all-targets -- -D warnings` over that file reported no size lint:
`grep -c -e large_enum_variant -e large-enum-variant -e 'large size difference'` over its output
printed `0`. Control: `grep -c` for ``unreachable `pub` item`` over the same output printed `19`.

Consequences: the five variants together are smaller than `Error` is today, so adding them does not
grow it and `result_large_err`, which passes today, still passes; `Verdict` and `PlannedKey` hold
their keys by value (§3.4).

## 2. Decisions

|#|Question|Decision|Because|
|---|---|---|---|
|D1|What a key holds for a dependency, and what Plan says downstream of a dirty target|The dependency's label and each of its outputs' name and digest. Plan has three verdicts: clean, dirty, conditional|Chosen by the author. Early cutoff is binding (intent); a key holding the dependency's own key changes whenever the dependency's inputs do. A dirty dependency's new output does not exist at plan time, so a target whose only open question is that output is conditional, not dirty|
|D2|Whether a build file is a key input|No|Chosen by the author. Every field of a target that changes what its action produces is in the key (D5), so an edit that changes one changes the key and an edit that does not rebuilds nothing|
|D3|Which wants are checked|Programs, environment names, declared inputs and the workspace's write roots|Chosen by the author. A ceiling list nothing checks is a hole|
|D4|The approver|Not in this slice|Intent; `docs/design.md:643`|
|D5|What else a key holds|§3.2. Not the label, provenance, role, description or `always`. `env` is keyed by name|None of the five changes what the action produces. An environment is a set of variables, so reordering an `env` list rebuilds nothing; `inputs` and `deps` keep written order because `$in` and `$deps` expand in it (`docs/design.md:153`)|
|D6|Which argument names a tool, and what a fingerprint is|The first argument, when it is non-empty and holds no `$`. A key holds at most one tool. A fingerprint is a `Digest`|PC. `docs/design.md:387` leaves the fingerprint's source to the resolver, and `Digest` is the crate's one identity type. A first argument holding a placeholder names nothing until the executor expands it; checking that is carried to I8 (§11)|
|D7|What a cache row holds|The whole `ActionKey`, components included, and one digest per declared output|Without the previous components there is nothing to diff; without the output digests a dependent has nothing to key on|
|D8|What a dirty reason holds|`Always`, `NeverBuilt`, or the names of every component that differs. No old or new values|Agreed with the author. The values are in the cache row and the plan for a renderer that wants them|
|D9|The shape of `Plan`|One verdict per node, the nodes' labels, the wanted set. Serialized keyed by label, by hand; no `Deserialize`|A plan must serialize without the graph beside it, and no `NodeId` goes to a file (Resolve D7)|
|D10|Who supplies environment values and the platform|One new port, `Host`|N1. Both are facts about the machine the actions run on|
|D11|Who reads the stat cache, and who writes it|`Digester` reports a file's metadata and digests; `StatCache` holds recorded entries; the reuse rule is Plan's. Plan records and commits, and commits nothing when it recorded nothing|Agreed with the author. The rule is logic, so it is tested against fakes. A stat cache Plan could not write would re-hash every changed file on every `plan`|
|D12|How much of `ActionCache` lands|`row` only|This slice reads rows; `upsert` and `commit` belong to the slice that writes them|
|D13|How a pipeline takes its ports|One `PortSet<P>` struct with a public field per port; `Pipeline::new(PortSet<P>)`|PD. Seven ports now and more with I8|
|D14|Where an override is checked, and how it arrives|At Plan, as `&BTreeMap<SettingName, SettingValue>`|Agreed with the author. Plan is the first phase an invocation value reaches. Closes `docs/architecture.md:308`|
|D15|How many failures are reported|The first, in the fixed order of §5.1|Same as Load and Resolve (Resolve D5)|
|D16|What a refusal names|The first want outside the ceiling, the lowest-labelled target wanting it with its build file, and the ceiling|`docs/design.md:167`; D15|
|D17|What the manifest gives Plan|`Workspace { out, state, ceiling }`; ceiling `fs` entries are `Directory` values|`docs/design.md:86-96`. `out` and `state` are `buildl-core`'s half of follow-up #13|
|D18|A `Grants` type|Not introduced here|With no approver the grant is the wanted set, once it is inside the ceiling. Whether the type exists is milestone 4's to decide, with the approver; the documents that name it are left as they are (§10)|
|D19|The order targets are visited in|A topological order computed here, iteratively|PB, N2; no recursion over the graph (`docs/architecture.md:203`)|
|D20|How an adapter reports its own failure|One variant, `Error::Adapter`, naming the port|The crate has one error enum, and a port's failure is not a fact about a declaration|

## 3. Data model

Nine new files under `types/`. Every type derives `Debug, Clone, PartialEq, Eq` unless stated.

### 3.1 Small values

|Type|File|Grammar and form|
|---|---|---|
|`Program`|`types/program.rs`|Text naming a program to run. Non-empty, no `NUL`, no `$`. `Program::parse(raw) -> Result<Self>`; `Program::of(&Argument) -> Option<Self>`, `None` when the argument is empty or holds `$`. `Ord`, `Display`, serializes as its text|
|`EnvValue`|`types/env_value.rs`|An environment variable's value: any UTF-8 text without `NUL`, the empty string included. `EnvValue::parse`. Serializes as its text|
|`Platform`|`types/platform.rs`|`<os>/<arch>`: two parts joined by one `/`, each 1 to 64 bytes of ASCII letters, digits, `_`, `-` and `.`. `Platform::parse`. Serializes as its text|
|`FileStat`|`types/file_stat.rs`|`{ modified: Timestamp, size: u64 }`, `Copy`. What decides whether a file changed|
|`StatEntry`|`types/file_stat.rs`|`{ stat: FileStat, digest: Digest }`, `Copy`. A file's digest and the metadata it was taken under|

`Program`, `EnvValue` and `Platform` follow the shape of the existing names: a private field, a
fallible `parse` returning `Error::InvalidName`, `as_str`, `Display`, `FromStr`
(rust-guidelines `strong-types.md:39-48`). `NameKind` gains `Program`, `EnvValue` and `Platform`.

### 3.2 The key

`types/action_key.rs`.

```rust
pub struct InputDigest  { pub path: SourcePath, pub digest: Digest }
pub struct Produced     { pub name: OutputName, pub digest: Digest }
pub struct ToolIdentity { pub program: Program, pub fingerprint: Digest }
pub struct DepProducts  { pub label: Label, pub outputs: Vec<Produced> }

pub struct OwnComponents {
    pub command: Command,
    pub inputs: Vec<InputDigest>,
    pub outputs: Vec<OutputName>,
    pub env: BTreeMap<EnvName, Option<EnvValue>>,
    pub tool: Option<ToolIdentity>,
    pub settings: BTreeMap<SettingName, SettingValue>,
    pub network: NetworkAccess,
    pub platform: Platform,
}

pub struct KeyComponents { pub own: OwnComponents, pub deps: Vec<DepProducts> }

pub struct ActionKey { /* digest: Digest, components: KeyComponents */ }
```

`OwnComponents` is everything a key holds that does not wait on another target. It is its own type
because a target downstream of a dirty one has exactly this much of its key at plan time (§3.4).

|Field|Holds|
|---|---|
|`command`|the node's `run`, placeholders unexpanded|
|`inputs`|the node's inputs in written order, each with its content digest|
|`outputs`|the node's output names in written order|
|`env`|each name in the node's `env`, with its value, or `None` when the variable is unset|
|`tool`|the program the command names (D6) and its fingerprint; `None` when it names none|
|`settings`|each setting the command references, with its effective value: the override, or else the default|
|`network`|the node's `network`|
|`platform`|the host's|
|`deps`|per dependency in edge order: its label, and its outputs' names and digests in its own written order|

`ActionKey::new(components: KeyComponents) -> Result<Self>` serializes the components through
`json::canonical::to_vec` and hashes the bytes with `Digest::of`. It is the only constructor, so a
key's digest always matches its components. Accessors: `digest() -> Digest`,
`components() -> &KeyComponents`.

All seven types implement `Serialize`. The serialized components, which are the bytes a key is the
digest of:

```
{
  "deps": [ { "label": "<label>", "outputs": [ { "digest": "<hex>", "name": "<name>" }, ... ] }, ... ],
  "own": {
    "command":  ["<argument>", ...],
    "env":      { "<name>": "<value>" | null, ... },
    "inputs":   [ { "digest": "<hex>", "path": "<path>" }, ... ],
    "network":  "Sealed" | "Declared",
    "outputs":  ["<name>", ...],
    "platform": "<os>/<arch>",
    "settings": { "<name>": "<value>", ... },
    "tool":     { "fingerprint": "<hex>", "program": "<text>" } | null
  }
}
```

An `ActionKey` serializes as `{ "components": <the above>, "digest": "<hex>" }`. These shapes are
requirements, pinned by §9's golden file. No type here implements `Deserialize`: nothing in this
slice reads a key back, and the cache adapter's slice adds what it needs (§11).

### 3.3 The cache row

`types/cache_row.rs`.

```rust
pub struct CacheRow { /* key: ActionKey, outputs: Vec<Digest> */ }
```

`CacheRow::new(key: ActionKey, outputs: Vec<Digest>) -> Option<Self>` returns `None` when `outputs`
does not hold one digest per name in `key.components().own.outputs`. Accessors: `key() -> &ActionKey`,
`produced() -> Vec<Produced>`, pairing each output name with its digest in order. `stdout` and
`stderr` (`docs/architecture.md:236`) arrive with the slice that captures them.

### 3.4 Verdicts and the plan

`types/plan.rs`.

```rust
#[non_exhaustive]
pub enum Difference {
    Command, Input(SourcePath), InputOrder, Outputs, Env(EnvName), Tool,
    Setting(SettingName), Network, Platform, Dep(Label), DepOrder,
}

pub enum DirtyReason { Always, NeverBuilt, Changed(Vec<Difference>) }

pub struct Unfinished { pub own: OwnComponents, pub awaiting: Vec<NodeId> }

pub enum PlannedKey { Finished(ActionKey), Unfinished(Unfinished) }

pub enum Verdict {
    Clean { key: ActionKey },
    Dirty { reason: DirtyReason, key: PlannedKey },
    Conditional(Unfinished),
}

pub struct Plan { /* labels: Vec<Label>, verdicts: Vec<Verdict>, wanted: WantedSet */ }
```

|Verdict|Meaning|
|---|---|
|`Clean`|the key equals the previous build's; served from cache|
|`Dirty`|will run. Its key is finished when every dependency is clean, unfinished otherwise|
|`Conditional`|runs only if a dependency in `awaiting` produces something other than what the previous build recorded|

`Unfinished::awaiting` holds the dependencies that are not clean, in edge order. An unfinished key
carries everything `Manifest`, `Host`, `Digester`, `StatCache` and `ToolResolver` had to supply, so
the slice that runs actions finishes it from the graph's edges and its dependencies' cache rows,
without asking those five ports again. No variant is boxed (PE).

`Plan`'s fields are private. `labels[i]` and `verdicts[i]` belong to `NodeId::new(i)` of the graph the
plan was made from. Constructor: `pub(crate) fn new(labels, verdicts, wanted)`. Accessors:
`len()`, `is_empty()`, `label(id) -> Option<&Label>`, `verdict(id) -> Option<&Verdict>`,
`verdicts() -> &[Verdict]`, `wanted() -> &WantedSet`.

`Plan` implements `Serialize` by hand and not `Deserialize`. The shape:

```
{
  "targets": {
    "<label>": {
      "verdict":    "Clean" | "Dirty" | "Conditional",
      "reason":     null | "Always" | "NeverBuilt" | { "Changed": [ <difference>, ... ] },
      "key":        <ActionKey> | null,
      "unfinished": { "awaiting": ["<label>", ...], "own": <OwnComponents> } | null
    }
  },
  "wanted": { "env": ["<name>", ...], "network": ["<label>", ...],
              "programs": ["<text>", ...], "write": ["<dir>", ...] }
}
```

A difference is its name as text, or an object of one key for the four that name something:
`{ "Input": "<path>" }`, `{ "Env": "<name>" }`, `{ "Setting": "<name>" }`, `{ "Dep": "<label>" }`.
`reason` is `null` unless the verdict is `Dirty`. Exactly one of `key` and `unfinished` is non-null.
`awaiting` is written as labels. Requirements, pinned by §9's golden file.

### 3.5 Authority and the workspace

`types/authority.rs` and `types/workspace.rs`. Fields are public: these values have no invariant
beyond their fields' own.

```rust
pub struct Ceiling {
    pub run: BTreeSet<Program>,       // proc.run
    pub env: BTreeSet<EnvName>,       // env.read
    pub read: BTreeSet<Directory>,    // fs.read
    pub write: BTreeSet<Directory>,   // fs.write
}

pub struct WantedSet {
    pub programs: BTreeSet<Program>,
    pub env: BTreeSet<EnvName>,
    pub write: BTreeSet<Directory>,
    pub network: BTreeSet<Label>,
}

pub enum Want { Program(Program), Env(EnvName), Read(SourcePath), Write(Directory) }

pub struct Workspace { pub out: Directory, pub state: Directory, pub ceiling: Ceiling }
```

`Workspace::out` is the directory named outputs are linked into and `state` the directory buildl
keeps its own files in (`out/` and `.buildl/` in `docs/design.md:71-76`). `WantedSet` does not copy
the declared inputs: they are the graph's, and §5.3 checks them where they are.

`Directory` gains `contains(&self, other: &Directory) -> bool`: true when the two are equal or `self`
is an ancestor of `other`; the root contains every directory.

## 4. Ports

Six new traits under `ports/`, one file each, and the `PortSet` struct. Every fallible method returns
the crate's `Result`.

```rust
pub trait Manifest {
    fn workspace(&self) -> Result<Workspace>;
}

pub trait Host {
    fn platform(&self) -> Platform;
    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>;
}

pub trait Digester {
    fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>>;
    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>>;
}

pub trait StatCache {
    fn entry(&self, path: &SourcePath) -> Option<StatEntry>;
    fn record(&mut self, path: SourcePath, entry: StatEntry);
    fn commit(&mut self) -> Result<()>;
}

pub trait ToolResolver {
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>>;
}

pub trait ActionCache {
    fn row(&self, label: &Label) -> Option<&CacheRow>;
}
```

|Method|Contract|
|---|---|
|`Manifest::workspace`|the workspace's directories and ceiling. An unreadable or invalid manifest is `Error::Adapter`|
|`Host::env`|the variable's value; `Ok(None)` when it is unset; `Error::Adapter` when it has a value an `EnvValue` cannot hold|
|`Digester::stat`|the file's metadata; `Ok(None)` when no file is there|
|`Digester::digest`|one content digest per path, in the order given. Batched so an adapter may hash in parallel (`docs/architecture.md:209`)|
|`StatCache::entry`|what was recorded for the path, if anything|
|`StatCache::record`, `commit`|`record` replaces the path's entry in memory; `commit` persists every entry|
|`ToolResolver::fingerprint`|the program's fingerprint; `Ok(None)` when it cannot be found|
|`ActionCache::row`|the previous build's row for the label, if it has one|

`ports/bundle.rs`'s `Ports` gains six associated types beside `Source`: `Manifest`, `Host`,
`Digester`, `StatCache`, `Tools: ToolResolver` and `Cache: ActionCache`. `ports/port_set.rs`:

```rust
pub struct PortSet<P: Ports> {
    pub source: P::Source,
    pub manifest: P::Manifest,
    pub host: P::Host,
    pub digester: P::Digester,
    pub stat_cache: P::StatCache,
    pub tools: P::Tools,
    pub cache: P::Cache,
}
```

It is the value-level twin of `Ports`: the trait names one implementation of each port, the struct
holds one instance of each (D13). The crate gains no production implementation of any port.

## 5. Plan

### 5.1 Signature, placement and order

```rust
pub fn plan<P: Ports>(
    graph: &TargetGraph,
    overrides: &BTreeMap<SettingName, SettingValue>,
    ports: &mut PortSet<P>,
) -> Result<Plan>
```

A free function in a new phase module `plan/`, re-exported from the crate root as `load` and `resolve`
are. It names no other phase module, and reads every field of `ports` except `source`.

```
 graph, overrides, ports
      │
  1. manifest    ports.manifest.workspace()                       ─ Err: Adapter
      │
  2. settings    every override names a declared setting          ─ Err: UnknownOverride
      │
  3. authority   every want is inside the ceiling, §5.3           ─ Err: Refused
      │
  4. tools       one fingerprint per distinct program             ─ Err: MissingTool, Adapter
      │
  5. inputs      stat, reuse or digest, record, commit, §5.4      ─ Err: MissingInput, Adapter
      │
  6. host        the platform; one value per env name, §5.4       ─ Err: Adapter
      │
  7. components  one OwnComponents per node, in id order
      │
  8. verdicts    bottom-up over a topological order, §5.5–§5.6
      │
  9. assemble    Plan::new
```

Plan returns the first error it meets (D15). The steps run in the order shown and each step's own
order is fixed below, so a workspace with several faults always reports the same one. Nothing is
asked of `Digester`, `StatCache`, `ToolResolver`, `Host` or `ActionCache` before step 3 has passed.

### 5.2 Settings

Overrides are visited in `SettingName` order. One that names no setting in `graph.settings()` is
`UnknownOverride`. The effective value of a setting is its override, or else its default. A node's
`settings` component holds the settings its command references — `Argument::setting_references` over
each argument — and no other, so an override changes the keys of exactly the targets that refer to it.

### 5.3 Authority

|Want|Derived from|Inside the ceiling when|
|---|---|---|
|`Program`|`Program::of` a node's first argument, when it yields one|it is in `ceiling.run`|
|`Env`|each name in a node's `env`|it is in `ceiling.env`|
|`Read`|each of a node's `inputs`|a directory in `ceiling.read` is the root, or the path starts with that directory followed by `/`|
|`Write`|`workspace.out` and `workspace.state`|a directory in `ceiling.write` `contains` it|

Classes are checked in that order, and within a class the lowest want first, by its type's `Ord`. The
first want outside the ceiling is `Refused`. Its site is the node with the lowest label that wants it.
A `Write` want has no site: the two write roots are the manifest's, and no target or build file
declares them. This narrows the intent's wording, which has every refusal name a target and its
declaring file. A node whose `network` is `Declared` adds its label to
`WantedSet::network`, which nothing is compared with: the ceiling has no entry for it
(`docs/design.md:92-96`).

`WantedSet` holds the distinct programs, the distinct environment names, the two write roots and the
network labels.

### 5.4 Tools, inputs and the host

**Tools.** The distinct programs of step 3, in `Program` order. `fingerprint` returning `Ok(None)` is
`MissingTool`, with the lowest-labelled node naming that program as its site.

**Inputs.** The distinct inputs of every node, in `SourcePath` order:

1. `stat` each. `Ok(None)` is `MissingInput`, with the lowest-labelled node declaring that input.
2. An input whose `StatCache::entry` holds a `FileStat` equal to the one just read reuses that
   entry's digest. Equality, not order: a modification time earlier than the recorded one is a change.
3. The remaining inputs go to `Digester::digest` in one call, in `SourcePath` order. A reply that
   does not hold one digest per path is `Error::Adapter` naming `Digester`.
4. Each digested input is `record`ed with the `FileStat` of step 1. If anything was recorded,
   `commit` is called once; otherwise it is not called, so a plan that hashes nothing writes nothing.

**Host.** `platform` is read once. Then `env` is asked once for each distinct name in any node's
`env`, in `EnvName` order, so the first variable an adapter cannot report is always the same one.

**Components.** Step 7 builds one `OwnComponents` per node, in id order, from the node and the
values steps 2 to 6 gathered. It asks no port and cannot fail.

### 5.5 Order

A topological order of the graph's nodes, every node after all of its dependencies. Computed with a
queue and one counter per node: the counter starts at `deps(id).len()`, nodes whose counter is zero
are queued in id order, and removing a node decrements each of its `dependents`, queuing one when its
counter reaches zero. No function recurses over the graph. The graph has no cycle (`resolve`'s
guarantee, `target_graph.rs:53-56`), so every node is reached.

### 5.6 Verdicts

For each node in that order, with `row` the cache's row for its label, `own` its `OwnComponents`, and
`settled` true when every dependency's verdict is `Clean`:

```
 freshness is Always ─────────────────────────────► Dirty(Always)
 row is None ─────────────────────────────────────► Dirty(NeverBuilt)
 settled,     key digest equals row's ────────────► Clean
 settled,     otherwise ──────────────────────────► Dirty(Changed(differences))
 not settled, known differences is not empty ─────► Dirty(Changed(known differences))
 not settled, otherwise ──────────────────────────► Conditional
```

The rows are tried top to bottom. When `settled`, the key is finished: its `deps` are each
dependency's label and `row.produced()` from that dependency's own row, which a clean dependency
always has. Otherwise the key is `Unfinished`, awaiting the dependencies that are not clean.

**Differences** compare the node's components with `row.key().components()`, and are listed in this
order:

|Difference|Present when|
|---|---|
|`Command`|the commands differ|
|`Input(path)`|the path is an input on one side only, or on both with different digests; in path order|
|`InputOrder`|both sides hold the same paths in a different order|
|`Outputs`|the output name lists differ|
|`Env(name)`|the name is on one side only, or on both with different values; in name order|
|`Tool`|the tools differ|
|`Setting(name)`|the name is on one side only, or on both with different values; in name order|
|`Network`, `Platform`|the values differ|
|`Dep(label)`|the label is a dependency on one side only, or on both with different products; in label order|
|`DepOrder`|both sides hold the same dependency labels in a different order|

The **known differences** of a node that is not settled are the same list, with `Dep(label)` reporting
different products only for a dependency that is clean. A dependency that is not clean has no
products yet, and is in `awaiting` instead.

When `settled` and the digests differ, the list is not empty: equal components serialize to equal
bytes.

## 6. The pipeline

```rust
impl<P: Ports> Pipeline<P> {
    pub const fn new(ports: PortSet<P>) -> Self;
    pub const fn ports(&self) -> &PortSet<P>;
    pub const fn ports_mut(&mut self) -> &mut PortSet<P>;
    pub fn plan(
        &mut self,
        entry: &EntryName,
        overrides: &BTreeMap<SettingName, SettingValue>,
    ) -> Result<Plan>;
}
```

`Pipeline` holds one field, the `PortSet`. `new` takes it in place of the source (D13). `check` and
`graph` keep their signatures and read `ports.source`. `plan` runs `load` once, `resolve`, then
`plan`, and returns whatever any of them returns. It takes `&mut self` because recording into the stat
cache does. `ports_mut` lets a caller that owns the adapters reach them between commands; the flows of
§9 use it to change a fake's files and rows between two plans. `pipeline/` stays the only module that
names more than one phase.

## 7. Errors

Five variants join `Error`; none is reshaped. One helper type joins `error.rs`:

```rust
#[non_exhaustive]
pub enum PortName { Manifest, Host, Digester, StatCache, ToolResolver }
```

`Display`: `manifest`, `host`, `digester`, `stat cache`, `tool resolver`. Derives
`Debug, Clone, Copy, PartialEq, Eq`.

|Variant|Fields|
|---|---|
|`UnknownOverride`|`name: SettingName`|
|`Refused`|`want: Want`, `site: Option<Box<DeclarationSite>>`, `ceiling: Box<Ceiling>`|
|`MissingTool`|`site: Box<DeclarationSite>`, `program: Program`|
|`MissingInput`|`site: Box<DeclarationSite>`, `path: SourcePath`|
|`Adapter`|`port: PortName`, `diagnostic: Diagnostic`|

`Adapter` is the variant an adapter returns for a failure of its own; Plan raises it once itself, for
a `Digester` reply of the wrong length (§5.4).

Required renderings, one line each. `allows:` lists the ceiling's entries for the want's class, in
order, separated by `, `; an empty list renders as `nothing`.

|Variant|`Display`|
|---|---|
|`UnknownOverride`|`setting test_fliter is overridden but not declared`|
|`Refused`, program|`build.lua: //:image: program docker is outside the ceiling (proc.run allows: cc, ld)`|
|`Refused`, env|`build.lua: //:app: environment variable AWS_SECRET is outside the ceiling (env.read allows: HOME, PATH)`|
|`Refused`, read|`lib/build.lua: //lib:text: input lib/key.pem is outside the ceiling (fs.read allows: src)`|
|`Refused`, write|`directory out is outside the ceiling (fs.write allows: nothing)`|
|`MissingTool`|`build.lua: //:app: program cc was not found`|
|`MissingInput`|`build.lua: //:app: input src/main.c does not exist`|
|`Adapter`|`stat cache: <the diagnostic>`|

The workspace root in an `allows:` list renders as `.`. These are the texts the implementation must
produce for those inputs; they are requirements, not captured output.

## 8. Module layout

```
crates/buildl-core/src/
  types/program.rs          new   Program
  types/env_value.rs        new   EnvValue
  types/platform.rs         new   Platform
  types/file_stat.rs        new   FileStat, StatEntry
  types/action_key.rs       new   the key's seven types
  types/cache_row.rs        new   CacheRow
  types/plan.rs             new   Difference, DirtyReason, Unfinished, PlannedKey, Verdict, Plan
  types/authority.rs        new   Ceiling, WantedSet, Want
  types/workspace.rs        new   Workspace
  types/directory.rs        edit  Directory::contains
  types/mod.rs              edit  modules, re-exports, doc index
  ports/manifest.rs         new   Manifest
  ports/host.rs             new   Host
  ports/digester.rs         new   Digester
  ports/stat_cache.rs       new   StatCache
  ports/tool_resolver.rs    new   ToolResolver
  ports/action_cache.rs     new   ActionCache
  ports/port_set.rs         new   PortSet
  ports/bundle.rs           edit  six associated types; the doctest
  ports/mod.rs              edit  modules, re-exports, doc index
  plan/mod.rs               new   docs, mod, pub use — nothing else
  plan/fixtures.rs          new   test-only: graphs built by hand, shared by the unit tests
  plan/assembly.rs          new   plan: the nine steps in order
  plan/settings.rs          new   §5.2
  plan/authority.rs         new   §5.3
  plan/tools.rs             new   §5.4, tools
  plan/inputs.rs            new   §5.4, inputs
  plan/components.rs        new   steps 6 and 7
  plan/order.rs             new   §5.5
  plan/difference.rs        new   §5.6, the comparison
  plan/verdict.rs           new   §5.6, the verdicts
  error.rs                  edit  five variants, PortName, three NameKind variants
  pipeline/driver.rs        edit  PortSet, ports, ports_mut, plan
  lib.rs                    edit  mod plan, re-exports, crate doc
crates/buildl-core/tests/flows/
  plan.rs                   new   §9's flows
  golden/plan.json          new   the pinned serialized plan
  common.rs                 edit  six fakes, FakePorts, a PortSet builder
  check.rs, graph.rs        edit  the five Pipeline::new call sites
  main.rs                   edit  mod plan
crates/expected-module-edges.txt   edit
```

`plan/fixtures.rs` builds its graphs with `TargetGraph::new`, which is `pub(crate)`, so the unit tests
need neither `resolve` nor a build file. It holds no implementation of a port. Each `plan/` file
keeps its decisions in functions over plain values, and the functions that call a port stay thin
around them: the first kind is unit-tested in the file, the second by the flows, which is the
crate's fifth module rule (`lib.rs:46-49`) and how `load/traversal.rs` is tested today. The golden edge file gains exactly four lines:
`pipeline -> plan`, `plan -> error`, `plan -> ports`, `plan -> types`. It must hold no edge between
`plan` and `load` or `resolve` in either direction.

## 9. Testing

Unit tests sit in the file under test and take plain values, never a port (§8). `plan/mod.rs`,
`ports/mod.rs`, the six port files and `ports/port_set.rs` hold no logic and carry the exemption note
`ports/declaration_source.rs:11` carries; so do `types/authority.rs`, `types/workspace.rs` and
`types/file_stat.rs`, which hold only type definitions.

|File|Covers|
|---|---|
|`types/program.rs`, `env_value.rs`, `platform.rs`|each grammar at its edges; `Program::of` over PC's six arguments|
|`types/action_key.rs`|the exact bytes of §3.2 for one value; the same digest from two insertion orders; a different digest when each field changes in turn|
|`types/cache_row.rs`|`new` with too few, too many and the right number of digests; `produced` pairs in order|
|`types/plan.rs`|the exact JSON of one plan holding all three verdicts and each reason; accessors in and out of bounds|
|`types/directory.rs`|`contains`: equal, ancestor, descendant, sibling with a shared prefix (`lib` and `lib2`), the root|
|`error.rs`|§7's renderings, one per row|
|`plan/settings.rs`|an unknown override; the lowest name reported; a default and an override; only referenced settings kept|
|`plan/authority.rs`|every row of §5.3 inside and outside the ceiling; class order; the lowest-labelled site; an empty ceiling; `lib2/x` against a ceiling of `lib`|
|`plan/tools.rs`|the distinct programs of a graph, each with its lowest-labelled site; an empty and a placeholder first argument yield none|
|`plan/inputs.rs`|the reuse decision: an equal stat, no entry, an earlier modification time, a changed size; the batch in path order; a reply of the wrong length|
|`plan/components.rs`|one node's `OwnComponents` from gathered values: an unset variable, a command naming no program, only the referenced settings, inputs in written order|
|`plan/order.rs`|every node after its dependencies for a diamond; a chain of 100 000 nodes|
|`plan/difference.rs`|every row of §5.6's table, alone and together, in the stated order|
|`plan/verdict.rs`|every row of §5.6's diagram; a clean dependency whose products differ from the row's; `awaiting` in edge order|
|`plan/assembly.rs`|drives the ports, so the flows below cover it; any pure helper it keeps is tested in the file|

Flows in `tests/flows/plan.rs`, through `Pipeline::<FakePorts>::plan`. The fakes are in-memory: a
manifest holding one `Workspace`; a host holding a platform and a map of variables; a digester holding
files as bytes with a `FileStat` each, counting `stat` and `digest` calls; a stat cache counting
commits; a resolver holding fingerprints, counting lookups; a cache holding rows. A previous build is
simulated through the public API: the targets are taken in dependency order, each one's key is
finished with `ActionKey::new` from the `OwnComponents` in its plan entry and its dependencies' rows,
and a row is inserted for it with chosen output digests. The helper works for a target that is never
clean, which a loop waiting for every target to be clean would not.

|Scenario|Assertion|
|---|---|
|a workspace never built|every target `Dirty(NeverBuilt)`; a target with dependencies has an unfinished key awaiting them|
|after a simulated build|every target `Clean`; `digest` is not called and nothing is committed|
|one input's content and metadata change|its target `Dirty(Changed([Input(path)]))`; its dependents `Conditional` awaiting it; an unrelated target `Clean`|
|that target's row is replaced by one with a new key and the same output digests|its dependents `Clean` — early cutoff|
|that target's row is replaced by one with different output digests|its dependent `Dirty(Changed([Dep(label)]))`|
|a file's metadata changes and its content does not|it is digested once, its target stays `Clean`, the stat cache is committed once|
|an override of a setting two of three targets reference|those two `Dirty(Changed([Setting(name)]))`, the third `Clean`|
|an environment value, a tool fingerprint and the platform each change in turn|`Env(name)`, `Tool`, `Platform`|
|a target with `always = true`|`Dirty(Always)` on every plan; its dependents `Conditional`|
|a program outside the ceiling|`Refused` naming the program, the target and its build file; `stat` and `digest` call counts are zero|
|an environment name, an input and the output directory outside the ceiling|`Refused` for each|
|a tool that cannot be found; an input that does not exist|`MissingTool`, `MissingInput`, each with its site|
|two targets naming one program; two variables a host cannot report|one lookup for the program; the `Adapter` error is for the lower name|
|a workspace with an unknown override, a refused program, a missing tool and a missing input at once|`UnknownOverride`; with the override removed, `Refused`; then `MissingTool`; then `MissingInput`|
|an override naming no setting|`UnknownOverride`|
|the two-directory workspace of the graph flow, after a simulated build and one edit|the canonical JSON of the plan equals `golden/plan.json`|
|a build file that fails to evaluate|the Load error, unchanged|

## 10. Documentation deliverables and amendments

|File|Change|
|---|---|
|`crates/buildl-core/src/lib.rs`, `crates/buildl-core/README.md`|the module map gains `plan`; the status paragraph names Plan as implemented|
|`docs/design.md` §5 (`:150`, `:153`), §12.1 item 2 (`:587`)|every field of a target is a key component except `always`; reordering `env` does not change a key, reordering `deps`, `inputs` or `outputs` does (D5)|
|`docs/design.md` §6 (`:157`)|what a build wants also includes every declared input, checked against `fs.read`. The sentence is about the classes of want; the `WantedSet` a plan reports lists programs, environment names, write roots and network targets, and leaves the inputs in the graph (§3.5)|
|`docs/design.md` §7 rule 2 (`:197`), §12.4 last row (`:628`)|a build file reaches a key through what it declares; its own hash is not a key component (D2)|
|`docs/design.md` §8.3 (`:230`, `:237`), §12.4 (`:622`)|the key folds each dependency's outputs, not its key; a target downstream of a dirty one is conditional (D1). The diagram stays Mermaid|
|`docs/architecture.md` §2 (`:123-127`, `:151`)|`Plan` is a verdict per node and the wanted set; the components list says dependency outputs|
|`docs/architecture.md` §3 (`:185`), §3.3 (`:207-211`)|Plan's ports gain `Host`; the bottom-up walk, the three verdicts, the unfinished key|
|`docs/architecture.md` §3.5 (`:236`)|a `CacheRow` holds the whole `ActionKey` (D7)|
|`docs/architecture.md` §8 (`:308`)|an override is checked at Plan (D14); the question is closed|
|`docs/architecture-building-blocks.md` §4 (`:110`), §5.1 (`:149`)|the ports box gains `Host`; Authority gains `Want` and `Workspace`. `Grants`, the `grants` logic row (`:256`) and `Approver` (`:275`) stay as written: they describe milestone 4 (D18)|
|`docs/architecture-building-blocks.md` §5.2–§5.4, §6, §8|the six port signatures; `Ports` and `PortSet`; `plan`'s row; `Host` in the catalog; the fakes table|
|`README.md` (`:91`, `:98`)|the same rule as design §8.3; the status paragraph|
|`CLAUDE.md` (`:28`)|the same rule as design §8.3. Outside the intent's list of affected systems: it is the one other file that states the rule D1 replaces. The author approved this edit explicitly when approving the spec, 2026-10-11|
|`docs/roadmap.md`|the I7 row and §4 on completion; the follow-ups of §11|

`docs/fundamental-walkthrough.md` needs no change: it states early cutoff and never that a key holds a
dependency's key. `grep -c -i -e 'dep keys' -e 'dep action' -e 'their action keys'` over it printed
`0`; control: `grep -c 'Early cutoff'` over the same file printed `3`. Every section cross-reference
survives.

## 11. Definition of Done

- `cargo make dod` passes, including `guard-core-purity`, `guard-crate-edges` and
  `guard-module-edges`.
- `cargo deny check` passes; `crates/buildl-core/Cargo.toml`'s dependencies are unchanged.
- Every row of §9 has a passing test.
- The documents of §10 are amended.
- `docs/roadmap.md` §5 carries these follow-ups:

|Follow-up|Target|
|---|---|
|A command whose first argument holds a placeholder is checked against `proc.run` once it is expanded (D6)|I8|
|A clean target's outputs are confirmed present in the cas before they are linked into `out/`|I8|
|`ActionCache` gains `upsert` and `commit`; `CacheRow` gains `stdout` and `stderr` (D12)|I8|
|Reading a cache row back: `Deserialize` for the key's types|the `buildl` adapters slice|
|Follow-up #13's `buildl-lua` half: the sources walk skips `Workspace::out` and `Workspace::state`|the `buildl` adapters slice|

## 12. Non-goals

- The `Approver` port, the interactive prompt, CI detection, and whether a `Grants` type exists —
  milestone 4.
- Running anything, `OutcomeClass`, and every write to the cas, the action cache and the log — I8.
- Placeholder expansion, and any check of what a placeholder expands to — I8.
- Every real adapter: the `buildl.toml` reader, the filesystem digester, `.buildl/statcache.json`, the
  `PATH` lookup, `.buildl/cache.json`.
- Parsing `--set`, a did-you-mean for an override, and rendering a plan for a terminal — the CLI
  slice.
- Selecting the subgraph a requested target needs, and demand-driven loading (`docs/design.md` §11.1).
- A ceiling entry outside the workspace, and a ceiling entry for the network.
- Removing stat-cache entries for files that are no longer inputs; keying the stat cache by device
  and inode (`docs/architecture.md` §8).
- Hermetic toolchains, `b.fetch` and tools that arrive as dependencies — milestone 5.
- Per-target grant narrowing (`docs/design.md` §14).
- Reporting more than one failure, old and new values in a dirty reason, and reading a plan back from
  JSON.
- Parallel plan.
