---
status: approved
created: 2026-10-10
---

# Spec: Resolve — from `Vec<Declaration>` to `TargetGraph`

`buildl-core` can load a workspace into a sorted `Vec<Declaration>` but relates no declaration to
another. This spec adds the second phase. Resolve takes the declarations, refuses a workspace whose
names collide, whose references name nothing or the wrong kind of thing, or whose dependencies loop,
and hands back one `TargetGraph`: an arena of runnable targets with forward and reverse edges. It adds
`Pipeline::graph`, the graph's serialized form, and the guard that keeps one phase module from
importing another. The decisions of §2 (D1–D12) answer the eight questions the intent left open and
record the further choices the work forced. Resolve uses no port.

---

## 1. Design premises, and where each one comes from

Every claim here was checked in this task against the artifact named.

|Premise|Evidence|
|---|---|
|Resolve is pure in-memory Rust with three validations that each fail with provenance: duplicates (both sites named), unknown references (with a did-you-mean), cycles (the whole path)|`docs/design.md:217` (§8.2)|
|The graph is order-insensitive: "keyed by names, sorted at Resolve"|`docs/design.md:585` (§12.1 item 2)|
|`graph.json` is "sorted keys, diff-stable"|`docs/design.md:72` (§3)|
|`buildl graph` stops after Resolve and emits the DAG|`docs/design.md:184` (§7)|
|Load and Resolve always run; the graph is never cached across edits|`docs/design.md:195` (§7 rule 2)|
|A setting is "referenced in argv as `$opt:name`"; its name and default are visible in `buildl graph`|`docs/design.md:151` (§5), `docs/design.md:496` (§10.2)|
|`b.target`'s table is closed and "the surface grows reluctantly"|`docs/design.md:150` (§5)|
|Phases communicate through values; the pipeline owns the sequence|`docs/architecture.md:42` (§1.1)|
|From Resolve onward a target is a `NodeId`; `Provenance` is threaded, never reconstructed|`docs/architecture.md:135`, `docs/architecture.md:138` (§2)|
|`TargetGraph` is struct-of-arrays: `nodes`, `edges`, `reverse`, `by_label`; petgraph declined|`docs/architecture.md:186` (§3.2), `docs/architecture.md:266` (§6)|
|Cycle detection is iterative DFS with an explicit stack; the error carries the full path as labels with provenance|`docs/architecture.md:188` (§3.2)|
|`Schedule` owns one dependency counter per node|`docs/architecture.md:200` (§3.4)|
|Every error that can name a target carries `Label` and `Provenance`|`docs/architecture.md:240` (§4)|
|No `HashMap` iteration reaches any output|`docs/architecture.md:246` (§5 rule 1)|
|The double evaluation is `buildl check`'s|`docs/architecture.md:259` (§5 rule 5)|
|`graph` is tested from staging lists to a golden `graph.json`, with duplicate, unknown and cycle fixtures|`docs/architecture.md:283` (§7)|
|The resolve logic uses no port|`docs/architecture.md:169` (§3), `docs/architecture-building-blocks.md:254` (§5.4)|
|The fakes live in `crates/buildl-core/tests/flows/common.rs`|`docs/architecture-building-blocks.md:307` (§8)|
|The crate's five module rules, the first being that no phase module names another|`crates/buildl-core/src/lib.rs:34-49`|
|A declared label is always in the declaring directory; a directory is evaluated once and has one build file; the fake source holds one file per directory|`crates/buildl-core/src/load/traversal.rs:153-157`, `:46`, `:62`, `:76-83`; `crates/buildl-core/tests/flows/common.rs:32`|
|`NodeId` wraps a `u32`; the workspace denies `unwrap_used` and `panic`|`crates/buildl-core/src/types/node_id.rs:20`; `Cargo.toml:72`, `:74`|
|`Declared` has four kinds; a target's `Action` is `UseRule(Label)` or `Run(Command)`|`crates/buildl-core/src/types/declaration.rs:51`, `:112`|
|`DeclarationField` already has `Dep`, `Rule` and `Target`|`crates/buildl-core/src/error.rs:202`, `:212`, `:216`|
|`Pipeline` holds one field, `source`, and has one command, `check`|`crates/buildl-core/src/pipeline/driver.rs:19-21`, `:51`|
|A setting name is ASCII letters, digits, `-` and `_`, 1 to 256 bytes|`crates/buildl-core/src/types/grammar.rs:67-82`; probe P4|
|Labels are ASCII: names and path segments hold only ASCII letters, digits, `.`, `-`, `_`|`crates/buildl-core/src/types/grammar.rs:19-22`, `:24-25`, `:50`|
|The architecture guards hang off the `clippy` task; `guard-crate-edges` diffs a golden file|`Makefile.toml:60`, `Makefile.toml:121-144`|
|CI runs `cargo make dod` on Ubuntu and macOS|`.github/workflows/ci.yml:36`, `:67`|
|Follow-up #11 asks for a golden file of permitted intra-crate module edges, diffed by a `cargo make` task|`docs/roadmap.md:102`|
|Every logic-bearing `src/*.rs` ships a colocated test module; `mod.rs` holds only docs, `mod` and `pub use`|rust-guidelines `unit-test-mandate.md:9`, `mod-rs-export-only.md:7`|

### 1.1 Facts established by running something

Probes P1–P5 were a temporary file `crates/buildl-core/tests/flows/probe.rs`, run with
`cargo test -p buildl-core --test flows probe -- --nocapture --test-threads=1` and deleted afterwards.
All five passed (`5 passed; 0 failed`).

**P1 — Load keeps duplicate declarations and the written order of a list.** A root file declaring
target `a` twice, setting `s` twice, and target `b` with `deps = { "z", "a", ":a", "//:a" }`:

```
P1 target //:a deps=[]
P1 target //:a deps=[]
P1 target //:b deps=["//:z", "//:a", "//:a", "//:a"]
P1 other s
P1 other s
```

Consequences: duplicates reach Resolve intact, so Resolve can name each declaration's site (§4.2); a
`deps` list arrives in written order and may repeat a label, so Resolve is where repeats go (D3). The
probe declared both copies in one file. Through Load that is the only way a label repeats, because a
label is always in its declaring directory and a directory has one build file (§1, `traversal.rs`
row); a setting, named workspace-wide, can repeat across files.

**P2 — `Label`'s order is not the order of its text.**

```
P2 Label Ord : ["//:b", "//a:a", "//a:z", "//a-b:x", "//a/b:c"]
P2 text order: ["//:b", "//a-b:x", "//a/b:c", "//a:a", "//a:z"]
```

Consequence: node numbering follows `Label`'s `Ord`, and the keys of the serialized graph follow text
order. The two orders differ, and nothing may assume they agree (§3.3).

**P3 — a hand-written label-keyed `Serialize` impl goes through the canonical serializer with its keys
sorted.** Entries inserted as `//a:z`, `//a-b:x`, `//:b`, and a `BTreeMap<SettingName, String>`:

```
P3 {"//:b":[],"//a-b:x":[],"//a:z":["//a-b:x","//:b"]}
P3 {"test_filter":""}
```

Consequence: D7's shape needs no change to `json::canonical`. Arrays keep the order they are given.

**P4 — the setting-name grammar.**

```
P4 "a-b_1" -> ok=true      P4 "a b" -> ok=false
P4 "A9" -> ok=true         P4 "" -> ok=false
P4 "a.b" -> ok=false       P4 "<256 bytes>" -> ok=true
P4 "a:b" -> ok=false       P4 "<257 bytes>" -> ok=false
```

**P5 — existing renderings the new errors reuse.**

```
P5 fields: dep=dep rule=rule target=target
P5 provenance: build.lua | lib/build.lua
```

**P6 — today's intra-crate module edges, as the guard of §7 lists them.** The script of §7.1, run
over `crates/buildl-core/src` on macOS:

```
error -> types        pipeline -> error
json -> error         pipeline -> load
load -> error         pipeline -> ports
load -> ports         pipeline -> types
load -> types         ports -> error
types -> error        ports -> types
types -> json
```

With a temporary file `src/load/zz_probe.rs` holding a doc line mentioning `crate::json` and the line
`use crate::pipeline::Pipeline;`, the same script printed `load -> pipeline` and no `load -> json`: a
code import is listed, a comment is not. The file was deleted.

**P7 — the two import spellings the script cannot read are absent today.**
`grep -rnE 'crate::\{|super::super' crates/buildl-core/src` printed nothing and exited `1`. Control:
`grep -rncE 'crate::types'` over `src/load` and `src/pipeline` reported `2` matches in each of
`load/traversal.rs` and `pipeline/driver.rs`.

**P8 — no document defines an escape for `$`.**
`grep -n -F -e '$$' docs/design.md docs/architecture.md docs/architecture-building-blocks.md` printed
nothing and exited `1`. Control: `grep -c -F -e '$opt' docs/design.md` printed `2`.

**P9 — no module-root `mod.rs` uses `super::`.**
`grep -n 'super::' crates/buildl-core/src/*/mod.rs` printed nothing and exited `1`. Control:
`grep -c 'super::'` reported `1` in each of `src/error.rs` and `src/load/traversal.rs`.

**P10 — `check` already treats a list's order as part of the declaration.** A temporary flow test,
run and deleted like P1–P5: one root file evaluated twice through `FakeSource::with_second_run`,
target `app` declaring `deps = { "a", "b" }` on the first evaluation and `{ "b", "a" }` on the second.

```
P10 check failed: build.lua: declarations differ between two evaluations
```

Consequence: Load compares lists in written order, so a build file whose list order varies is already
refused. Resolve keeping that order (D3) agrees with it; sorting would have made `graph` ignore an
order `check` rejects a file over.

Not verified: the guard script on Linux. It uses only `grep -E -o -r -h -v`, `sed`, `sort -u`,
`basename`, `mktemp` and `diff`, as `guard-crate-edges` does; CI's Ubuntu job is the first run there.

## 2. Decisions

|#|Question|Decision|Because|
|---|---|---|---|
|D1|What a graph node is|A node is one build or test target. Aliases and settings are side tables; a rule is not in the graph|Every later phase (keys, scheduling, counters) wants a node to be one action. Chosen by the author over "aliases are nodes" and "every label is a node"|
|D2|Where a rule expands|At Resolve: a node holds the command it runs and the rule's description|Follows from D1; a `rule =` reference is a cross-file relationship, which is Resolve's|
|D3|Order inside a node's lists|`deps`, `inputs`, `outputs` and `env` keep the order the build file wrote; a repeated entry is dropped, the first occurrence kept|Chosen by the author over sorted sets. `$deps` is the only way a command names a dependency's outputs (`docs/design.md:148`), so sorting would leave their argument order beyond a build file's reach; and `check` already treats list order as meaningful (P10). The cost: reordering a list changes the command, hence the action key. `docs/design.md:585` is amended to say so (§10)|
|D4|What a reference may name|§4.3's table. An alias names a target only|Chosen by the author: loosening later breaks no build file, tightening would (`docs/design.md:150`)|
|D5|How many failures are reported|The first, in the fixed order of §4.1|Chosen by the author; same as Load, and the crate's `Result` carries one `Error`|
|D6|Settings checks|Every `$opt:name` in a declared command names a declared setting. `--set` is not checked|`docs/design.md:151`; no invocation value reaches the pipeline (intent non-goal)|
|D7|Serialized shape|Keyed by label, dependencies written as labels; hand-written `Serialize`, no `Deserialize`|Chosen by the author: adding a target changes only its own lines. Nothing reads a graph back (`docs/design.md:195`)|
|D8|`Pipeline::graph`|Loads once, then resolves|`docs/architecture.md:259`: the double evaluation is `check`'s|
|D9|Node numbering|`NodeId` `i` is the `i`-th target in `Label` order|Independent of the order declarations arrive in. Ids never leave memory (D7), so renumbering on an edit costs nothing|
|D10|Dependency count|Not stored: it is `deps(id).len()`|`docs/architecture.md:200`: the counters that change belong to `Schedule`|
|D11|Did-you-mean|Own edit distance over the rendered text; no new dependency|`docs/architecture-building-blocks.md:27` (§1.2)|
|D12|More targets than `u32` ids|`Error::TooManyTargets` when there are more than 4 294 967 296 targets, the number of distinct `u32` ids|`NodeId` wraps a `u32` (`node_id.rs:20`), and the workspace denies `unwrap_used` and `panic` (`Cargo.toml:72`, `:74`), so the conversion must be fallible|

## 3. Data model

One new file, `types/target_graph.rs`. A graph is a different value from the declarations it is built
from, so it gets its own types rather than reusing `Target` (crate rule 3).

### 3.1 `Node`

```rust
pub struct Node {
    pub label: Label,
    pub provenance: Provenance,
    pub role: TargetRole,
    pub run: Command,
    pub description: Option<Description>,
    pub inputs: Vec<SourcePath>,
    pub outputs: Vec<OutputName>,
    pub env: Vec<EnvName>,
    pub network: NetworkAccess,
    pub freshness: Freshness,
}
```

Derives `Debug, Clone, PartialEq, Eq`. Differences from `Target`:

|Field|In a `Node`|
|---|---|
|`action`|replaced by `run`: the inline command, or a copy of the named rule's|
|`description`|the named rule's description; `None` for an inline `run`, or when the rule has none|
|`deps`|gone: dependencies are the graph's edges|
|`provenance`|the declaring build file, moved in from the `Declaration`|
|`inputs`, `outputs`, `env`|in written order; a repeated entry dropped, the first occurrence kept|

### 3.2 `TargetGraph`

```rust
pub struct TargetGraph {
    nodes: Vec<Node>,
    edges: Vec<Vec<NodeId>>,
    reverse: Vec<Vec<NodeId>>,
    by_label: BTreeMap<Label, NodeId>,
    aliases: BTreeMap<Label, NodeId>,
    settings: BTreeMap<SettingName, SettingValue>,
}
```

Derives `Debug, Clone, PartialEq, Eq`. Fields are private. Invariants, true of every value:

- `nodes` is in `Label` order; `NodeId::new(i)` names `nodes[i]`.
- `edges[i]` holds the nodes `i` depends on, in the order its `deps` were written, no repeats, all in
  bounds.
- `reverse[i]` holds the nodes that depend on `i`, ascending, no repeats.
- The edges form no cycle.
- `by_label` maps each node's label to its id. `aliases` maps each alias's label to the id of the
  target it names. No label is in both.
- `settings` maps each declared setting to its default.

The only constructor is `pub(crate) fn new(nodes, edges, aliases, settings)`, called by Resolve. It
derives `reverse` and `by_label` from its arguments; the remaining invariants are its caller's to
establish, and §4 does. Outside the crate a graph is obtained from `resolve` alone, so the fakes of
later slices build one the way an adapter would.

Public accessors:

|Method|Returns|
|---|---|
|`len()`, `is_empty()`|the number of nodes|
|`nodes()`|`&[Node]`, in id order|
|`node(id)`|`Option<&Node>`|
|`deps(id)`|`Option<&[NodeId]>`: what `id` depends on|
|`dependents(id)`|`Option<&[NodeId]>`: what depends on `id`|
|`id_of(&Label)`|`Option<NodeId>`: the target with that label, or the target an alias with that label names|
|`aliases()`|`&BTreeMap<Label, NodeId>`|
|`settings()`|`&BTreeMap<SettingName, SettingValue>`|

`None` means the id was not issued by this graph. A node's dependency count is `deps(id).len()` (D10).

### 3.3 Serialized form

`TargetGraph` implements `Serialize` by hand and does not implement `Deserialize`. The shape, with
every object's keys sorted by the canonical serializer:

```
{
  "aliases":  { "<alias label>": "<target label>", ... },
  "settings": { "<setting name>": "<default>", ... },
  "targets":  {
    "<label>": {
      "deps":        ["<label>", ...],
      "description": "<text>" | null,
      "env":         ["<name>", ...],
      "freshness":   "Cached" | "Always",
      "inputs":      ["<path>", ...],
      "network":     "Sealed" | "Declared",
      "outputs":     ["<name>", ...],
      "provenance":  { "directory": "<dir>", "file": "<path>" },
      "role":        "Build" | "Test",
      "run":         ["<argument>", ...]
    }
  }
}
```

The flag texts and the `provenance` object are the ones `Declaration` already serializes with
(`crates/buildl-core/src/types/declaration.rs:242-245`). A target's label is its key and is not
repeated in its body. `deps` lists the labels of `edges[i]` in the order the build file wrote them, and
`env`, `inputs` and `outputs` are in written order too (D3); object keys are in text order. `reverse` and `by_label` are not written: both are derivable.
No `NodeId` appears, so adding a target changes only that target's lines and the `deps` of whatever
names it.

### 3.4 `Argument::setting_references`

`types/argument.rs` gains one method:

```rust
pub fn setting_references(&self) -> Vec<&str>
```

It scans the argument's text left to right. At each occurrence of the literal `$opt:`, the name is the
longest run of ASCII letters, digits, `-` and `_` that follows — the byte class of a setting name
(`grammar.rs:67-82`, P4). A non-empty run is a reference and scanning resumes after it; an empty run is
not a reference and scanning resumes after the `$opt:`. No escape for `$` exists (P8) and none is added.

|Argument|References|
|---|---|
|`$opt:test_filter`|`test_filter`|
|`--run=$opt:a.$opt:b-c`|`a`, `b-c`|
|`$opt:`, `$opt:.x`, `$option`|none|

The method reads an argument's own text, so it belongs to `Argument` (crate rule 2). It expands
nothing; expansion stays the executor's.

## 4. Resolve

### 4.1 Signature, placement and order

```rust
pub fn resolve(declarations: Vec<Declaration>) -> Result<TargetGraph>
```

A free function in a new phase module `resolve/`, re-exported from the crate root as `load` is
(`lib.rs:70`). It
takes no port and names no other phase module. Its result does not depend on the order of
`declarations`: every step below iterates a `BTreeMap` or a sorted `Vec`. The order inside one
declaration's own lists is part of that declaration and is kept (D3).

```
 declarations
      │
  1. index      labels → declarations, settings → declarations   ─ Err: DuplicateLabel, DuplicateSetting
      │
  2. number     targets in Label order get NodeId 0..n            ─ Err: TooManyTargets
      │
  3. check      every reference of every declaration, §4.3        ─ Err: UnknownReference,
      │                                                                  WrongReferenceKind, UnknownSetting
  4. build      nodes (rule expanded, repeats dropped) and edges
      │
  5. cycles     iterative DFS over the edges                      ─ Err: DependencyCycle
      │
  6. assemble   TargetGraph::new
```

Resolve returns the first error it meets (D5). The steps run in the order shown, and each step's own
order is fixed below, so a workspace with several faults always reports the same one.

### 4.2 Index — one namespace for labels, one for settings

Targets, rules and aliases share the label namespace; settings are named workspace-wide by
`SettingName`. Step 1 groups the declarations by label and, separately, by setting name.

|Check|Order|Error|
|---|---|---|
|a label declared more than once, in any mix of kinds|labels first, lowest `Label` first|`DuplicateLabel`|
|a setting declared more than once|after labels, lowest `SettingName` first|`DuplicateSetting`|

The error's `sites` holds the provenance of every declaration of that name, in the order of the
declarations' own `Ord`. Two declarations in one file give the same provenance twice (P1). For a label
that is the only case Load can produce, so `DuplicateLabel` names one file as many times as it declares
the label; `resolve` itself does not rely on that, and reports whatever sites its input holds.

### 4.3 References

Step 3 walks the label index in `Label` order. Within one declaration the order is:

|Declaration|Checked, in this order|
|---|---|
|target|its `rule` reference, if it has one; its `deps` in written order; the `$opt:` references of its inline `run`, in argument order then position|
|rule|the `$opt:` references of its `run`, in argument order then position|
|alias|its target|

What each label reference may name:

|Reference|`field`|May name|Names something else|Names nothing|
|---|---|---|---|---|
|a target's `deps` entry|`Dep`|a target of either role, or an alias|`WrongReferenceKind` (a rule)|`UnknownReference`|
|a target's `rule`|`Rule`|a rule|`WrongReferenceKind` (a target or alias)|`UnknownReference`|
|an alias's target|`Target`|a target of either role|`WrongReferenceKind` (an alias or rule)|`UnknownReference`|

A dependency that names an alias becomes an edge to the target the alias names. An alias whose own
target is wrong is that alias's error, reported when the walk reaches the alias.

A `$opt:name` reference whose name is not a declared setting is `UnknownSetting`. A rule's command is
checked once, at the rule, whether or not a target uses it; a target using a rule adds no check.

**Suggestions.** `UnknownReference` and `UnknownSetting` carry an optional suggestion: the candidate
with the smallest edit distance (Levenshtein, over the bytes of the rendered text; both grammars are
ASCII) from the reference, provided the distance is at most `max(1, n / 3)` where `n` is the
reference's length in bytes. Ties go to the candidate that is lowest by its type's `Ord` (`Label`
or `SettingName`), not by its text. The candidates are the labels of the
kinds the reference may name (the "May name" column), or the declared setting names.

### 4.4 Nodes and edges

For each target, in id order:

- `run` is the inline command, or a clone of the named rule's; `description` is that rule's.
- `inputs`, `outputs` and `env` keep their written order; a repeated entry is dropped and the first
  occurrence kept.
- its edge list is the ids its `deps` resolve to, in written order; an id already in the list is
  dropped, so a target named both directly and through an alias appears once, where it first was. A
  target naming itself,
  directly or through an alias, yields an edge to itself, which step 5 reports as a cycle of one.

### 4.5 Cycles

A depth-first search with an explicit stack and three node states (unvisited, on the stack, finished).
Roots are tried in id order and each node's edges in edge-list order. No function recurses over the graph.
The first edge that reaches a node on the stack closes a cycle: the nodes from that node to the top of
the stack. The reported path is that cycle rotated to start at the label lowest by `Label`'s `Ord`;
each entry depends on the next, and the last depends on the first.

The path names the cycle's targets and their build files. An edge that came through an alias appears
as the edge to the alias's target; the alias is not named, because an edge keeps no record of the name
it was written with (D1).

## 5. Errors

Seven variants join `Error`; none is reshaped. Two helper types join `error.rs`:

```rust
#[non_exhaustive]
pub enum DeclaredKind { Target, Rule, Alias }      // Display: "target", "rule", "alias"

pub struct DeclarationSite { pub label: Label, pub provenance: Provenance }
```

`DeclaredKind` derives `Debug, Clone, Copy, PartialEq, Eq`; `DeclarationSite` derives
`Debug, Clone, PartialEq, Eq`. A `DeclarationSite` is a declared label and the build file that
declared it: the declaration holding a bad reference, or one target on a cycle.

|Variant|Fields|
|---|---|
|`DuplicateLabel`|`label: Label`, `sites: Vec<Provenance>`|
|`DuplicateSetting`|`name: SettingName`, `sites: Vec<Provenance>`|
|`UnknownReference`|`site: Box<DeclarationSite>`, `field: DeclarationField`, `reference: Label`, `suggestion: Option<Label>`|
|`WrongReferenceKind`|`site: Box<DeclarationSite>`, `field: DeclarationField`, `reference: Label`, `found: DeclaredKind`, `declared_in: Provenance`|
|`UnknownSetting`|`site: Box<DeclarationSite>`, `name: String`, `suggestion: Option<SettingName>`|
|`DependencyCycle`|`path: Vec<DeclarationSite>`|
|`TooManyTargets`|`count: usize`|

**Amended 2026-10-10, while planning.** This section first gave the three reference errors separate
`provenance: Provenance` and `label: Label` fields, and a `CycleStep` type for the cycle path. Built
that way in a scratch copy of the crate, `cargo clippy -p buildl-core --all-targets -- -D warnings`
failed on every function returning `Result`:

```
error: the `Err`-variant returned from this function is very large
    | |_____- the variant `UnknownReference` contains at least 193 bytes
    | |_____- the largest variant contains at least 194 bytes
    | |_____- the variant `UnknownSetting` contains at least 144 bytes
    = note: `-D clippy::result-large-err` implied by `-D warnings`
```

Boxing the holder as one `site` passes the same command. `CycleStep` had the same two fields, so one
type, `DeclarationSite`, serves both (crate rule 3). The renderings below are unchanged.

`site` is the declaration holding the reference. `UnknownSetting::name` is a
`String` because a run longer than 256 bytes is a reference that is not a valid `SettingName` (P4), in
the way `InvalidName::value` holds rejected input as given (`error.rs:43`).

Required renderings, one line each. The field and provenance texts are those of P5.

|Variant|`Display`|
|---|---|
|`DuplicateLabel`|`//:a is declared more than once: build.lua, build.lua`|
|`DuplicateSetting`|`setting test_filter is declared more than once: build.lua, lib/build.lua`|
|`UnknownReference`|`build.lua: //:app: dep //:mian.o is not declared (did you mean //:main.o?)`|
|`UnknownReference`, no suggestion|`build.lua: //:default: target //:nope is not declared`|
|`WrongReferenceKind`|`build.lua: //:app: dep //:cc names a rule, declared in tools/build.lua`|
|`UnknownSetting`|`build.lua: //:go_test: setting test_fliter is not declared (did you mean test_filter?)`|
|`DependencyCycle`|`dependency cycle: //:a (build.lua) -> //lib:b (lib/build.lua) -> //:a`|
|`TooManyTargets`|`4294967297 targets are more than a graph can number`|

These are the texts the implementation must produce for those inputs; they are requirements, not
captured output.

## 6. The pipeline

```rust
impl<P: Ports> Pipeline<P> {
    pub fn graph(&self, entry: &EntryName) -> Result<TargetGraph>
}
```

`graph` runs `load` once and passes the result to `resolve` (D8). It returns whatever either returns.
`Pipeline`'s fields, `Pipeline::new`, `check` and the `Ports` bundle are unchanged. `pipeline/` is the
only module that names both phases.

## 7. The module-edge guard

Closes roadmap follow-up #11.

### 7.1 The task

A `cargo make` task `guard-module-edges`, in the shape of `guard-crate-edges`
(`Makefile.toml:121-144`), added to the `clippy` task's dependencies (`Makefile.toml:60`). It:

1. fails if any line under `crates/buildl-core/src` matches `crate::\{|super::super`, or any line of
   a `src/*/mod.rs` matches `super::`, naming the lines. These spellings import across modules
   without the `crate::<module>` text step 2 reads: in a module's `mod.rs`, `super` is the crate root.
   None is in use (P7, P9);
2. lists the edges, with `LC_ALL=C` so the order is the same on every host:

   ```sh
   for entry in "${src}"/*; do
       module="$(basename "${entry}" .rs)"
       if [ "${module}" = "lib" ]; then
           continue
       fi
       grep -rhv '^[[:space:]]*//' "${entry}" \
           | grep -oE 'crate::[a-z_]+' \
           | sort -u \
           | sed "s|^crate::|${module} -> |" \
           | grep -v "^${module} -> ${module}$" >> "${actual}" || true
   done
   ```

   `src` is `crates/buildl-core/src` and `actual` a temporary file. The `|| true` keeps a module
   with no edge from ending the script.

3. diffs that list against the golden file and fails on any difference, with the message naming the
   file and saying to fix the import or update the golden if the change is intended.

### 7.2 The golden file

`crates/expected-module-edges.txt`, beside `crates/expected-edges.txt`. Its content is the script's
output when the last task of this chain lands. It must hold `pipeline -> resolve`, and must hold
neither `load -> resolve` nor `resolve -> load`. Today's list is P6.

The guard works the way `guard-crate-edges` does: a new edge of any kind turns the gate red until the
golden file is edited, so an import from one phase module into another, written in any spelling steps
1 and 2 cover, cannot land unseen. Every phase module is a directory, so step 1's `mod.rs` rule covers
each of them. One spelling stays outside the guard: a top-level `super::` in a single-file module.
`error.rs` is the only such module today, and it is not a phase.

## 8. Module layout

```
crates/buildl-core/src/
  types/target_graph.rs     new   Node, TargetGraph, its Serialize impl
  types/argument.rs         edit  Argument::setting_references
  types/grammar.rs          edit  the key byte class, named so the scanner shares it
  types/declaration.rs      edit  Declaration::into_parts, so the index moves parts, not clones
  types/mod.rs              edit  module, re-exports, doc index
  error.rs                  edit  seven variants, DeclaredKind, DeclarationSite
  resolve/mod.rs            new   docs, mod, pub use — nothing else
  resolve/fixtures.rs       new   test-only: declarations built by hand, shared by the unit tests
  resolve/assembly.rs       new   resolve: the six steps in order
  resolve/index.rs          new   grouping by name, the two duplicate checks
  resolve/references.rs     new   §4.3: kinds, $opt, alias targets
  resolve/suggest.rs        new   edit distance, nearest candidate
  resolve/cycle.rs          new   the iterative search, the rotated path
  pipeline/driver.rs        edit  Pipeline::graph
  lib.rs                    edit  mod resolve, re-exports, crate doc
crates/buildl-core/tests/flows/
  graph.rs                  new   §9's flows
  golden/graph.json         new   the pinned serialized graph
  common.rs, main.rs        edit  builders for rules, aliases, rule-using targets; mod graph
crates/expected-module-edges.txt   new
Makefile.toml                      edit  guard-module-edges
```

Each `resolve/` file has one responsibility and is small enough to test alone
(rust-guidelines `modularity.md:11`). New crate-root re-exports: `resolve`, `TargetGraph`, `Node`,
`DeclaredKind`, `DeclarationSite`. `Declaration::into_parts` adds a method and reshapes nothing: the
type's fields and its existing accessors are unchanged.

## 9. Testing

Unit tests sit in the file under test (rust-guidelines `unit-test-mandate.md:9`); `resolve/mod.rs`
holds no logic and carries the exemption note `load/mod.rs:12` carries.

|File|Covers|
|---|---|
|`types/target_graph.rs`|`new` derives `reverse` and `by_label`; every accessor, in and out of bounds; `id_of` through an alias; the exact JSON of a small graph, including a label pair that sorts differently as text (P2)|
|`types/argument.rs`|§3.4's table, and a run of 257 bytes|
|`error.rs`|§5's renderings, one per row|
|`resolve/index.rs`|over hand-built declarations: duplicates within one file, across files, across kinds; three declarations of one label; the lowest label wins; labels before settings|
|`resolve/references.rs`|every cell of §4.3's reference table; the within-declaration order; a rule's `$opt:` checked without a user|
|`resolve/suggest.rs`|distance of known pairs; the threshold at its edge; ties; no candidates|
|`resolve/cycle.rs`|a cycle of one, of two, of several; rotation to the lowest label; an acyclic diamond; a chain of 100 000 nodes, which a recursive search would not survive|
|`resolve/assembly.rs`|rule expansion; an alias dependency becoming an edge to its target; lists in written order with repeats dropped; the same graph from shuffled declarations; the order of failure across steps|

Flows in `tests/flows/graph.rs`, through `Pipeline::<FakePorts>::graph` and `FakeSource`:

|Scenario|Assertion|
|---|---|
|a two-directory workspace with a rule, an alias, a setting, a test target and a cross-directory dependency|the canonical JSON of the graph equals `golden/graph.json`|
|the same workspace with each file's declarations in another order|the same bytes|
|a label declared twice in one file|`DuplicateLabel` naming that file twice|
|a setting declared in two files|`DuplicateSetting` naming both files|
|a misspelled dependency|`UnknownReference` with the declaring file and the suggestion|
|a dependency naming a rule|`WrongReferenceKind` with both files|
|a cycle across two directories|`DependencyCycle` with every label and its file, lowest label first|
|a build file that fails to evaluate|the Load error, unchanged|

## 10. Documentation deliverables and amendments

|File|Change|
|---|---|
|`crates/buildl-core/src/lib.rs`, `crates/buildl-core/README.md`|the module map gains `resolve`; the status paragraph names Resolve as implemented|
|`docs/architecture.md` §2|the `TargetGraph` class in the Mermaid diagram gains `aliases` and `settings`, and its `nodes` become `Vec~Node~`, with a `Node` class|
|`docs/architecture.md` §3.2|`nodes: Vec<Node>`; the serialization sentence says the graph serializes keyed by label through a hand-written impl, with no `NodeId` in the file|
|`docs/architecture.md` §8|the settings question records that `$opt:name` is checked at Resolve and `--set` validation stays open|
|`docs/design.md` §5|an alias names a target; a list field keeps its written order with repeats dropped, so `$deps` and `$in` expand in that order|
|`docs/design.md` §12.1 item 2|the order build files issue their declarations in never changes the graph; the order inside one list is part of the command and of the action key, and a list whose order varies between evaluations fails `check`|
|`docs/design.md` §8.2|the dependency count is the length of a node's edge list; the counters belong to the scheduler|
|`docs/architecture-building-blocks.md` §7|the rules table gains the module-edge guard|
|`docs/roadmap.md`|the I4 row and §4 on completion; follow-up #11 closed. Grants and follow-up #13 already sit under I5 (`docs/roadmap.md:70-71`, `:103`), edited when this chain's intent was written|
|`README.md`|no change: its only passage on dependencies, aliases and placeholders (`README.md:60-68`) states no list order and no alias rule|

Mermaid stays Mermaid, and every section cross-reference survives.

## 11. Definition of Done

- `cargo make dod` passes, including `guard-core-purity`, `guard-crate-edges` and
  `guard-module-edges`.
- `cargo deny check` passes; `crates/buildl-core/Cargo.toml`'s dependencies are unchanged.
- Adding `use crate::load::load;` to a file under `src/resolve/` turns `cargo make clippy` red.
- Every row of §9 has a passing test.
- The documents of §10 are amended.

## 12. Non-goals

- The wanted set, the ceiling intersection, grants, the `Manifest` and `Approver` ports — I5 Plan.
- Roadmap follow-up #13 — I5, with the `Manifest` port. Follow-up #5 — the CLI slice.
- The action key, `KeyComponents`, dirtiness, tool resolution, a topological order — I5.
- Placeholder expansion, and any check of `$in`, `$out`, `$deps` or an unrecognised `$name` — I6.
- `--set` values reaching the pipeline, and checking them against declared settings.
- Selecting the subgraph a requested target needs, and demand-driven loading (design §11.1).
- `query` and `rdeps` — milestone 2.
- Writing `.buildl/graph.json` to disk, the `--dot` rendering, and argument parsing — the `buildl`
  adapters and the CLI slice.
- Alias chains, reporting more than one failure, and reading a graph back from JSON.
- Parallel resolve.
- Reshaping `Declaration` (it gains one method, `into_parts`, §8), and any change to the
  `DeclarationSource` contract, the `Ports` bundle or `buildl-lua`.
