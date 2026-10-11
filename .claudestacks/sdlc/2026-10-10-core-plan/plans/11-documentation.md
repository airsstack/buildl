---
status: done
created: 2026-10-11
depends-on: [10]
---

# Documentation Implementation Plan

**Goal:** The crate docs, the design documents and the roadmap describe what `buildl-core` holds once Plan has landed.

**Architecture:** No code changes beyond doc comments. Six groups of edits, one commit each: the crate's own docs, the design, the architecture, the building blocks, the two files that restate the design's key rule, and the roadmap. Each edit either replaces text a decision of this chain contradicts — above all the rule that a key folds its dependencies' keys, which contradicts early cutoff — or records what was built. `docs/fundamental-walkthrough.md` is not edited: it states early cutoff and never that a key holds a dependency's key. `Grants`, the `grants` logic row and `Approver` stay as written in the building blocks: they describe milestone 4. Mermaid diagrams stay Mermaid, and every section cross-reference survives.

**Tech Stack:** Markdown and Mermaid for the documents; rustdoc for the crate doc, checked by `cargo-make`'s gate. No code change beyond doc comments.

**Content authority:** spec §10 (documentation deliverables and amendments), §11 (the follow-ups the roadmap carries).

**Checkpoints:** review once, when all six tasks are done.

---

## Context an implementer needs

What plans `01` to `10` leave that this plan describes:

| Item | Where |
|---|---|
| `plan` (a public function), `Pipeline::plan`, `Plan`, `Verdict`, `ActionKey`, `CacheRow`, `Ceiling`, `WantedSet`, `Want`, `Workspace`, `PortSet`, six new ports | `crates/buildl-core/src/` |
| The rule each decision settled | spec §2: D1 (a key holds each dependency's outputs; three verdicts), D2 (a build file is not a key input), D3 (declared inputs are checked against `fs.read`), D5 (`always` is not a key component; `env` is keyed by name), D7 (a cache row holds the whole key), D10 (`Host`), D13 (`PortSet`), D14 (an override is checked at Plan), D18 (`Grants` stays as documented) |

Facts that bite here:

- A `Replace` block below gives the exact text to find, which may be part of a longer line; the
  text outside the block stays as it is. Every block was checked to occur exactly once in its
  file.
- A doc comment in `lib.rs` that links an item fails the gate's rustdoc step when the link does
  not resolve; task 1 links only items plans `01` to `10` created.
- The documents use Mermaid on purpose. Six edits change text inside a `mermaid` block, one of
  them adding a class; none converts a diagram. The number of `mermaid` blocks in each document
  is the same before and after: 13 in `docs/design.md`, 3 in `docs/architecture.md`, 7 in
  `docs/architecture-building-blocks.md`, 3 in `README.md`.
- Task 6 replaces follow-up #13's row as this chain's intent step left it in `docs/roadmap.md`.
  That edit must be committed, with the chain's intent, spec and plans, before this plan runs.
- `CLAUDE.md` is outside the intent's list of affected systems. It is edited because it is the
  one other file that states the rule D1 replaces, and the author approved that edit when
  approving the spec.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/lib.rs          — [modify] the module table and the status paragraph of the crate doc
crates/buildl-core/README.md           — [modify] four table rows, the Errors row, the status
docs/design.md                         — [modify] §5, §6, §7, §8.3, §12.1, §12.4
docs/architecture.md                   — [modify] §2, §3, §3.3, §3.5, §8
docs/architecture-building-blocks.md   — [modify] §4, §5.1, §5.2, §5.3, §5.4, §6, §8
README.md                              — [modify] the status line, the action-key diagram and its paragraph
CLAUDE.md                              — [modify] the incrementality paragraph
docs/roadmap.md                        — [modify] the I5 row, the completed-intents row, the follow-ups
```

### Task 1 — Describe Plan in the crate docs

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/README.md`

**Steps:**

1. Add the module to the table in the crate doc of `crates/buildl-core/src/lib.rs`. Replace:

   ```rust
   //! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph |
   ```

   With:

   ```rust
   //! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph |
   //! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
   ```

2. In the same table, name the new values in the `types` row. Replace:

   ```rust
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph — and the as-written values a build file stages |
   ```

   With:

   ```rust
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan — and the as-written values a build file stages |
   ```

3. And the set of ports in the `ports` row. Replace:

   ```rust
   //! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each |
   ```

   With:

   ```rust
   //! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each, with the set that holds one instance of each |
   ```

4. Update the end of the status paragraph. Replace:

   ```rust
   //! wrong kind of thing, and a dependency cycle; [`Pipeline::graph`] runs both phases. Plan,
   //! Execute and Record are not implemented yet.
   ```

   With:

   ```rust
   //! wrong kind of thing, and a dependency cycle; [`Pipeline::graph`] runs both phases. Plan is
   //! implemented: [`plan`](fn@plan) refuses a build that wants more than its workspace's
   //! [`Ceiling`] allows, gives every target an [`ActionKey`] from what the ports report, and
   //! compares it with the key the target's last build recorded, so each target is clean, dirty
   //! with a reason, or conditional on a dependency's output; [`Pipeline::plan`] runs all three
   //! phases. Execute and Record are not implemented yet.
   ```

5. Add four rows to the table in `crates/buildl-core/README.md`, after the `Resolve` row. Replace:

   ```markdown
   | Errors | one structured enum
   ```

   With:

   ```markdown
   | Keys | `ActionKey` — the SHA-256 of a target's `KeyComponents`: its command, input digests, output names, environment values, tool fingerprint, settings, network access and platform, with what each dependency produced — and `CacheRow`, what the last build of a target recorded |
   | Authority | `Ceiling` — what a workspace allows — `WantedSet` and `Want` — what a build asks for — and `Workspace`, what the manifest says |
   | Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`, held with `DeclarationSource` in one `PortSet` |
   | Plan | `plan` refuses a build whose wants exceed the ceiling, keys every target, and returns a `Plan`: a `Verdict` per target — clean, dirty with the components that differ, or conditional on a dependency's output — and the wanted set; `Pipeline::plan` runs Load once, Resolve, then Plan |
   | Errors | one structured enum
   ```

6. Extend the `Errors` row. Replace:

   ```markdown
   Resolve's name every declaration involved |
   ```

   With:

   ```markdown
   Resolve's name every declaration involved; Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure |
   ```

7. Update the status. Replace:

   ```markdown
   **Status:** pre-release. Load and Resolve, the first two pipeline phases, are implemented and
   tested against in-memory ports; Plan, Execute and Record are not implemented yet.
   ```

   With:

   ```markdown
   **Status:** pre-release. Load, Resolve and Plan, the first three pipeline phases, are
   implemented and tested against in-memory ports; Execute and Record are not implemented yet.
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

9. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/README.md
   $ git commit -m "docs(buildl-core): describe the Plan phase in the crate docs"
   ```

### Task 2 — State in the design what a key holds

**Files:**
- Modify `docs/design.md`

**Steps:**

1. In §5 of `docs/design.md`, except `always` from the key components. Replace:

   ```markdown
   and `always = true` for phony targets. Every field is an action-key component; the surface grows reluctantly.
   ```

   With:

   ```markdown
   and `always = true` for phony targets. Every field but `always` is an action-key component; the surface grows reluctantly.
   ```

2. In the last bullet of §5, say which lists' order reaches the key. Replace:

   ```markdown
   so `$deps` and `$in` expand in that order. Reordering a list changes the command, and with it the action key.
   ```

   With:

   ```markdown
   so `$deps` and `$in` expand in that order. Reordering `deps`, `inputs` or `outputs` changes the action key. Reordering `env` does not: an environment is a set of variables, and the key holds each one by name.
   ```

3. In §6, add declared inputs to what a build wants, and say what the wanted set a plan reports holds. Replace:

   ```markdown
   computes the _wanted set_ — every program in every `run`, every env name, every output root. That set flows
   ```

   With:

   ```markdown
   computes what the build wants — every program in every `run`, every env name, every declared input, every output root — and checks each against its list in the ceiling: `proc.run`, `env.read`, `fs.read`, `fs.write`. The _wanted set_ a plan reports lists the programs, the env names, the output roots and the targets declaring `network = true`; the inputs stay in the graph, where each target declares them. That set flows
   ```

4. In §7 rule 2, say how a build file reaches a key. Replace:

   ```markdown
   Build files are themselves inputs whose hashes invalidate their own targets, ninja's discipline [4].
   ```

   With:

   ```markdown
   A build file reaches a key through what it declares, not through its own hash: every field of a target that changes what its action produces is a key component (§8.3), so an edit that changes a target rebuilds it and an edit that changes nothing rebuilds nothing.
   ```

5. In the §8.3 diagram, name what a key folds for a dependency, and add the third verdict. The diagram stays Mermaid. Replace:

   ````markdown
       DEP["Dep action keys<br/>propagation built in"] --> KEY
       ENV["Env values<br/>granted names only"] --> KEY
       KEY --> CMP{"equals cached key?"}
       CMP -- yes --> CLEAN["Clean — served from cache"]
       CMP -- no --> DIRTY["Dirty — plus the reason why"]
   ```

   Because dep _keys_ fold into the hash, dirtiness propagates downstream with no separate marking pass. Two internals
   ````

   With:

   ````markdown
       DEP["Dep output hashes<br/>early cutoff built in"] --> KEY
       ENV["Env values<br/>granted names only"] --> KEY
       KEY --> CMP{"equals cached key?"}
       CMP -- yes --> CLEAN["Clean — served from cache"]
       CMP -- no --> DIRTY["Dirty — plus the reason why"]
       CMP -- "a dep is not clean" --> COND["Conditional — runs only if<br/>that dep's output changes"]
   ```

   A key folds what each dependency _produced_ — the name and hash of each of its outputs — not the dependency's own key, so a dependency rebuilt to the same bytes leaves everything downstream clean (early cutoff, §8.5). A dirty dependency's new output does not exist at plan time, so a target whose only open question is that output is neither clean nor dirty but _conditional_: it runs only if the dependency produces something other than what the last build recorded. Two internals
   ````

6. In §12.1 item 2, say which lists' order reaches the key. Replace:

   ```markdown
   The order inside one list is different: it is part of the command and of the action key, and a list whose order varies between two evaluations fails `buildl check`.
   ```

   With:

   ```markdown
   The order inside one list is different: it reaches the graph as written and, for `deps`, `inputs` and `outputs`, the action key (§5), and a list whose order varies between two evaluations fails `buildl check`.
   ```

7. In the §12.4 ledger, name what folds in for a dependency. Replace:

   ```markdown
   |Products of other targets|`deps` — their action keys fold in|
   ```

   With:

   ```markdown
   |Products of other targets|`deps` — the hashes of their outputs fold in (§8.3)|
   ```

8. In the ledger's last row, say how a build file reaches a key. Replace:

   ```markdown
   |Build-file logic itself|build files are inputs; declaration is sandboxed, `os`-free, and double-run checkable (§12.1)|
   ```

   With:

   ```markdown
   |Build-file logic itself|a build file reaches a key through the fields it declares, not through its own hash (§7); declaration is sandboxed, `os`-free, and double-run checkable (§12.1)|
   ```

9. Confirm the diagram and the ledger name outputs, and that the document still holds its thirteen diagrams:

   ```
   $ grep -c -e 'Dep output hashes' -e 'the hashes of their outputs fold in' docs/design.md
   2
   $ grep -c '```mermaid' docs/design.md
   13
   ```

10. Commit:

   ```
   $ git add docs/design.md
   $ git commit -m "docs(repo): state in the design what an action key holds"
   ```

### Task 3 — Record the plan's shape in the architecture

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. In the §2 class diagram of `docs/architecture.md`, give `Plan` its real fields and its verdict type. Replace:

   ```markdown
       class Plan {
           dirty: Vec~NodeId~
           reasons: BTreeMap~NodeId_DirtyReason~
           wanted: WantedSet
       }
   ```

   With:

   ```markdown
       class Plan {
           labels: Vec~Label~
           verdicts: Vec~Verdict~
           wanted: WantedSet
       }
       class Verdict {
           <<enumeration>>
           Clean
           Dirty
           Conditional
       }
   ```

2. In the same diagram, route the key through the verdict. Replace:

   ```markdown
       Plan --> ActionKey
   ```

   With:

   ```markdown
       Plan --> Verdict
       Verdict --> ActionKey
   ```

3. In the decisions below the diagram, list what the components hold. Replace:

   ```markdown
   (command, input digests, dep keys, env values, tool fingerprints, settings, os/arch — the §12.4 ledger as a Rust struct)
   ```

   With:

   ```markdown
   (command, input digests, each dependency's output digests, env values, the tool's fingerprint, settings, network access, os/arch — the §12.4 ledger as a Rust struct)
   ```

4. In the §3 table, give `plan` its logic and its ports. Replace:

   ```markdown
   |§3.3 `plan`|key assembly, dirty set, ceiling check|`Digester`, `StatCache`, `ToolResolver`, `ActionCache`, `Manifest`|`buildl`|
   ```

   With:

   ```markdown
   |§3.3 `plan`|ceiling check, key assembly, a verdict per target|`Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ActionCache`|`buildl`|
   ```

5. Rewrite the first paragraph of §3.3. Replace:

   ```markdown
   Bottom-up over a toposort. Per node: assemble `KeyComponents`, hash inputs through the stat cache, fold dep keys (already computed — toposort guarantees it), digest the canonical serialization of the components.
   ```

   With:

   ```markdown
   The ceiling check comes first: every program, env name, declared input and output root is compared with the manifest's ceiling, and nothing is hashed or looked up until it passes. Then, once per graph: one fingerprint per distinct program (`ToolResolver`), one digest per distinct input (`Digester`, through the stat cache), the platform and one value per env name (`Host`). Each node's own components are assembled from those.

   Verdicts are decided bottom-up over a topological order, computed with a queue rather than by recursion. A node whose dependencies are all clean has a finished key — its own components plus what each dependency's cache row says it produced — digested from the canonical serialization of the components and compared with the key in the node's own row: equal is `Clean`, different is `Dirty` with the components that differ. A node with a dependency that is not clean cannot finish its key, because that dependency's output does not exist yet: it is `Dirty` when something already known differs and `Conditional` otherwise, running only if the dependency produces something other than what was recorded. Its unfinished key holds everything the ports supplied, so the executor finishes it from the graph and its dependencies' new rows.
   ```

6. In the stat-cache paragraph of §3.3, say what an entry holds and who decides reuse. Replace:

   ```markdown
   The stat cache is a `BTreeMap<PathBuf, StatEntry { mtime, size, digest }>` loaded from `.buildl/statcache.json`; a hit on (mtime, size) reuses the digest, a miss re-hashes and updates — ninja's mtime discipline with content-hash truth underneath [3].
   ```

   With:

   ```markdown
   The stat cache holds, per input path, a `StatEntry { stat: FileStat { modified, size }, digest }`, loaded by its adapter from `.buildl/statcache.json`; an entry whose `FileStat` equals the file's current one reuses the digest, anything else re-hashes and records — ninja's mtime discipline with content-hash truth underneath [3]. The reuse rule is Plan's, tested against fakes, and the adapter only stores; Plan commits the cache once when it recorded something and not at all otherwise.
   ```

7. In the last paragraph of §3.3, name the output. Replace:

   ```markdown
   Output: `Plan { dirty, reasons, wanted }` — and the ceiling check runs here, so a `Plan` that would exceed the ceiling is an `Err`, not a plan.
   ```

   With:

   ```markdown
   Output: a `Plan` — one `Verdict` per node, and the `WantedSet` — and a build that would exceed the ceiling is an `Err`, not a plan.
   ```

8. In §3.5, say what a cache row keeps. Replace:

   ```markdown
   `BTreeMap<Label, CacheRow { key: Digest, outputs: Vec<Digest>, stdout, stderr }>` serialized as sorted-key JSON;
   ```

   With:

   ```markdown
   `BTreeMap<Label, CacheRow { key: ActionKey, outputs: Vec<Digest>, stdout, stderr }>` — the whole key, components included, so the next plan can name the component that changed — serialized as sorted-key JSON;
   ```

9. In §8, close the question about settings. Replace:

   ```markdown
   - Where settings (`--set`) validation lives: `manifest` (they're workspace surface) vs `settings` (they're invocation surface) — leaning `settings`, validated against declarations at Resolve. Settled so far: a `$opt:name` reference in a command is checked against the declared settings at Resolve. Where `--set` values are validated stays open.
   ```

   With:

   ```markdown
   - Settled: where settings validation lives. A `$opt:name` reference in a command is checked against the declared settings at Resolve; a `--set` override is checked against them at Plan, the first phase an invocation value reaches, and one naming no declared setting fails the plan.
   ```

10. Confirm the three edits to the diagram landed:

   ```
   $ grep -n -e 'class Verdict' -e 'Plan --> Verdict' -e 'Verdict --> ActionKey' docs/architecture.md
   128:    class Verdict {
   150:    Plan --> Verdict
   151:    Verdict --> ActionKey
   ```

11. Commit:

   ```
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): record the plan's shape and its verdicts in the architecture"
   ```

### Task 4 — Add the ports Plan reads to the building blocks

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. In the §4 diagram of `docs/architecture-building-blocks.md`, add `Host` to the ports box. Replace:

   ```markdown
   PORTS["ports — traits<br/>DeclarationSource, Manifest<br/>Digester, StatCache, ToolResolver<br/>
   ```

   With:

   ```markdown
   PORTS["ports — traits<br/>DeclarationSource, Manifest, Host<br/>Digester, StatCache, ToolResolver<br/>
   ```

2. In the §5.1 table, add the two authority values. `Grants` stays: it describes milestone 4. Replace:

   ```markdown
   |Authority|`Ceiling`, `WantedSet`, `Grants`|
   ```

   With:

   ```markdown
   |Authority|`Ceiling`, `WantedSet`, `Want`, `Workspace`, `Grants`|
   ```

3. In §5.2, add the five ports Plan reads beside the cache, after `DeclarationSource`. Replace:

   ```markdown
   pub trait DeclarationSource {
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
   }
   ```

   With:

   ```markdown
   pub trait DeclarationSource {
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
   }

   /// What the workspace's manifest says: its output and state directories, its ceiling.
   pub trait Manifest {
       fn workspace(&self) -> Result<Workspace>;
   }

   /// The machine actions run on. `Ok(None)` is an unset variable.
   pub trait Host {
       fn platform(&self) -> Platform;
       fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>;
   }

   /// Metadata is asked of every input; digests only of those whose metadata changed,
   /// all in one call so an adapter may hash in parallel.
   pub trait Digester {
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>>;
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
   ```

4. In §5.3, name the ports the bundle now carries, and show the set a pipeline is built from. Replace:

   ```markdown
   pub trait Ports {
       type Source: DeclarationSource;
       type Cas: ContentStore;
       type Cache: ActionCache;
       type Log: EventLog;
       type Strategy: ExecStrategy;
       type Clock: Clock;
       // Manifest, Digester, StatCache, ToolResolver, Dispatcher, Approver, Reporter
   }

   pub struct Pipeline<P: Ports> { /* one field per port */ }
   ```

   With:

   ```markdown
   pub trait Ports {
       type Source: DeclarationSource;
       type Manifest: Manifest;
       type Host: Host;
       type Digester: Digester;
       type StatCache: StatCache;
       type Tools: ToolResolver;
       type Cache: ActionCache;
       type Cas: ContentStore;
       type Log: EventLog;
       type Strategy: ExecStrategy;
       type Clock: Clock;
       // Dispatcher, Approver, Reporter
   }

   /// One instance of each port the bundle names, with a public field per port.
   pub struct PortSet<P: Ports> { pub source: P::Source, pub manifest: P::Manifest, /* ... */ }

   pub struct Pipeline<P: Ports> { /* ports: PortSet<P> */ }
   ```

5. In the §5.3 diagram, list the same ports. The diagram stays Mermaid. Replace:

   ```markdown
           Source: DeclarationSource
           Cas: ContentStore
           Cache: ActionCache
   ```

   With:

   ```markdown
           Source: DeclarationSource
           Manifest: Manifest
           Host: Host
           Digester: Digester
           StatCache: StatCache
           Tools: ToolResolver
           Cas: ContentStore
           Cache: ActionCache
   ```

6. In the §5.4 table, give `plan` its input, its output and its ports. Replace:

   ```markdown
   |plan|`TargetGraph` → `Plan` (key assembly, dirty reasons)|`Digester`, `StatCache`, `ToolResolver`, `ActionCache`|
   ```

   With:

   ```markdown
   |plan|`TargetGraph` and setting overrides → `Plan` (ceiling check, key assembly, a verdict per target)|`Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ActionCache`|
   ```

7. In the §6 catalog, say what Plan reads from the manifest, and add `Host`. Replace:

   ```markdown
   |`Manifest`|ceiling, declaration limits, settings|`buildl.toml` reader|`buildl`|
   ```

   With:

   ```markdown
   |`Manifest`|the ceiling and the workspace's output and state directories; declaration limits, settings|`buildl.toml` reader|`buildl`|
   |`Host`|the platform in every key, and the values of the env names targets read|the running machine's OS, architecture and environment|`buildl`|
   ```

8. In the §8 fakes table, add the fakes of the ports Plan reads. Replace:

   ```markdown
   |`DeclarationSource`|a fixed staged file, absence or evaluation failure per directory|airsl, Lua|
   ```

   With:

   ```markdown
   |`DeclarationSource`|a fixed staged file, absence or evaluation failure per directory|airsl, Lua|
   |`Manifest`|one fixed workspace and ceiling|a `buildl.toml`|
   |`Host`|a fixed platform and fixed variables, some of which it can be told to fail on|the real environment|
   |`Digester`|files held as bytes with their metadata, counting every call|a filesystem|
   |`StatCache`|in-memory, counting commits|a filesystem|
   |`ToolResolver`|fixed fingerprints, recording every lookup|a `PATH`|
   ```

9. Confirm the bundle, the set and the diagram name the host:

   ```
   $ grep -n -e 'type Host: Host' -e 'pub struct PortSet' -e 'Host: Host$' docs/architecture-building-blocks.md
   235:    type Host: Host;
   248:pub struct PortSet<P: Ports> { pub source: P::Source, pub manifest: P::Manifest, /* ... */ }
   269:        Host: Host
   ```

10. Confirm the lines that describe milestone 4 are untouched. The count of lines naming `` `Grants` `` or `Approver` is what it was before this task:

   ```
   $ grep -c -e '`Grants`' -e 'Approver' docs/architecture-building-blocks.md
   5
   ```

11. Commit:

   ```
   $ git add docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): add the ports Plan reads to the building blocks"
   ```

### Task 5 — Restate the key rule where it is repeated

**Files:**
- Modify `README.md`
- Modify `CLAUDE.md`

**Steps:**

1. In the status line at the top of `README.md`, name Plan as implemented. Replace:

   ```markdown
   the `Clock` and `DeclarationSource` ports, and the first two pipeline phases — Load and Resolve, with `check` and `graph`, whose flows run against in-memory fakes; Plan, Execute and Record are not implemented yet.
   ```

   With:

   ```markdown
   the `Clock` and `DeclarationSource` ports with the six ports Plan reads, and the first three pipeline phases — Load, Resolve and Plan, with `check`, `graph` and `plan`, whose flows run against in-memory fakes; Execute and Record are not implemented yet.
   ```

2. In the action-key diagram, name what a key folds for a dependency. The diagram stays Mermaid. Replace:

   ```markdown
       DEP["Dep action keys<br/>propagation built in"] --> KEY
   ```

   With:

   ```markdown
       DEP["Dep output hashes<br/>early cutoff built in"] --> KEY
   ```

3. In the paragraph below the diagram, state the same rule as design §8.3. Replace:

   ```markdown
   Because dep *keys* fold into the hash, dirtiness propagates downstream with no separate marking pass, and the mismatching component is retained per dirty target
   ```

   With:

   ```markdown
   A key folds what each dependency *produced*, not the dependency's own key, so a dependency rebuilt to the same bytes leaves everything downstream clean, and a target downstream of a dirty one is *conditional*: it runs only if that output changes. The mismatching component is retained per dirty target
   ```

4. In `CLAUDE.md`, state the same rule. The author approved this edit when approving the spec. Replace:

   ```markdown
   the action key — SHA-256 over command, input hashes, dep action keys, env values, tool fingerprints, settings, os/arch. Dep *keys* fold into the hash so dirtiness propagates with no marking pass; `KeyComponents` is kept
   ```

   With:

   ```markdown
   the action key — SHA-256 over command, input hashes, each dependency's output hashes, env values, tool fingerprints, settings, os/arch. A key folds what a dependency *produced*, not the dependency's key, so a dependency rebuilt to the same bytes leaves downstream targets clean (early cutoff), and a target downstream of a dirty one is *conditional* until that output is known; `KeyComponents` is kept
   ```

5. Confirm no document still says a key folds a dependency's key. `grep` prints nothing and exits `1` when nothing matches, which is the expected result:

   ```
   $ grep -rn -i -e 'dep action keys' -e 'dep _keys_' -e 'dep \*keys\*' -e 'dep keys' -e 'their action keys' docs README.md CLAUDE.md
   ```

6. Confirm the search can find something, with the phrase the edits introduced:

   ```
   $ grep -c -i 'early cutoff' docs/design.md README.md CLAUDE.md
   docs/design.md:3
   README.md:1
   CLAUDE.md:1
   ```

7. Confirm no diagram was lost or converted:

   ````
   $ grep -c '```mermaid' docs/design.md docs/architecture.md docs/architecture-building-blocks.md README.md
   docs/design.md:13
   docs/architecture.md:3
   docs/architecture-building-blocks.md:7
   README.md:3
   ````

8. Commit:

   ```
   $ git add README.md CLAUDE.md
   $ git commit -m "docs(repo): restate the action-key rule in the README and CLAUDE.md"
   ```

### Task 6 — Mark I5 Plan done in the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. In the §3 table of `docs/roadmap.md`, replace the last cell of the **I5 Plan** row — everything after ``|the pipeline stops at `plan`|`` — with `**done**, <date> (§4)|`, where `<date>` is what this command prints:

   ```
   $ date +%Y-%m-%d
   ```

2. In §4, add the chain's row directly below the I4 Resolve row. In the last cell, write the two hashes printed by the commands below in place of `FIRST` and `LAST`: the commit of plan `01`'s first task, and the latest commit.

   ```markdown
   |I5 Plan|`.claudestacks/sdlc/2026-10-10-core-plan/`|`01-small-values` (6 tasks), `02-action-key` (2 tasks), `03-authority-types` (1 task), `04-plan-types` (1 task), `05-plan-errors` (3 tasks), `06-ports` (5 tasks), `07-plan-checks` (3 tasks), `08-plan-gather` (2 tasks), `09-plan-verdicts` (3 tasks), `10-pipeline-plan` (3 tasks), `11-documentation` (6 tasks)|`FIRST` … `LAST`|
   ```

   ```
   $ git log --reverse --format=%h --grep='name the program, env value and platform kinds' | head -n 1
   $ git log -n 1 --format=%h
   ```

3. In §5, bring follow-up #13 up to date and add the four follow-ups this chain hands on. Replace:

   ```markdown
   |13|Exclude the workspace's `out` and `.buildl` directories from `b.sources()`. It walks the declaring directory whole, so at the workspace root it also lists build outputs; which directories to skip is known only once `buildl.toml` is parsed. Split in the I5 intent: I5 makes the workspace's output directories known to `buildl-core` through the `Manifest` port; the change to `buildl-lua`'s walk follows where a manifest is first read.|I-lua spec §6.2|I5 Plan (the `Manifest` port), then the `buildl` adapters slice (the walk)|
   ```

   With:

   ```markdown
   |13|Exclude the workspace's `out` and `.buildl` directories from `b.sources()`. It walks the declaring directory whole, so at the workspace root it also lists build outputs. I5 made both directories known to `buildl-core`: `Manifest::workspace` returns them as `Workspace::out` and `Workspace::state`. What remains is `buildl-lua`'s walk skipping them, where a manifest is first read.|I-lua spec §6.2|the `buildl` adapters slice|
   |14|A command whose first argument holds a placeholder names no program at plan time, so it is neither checked against `proc.run` nor fingerprinted. Check it once the executor has expanded it.|I5 spec §2 (D6)|I6 Execute + Record|
   |15|Confirm a clean target's outputs are present in the cas before they are linked into `out/`. A plan trusts the cache row.|I5 spec §11|I6 Execute + Record|
   |16|`ActionCache` gains `upsert` and `commit`, and `CacheRow` gains `stdout` and `stderr`. I5 landed `row`, and a row holding the key and the output digests.|I5 spec §2 (D12)|I6 Execute + Record|
   |17|Read a cache row back: `Deserialize` for `ActionKey` and the types it is made of, which I5 left `Serialize`-only.|I5 spec §3.2|the `buildl` adapters slice|
   ```

4. Confirm the four new follow-ups are there, that #13 no longer waits on I5, and that §4 has the chain's row. `grep -c` prints `0` and exits `1` when nothing matches, which is the expected result of the second command:

   ```
   $ grep -c -e '^|14|' -e '^|15|' -e '^|16|' -e '^|17|' docs/roadmap.md
   4
   ```
   ```
   $ grep -c -e 'I5 Plan (the `Manifest` port)' docs/roadmap.md
   0
   ```
   ```
   $ grep -c -e '^|I5 Plan|' docs/roadmap.md
   1
   ```

5. Commit:

   ```
   $ git add docs/roadmap.md
   $ git commit -m "docs(repo): mark I5 Plan done in the roadmap"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`: the crate doc's links resolve.
- No document says a key folds a dependency's key; the design, the README and `CLAUDE.md` say it
  folds what each dependency produced, and that a target downstream of a dirty one is conditional.
- The design says `always` is not a key component, that reordering `env` does not change a key,
  that declared inputs are checked against `fs.read`, and that a build file reaches a key through
  what it declares.
- The architecture gives `Plan` its verdicts, `plan` its six ports, a cache row its whole key,
  and closes the question of where an override is checked.
- The building blocks list `Host` and `PortSet`, and still describe `Grants` and `Approver`.
- The roadmap shows I5 Plan done and carries follow-ups #13 to #17.
- Every `mermaid` block is still a `mermaid` block.

## Review findings

One reviewer pass over the plan's whole diff, 2026-10-11. Verdict: compliant with the plan and the spec, blocking set empty; all 37 replacements are in their files as the plan gives them, and no document contradicts the key rule, the verdicts, where overrides and inputs are checked, the six ports, `PortSet` or the cache row. The reviewer's own gate run: `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 5.31 seconds.`), `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`).

- doc accuracy (risk) — "gives every target an [`ActionKey`]" overstates: a target downstream of a dependency that is not clean gets an unfinished key, its own components and what it awaits, and no `ActionKey`. The same in the crate README's Plan row, "keys every target". Both are the plan's own text — `crates/buildl-core/src/lib.rs`, status paragraph; `crates/buildl-core/README.md`, Plan row
- doc accuracy (nit) — "Plan's name the target and build file behind a refused want": a refused `fs.write` directory has no site — `crates/buildl-core/README.md`, Errors row
- doc accuracy (nit) — the component list leaves out output names and network access, and says "tool fingerprints" where a key holds at most one tool — `CLAUDE.md`, Incrementality
- consistency (nit) — the action-key diagram has no conditional branch, while design §8.3's gained one and the paragraph below it names the verdict — `README.md`
- doc accuracy (nit) — the list of what `ActionKey` keeps leaves out output names — `docs/architecture.md` §2
- doc accuracy (nit) — the verdicts paragraph leaves out the two rules tried first, always-run and never-built, and "compared with the key in the node's own row" presumes a row — `docs/architecture.md` §3.3
- consistency (nit) — the `plan` bullet, "synthetic graphs + a tempdir tree → dirty sets and reasons", now disagrees with §3.3; the plan does not name this bullet — `docs/architecture.md` §7
- consistency (nit) — §5.2's new signatures use `Platform`, `EnvValue`, `Program`, `FileStat` and `StatEntry`, which neither §5.1 nor architecture §2 names — `docs/architecture-building-blocks.md` §5.1
- consistency (nit) — the code block lists `Cache` before `Cas`, the Mermaid class the other way round — `docs/architecture-building-blocks.md` §5.3
- doc accuracy (nit) — the fakes table gives the `ActionCache` fake "injectable failures", and the flows table says `Refusal` where the error built is `Error::Refused`; the plan names neither row — `docs/architecture-building-blocks.md` §8

No finding was fixed: none is in the blocking set.

## Probe results

No throwaway test was written: the plan changes documents and doc comments, and the checks are the plan's own commands, each run on the tree all six tasks leave.

- **No document says a key folds a dependency's key.** `grep -rn -i -e 'dep action keys' -e 'dep _keys_' -e 'dep \*keys\*' -e 'dep keys' -e 'their action keys' docs README.md CLAUDE.md` → no output, exit `1`. Not against the plan.
- **Early cutoff is stated where the plan says.** `grep -c -i 'early cutoff' docs/design.md README.md CLAUDE.md` → `docs/design.md:3`, `README.md:1`, `CLAUDE.md:1`. Not against the plan.
- **Every diagram is still Mermaid.** `grep -c '```mermaid' docs/design.md docs/architecture.md docs/architecture-building-blocks.md README.md` → `13`, `3`, `7`, `3`. Not against the plan.
- **The line numbers the plan quotes hold.** `docs/architecture.md`: `128` `class Verdict`, `150` `Plan --> Verdict`, `151` `Verdict --> ActionKey`. `docs/architecture-building-blocks.md`: `235` `type Host: Host;`, `248` `pub struct PortSet`, `269` `Host: Host`; `Grants` and `Approver` counted `5`. `docs/design.md`: `grep -c -e 'Dep output hashes' -e 'the hashes of their outputs fold in'` → `2`. Not against the plan.
- **The roadmap's checks hold.** `grep -c -e '^|14|' -e '^|15|' -e '^|16|' -e '^|17|' docs/roadmap.md` → `4`; ``grep -c -e 'I5 Plan (the `Manifest` port)' docs/roadmap.md`` → `0`, exit `1`; `grep -c -e '^|I5 Plan|' docs/roadmap.md` → `1`. Not against the plan.
- **The crate doc's links resolve.** The gate's rustdoc step ran with `-D warnings` and passed. Not against the plan.
- **The plan's premise about the roadmap's I5 row did not hold.** Task 6 was written when the row's last cell read as the chain's intent step left it; by the time it ran, accepting plans `01` to `06` had changed that cell to "in progress …". The instruction is stated by position, everything after ``|the pipeline stops at `plan`|``, so it applied unchanged.

## Deviations

- 2026-10-11 — the commit steps were not run. The work was left in the working tree, uncommitted, because committing is the author's act in this run.
- 2026-10-11 — tasks 2 to 6 ran at the same time, one per file, and task 1 after them; the plan lists them in order. Their files are disjoint. Task 5's three cross-file checks were therefore run again once every task had finished; the figures under Probe results are from that run.
- 2026-10-11 — task 6 step 2: the second command, `git log -n 1 --format=%h`, was not run. The chain's last commit does not exist while plans `07` to `11` are uncommitted, so the §4 row of `docs/roadmap.md` reads `` `05dbe53` … `LAST` ``, with the literal `LAST` to be replaced by the hash of the commit that precedes the roadmap's own, when the author commits. `05dbe53` is what the first command prints.
- 2026-10-11 — on the author's acceptance the work of plans `07` to `11` was committed as three commits, not one per task: `d6bf0fa` holds everything under `crates/` (plans `07` to `10` and this plan's task 1), `4f130d5` the design documents, the README and `CLAUDE.md` (tasks 2 to 5), and the commit after it the roadmap with the chain's records. `LAST` in the roadmap's §4 row was replaced by `4f130d5`, the commit that precedes the roadmap's own.
