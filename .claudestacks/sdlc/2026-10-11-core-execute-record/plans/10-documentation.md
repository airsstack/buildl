---
status: approved
created: 2026-10-11
depends-on: [09]
---

# Documentation Implementation Plan

**Goal:** The crate docs, the design documents and the roadmap describe what `buildl-core` holds once Execute and Record have landed.

**Architecture:** No code changes beyond doc comments. Six groups of edits, one commit each: the crate's own docs, the design, the architecture, the building blocks, the README, and the roadmap. Each edit either replaces text a decision of this chain contradicts — a cache written once per build, a `Refusal` outcome class, placeholders expanded to "real paths", a target that can be cached only from the plan — or records what was built. `CLAUDE.md` and `docs/fundamental-walkthrough.md` are not edited: neither repeats an amended sentence. The two design examples whose output is a directory tree (`--outdir $out`, `tar … -C $out`) stay as written and become a roadmap follow-up. `Grants`, the `grants` logic row, `Approver` and `Reporter` stay as written in the building blocks: they describe later work. Mermaid diagrams stay Mermaid, and every section cross-reference survives.

**Tech Stack:** Markdown and Mermaid for the documents; rustdoc for the crate doc, checked by `cargo-make`'s gate. No code change beyond doc comments.

**Content authority:** spec §10 (documentation deliverables and amendments), §11 (the follow-ups the roadmap carries), §2 (the decisions each amendment states).

**Checkpoints:** review once, when all six tasks are done.

---

## Context an implementer needs

What plans `01` to `09` leave that this plan describes:

| Item | Where |
|---|---|
| `execute` (a public function, re-exported from the crate root beside the module of the same name) and `Pipeline::build` | `crates/buildl-core/src/execute/`, `crates/buildl-core/src/pipeline/driver.rs` |
| `OutputPath`, `ReadyAction`, `RawOutcome`, `Exit`, `OnFailure`, `ActionOutcome`, `OutcomeClass`, `InfraKind`, `Summary`, `OutcomeEvent`, and `CacheRow` with `stdout` and `stderr` | `crates/buildl-core/src/types/` |
| `ContentStore`, `EventLog`, `ExecStrategy`, `Dispatcher`, `ActionCache::record`, and a `Ports` bundle of twelve port types | `crates/buildl-core/src/ports/` |

The rule each decision of the spec settled, as the documents must now state it:

| Decision | Rule |
|---|---|
| D1 | A placeholder expands to a workspace-relative path, the same in every isolation tier; a strategy makes those paths true in the directory it runs the action in |
| D2 | `$in`, `$out` and `$deps` are lists of paths: the target's inputs, its own outputs, its dependencies' outputs. An argument that is exactly one of them becomes one argument per path; inside other text the paths are joined by one space |
| D4 | An output lives at `<out>/<directory of the label>/<output name>`. Two targets claiming one path, or one claiming a path another needs as a directory, is an error before anything runs |
| D5 | Every command is expanded before anything runs, and its first argument must be a program in `proc.run`; otherwise the build fails with `Error::Refused`. There is no outcome class for a refusal |
| D6 | A target is cached when it is not declared `always`, its key — finished from its dependencies' current rows — equals its row's key, and every digest the row holds is in the cas. Otherwise it runs |
| D7 | The dispatching port is `dispatch(work, step)`: `work` runs a task on any thread, `step` runs on the calling thread and returns the tasks that became ready |
| D8 | A worker prepares, runs, stores what was printed and stores the outputs. The calling thread places outputs, writes the row, appends the event and releases dependents |
| D9 | A row is written by `ActionCache::record`, one call per target, durable when it returns `Ok` and absent when it returns `Err` |
| D10, D11 | `ContentStore::place` makes a stored output the file at its path; `ExecStrategy::collect` stores what an action left and answers one optional digest per declared output |
| D12, D18 | The outcome classes are `Cached`, `Built`, `ActionFailure`, `OutputMissing`, `InfraFailure` and `Skipped`; `Skipped` names the failed target by `Label` |
| D13 | Under the default policy a failure skips every target not yet begun; a target handed to the dispatcher before the failure comes back not begun |

Facts that bite here:

- Every `Replace` block below is one or more whole lines, quoted exactly; each step names the
  section the lines are in. A paragraph in these documents is one long line, so a block may
  replace a whole paragraph to change one clause of it.
- No other plan of this chain edits `docs/design.md`, `docs/architecture.md`,
  `docs/architecture-building-blocks.md`, `README.md` or `crates/buildl-core/README.md`, so the
  line numbers in the quoted `grep` results hold on any tree that has only this plan's edits to
  that file.
- Other plans edit `crates/buildl-core/src/lib.rs`, but only its `pub mod` and `pub use` lines.
  Task 1 edits only lines of the crate doc, above them.
- A doc comment in `lib.rs` that links an item fails the gate's rustdoc step when the link does
  not resolve. Task 1 links only items that exist once plans `01` to `09` have landed, and
  disambiguates `execute`, which names both a module and a function, as `mod@execute` and
  `fn@execute`.
- The documents use Mermaid on purpose. Four edits change text inside a `mermaid` block; none
  converts a diagram. The number of `mermaid` blocks in each document is the same before and
  after: 13 in `docs/design.md`, 3 in `docs/architecture.md`, 7 in
  `docs/architecture-building-blocks.md`, 3 in `README.md`.
- `CLAUDE.md` is not edited. Its status paragraph points at the roadmap rather than naming what
  is implemented, and its pipeline and crash-safety sentences stay true.
- Task 6 replaces the status cell of the I6 row as this chain's earlier steps left it in
  `docs/roadmap.md`; the instruction is by position, so it holds whatever that cell reads.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every edit below was made for real in a copy of the repository while this plan was written, and each `Replace` and `With` block is quoted from that copy, before and after the edit. Every quoted `grep` result was captured by running that command on that copy with the task's edits made. The gate step of task 1 was checked on a second copy holding the code of plans `01` to `09` with every edit of this plan applied: `cargo make dod` exited `0` there, so each of the crate doc's new links resolves.

## File structure

```
crates/buildl-core/src/lib.rs          — [modify] the module table and the status paragraph of the crate doc
crates/buildl-core/README.md           — [modify] three new table rows, the Errors row, the status
docs/design.md                         — [modify] §3, §5, §8.5, §8.7
docs/architecture.md                   — [modify] §2, §2.1, §3, §3.4, §3.5, §4, §7
docs/architecture-building-blocks.md   — [modify] §5.1, §5.2, §5.3, §5.4, §6, §8
README.md                              — [modify] the status line, the placeholder sentence
docs/roadmap.md                        — [modify] the I6 row, the completed-intents row, the follow-ups
```

### Task 1 — Describe Execute and Record in the crate docs

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/README.md`

**Steps:**

1. In the module table of the crate doc of `crates/buildl-core/src/lib.rs`, name the new values in the `types` row. Replace:

   ```rust
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan — and the as-written values a build file stages |
   ```

   With:

   ```rust
   //! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan, ready actions, outcomes, log events — and the as-written values a build file stages |
   ```

2. In the same table, add the module after the `plan` row. Replace:

   ```rust
   //! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
   ```

   With:

   ```rust
   //! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
   //! | [`execute`](mod@execute) | Execute and Record, the last phase: a plan to one outcome per target, each result recorded as the last step of its action |
   ```

3. Rewrite the last line of the status paragraph. Replace:

   ```rust
   //! phases. Execute and Record are not implemented yet.
   ```

   With:

   ```rust
   //! phases. Execute and Record are implemented: [`execute`](fn@execute) gives every output a
   //! path under the workspace's output directory, expands every command and holds its program to
   //! the [`Ceiling`] before anything runs, then runs the plan through the [`Dispatcher`] port. A
   //! target whose finished key equals the one its last build recorded, with every recorded output
   //! still in the [`ContentStore`], is cached; any other runs through the [`ExecStrategy`] port.
   //! Recording is the last step of each action — its outputs into the content store, then its
   //! [`CacheRow`], then one [`OutcomeEvent`] in the [`EventLog`] — so a failure leaves no row.
   //! [`Pipeline::build`] runs all four phases and returns one [`ActionOutcome`] per target; what
   //! a failure does to the rest of the build is the caller's [`OnFailure`].
   ```

4. Add three rows to the table in `crates/buildl-core/README.md`, after the `Plan` row, and extend the `Errors` row. Replace:

   ```markdown
   | Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved; Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure |
   ```

   With:

   ```markdown
   | Build values | `OutputPath` — where one output lives under the workspace's output directory — `ReadyAction` — one action with its command expanded — `RawOutcome` — how a run ended and what it printed — `ActionOutcome` with its `OutcomeClass`, `Summary`, and `OutcomeEvent`, one line of the build log |
   | Build ports | `ContentStore`, `EventLog`, `ExecStrategy` and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in one `PortSet` |
   | Execute and Record | `execute` gives every output a path, expands every command and holds its program to the ceiling before anything runs, then runs the plan: a target whose key and stored outputs still stand is cached, any other runs, and each result is recorded as the last step of its action — content store, then cache row, then log — so a failure leaves no row; `Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` per target |
   | Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved; Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure; Execute's name the two targets claiming one output path, and the target whose command names no program once expanded |
   ```

5. Update the status. Replace:

   ```markdown
   **Status:** pre-release. Load, Resolve and Plan, the first three pipeline phases, are
   implemented and tested against in-memory ports; Execute and Record are not implemented yet.
   ```

   With:

   ```markdown
   **Status:** pre-release. Every pipeline phase — Load, Resolve, Plan, Execute and Record — is
   implemented and tested against in-memory ports. The adapters that run a real build belong to
   other crates and are not written yet.
   ```

6. Confirm the crate doc names the module and the function, and that neither file still says the phase is missing. The second command prints `0` for each file and exits `1`, which is the expected result:

   ```
   $ grep -c -e 'mod@execute' -e 'fn@execute' crates/buildl-core/src/lib.rs
   2
   ```
   ```
   $ grep -c 'Execute and Record are not implemented yet' crates/buildl-core/src/lib.rs crates/buildl-core/README.md
   crates/buildl-core/src/lib.rs:0
   crates/buildl-core/README.md:0
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. The rustdoc step of `cargo make dod` is the one that checks this task: it fails on a link that does not resolve. If it names one, the item is missing from the crate root's re-exports in `lib.rs`; the ten items the new text links are `execute`, `Ceiling`, `Dispatcher`, `ContentStore`, `ExecStrategy`, `CacheRow`, `OutcomeEvent`, `EventLog`, `ActionOutcome` and `OnFailure`, with `Pipeline::build`.

8. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/README.md
   $ git commit -m "docs(buildl-core): describe Execute and Record in the crate docs"
   ```

### Task 2 — State in the design where an output lives and how a placeholder expands

**Files:**
- Modify `docs/design.md`

**Steps:**

1. In the layout of §3 of `docs/design.md`, say where under `out/` an output lives. Replace:

   ```markdown
     out/                 # named outputs, linked from the cas
   ```

   With:

   ```markdown
     out/                 # named outputs, linked from the cas: out/<directory>/<name>
   ```

2. Still in §3, add a paragraph after the one that begins `All metadata files use airsl's sorted-key JSON encoding`, with a blank line between them:

   ```markdown
   An output lives at `out/<directory>/<name>`: the directory of its target's label, then the output's name, so `//lib:text` writes `out/lib/text` and `//:app` writes `out/app`. Two targets may not claim one path, and no target may claim a path another needs as a directory: either is an error before anything runs.
   ```

3. In the example of §5, run the test target on its dependency's output. `$out` is the target's own output, so the old line named a path that does not exist. Replace:

   ```lua
   b.test("app_test", { deps = { "app" }, run = { "$out/app", "--self-test" } })
   ```

   With:

   ```lua
   b.test("app_test", { deps = { "app" }, run = { "$deps", "--self-test" } })
   ```

4. In the bullets below the example, state what each placeholder expands to. Replace:

   ```markdown
   - **Placeholders expand host-side.** `$in`, `$out`, `$deps` are substituted by the executor at run time, the ninja model [4]. Lua never holds a real artifact path.
   ```

   With:

   ```markdown
   - **Placeholders expand host-side.** `$in`, `$out` and `$deps` are substituted by the executor, the ninja model [4]. Each is a list of paths: the target's inputs, its own outputs, and every output of its dependencies. An argument that is exactly one placeholder becomes one argument per path, and none when the list is empty; inside other text the paths are joined by one space. So `{ "$deps", "--self-test" }` above runs `out/app --self-test`. A placeholder is `$` and the longest run of letters after it: `$input` is plain text, and `$out/app` is the outputs followed by `/app`. Every command is expanded before any action runs. Lua never holds a real artifact path.
   ```

5. In the §8.5 diagram, name the paths a placeholder becomes. The diagram stays Mermaid. Replace:

   ```markdown
       Expand : Expand placeholders — real paths
   ```

   With:

   ```markdown
       Expand : Expand placeholders — workspace-relative paths
   ```

6. Still in §8.5, add a paragraph after the one that begins `The ordering is the consistency story`, with a blank line between them:

   ```markdown
   Placeholders expand to workspace-relative paths — `lib/a.c`, `out/lib/text` — the same in every isolation tier (§12.2), so a command is one text whichever tier runs it. Each tier makes those paths true in the directory it runs the action in.
   ```

7. In the table of §8.7, widen the refusal row, and add a paragraph after the table saying a refusal always comes first. Replace:

   ```markdown
   |Policy refusal|the plan wants more than the ceiling|ungranted program or path|
   ```

   With:

   ```markdown
   |Policy refusal|the build wants more than the ceiling|ungranted program or path, a program a command names only once its placeholders are expanded|

   A policy refusal precedes execution in every case. Every command is expanded, and its program held to `proc.run`, before the first action starts, so a refusal is an error of the whole build and never the result of one action.
   ```

8. Confirm the five additions are there. The count is `6` because the new command of step 3 is quoted once more in the bullet of step 4:

   ```
   $ grep -c -e '"\$deps", "--self-test"' -e 'Expand placeholders — workspace-relative paths' -e '^An output lives at' -e '^Placeholders expand to workspace-relative paths' -e '^A policy refusal precedes execution' docs/design.md
   6
   ```

9. Confirm the old test command is gone, and that the document still holds its thirteen diagrams. The first command prints `0` and exits `1`, which is the expected result:

   ```
   $ grep -c '"\$out/app", "--self-test"' docs/design.md
   0
   ```
   ````
   $ grep -c '```mermaid' docs/design.md
   13
   ````

10. Commit:

   ```
   $ git add docs/design.md
   $ git commit -m "docs(repo): state in the design where an output lives and how placeholders expand"
   ```

### Task 3 — Record a build's states and its outcome classes in the architecture

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. In the §2 class diagram of `docs/architecture.md`, give `ActionOutcome` its real fields. The diagram stays Mermaid. Replace:

   ```markdown
       class ActionOutcome {
           node: NodeId
           class: OutcomeClass
           outputs: Vec~Digest~
           stdout: Digest
           stderr: Digest
       }
   ```

   With:

   ```markdown
       class ActionOutcome {
           label: Label
           class: OutcomeClass
           diagnostic: Option~Diagnostic~
           outputs: Vec~Produced~
           stdout: Option~Digest~
           stderr: Option~Digest~
       }
   ```

2. In the §2.1 state diagram, draw the states a build keeps. Every target starts `Pending`, whatever its verdict, and there is no `Clean` state. The diagram stays Mermaid; its first two lines and its closing fence are unchanged. Replace:

   ```markdown
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

   With:

   ```markdown
       [*] --> Pending : every target, whatever its verdict
       Pending --> Started : dep counter hits zero
       Started --> Succeeded : cached, or built and recorded
       Started --> Failed : action / infra failure
       Started --> Skipped : the build stopped before a worker began it
       Pending --> Skipped : upstream failed, or the build stopped
       Succeeded --> [*]
       Failed --> [*]
       Skipped --> [*]
   ```

3. Rewrite the paragraph below that diagram: a target becomes cached when it is settled, not only from the plan. Replace:

   ```markdown
   `Clean` and `Skipped` are terminal without ever running — the two states a task runner cannot represent, and the reason the summary line can say `1 failed, 2 skipped, 2 built, 9 cached`.
   ```

   With:

   ```markdown
   A started target is _settled_ before it runs: its key is finished from what its dependencies' rows now hold, and when the target is not declared `always`, that key equals the one its last build recorded, and every digest the row holds is still in the cas, it succeeds as cached without running. So a target becomes cached when it is settled, not only when the plan called it clean: a conditional target (§3.3) gets its answer here, and a clean one whose output left the cas runs again. `Succeeded` covers cached and built, and the outcome's class tells them apart (§4). A cached target and a skipped one never run — the two results a task runner cannot represent, and the reason the summary line can say `1 failed, 2 skipped, 2 built, 9 cached`.
   ```

4. In the §3 table, make the `exec` and `store` rows one row for the one phase module. The headings of §3.4 and §3.5 stay. The new row lists the ports `execute` reads, so `Reporter` leaves it: this was the file's only mention of `Reporter`, which no code uses yet and which stays in the building blocks' port catalog. Replace:

   ```markdown
   |§3.4 `exec`|`Schedule` state machine|`Dispatcher`, `ExecStrategy`, `ContentStore`, `Reporter`|`buildl`|
   |§3.5 `store`|record ordering: cas, row, log|`ContentStore`, `ActionCache`, `EventLog`, `Clock`|`buildl`|
   ```

   With:

   ```markdown
   |§3.4 `exec` and §3.5 `store` — one phase module, `execute`|output layout, placeholder expansion, the program check, the `Schedule` state machine, cached or run, record ordering: cas, row, log|`Manifest`, `Dispatcher`, `ExecStrategy`, `ContentStore`, `ActionCache`, `EventLog`, `Clock`|`buildl`|
   ```

5. Rewrite the first paragraph of §3.4 and add one after it: what the dispatching port is, and what runs where. Replace:

   ```markdown
   Two halves. The scheduling logic is `Schedule`, a pure state machine in `buildl-core`: it owns the state array (§2.1) and one dependency counter per node, hands out ready `NodeId`s, and on `complete(node, class)` marks dependents ready or skipped. Its caller is the only writer, so the counters are plain integers — single-writer bookkeeping, so no lock ordering to reason about. Parallelism is the `Dispatcher` port. Its v1 adapter in `buildl` uses plain threads, no async runtime (decision record §6): a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a `NodeId`, run the action through the strategy (below), and send an `ActionOutcome` back on a results channel. Tests drive `Schedule` through an inline dispatcher on one thread.
   ```

   With:

   ```markdown
   Two halves. The scheduling logic is `Schedule`, a pure state machine in `buildl-core`: it owns the state array (§2.1) and one dependency counter per node, hands out the `NodeId`s that became ready, and on `complete(node, finish)` starts dependents or skips them. Its caller is the only writer, so the counters are plain integers — single-writer bookkeeping, so no lock ordering to reason about. Parallelism is the `Dispatcher` port, one method: `dispatch(work, step)`. `work` runs one task and may run on any thread, several at once; `step` runs on the calling thread, first with nothing and then with each result, and returns the tasks that became ready. The port is generic over the task and the result, so it knows nothing of builds. Its v1 adapter in `buildl` uses plain threads, no async runtime (decision record §6): a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a task, run it through `work`, and send the result back on a results channel. Tests drive the same two closures through an inline dispatcher on one thread.

   What runs where follows the record ordering (§3.5). A worker prepares the exec dir, runs the action through the strategy (below), stores what the action printed, and stores its outputs in the cas. The calling thread links the outputs into `out/`, writes the cache row, appends the log event and releases dependents. Stopping after a failure is a flag `step` sets and `work` reads before it begins an action: the dispatcher runs every task it was given, so a task handed over before the failure comes back not begun, and its target is skipped.
   ```

6. In the trait of §3.4, add `collect`. Replace:

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

   With:

   ```rust
   pub trait ExecStrategy: Send + Sync {
       /// The materialised exec dir; each isolation tier has its own shape.
       type Dir;
       /// Materialise the action's exec dir (inputs visible per this strategy's tier).
       fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
       /// Spawn and wait, capturing output. No shell, ever.
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
       /// Store what the action left at each declared output: one entry per output,
       /// `None` where nothing is there.
       fn collect<C: ContentStore>(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           cas: &C,
       ) -> Result<Vec<Option<Digest>>>;
   }
   ```

7. In the paragraph below the trait, say what a strategy is handed and what it returns. Replace:

   ```markdown
   The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace cwd, granted env only) and `InputSandbox` (tier 2: temp dir, symlinked declared inputs). `OsSandbox` and `Container` are later impls of the same trait — the trait is in v1 so the ladder is a seam, not a refactor.
   ```

   With:

   ```markdown
   The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace cwd, granted env only) and `InputSandbox` (tier 2: temp dir, symlinked declared inputs). `OsSandbox` and `Container` are later impls of the same trait — the trait is in v1 so the ladder is a seam, not a refactor. A `ReadyAction` holds its command already expanded to workspace-relative paths (design doc §8.5), so a strategy only makes those paths true in its exec dir. `collect` is the strategy's because it alone knows that dir, and an output never passes through `buildl-core` as bytes. `run` returns a `RawOutcome`: how the action ended — an exit status, or a timeout — and what it printed.
   ```

8. In §3.5, say when a row is durable. Replace:

   ```markdown
   The cas is `.buildl/cas/ab/cdef...` (two-char shard dirs). Every write is temp-file-then-rename within the same filesystem — atomic, the `fs.atomic_write` discipline [1]. `out/` entries are hard links into the cas (copy fallback for filesystems that refuse). The action cache is a `BTreeMap<Label, CacheRow { key: ActionKey, outputs: Vec<Digest>, stdout, stderr }>` — the whole key, components included, so the next plan can name the component that changed — serialized as sorted-key JSON; rewritten atomically once per build, rows updated in memory as outcomes land — write ordering enforces the §8.5 invariant: **cas first, row after, log always**. `log.jsonl` is append-only, one sorted-key JSON object per outcome, output blobs referenced by digest rather than inlined.
   ```

   With:

   ```markdown
   The cas is `.buildl/cas/ab/cdef...` (two-char shard dirs). Every write is temp-file-then-rename within the same filesystem — atomic, the `fs.atomic_write` discipline [1]. `out/` entries are hard links into the cas (copy fallback for filesystems that refuse). The action cache is a `BTreeMap<Label, CacheRow { key: ActionKey, outputs: Vec<Digest>, stdout, stderr }>` — the whole key, components included, so the next plan can name the component that changed — serialized as sorted-key JSON. A row is written through `ActionCache::record`, one write per target, and is durable when that call returns: a dependent keys on its dependency's row, so the row must be safe before the dependent starts, and a write that fails leaves no row. Write ordering enforces the §8.5 invariant: **cas first, row after, log always**. `log.jsonl` is append-only, one sorted-key JSON object per outcome, output blobs referenced by digest rather than inlined.
   ```

9. In §4, say where a refusal lives. Replace:

   ```markdown
   One `thiserror` enum in `buildl-core`, airsl-style: structured fields, no message parsing anywhere [1]. The design doc's three failure classes (§8.7) are a `#[non_exhaustive]` enum carried _on_ errors and outcomes, not inferred from them:
   ```

   With:

   ```markdown
   One `thiserror` enum in `buildl-core`, airsl-style: structured fields, no message parsing anywhere [1]. The design doc's failure classes (§8.7) are carried, not inferred. An action failure and an infrastructure failure are variants of a `#[non_exhaustive]` enum every outcome holds; a policy refusal is an error, `Error::Refused`, raised before any action runs, so no outcome carries one:
   ```

10. In the enum of §4, list the classes an outcome carries. Replace:

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

   With:

   ```rust
   pub enum OutcomeClass {
       Cached,
       Built,
       ActionFailure { status: i32 },          // the build did its job
       OutputMissing { name: OutputName },     // exit 0, a declared output absent
       InfraFailure { kind: InfraKind },       // Prepare, Run, TimedOut, Store
       Skipped { because: Label },
   }
   ```

11. In the paragraph below the enum, explain the new class and what `Skipped` names. Replace:

   ```markdown
   Classification is structural at the site that knows (spawn error vs non-zero exit vs timeout flag), never downstream. The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim. Every error that can name a target carries `Label + Provenance`; the display impl renders airsl-refusal-shaped messages — what was wanted, what was granted or found, where it was declared.
   ```

   With:

   ```markdown
   `OutputMissing` is the build revealing a fault, like `ActionFailure`: the action exited zero and left nothing at an output it declared. `Skipped` names the failed target by `Label`, so no `NodeId` reaches the log. Classification is structural at the site that knows (spawn error vs non-zero exit vs timeout flag), never downstream. The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim. Every error that can name a target carries `Label + Provenance`; the display impl renders airsl-refusal-shaped messages — what was wanted, what was granted or found, where it was declared.
   ```

12. In the `exec` bullet of §7, name the write that fails. Replace:

   ```markdown
   - **`exec`**: scripted fake strategies in `buildl-core` → state-machine transitions, keep-going semantics, crash-safety (a failing cache commit leaves no row); tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → output block integrity, a worker killed mid-action leaves no row.
   ```

   With:

   ```markdown
   - **`exec`**: scripted fake strategies in `buildl-core` → state-machine transitions, keep-going semantics, crash-safety (a failing cache write leaves no row); tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → output block integrity, a worker killed mid-action leaves no row.
   ```

13. Confirm the new state, the new class, the new method and the label landed:

   ```
   $ grep -n -e 'Pending --> Started' -e 'OutputMissing { name' -e 'fn collect' -e 'because: Label' docs/architecture.md
   171:    Pending --> Started : dep counter hits zero
   238:    fn collect<C: ContentStore>(
   266:    OutputMissing { name: OutputName },     // exit 0, a declared output absent
   268:    Skipped { because: Label },
   ```

14. Confirm nothing the edits replace is left, and that the document still holds its three diagrams. The first command prints `0` and exits `1`, which is the expected result:

   ```
   $ grep -c -e 'Refusal {' -e 'once per build' -e 'cache commit' -e 'because: NodeId' -e 'Ready --> Running' docs/architecture.md
   0
   ```
   ````
   $ grep -c '```mermaid' docs/architecture.md
   3
   ````

15. Commit:

   ```
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): record a build's states and its outcome classes in the architecture"
   ```

### Task 4 — Give the building blocks the ports a build uses

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. In the §5.1 table of `docs/architecture-building-blocks.md`, add the values a build hands around, and name `InfraKind`. `Grants` stays: it describes milestone 4. Replace:

   ```markdown
   |Phase handoffs|`Declaration`, `TargetGraph`, `Plan`, `ActionOutcome`|
   |Incrementality|`KeyComponents`, `ActionKey`|
   |Authority|`Ceiling`, `WantedSet`, `Want`, `Workspace`, `Grants`|
   |Classification|`OutcomeClass` (`#[non_exhaustive]`, `architecture.md` §4)|
   ```

   With:

   ```markdown
   |Phase handoffs|`Declaration`, `TargetGraph`, `Plan`, `ActionOutcome`|
   |Execution handoffs|`OutputPath` — where one output lives under `out/` — `ReadyAction` — one action with its command expanded — `RawOutcome` and `Exit` — how a run ended and what it printed — `OutcomeEvent` — one line of the log — `OnFailure`, `Summary`|
   |Incrementality|`KeyComponents`, `ActionKey`|
   |Authority|`Ceiling`, `WantedSet`, `Want`, `Workspace`, `Grants`|
   |Classification|`OutcomeClass` and `InfraKind` (`#[non_exhaustive]`, `architecture.md` §4)|
   ```

2. In the code block of §5.2, give the four ports a build uses their signatures, and add `Dispatcher`. The lines to replace run from the comment above `ContentStore` to the closing brace of `ExecStrategy`; `Clock`, below them, is unchanged. Replace:

   ```rust
   /// Workers write output blobs concurrently.
   pub trait ContentStore: Send + Sync {
       fn put(&self, bytes: &[u8]) -> Result<Digest>;
       fn contains(&self, digest: &Digest) -> bool;
   }

   /// The scheduler side is the single writer.
   pub trait ActionCache {
       fn row(&self, label: &Label) -> Option<&CacheRow>;
       fn upsert(&mut self, label: Label, row: CacheRow);
       fn commit(&mut self) -> Result<()>;
   }

   pub trait EventLog {
       fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
   }

   pub trait ExecStrategy: Send + Sync {
       /// Each isolation tier materialises a differently shaped exec dir.
       type Dir;
       fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
   }
   ```

   With:

   ```rust
   /// Workers write output blobs concurrently. `place` makes a stored blob the file at
   /// an output's path, replacing what is there.
   pub trait ContentStore: Send + Sync {
       fn put(&self, bytes: &[u8]) -> Result<Digest>;
       fn contains(&self, digest: &Digest) -> bool;
       fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()>;
   }

   /// The scheduler side is the single writer. A row is durable once `record` returns
   /// `Ok`; after `Err` the cache holds what it held before the call.
   pub trait ActionCache {
       fn row(&self, label: &Label) -> Option<&CacheRow>;
       fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
   }

   pub trait EventLog {
       fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
   }

   pub trait ExecStrategy: Send + Sync {
       /// Each isolation tier materialises a differently shaped exec dir.
       type Dir;
       fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
       /// One entry per declared output, in order: its digest once stored in `cas`,
       /// `None` when the action left nothing there.
       fn collect<C: ContentStore>(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           cas: &C,
       ) -> Result<Vec<Option<Digest>>>;
   }

   /// `work` runs one task, on any thread. `step` runs on the calling thread, first with
   /// `None` and then with each result, and returns the tasks that became ready.
   pub trait Dispatcher {
       fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
       where
           T: Send,
           R: Send,
           W: Fn(T) -> R + Sync,
           S: FnMut(Option<R>) -> Vec<T>;
   }
   ```

3. In the bundle of §5.3, add the dispatcher and the clock's bound. Replace:

   ```rust
       type Strategy: ExecStrategy;
       type Clock: Clock;
       // Dispatcher, Approver, Reporter
   ```

   With:

   ```rust
       type Strategy: ExecStrategy;
       type Dispatcher: Dispatcher;
       type Clock: Clock + Sync;   // a worker reads the clock
       // Approver, Reporter
   ```

4. In the §5.3 diagram, list the same two. The diagram stays Mermaid. Replace:

   ```markdown
           Strategy: ExecStrategy
           Clock: Clock
   ```

   With:

   ```markdown
           Strategy: ExecStrategy
           Dispatcher: Dispatcher
           Clock: Clock + Sync
   ```

5. In the §5.4 table, make the `Schedule` and `record` rows one `execute` row. The `grants` row stays. Replace:

   ```markdown
   |`Schedule`|pure state machine: `ready()`, `complete(node, class)`|— (driven through `Dispatcher`)|
   |record|`ActionOutcome` → cas, then row, then log|`ContentStore`, `ActionCache`, `EventLog`, `Clock`|
   ```

   With:

   ```markdown
   |execute|`TargetGraph`, `Plan` and the failure policy → `Vec<ActionOutcome>` (output layout, placeholder expansion, the program check, the `Schedule` state machine, cached or run, then cas, row, log)|`Manifest`, `Dispatcher`, `ExecStrategy`, `ContentStore`, `ActionCache`, `EventLog`, `Clock`|
   ```

6. In the §6 catalog, say what a build uses the content store and the action cache for. Replace:

   ```markdown
   |`ContentStore`|the cas|`.buildl/cas/ab/…`, temp file then rename|`buildl`|
   |`ActionCache`|cache rows|`.buildl/cache.json`|`buildl`|
   ```

   With:

   ```markdown
   |`ContentStore`|the cas: storing a blob, asking whether one is held, and making a stored output the file at its path under `out/`|`.buildl/cas/ab/…`, temp file then rename; hard links into `out/`|`buildl`|
   |`ActionCache`|cache rows: reading the last build's, and recording one durably for each target that was built|`.buildl/cache.json`|`buildl`|
   ```

7. In the paragraph that opens the fakes table of §8, say where `Clock` now stands: a build reads it, so it has a fake among the flows as well. Replace:

   ```markdown
   The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`. They implement the public traits through the public API alone, so whatever a fake does an adapter crate can do too, and the compiler checks that on every test build. A port no flow reaches yet keeps a unit-test fake beside its trait, as `Clock` does:
   ```

   With:

   ```markdown
   The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`. They implement the public traits through the public API alone, so whatever a fake does an adapter crate can do too, and the compiler checks that on every test build. A port no flow reaches yet keeps a unit-test fake beside its trait; `Clock` still has the one it had before a build read it:
   ```

8. In the fakes table, describe the fakes of the ports a build uses. Replace:

   ```markdown
   |`ContentStore`, `ActionCache`, `EventLog`|in-memory, with injectable failures|a filesystem|
   |`ExecStrategy`|scripted outcomes per label|process spawning|
   |`Dispatcher`|inline, on the test thread|threads; results are deterministic|
   |`Clock`|fixed|wall-clock time|
   ```

   With:

   ```markdown
   |`ContentStore`|blobs and placed paths held in memory; digests it can be told to forget, paths and content it can be told to fail on|a filesystem|
   |`ActionCache`|rows held in memory, with labels it can be told to fail on|a filesystem|
   |`EventLog`|the events appended, and whether to fail|a filesystem|
   |`ExecStrategy`|scripted per label: how the action exits, what it prints, what each output holds or that it is missing|process spawning|
   |`Dispatcher`|inline, on the test thread, taking the oldest or the newest waiting task first|threads; results are deterministic|
   |`Clock`|a counter, one tick per read|wall-clock time|
   ```

9. In the flows table of §8, name the write that fails. Replace:

   ```markdown
   |cas write succeeds, cache commit fails|no row for the target — the crash-safety invariant (design §8.5)|
   ```

   With:

   ```markdown
   |cas write succeeds, cache write fails|no row for the target — the crash-safety invariant (design §8.5)|
   ```

10. In the same table, say a refusal is an error and not a class. Replace:

   ```markdown
   |wanted program outside the ceiling|`Refusal` before any strategy runs|
   ```

   With:

   ```markdown
   |wanted program outside the ceiling|an `Error::Refused` before any strategy runs — an error of the build, not an outcome class|
   ```

11. Confirm the new methods, the dispatcher and the clock's bound are there:

   ```
   $ grep -n -e 'fn place' -e 'fn record(&mut self, label' -e 'fn collect' -e 'fn dispatch' -e 'type Dispatcher: Dispatcher' -e 'Clock + Sync' docs/architecture-building-blocks.md
   199:    fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()>;
   206:    fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
   220:    fn collect<C: ContentStore>(
   231:    fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
   265:    type Dispatcher: Dispatcher;
   266:    type Clock: Clock + Sync;   // a worker reads the clock
   301:        Clock: Clock + Sync
   ```

12. Confirm nothing the edits replace is left. The command prints `0` and exits `1`, which is the expected result:

   ```
   $ grep -c -e 'fn upsert' -e 'cache commit fails' -e '`Refusal`' docs/architecture-building-blocks.md
   0
   ```

13. Confirm the lines that describe milestone 4 are untouched, and that the document still holds its seven diagrams. The count of lines naming `` `Grants` `` or `Approver` is what it was before this task:

   ```
   $ grep -c -e '`Grants`' -e 'Approver' docs/architecture-building-blocks.md
   5
   ```
   ````
   $ grep -c '```mermaid' docs/architecture-building-blocks.md
   7
   ````

14. Commit:

   ```
   $ git add docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): give the building blocks the ports a build uses"
   ```

### Task 5 — Restate the status and the placeholder rule in the README

**Files:**
- Modify `README.md`

**Steps:**

1. In the status line at the top of `README.md`, name every phase as implemented. The line is one long paragraph; only its second sentence changes. Replace:

   ```markdown
   > **Status: design settled, implementation under way.** `buildl-core` ships the validated domain vocabulary, the one error enum, the canonical JSON serializer, the `Clock` and `DeclarationSource` ports with the six ports Plan reads, and the first three pipeline phases — Load, Resolve and Plan, with `check`, `graph` and `plan`, whose flows run against in-memory fakes; Execute and Record are not implemented yet. `buildl-lua` evaluates build files on airsl, and no command composes it with the pipeline yet. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ```

   With:

   ```markdown
   > **Status: design settled, implementation under way.** `buildl-core` ships the validated domain vocabulary, the one error enum, the canonical JSON serializer, the twelve ports the pipeline reaches everything outside it through, and every pipeline phase — Load, Resolve, Plan, Execute and Record, with `check`, `graph`, `plan` and `build`, whose flows run against in-memory fakes; the adapters that run a real build are not implemented yet. `buildl-lua` evaluates build files on airsl, and no command composes it with the pipeline yet. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ```

2. In the paragraph below the `build.lua` example, state the placeholder rule as design §5 now does. Replace:

   ```markdown
   Design decisions visible in those lines: cross-directory references are graph **labels** (`//lib:text`) resolved in Rust, never `require` — build files reference each other's *targets*, not each other's code. Placeholders (`$in`, `$out`, `$deps`) expand host-side at run time, the ninja model [4]; Lua never holds a real artifact path. And `b.sources()` is the declaration phase's only filesystem read, returning sorted results so declaration is deterministic regardless of filesystem order.
   ```

   With:

   ```markdown
   Design decisions visible in those lines: cross-directory references are graph **labels** (`//lib:text`) resolved in Rust, never `require` — build files reference each other's *targets*, not each other's code. Placeholders (`$in`, `$out`, `$deps`) expand host-side, each to a list of workspace-relative paths, the ninja model [4]; Lua never holds a real artifact path. And `b.sources()` is the declaration phase's only filesystem read, returning sorted results so declaration is deterministic regardless of filesystem order.
   ```

3. Confirm no file still says the last phase is missing. `grep` prints nothing and exits `1` when nothing matches, which is the expected result:

   ```
   $ grep -rn 'Execute and Record are not implemented yet' README.md CLAUDE.md docs crates/buildl-core/README.md crates/buildl-core/src/lib.rs
   ```

4. Confirm the new sentence is there, and that the README still holds its three diagrams:

   ```
   $ grep -c 'workspace-relative paths' README.md
   1
   ```
   ````
   $ grep -c '```mermaid' README.md
   3
   ````

5. Confirm `CLAUDE.md` was left alone. The command prints nothing, which is the expected result:

   ```
   $ git status --short CLAUDE.md
   ```

6. Commit:

   ```
   $ git add README.md
   $ git commit -m "docs(repo): restate the status and the placeholder rule in the README"
   ```

### Task 6 — Mark I6 Execute + Record done in the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. In the §3 table of `docs/roadmap.md`, replace the last cell of the **I6 Execute + Record** row — everything after ``|a full `build` runs against fakes|`` — with `**done**, <date> (§4)|`, where `<date>` is what this command prints:

   ```
   $ date +%Y-%m-%d
   ```

2. In §4, add the chain's row directly below the I5 Plan row. Write each plan's number of tasks in place of its `N`: the first command below prints one `path:count` line per plan. In the last cell, write the two hashes printed by the other two commands in place of `FIRST` and `LAST`: the commit of plan `01`'s first task, and the latest commit.

   ```markdown
   |I6 Execute + Record|`.claudestacks/sdlc/2026-10-11-core-execute-record/`|`01-handoff-types` (N tasks), `02-outcome-types` (N tasks), `03-cache-row` (N tasks), `04-errors` (N tasks), `05-ports` (N tasks), `06-before-run` (N tasks), `07-schedule` (N tasks), `08-decisions` (N tasks), `09-pipeline-build` (N tasks), `10-documentation` (6 tasks)|`FIRST` … `LAST`|
   ```

   ```
   $ grep -c '^### Task ' .claudestacks/sdlc/2026-10-11-core-execute-record/plans/*.md
   $ git log --reverse --format=%h -- crates/buildl-core/src/types/output_path.rs crates/buildl-core/src/types/ready_action.rs crates/buildl-core/src/types/raw_outcome.rs crates/buildl-core/src/types/on_failure.rs | head -n 1
   $ git log -n 1 --format=%h
   ```

   The four files in the second command are the ones plan `01` creates, so the oldest commit that touches any of them is that plan's first task.

3. In §5, drop the three follow-ups this chain closed, bring #17 up to date, and add the four this chain hands on. Replace:

   ```markdown
   |14|A command whose first argument holds a placeholder names no program at plan time, so it is neither checked against `proc.run` nor fingerprinted. Check it once the executor has expanded it.|I5 spec §2 (D6)|I6 Execute + Record|
   |15|Confirm a clean target's outputs are present in the cas before they are linked into `out/`. A plan trusts the cache row.|I5 spec §11|I6 Execute + Record|
   |16|`ActionCache` gains `upsert` and `commit`, and `CacheRow` gains `stdout` and `stderr`. I5 landed `row`, and a row holding the key and the output digests.|I5 spec §2 (D12)|I6 Execute + Record|
   |17|Read a cache row back: `Deserialize` for `ActionKey` and the types it is made of, which I5 left `Serialize`-only.|I5 spec §3.2|the `buildl` adapters slice|
   ```

   With:

   ```markdown
   |17|Read a cache row back: `Deserialize` for `ActionKey` and the types it is made of, which I5 left `Serialize`-only. Since I6 a row also holds the digests of what its action printed, `stdout` and `stderr`, so reading one back covers them too.|I5 spec §3.2, I6 spec §11|the `buildl` adapters slice|
   |18|A program a command names through `$opt:` in its first argument is held to `proc.run` when the build expands it, but has no fingerprint in the action key, so replacing that tool leaves the target clean.|I6 spec §11|the next slice that changes Plan|
   |19|A rule's `desc` is not expanded: nothing shows one yet.|I6 spec §11|the slice that adds the `Reporter` port|
   |20|An output that is a directory tree cannot be stored: `ExecStrategy::collect` answers one digest per declared output. Design §9.2 and §9.6 declare such outputs (`--outdir $out`, `tar … -C $out`) without the stamp file §9.2 prescribes.|I6 spec §11|the `buildl` adapters slice|
   |21|`[execution]`'s `timeout`, `workers` and `strategy` are not in `Workspace`, so the adapters that need them have no port to read them from.|I6 spec §11|the `buildl` adapters slice|
   ```

4. Confirm the three closed follow-ups are gone, that #17 to #21 are there, and that §4 has the chain's row. The first command prints `0` and exits `1`, which is the expected result:

   ```
   $ grep -c -e '^|14|' -e '^|15|' -e '^|16|' docs/roadmap.md
   0
   ```
   ```
   $ grep -c -e '^|17|' -e '^|18|' -e '^|19|' -e '^|20|' -e '^|21|' docs/roadmap.md
   5
   ```
   ```
   $ grep -c '^|I6 Execute + Record|' docs/roadmap.md
   1
   ```

5. Commit:

   ```
   $ git add docs/roadmap.md
   $ git commit -m "docs(repo): mark I6 Execute + Record done in the roadmap"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`: the crate doc's links resolve.
- No document says Execute and Record are missing; the crate docs and the README say every phase
  is implemented and tested against in-memory ports.
- The design says an output lives at `out/<directory>/<name>` and that two targets may not claim
  one path, runs its test example on `$deps`, states what each placeholder expands to, and says a
  refusal precedes execution in every case.
- The architecture draws the states `Pending`, `Started`, `Succeeded`, `Failed` and `Skipped`,
  gives `ActionOutcome` and `OutcomeClass` their real shape with no `Refusal`, describes
  `dispatch(work, step)`, `collect` and what runs where, and says a row is durable when it is
  recorded.
- The building blocks give `ContentStore`, `ActionCache`, `EventLog`, `ExecStrategy` and
  `Dispatcher` the signatures the code has, list `Dispatcher` and `Clock + Sync` in the bundle,
  and still describe `Grants`, `Approver` and `Reporter`.
- The roadmap shows I6 Execute + Record done, no longer lists follow-ups #14, #15 and #16, and
  carries #17 to #21.
- Every `mermaid` block is still a `mermaid` block.
