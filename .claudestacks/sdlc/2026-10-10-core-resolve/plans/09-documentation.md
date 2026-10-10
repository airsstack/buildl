---
status: done
created: 2026-10-10
depends-on: [08]
---

# Documentation Implementation Plan

**Goal:** The crate docs, the design documents and the roadmap describe what `buildl-core` holds once Resolve has landed.

**Architecture:** No code changes. Four groups of edits, one commit each: the crate's own docs (`lib.rs` and its README), the two architecture documents, the design document, and the roadmap. Each edit either settles text that a decision of this chain contradicts or records what was built. Mermaid diagrams stay Mermaid and every section number stays where it is, because the documents cite one another by section.

**Tech Stack:** rustdoc, Markdown, Mermaid, `cargo-make`. No new dependency.

**Content authority:** spec §10 (documentation deliverables and amendments); decisions D1, D3, D4, D6, D7, D10.

**Checkpoints:** review once, when all four tasks are done.

---

## Context an implementer needs

What plans `01`–`08` leave that this plan describes:

| Item | Where |
|---|---|
| `Node`, `TargetGraph` (label-keyed JSON, no `Deserialize`) | `crates/buildl-core/src/types/target_graph.rs` |
| `resolve`, re-exported from the crate root; the `resolve` module | `crates/buildl-core/src/resolve/` |
| `Pipeline::graph` | `crates/buildl-core/src/pipeline/driver.rs` |
| `cargo make guard-module-edges` and its golden file | `Makefile.toml`, `crates/expected-module-edges.txt` |

Facts that bite here:

- In `lib.rs` the name `resolve` is both a module and a function, so a doc link to it must say
  which: `[`resolve`](mod@resolve)` or `[`resolve`](fn@resolve)`. A bare link fails rustdoc, and
  rustdoc warnings fail the gate.
- The long lines in `docs/` are single lines; do not wrap them.
- `docs/roadmap.md` asks for a date and two commit hashes that exist only when this plan runs.
  Task 4 gives the command that prints each one.
- The root `README.md` changes in one place, its status paragraph (`README.md:7`), which task 4
  brings up to date. Its only passage on dependencies, aliases and placeholders
  (`README.md:60-68`) states no list order and no alias rule, and stays.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every command output below was captured by running that command on exactly the state the step describes.

## File structure

```
crates/buildl-core/src/lib.rs                    — [modify] module table, status paragraph (task 1)
crates/buildl-core/README.md                     — [modify] what the crate holds, status (task 1)
docs/architecture.md                            — [modify] §2 diagram, §3.2, §8 (task 2)
docs/architecture-building-blocks.md            — [modify] §7 rules table (task 2)
docs/design.md                                  — [modify] §5, §8.2, §12.1 (task 3)
docs/roadmap.md                                 — [modify] the I4 row, §4, §5 (task 4)
README.md                                       — [modify] the status paragraph (task 4)
```

### Task 1 — Describe Resolve in the crate docs

**Files:**
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/README.md`

**Steps:**

1. Add the module to the table in the crate doc of `crates/buildl-core/src/lib.rs`. Insert:

   ```rust
   //! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph |
   ```

   immediately after:

   ```rust
   //! | [`load`](mod@load) | Load, the first phase: build files to validated, sorted declarations |
   ```

2. In the same table, name the graph in the `types` row. Replace:

   ```rust
   names, identities, instants, declarations — and the as-written values a build file stages |
   ```

   with:

   ```rust
   names, identities, instants, declarations, the target graph — and the as-written values a build file stages |
   ```

3. Update the last line of the status paragraph. Replace:

   ```rust
   //! each evaluation. Resolve, Plan, Execute and Record are not implemented yet.
   ```

   with:

   ```rust
   //! each evaluation. Resolve is implemented: [`resolve`](fn@resolve) turns those declarations
   //! into a [`TargetGraph`], refusing a name declared twice, a reference that names nothing or the
   //! wrong kind of thing, and a dependency cycle; [`Pipeline::graph`] runs both phases. Plan,
   //! Execute and Record are not implemented yet.
   ```

4. Add two rows to the table in `crates/buildl-core/README.md`. Insert:

   ```markdown
   | Graph | `TargetGraph` — the runnable targets as `Node`s numbered in label order, their dependencies as edges in written order, with aliases and build settings beside them; it serializes keyed by label, with no id in the output |
   | Resolve | `resolve` turns declarations into a `TargetGraph`, refusing a name declared twice, a reference that names nothing or the wrong kind of thing, an undeclared `$opt:` setting, and a dependency cycle; `Pipeline::graph` runs Load once, then Resolve |
   ```

   immediately before:

   ```markdown
   | Errors | one structured enum
   ```

5. Extend the `Errors` row. Replace:

   ```markdown
   Load's errors name the build file and, where there is one, the declaration and field |
   ```

   with:

   ```markdown
   Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved |
   ```

6. Update the status. Replace:

   ```markdown
   **Status:** pre-release. Load, the first pipeline phase, is implemented and tested against
   in-memory ports; Resolve, Plan, Execute and Record are not implemented yet.
   ```

   with:

   ```markdown
   **Status:** pre-release. Load and Resolve, the first two pipeline phases, are implemented and
   tested against in-memory ports; Plan, Execute and Record are not implemented yet.
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

8. Commit:

   ```
   $ git add crates/buildl-core/src/lib.rs crates/buildl-core/README.md
   $ git commit -m "docs(buildl-core): describe Resolve in the crate docs"
   ```

### Task 2 — Record the graph's shape and the new guard in the architecture documents

**Files:**
- Modify `docs/architecture.md`
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. In the §2 class diagram of `docs/architecture.md`, give the graph its node type and its two side tables. Replace:

   ```text
       class TargetGraph {
           nodes: Vec~Target~
           edges: Vec~Vec~NodeId~~
           reverse: Vec~Vec~NodeId~~
           by_label: BTreeMap~Label_NodeId~
       }
   ```

   with:

   ```text
       class Node {
           label: Label
           provenance: Provenance
           role: TargetRole
           run: Command
           description: Option~Description~
           inputs: Vec~SourcePath~
           outputs: Vec~OutputName~
           env: Vec~EnvName~
           network: NetworkAccess
           freshness: Freshness
       }
       class TargetGraph {
           nodes: Vec~Node~
           edges: Vec~Vec~NodeId~~
           reverse: Vec~Vec~NodeId~~
           by_label: BTreeMap~Label_NodeId~
           aliases: BTreeMap~Label_NodeId~
           settings: BTreeMap~SettingName_SettingValue~
       }
   ```

2. In the same diagram, add the relation. Insert:

   ```text
       TargetGraph --> Node
   ```

   immediately after:

   ```text
       TargetGraph --> Declaration : built from
   ```

3. In §3.2, name the node type and the side tables. Replace:

   ```markdown
   `nodes: Vec<Target>`, `edges: Vec<Vec<NodeId>>` (deps), `reverse: Vec<Vec<NodeId>>`, plus `by_label: BTreeMap<Label, NodeId>`.
   ```

   with:

   ```markdown
   `nodes: Vec<Node>`, `edges: Vec<Vec<NodeId>>` (deps, in written order), `reverse: Vec<Vec<NodeId>>`, plus `by_label: BTreeMap<Label, NodeId>`; aliases and build settings sit beside the nodes as two more maps. A `Node` is a target with its rule expanded: it holds the command it runs.
   ```

4. In the same paragraph, say how the graph serializes. Replace:

   ```markdown
   and owning the representation keeps it `serde`-serializable as the stable `graph.json` without an adapter layer.
   ```

   with:

   ```markdown
   and owning the representation keeps `graph.json` stable: the graph serializes through a hand-written `Serialize` keyed by label, with dependencies as labels and no `NodeId` in the file, so adding a target changes only that target's entry.
   ```

5. In §8, record what is settled about settings. Replace:

   ```markdown
   — leaning `settings`, validated against declarations at Resolve.
   ```

   with:

   ```markdown
   — leaning `settings`, validated against declarations at Resolve. Settled so far: a `$opt:name` reference in a command is checked against the declared settings at Resolve. Where `--set` values are validated stays open.
   ```

6. In the §7 rules table of `docs/architecture-building-blocks.md`, add the guard. Insert:

   ```markdown
   |"no phase module names another" (`architecture.md` §1.1)|`crates/expected-module-edges.txt`, a committed list of `buildl-core`'s module-to-module imports, diffed by `cargo make guard-module-edges`. A grouped `crate::{…}` import, a `super::` that climbs to the crate root and an item named through the crate root's re-export (`crate::<Item>`) are refused outright, because the listing cannot read them|
   ```

   immediately after:

   ```markdown
   |the "no I/O API" half of 1|`crates/buildl-core/clippy.toml`, an enumerated ban on the filesystem, process, thread, network, environment, standard-stream and clock APIs, asserted by `cargo make guard-core-purity`|
   ```

7. Confirm the three edits to the diagram and §3.2 landed, one line each:

   ```
   $ grep -c -e 'nodes: Vec~Node~' -e 'TargetGraph --> Node' -e 'nodes: Vec<Node>' docs/architecture.md
   3
   ```

8. Confirm the rules table names the guard:

   ```
   $ grep -c 'guard-module-edges' docs/architecture-building-blocks.md
   1
   ```

9. Commit:

   ```
   $ git add docs/architecture.md docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): record the graph's shape and the module-edge guard"
   ```

### Task 3 — State the alias and list-order rules in the design

**Files:**
- Modify `docs/design.md`

**Steps:**

1. In §5 of `docs/design.md`, add two bullets after the `b.option` bullet. Insert:

   ```markdown
   - **`b.alias(name, target)` gives a target a second name.** An alias names a target, never another alias or a rule. A dependency may name an alias, and resolves to the target behind it.
   - **A list keeps the order it was written in.** `deps`, `inputs`, `outputs` and `env` reach the graph in written order with repeats dropped, so `$deps` and `$in` expand in that order. Reordering a list changes the command, and with it the action key.
   ```

   immediately after:

   ```markdown
   so differently-parameterised runs cache separately and correctly (§10.2).
   ```

2. In §8.2, say where the dependency count lives. Replace:

   ```markdown
   Output: the DAG, including the reverse edges and per-node dependency counters the executor needs.
   ```

   with:

   ```markdown
   Output: the DAG, including the reverse edges. A node's dependency count is the length of its edge list; the counters that change during a build belong to the scheduler (§8.4).
   ```

3. In §12.1 item 2, separate the order of declarations from the order inside a list. Replace:

   ```markdown
   so the graph is made order-insensitive: keyed by names, sorted at Resolve, action keys order-independent. A `pairs` loop yields a byte-identical graph in any order.
   ```

   with:

   ```markdown
   so the graph is made insensitive to the order declarations are issued in: keyed by names, numbered in label order at Resolve. A `pairs` loop that issues declarations yields a byte-identical graph in any order. The order inside one list is different: it is part of the command and of the action key, and a list whose order varies between two evaluations fails `buildl check`.
   ```

4. Confirm the four edits landed, one line each:

   ```
   $ grep -c -e 'gives a target a second name' -e 'keeps the order it was written in' -e 'belong to the scheduler (§8.4)' -e 'numbered in label order at Resolve' docs/design.md
   4
   ```

5. Commit:

   ```
   $ git add docs/design.md
   $ git commit -m "docs(repo): state the alias and list-order rules in the design"
   ```

### Task 4 — Mark I4 Resolve done in the roadmap

**Files:**
- Modify `docs/roadmap.md`
- Modify `README.md`

**Steps:**

1. In the §3 table of `docs/roadmap.md`, replace the last cell of the **I4 Resolve** row — everything after ``|the pipeline stops at `graph`|`` — with `**done**, <date> (§4)|`, where `<date>` is what this command prints:

   ```
   $ date +%F
   ```

2. In §4, add the chain's row after the I-lua row. In the last cell, write the two hashes printed by the commands below in place of `FIRST` and `LAST`: the commit of plan `01`'s task, and the latest commit.

   ```
   $ git log --format=%h -1 --grep='add Node and TargetGraph'
   $ git log --format=%h -1
   ```

    Insert:

   ```markdown
   |I4 Resolve|`.claudestacks/sdlc/2026-10-10-core-resolve/`|`01-graph-types` (1 task), `02-setting-references` (2 tasks), `03-resolve-errors` (2 tasks), `04-suggest` (1 task), `05-cycle` (1 task), `06-resolve` (4 tasks), `07-pipeline-graph` (2 tasks), `08-module-edge-guard` (1 task), `09-documentation` (4 tasks)|`FIRST` … `LAST`|
   ```

   immediately after:

   ```markdown
   |`27f7976` … `5e83952`|
   ```

3. In §5, delete the whole row that starts `|11|No guard asserts`: plan `08` closed that follow-up.

4. Confirm follow-up #11 is gone and the I4 row reads as done. `grep -c` prints `0` and exits `1` when nothing matches, which is the expected result of the first command:

   ```
   $ grep -c -e '^|11|' docs/roadmap.md
   0
   $ grep -c -e 'I4 Resolve.*done' docs/roadmap.md
   1
   ```

5. Bring the root `README.md`'s status paragraph up to date. Replace:

   ```markdown
   and Load — the first pipeline phase, with `check`, whose flows run against in-memory fakes; Resolve, Plan, Execute and Record are not implemented yet, and no build file can be evaluated until the Lua adapter lands. The design
   ```

   with:

   ```markdown
   and the first two pipeline phases — Load and Resolve, with `check` and `graph`, whose flows run against in-memory fakes; Plan, Execute and Record are not implemented yet. `buildl-lua` evaluates build files on airsl, and no command composes it with the pipeline yet. The design
   ```

6. Commit:

   ```
   $ git add docs/roadmap.md README.md
   $ git commit -m "docs(repo): mark I4 Resolve done in the roadmap"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` exits `0` after task 1: the crate doc's new links resolve under
  `RUSTDOCFLAGS=-D warnings`.
- No `mermaid` block in `docs/` was converted, and no section was renumbered:
  `git diff --stat` shows only the files in the file structure above.
- `docs/roadmap.md` shows I4 as done with a row in §4, and §5 no longer lists follow-up #11.

## Review findings

One reviewer pass over the four tasks, 2026-10-10. Verdict: compliant with amendments; blocking set of two, both fixed in one round. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0; `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). The delivery matched this plan text for text; the Mermaid field lists match the structs in `types/target_graph.rs`; no heading or fence line changed in `docs/`; `§8.4`, `§10.2`, `§12.1` and `§4` point where they say.

- blocking, fixed and verified — the root README's status paragraph still said Resolve was not implemented and that no build file can be evaluated until the Lua adapter lands. Both were false: this chain built Resolve, and the roadmap marks I-lua done on 2026-10-08. This plan and the spec's §10 said the README needed no change, having examined only `README.md:60-68`. The paragraph is rewritten in task 4, and the spec's §10 row and this plan's note, file list and task 4 are amended. Shown by `grep -rn -E "LuaSource|Pipeline|buildl_lua" crates/buildl/src crates/buildl-cli/src`, which printed one line, a doc comment (`crates/buildl/src/lib.rs:26`): no command composes the adapter with the pipeline, as the new sentence says — `README.md:7`
- blocking, fixed and verified — "so adding a target changes only that target's lines" is false as worded: the canonical serializer has no multi-line form, and the golden `graph.json` is one line. The word is now "entry" in `docs/architecture.md`, in the rustdoc on `impl Serialize for TargetGraph`, in this plan's task 2 and in the spec's D7. After the fix `cargo make dod` exited 0, ending `[cargo-make] INFO - Build Done in 12.65 seconds.` — `docs/architecture.md:201`, `crates/buildl-core/src/types/target_graph.rs:195`
- risk — an unchanged neighbour of the amended §12.1 text now disagrees with it: "A `pairs` loop does not fail the check" is unqualified, while a `pairs` loop that fills a list field does fail it. Not in the spec's §10 list. Not fixed — `docs/design.md:589`
- nit — the quoted rule "no phase module names another" is cited to `architecture.md` §1.1, whose words are "no phase invokes the next"; the quoted words are rule 1 of the crate doc. The reference lands in the right section. Not fixed — `docs/architecture-building-blocks.md:292`
- nit — "Resolve's name every declaration involved": `Error::TooManyTargets { count }` names none. Not fixed — `crates/buildl-core/README.md:22`
- nit — the crate doc's list of what Resolve refuses leaves out the undeclared `$opt:` setting, which the crate README lists. Two lists of one thing differ. Not fixed — `crates/buildl-core/src/lib.rs:56`
- nit — "Reordering a list changes the command" is over-general for `env`, which no placeholder expands. The spec's D3 wording. Not fixed — `docs/design.md:153`
- nit — an unchanged neighbour says Resolve runs "three validations"; `resolve` also refuses a wrong-kind reference, an undeclared setting and too many targets. Not in the spec's §10 list. Not fixed — `docs/design.md:219`

## Probe results

The plan's verification commands are its probes. Each matched:

- `cargo fmt --all -- --check` and `cargo make dod` after task 1 — exit 0 and exit 0, ending `[cargo-make] INFO - Build Done in 14.27 seconds.`
- `grep -c -e 'nodes: Vec~Node~' -e 'TargetGraph --> Node' -e 'nodes: Vec<Node>' docs/architecture.md` — `3`
- `grep -c 'guard-module-edges' docs/architecture-building-blocks.md` — `1`
- `grep -c -e 'gives a target a second name' -e 'keeps the order it was written in' -e 'belong to the scheduler (§8.4)' -e 'numbered in label order at Resolve' docs/design.md` — `4`
- `grep -c -e '^|11|' docs/roadmap.md` — `0`
- `grep -c -e 'I4 Resolve.*done' docs/roadmap.md` — `1`
- `git log --format=%h -1 --grep='add Node and TargetGraph'` — `46a8164`

One claim came out against the plan and the spec:

- **against the plan** — claim: the root `README.md` needs no change. `README.md:7` said "Resolve, Plan, Execute and Record are not implemented yet, and no build file can be evaluated until the Lua adapter lands". Closed by the first finding above.

## Deviations

- 2026-10-10 — task 4 gained a file and a step: the root `README.md`'s status paragraph. The review disproved the premise that the README needed no change; this plan and the spec's §10 row were amended on the main thread.
- 2026-10-10 — task 2's guard sentence was written with a third refused spelling, `crate::<Item>`, after plan `08`'s guard was tightened in this run. This plan's step 6 was amended to match.
- 2026-10-10 — the review's wording fix ("entry" for "lines") was committed on its own, as `c8b84e8` `docs(repo): say a new target changes one entry of the graph file`, because it reaches a file of plan `01` as well as task 2's. The roadmap's §4 range therefore ends at `c8b84e8`, not at task 3's commit `45313cf`.
- 2026-10-10 — tasks 2 and 3 ran before task 1, alongside plan `08`'s review: they edit only `docs/` and run no build. Tasks 1 to 3 were committed, at the author's request, before the plan's one review, so that task 4 could cite real hashes.
- 2026-10-10 — the plan ran while plan `08`, which it depends on, was built and reviewed but not yet marked `done`. Plans `06` to `09` were executed as one requested run.
