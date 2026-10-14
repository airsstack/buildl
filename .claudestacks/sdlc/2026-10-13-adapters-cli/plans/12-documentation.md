---
status: done
created: 2026-10-14
depends-on: [10, 11]
---

# Documentation Implementation Plan

**Goal:** Every document says what buildl is once the adapters, the `Workspace` facade and the `buildl` binary have landed.

**Architecture:** No code changes. Thirty-two tasks, one commit each. A task is one section of one document, or one topic across documents, and every edit in it either replaces a sentence that is wrong once plans `01` to `11` have landed or adds what no document states yet. The order is the design, the architecture, the building blocks, the four crate READMEs, the root README, then the roadmap. The first thirty-one tasks describe what is built and where later work lands; only the last task marks I9 and milestone 1 done, and it ends with a sweep of every document. `docs/fundamental-walkthrough.md` is not edited: it repeats no amended sentence. The design documents and the READMEs never name an intent or a milestone; where one of them has to say when something arrives it cites the item of design §13, which is what the roadmap calls a milestone. Diagrams in `docs/` stay Mermaid, no section is renumbered, and every section cross-reference survives.

**Tech Stack:** Markdown and Mermaid for the documents; `grep`, `sed` and `perl` for the searches; `cargo make dod` and `cargo deny check` as the gate; git.

**Content authority:** spec §15 (every row of §15.1 to §15.4 that names a document), §16 (both parts: every placement stands as approved, and parallel loading is left out of milestone 1), §3 (the Unix-only statement) and §6.2 (the paragraph for design §12.2), with the sections those rows take their content from: §2, §4.1, §4.2, §4.5, §4.6, §5, §6, §7, §8, §9.1 to §9.3, §10, §12.1, §12.4, §13.2, §13.6 and §14.

---

## Context an implementer needs

What plans `01` to `11` leave that this plan describes. Nothing here is created by this plan; if one of these is missing or named differently on the tree, the sentence that names it is wrong and the task that writes it must not be committed.

| Item | Left by |
|---|---|
| In `buildl-core`: the values `Scope`, `LiveOutput`, `Channel`, `LiveSink`, `Captured`, `Turn` (`Begin`, `Told`, `Done`), `InputStat`, `InputKind` and `Hasher`; `Exit::Signaled` and `OutcomeClass::Killed { signal }`; `Error::UnknownTarget`, `Error::CorruptState` and `Error::InputNotAFile` | plan `01` |
| In `buildl-core`: the ports `Reporter` (`begun`, `started`, `printed`, `finished`, `summary`) and `GraphRecord` (`record`); `Dispatcher::dispatch<T, M, R, W, S>`; `ExecStrategy::run` with `live: Option<LiveSink<'_>>`; `Digester::stat` returning `Result<InputStat>`; `Ports::Graph` and `Ports::Reporter`, and the `PortSet` fields `graph` and `reporter`, fourteen ports in all | plan `02` |
| In `buildl-core`: `resolve::narrow`; the free functions `pipeline::check` and `pipeline::graph`; `Pipeline::plan(entry, scope, overrides)` and `Pipeline::build(entry, scope, overrides, on_failure, live)`; the graph recorded by every build | plan `03` |
| In `buildl`: the seventeen modules `manifest`, `host`, `digest`, `tools`, `cas`, `cache`, `log`, `statcache`, `graph`, `exec`, `dispatch`, `clock`, `report`, `state`, `search_path`, `local_ports` and `workspace`; the dependencies `toml`, `rayon`, `serde`, `thiserror` and `tempfile`; the gate task `cargo make guard-adapter-isolation` | plan `04` |
| In `buildl`: the adapters, behaving as the documents will say — `buildl.toml` read strictly; a tool fingerprinted by the bytes of the file its name leads to; the cas as `.buildl/cas/<2 hex>/<62 hex>` with read-only blobs, the `.x` twin and `cas/tmp/`; `cache.json` with `cache.journal`; `.buildl/lock`; the two-second rule of the stat cache; `graph.json` written by every build; `Bare`; the thread pool; `TerminalReporter` and `Silent` | plans `05` to `08` |
| In `buildl`: `LocalPorts<R>`, `LocalPipeline<R>`, `Wiring`, `Workspace` with `open`, `root`, `label`, `check`, `graph`, `plan` and `build`, and `pub use buildl_core;` | plan `09` |
| The test `two_clean_builds_in_two_directories_are_byte_identical` in `crates/buildl/tests/e2e`, and fixtures under `crates/buildl/tests/fixtures` | plan `10` |
| The binary: `check`, `graph` with `--json` or `--dot`, `plan` and `build` with `[TARGET]...` and `--set NAME=VALUE`, `plan --json`, `build -k`/`--keep-going` and `--stream`, `--color`; the exit codes `0` to `7`; its tests under `crates/buildl-cli/src` and `crates/buildl-cli/tests/cli` | plan `11` |

The spec also lists rustdoc that goes stale with these documents. It is not in this plan, because a crate's rustdoc belongs to the plan that writes the crate's code:

| Rustdoc | Belongs to |
|---|---|
| `crates/buildl-core/src/lib.rs` (status text and module table), `crates/buildl-core/src/error.rs` (the sentence on how an error is shown), `crates/buildl-core/src/types/workspace.rs` (a strategy the running buildl lacks is refused) | plans `01` to `03` |
| `crates/buildl/src/lib.rs` (the adapter list and "It will assemble") | plans `04` and `09` |
| `crates/buildl-cli/src/main.rs` ("Its own job will be…", "`main` is empty") | plan `11` |

Where each amendment comes from, by spec row, so a reviewer can count them. A task number in brackets is the task that carries the row.

| Spec | Rows, in the spec's order |
|---|---|
| §15.1 `docs/design.md` | line 3 [1]; §3 tree [2]; §3 metadata files [2]; §4 [3]; §5 [4]; §7 table [5]; §7 rule 3 [5]; §8.5 [6]; §8.7 output [7]; §8.7 exit codes and failure table [7]; §9.6 [8]; §12.2 [9]; §12.3 [9]; §13 [10] |
| §15.2 `docs/architecture.md` | line 3 [1]; §3.1 [11]; §5 rule 3 [11]; §3.3 [12]; §3.4 [13]; §3.5 [14]; §4 [15]; §5 rule 4 [16]; §5 rule 5 [16]; §6 [17]; §7 [18]; §8 [18] |
| §15.3 `docs/architecture-building-blocks.md` | line 3 [1]; §3 [19]; §4 diagram and crate table [19]; §4 platforms [19]; §5.2 [20]; §5.3 [21]; §5.4 [21]; §6 [22]; §7 [23]; §8 [24]; §9 [25]; §10 [25]; §11 [25] |
| §15.4 other files | `README.md` [30]; `CLAUDE.md` structural rule [23] and Status [32]; `crates/buildl/README.md` [28]; `crates/buildl-cli/README.md` [29]; `crates/buildl-core/README.md` [26]; `crates/buildl-lua/README.md` [27]; `docs/roadmap.md` [31, 32]. Its rows for rustdoc, `Cargo.toml`, `deny.toml` and `Makefile.toml` belong to the code plans |
| §16.1, §16.2 placements | design §13 [10]; roadmap §2 [31]; the README's roadmap list [30] |
| §16.3 this slice's status | roadmap §1, §2 row 1, §3 and §4 [32] |
| §3 Unix targets only | building blocks §4 [19]; the `buildl` and `buildl-cli` READMEs [28, 29]; the root README [30] |
| §6.2 the paragraph for design §12.2 | [9] |

Two edits go beyond a row of §15, each because a row's own edit needs it: task 20 adds two rows to the table of building blocks §5.1 for the values the new signatures of §5.2 name, and task 32 puts the roadmap's ground rule on I9 in the past tense, since milestone 1 owes nothing once it is done. The roadmap's ground rule on publishing is left as it is, as spec §16.2 decides.

Facts that bite:

- **Blocks are exact.** Every `Replace` block is text that occurs in its file exactly once at that point of the plan, and every `With` block is the whole text that takes its place. Blocks are fenced with `~~~~` because the text inside holds ` ``` ` fences. A block is indented to sit under its step; that indentation — three spaces under steps 1 to 9, four from step 10 — is not part of the text, and an empty line inside a block is an empty line. A paragraph in these documents is one long line, and a `Replace` block is often a part of such a line, not the whole of it.
- **Step 1 of every task shows the anchors.** Each search quotes a piece of a block and prints how many lines hold it. If a search of step 1 prints anything but what the plan shows, stop: the file is not the one this plan was written against, and no edit of that task is safe.
- **`grep -c` exits `1` when it prints `0`.** That is the expected result of every search for text an edit has not written yet, or has removed.
- **No other plan of this chain edits these ten files.** Plans `01` to `11` write code, manifests and rustdoc. The one exception to watch is `docs/roadmap.md`, whose I9 row and diagram label change whenever the chain changes state; task 32 therefore replaces those two lines by how they start, whatever the rest reads, and its step 1 shows that exactly one line starts that way.
- **No crate README is compiled.** No crate includes its README as crate documentation (`grep -rn -F 'include_str!("../README.md")' crates/` printed nothing when this plan was written; the three `include_str!` uses in the workspace read golden files). Tasks 26 to 29 also leave the four crate READMEs without a fenced code block, which task 29 checks, so `cargo make dod`'s doctest step has nothing of theirs to run either way.
- **Diagrams stay Mermaid.** Ten edits change text inside a `mermaid` block and none converts, adds or removes one: 13 blocks in `docs/design.md`, 3 in `docs/architecture.md`, 7 in `docs/architecture-building-blocks.md`, 2 in `docs/roadmap.md` and 3 in `README.md`, before and after. Task 32 checks the counts.
- **Words.** `docs/design.md`, `docs/architecture.md`, `docs/architecture-building-blocks.md` and the five READMEs never say "milestone", "intent" or "slice", and no new text may. "Item 3 of design §13" is how they say what the roadmap calls milestone 3. `docs/roadmap.md` and `CLAUDE.md` do name milestones and intents.
- **How this plan was proven.** Every edit below was made for real, in task order, on a copy of the eleven files as they stand on this branch, uncommitted roadmap edits included. A script confirmed that each `Replace` block occurred exactly once in its file before its edit and each `With` block exactly once after it, and every search result quoted in a step is the output `grep`, `sed` or `perl` really printed on that copy at that point. The gate was not run on that copy, which holds documents only; the gate steps state the exit status to expect, nothing more.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task; the scope is `repo` for the documents in `docs/`, the root `README.md` and `CLAUDE.md`, and the crate's name for a crate README.

## File structure

```
docs/design.md                         — [modify] the status line; §3, §4, §5, §7, §8.5, §8.7, §9.6, §12.2, §12.3, §13
docs/architecture.md                   — [modify] the status line; §3.1, §3.3, §3.4, §3.5, §4, §5 rules 3 to 5, §6, §7, §8
docs/architecture-building-blocks.md   — [modify] the status line; §3, §4, §5.1 to §5.4, §6, §7, §8, §9, §10, §11
docs/roadmap.md                        — [modify] §1, §2, §3, §4
README.md                              — [modify] the status, the command table, a new section "Running it", the roadmap list
CLAUDE.md                              — [modify] the structural rule on adapter modules; the Status paragraph
crates/buildl-core/README.md           — [modify] eight table rows and the status
crates/buildl-lua/README.md            — [modify] the status
crates/buildl/README.md                — [modify] everything between the opening line and the licence
crates/buildl-cli/README.md            — [modify] everything between the opening line and the licence
```

Every task has the same shape: step 1 shows the anchors, the middle steps are the edits, and the last three steps verify with the same searches, run `cargo make dod` and `cargo deny check`, and commit.

### Task 1 — Say in the three design documents that the core pipeline is built

**Files:**
- Modify `docs/design.md`
- Modify `docs/architecture.md`
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '**Status: draft for review.** Nothing described here is implemented.' docs/design.md
   1
   $ grep -c -F -e '**Status: the core pipeline is implemented; the rest is design.** Item 1 of the sequence in §13' docs/design.md
   0
   # step 3
   $ grep -c -F -e '**Status: draft for review.** Companion to [`design.md`](./design.md): that document says' docs/architecture.md
   1
   $ grep -c -F -e '**Status: the build-system core is implemented.** Companion to [`design.md`](./design.md): that' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 'Plugins, remote, and watch are designed for but not implemented by anything here.' docs/architecture.md
   1
   $ grep -c -F -e 'That core is built, with actions run at isolation tier 1 (design doc §12.2): what this document' docs/architecture.md
   0
   # step 5
   $ grep -c -F -e '**Status: draft for review.** Companion to [`design.md`](./design.md) and [`architecture.md`](./' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '**Status: built for the core pipeline.** What this document describes exists, except what arrive' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/design.md`, on line 3, the status line, say what is built and what is design. Replace:

   ~~~~markdown
   **Status: draft for review.** Nothing described here is implemented.
   ~~~~

   With:

   ~~~~markdown
   **Status: the core pipeline is implemented; the rest is design.** Item 1 of the sequence in §13 is built: Load, Resolve, Plan, Execute and Record, the `check`, `graph`, `plan` and `build` commands, the local cache and the cas, with actions run at isolation tier 1 (§12.2). Everything from item 2 on is described here and not built.
   ~~~~

3. In `docs/architecture.md`, on line 3, the status line, replace the opening words. Replace:

   ~~~~markdown
   **Status: draft for review.** Companion to [`design.md`](./design.md): that document says
   ~~~~

   With:

   ~~~~markdown
   **Status: the build-system core is implemented.** Companion to [`design.md`](./design.md): that document says
   ~~~~

4. In `docs/architecture.md`, on the same line, say what of the scope is built, before its last sentence. Replace:

   ~~~~markdown
   Plugins, remote, and watch are designed for but not implemented by anything here.
   ~~~~

   With:

   ~~~~markdown
   That core is built, with actions run at isolation tier 1 (design doc §12.2): what this document describes runs, except where a section says a part arrives with a later item of the design doc's §13. Plugins, remote, and watch are designed for but not implemented by anything here.
   ~~~~

5. In `docs/architecture-building-blocks.md`, on line 3, the status line, replace the opening words. Replace:

   ~~~~markdown
   **Status: draft for review.** Companion to [`design.md`](./design.md) and [`architecture.md`](./architecture.md):
   ~~~~

   With:

   ~~~~markdown
   **Status: built for the core pipeline.** What this document describes exists, except what arrives with a later item of design §13: for grant negotiation the `Approver` port, the `Grants` value and the `grants` logic, and for the input sandbox the `InputSandbox` strategy. Companion to [`design.md`](./design.md) and [`architecture.md`](./architecture.md):
   ~~~~

6. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '**Status: draft for review.** Nothing described here is implemented.' docs/design.md
   0
   $ grep -c -F -e '**Status: the core pipeline is implemented; the rest is design.** Item 1 of the sequence in §13' docs/design.md
   1
   # step 3
   $ grep -c -F -e '**Status: draft for review.** Companion to [`design.md`](./design.md): that document says' docs/architecture.md
   0
   $ grep -c -F -e '**Status: the build-system core is implemented.** Companion to [`design.md`](./design.md): that' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 'Plugins, remote, and watch are designed for but not implemented by anything here.' docs/architecture.md
   1
   $ grep -c -F -e 'That core is built, with actions run at isolation tier 1 (design doc §12.2): what this document' docs/architecture.md
   1
   # step 5
   $ grep -c -F -e '**Status: draft for review.** Companion to [`design.md`](./design.md) and [`architecture.md`](./' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '**Status: built for the core pipeline.** What this document describes exists, except what arrive' docs/architecture-building-blocks.md
   1
   ~~~~

7. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

8. Commit:

   ~~~~text
   $ git add docs/design.md docs/architecture.md docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): say in the three design documents that the core pipeline is built"
   ~~~~

### Task 2 — Describe the state directory buildl writes in design §3

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'buildl.toml          # workspace manifest: ceiling, approver, limits' docs/design.md
   1
   $ grep -c -F -e 'buildl.toml          # workspace manifest: ceiling, limits, execution (§4)' docs/design.md
   0
   # step 3
   $ grep -c -F -e 'graph.json         # last resolved DAG (sorted keys, diff-stable)' docs/design.md
   1
   $ grep -c -F -e 'lock               # held while a plan or a build uses the directory' docs/design.md
   0
   # step 4
   $ grep -c -F -e 'All metadata files use airsl' docs/design.md
   1
   $ grep -c -F -e 'All metadata files are written by buildl' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the tree of §3, drop the approver from the manifest's comment: no approver table is read yet. Replace:

   ~~~~markdown
     buildl.toml          # workspace manifest: ceiling, approver, limits
   ~~~~

   With:

   ~~~~markdown
     buildl.toml          # workspace manifest: ceiling, limits, execution (§4)
   ~~~~

3. In `docs/design.md`, in the same tree, list every file of `.buildl/` and say what `out/` holds. Replace:

   ~~~~markdown
       graph.json         # last resolved DAG (sorted keys, diff-stable)
       cache.json         # action cache: per target, its action key and what its last build produced and printed
       cas/<sha256>       # content-addressed artifact store
       log.jsonl          # one JSON event per action
     out/                 # named outputs, linked from the cas: out/<directory>/<name>
   ~~~~

   With:

   ~~~~markdown
       lock               # held while a plan or a build uses the directory
       graph.json         # the graph the last build resolved, written by every build (sorted keys, diff-stable)
       cache.json         # action cache: per target, its action key and what its last build produced and printed
       cache.journal      # rows recorded since cache.json was written; gone once a build has returned
       statcache.json     # per input: its modification time, size and content digest (§8.3)
       cas/ab/cdef…       # content-addressed artifact store: two hex digits of the SHA-256, then the other 62
       cas/ab/cdef….x     # the same bytes as an executable file, kept for an output placed executable
       cas/tmp/           # staging for the store
       log.jsonl          # one JSON event per action
     out/                 # named outputs, out/<directory>/<name>: read-only links into the cas, copies where linking is not used
   ~~~~

4. In `docs/design.md`, in the paragraph below the tree, say whose serializer writes the files, which of them are byte-stable, and what a file that fails to read does. Replace:

   ~~~~markdown
   All metadata files use airsl's sorted-key JSON encoding, so they are byte-stable across runs and diff cleanly in review. The cache is read back by the same rule it is written by: a file that is not byte for byte what buildl writes for the rows it holds — a field missing or unknown, a key out of order, a key digest that does not match its components — is refused as corrupt rather than trusted.
   ~~~~

   With:

   ~~~~markdown
   All metadata files are written by buildl's own canonical JSON serializer — object keys sorted at every level, no whitespace — so they diff cleanly in review. `graph.json` and `cache.json` are byte-identical across two builds of the same workspace on one machine, wherever the workspace stands; `log.jsonl` holds clock readings and `statcache.json` modification times, so those two are not. The cache is read back by the same rule it is written by: a file that is not byte for byte what buildl writes for the rows it holds — a field missing or unknown, a key out of order, a key digest that does not match its components — is refused as corrupt rather than trusted. The refusal is an error that names the file and the remedy: remove it and build again, and every target is built anew. `statcache.json` is the exception: one that fails to read is discarded and written again, since it holds nothing a hash cannot give back.
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'buildl.toml          # workspace manifest: ceiling, approver, limits' docs/design.md
   0
   $ grep -c -F -e 'buildl.toml          # workspace manifest: ceiling, limits, execution (§4)' docs/design.md
   1
   # step 3
   $ grep -c -F -e 'graph.json         # last resolved DAG (sorted keys, diff-stable)' docs/design.md
   0
   $ grep -c -F -e 'lock               # held while a plan or a build uses the directory' docs/design.md
   1
   # step 4
   $ grep -c -F -e 'All metadata files use airsl' docs/design.md
   0
   $ grep -c -F -e 'All metadata files are written by buildl' docs/design.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): describe the state directory buildl writes in design §3"
   ~~~~

### Task 3 — State how buildl.toml is read in design §4

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'default = "interactive"    # prompt on the first build of a fresh clone' docs/design.md
   1
   # step 3
   $ grep -c -F -e 'requests are intersected with it, never unioned.' docs/design.md
   1
   $ grep -c -F -e 'The manifest is read strictly. A key this version of buildl does not read is an error that names' docs/design.md
   0
   # step 4
   $ grep -c -F -e 'None of the three is an action-key component (§8.3): they decide how an action is run, never whi' docs/design.md
   1
   $ grep -c -F -e 'Until the input sandbox ships (§13 item 3) buildl runs `bare` alone. `build` refuses a manifest' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the example manifest of §4, remove the `[approver]` table and the blank line after it: a manifest that holds one is refused today. This edit only removes text. Replace:

   ~~~~markdown
   [approver]
   default = "interactive"    # prompt on the first build of a fresh clone
   ci      = "manifest"       # honour declarations silently when CI is detected

   [declaration]              # ceilings for the Lua phase — it only declares
   ~~~~

   With:

   ~~~~markdown
   [declaration]              # ceilings for the Lua phase — it only declares
   ~~~~

3. In `docs/design.md`, after the paragraph that follows the example, add a paragraph on how the file is read. Replace:

   ~~~~markdown
   requests are intersected with it, never unioned.
   ~~~~

   With:

   ~~~~markdown
   requests are intersected with it, never unioned.

   The manifest is read strictly. A key this version of buildl does not read is an error that names the key with its line and column, so a misspelt `fs.wirte` cannot silently drop a grant. An `[approver]` table is such a key until grant negotiation ships (§13 item 4), which is why the example has none. Every table and every key is optional, and an empty file is a valid manifest: every default, and an empty ceiling, under which `check` and `graph` work and `plan` and `build` are refused, since an empty ceiling allows nothing. A directory with no `buildl.toml` in it or above it is not a workspace: that is an error, never a default ceiling (§7). `config` must be `1`. `out` may not be the workspace root, and may not be `.buildl`, contain it or lie inside it. The state directory is always `.buildl`; no key names it. A directory entry of the ceiling is workspace-relative, and `"."` is the workspace root. `memory` is a whole number directly followed by one unit — `B`, `KB`, `MB` or `GB`, each 1024 times the one before — and never zero.
   ~~~~

4. In `docs/design.md`, below the `[execution]` table, say which strategy runs today and what each command does with another one. Replace:

   ~~~~markdown
   None of the three is an action-key component (§8.3): they decide how an action is run, never which action it is.
   ~~~~

   With:

   ~~~~markdown
   Until the input sandbox ships (§13 item 3) buildl runs `bare` alone. `build` refuses a manifest whose strategy is anything else, the default `sandbox` included, rather than run its actions under a weaker tier than the one asked for; the error names the line to write, `strategy = "bare"`. `check`, `graph` and `plan` run nothing, and work under any strategy.

   None of the three is an action-key component (§8.3): they decide how an action is run, never which action it is.
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'default = "interactive"    # prompt on the first build of a fresh clone' docs/design.md
   0
   # step 3
   $ grep -c -F -e 'requests are intersected with it, never unioned.' docs/design.md
   1
   $ grep -c -F -e 'The manifest is read strictly. A key this version of buildl does not read is an error that names' docs/design.md
   1
   # step 4
   $ grep -c -F -e 'None of the three is an action-key component (§8.3): they decide how an action is run, never whi' docs/design.md
   1
   $ grep -c -F -e 'Until the input sandbox ships (§13 item 3) buildl runs `bare` alone. `build` refuses a manifest' docs/design.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): state how buildl.toml is read in design §4"
   ~~~~

### Task 4 — Say in design §5 that a declared input is a regular file

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Every field but `always` is an action-key component; the surface grows reluctantly.' docs/design.md
   1
   $ grep -c -F -e '**A declared input is a regular file inside the workspace.** A symbolic link, a directory or a s' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the bullets below the example of §5, add one bullet after the `b.target` bullet. Replace:

   ~~~~markdown
   Every field but `always` is an action-key component; the surface grows reluctantly.
   ~~~~

   With:

   ~~~~markdown
   Every field but `always` is an action-key component; the surface grows reluctantly.
   - **A declared input is a regular file inside the workspace.** A symbolic link, a directory or a special file at an input's path is refused at plan as a mistake of the build file, like an input that does not exist: the error names the target that declares the input and what stands at the path.
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Every field but `always` is an action-key component; the surface grows reluctantly.' docs/design.md
   1
   $ grep -c -F -e '**A declared input is a regular file inside the workspace.** A symbolic link, a directory or a s' docs/design.md
   1
   ~~~~

4. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

5. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): say in design §5 that a declared input is a regular file"
   ~~~~

### Task 5 — Bring the command table of design §7 up to the binary

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '|`buildl graph`|Resolve|emit the DAG (`--json`, `--dot`)|' docs/design.md
   1
   $ grep -c -F -e '|`buildl graph`|Resolve|emit the DAG (`--json`, the default, or `--dot`)|' docs/design.md
   0
   # step 3
   $ grep -c -F -e '3. **The log is an artifact.** `log.jsonl` records one sorted-key JSON object per action: target' docs/design.md
   1
   $ grep -c -F -e 'ation, cache hit or miss. It grows across builds, and the newest line for a label is that target' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the command table of §7, give `graph` its default, `plan` and `build` their targets, add the `log` row, and add a paragraph below the table. Replace:

   ~~~~markdown
   |`buildl graph`|Resolve|emit the DAG (`--json`, `--dot`)|
   |`buildl plan`|Plan|dry run: what would rebuild, and why|
   |`buildl build [target]`|Record|the full pipeline|
   |`buildl watch [target]`|loops|persistent process, re-plans on file change|
   |`buildl test` / `clean` / `gc`|—|run test targets; drop outputs; prune the cas|
   |`buildl query <expr>`|Resolve|graph queries: `deps()`, `rdeps()` (§10.1)|
   |`buildl doctor`|—|print the effective ceiling, grants, workers|
   ~~~~

   With:

   ~~~~markdown
   |`buildl graph`|Resolve|emit the DAG (`--json`, the default, or `--dot`)|
   |`buildl plan [target]...`|Plan|dry run: what would rebuild, and why|
   |`buildl build [target]...`|Record|the full pipeline|
   |`buildl watch [target]`|loops|persistent process, re-plans on file change|
   |`buildl test` / `clean` / `gc`|—|run test targets; drop outputs; prune the cas|
   |`buildl log <target>`|—|show what a target printed when it last ran (§8.7)|
   |`buildl query <expr>`|Resolve|graph queries: `deps()`, `rdeps()` (§10.1)|
   |`buildl doctor`|—|print the effective ceiling, grants, workers|

   `check`, `graph`, `plan` and `build` are built; each other command arrives with the item of §13 that names it. `plan` and `build` take any number of targets and, with none, every target; `plan` takes exactly what `build` takes, because it is that build's dry run. Both accept `--set name=value`, repeated, to give a build setting a value for the run (§10.2); `build` adds `--keep-going` (§8.4) and `--stream` (§8.7). A command finds its workspace at the nearest `buildl.toml` at or above the current directory and prints paths relative to it. A target is written `//dir:name`, or `:name` and `name`, which are read against the current directory.
   ~~~~

3. In `docs/design.md`, in workflow rule 3 below, say the log grows. Replace:

   ~~~~markdown
   3. **The log is an artifact.** `log.jsonl` records one sorted-key JSON object per action: target, command, hashes, duration, cache hit or miss.
   ~~~~

   With:

   ~~~~markdown
   3. **The log is an artifact.** `log.jsonl` records one sorted-key JSON object per action: target, command, hashes, duration, cache hit or miss. It grows across builds, and the newest line for a label is that target's current one.
   ~~~~

4. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '|`buildl graph`|Resolve|emit the DAG (`--json`, `--dot`)|' docs/design.md
   0
   $ grep -c -F -e '|`buildl graph`|Resolve|emit the DAG (`--json`, the default, or `--dot`)|' docs/design.md
   1
   # step 3
   $ grep -c -F -e '3. **The log is an artifact.** `log.jsonl` records one sorted-key JSON object per action: target' docs/design.md
   1
   $ grep -c -F -e 'ation, cache hit or miss. It grows across builds, and the newest line for a label is that target' docs/design.md
   1
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): bring the command table of design §7 up to the binary"
   ~~~~

### Task 6 — Say in design §8.5 where a tier 1 action writes its outputs

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Temp : Outputs to temp — never in place' docs/design.md
   1
   $ grep -c -F -e 'Temp : Outputs written — in place at tier 1, in a temp dir from tier 2' docs/design.md
   0
   # step 3
   $ grep -c -F -e 'not; the next run re-plans and continues. No "corrupt cache, run clean" state is representable.' docs/design.md
   1
   $ grep -c -F -e 'hine itself dying mid-build; the remedy then is to remove `.buildl/` and `out/` and build again.' docs/design.md
   0
   # step 4
   $ grep -c -F -e 'Hashing into the cas means every regular file the action left at an output path, then one manife' docs/design.md
   1
   $ grep -c -F -e 'Where an action writes depends on the tier. At tier 1 it runs in the workspace root and writes i' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the state diagram of §8.5, which stays Mermaid, replace the label of the `Temp` state. Replace:

   ~~~~markdown
       Temp : Outputs to temp — never in place
   ~~~~

   With:

   ~~~~markdown
       Temp : Outputs written — in place at tier 1, in a temp dir from tier 2
   ~~~~

3. In `docs/design.md`, in the paragraph below the diagram, say what the consistency promise covers. Replace:

   ~~~~markdown
   An interrupted build means some finished work is remembered and unfinished work is not; the next run re-plans and continues. No "corrupt cache, run clean" state is representable.
   ~~~~

   With:

   ~~~~markdown
   An interrupted build means some finished work is remembered and unfinished work is not; the next run re-plans and continues. For a build that is interrupted — stopped, killed, or ended with its terminal — no "corrupt cache, run clean" state is representable: every state file and every blob is renamed into place whole, and a row follows its content. Nothing is synced to disk, so that does not cover the machine itself dying mid-build; the remedy then is to remove `.buildl/` and `out/` and build again.
   ~~~~

4. In `docs/design.md`, add a paragraph before the one that begins "Hashing into the cas means". Replace:

   ~~~~markdown
   Hashing into the cas means every regular file the action left at an output path, then one manifest per output (§3).
   ~~~~

   With:

   ~~~~markdown
   Where an action writes depends on the tier. At tier 1 it runs in the workspace root and writes its outputs in place under `out/`; what it left there is then hashed into the cas and placed again from the cas, so a finished output is always a link into the store or a copy of it. An action that fails or is interrupted can leave a partial output at its path: no cache row names it, nothing reads it as a result, and the next build clears the path before it runs the action again (§12.2). From tier 2 an action writes in a directory of its own, and nothing partial reaches `out/`.

   Hashing into the cas means every regular file the action left at an output path, then one manifest per output (§3).
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Temp : Outputs to temp — never in place' docs/design.md
   0
   $ grep -c -F -e 'Temp : Outputs written — in place at tier 1, in a temp dir from tier 2' docs/design.md
   1
   # step 3
   $ grep -c -F -e 'not; the next run re-plans and continues. No "corrupt cache, run clean" state is representable.' docs/design.md
   0
   $ grep -c -F -e 'hine itself dying mid-build; the remedy then is to remove `.buildl/` and `out/` and build again.' docs/design.md
   1
   # step 4
   $ grep -c -F -e 'Hashing into the cas means every regular file the action left at an output path, then one manife' docs/design.md
   1
   $ grep -c -F -e 'Where an action writes depends on the tier. At tier 1 it runs in the workspace root and writes i' docs/design.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): say in design §8.5 where a tier 1 action writes its outputs"
   ~~~~

### Task 7 — Give design §8.7 the exit codes and where a build prints

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'A TTY gets a live status line with blocks as actions finish; CI gets plain sequential blocks; `-' docs/design.md
   1
   $ grep -c -F -e 'Everything a build prints about how it went — blocks, the status line, the summary — goes to std' docs/design.md
   0
   # step 3
   $ grep -c -F -e '|Action failure|the build did its job by revealing it|compile error, test failure, a declared ou' docs/design.md
   1
   $ grep -c -F -e '|Action failure|the build did its job by revealing it|compile error, test failure, a command end' docs/design.md
   0
   # step 4
   $ grep -c -F -e 'ess exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text.' docs/design.md
   1
   $ grep -c -F -e 'ess exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text:' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, at the end of the **Output** paragraph of §8.7, say where output goes, what `--stream` shows and when no summary is printed. Replace:

   ~~~~markdown
   A TTY gets a live status line with blocks as actions finish; CI gets plain sequential blocks; `--stream` attaches a single long action's output live.
   ~~~~

   With:

   ~~~~markdown
   Everything a build prints about how it went — blocks, the status line, the summary — goes to stderr; stdout carries only what a command was asked for, the graph of `graph` and the plan of `plan`, so a pipe never mixes the two. A TTY gets a live status line with blocks as actions finish; CI gets plain sequential blocks. Under `--stream` one running action at a time is shown as it prints, under its header, and every other action prints as a block once that one has ended: output is still never interleaved, and what is stored is what a quiet build stores. A build ends with one summary line, and an interrupted build prints none.
   ~~~~

3. In `docs/design.md`, in the failure-class table, add a command ended by a signal to the action failures. Replace:

   ~~~~markdown
   |Action failure|the build did its job by revealing it|compile error, test failure, a declared output left missing or left as something that cannot be kept|
   ~~~~

   With:

   ~~~~markdown
   |Action failure|the build did its job by revealing it|compile error, test failure, a command ended by a signal, a declared output left missing or left as something that cannot be kept|
   ~~~~

4. In `docs/design.md`, replace the sentence on exit codes with the table of codes. Replace:

   ~~~~markdown
   Each class maps to a distinct process exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text.
   ~~~~

   With:

   ~~~~markdown
   Each class maps to a distinct process exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text:

   |Code|Means|Given for|
   |---|---|---|
   |0|done|a command that did what it was asked; for `build`, every target built or served from the cache|
   |1|fix your code|an action failure: a non-zero exit, a command ended by a signal, an output missing or refused|
   |2|fix the command line|a flag or argument that is wrong, a target nothing declares, `--set` on a setting nothing declares|
   |3|fix your builder, or run again|an infrastructure failure: a missing tool, a command that cannot be started or runs past the timeout, an output that cannot be stored, a file of the store or the state that cannot be read or written, a workspace another buildl is using|
   |4|the build wants more than the ceiling|a policy refusal|
   |5|fix your build files or `buildl.toml`|a build file that fails, is missing or declares differently twice; a duplicate, an unknown reference or a cycle; an input that is missing or is not a regular file; two outputs at one path; a command that names no program; no `buildl.toml`, an invalid one, or a strategy this version cannot run (§4)|
   |6|a state file is not what buildl writes|`cache.json`, or a line of its journal, that is damaged or was edited (§3)|
   |7|buildl is at fault|an internal error|

   When a build has both, an infrastructure failure decides the code over an action failure: a sick builder means the run says nothing reliable about the code. A skipped target never decides it; the failure that caused the skip does. The codes stay clear of 126, 127 and 128+n, to which a shell gives meanings of its own, and an action's own exit status is never buildl's: it is in the failure report.
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'A TTY gets a live status line with blocks as actions finish; CI gets plain sequential blocks; `-' docs/design.md
   0
   $ grep -c -F -e 'Everything a build prints about how it went — blocks, the status line, the summary — goes to std' docs/design.md
   1
   # step 3
   $ grep -c -F -e '|Action failure|the build did its job by revealing it|compile error, test failure, a declared ou' docs/design.md
   0
   $ grep -c -F -e '|Action failure|the build did its job by revealing it|compile error, test failure, a command end' docs/design.md
   1
   # step 4
   $ grep -c -F -e 'ess exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text.' docs/design.md
   0
   $ grep -c -F -e 'ess exit code, so CI distinguishes "fix your code" from "fix your builder" without parsing text:' docs/design.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): give design §8.7 the exit codes and where a build prints"
   ~~~~

### Task 8 — Say in design §9.6 what a tool's fingerprint is

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'the host resolves it on `PATH` at plan time (missing tool → plan refusal, naming the target and' docs/design.md
   1
   $ grep -c -F -e 'the host resolves it on buildl' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the **Level 1** paragraph of §9.6, replace the clause on resolving and fingerprinting. Replace:

   ~~~~markdown
   the host resolves it on `PATH` at plan time (missing tool → plan refusal, naming the target and declaring file), fingerprints the resolved binary (content hash or version string), and folds the **tool fingerprint into the action key**. A toolchain upgrade dirties every affected target; machines with different versions never share cache entries.
   ~~~~

   With:

   ~~~~markdown
   the host resolves it on buildl's own `PATH` at plan time (missing tool → plan refusal, naming the target and declaring file), takes as its fingerprint the content hash of the file the name leads to, links followed, and folds the **tool fingerprint into the action key**. A name holding a `/` is not searched for: it is that path, taken from the workspace root when it is relative. The file run is the file fingerprinted, since one lookup serves both. A toolchain upgrade dirties every affected target; machines with different versions never share cache entries. A launcher that chooses the real tool when it runs — the `cc` stub of macOS, a `rustup` proxy — is fingerprinted as the launcher, so a change behind it is not seen, and neither is a change to a library a tool loads; a workspace that needs one seen names the real program by its path in the command and in `proc.run`, or declares the file that pins the version as an input.
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'the host resolves it on `PATH` at plan time (missing tool → plan refusal, naming the target and' docs/design.md
   0
   $ grep -c -F -e 'the host resolves it on buildl' docs/design.md
   1
   ~~~~

4. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

5. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): say in design §9.6 what a tool's fingerprint is"
   ~~~~

### Task 9 — State in design §12 what tier 1 guarantees and what it does not

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Parallel actions are isolated from each other by construction: own exec dir, own temp outputs, a' docs/design.md
   1
   $ grep -c -F -e 'From tier 2, parallel actions are isolated from each other by construction: own exec dir, own te' docs/design.md
   0
   # step 3
   $ grep -c -F -e '|Hidden env, locale, timezone variation|**mitigated** — sandbox sets `TZ=UTC`, `LC_ALL=C`, `SOUR' docs/design.md
   1
   $ grep -c -F -e '|Hidden env, locale, timezone variation|**mitigated** from tier 2 up — the sandbox sets `TZ=UTC`' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, at the end of the paragraph below the ladder of §12.2, say from which tier parallel actions are isolated, and add the paragraph on tier 1. Replace:

   ~~~~markdown
   Parallel actions are isolated from each other by construction: own exec dir, own temp outputs, atomic landing in the cas.
   ~~~~

   With:

   ~~~~markdown
   From tier 2, parallel actions are isolated from each other by construction: own exec dir, own temp outputs, atomic landing in the cas. At tier 1 they share the workspace root, and are kept apart only by each owning its own output paths (§3).

   **Tier 1, `bare`, runs an action as a plain process in the workspace root.** It guarantees five things: the command is started as an argument vector, never through a shell; its program, when the command writes it out or names it through a build setting, is the file the plan fingerprinted; its environment holds exactly the variables the target declared that have a value, and nothing of the host's; its standard input is empty; and its own process is stopped when it runs past the workspace's `timeout`. A program the command names through `$in`, `$out` or `$deps` has no fingerprint: it is known only once the command is expanded, when its expanded text is held to `proc.run` and the file is found by the rule every program is found by. Named through `$deps` it is a dependency's output under `out/`, the file that dependency's build placed, and its bytes are in this action's key. Named through `$in` it is the input file, its digest in the key, when the input's path holds a `/`; an input at the workspace root is a bare name, looked for on buildl's `PATH` like any other and never in the workspace. `$out` names the action's own output, which is not there when the action starts. Tier 1 confines nothing. An action can read any file the user can, declared or not, so a hidden input is silent instead of an error; it can reach the network whatever `network` says; it can write anywhere the user can, another target's output included; the processes it starts are not stopped with it, at a timeout or at its exit; and it writes its outputs in place, so an action that fails, times out or is interrupted can leave a partial output at its path. No cache row names such a file, buildl never reads it as a result, and the next build clears the path before it runs the action again. A dependency's output under `out/` is read-only, and a tool that changes its mode first can damage the store. The machine's clock, time zone and locale are the action's. Tier 2 is where an undeclared input becomes an error, and tier 3 where the network does.
   ~~~~

3. In `docs/design.md`, in the table of §12.3, say from which tier the environment is pinned. Replace:

   ~~~~markdown
   |Hidden env, locale, timezone variation|**mitigated** — sandbox sets `TZ=UTC`, `LC_ALL=C`, `SOURCE_DATE_EPOCH` [3]|
   ~~~~

   With:

   ~~~~markdown
   |Hidden env, locale, timezone variation|**mitigated** from tier 2 up — the sandbox sets `TZ=UTC`, `LC_ALL=C`, `SOURCE_DATE_EPOCH` [3]; at tier 1 the time zone, the locale and the clock are the machine's|
   ~~~~

4. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'Parallel actions are isolated from each other by construction: own exec dir, own temp outputs, a' docs/design.md
   0
   $ grep -c -F -e 'From tier 2, parallel actions are isolated from each other by construction: own exec dir, own te' docs/design.md
   1
   # step 3
   $ grep -c -F -e '|Hidden env, locale, timezone variation|**mitigated** — sandbox sets `TZ=UTC`, `LC_ALL=C`, `SOUR' docs/design.md
   0
   $ grep -c -F -e '|Hidden env, locale, timezone variation|**mitigated** from tier 2 up — the sandbox sets `TZ=UTC`' docs/design.md
   1
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): state in design §12 what tier 1 guarantees and what it does not"
   ~~~~

### Task 10 — Give every command of design §7 a place in the sequence of §13

**Files:**
- Modify `docs/design.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos.' docs/design.md
   1
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos. With it the commands aroun' docs/design.md
   0
   # step 3
   $ grep -c -F -e '3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on.' docs/design.md
   1
   $ grep -c -F -e '3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on. `' docs/design.md
   0
   # step 4
   $ grep -c -F -e '4. **Grant negotiation UX** — wanted set, ceiling intersection, approver. Consumes the airsl ext' docs/design.md
   1
   $ grep -c -F -e '4. **Grant negotiation UX** — wanted set, ceiling intersection, approver, and `doctor`, which pr' docs/design.md
   0
   # step 5
   $ grep -c -F -e '5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format.' docs/design.md
   1
   $ grep -c -F -e '5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format. Disco' docs/design.md
   0
   ~~~~

2. In `docs/design.md`, in the numbered list of §13, add two sentences to item 2. No item is rewritten: each edit of this task only adds words. Replace:

   ~~~~markdown
   2. **`query` / `rdeps`** — small work, transforms CI for large repos.
   ~~~~

   With:

   ~~~~markdown
   2. **`query` / `rdeps`** — small work, transforms CI for large repos. With it the commands around a build: `test`, `log`, `clean` and `gc` (§7). Demand-driven and parallel loading of build files (§11.1) land here.
   ~~~~

3. In `docs/design.md`, add one sentence to item 3. Replace:

   ~~~~markdown
   3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on.
   ~~~~

   With:

   ~~~~markdown
   3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on. `verify` (§12.3) lands here.
   ~~~~

4. In `docs/design.md`, add `doctor` and one sentence to item 4. Replace:

   ~~~~markdown
   4. **Grant negotiation UX** — wanted set, ceiling intersection, approver. Consumes the airsl extension host's ceiling + `Approver` (buildl is their first consumer; the manifest parser and `ext.on` dispatcher are not needed yet).
   ~~~~

   With:

   ~~~~markdown
   4. **Grant negotiation UX** — wanted set, ceiling intersection, approver, and `doctor`, which prints the result (§7). Consumes the airsl extension host's ceiling + `Approver` (buildl is their first consumer; the manifest parser and `ext.on` dispatcher are not needed yet). Per-target grant narrowing (§14) is settled here.
   ~~~~

5. In `docs/design.md`, add one sentence to item 5. Replace:

   ~~~~markdown
   5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format.
   ~~~~

   With:

   ~~~~markdown
   5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format. Discovery targets (§10) land here.
   ~~~~

6. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos.' docs/design.md
   1
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos. With it the commands aroun' docs/design.md
   1
   # step 3
   $ grep -c -F -e '3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on.' docs/design.md
   1
   $ grep -c -F -e '3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on. `' docs/design.md
   1
   # step 4
   $ grep -c -F -e '4. **Grant negotiation UX** — wanted set, ceiling intersection, approver. Consumes the airsl ext' docs/design.md
   0
   $ grep -c -F -e '4. **Grant negotiation UX** — wanted set, ceiling intersection, approver, and `doctor`, which pr' docs/design.md
   1
   # step 5
   $ grep -c -F -e '5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format.' docs/design.md
   1
   $ grep -c -F -e '5. **Plugins** — manifest, attribution, `b.use()`. Consumes the extension manifest format. Disco' docs/design.md
   1
   ~~~~

7. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

8. Commit:

   ~~~~text
   $ git add docs/design.md
   $ git commit -m "docs(repo): give every command of design §7 a place in the sequence of §13"
   ~~~~

### Task 11 — Say in the architecture that build files are loaded one at a time

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Parallel load: the directory queue is processed by a small pool;' docs/architecture.md
   1
   $ grep -c -F -e 'Load evaluates build files one at a time today, from one queue of directories. Parallel load is' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 'Parallel load merges sorted (§3.1);' docs/architecture.md
   1
   $ grep -c -F -e 'Parallel load merges sorted (§3.1) — Load is sequential today, so this clause has nothing to hol' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in §3.1, put two sentences before the paragraph on parallel load, which stays as its design. Replace:

   ~~~~markdown
   Parallel load: the directory queue is processed by a small pool;
   ~~~~

   With:

   ~~~~markdown
   Load evaluates build files one at a time today, from one queue of directories. Parallel load is not built with the core pipeline — the design doc's §13 says where it lands — and the rest of this paragraph is its design. Parallel load: the directory queue is processed by a small pool;
   ~~~~

3. In `docs/architecture.md`, in §5 rule 3, say the clause on parallel load has nothing to hold yet. Replace:

   ~~~~markdown
   Parallel load merges sorted (§3.1);
   ~~~~

   With:

   ~~~~markdown
   Parallel load merges sorted (§3.1) — Load is sequential today, so this clause has nothing to hold yet;
   ~~~~

4. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Parallel load: the directory queue is processed by a small pool;' docs/architecture.md
   1
   $ grep -c -F -e 'Load evaluates build files one at a time today, from one queue of directories. Parallel load is' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 'Parallel load merges sorted (§3.1);' docs/architecture.md
   0
   $ grep -c -F -e 'Parallel load merges sorted (§3.1) — Load is sequential today, so this clause has nothing to hol' docs/architecture.md
   1
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): say in the architecture that build files are loaded one at a time"
   ~~~~

### Task 12 — Describe the plan-side adapters in architecture §3.3

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Then, once per graph: one fingerprint per distinct program (`ToolResolver`), one digest per dist' docs/architecture.md
   1
   $ grep -c -F -e 'Then, once per graph: one fingerprint per distinct program (`ToolResolver`) — the SHA-256 of the' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 's, tested against fakes, and the adapter only stores; Plan commits the cache once when it record' docs/architecture.md
   1
   $ grep -c -F -e 's, tested against fakes. The adapter stores, keyed by path alone, with two rules of its own. It' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 's pool is confined to the `Digester` adapter in `buildl` (decision record §6).' docs/architecture.md
   1
   $ grep -c -F -e 's pool is confined to the `Digester` adapter in `buildl` (decision record §6). No file is read w' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the first paragraph of §3.3, say what a tool's fingerprint is. Replace:

   ~~~~markdown
   Then, once per graph: one fingerprint per distinct program (`ToolResolver`), one digest per distinct input (`Digester`, through the stat cache), the platform and one value per env name (`Host`).
   ~~~~

   With:

   ~~~~markdown
   Then, once per graph: one fingerprint per distinct program (`ToolResolver`) — the SHA-256 of the bytes of the file the name leads to on buildl's own `PATH`, links followed, with no path, size or time in it, and taken again on every plan — one digest per distinct input (`Digester`, through the stat cache), the platform and one value per env name (`Host`).
   ~~~~

3. In `docs/architecture.md`, in the paragraph on the stat cache, replace the sentence that says the adapter only stores. Replace:

   ~~~~markdown
   The reuse rule is Plan's, tested against fakes, and the adapter only stores; Plan commits the cache once when it recorded something and not at all otherwise.
   ~~~~

   With:

   ~~~~markdown
   The reuse rule is Plan's, tested against fakes. The adapter stores, keyed by path alone, with two rules of its own. It keeps an entry only for a file last modified more than two seconds before the run began: an edit inside a filesystem's timestamp granule can leave modification time and size equal, and two seconds is the coarsest granule met. And it discards a file it cannot read back instead of failing on it, since the file holds nothing a hash cannot give again. Plan commits the cache once when it recorded something and not at all otherwise.
   ~~~~

4. In `docs/architecture.md`, at the end of the same paragraph, say how files are hashed and what `stat` reports. Replace:

   ~~~~markdown
   rayon's pool is confined to the `Digester` adapter in `buildl` (decision record §6).
   ~~~~

   With:

   ~~~~markdown
   rayon's pool is confined to the `Digester` adapter in `buildl` (decision record §6). No file is read whole: each is streamed in pieces through `Hasher`, `buildl-core`'s incremental form of the one SHA-256. Digests come back in the order asked, and a failure is always the one for the first failing path in that order, whatever the scheduling. Before anything is hashed, `Digester::stat` says what stands at each input's path, following no symbolic link: a regular file with its metadata; nothing, which Plan raises as `Error::MissingInput`; or a link, a directory or a special file, which Plan raises as `Error::InputNotAFile`, with the target that declares the input.
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'Then, once per graph: one fingerprint per distinct program (`ToolResolver`), one digest per dist' docs/architecture.md
   0
   $ grep -c -F -e 'Then, once per graph: one fingerprint per distinct program (`ToolResolver`) — the SHA-256 of the' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 's, tested against fakes, and the adapter only stores; Plan commits the cache once when it record' docs/architecture.md
   0
   $ grep -c -F -e 's, tested against fakes. The adapter stores, keyed by path alone, with two rules of its own. It' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 's pool is confined to the `Digester` adapter in `buildl` (decision record §6).' docs/architecture.md
   1
   $ grep -c -F -e 's pool is confined to the `Digester` adapter in `buildl` (decision record §6). No file is read w' docs/architecture.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): describe the plan-side adapters in architecture §3.3"
   ~~~~

### Task 13 — Describe the reporter, live output and the bare strategy in architecture §3.4

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '`work` runs one task and may run on any thread, several at once; `step` runs on the calling thre' docs/architecture.md
   1
   $ grep -c -F -e '`work` runs one task and may run on any thread, several at once, and is handed a way to send mes' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 'a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a task, run it throu' docs/architecture.md
   1
   $ grep -c -F -e 'a ready queue (`std::sync::mpsc` channel) and up to N scoped worker threads, started as tasks ne' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 'A worker prepares the exec dir, runs the action through the strategy (below),' docs/architecture.md
   1
   $ grep -c -F -e 'A worker first says it has begun — a target counts as started when a worker takes it, not when i' docs/architecture.md
   0
   # step 5
   $ grep -c -F -e 'fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;' docs/architecture.md
   1
   $ grep -c -F -e '/// Spawn and wait, capturing output. No shell, ever. With `live`, every run of' docs/architecture.md
   0
   # step 6
   $ grep -c -F -e 'The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace' docs/architecture.md
   1
   $ grep -c -F -e 'The trait is a `buildl-core` port, and `buildl` ships one adapter today: `Bare` (tier 1: workspa' docs/architecture.md
   0
   # step 7
   $ grep -c -F -e '`run` returns a `RawOutcome`: how the action ended — an exit status, or a timeout — and what it' docs/architecture.md
   1
   $ grep -c -F -e '`run` returns a `RawOutcome`: how the action ended — an exit status, a timeout, or the signal th' docs/architecture.md
   0
   # step 8
   $ grep -c -F -e 'Output capture: piped stdout/stderr read by two threads per action into buffers, blobs written t' docs/architecture.md
   1
   $ grep -c -F -e 'Output capture: piped stdout/stderr read by two reader threads per action, which send what they' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the first paragraph of §3.4, give `work` and `step` the messages they carry. Replace:

   ~~~~markdown
   `work` runs one task and may run on any thread, several at once; `step` runs on the calling thread, first with nothing and then with each result, and returns the tasks that became ready. The port is generic over the task and the result, so it knows nothing of builds.
   ~~~~

   With:

   ~~~~markdown
   `work` runs one task and may run on any thread, several at once, and is handed a way to send messages back while it runs; `step` runs on the calling thread, first with `Turn::Begin`, then with each message (`Turn::Told`) and each result (`Turn::Done`), and returns the tasks that became ready. The port is generic over the task, the message and the result, so it knows nothing of builds.
   ~~~~

3. In `docs/architecture.md`, in the same paragraph, describe the thread pool as it is. Replace:

   ~~~~markdown
   a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a task, run it through `work`, and send the result back on a results channel.
   ~~~~

   With:

   ~~~~markdown
   a ready queue (`std::sync::mpsc` channel) and up to N scoped worker threads, started as tasks need them — a build of cached targets starts none — that receive a task, run it through `work`, and send its messages and its result back on one channel. A task that panics is reported lost instead of taking its worker down, and the build turns the loss into `Error::DispatcherFault` (below).
   ~~~~

4. In `docs/architecture.md`, in the paragraph that begins "What runs where", say a worker first says it began. Replace:

   ~~~~markdown
   A worker prepares the exec dir, runs the action through the strategy (below),
   ~~~~

   With:

   ~~~~markdown
   A worker first says it has begun — a target counts as started when a worker takes it, not when it is handed out — then prepares the exec dir, runs the action through the strategy (below),
   ~~~~

5. In `docs/architecture.md`, in the `ExecStrategy` block, give `run` its `live` argument. Replace:

   ~~~~markdown
       /// Spawn and wait, capturing output. No shell, ever.
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
   ~~~~

   With:

   ~~~~markdown
       /// Spawn and wait, capturing output. No shell, ever. With `live`, every run of
       /// bytes the command writes is also handed to it as it is read.
       fn run(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome>;
   ~~~~

6. In `docs/architecture.md`, in the paragraph below the block, name the one strategy that ships and how it finds its program. Replace:

   ~~~~markdown
   The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace cwd, granted env only) and `InputSandbox` (tier 2: temp dir, symlinked declared inputs). `OsSandbox` and `Container` are later impls of the same trait — the trait is in v1 so the ladder is a seam, not a refactor.
   ~~~~

   With:

   ~~~~markdown
   The trait is a `buildl-core` port, and `buildl` ships one adapter today: `Bare` (tier 1: workspace cwd, granted env only). `InputSandbox` (tier 2: temp dir, symlinked declared inputs) arrives with item 3 of the design doc's §13, and `OsSandbox` and `Container` are later impls of the same trait — the trait is in v1 so the ladder is a seam, not a refactor. `Bare` finds its program through the search path the plan's tools were fingerprinted through, one value handed to both adapters, so the file run is the file fingerprinted.
   ~~~~

7. In `docs/architecture.md`, at the end of the same paragraph, say how a run can end and what `live` is for. Replace:

   ~~~~markdown
   `run` returns a `RawOutcome`: how the action ended — an exit status, or a timeout — and what it printed.
   ~~~~

   With:

   ~~~~markdown
   `run` returns a `RawOutcome`: how the action ended — an exit status, a timeout, or the signal that ended it — and what it printed. Its `live` argument is how `--stream` works: when it is given, every run of bytes the command writes is also handed to it as it is read, with its channel, and the outcome returned is the same with or without it, so what is stored cannot differ.
   ~~~~

8. In `docs/architecture.md`, replace the last paragraph of §3.4, on output capture and printing. Replace:

   ~~~~markdown
   Output capture: piped stdout/stderr read by two threads per action into buffers, blobs written to the cas, digests recorded on the outcome. Printing is owned by a single reporter on the scheduler side (block per completed action; TTY status line) — workers never touch the terminal.
   ~~~~

   With:

   ~~~~markdown
   Output capture: piped stdout/stderr read by two reader threads per action, which send what they read to the worker over one channel; the worker appends each chunk to its stream's buffer, so each stream's order is exact, and it alone touches the child. The worker enforces the timeout: it waits with a deadline read from the monotonic clock and kills the child when the deadline passes — the child alone, since tier 1 makes no process group (decision record §6). When the child is gone and something it started still holds a pipe, the readers get one second more, and the action ends with what arrived. Blobs are written to the cas, digests recorded on the outcome.

   Printing is owned by a single reporter on the scheduler side — workers never touch the terminal. `Reporter` is a `buildl-core` port of five calls, each made on the thread that called `build` and none able to fail: the build began, an action started, the action shown live printed, a target finished — with its log event and what it printed, read back from the cas for a cached target as for a built one — and the summary. A worker reaches the reporter only through the dispatcher's messages. An event is appended to the log before anything is read for display, so "log always" holds whatever the reporter does. The one adapter is a terminal reporter with two modes, a live one that keeps a status line and a plain one for a pipe; it is lent to one build by whoever calls it, and a build that ends in an error gets no summary.
   ~~~~

9. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e '`work` runs one task and may run on any thread, several at once; `step` runs on the calling thre' docs/architecture.md
   0
   $ grep -c -F -e '`work` runs one task and may run on any thread, several at once, and is handed a way to send mes' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 'a ready queue (`std::sync::mpsc` channel) and N worker threads that receive a task, run it throu' docs/architecture.md
   0
   $ grep -c -F -e 'a ready queue (`std::sync::mpsc` channel) and up to N scoped worker threads, started as tasks ne' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 'A worker prepares the exec dir, runs the action through the strategy (below),' docs/architecture.md
   0
   $ grep -c -F -e 'A worker first says it has begun — a target counts as started when a worker takes it, not when i' docs/architecture.md
   1
   # step 5
   $ grep -c -F -e 'fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;' docs/architecture.md
   0
   $ grep -c -F -e '/// Spawn and wait, capturing output. No shell, ever. With `live`, every run of' docs/architecture.md
   1
   # step 6
   $ grep -c -F -e 'The trait is a `buildl-core` port, and `buildl` ships the v1 adapters: `Bare` (tier 1: workspace' docs/architecture.md
   0
   $ grep -c -F -e 'The trait is a `buildl-core` port, and `buildl` ships one adapter today: `Bare` (tier 1: workspa' docs/architecture.md
   1
   # step 7
   $ grep -c -F -e '`run` returns a `RawOutcome`: how the action ended — an exit status, or a timeout — and what it' docs/architecture.md
   0
   $ grep -c -F -e '`run` returns a `RawOutcome`: how the action ended — an exit status, a timeout, or the signal th' docs/architecture.md
   1
   # step 8
   $ grep -c -F -e 'Output capture: piped stdout/stderr read by two threads per action into buffers, blobs written t' docs/architecture.md
   0
   $ grep -c -F -e 'Output capture: piped stdout/stderr read by two reader threads per action, which send what they' docs/architecture.md
   1
   ~~~~

10. Run the gate; both commands exit `0`:

    ~~~~text
    $ cargo make dod
    $ cargo deny check
    ~~~~

11. Commit:

    ~~~~text
    $ git add docs/architecture.md
    $ git commit -m "docs(repo): describe the reporter, live output and the bare strategy in architecture §3.4"
    ~~~~

### Task 14 — Describe the store on disk in architecture §3.5

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'The cas holds bytes by their digest and nothing else: whether a file may be run is not stored wi' docs/architecture.md
   1
   $ grep -c -F -e 'Blobs are staged in `.buildl/cas/tmp/` and state files in `.buildl/` itself, and both places are' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e '`out/` entries are hard links into the cas (copy fallback for filesystems that refuse); a hard l' docs/architecture.md
   1
   $ grep -c -F -e '`out/` entries are hard links into the cas: to the blob for a plain file and to its twin for an' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 'a dependent keys on its dependency' docs/architecture.md
   1
   $ grep -c -F -e 's therefore in the journal alone, and `cache.json` holds the same bytes whatever that order was.' docs/architecture.md
   0
   # step 5
   $ grep -c -F -e '`gc` walks roots (current `graph.json` outputs + cache rows),' docs/architecture.md
   1
   $ grep -c -F -e '`gc` is designed and not built: it arrives with item 2 of the design doc' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the first paragraph of §3.5, add the staging places and the no-sync rule before the sentence on what the cas holds, and the modes, the twin, `put` and `get` after it. Replace:

   ~~~~markdown
   The cas holds bytes by their digest and nothing else: whether a file may be run is not stored with it, and is said again each time the file is placed.
   ~~~~

   With:

   ~~~~markdown
   Blobs are staged in `.buildl/cas/tmp/` and state files in `.buildl/` itself, and both places are swept when a run takes the state lock. Nothing is synced to disk: a rename already gives a reader the old file whole or the new one whole when the process dies, which is what design doc §8.5 promises (decision record §6). The cas holds bytes by their digest and nothing else: whether a file may be run is not stored with it, and is said again each time the file is placed. A blob is read-only, mode 0444. One that was ever placed executable has a twin beside it, `cdef....x`: the same bytes with mode 0555, because a mode belongs to the inode. `put` compares the blob, and a twin that is there, with the bytes in hand and writes again what differs, removing neither; only a placement makes a twin. So the files the store holds after a build depend on the graph alone, never on the order workers finished in. `get` checks what it reads against the digest it was asked for: content that no longer has its digest is removed and reported as not held.
   ~~~~

3. In `docs/architecture.md`, in the paragraph on putting an output in place, say when an entry of `out/` is a link and when a copy. Replace:

   ~~~~markdown
   `out/` entries are hard links into the cas (copy fallback for filesystems that refuse); a hard link shares its inode's mode, so a blob placed both as plain and as executable needs a second inode or a copy with the mode set.
   ~~~~

   With:

   ~~~~markdown
   `out/` entries are hard links into the cas: to the blob for a plain file and to its twin for an executable one, since a hard link shares its inode's mode. A link is used only where it protects the store. Once per store the adapter writes a read-only file and tries to open it for writing; where that succeeds — as root, or on a filesystem without permission bits — every placement is a copy. A filesystem that refuses a link gets a copy too.
   ~~~~

4. In `docs/architecture.md`, in the paragraph on the action cache, say what the cache is on disk. Replace:

   ~~~~markdown
   a dependent keys on its dependency's row, so the row must be safe before the dependent starts, and a write that fails leaves no row.
   ~~~~

   With:

   ~~~~markdown
   a dependent keys on its dependency's row, so the row must be safe before the dependent starts, and a write that fails leaves no row. On disk the cache is two files: `cache.json`, the canonical snapshot, and `cache.journal`, to which `record` appends one line per row, because rewriting the snapshot for every row would be quadratic in the number of targets. The journal is folded into the snapshot once, after a build has returned, whatever it returned; one an interrupted run left behind is folded when the cache is next opened, a cut last line left out. The order workers finished in is therefore in the journal alone, and `cache.json` holds the same bytes whatever that order was.
   ~~~~

5. In `docs/architecture.md`, in the last paragraph of §3.5, say `gc` is not built. Replace:

   ~~~~markdown
   `gc` walks roots (current `graph.json` outputs + cache rows),
   ~~~~

   With:

   ~~~~markdown
   `gc` is designed and not built: it arrives with item 2 of the design doc's §13, and until then the store, the log and the rows of targets no longer declared only grow. Its design: `gc` walks roots (current `graph.json` outputs + cache rows),
   ~~~~

6. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'The cas holds bytes by their digest and nothing else: whether a file may be run is not stored wi' docs/architecture.md
   1
   $ grep -c -F -e 'Blobs are staged in `.buildl/cas/tmp/` and state files in `.buildl/` itself, and both places are' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e '`out/` entries are hard links into the cas (copy fallback for filesystems that refuse); a hard l' docs/architecture.md
   0
   $ grep -c -F -e '`out/` entries are hard links into the cas: to the blob for a plain file and to its twin for an' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 'a dependent keys on its dependency' docs/architecture.md
   1
   $ grep -c -F -e 's therefore in the journal alone, and `cache.json` holds the same bytes whatever that order was.' docs/architecture.md
   1
   # step 5
   $ grep -c -F -e '`gc` walks roots (current `graph.json` outputs + cache rows),' docs/architecture.md
   1
   $ grep -c -F -e '`gc` is designed and not built: it arrives with item 2 of the design doc' docs/architecture.md
   1
   ~~~~

7. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

8. Commit:

   ~~~~text
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): describe the store on disk in architecture §3.5"
   ~~~~

### Task 15 — Add the new class, the new errors and the exit codes to architecture §4

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'ActionFailure { status: i32 },          // the build did its job' docs/architecture.md
   1
   $ grep -c -F -e 'Killed { signal: i32 },                 // ended by a signal: no exit status' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e '`OutputMissing` is the build revealing a fault, like `ActionFailure`:' docs/architecture.md
   1
   $ grep -c -F -e '`Killed` is an action failure too: a command a signal ended has no exit status to hold, and it c' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 'The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim.' docs/architecture.md
   1
   $ grep -c -F -e 'The CLI maps every class and every `Error` variant to one of eight exit codes (design doc §8.7)' docs/architecture.md
   0
   # step 5
   $ grep -c -F -e 'Three kinds carry data a caller branches on,' docs/architecture.md
   1
   $ grep -c -F -e 'Six kinds carry data a caller branches on,' docs/architecture.md
   0
   # step 6
   $ grep -c -F -e '|`UnreadableJson`, `NonCanonicalJson`|bytes handed to the canonical reader cannot be trusted (§5' docs/architecture.md
   1
   $ grep -c -F -e '|`CorruptState`|a file an adapter keeps a workspace' docs/architecture.md
   0
   # step 7
   $ grep -c -F -e 'file from one it could not read: the first is one of these, the second its own `Error::Adapter`.' docs/architecture.md
   1
   $ grep -c -F -e 'wrapped in `CorruptState` with the name of the file, and the second is its own `Error::Adapter`.' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the `OutcomeClass` listing of §4, add the class of a command a signal ended. Replace:

   ~~~~markdown
       ActionFailure { status: i32 },          // the build did its job
   ~~~~

   With:

   ~~~~markdown
       ActionFailure { status: i32 },          // the build did its job
       Killed { signal: i32 },                 // ended by a signal: no exit status
   ~~~~

3. In `docs/architecture.md`, in the paragraph below the listing, describe `Killed`. Replace:

   ~~~~markdown
   `OutputMissing` is the build revealing a fault, like `ActionFailure`:
   ~~~~

   With:

   ~~~~markdown
   `Killed` is an action failure too: a command a signal ended has no exit status to hold, and it counts as failed. `OutputMissing` is the build revealing a fault, like `ActionFailure`:
   ~~~~

4. In `docs/architecture.md`, in the same paragraph, say how classes and errors become exit codes. Replace:

   ~~~~markdown
   The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim.
   ~~~~

   With:

   ~~~~markdown
   The CLI maps every class and every `Error` variant to one of eight exit codes (design doc §8.7) by matching on the variant, never on text, and one the binary has no row for exits 7, as buildl's own fault; `log.jsonl` carries the class verbatim.
   ~~~~

5. In `docs/architecture.md`, in the paragraph above the error table, count six kinds. Replace:

   ~~~~markdown
   Three kinds carry data a caller branches on,
   ~~~~

   With:

   ~~~~markdown
   Six kinds carry data a caller branches on,
   ~~~~

6. In `docs/architecture.md`, add three rows at the end of the error table. Replace:

   ~~~~markdown
   |`UnreadableJson`, `NonCanonicalJson`|bytes handed to the canonical reader cannot be trusted (§5 rule 2)|`source`, the `serde_json` error with its line and column; or `offset`, the first byte that differs from the canonical form|
   ~~~~

   With:

   ~~~~markdown
   |`UnreadableJson`, `NonCanonicalJson`|bytes handed to the canonical reader cannot be trusted (§5 rule 2)|`source`, the `serde_json` error with its line and column; or `offset`, the first byte that differs from the canonical form|
   |`CorruptState`|a file an adapter keeps a workspace's state in does not hold what the adapter writes there (§3.5)|`file`, and `source`: the JSON variant that says why|
   |`UnknownTarget`|a run was asked for a target, and no target or alias has that label|the label, and the nearest declared one as a suggestion|
   |`InputNotAFile`|a declared input's path holds a symbolic link, a directory or a special file (§3.3)|the lowest-labelled target declaring the input with its build file, the path, and what was found|
   ~~~~

7. In `docs/architecture.md`, in the paragraph below the table, say the JSON variants reach a caller inside `CorruptState`. Replace:

   ~~~~markdown
   The two JSON variants are how a storage adapter tells a corrupt file from one it could not read: the first is one of these, the second its own `Error::Adapter`.
   ~~~~

   With:

   ~~~~markdown
   The two JSON variants are how a storage adapter tells a corrupt file from one it could not read: the first is one of these, wrapped in `CorruptState` with the name of the file, and the second is its own `Error::Adapter`.
   ~~~~

8. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'ActionFailure { status: i32 },          // the build did its job' docs/architecture.md
   1
   $ grep -c -F -e 'Killed { signal: i32 },                 // ended by a signal: no exit status' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e '`OutputMissing` is the build revealing a fault, like `ActionFailure`:' docs/architecture.md
   1
   $ grep -c -F -e '`Killed` is an action failure too: a command a signal ended has no exit status to hold, and it c' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 'The CLI maps classes to exit codes; `log.jsonl` carries the class verbatim.' docs/architecture.md
   0
   $ grep -c -F -e 'The CLI maps every class and every `Error` variant to one of eight exit codes (design doc §8.7)' docs/architecture.md
   1
   # step 5
   $ grep -c -F -e 'Three kinds carry data a caller branches on,' docs/architecture.md
   0
   $ grep -c -F -e 'Six kinds carry data a caller branches on,' docs/architecture.md
   1
   # step 6
   $ grep -c -F -e '|`UnreadableJson`, `NonCanonicalJson`|bytes handed to the canonical reader cannot be trusted (§5' docs/architecture.md
   1
   $ grep -c -F -e '|`CorruptState`|a file an adapter keeps a workspace' docs/architecture.md
   1
   # step 7
   $ grep -c -F -e 'file from one it could not read: the first is one of these, the second its own `Error::Adapter`.' docs/architecture.md
   0
   $ grep -c -F -e 'wrapped in `CorruptState` with the name of the file, and the second is its own `Error::Adapter`.' docs/architecture.md
   1
   ~~~~

9. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

10. Commit:

    ~~~~text
    $ git add docs/architecture.md
    $ git commit -m "docs(repo): add the new class, the new errors and the exit codes to architecture §4"
    ~~~~

### Task 16 — Bring determinism rules 4 and 5 of the architecture up to the adapters

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'via the `Clock` port, whose only real adapter lives in `buildl`.' docs/architecture.md
   1
   $ grep -c -F -e 'via the `Clock` port, whose only real adapter lives in `buildl`. Two other time values exist, an' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 'the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.' docs/architecture.md
   1
   $ grep -c -F -e 'the test suite builds each of two fixture workspaces twice — in two directories, once with four' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in §5 rule 4, name the two other time values an adapter reads. Replace:

   ~~~~markdown
   via the `Clock` port, whose only real adapter lives in `buildl`.
   ~~~~

   With:

   ~~~~markdown
   via the `Clock` port, whose only real adapter lives in `buildl`. Two other time values exist, and neither is a wall-clock reading that reaches an output: the `exec` adapter reads the monotonic clock for an action's deadline, a value that reaches no key, file or log; and the stat cache's "began" (§3.3) is a file's modification time, the one the filesystem gave the state lock.
   ~~~~

3. In `docs/architecture.md`, in §5 rule 5, say what the double-build test compares, name it, and say what it leaves out. Replace:

   ~~~~markdown
   the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.json`, and cas contents.
   ~~~~

   With:

   ~~~~markdown
   the test suite builds each of two fixture workspaces twice — in two directories, once with four workers and once with one — and asserts byte-identical `out/`, `graph.json`, `cache.json`, and cas contents (`two_clean_builds_in_two_directories_are_byte_identical`, in `crates/buildl/tests/e2e`). `log.jsonl` and `statcache.json` are left out of the comparison: the first holds clock readings and completion order, the second modification times.
   ~~~~

4. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'via the `Clock` port, whose only real adapter lives in `buildl`.' docs/architecture.md
   1
   $ grep -c -F -e 'via the `Clock` port, whose only real adapter lives in `buildl`. Two other time values exist, an' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 'the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.' docs/architecture.md
   0
   $ grep -c -F -e 'the test suite builds each of two fixture workspaces twice — in two directories, once with four' docs/architecture.md
   1
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): bring determinism rules 4 and 5 of the architecture up to the adapters"
   ~~~~

### Task 17 — Record the adapter decisions in architecture §6

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'so a file that fails is `Error::UnreadableJson` or `Error::NonCanonicalJson` — corrupt, and dist' docs/architecture.md
   1
   $ grep -c -F -e 'so a file that fails surfaces as `Error::CorruptState`, which names the file and wraps `Error::U' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 'Nothing in Plan reads it, so editing only a `desc` leaves every target cached (§3.4).|' docs/architecture.md
   1
   $ grep -c -F -e '|How a running build reaches the reporter|**Messages through the dispatcher: `work(task, tell)`' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the "Cache files" row of §6, name the error a failing file surfaces as. Replace:

   ~~~~markdown
   so a file that fails is `Error::UnreadableJson` or `Error::NonCanonicalJson` — corrupt, and distinct from an `Error::Adapter` read failure (§3.5, §4).|
   ~~~~

   With:

   ~~~~markdown
   so a file that fails surfaces as `Error::CorruptState`, which names the file and wraps `Error::UnreadableJson` or `Error::NonCanonicalJson` — corrupt, and distinct from an `Error::Adapter` read failure (§3.5, §4).|
   ~~~~

3. In `docs/architecture.md`, add fifteen rows at the end of the table, after its last row. Replace:

   ~~~~markdown
   Nothing in Plan reads it, so editing only a `desc` leaves every target cached (§3.4).|
   ~~~~

   With:

   ~~~~markdown
   Nothing in Plan reads it, so editing only a `desc` leaves every target cached (§3.4).|
   |How a running build reaches the reporter|**Messages through the dispatcher: `work(task, tell)` and `step(Turn)`**|The scheduling thread blocks inside `dispatch`, and the dispatcher's own channel is the only way in that needs no second consumer. A target is started when a worker says it began, not when it is handed out: forty ready tasks on eight workers would otherwise show forty running (§3.4).|
   |Live output|**One action at a time; the others wait as blocks**|`--stream` shows the first action to begin while none is shown. Outcomes that end meanwhile are logged at once and reported after it, so output is never interleaved and parallelism is kept; `run` returns the same outcome with or without the live sink, so the stored bytes cannot differ (§3.4).|
   |A build of named targets|**The graph is narrowed between Resolve and Plan**|Plan asks for the tool and every input of every node it is given, so narrowing any later would fail a build on a missing input of an unrelated target. Output places are still checked over the whole graph, because an unchecked collision would delete another target's output. A label nothing declares is `Error::UnknownTarget`.|
   |When `graph.json` is written|**By every build, right after Resolve, always the whole graph, through the `GraphRecord` port**|The graph on disk then matches the rows and blobs beside it, and is the same bytes for `build` and `build <target>`. `plan` and `graph` record nothing. Closes the question §8 held.|
   |A command a signal ended|**Its own class, `Killed { signal }`, an action failure**|A process a signal ended has no exit status, so `ActionFailure { status }` has nothing true to hold. A timeout is told by the worker's own flag, never by reading the signal (§3.4, §4).|
   |The action cache on disk|**A snapshot, `cache.json`, and a journal, `cache.journal`, folded once per build**|A row must be durable when `record` returns, and rewriting the whole file for each row is quadratic: seconds for a thousand rows, where one appended line is microseconds. Rows reach `cache.json` only through the sorted map and the canonical serializer, so completion order never reaches it (§3.5).|
   |Syncing to disk|**None, for any file**|A rename already covers the death of the process, which is what design §8.5 promises. A sync would add safety only against the machine itself dying, whose remedy is to remove `.buildl/` and `out/`, at some thirty times the cost of the write.|
   |Protecting the cas|**Read-only blobs, an executable twin, and a link only where a write through it is refused**|An action that opens a dependency's output for writing would otherwise write into the store. The adapter checks once per store that a write to a read-only file is refused, and copies everywhere when it is not. A tool that changes the mode first can still damage a blob; tier 1 does not defend against that, since such a tool can write under `.buildl/` directly (§3.5).|
   |A second buildl in one workspace|**Refused, through a lock on `.buildl/lock` held for one `plan` or `build` call**|Two builds at tier 1 write the same paths under `out/`. The lock is the system's, so it is freed when its holder dies and no stale lock exists; `check` and `graph` take none. A state directory outside `fs.write` is refused before it is made, with the error Plan gives for that want, so taking the lock never exceeds the ceiling.|
   |Which stat-cache entries are kept|**Those for a file modified more than 2 s before the run began; keyed by path**|An edit Plan cannot see must leave modification time and size equal, so it falls inside the timestamp's granule, and 2 s is the coarsest granule met. The price: a file edited in the two seconds before a run is hashed again on the next one. A renamed file is hashed once more, which closes the `(dev, inode)` question §8 held (§3.3).|
   |Stopping an action|**The direct child only; no process group at tier 1**|A child in a group of its own stops receiving the terminal's Ctrl-C, and std can neither signal a group nor forward a signal to one. What an action started is therefore not stopped with it, at a timeout or at its exit (design §12.2).|
   |Signals to buildl itself|**No handler**|std has no safe way to install one, and the workspace forbids `unsafe`. Every action stays in buildl's process group, so a terminal's Ctrl-C ends buildl and its actions together; storage survives a kill at any instruction, and an interrupted build prints no summary (§3.5).|
   |Finding a program|**One search path, read once, used by the tool resolver and by `Bare`; no trait between them**|The file run must be the file fingerprinted. A search path can be built from any list of directories, so tests need no seam.|
   |Tools in adapter and end-to-end tests|**System tools, run by argv; `sh -c` in tests only; no compiled helper**|Cargo hands a test the path of a built binary only for integration tests of the package that owns it, so a helper would cost the published `buildl` crate a binary or a feature, or the workspace a fifth member (§7).|
   |Adapter isolation inside `buildl`|**A gate task, `cargo make guard-adapter-isolation`, not review**|Thirteen adapters in one crate is where a cross-import slips in. An adapter module may name the two shared modules, `state` and `search_path`, and nothing else of the crate (building blocks §7).|
   ~~~~

4. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'so a file that fails is `Error::UnreadableJson` or `Error::NonCanonicalJson` — corrupt, and dist' docs/architecture.md
   0
   $ grep -c -F -e 'so a file that fails surfaces as `Error::CorruptState`, which names the file and wraps `Error::U' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 'Nothing in Plan reads it, so editing only a `desc` leaves every target cached (§3.4).|' docs/architecture.md
   1
   $ grep -c -F -e '|How a running build reaches the reporter|**Messages through the dispatcher: `work(task, tell)`' docs/architecture.md
   1
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add docs/architecture.md
   $ git commit -m "docs(repo): record the adapter decisions in architecture §6"
   ~~~~

### Task 18 — Bring the architecture's testing strategy and open questions up to date

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '**`plan`**: synthetic graphs + a tempdir tree → dirty sets and reasons; stat-cache hit/miss/mtim' docs/architecture.md
   1
   $ grep -c -F -e '**`plan`**: synthetic graphs against fake ports → dirty sets and reasons; stat-cache hit/miss/mt' docs/architecture.md
   0
   # step 3
   $ grep -c -F -e 'tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → out' docs/architecture.md
   1
   $ grep -c -F -e 'system tools run by argv against the real `Bare` adapter, with `sh -c` only where one tool canno' docs/architecture.md
   0
   # step 4
   $ grep -c -F -e 'second run all-cached and byte-identical state (§5.5)' docs/architecture.md
   1
   $ grep -c -F -e 'second run all-cached and byte-identical state (§5 rule 5)' docs/architecture.md
   0
   # step 5
   $ grep -c -F -e '**The airsl repo as dogfood** (design doc §13): buildl' docs/architecture.md
   1
   $ grep -c -F -e '**The airsl repo as dogfood**: buildl' docs/architecture.md
   0
   # step 6
   $ grep -c -F -e 'Whether `ExecDir` materialisation for `InputSandbox` uses symlinks' docs/architecture.md
   1
   $ grep -c -F -e 'Open until the input sandbox is built (item 3 of the design doc' docs/architecture.md
   0
   # step 7
   $ grep -c -F -e 'Whether the stat cache should be keyed by `(dev, inode)` in addition to path, to survive renames' docs/architecture.md
   1
   $ grep -c -F -e 'Settled: the stat cache is keyed by path alone. `FileStat` holds no inode, and a renamed file is' docs/architecture.md
   0
   # step 8
   $ grep -c -F -e 'Whether `graph.json` is written on every build or only on `buildl graph` — writing always makes' docs/architecture.md
   1
   $ grep -c -F -e 'Settled: `graph.json` is written by every build, right after Resolve, and by no other command (d' docs/architecture.md
   0
   ~~~~

2. In `docs/architecture.md`, in the `plan` bullet of §7, say Plan is tested against fakes. Replace:

   ~~~~markdown
   - **`plan`**: synthetic graphs + a tempdir tree → dirty sets and reasons; stat-cache hit/miss/mtime-rollback cases.
   ~~~~

   With:

   ~~~~markdown
   - **`plan`**: synthetic graphs against fake ports → dirty sets and reasons; stat-cache hit/miss/mtime-rollback cases. Temp directories belong to the adapter tests, not to Plan's.
   ~~~~

3. In `docs/architecture.md`, in the `exec` bullet, say what the real strategy is tested with and how an interrupted build is tested. Replace:

   ~~~~markdown
   tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → output block integrity, a worker killed mid-action leaves no row.
   ~~~~

   With:

   ~~~~markdown
   system tools run by argv against the real `Bare` adapter, with `sh -c` only where one tool cannot print to a chosen stream, pick an exit status or hold a pipe → the timeout, a command a signal ended, exact capture of both streams. An interrupted build is tested as the four states a crash can leave, each made from honest builds: content with no row, a row in the journal that was never folded, a staging file, and a partial output no row names.
   ~~~~

4. In `docs/architecture.md`, in the end-to-end bullet, correct the cross-reference: the rule is §5 rule 5. Replace:

   ~~~~markdown
   second run all-cached and byte-identical state (§5.5)
   ~~~~

   With:

   ~~~~markdown
   second run all-cached and byte-identical state (§5 rule 5)
   ~~~~

5. In `docs/architecture.md`, in the last bullet, drop the citation, which names a section that says nothing of it, and say nothing owns this yet. Replace:

   ~~~~markdown
   - **The airsl repo as dogfood** (design doc §13): buildl's own gate eventually runs under buildl.
   ~~~~

   With:

   ~~~~markdown
   - **The airsl repo as dogfood**: buildl's own gate eventually runs under buildl. This is an aim; no item of the design's sequence owns it.
   ~~~~

6. In `docs/architecture.md`, in §8, mark the first question as open until the input sandbox is built. Replace:

   ~~~~markdown
   - Whether `ExecDir` materialisation for `InputSandbox` uses symlinks
   ~~~~

   With:

   ~~~~markdown
   - Open until the input sandbox is built (item 3 of the design doc's §13): whether `ExecDir` materialisation for `InputSandbox` uses symlinks
   ~~~~

7. In `docs/architecture.md`, settle the question on the stat cache's key. Replace:

   ~~~~markdown
   - Whether the stat cache should be keyed by `(dev, inode)` in addition to path, to survive renames without re-hashing.
   ~~~~

   With:

   ~~~~markdown
   - Settled: the stat cache is keyed by path alone. `FileStat` holds no inode, and a renamed file is hashed once more (decision record §6).
   ~~~~

8. In `docs/architecture.md`, settle the question on when `graph.json` is written. Replace:

   ~~~~markdown
   - Whether `graph.json` is written on every build or only on `buildl graph` — writing always makes `gc` roots simpler; measuring the cost first.
   ~~~~

   With:

   ~~~~markdown
   - Settled: `graph.json` is written by every build, right after Resolve, and by no other command (decision record §6).
   ~~~~

9. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e '**`plan`**: synthetic graphs + a tempdir tree → dirty sets and reasons; stat-cache hit/miss/mtim' docs/architecture.md
   0
   $ grep -c -F -e '**`plan`**: synthetic graphs against fake ports → dirty sets and reasons; stat-cache hit/miss/mt' docs/architecture.md
   1
   # step 3
   $ grep -c -F -e 'tiny compiled helpers (`/bin/sh`-free) against the real `Bare` and `InputSandbox` adapters → out' docs/architecture.md
   0
   $ grep -c -F -e 'system tools run by argv against the real `Bare` adapter, with `sh -c` only where one tool canno' docs/architecture.md
   1
   # step 4
   $ grep -c -F -e 'second run all-cached and byte-identical state (§5.5)' docs/architecture.md
   0
   $ grep -c -F -e 'second run all-cached and byte-identical state (§5 rule 5)' docs/architecture.md
   1
   # step 5
   $ grep -c -F -e '**The airsl repo as dogfood** (design doc §13): buildl' docs/architecture.md
   0
   $ grep -c -F -e '**The airsl repo as dogfood**: buildl' docs/architecture.md
   1
   # step 6
   $ grep -c -F -e 'Whether `ExecDir` materialisation for `InputSandbox` uses symlinks' docs/architecture.md
   0
   $ grep -c -F -e 'Open until the input sandbox is built (item 3 of the design doc' docs/architecture.md
   1
   # step 7
   $ grep -c -F -e 'Whether the stat cache should be keyed by `(dev, inode)` in addition to path, to survive renames' docs/architecture.md
   0
   $ grep -c -F -e 'Settled: the stat cache is keyed by path alone. `FileStat` holds no inode, and a renamed file is' docs/architecture.md
   1
   # step 8
   $ grep -c -F -e 'Whether `graph.json` is written on every build or only on `buildl graph` — writing always makes' docs/architecture.md
   0
   $ grep -c -F -e 'Settled: `graph.json` is written by every build, right after Resolve, and by no other command (d' docs/architecture.md
   1
   ~~~~

10. Run the gate; both commands exit `0`:

    ~~~~text
    $ cargo make dod
    $ cargo deny check
    ~~~~

11. Commit:

    ~~~~text
    $ git add docs/architecture.md
    $ git commit -m "docs(repo): bring the architecture's testing strategy and open questions up to date"
    ~~~~

### Task 19 — Give building blocks §3 and §4 the modules buildl holds

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, log.jsonl,<br/>statcache.json, graph.json' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, cache.journal,<br/>log.jsonl, statcache.j' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e 'clap argument surface<br/>maps OutcomeClass to exit codes"]' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'clap argument surface<br/>maps outcome classes and<br/>error variants to exit codes"]' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'adapters: store, exec, digest, manifest,<br/>tools, approver, clock, reporter<br/>assembly: Loca' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'adapters: manifest, host, digest, tools,<br/>cas, cache, log, statcache, graph,<br/>exec, dispat' docs/architecture-building-blocks.md
   0
   # step 5
   $ grep -c -F -e 'ExecStrategy, Dispatcher<br/>Approver, Clock, Reporter"]' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'ExecStrategy, Dispatcher<br/>Clock, GraphRecord, Reporter<br/>later: Approver"]' docs/architecture-building-blocks.md
   0
   # step 6
   $ grep -c -F -e '|`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core`|`b' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core` as' docs/architecture-building-blocks.md
   0
   # step 7
   $ grep -c -F -e 'Adapter dependencies are finalised as each adapter lands, following the per-dependency reasons r' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'Each adapter dependency has its reason recorded in the workspace `Cargo.toml`: `toml` reads `bui' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in the container diagram of §3, which stays Mermaid, list every file of the state store. Replace:

   ~~~~markdown
   STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, log.jsonl,<br/>statcache.json, graph.json"]
   ~~~~

   With:

   ~~~~markdown
   STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, cache.journal,<br/>log.jsonl, statcache.json,<br/>graph.json, lock"]
   ~~~~

3. In `docs/architecture-building-blocks.md`, in the component diagram of §4, which stays Mermaid, say what the CLI maps to exit codes. Replace:

   ~~~~markdown
   clap argument surface<br/>maps OutcomeClass to exit codes"]
   ~~~~

   With:

   ~~~~markdown
   clap argument surface<br/>maps outcome classes and<br/>error variants to exit codes"]
   ~~~~

4. In `docs/architecture-building-blocks.md`, in the same diagram, list the modules of `buildl` as they are. Replace:

   ~~~~markdown
   adapters: store, exec, digest, manifest,<br/>tools, approver, clock, reporter<br/>assembly: LocalPorts, Workspace facade"]
   ~~~~

   With:

   ~~~~markdown
   adapters: manifest, host, digest, tools,<br/>cas, cache, log, statcache, graph,<br/>exec, dispatch, clock, report<br/>shared: state, search_path<br/>assembly: LocalPorts, Workspace facade"]
   ~~~~

5. In `docs/architecture-building-blocks.md`, in the same diagram, list the ports that exist and mark the one that arrives later. Replace:

   ~~~~markdown
   ExecStrategy, Dispatcher<br/>Approver, Clock, Reporter"]
   ~~~~

   With:

   ~~~~markdown
   ExecStrategy, Dispatcher<br/>Clock, GraphRecord, Reporter<br/>later: Approver"]
   ~~~~

6. In `docs/architecture-building-blocks.md`, in the crate table below the diagram, give the path of the re-export and the dependencies of `buildl`. Replace:

   ~~~~markdown
   |`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core`|`buildl-core`, `buildl-lua`, plus each adapter's own dependencies|— (composition root, §7)|
   ~~~~

   With:

   ~~~~markdown
   |`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core` as `buildl::buildl_core`|`buildl-core`, `buildl-lua`, `toml`, `rayon`, `serde`, `thiserror`, `tempfile`|— (composition root, §7)|
   ~~~~

7. In `docs/architecture-building-blocks.md`, replace the sentence below the crate table with what each dependency is for, the module table of `buildl`, and the supported platforms. Replace:

   ~~~~markdown
   Adapter dependencies are finalised as each adapter lands, following the per-dependency reasons recorded in the workspace `Cargo.toml`.
   ~~~~

   With:

   ~~~~markdown
   Each adapter dependency has its reason recorded in the workspace `Cargo.toml`: `toml` reads `buildl.toml`; `rayon` hashes files, in the `Digester` adapter and nowhere else; `serde` derives the manifest's tables and the two lines the state adapters write; `thiserror` derives the state lock's error; and `tempfile` gives the temp-file-then-rename that every state file and blob is written by.

   Inside `buildl` there is one module per adapter, two shared modules that implement no port, and two composition modules (§7 rule 2):

   |Module|Holds|Implements|
   |---|---|---|
   |`manifest`|`buildl.toml`, and finding the workspace root|`Manifest`|
   |`host`|the OS, the architecture, environment values|`Host`|
   |`digest`|what stands at an input's path, and file hashing on rayon|`Digester`|
   |`tools`|a program's lookup and its fingerprint|`ToolResolver`|
   |`cas`|`.buildl/cas`, and the files placed under `out/`|`ContentStore`|
   |`cache`|`cache.json` and `cache.journal`|`ActionCache`|
   |`log`|`log.jsonl`|`EventLog`|
   |`statcache`|`statcache.json`|`StatCache`|
   |`graph`|`graph.json`|`GraphRecord`|
   |`exec`|tier 1, `bare`|`ExecStrategy`|
   |`dispatch`|worker threads|`Dispatcher`|
   |`clock`|the system clock|`Clock`|
   |`report`|terminal output, and every rendering of a value into text|`Reporter`|
   |`state`|shared: where each state file is, the state lock, writing a file whole, clearing a path|—|
   |`search_path`|shared: the lookup of a program name|—|
   |`local_ports`|composition: the `Ports` bundle and its wiring|—|
   |`workspace`|composition: the `Workspace` facade|—|

   `DeclarationSource` is `buildl_lua::LuaSource`. An `approver` module arrives with the `Approver` port (design §13 item 4); neither exists yet.

   `buildl` and `buildl-cli` build on Unix targets only, and the supported platforms are Linux and macOS, the two the CI matrix runs. `buildl` uses `std::os::unix` for file modes and for the signal that ended a process.
   ~~~~

8. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, log.jsonl,<br/>statcache.json, graph.json' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'STATE["[Data store]<br/>.buildl/<br/>cas/, cache.json, cache.journal,<br/>log.jsonl, statcache.j' docs/architecture-building-blocks.md
   1
   # step 3
   $ grep -c -F -e 'clap argument surface<br/>maps OutcomeClass to exit codes"]' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'clap argument surface<br/>maps outcome classes and<br/>error variants to exit codes"]' docs/architecture-building-blocks.md
   1
   # step 4
   $ grep -c -F -e 'adapters: store, exec, digest, manifest,<br/>tools, approver, clock, reporter<br/>assembly: Loca' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'adapters: manifest, host, digest, tools,<br/>cas, cache, log, statcache, graph,<br/>exec, dispat' docs/architecture-building-blocks.md
   1
   # step 5
   $ grep -c -F -e 'ExecStrategy, Dispatcher<br/>Approver, Clock, Reporter"]' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'ExecStrategy, Dispatcher<br/>Clock, GraphRecord, Reporter<br/>later: Approver"]' docs/architecture-building-blocks.md
   1
   # step 6
   $ grep -c -F -e '|`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core`|`b' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '|`buildl`|every other adapter; `LocalPorts`; the `Workspace` facade; re-exports `buildl-core` as' docs/architecture-building-blocks.md
   1
   # step 7
   $ grep -c -F -e 'Adapter dependencies are finalised as each adapter lands, following the per-dependency reasons r' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'Each adapter dependency has its reason recorded in the workspace `Cargo.toml`: `toml` reads `bui' docs/architecture-building-blocks.md
   1
   ~~~~

9. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

10. Commit:

    ~~~~text
    $ git add docs/architecture-building-blocks.md
    $ git commit -m "docs(repo): give building blocks §3 and §4 the modules buildl holds"
    ~~~~

### Task 20 — Give building blocks §5.1 and §5.2 the ports as they are now

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '|Classification|`OutcomeClass`, `InfraKind` and `OutputRefusal` (`#[non_exhaustive]`, `architect' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|A run and what it shows|`Scope` — what a run was asked for: the workspace, or named targets — `' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e '/// Metadata is asked of every input; digests only of those whose metadata changed,' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// What stands at its path is asked of every input, following no symbolic link:' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// With `live`, every run of bytes the command writes is also handed to it as it' docs/architecture-building-blocks.md
   0
   # step 5
   $ grep -c -F -e '/// `work` runs one task, on any thread. `step` runs on the calling thread, first with' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// `work` runs one task, on any thread, and may send messages through its second' docs/architecture-building-blocks.md
   0
   # step 6
   $ grep -c -F -e 'pub trait Clock {' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// Keeps the graph a build resolved. `record` replaces what is kept with `graph`, and' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in the table of §5.1, add two rows above the Classification row, for the values the signatures of §5.2 now name. Replace:

   ~~~~markdown
   |Classification|`OutcomeClass`, `InfraKind` and `OutputRefusal` (`#[non_exhaustive]`, `architecture.md` §4); `TreeFault` — why a list of files is not a tree|
   ~~~~

   With:

   ~~~~markdown
   |A run and what it shows|`Scope` — what a run was asked for: the workspace, or named targets — `LiveOutput` — whether one action is shown as it prints — `Channel`, `LiveSink` and `Captured` — which stream an action wrote to, where its bytes go as they are read, and what it printed — `Turn` — what a dispatcher hands the scheduling thread|
   |Inputs|`InputStat` and `InputKind` — what stands at an input's path: nothing, a regular file, or a link, a directory or a special file; `Hasher` — `Digest::of` for bytes handed over in pieces|
   |Classification|`OutcomeClass`, `InfraKind` and `OutputRefusal` (`#[non_exhaustive]`, `architecture.md` §4); `TreeFault` — why a list of files is not a tree|
   ~~~~

3. In `docs/architecture-building-blocks.md`, in the code block of §5.2, give `Digester::stat` its three answers. Replace:

   ~~~~markdown
   /// Metadata is asked of every input; digests only of those whose metadata changed,
   /// all in one call so an adapter may hash in parallel.
   pub trait Digester {
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>>;
   ~~~~

   With:

   ~~~~markdown
   /// What stands at its path is asked of every input, following no symbolic link:
   /// nothing, a regular file with its metadata, or something that is not a regular
   /// file. Digests are asked only of files whose metadata changed, all in one call so
   /// an adapter may hash in parallel.
   pub trait Digester {
       fn stat(&self, path: &SourcePath) -> Result<InputStat>;
   ~~~~

4. In `docs/architecture-building-blocks.md`, in the same block, give `ExecStrategy::run` its `live` argument. Replace:

   ~~~~markdown
       fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;
   ~~~~

   With:

   ~~~~markdown
       /// With `live`, every run of bytes the command writes is also handed to it as it
       /// is read, with its channel. The outcome is the same with or without it.
       fn run(
           &self,
           dir: &Self::Dir,
           action: &ReadyAction,
           live: Option<LiveSink<'_>>,
       ) -> Result<RawOutcome>;
   ~~~~

5. In `docs/architecture-building-blocks.md`, in the same block, give `Dispatcher` its messages. Replace:

   ~~~~markdown
   /// `work` runs one task, on any thread. `step` runs on the calling thread, first with
   /// `None` and then with each result, and returns the tasks that became ready. Every task
   /// `step` returns is run and every result reaches `step`; a build over a dispatcher that
   /// loses one fails with `Error::DispatcherFault` instead of reporting success.
   pub trait Dispatcher {
       fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
       where
           T: Send,
           R: Send,
           W: Fn(T) -> R + Sync,
           S: FnMut(Option<R>) -> Vec<T>;
   }
   ~~~~

   With:

   ~~~~markdown
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
   ~~~~

6. In `docs/architecture-building-blocks.md`, at the end of the same block, add the two ports after `Clock`. Replace:

   ~~~~markdown
   pub trait Clock {
       fn now(&self) -> Timestamp;
   }
   ~~~~

   With:

   ~~~~markdown
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
       /// for a cached target, what it wrote when it last ran.
       fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>);
       /// Every target has its outcome.
       fn summary(&mut self, summary: Summary);
   }
   ~~~~

7. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '|Classification|`OutcomeClass`, `InfraKind` and `OutputRefusal` (`#[non_exhaustive]`, `architect' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|A run and what it shows|`Scope` — what a run was asked for: the workspace, or named targets — `' docs/architecture-building-blocks.md
   1
   # step 3
   $ grep -c -F -e '/// Metadata is asked of every input; digests only of those whose metadata changed,' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '/// What stands at its path is asked of every input, following no symbolic link:' docs/architecture-building-blocks.md
   1
   # step 4
   $ grep -c -F -e 'fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '/// With `live`, every run of bytes the command writes is also handed to it as it' docs/architecture-building-blocks.md
   1
   # step 5
   $ grep -c -F -e '/// `work` runs one task, on any thread. `step` runs on the calling thread, first with' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '/// `work` runs one task, on any thread, and may send messages through its second' docs/architecture-building-blocks.md
   1
   # step 6
   $ grep -c -F -e 'pub trait Clock {' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// Keeps the graph a build resolved. `record` replaces what is kept with `graph`, and' docs/architecture-building-blocks.md
   1
   ~~~~

8. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

9. Commit:

   ~~~~text
   $ git add docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): give building blocks §5.1 and §5.2 the ports as they are now"
   ~~~~

### Task 21 — Show the reporter and the graph record in the Ports bundle of building blocks §5

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '// Approver, Reporter' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'type Graph: GraphRecord;' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e '/// One instance of each port the bundle names, with a public field per port.' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '/// One instance of each port the bundle names, with a public field per port: `source`,' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'pub type LocalPipeline = Pipeline<LocalPorts>;' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '// in `buildl`: the bundle is generic over the reporter a build is lent' docs/architecture-building-blocks.md
   0
   # step 5
   $ grep -c -F -e 'class LocalPorts {' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'generic over R, the reporter of one build' docs/architecture-building-blocks.md
   0
   # step 6
   $ grep -c -F -e '|resolve|`Vec<Declaration>` → `TargetGraph` (validations, cycle detection)|—|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|narrow|`TargetGraph` and a `Scope` → the graph of the targets a run was asked for and everythin' docs/architecture-building-blocks.md
   0
   # step 7
   $ grep -c -F -e '|grants|wanted set ∩ ceiling → grants or refusal|`Manifest`, `Approver`|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|grants — not built; arrives with design §13 item 4|wanted set ∩ ceiling → grants or refusal. To' docs/architecture-building-blocks.md
   0
   # step 8
   $ grep -c -F -e '|execute|`TargetGraph`, `Plan` and the failure policy → `Vec<ActionOutcome>` (output layout, pla' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|execute|the whole `TargetGraph`, the narrowed one, its `Plan`, the failure policy and `LiveOutp' docs/architecture-building-blocks.md
   0
   # step 9
   $ grep -c -F -e '|canonical JSON|value → sorted-key bytes; those bytes, and no others, → the value|—|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '`Pipeline` strings these together, one method per command. `plan` and `build` take a `Scope` and' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in the code block of §5.3, add the two associated types and say when `Approver` joins. Replace:

   ~~~~markdown
       type Clock: Clock + Sync;   // a worker reads the clock
       // Approver, Reporter
   }
   ~~~~

   With:

   ~~~~markdown
       type Clock: Clock + Sync;   // a worker reads the clock
       type Graph: GraphRecord;
       type Reporter: Reporter;
       // `Approver` joins with grant negotiation (design §13 item 4)
   }
   ~~~~

3. In `docs/architecture-building-blocks.md`, in the same block, name the fields of `PortSet`. Replace:

   ~~~~markdown
   /// One instance of each port the bundle names, with a public field per port.
   ~~~~

   With:

   ~~~~markdown
   /// One instance of each port the bundle names, with a public field per port: `source`,
   /// `manifest`, `host`, `digester`, `stat_cache`, `tools`, `cache`, `cas`, `log`, `strategy`,
   /// `dispatcher`, `clock`, `graph` and `reporter`.
   ~~~~

4. In `docs/architecture-building-blocks.md`, in the same block, make the bundle of `buildl` generic over the reporter. Replace:

   ~~~~markdown
   // in `buildl`:
   pub type LocalPipeline = Pipeline<LocalPorts>;
   ~~~~

   With:

   ~~~~markdown
   // in `buildl`: the bundle is generic over the reporter a build is lent
   pub struct LocalPorts<R>(PhantomData<fn() -> R>);
   pub type LocalPipeline<R> = Pipeline<LocalPorts<R>>;
   ~~~~

5. In `docs/architecture-building-blocks.md`, in the class diagram below, which stays Mermaid, add the two types to `Ports` and the reporter to `LocalPorts`. Replace:

   ~~~~markdown
           Clock: Clock + Sync
       }
       class LocalPorts {
           crate buildl
           real adapters
       }
   ~~~~

   With:

   ~~~~markdown
           Clock: Clock + Sync
           Graph: GraphRecord
           Reporter: Reporter
       }
       class LocalPorts {
           crate buildl
           real adapters
           generic over R, the reporter of one build
       }
   ~~~~

6. In `docs/architecture-building-blocks.md`, in the table of §5.4, add a row for narrowing after the `resolve` row. Replace:

   ~~~~markdown
   |resolve|`Vec<Declaration>` → `TargetGraph` (validations, cycle detection)|—|
   ~~~~

   With:

   ~~~~markdown
   |resolve|`Vec<Declaration>` → `TargetGraph` (validations, cycle detection)|—|
   |narrow|`TargetGraph` and a `Scope` → the graph of the targets a run was asked for and everything they depend on, or the whole graph for `Scope::Workspace`; a label nothing declares is `Error::UnknownTarget`|—|
   ~~~~

7. In `docs/architecture-building-blocks.md`, in the same table, say the `grants` logic is not built. Replace:

   ~~~~markdown
   |grants|wanted set ∩ ceiling → grants or refusal|`Manifest`, `Approver`|
   ~~~~

   With:

   ~~~~markdown
   |grants — not built; arrives with design §13 item 4|wanted set ∩ ceiling → grants or refusal. Today the ceiling check is Plan's, and a want outside the ceiling refuses the plan|`Manifest`, and `Approver` once it exists|
   ~~~~

8. In `docs/architecture-building-blocks.md`, in the same table, replace the `execute` row: it takes the whole graph and `LiveOutput`, and uses the `Reporter` port. Replace:

   ~~~~markdown
   |execute|`TargetGraph`, `Plan` and the failure policy → `Vec<ActionOutcome>` (output layout, placeholder expansion of commands and descriptions under the plan's settings, the program check, the `Schedule` state machine, cached or run, what an action left judged into one manifest per output, then cas, row, log; an outcome per target, or `Error::DispatcherFault`)|`Manifest`, `Dispatcher`, `ExecStrategy`, `ContentStore`, `ActionCache`, `EventLog`, `Clock`|
   ~~~~

   With:

   ~~~~markdown
   |execute|the whole `TargetGraph`, the narrowed one, its `Plan`, the failure policy and `LiveOutput` → `Vec<ActionOutcome>` (output layout, checked over the whole graph and used for the narrowed one; placeholder expansion of commands and descriptions under the plan's settings, the program check, the `Schedule` state machine, cached or run, what an action left judged into one manifest per output, then cas, row, log, with each step shown through the reporter; an outcome per target of the narrowed graph, or `Error::DispatcherFault`)|`Manifest`, `Dispatcher`, `ExecStrategy`, `ContentStore`, `ActionCache`, `EventLog`, `Clock`, `Reporter`|
   ~~~~

9. In `docs/architecture-building-blocks.md`, below the last row of the same table, add a paragraph on how `Pipeline` joins the logic and on the two free functions. Replace:

   ~~~~markdown
   |canonical JSON|value → sorted-key bytes; those bytes, and no others, → the value|—|
   ~~~~

   With:

   ~~~~markdown
   |canonical JSON|value → sorted-key bytes; those bytes, and no others, → the value|—|

   `Pipeline` strings these together, one method per command. `plan` and `build` take a `Scope` and narrow between Resolve and Plan; `build` also records the whole graph through `GraphRecord` before it narrows. `check` and `graph` read only the source of build files, so each also exists as a free function, `pipeline::check` and `pipeline::graph`, for a caller that holds a `DeclarationSource` and nothing else: a command that reads only build files then needs no state directory, takes no lock and is not stopped by a state file.
   ~~~~

10. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

    ~~~~text
    # step 2
    $ grep -c -F -e '// Approver, Reporter' docs/architecture-building-blocks.md
    0
    $ grep -c -F -e 'type Graph: GraphRecord;' docs/architecture-building-blocks.md
    1
    # step 3
    $ grep -c -F -e '/// One instance of each port the bundle names, with a public field per port.' docs/architecture-building-blocks.md
    0
    $ grep -c -F -e '/// One instance of each port the bundle names, with a public field per port: `source`,' docs/architecture-building-blocks.md
    1
    # step 4
    $ grep -c -F -e 'pub type LocalPipeline = Pipeline<LocalPorts>;' docs/architecture-building-blocks.md
    0
    $ grep -c -F -e '// in `buildl`: the bundle is generic over the reporter a build is lent' docs/architecture-building-blocks.md
    1
    # step 5
    $ grep -c -F -e 'class LocalPorts {' docs/architecture-building-blocks.md
    1
    $ grep -c -F -e 'generic over R, the reporter of one build' docs/architecture-building-blocks.md
    1
    # step 6
    $ grep -c -F -e '|resolve|`Vec<Declaration>` → `TargetGraph` (validations, cycle detection)|—|' docs/architecture-building-blocks.md
    1
    $ grep -c -F -e '|narrow|`TargetGraph` and a `Scope` → the graph of the targets a run was asked for and everythin' docs/architecture-building-blocks.md
    1
    # step 7
    $ grep -c -F -e '|grants|wanted set ∩ ceiling → grants or refusal|`Manifest`, `Approver`|' docs/architecture-building-blocks.md
    0
    $ grep -c -F -e '|grants — not built; arrives with design §13 item 4|wanted set ∩ ceiling → grants or refusal. To' docs/architecture-building-blocks.md
    1
    # step 8
    $ grep -c -F -e '|execute|`TargetGraph`, `Plan` and the failure policy → `Vec<ActionOutcome>` (output layout, pla' docs/architecture-building-blocks.md
    0
    $ grep -c -F -e '|execute|the whole `TargetGraph`, the narrowed one, its `Plan`, the failure policy and `LiveOutp' docs/architecture-building-blocks.md
    1
    # step 9
    $ grep -c -F -e '|canonical JSON|value → sorted-key bytes; those bytes, and no others, → the value|—|' docs/architecture-building-blocks.md
    1
    $ grep -c -F -e '`Pipeline` strings these together, one method per command. `plan` and `build` take a `Scope` and' docs/architecture-building-blocks.md
    1
    ~~~~

11. Run the gate; both commands exit `0`:

    ~~~~text
    $ cargo make dod
    $ cargo deny check
    ~~~~

12. Commit:

    ~~~~text
    $ git add docs/architecture-building-blocks.md
    $ git commit -m "docs(repo): show the reporter and the graph record in the Ports bundle of building blocks §5"
    ~~~~

### Task 22 — Give the port catalog of building blocks §6 the adapters as built

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'to the dispatcher and the strategy; declaration limits, settings|`buildl.toml` reader|`buildl`|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '"Since" is the item of design §13 an adapter arrives with; item 1 is built, and a row with a lat' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, replace the whole table of §6, its header included, with the table below and the sentence above it. Every row gains a "Since" cell, `ExecStrategy` gets a row per tier, and `GraphRecord` gets a row. Replace:

   ~~~~markdown
   |Port|`buildl-core` uses it for|v1 adapter|Crate|
   |---|---|---|---|
   |`DeclarationSource`|Load — one build file at a time|airsl engine per file, `buildl` module table|`buildl-lua`|
   |`Manifest`|the ceiling and the workspace's output and state directories; the `[execution]` settings, which no phase reads and the composition root hands to the dispatcher and the strategy; declaration limits, settings|`buildl.toml` reader|`buildl`|
   |`Host`|the platform in every key, and the values of the env names targets read|the running machine's OS, architecture and environment|`buildl`|
   |`Digester`|input content hashes|filesystem + rayon|`buildl`|
   |`StatCache`|reusing digests on unchanged mtime + size|`.buildl/statcache.json`|`buildl`|
   |`ToolResolver`|tool fingerprints in the key|`PATH` lookup + fingerprint|`buildl`|
   |`ContentStore`|the cas: storing a blob, asking whether one is held, reading a held one back (an output's manifest), clearing an output's path under `out/` — before the output is placed, and again when it cannot be placed whole — and making a stored blob a file there, runnable or not|`.buildl/cas/ab/…`, temp file then rename; hard links into `out/`, with a second inode or a copy where one blob is placed both plain and executable|`buildl`|
   |`ActionCache`|cache rows: reading the last build's, and recording one durably for each target that was built|`.buildl/cache.json`, written by the canonical serializer and read back by the canonical reader|`buildl`|
   |`EventLog`|one event per outcome|`.buildl/log.jsonl`|`buildl`|
   |`ExecStrategy`|running one action, started with each output's containing directory in place and nothing at the output's own path, then reporting what the action left at each output path — a file, a directory with everything below it, or neither — and storing every regular file of it|`Bare` (tier 1), `InputSandbox` (tier 2)|`buildl`|
   |`Dispatcher`|running ready actions in parallel|std threads + `mpsc`|`buildl`|
   |`Approver`|turning the wanted set into grants|interactive, manifest|`buildl`|
   |`Clock`|log timestamps and durations|system clock|`buildl`|
   |`Reporter`|output blocks and the status line|TTY, plain|`buildl`|
   ~~~~

   With:

   ~~~~markdown
   "Since" is the item of design §13 an adapter arrives with; item 1 is built, and a row with a later item describes what is designed.

   |Port|`buildl-core` uses it for|v1 adapter|Crate|Since|
   |---|---|---|---|---|
   |`DeclarationSource`|Load — one build file at a time|airsl engine per file, `buildl` module table|`buildl-lua`|1|
   |`Manifest`|the ceiling and the workspace's output and state directories; the `[execution]` settings, which no phase reads and the composition root hands to the dispatcher and the strategy|`buildl.toml` reader. The entry file's name and the declaration limits do not go through the port: the adapter hands them to the composition root, which makes the declaration source from them|`buildl`|1|
   |`Host`|the platform in every key, and the values of the env names targets read|the running machine's OS, architecture and environment|`buildl`|1|
   |`Digester`|what stands at each input's path, and input content hashes|filesystem + rayon, each file streamed in pieces|`buildl`|1|
   |`StatCache`|reusing digests on unchanged mtime + size|`.buildl/statcache.json`, keyed by path; keeps an entry only for a file modified more than 2 s before the run began, and discards a file it cannot read back|`buildl`|1|
   |`ToolResolver`|tool fingerprints in the key|`PATH` lookup + content hash|`buildl`|1|
   |`ContentStore`|the cas: storing a blob, asking whether one is held, reading a held one back (an output's manifest), clearing an output's path under `out/` — before the output is placed, and again when it cannot be placed whole — and making a stored blob a file there, runnable or not|`.buildl/cas/ab/…`, temp file then rename, nothing synced; read-only blobs, with an executable twin `….x` beside a blob that was placed executable; hard links into `out/` where a write through a read-only file is refused, copies elsewhere|`buildl`|1|
   |`ActionCache`|cache rows: reading the last build's, and recording one durably for each target that was built|`.buildl/cache.json`, written by the canonical serializer and read back by the canonical reader, with `.buildl/cache.journal`: one appended line per recorded row, folded into the snapshot once per build|`buildl`|1|
   |`EventLog`|one event per outcome|`.buildl/log.jsonl`, appended to across builds|`buildl`|1|
   |`ExecStrategy`|running one action, started with each output's containing directory in place and nothing at the output's own path, handing over what it prints as it prints when asked to, then reporting what the action left at each output path — a file, a directory with everything below it, or neither — and storing every regular file of it|`Bare` (tier 1)|`buildl`|1|
   |`ExecStrategy`|the same, with undeclared inputs absent|`InputSandbox` (tier 2)|`buildl`|3|
   |`Dispatcher`|running ready actions in parallel, and carrying what a running action says to the scheduling thread|std scoped threads + `mpsc`, started as tasks need them|`buildl`|1|
   |`Approver`|turning the wanted set into grants|interactive, manifest|`buildl`|4|
   |`Clock`|log timestamps and durations|system clock|`buildl`|1|
   |`GraphRecord`|keeping the graph a build resolved|`.buildl/graph.json`: the canonical bytes, written whole by every build|`buildl`|1|
   |`Reporter`|output blocks and the status line|one terminal reporter with a live and a plain mode|`buildl`|1|
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'to the dispatcher and the strategy; declaration limits, settings|`buildl.toml` reader|`buildl`|' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '"Since" is the item of design §13 an adapter arrives with; item 1 is built, and a row with a lat' docs/architecture-building-blocks.md
   1
   ~~~~

4. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

5. Commit:

   ~~~~text
   $ git add docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): give the port catalog of building blocks §6 the adapters as built"
   ~~~~

### Task 23 — Name the adapter-isolation guard in the dependency rules

**Files:**
- Modify `docs/architecture-building-blocks.md`
- Modify `CLAUDE.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '2. **Adapters depend on `buildl-core`.** `buildl-lua` never depends on `buildl`; inside `buildl`' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'of the crate; adapters meet only in the two composition modules, `local_ports` and `workspace`.' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e '|adapter isolation inside `buildl`|review, until an adapter earns its own crate (§11)|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|adapter isolation inside `buildl`|`cargo make guard-adapter-isolation`, which reads every file' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'and adapter modules inside `buildl` never import each other.' CLAUDE.md
   1
   $ grep -c -F -e 'and adapter modules inside `buildl` never import each other (`cargo make guard-adapter-isolation' CLAUDE.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in §7 rule 2, say where what adapters share lives. Replace:

   ~~~~markdown
   2. **Adapters depend on `buildl-core`.** `buildl-lua` never depends on `buildl`; inside `buildl`, adapter modules never import each other.
   ~~~~

   With:

   ~~~~markdown
   2. **Adapters depend on `buildl-core`.** `buildl-lua` never depends on `buildl`; inside `buildl`, adapter modules never import each other. What several adapters need lives in two shared modules, `state` and `search_path`, which implement no port and name no module of the crate; adapters meet only in the two composition modules, `local_ports` and `workspace`.
   ~~~~

3. In `docs/architecture-building-blocks.md`, in the table below the rules, replace the last row: the rule is checked by a gate task now. Replace:

   ~~~~markdown
   |adapter isolation inside `buildl`|review, until an adapter earns its own crate (§11)|
   ~~~~

   With:

   ~~~~markdown
   |adapter isolation inside `buildl`|`cargo make guard-adapter-isolation`, which reads every file under `crates/buildl/src` but `lib.rs`, test code included: an adapter module may name `crate::state` and `crate::search_path` and no other module of the crate, the two shared modules name none, and only `local_ports` and `workspace` name any module they like. A grouped `crate::{…}` import, `super::super`, a `super::` in a `mod.rs` or in a module that is one file, and an item named through the crate root (`crate::<Item>`) are refused outright, because each would hide an edge|
   ~~~~

4. In `CLAUDE.md`, in the structural rules, name the guard and the two shared modules. Replace:

   ~~~~markdown
   and adapter modules inside `buildl` never import each other.
   ~~~~

   With:

   ~~~~markdown
   and adapter modules inside `buildl` never import each other (`cargo make guard-adapter-isolation`); what they share lives in `state` and `search_path`.
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '2. **Adapters depend on `buildl-core`.** `buildl-lua` never depends on `buildl`; inside `buildl`' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'of the crate; adapters meet only in the two composition modules, `local_ports` and `workspace`.' docs/architecture-building-blocks.md
   1
   # step 3
   $ grep -c -F -e '|adapter isolation inside `buildl`|review, until an adapter earns its own crate (§11)|' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e '|adapter isolation inside `buildl`|`cargo make guard-adapter-isolation`, which reads every file' docs/architecture-building-blocks.md
   1
   # step 4
   $ grep -c -F -e 'and adapter modules inside `buildl` never import each other.' CLAUDE.md
   0
   $ grep -c -F -e 'and adapter modules inside `buildl` never import each other (`cargo make guard-adapter-isolation' CLAUDE.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/architecture-building-blocks.md CLAUDE.md
   $ git commit -m "docs(repo): name the adapter-isolation guard in the dependency rules"
   ~~~~

### Task 24 — Describe the test layers of buildl and buildl-cli in building blocks §8

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'ADP["buildl adapters<br/>cas atomicity, strategies against helper binaries"]' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'ADP["buildl adapters<br/>each module alone: a temp directory, system tools run by argv"]' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e 'E2E["buildl/tests<br/>real adapters: a fixture workspace built twice, byte-identical"]' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'E2E["buildl/tests/e2e<br/>real adapters through the Workspace facade:<br/>a fixture workspace bu' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'CLIT["buildl-cli<br/>argument parsing, exit-class mapping"]' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'CLIT["buildl-cli<br/>argument parsing and exit mapping in process,<br/>then the built binary"]' docs/architecture-building-blocks.md
   0
   # step 5
   $ grep -c -F -e 'The fakes live in `buildl-core`' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'The four layers that touch real I/O, in `buildl` and `buildl-cli`:' docs/architecture-building-blocks.md
   0
   # step 6
   $ grep -c -F -e '|`Clock`|a counter, one tick per read|wall-clock time|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|`GraphRecord`|the graphs recorded, and whether to fail|a filesystem|' docs/architecture-building-blocks.md
   0
   # step 7
   $ grep -c -F -e '|a build file enqueues subdirectories out of order|merged declarations sorted by directory, then' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'The same promises against the real adapters, through the `Workspace` facade (`crates/buildl/test' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in the diagram of §8, which stays Mermaid, say what the adapter tests use. Replace:

   ~~~~markdown
   ADP["buildl adapters<br/>cas atomicity, strategies against helper binaries"]
   ~~~~

   With:

   ~~~~markdown
   ADP["buildl adapters<br/>each module alone: a temp directory, system tools run by argv"]
   ~~~~

3. In `docs/architecture-building-blocks.md`, in the same diagram, say what the end-to-end tests go through. Replace:

   ~~~~markdown
   E2E["buildl/tests<br/>real adapters: a fixture workspace built twice, byte-identical"]
   ~~~~

   With:

   ~~~~markdown
   E2E["buildl/tests/e2e<br/>real adapters through the Workspace facade:<br/>a fixture workspace built twice, byte-identical"]
   ~~~~

4. In `docs/architecture-building-blocks.md`, in the same diagram, name the two CLI layers. Replace:

   ~~~~markdown
   CLIT["buildl-cli<br/>argument parsing, exit-class mapping"]
   ~~~~

   With:

   ~~~~markdown
   CLIT["buildl-cli<br/>argument parsing and exit mapping in process,<br/>then the built binary"]
   ~~~~

5. In `docs/architecture-building-blocks.md`, below the diagram, before the paragraph on the fakes, add the table of the layers that touch real I/O. Replace:

   ~~~~markdown
   The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`.
   ~~~~

   With:

   ~~~~markdown
   The four layers that touch real I/O, in `buildl` and `buildl-cli`:

   |Layer|Lives in|May touch|Must not|
   |---|---|---|---|
   |Adapter unit|`crates/buildl/src/<module>/`, beside the code|its own module, `state`, `search_path`, `buildl-core`, one temp directory; `exec` starts system tools|name another adapter module; read the process's current directory; set an environment variable|
   |End to end|`crates/buildl/tests/e2e`, with fixtures in `crates/buildl/tests/fixtures`|`buildl`'s public API; a temp copy of a fixture|a private item; a path outside the package|
   |CLI unit|`crates/buildl-cli/src`, beside the code|argument parsing, the exit mapping, terminal detection, a command's flow over byte buffers and a temp workspace|a process; a terminal|
   |CLI binary|`crates/buildl-cli/tests/cli`|the built binary, with a chosen working directory and environment and its output piped|`buildl` internals; a terminal|

   The tools these tests run are system tools — `sh`, `cp`, `sort`, `true`, `false`, `sleep` and their like — run by argv; `sh -c` appears in tests only, never in buildl. There is no compiled helper program and no shared kit of fakes (§10, §11).

   The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`.
   ~~~~

6. In `docs/architecture-building-blocks.md`, in the table of fakes, add the two new ports after `Clock`. Replace:

   ~~~~markdown
   |`Clock`|a counter, one tick per read|wall-clock time|
   ~~~~

   With:

   ~~~~markdown
   |`Clock`|a counter, one tick per read|wall-clock time|
   |`GraphRecord`|the graphs recorded, and whether to fail|a filesystem|
   |`Reporter`|every call kept as an owned value, in order|a terminal|
   ~~~~

7. In `docs/architecture-building-blocks.md`, below the table of flows, add the scenarios that run against the real adapters. Replace:

   ~~~~markdown
   |a build file enqueues subdirectories out of order|merged declarations sorted by directory, then declared name|
   ~~~~

   With:

   ~~~~markdown
   |a build file enqueues subdirectories out of order|merged declarations sorted by directory, then declared name|

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
   |a failure, without and with `--keep-going`|`1 failed, 4 skipped`, then `1 failed, 2 skipped, 2 built`; no row for the failed target, which runs again next time|
   |the states an interrupted build leaves: blobs with no row, rows in the journal, staging files, a partial output|the next build finishes: it trusts no blob without a row, folds the journal, sweeps the staging and clears the partial output|
   |a corrupt `cache.json`|`Error::CorruptState` naming the file, the file untouched, no action run; `check` and `graph` still work|
   |a workspace another buildl holds|`plan` and `build` refused; `check` and `graph` succeed|
   |a strategy other than `bare`; a state directory outside `fs.write`|`build` refused and `plan` not; `plan` refused before `.buildl/` is made|
   ~~~~

8. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e 'ADP["buildl adapters<br/>cas atomicity, strategies against helper binaries"]' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'ADP["buildl adapters<br/>each module alone: a temp directory, system tools run by argv"]' docs/architecture-building-blocks.md
   1
   # step 3
   $ grep -c -F -e 'E2E["buildl/tests<br/>real adapters: a fixture workspace built twice, byte-identical"]' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'E2E["buildl/tests/e2e<br/>real adapters through the Workspace facade:<br/>a fixture workspace bu' docs/architecture-building-blocks.md
   1
   # step 4
   $ grep -c -F -e 'CLIT["buildl-cli<br/>argument parsing, exit-class mapping"]' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'CLIT["buildl-cli<br/>argument parsing and exit mapping in process,<br/>then the built binary"]' docs/architecture-building-blocks.md
   1
   # step 5
   $ grep -c -F -e 'The fakes live in `buildl-core`' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'The four layers that touch real I/O, in `buildl` and `buildl-cli`:' docs/architecture-building-blocks.md
   1
   # step 6
   $ grep -c -F -e '|`Clock`|a counter, one tick per read|wall-clock time|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|`GraphRecord`|the graphs recorded, and whether to fail|a filesystem|' docs/architecture-building-blocks.md
   1
   # step 7
   $ grep -c -F -e '|a build file enqueues subdirectories out of order|merged declarations sorted by directory, then' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'The same promises against the real adapters, through the `Workspace` facade (`crates/buildl/test' docs/architecture-building-blocks.md
   1
   ~~~~

9. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

10. Commit:

    ~~~~text
    $ git add docs/architecture-building-blocks.md
    $ git commit -m "docs(repo): describe the test layers of buildl and buildl-cli in building blocks §8"
    ~~~~

### Task 25 — Record the composition decisions and the publishing rules in the building blocks

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '|crate `README.md`|written for crates.io readers of that crate|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'Nothing is published yet. Four rules keep publishing possible, and every change to the workspace' docs/architecture-building-blocks.md
   0
   # step 3
   $ grep -c -F -e 's dedicated C4 syntax is marked experimental.|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|Re-exporting the core|**`pub use buildl_core;` — the crate under its own name, `buildl::buildl_' docs/architecture-building-blocks.md
   0
   # step 4
   $ grep -c -F -e 'Whether a shared test kit of fake ports is published or kept internal, once `buildl` or `buildl-' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'Settled: no test kit of fake ports is published, and none is shared between crates. `buildl`' docs/architecture-building-blocks.md
   0
   ~~~~

2. In `docs/architecture-building-blocks.md`, in §9, below the requirements table, add the four rules that keep publishing possible. Replace:

   ~~~~markdown
   |crate `README.md`|written for crates.io readers of that crate|
   ~~~~

   With:

   ~~~~markdown
   |crate `README.md`|written for crates.io readers of that crate|

   Nothing is published yet. Four rules keep publishing possible, and every change to the workspace follows them:

   1. Every dependency between members is `{ workspace = true }`, and no member depends, dev-dependencies included, on a member later in the publish order: `cargo package` fails on one, and a dev-dependency given by path alone is silently dropped from the packaged manifest.
   2. The workspace has no fifth member, and `buildl` gains no `[[bin]]`, `[[example]]` or `[features]` for the sake of tests.
   3. Fixtures and golden files live inside the package whose tests read them, reached through `CARGO_MANIFEST_DIR` or `include_str!`; no test reads another package's directory.
   4. No fixture path holds a component named `target` or `debug`.
   ~~~~

3. In `docs/architecture-building-blocks.md`, in §10, add five rows at the end of the table, after its last row. Replace:

   ~~~~markdown
   Mermaid's dedicated C4 syntax is marked experimental.|
   ~~~~

   With:

   ~~~~markdown
   Mermaid's dedicated C4 syntax is marked experimental.|
   |Re-exporting the core|**`pub use buildl_core;` — the crate under its own name, `buildl::buildl_core`**|`pub use buildl_core as core` is unusable: in a crate root that also names `core::fmt` it is `error[E0432]: unresolved import core::fmt`, and a downstream crate that glob-imports the re-export and names `core::fmt` gets `error[E0659]: core is ambiguous`. A third name would invent vocabulary. The facade `buildl::Workspace` and the manifest value `buildl::buildl_core::Workspace` stay apart.|
   |When ports are wired|**Per `plan` or `build` call; a source of build files alone for `check` and `graph`**|`Pipeline` needs every port, and a command that reads only build files must not need, lock or be stopped by a state file: a corrupt `cache.json` does not stop `check`. The state lock is held for one call, so an embedder that keeps a `Workspace` open does not hold the directory while idle.|
   |Who owns the reporter|**The caller: `Workspace::build` is lent `&mut R` for one build**|Only `build` reports, and an embedder supplies its own reporter. `LocalPorts<R>` is generic over it, and `&mut R` is a `Reporter` when `R` is, so the bundle needs no bound beyond that.|
   |Tools in tests|**System tools by argv; no compiled helper, no published kit of fakes**|Cargo sets `CARGO_BIN_EXE_<name>` only for integration tests of the package that owns the binary, so a helper would cost the published `buildl` crate a `[[bin]]` or a feature, or the workspace a fifth member (§9).|
   |Adapter isolation|**Checked by `cargo make guard-adapter-isolation`**|Thirteen adapter modules in one crate is where a cross-import slips in. What they share lives in `state` and `search_path`, which implement no port (§7 rule 2).|
   ~~~~

4. In `docs/architecture-building-blocks.md`, in §11, close the question on a shared test kit. Replace:

   ~~~~markdown
   - Whether a shared test kit of fake ports is published or kept internal, once `buildl` or `buildl-lua` tests need `buildl-core`'s fakes.
   ~~~~

   With:

   ~~~~markdown
   - Settled: no test kit of fake ports is published, and none is shared between crates. `buildl`'s adapter tests need another port in one place only — collecting an action's outputs into a content store — and write a small in-memory store there (§8).
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '|crate `README.md`|written for crates.io readers of that crate|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e 'Nothing is published yet. Four rules keep publishing possible, and every change to the workspace' docs/architecture-building-blocks.md
   1
   # step 3
   $ grep -c -F -e 's dedicated C4 syntax is marked experimental.|' docs/architecture-building-blocks.md
   1
   $ grep -c -F -e '|Re-exporting the core|**`pub use buildl_core;` — the crate under its own name, `buildl::buildl_' docs/architecture-building-blocks.md
   1
   # step 4
   $ grep -c -F -e 'Whether a shared test kit of fake ports is published or kept internal, once `buildl` or `buildl-' docs/architecture-building-blocks.md
   0
   $ grep -c -F -e 'Settled: no test kit of fake ports is published, and none is shared between crates. `buildl`' docs/architecture-building-blocks.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add docs/architecture-building-blocks.md
   $ git commit -m "docs(repo): record the composition decisions and the publishing rules in the building blocks"
   ~~~~

### Task 26 — Describe the two new ports and narrowing in the buildl-core README

**Files:**
- Modify `crates/buildl-core/README.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '| Identity | `Digest` (SHA-256), `NodeId` (a graph arena index),' crates/buildl-core/README.md
   1
   $ grep -c -F -e '| Identity | `Digest` (SHA-256), with `Hasher`, which takes bytes in pieces and gives the digest' crates/buildl-core/README.md
   0
   # step 3
   $ grep -c -F -e '`Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ |' crates/buildl-core/README.md
   1
   $ grep -c -F -e '`Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ, an' crates/buildl-core/README.md
   0
   # step 4
   $ grep -c -F -e '`Pipeline::graph` runs Load once, then Resolve |' crates/buildl-core/README.md
   1
   $ grep -c -F -e '`narrow` cuts a graph down to the targets a run was asked for and everything they depend on, and' crates/buildl-core/README.md
   0
   # step 5
   $ grep -c -F -e '| Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`,' crates/buildl-core/README.md
   1
   $ grep -c -F -e '| Plan ports | `Manifest`, `Host`, `Digester` — which says what stands at an input' crates/buildl-core/README.md
   0
   # step 6
   $ grep -c -F -e '`Pipeline::plan` runs Load once, Resolve, then Plan |' crates/buildl-core/README.md
   1
   $ grep -c -F -e '`Pipeline::plan` takes a `Scope` — the whole workspace, or named targets — and the setting overr' crates/buildl-core/README.md
   0
   # step 7
   $ grep -c -F -e 'and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in on' crates/buildl-core/README.md
   1
   $ grep -c -F -e '`Dispatcher`, which also carries what a running action says to the thread that schedules, `Graph' crates/buildl-core/README.md
   0
   # step 8
   $ grep -c -F -e '`Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` p' crates/buildl-core/README.md
   1
   $ grep -c -F -e 'a command a signal ended fails its target as `Killed`; `Pipeline::build` takes a `Scope`, the se' crates/buildl-core/README.md
   0
   # step 9
   $ grep -c -F -e 's name the target and build file behind a refused want, a missing tool or a missing input, and t' crates/buildl-core/README.md
   1
   $ grep -c -F -e 'a run asked for a target nothing declares is told so, with the nearest label that is declared; P' crates/buildl-core/README.md
   0
   # step 10
   $ grep -c -F -e 's own failure |' crates/buildl-core/README.md
   1
   $ grep -c -F -e 's own failure, and a state file that does not hold what was written there is named, with the rea' crates/buildl-core/README.md
   0
   # step 11
   $ grep -c -F -e 'The adapters that run a real build belong to' crates/buildl-core/README.md
   1
   $ grep -c -F -e 'The adapters that run a real build are in' crates/buildl-core/README.md
   0
   ~~~~

2. In `crates/buildl-core/README.md`, in the table, name `Hasher` in the Identity row. Replace:

   ~~~~markdown
   | Identity | `Digest` (SHA-256), `NodeId` (a graph arena index),
   ~~~~

   With:

   ~~~~markdown
   | Identity | `Digest` (SHA-256), with `Hasher`, which takes bytes in pieces and gives the digest of the pieces joined; `NodeId` (a graph arena index),
   ~~~~

3. In `crates/buildl-core/README.md`, at the end of the Load row, name the free function. Replace:

   ~~~~markdown
   `Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ |
   ~~~~

   With:

   ~~~~markdown
   `Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ, and `pipeline::check` does the same for a caller that holds only a source of build files |
   ~~~~

4. In `crates/buildl-core/README.md`, at the end of the Resolve row, name `narrow` and the free function. Replace:

   ~~~~markdown
   `Pipeline::graph` runs Load once, then Resolve |
   ~~~~

   With:

   ~~~~markdown
   `narrow` cuts a graph down to the targets a run was asked for and everything they depend on, and refuses a label nothing declares; `Pipeline::graph` runs Load once, then Resolve, and `pipeline::graph` does the same over a source alone |
   ~~~~

5. In `crates/buildl-core/README.md`, in the Plan ports row, say what `Digester` reports about an input. Replace:

   ~~~~markdown
   | Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`,
   ~~~~

   With:

   ~~~~markdown
   | Plan ports | `Manifest`, `Host`, `Digester` — which says what stands at an input's path as an `InputStat`: nothing, a regular file, or something that is neither — `StatCache`, `ToolResolver` and `ActionCache`,
   ~~~~

6. In `crates/buildl-core/README.md`, at the end of the Plan row, give `Pipeline::plan` its arguments. Replace:

   ~~~~markdown
   `Pipeline::plan` runs Load once, Resolve, then Plan |
   ~~~~

   With:

   ~~~~markdown
   `Pipeline::plan` takes a `Scope` — the whole workspace, or named targets — and the setting overrides, and runs Load once, Resolve, the narrowing, then Plan |
   ~~~~

7. In `crates/buildl-core/README.md`, at the end of the Build ports row, add the two ports and count fourteen. Replace:

   ~~~~markdown
   and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in one `PortSet` |
   ~~~~

   With:

   ~~~~markdown
   `Dispatcher`, which also carries what a running action says to the thread that schedules, `GraphRecord`, which keeps the graph a build resolved, and `Reporter`, five calls that show a build as it runs, with `ActionCache::record` and a `Clock` a worker may read: fourteen ports in one `PortSet` |
   ~~~~

8. In `crates/buildl-core/README.md`, at the end of the Execute and Record row, name `Killed` and give `Pipeline::build` its arguments. Replace:

   ~~~~markdown
   `Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` per target |
   ~~~~

   With:

   ~~~~markdown
   a command a signal ended fails its target as `Killed`; `Pipeline::build` takes a `Scope`, the setting overrides, the failure policy and `LiveOutput` — whether one action is shown as it prints — and runs Load once and Resolve, records the whole graph, narrows it, then runs Plan and Execute, showing each step through the `Reporter`, and returns one `ActionOutcome` for each target asked for and each target those depend on |
   ~~~~

9. In `crates/buildl-core/README.md`, in the Errors row, add two of the new errors: a target nothing declares, and an input that is not a regular file. Replace:

   ~~~~markdown
   Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure;
   ~~~~

   With:

   ~~~~markdown
   a run asked for a target nothing declares is told so, with the nearest label that is declared; Plan's name the target and build file behind a refused want, a missing tool, a missing input or an input that is not a regular file, and the port behind an adapter's own failure;
   ~~~~

10. In `crates/buildl-core/README.md`, at the end of the same row, add the error that names a state file. Replace:

    ~~~~markdown
    JSON that cannot be read back is told apart as not readable or not canonical, and from an adapter's own failure |
    ~~~~

    With:

    ~~~~markdown
    JSON that cannot be read back is told apart as not readable or not canonical, and from an adapter's own failure, and a state file that does not hold what was written there is named, with the reason kept |
    ~~~~

11. In `crates/buildl-core/README.md`, in the status paragraph, say where the adapters are. Replace:

    ~~~~markdown
    The adapters that run a real build belong to
    other crates and are not written yet.
    ~~~~

    With:

    ~~~~markdown
    The adapters that run a real build are in
    [`buildl-lua`](../buildl-lua) and [`buildl`](../buildl).
    ~~~~

12. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

    ~~~~text
    # step 2
    $ grep -c -F -e '| Identity | `Digest` (SHA-256), `NodeId` (a graph arena index),' crates/buildl-core/README.md
    0
    $ grep -c -F -e '| Identity | `Digest` (SHA-256), with `Hasher`, which takes bytes in pieces and gives the digest' crates/buildl-core/README.md
    1
    # step 3
    $ grep -c -F -e '`Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ |' crates/buildl-core/README.md
    0
    $ grep -c -F -e '`Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ, an' crates/buildl-core/README.md
    1
    # step 4
    $ grep -c -F -e '`Pipeline::graph` runs Load once, then Resolve |' crates/buildl-core/README.md
    0
    $ grep -c -F -e '`narrow` cuts a graph down to the targets a run was asked for and everything they depend on, and' crates/buildl-core/README.md
    1
    # step 5
    $ grep -c -F -e '| Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`,' crates/buildl-core/README.md
    0
    $ grep -c -F -e '| Plan ports | `Manifest`, `Host`, `Digester` — which says what stands at an input' crates/buildl-core/README.md
    1
    # step 6
    $ grep -c -F -e '`Pipeline::plan` runs Load once, Resolve, then Plan |' crates/buildl-core/README.md
    0
    $ grep -c -F -e '`Pipeline::plan` takes a `Scope` — the whole workspace, or named targets — and the setting overr' crates/buildl-core/README.md
    1
    # step 7
    $ grep -c -F -e 'and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in on' crates/buildl-core/README.md
    0
    $ grep -c -F -e '`Dispatcher`, which also carries what a running action says to the thread that schedules, `Graph' crates/buildl-core/README.md
    1
    # step 8
    $ grep -c -F -e '`Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` p' crates/buildl-core/README.md
    0
    $ grep -c -F -e 'a command a signal ended fails its target as `Killed`; `Pipeline::build` takes a `Scope`, the se' crates/buildl-core/README.md
    1
    # step 9
    $ grep -c -F -e 's name the target and build file behind a refused want, a missing tool or a missing input, and t' crates/buildl-core/README.md
    0
    $ grep -c -F -e 'a run asked for a target nothing declares is told so, with the nearest label that is declared; P' crates/buildl-core/README.md
    1
    # step 10
    $ grep -c -F -e 's own failure |' crates/buildl-core/README.md
    0
    $ grep -c -F -e 's own failure, and a state file that does not hold what was written there is named, with the rea' crates/buildl-core/README.md
    1
    # step 11
    $ grep -c -F -e 'The adapters that run a real build belong to' crates/buildl-core/README.md
    0
    $ grep -c -F -e 'The adapters that run a real build are in' crates/buildl-core/README.md
    1
    ~~~~

13. Run the gate; both commands exit `0`:

    ~~~~text
    $ cargo make dod
    $ cargo deny check
    ~~~~

14. Commit:

    ~~~~text
    $ git add crates/buildl-core/README.md
    $ git commit -m "docs(buildl-core): describe the two new ports and narrowing in the README"
    ~~~~

### Task 27 — Say in the buildl-lua README that buildl composes the adapter

**Files:**
- Modify `crates/buildl-lua/README.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'The adapter is complete for the declaration phase; nothing in buildl composes it into a command' crates/buildl-lua/README.md
   1
   $ grep -c -F -e 'The adapter is complete for the declaration phase; [`buildl`](../buildl) composes it with the pi' crates/buildl-lua/README.md
   0
   ~~~~

2. In `crates/buildl-lua/README.md`, in the status line, say what composes the adapter. Replace:

   ~~~~markdown
   The adapter is complete for the declaration phase; nothing in buildl composes it into a command yet.
   ~~~~

   With:

   ~~~~markdown
   The adapter is complete for the declaration phase; [`buildl`](../buildl) composes it with the pipeline, and the `check`, `graph`, `plan` and `build` commands evaluate build files through it.
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'The adapter is complete for the declaration phase; nothing in buildl composes it into a command' crates/buildl-lua/README.md
   0
   $ grep -c -F -e 'The adapter is complete for the declaration phase; [`buildl`](../buildl) composes it with the pi' crates/buildl-lua/README.md
   1
   ~~~~

4. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

5. Commit:

   ~~~~text
   $ git add crates/buildl-lua/README.md
   $ git commit -m "docs(buildl-lua): say in the README that buildl composes the adapter"
   ~~~~

### Task 28 — Describe the adapters and the facade in the buildl README

**Files:**
- Modify `crates/buildl/README.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'This is the framework crate and the composition root. Its role is to bind concrete adapters to t' crates/buildl/README.md
   1
   $ grep -c -F -e 'This is the framework crate and the composition root. It binds concrete adapters to the pipeline' crates/buildl/README.md
   0
   ~~~~

2. In `crates/buildl/README.md`, replace lines 5 to 12 — from "This is the framework crate" to the status line — with the text below. The file holds no Rust code block before or after this edit. Replace:

   ~~~~markdown
   This is the framework crate and the composition root. Its role is to bind concrete adapters to the pipeline defined in [`buildl-core`](../buildl-core):

   - the adapters for every port except `DeclarationSource` — `Manifest`, `Digester`, `StatCache`, `ToolResolver`, `ContentStore`, `ActionCache`, `EventLog`, `ExecStrategy`, `Dispatcher`, `Approver`, `Clock` and `Reporter`;
   - the Lua evaluator from [`buildl-lua`](../buildl-lua), which implements `DeclarationSource`.

   It will assemble them as `LocalPorts`, expose the result as the `Workspace` facade, and re-export `buildl-core`. The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

   **Status:** pre-release; none of the adapters or the assembly exist yet, and the crate has no public API.
   ~~~~

   With:

   ~~~~markdown
   This is the framework crate and the composition root. It binds concrete adapters to the pipeline defined in [`buildl-core`](../buildl-core):

   - an adapter for every port a local build uses except `DeclarationSource` — `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ContentStore`, `ActionCache`, `EventLog`, `GraphRecord`, `ExecStrategy`, `Dispatcher`, `Clock` and `Reporter`;
   - the Lua evaluator from [`buildl-lua`](../buildl-lua), which implements `DeclarationSource`.

   It assembles them as `LocalPorts`, exposes the result as the `Workspace` facade, and re-exports `buildl-core` as `buildl::buildl_core`. The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

   ## What it holds today

   | | |
   |---|---|
   | Facade | `Workspace::open(start)` finds the workspace at the nearest `buildl.toml` at or above `start` and reads it, writing nothing. `check`, `graph`, `plan` and `build` run the pipeline that far; `label` reads a target as a person names it — `//dir:name`, or `:name` and `name` in the directory the workspace was opened from. `build` is lent the caller's `Reporter` for that one build |
   | Composition | `LocalPorts<R>`, the bundle of real adapters, generic over the reporter; `LocalPipeline<R>`; and `Wiring`, which makes every port for one `plan` or one `build`. `check` and `graph` make a source of build files and nothing else, so neither needs a state directory, takes the lock or is stopped by a state file |
   | Reading inputs | `manifest` — `buildl.toml`, read strictly: a key it does not know is an error with its line and column — and the workspace root; `host` — the platform and environment values; `digest` — what stands at an input's path, and file hashes in parallel; `tools` — a program's fingerprint, the SHA-256 of the file its name leads to |
   | State on disk | `cas` — `.buildl/cas`, read-only blobs with an executable twin where one is needed, and the files placed under `out/`; `cache` — `cache.json` and its journal; `log` — `log.jsonl`; `statcache` — `statcache.json`; `graph` — `graph.json`, written by every build |
   | Running actions | `exec` — `Bare`, isolation tier 1: a plain process in the workspace root, started without a shell, with exactly its declared environment, an empty standard input and a timeout; `dispatch` — worker threads, started as tasks need them; `clock` — the system clock |
   | Reporting | `report` — `TerminalReporter`, with a live status line or plain blocks; `Silent`; and the text of a plan, a graph as DOT, and an error |
   | Shared | `state` — where each state file is, the lock that keeps a second buildl out, writing a file whole, clearing a path — and `search_path`, the one lookup of a program name. Neither implements a port, and adapter modules name no other module of the crate |

   The crate builds on Unix targets only; the supported platforms are Linux and macOS.

   **Status:** pre-release. Every port a local build uses has its adapter, and a workspace is built end to end through `Workspace`. Actions run at isolation tier 1 only: `build` refuses a manifest whose `[execution]` `strategy` is anything but `"bare"`, the default included. The input sandbox and the `Approver` port are designed and are not in this crate.
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'This is the framework crate and the composition root. Its role is to bind concrete adapters to t' crates/buildl/README.md
   0
   $ grep -c -F -e 'This is the framework crate and the composition root. It binds concrete adapters to the pipeline' crates/buildl/README.md
   1
   ~~~~

4. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

5. Commit:

   ~~~~text
   $ git add crates/buildl/README.md
   $ git commit -m "docs(buildl): describe the adapters and the facade in the README"
   ~~~~

### Task 29 — Describe the commands and the exit codes in the buildl-cli README

**Files:**
- Modify `crates/buildl-cli/README.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e 'It is a thin shell: it will parse arguments for the `check`, `graph`, `plan` and `build` command' crates/buildl-cli/README.md
   1
   $ grep -c -F -e 'It is a thin shell: it parses arguments, calls into `buildl`, writes what the library rendered,' crates/buildl-cli/README.md
   0
   ~~~~

2. In `crates/buildl-cli/README.md`, replace lines 5 to 7 — from "It is a thin shell" to the status line — with the text below. The file holds no Rust code block before or after this edit. Replace:

   ~~~~markdown
   It is a thin shell: it will parse arguments for the `check`, `graph`, `plan` and `build` commands, call into `buildl`, and map the outcome class to an exit code. All build logic lives in the library crates.

   **Status:** pre-release; the binary provides no commands.
   ~~~~

   With:

   ~~~~markdown
   It is a thin shell: it parses arguments, calls into `buildl`, writes what the library rendered, and maps how the command ended to an exit code. All build logic lives in the library crates.

   ## What it holds today

   | Command | Does |
   |---|---|
   | `buildl check` | evaluates every build file twice and compares the two; runs nothing and writes nothing |
   | `buildl graph` | prints the target graph on stdout: canonical JSON with `--json`, the default, or DOT with `--dot` |
   | `buildl plan [TARGET]...` | prints on stdout what would run, and why; `--json` gives the plan as canonical JSON |
   | `buildl build [TARGET]...` | builds the targets and what they need; blocks, a status line on a terminal and a summary go to stderr. `-k`, `--keep-going` carries on past a failure; `--stream` shows one running action's output as it is printed |

   A target is `//dir:name`, or `:name` and `name`, read against the current directory; with none, every target is taken. `plan` and `build` accept `--set NAME=VALUE`, repeated, to give a build setting a value for the run. `--color auto|always|never` goes with every command. The workspace is the nearest `buildl.toml` at or above the current directory.

   | Exit code | Means |
   |---|---|
   | 0 | done |
   | 1 | an action failed: fix your code |
   | 2 | the command line is wrong |
   | 3 | fix your builder, or run again: a missing tool, a timeout, a file that cannot be read or written, a workspace another buildl is using |
   | 4 | the build wants more than the ceiling |
   | 5 | a build file or `buildl.toml` is wrong |
   | 6 | a state file is not what buildl writes |
   | 7 | buildl is at fault |

   The binary builds on Unix targets only; the supported platforms are Linux and macOS.

   **Status:** pre-release. The four commands above are the whole surface.
   ~~~~

3. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`).

   ~~~~text
   # step 2
   $ grep -c -F -e 'It is a thin shell: it will parse arguments for the `check`, `graph`, `plan` and `build` command' crates/buildl-cli/README.md
   0
   $ grep -c -F -e 'It is a thin shell: it parses arguments, calls into `buildl`, writes what the library rendered,' crates/buildl-cli/README.md
   1
   ~~~~

4. Confirm no crate README holds a fenced code block, so none can become a doctest should a crate ever include its README as crate documentation (`grep -rn -F 'include_str!("../README.md")' crates/` printed nothing when this plan was written). Each count is `0`:

   ~~~~text
   $ grep -c -e '^```' crates/buildl/README.md crates/buildl-cli/README.md crates/buildl-core/README.md crates/buildl-lua/README.md
   crates/buildl/README.md:0
   crates/buildl-cli/README.md:0
   crates/buildl-core/README.md:0
   crates/buildl-lua/README.md:0
   ~~~~

5. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

6. Commit:

   ~~~~text
   $ git add crates/buildl-cli/README.md
   $ git commit -m "docs(buildl-cli): describe the commands and the exit codes in the README"
   ~~~~

### Task 30 — Bring the README up to a buildl that builds

**Files:**
- Modify `README.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '> **Status: design settled, implementation under way.** `buildl-core` ships the validated domain' README.md
   1
   $ grep -c -F -e '> **Status: the core pipeline is built.** The `buildl` binary runs `check`, `graph`, `plan` and' README.md
   0
   # step 3
   $ grep -c -F -e '| `buildl graph` | Resolve | emit the DAG (`--json`, `--dot`) |' README.md
   1
   $ grep -c -F -e '| Command | Stops after | Purpose | Built |' README.md
   0
   # step 4
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos' README.md
   1
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos; with it the `test`, `log`,' README.md
   0
   ~~~~

2. In `README.md`, replace line 7, the status blockquote. Replace:

   ~~~~markdown
   > **Status: design settled, implementation under way.** `buildl-core` ships the validated domain vocabulary, the one error enum, the canonical JSON serializer and the reader that takes back only what it writes, the twelve ports the pipeline reaches everything outside it through, and every pipeline phase — Load, Resolve, Plan, Execute and Record, with `check`, `graph`, `plan` and `build`, whose flows run against in-memory fakes; the adapters that run a real build are not implemented yet. `buildl-lua` evaluates build files on airsl, and no command composes it with the pipeline yet. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ~~~~

   With:

   ~~~~markdown
   > **Status: the core pipeline is built.** The `buildl` binary runs `check`, `graph`, `plan` and `build` on a real workspace: it evaluates build files on airsl, keys every target, runs what is dirty in parallel with its output captured, and keeps results in a local content-addressed store and action cache, so a second build runs nothing and a build repeated in another directory is byte-identical. Actions run at isolation tier 1, a plain process holding only its declared environment. That is the first item of the [roadmap](#roadmap); the input sandbox, grant negotiation, plugins, the remote cache and watch mode are designed and not built. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ~~~~

3. In `README.md`, in the Commands section, replace the whole table, its header included, and add what follows it: a paragraph, and a new section on running buildl. Replace:

   ~~~~markdown
   | Command | Stops after | Purpose |
   |---|---|---|
   | `buildl check` | Load | syntax-check every build file, run nothing |
   | `buildl graph` | Resolve | emit the DAG (`--json`, `--dot`) |
   | `buildl plan` | Plan | dry run: what would rebuild, and why |
   | `buildl build [target]` | Record | the full pipeline |
   | `buildl watch [target]` | loops | persistent process, re-plans on file change |
   | `buildl query <expr>` | Resolve | graph queries: `deps()`, `rdeps()` |
   | `buildl test` / `clean` / `gc` | — | run test targets; drop outputs; prune the cas |
   | `buildl doctor` | — | print the effective ceiling, grants, workers |
   ~~~~

   With:

   ~~~~markdown
   | Command | Stops after | Purpose | Built |
   |---|---|---|---|
   | `buildl check` | Load | syntax-check every build file, run nothing | yes |
   | `buildl graph` | Resolve | emit the DAG (`--json`, the default, or `--dot`) | yes |
   | `buildl plan [target]...` | Plan | dry run: what would rebuild, and why | yes |
   | `buildl build [target]...` | Record | the full pipeline | yes |
   | `buildl watch [target]` | loops | persistent process, re-plans on file change | not yet |
   | `buildl query <expr>` | Resolve | graph queries: `deps()`, `rdeps()` | not yet |
   | `buildl test` / `clean` / `gc` | — | run test targets; drop outputs; prune the cas | not yet |
   | `buildl log <target>` | — | show what a target printed when it last ran | not yet |
   | `buildl doctor` | — | print the effective ceiling, grants, workers | not yet |

   `plan` and `build` take any number of targets and `--set name=value`; `build` adds `--keep-going` and `--stream`. A command finds its workspace at the nearest `buildl.toml` at or above the current directory. The exit code says what kind of end it was, so CI need not read text: `0` done, `1` an action failed, `2` a wrong command line, `3` a sick builder, `4` the build wants more than the ceiling, `5` a mistake in a build file or in `buildl.toml`, `6` a state file that is not what buildl writes, `7` a fault in buildl.

   ## Running it

   buildl builds and runs on Linux and macOS. From a checkout:

   ```sh
   cargo make install            # installs the buildl binary from crates/buildl-cli
   ```

   Then, in a directory that holds a `buildl.toml`, or below one:

   ```sh
   buildl check                  # evaluate every build file, run nothing
   buildl graph --dot            # the target graph; canonical JSON without the flag
   buildl plan                   # what would run, and why
   buildl build                  # build every target
   buildl build //lib:text -k    # one target and what it needs, carrying on past a failure
   ```

   Only the first isolation tier is built, and a manifest has to say so:

   ```toml
   [execution]
   strategy = "bare"
   ```

   Without that line `build` refuses to run, rather than run actions under a weaker tier than the default asks for; `check`, `graph` and `plan` work either way.
   ~~~~

4. In `README.md`, in the Roadmap list, add to items 2 to 5 what each now also owns. Replace:

   ~~~~markdown
   2. **`query` / `rdeps`** — small work, transforms CI for large repos
   3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on
   4. **Grant negotiation UX** — wanted set, ceiling intersection, approver
   5. **Plugins** — manifest, attribution, `b.use()`
   ~~~~

   With:

   ~~~~markdown
   2. **`query` / `rdeps`** — small work, transforms CI for large repos; with it the `test`, `log`, `clean` and `gc` commands, and demand-driven and parallel loading of build files
   3. **Local input sandbox (tier 2)** — the correctness discipline everything remote depends on; with it `verify`
   4. **Grant negotiation UX** — wanted set, ceiling intersection, approver, and the `doctor` command; per-target grant narrowing is settled here
   5. **Plugins** — manifest, attribution, `b.use()`; with them discovery targets
   ~~~~

5. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '> **Status: design settled, implementation under way.** `buildl-core` ships the validated domain' README.md
   0
   $ grep -c -F -e '> **Status: the core pipeline is built.** The `buildl` binary runs `check`, `graph`, `plan` and' README.md
   1
   # step 3
   $ grep -c -F -e '| `buildl graph` | Resolve | emit the DAG (`--json`, `--dot`) |' README.md
   0
   $ grep -c -F -e '| Command | Stops after | Purpose | Built |' README.md
   1
   # step 4
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos' README.md
   1
   $ grep -c -F -e '2. **`query` / `rdeps`** — small work, transforms CI for large repos; with it the `test`, `log`,' README.md
   1
   ~~~~

6. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

7. Commit:

   ~~~~text
   $ git add README.md
   $ git commit -m "docs(repo): bring the README up to a buildl that builds"
   ~~~~

### Task 31 — Give the commands and the work without an owner a milestone in the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 2
   $ grep -c -F -e '|2|**`query` / `rdeps`**|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|2|**`query` / `rdeps`**, and the commands `test`, `log`, `clean`, `gc`. Demand-driven and paral' docs/roadmap.md
   0
   # step 3
   $ grep -c -F -e '|3|**Local input sandbox (tier 2)**|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|3|**Local input sandbox (tier 2)**. `verify` (design §12.3) lands here.|not started|' docs/roadmap.md
   0
   # step 4
   $ grep -c -F -e '|4|**Grant negotiation UX**: wanted set, ceiling intersection, approver|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|4|**Grant negotiation UX**: wanted set, ceiling intersection, approver, and the `doctor` comman' docs/roadmap.md
   0
   # step 5
   $ grep -c -F -e '|5|**Plugins**: manifest, attribution, `b.use()`|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|5|**Plugins**: manifest, attribution, `b.use()`. Discovery targets (design §10) land here.|not' docs/roadmap.md
   0
   # step 6
   $ grep -c -F -e '|8|**Remote execution, toolchain containers**|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|—|**First publish to crates.io**: the four crates, in the order of architecture-building-blocks' docs/roadmap.md
   0
   ~~~~

2. In `docs/roadmap.md`, in the table of §2, add to the cell of row 2. Each edit of this task only adds words to a row or adds a row; no status cell changes. Replace:

   ~~~~markdown
   |2|**`query` / `rdeps`**|not started|
   ~~~~

   With:

   ~~~~markdown
   |2|**`query` / `rdeps`**, and the commands `test`, `log`, `clean`, `gc`. Demand-driven and parallel loading of build files (design §11.1) land here.|not started|
   ~~~~

3. In `docs/roadmap.md`, add to the cell of row 3. Replace:

   ~~~~markdown
   |3|**Local input sandbox (tier 2)**|not started|
   ~~~~

   With:

   ~~~~markdown
   |3|**Local input sandbox (tier 2)**. `verify` (design §12.3) lands here.|not started|
   ~~~~

4. In `docs/roadmap.md`, add to the cell of row 4. Replace:

   ~~~~markdown
   |4|**Grant negotiation UX**: wanted set, ceiling intersection, approver|not started|
   ~~~~

   With:

   ~~~~markdown
   |4|**Grant negotiation UX**: wanted set, ceiling intersection, approver, and the `doctor` command. Per-target grant narrowing (design §14) is settled here.|not started|
   ~~~~

5. In `docs/roadmap.md`, add to the cell of row 5. Replace:

   ~~~~markdown
   |5|**Plugins**: manifest, attribution, `b.use()`|not started|
   ~~~~

   With:

   ~~~~markdown
   |5|**Plugins**: manifest, attribution, `b.use()`. Discovery targets (design §10) land here.|not started|
   ~~~~

6. In `docs/roadmap.md`, add one row under row 8, for the first publish. Replace:

   ~~~~markdown
   |8|**Remote execution, toolchain containers**|not started|
   ~~~~

   With:

   ~~~~markdown
   |8|**Remote execution, toolchain containers**|not started|
   |—|**First publish to crates.io**: the four crates, in the order of architecture-building-blocks §9|not started; after milestone 1, with an intent of its own|
   ~~~~

7. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

   ~~~~text
   # step 2
   $ grep -c -F -e '|2|**`query` / `rdeps`**|not started|' docs/roadmap.md
   0
   $ grep -c -F -e '|2|**`query` / `rdeps`**, and the commands `test`, `log`, `clean`, `gc`. Demand-driven and paral' docs/roadmap.md
   1
   # step 3
   $ grep -c -F -e '|3|**Local input sandbox (tier 2)**|not started|' docs/roadmap.md
   0
   $ grep -c -F -e '|3|**Local input sandbox (tier 2)**. `verify` (design §12.3) lands here.|not started|' docs/roadmap.md
   1
   # step 4
   $ grep -c -F -e '|4|**Grant negotiation UX**: wanted set, ceiling intersection, approver|not started|' docs/roadmap.md
   0
   $ grep -c -F -e '|4|**Grant negotiation UX**: wanted set, ceiling intersection, approver, and the `doctor` comman' docs/roadmap.md
   1
   # step 5
   $ grep -c -F -e '|5|**Plugins**: manifest, attribution, `b.use()`|not started|' docs/roadmap.md
   0
   $ grep -c -F -e '|5|**Plugins**: manifest, attribution, `b.use()`. Discovery targets (design §10) land here.|not' docs/roadmap.md
   1
   # step 6
   $ grep -c -F -e '|8|**Remote execution, toolchain containers**|not started|' docs/roadmap.md
   1
   $ grep -c -F -e '|—|**First publish to crates.io**: the four crates, in the order of architecture-building-blocks' docs/roadmap.md
   1
   ~~~~

8. Run the gate; both commands exit `0`:

   ~~~~text
   $ cargo make dod
   $ cargo deny check
   ~~~~

9. Commit:

   ~~~~text
   $ git add docs/roadmap.md
   $ git commit -m "docs(repo): give the commands and the work without an owner a milestone in the roadmap"
   ~~~~

### Task 32 — Mark I9 and milestone 1 done, and sweep every document

**Files:**
- Modify `docs/roadmap.md`
- Modify `CLAUDE.md`

This is the one task that marks anything done. Its new text holds fourteen fill-ins, written `FILL-…`, for values that exist only now; step 2 computes each. Write the computed value in place of the fill-in wherever it appears in the `With` blocks below. The searches of step 1 and of the verify step quote no fill-in.

**Steps:**

1. See the anchors before editing. Under each `# step N` are the searches for the edit of that step: the first is for a piece of the text the step replaces and prints `1`; the second is for a piece of the text it writes and prints `0` (`grep -c` then exits `1`, which is expected). A step that only removes text has the first search alone.

   ~~~~text
   # step 3
   $ grep -c -F -e 'Only milestone 1 is broken into intents so far. Later milestones get intents when milestone 1 is' docs/roadmap.md
   1
   $ grep -c -F -e 'Milestone 1 was cut into nine intents (§3). No later milestone has an intent yet; milestone 2 is' docs/roadmap.md
   0
   # step 4
   $ grep -c -F -e 'local cache and cas|**in progress** (§3)|' docs/roadmap.md
   1
   $ grep -c -F -e 'local cache and cas|**done**,' docs/roadmap.md
   0
   # step 5
   $ grep -c -F -e '    I8 --> I9["I9 adapters + CLI' docs/roadmap.md
   1
   $ grep -c -F -e 'I8 --> I9["I9 adapters + CLI"]' docs/roadmap.md
   0
   # step 6
   $ grep -c -F -e '|**I9 adapters + CLI**|' docs/roadmap.md
   1
   $ grep -c -F -e '|**I9 adapters + CLI**|`buildl`, `buildl-cli`, `buildl-core`|`LocalPorts`, the `Workspace` facad' docs/roadmap.md
   0
   # step 7
   $ grep -c -F -e '- **I9 comes after I8 and I5**' docs/roadmap.md
   1
   $ grep -c -F -e '**I9 came after I8 and I5** and was the last slice of milestone 1: `LocalPorts`, the storage, ex' docs/roadmap.md
   0
   # step 8
   $ grep -c -F -e '|`f88cd77` … `6e39019`|' docs/roadmap.md
   1
   $ grep -c -F -e '|I9 adapters + CLI|`.claudestacks/sdlc/2026-10-13-adapters-cli/`|`01-core-values` (' docs/roadmap.md
   0
   # step 9
   $ grep -c -F -e 'What is implemented, in progress and next is tracked in `docs/roadmap.md` §2–§3; read it there r' CLAUDE.md
   1
   $ grep -c -F -e 'Milestone 1 is done: the `buildl` binary runs `check`, `graph`, `plan` and `build` on a real wor' CLAUDE.md
   0
   ~~~~

2. Compute the fill-ins. Run the four commands and keep what they print:

   ~~~~text
   $ date +%Y-%m-%d
   $ grep -c '^### Task ' .claudestacks/sdlc/2026-10-13-adapters-cli/plans/*.md
   $ git log --reverse --format=%h -- crates/buildl-core/src/types/turn.rs crates/buildl-core/src/types/channel.rs crates/buildl-core/src/types/scope.rs | head -n 1
   $ git log -n 1 --format=%h
   ~~~~

   |Fill-in|Value|
   |---|---|
   |`FILL-DATE`|the one line the first command prints, a date such as `2026-10-20`|
   |`FILL-N01` to `FILL-N11`|the second command prints one `path:count` line per plan file, in order; `FILL-N01` is the count on the line of `01-core-values.md`, `FILL-N02` that of `02-core-ports.md`, and so on to `FILL-N11`, that of `11-cli.md`. The line of `12-documentation.md` reads `32`, which the row below already holds. Where a count is `1`, write `(1 task)`, not `(1 tasks)`|
   |`FILL-FIRST`|the one hash the third command prints: the oldest commit that touches any of three files plan `01-core-values` creates, so the first commit of this chain's code|
   |`FILL-LAST`|the one hash the fourth command prints: the latest commit, which is the commit of this plan's task 31|

3. In `docs/roadmap.md`, in the last paragraph of §1, say how milestone 1 was cut and which milestone is next. Replace:

   ~~~~markdown
   Only milestone 1 is broken into intents so far. Later milestones get intents when milestone 1 is close to done.
   ~~~~

   With:

   ~~~~markdown
   Milestone 1 was cut into nine intents (§3). No later milestone has an intent yet; milestone 2 is next.
   ~~~~

4. In `docs/roadmap.md`, in the table of §2, mark milestone 1 done in the status cell of row 1. Replace:

   ~~~~markdown
   local cache and cas|**in progress** (§3)|
   ~~~~

   With:

   ~~~~markdown
   local cache and cas|**done**, FILL-DATE (§3)|
   ~~~~

5. In `docs/roadmap.md`, in the diagram of §3, which stays Mermaid, drop the state from the I9 node's label. Replace the whole line that starts with the text below — step 1 showed there is exactly one — whatever the rest of it reads:

   ~~~~text
       I8 --> I9["I9 adapters + CLI
   ~~~~

   When this plan was written the line read:

   ~~~~markdown
       I8 --> I9["I9 adapters + CLI<br/>spec approved"]
   ~~~~

   With:

   ~~~~markdown
       I8 --> I9["I9 adapters + CLI"]
   ~~~~

6. In `docs/roadmap.md`, in the table of §3, replace the I9 row: the crates it touched, what it built, and its status. Replace the whole line that starts with the text below — step 1 showed there is exactly one — whatever the rest of it reads:

   ~~~~text
   |**I9 adapters + CLI**|
   ~~~~

   When this plan was written the line read:

   ~~~~markdown
   |**I9 adapters + CLI**|`buildl`, `buildl-cli`|`LocalPorts`, the `Workspace` facade and the adapters for every port except `DeclarationSource` (architecture-building-blocks §6); the `Reporter` port; the `check`/`graph`/`plan`/`build` commands and exit codes in `buildl-cli`|the `buildl` binary builds a fixture workspace twice, byte-identical|**spec approved**, plans in progress: `.claudestacks/sdlc/2026-10-13-adapters-cli/`|
   ~~~~

   With:

   ~~~~markdown
   |**I9 adapters + CLI**|`buildl`, `buildl-cli`, `buildl-core`|`LocalPorts`, the `Workspace` facade and an adapter for every port milestone 1 uses: all of architecture-building-blocks §6 but `DeclarationSource` (I5) and `Approver` (milestone 4), with bare execution only; the `Reporter` and `GraphRecord` ports; building one named target; the `check`/`graph`/`plan`/`build` commands and exit codes in `buildl-cli`; the adapter-isolation guard; a milestone for every command of design §7|the `buildl` binary builds a fixture workspace twice, byte-identical|**done**, FILL-DATE (§4)|
   ~~~~

7. In `docs/roadmap.md`, in the ground rules below that table, put the rule on I9 in the past: nothing is owed any more. Replace the whole line that starts with the text below — step 1 showed there is exactly one — whatever the rest of it reads:

   ~~~~text
   - **I9 comes after I8 and I5**
   ~~~~

   When this plan was written the line read:

   ~~~~markdown
   - **I9 comes after I8 and I5** and is the last slice of milestone 1: `LocalPorts`, the storage, exec, digest and manifest adapters (architecture-building-blocks §6), the `Reporter` port, and the commands in `buildl-cli`. It takes everything milestone 1 still owes; its intent lists it.
   ~~~~

   With:

   ~~~~markdown
   - **I9 came after I8 and I5** and was the last slice of milestone 1: `LocalPorts`, the storage, exec, digest and manifest adapters (architecture-building-blocks §6), the `Reporter` and `GraphRecord` ports, and the commands in `buildl-cli`. It took everything milestone 1 still owed; its intent lists it.
   ~~~~

8. In `docs/roadmap.md`, in the table of §4, add the chain's row below the I8 row. Replace:

   ~~~~markdown
   |`f88cd77` … `6e39019`|
   ~~~~

   With:

   ~~~~markdown
   |`f88cd77` … `6e39019`|
   |I9 adapters + CLI|`.claudestacks/sdlc/2026-10-13-adapters-cli/`|`01-core-values` (FILL-N01 tasks), `02-core-ports` (FILL-N02 tasks), `03-core-pipeline` (FILL-N03 tasks), `04-buildl-foundation` (FILL-N04 tasks), `05-input-adapters` (FILL-N05 tasks), `06-state-adapters` (FILL-N06 tasks), `07-run-adapters` (FILL-N07 tasks), `08-reporter` (FILL-N08 tasks), `09-composition` (FILL-N09 tasks), `10-end-to-end` (FILL-N10 tasks), `11-cli` (FILL-N11 tasks), `12-documentation` (32 tasks)|`FILL-FIRST` … `FILL-LAST`|
   ~~~~

9. In `CLAUDE.md`, in the Status section, replace the last sentence of the first paragraph. Replace:

   ~~~~markdown
   What is implemented, in progress and next is tracked in `docs/roadmap.md` §2–§3; read it there rather than inferring a state from this file.
   ~~~~

   With:

   ~~~~markdown
   Milestone 1 is done: the `buildl` binary runs `check`, `graph`, `plan` and `build` on a real workspace, at isolation tier 1. What comes next is tracked in `docs/roadmap.md` §2; read it there rather than inferring a state from this file.
   ~~~~

10. Confirm every fill-in was written out. Both commands print `0`:

    ~~~~text
    $ grep -c -e 'FILL-' docs/roadmap.md
    0
    $ grep -c -e '(1 tasks)' docs/roadmap.md
    0
    ~~~~

11. Verify with the same searches: the piece of the old text is gone (`0`) and the piece of the new text is there once (`1`). Where the first search of a step still prints `1`, the step keeps that piece of text and writes around it.

    ~~~~text
    # step 3
    $ grep -c -F -e 'Only milestone 1 is broken into intents so far. Later milestones get intents when milestone 1 is' docs/roadmap.md
    0
    $ grep -c -F -e 'Milestone 1 was cut into nine intents (§3). No later milestone has an intent yet; milestone 2 is' docs/roadmap.md
    1
    # step 4
    $ grep -c -F -e 'local cache and cas|**in progress** (§3)|' docs/roadmap.md
    0
    $ grep -c -F -e 'local cache and cas|**done**,' docs/roadmap.md
    1
    # step 5
    $ grep -c -F -e '    I8 --> I9["I9 adapters + CLI' docs/roadmap.md
    1
    $ grep -c -F -e 'I8 --> I9["I9 adapters + CLI"]' docs/roadmap.md
    1
    # step 6
    $ grep -c -F -e '|**I9 adapters + CLI**|' docs/roadmap.md
    1
    $ grep -c -F -e '|**I9 adapters + CLI**|`buildl`, `buildl-cli`, `buildl-core`|`LocalPorts`, the `Workspace` facad' docs/roadmap.md
    1
    # step 7
    $ grep -c -F -e '- **I9 comes after I8 and I5**' docs/roadmap.md
    0
    $ grep -c -F -e '**I9 came after I8 and I5** and was the last slice of milestone 1: `LocalPorts`, the storage, ex' docs/roadmap.md
    1
    # step 8
    $ grep -c -F -e '|`f88cd77` … `6e39019`|' docs/roadmap.md
    1
    $ grep -c -F -e '|I9 adapters + CLI|`.claudestacks/sdlc/2026-10-13-adapters-cli/`|`01-core-values` (' docs/roadmap.md
    1
    # step 9
    $ grep -c -F -e 'What is implemented, in progress and next is tracked in `docs/roadmap.md` §2–§3; read it there r' CLAUDE.md
    0
    $ grep -c -F -e 'Milestone 1 is done: the `buildl` binary runs `check`, `graph`, `plan` and `build` on a real wor' CLAUDE.md
    1
    ~~~~

12. Sweep for a document that still calls a crate empty, a command missing or the design unbuilt. The command prints nothing and exits `1`. On the tree this plan was written against it printed eighteen lines, among them `docs/design.md:3:draft for review`, `README.md:7:not implemented yet` and `crates/buildl-cli/README.md:7:provides no commands`, so it does find these phrases where they stand.

    ~~~~text
    $ grep -n -i -o -e 'not implemented yet' -e 'draft for review' -e 'Nothing described here is implemented' -e 'provides no commands' -e 'has no public API' -e 'none of the adapters' -e 'It will assemble' -e 'it will parse' -e 'are not written yet' -e 'composes it into a command yet' -e 'no command composes' -e 'plans in progress' -e 'in progress and next' -e 'in progress\*\*' -e 'twelve ports' docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md docs/fundamental-walkthrough.md README.md CLAUDE.md crates/buildl/README.md crates/buildl-cli/README.md crates/buildl-core/README.md crates/buildl-lua/README.md
    ~~~~

13. Sweep for `Approver` and `InputSandbox`. The command prints every mention with the text around it, on fourteen lines, and none lists either as built: each says the part is later or not there (`later`, `arrives with`, `joins with`, `once it exists`, `not in this crate`, item `3` or `4` in the "Since" cell, `milestone 4`), or is the design of grant negotiation itself — the diagram of design §6, item 4 of design §13, and the walkthrough's sequence diagram.

    ~~~~text
    $ perl -CSD -ne 'while (/(.{0,50}(?:InputSandbox|Approver).{0,70})/g) { print "$ARGV:$.: $1\n" } close ARGV if eof' docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md docs/fundamental-walkthrough.md README.md CLAUDE.md crates/buildl/README.md crates/buildl-cli/README.md crates/buildl-core/README.md crates/buildl-lua/README.md | grep -v -e 'docs/roadmap.md'
    docs/design.md:184:     C --> A["Approver<br/>manifest / interactive / deny"]
    docs/design.md:717: ). Consumes the airsl extension host's ceiling + `Approver` (buildl is their first consumer; the manifest parser and `ext.on` di
    docs/architecture.md:261: Bare` (tier 1: workspace cwd, granted env only). `InputSandbox` (tier 2: temp dir, symlinked declared inputs) arrives with item 3 of
    docs/architecture.md:400: oc's §13): whether `ExecDir` materialisation for `InputSandbox` uses symlinks (fast, but visible to tools that `readlink`) or hard l
    docs/architecture-building-blocks.md:3: er item of design §13: for grant negotiation the `Approver` port, the `Grants` value and the `grants` logic, and for the input s
    docs/architecture-building-blocks.md:3: andbox the `InputSandbox` strategy. Companion to [`design.md`](./design.md) and [`architecture
    docs/architecture-building-blocks.md:110: tcher<br/>Clock, GraphRecord, Reporter<br/>later: Approver"]
    docs/architecture-building-blocks.md:156: uaSource`. An `approver` module arrives with the `Approver` port (design §13 item 4); neither exists yet.
    docs/architecture-building-blocks.md:349:     // `Approver` joins with grant negotiation (design §13 item 4)
    docs/architecture-building-blocks.md:412: ide the ceiling refuses the plan|`Manifest`, and `Approver` once it exists|
    docs/architecture-building-blocks.md:434: trategy`|the same, with undeclared inputs absent|`InputSandbox` (tier 2)|`buildl`|3|
    docs/architecture-building-blocks.md:436: |`Approver`|turning the wanted set into grants|interactive, manifest|`buildl`|4|
    docs/fundamental-walkthrough.md:115:     participant User as Approver prompt
    crates/buildl/README.md:26:  the default included. The input sandbox and the `Approver` port are designed and are not in this crate.
    $ grep -c -e 'all of architecture-building-blocks §6 but `DeclarationSource` (I5) and `Approver` (milestone 4)' docs/roadmap.md
    1
    ~~~~

14. Sweep for a command of design §7 without a place in the sequence. The first command prints the first cell of every row of the command table: eleven commands in nine rows. The next four show where §13 holds them: `check`, `graph`, `plan` and `build` in item 1; `query`, `test`, `log`, `clean` and `gc` in item 2; `doctor` in item 4; `watch` in item 7. The last four show the same in the roadmap's milestones, with milestone 1 done. Each of the eight prints `1`.

    ~~~~text
    $ grep -o -e '^|`buildl [^|]*|' docs/design.md
    |`buildl check`|
    |`buildl graph`|
    |`buildl plan [target]...`|
    |`buildl build [target]...`|
    |`buildl watch [target]`|
    |`buildl test` / `clean` / `gc`|
    |`buildl log <target>`|
    |`buildl query <expr>`|
    |`buildl doctor`|
    $ grep -c -e '^1\. \*\*Core pipeline\*\* .*`check`/`graph`/`plan`/`build`' docs/design.md
    1
    $ grep -c -e '^2\. \*\*`query` / `rdeps`\*\* .*`test`, `log`, `clean` and `gc` (§7)' docs/design.md
    1
    $ grep -c -e '^4\. \*\*Grant negotiation UX\*\* .*`doctor`, which prints the result (§7)' docs/design.md
    1
    $ grep -c -e '^7\. \*\*Watch mode\*\*' docs/design.md
    1
    $ grep -c -e '^|1|.*`check`/`graph`/`plan`/`build` commands.*|\*\*done\*\*, ' docs/roadmap.md
    1
    $ grep -c -e '^|2|.*`query` / `rdeps`.*`test`, `log`, `clean`, `gc`' docs/roadmap.md
    1
    $ grep -c -e '^|4|.*`doctor` command' docs/roadmap.md
    1
    $ grep -c -e '^|7|\*\*Watch mode\*\*' docs/roadmap.md
    1
    ~~~~

15. Sweep the roadmap's follow-ups: §5 still holds its one paragraph and no table row. The first command prints the section, the second prints `0`.

    ~~~~text
    $ sed -n '/^## 5\. Carried follow-ups/,/^## 6\. /p' docs/roadmap.md
    ## 5. Carried follow-ups

    No follow-up is open. A slice closes everything it finds before it is marked done, and nothing is carried to a later slice, plan or milestone (§6).

    ## 6. Process notes
    $ sed -n '/^## 5\. Carried follow-ups/,/^## 6\. /p' docs/roadmap.md | grep -c -e '^|'
    0
    ~~~~

16. Sweep the house rules. Every diagram is still Mermaid: the first command prints the counts the documents had before this plan. No design document or README names an intent or a milestone: the second command prints one line, the README's comment on what `roadmap.md` is. The header line of each spec document stands: the third command prints `1` for each. No section was added, removed or renumbered: the fourth prints the heading counts the documents had before this plan.

    ~~~~text
    $ grep -c -e '^```mermaid' docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md README.md
    docs/design.md:13
    docs/architecture.md:3
    docs/architecture-building-blocks.md:7
    docs/roadmap.md:2
    README.md:3
    $ grep -n -o -w -e 'I[1-9]' -e 'slice' -e 'slices' -e 'spec' -e 'milestone' -e 'milestones' -e 'Milestone' docs/design.md docs/architecture.md docs/architecture-building-blocks.md README.md crates/buildl/README.md crates/buildl-cli/README.md crates/buildl-core/README.md crates/buildl-lua/README.md
    README.md:173:milestone
    $ grep -c -e '^\*\*Author:\*\* rstlix0x0' docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md
    docs/design.md:1
    docs/architecture.md:1
    docs/architecture-building-blocks.md:1
    docs/roadmap.md:1
    $ grep -c -e '^##' docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md
    docs/design.md:38
    docs/architecture.md:16
    docs/architecture-building-blocks.md:19
    docs/roadmap.md:7
    ~~~~

17. Run the gate; both commands exit `0`:

    ~~~~text
    $ cargo make dod
    $ cargo deny check
    ~~~~

18. Commit:

    ~~~~text
    $ git add docs/roadmap.md CLAUDE.md
    $ git commit -m "docs(repo): mark I9 adapters + CLI and milestone 1 done"
    ~~~~

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0` after every task. No task changes code, a manifest or rustdoc.
- Every task's last search step prints what the plan shows: the piece of each replaced text is gone and the piece of each new text is in its file once.
- No document calls a crate empty, a command missing or the design unbuilt: the first sweep of task 32 prints nothing.
- `docs/design.md` says item 1 of §13 is built; lists every file of `.buildl/`; says how `buildl.toml` is read and that `bare` is the one strategy that runs; says a declared input is a regular file; has `log` in its command table and a place in §13 for every command of that table; gives the eight exit codes; says what a tool's fingerprint is; and states what tier 1 guarantees and what it does not.
- `docs/architecture.md` says Load is sequential; describes the plan-side adapters, the reporter, live output, the `bare` strategy and the store on disk as built; has `Killed` and the three new errors; names the double-build test; records fifteen decisions; and has no open question the adapters settled.
- `docs/architecture-building-blocks.md` lists the seventeen modules of `buildl` and the supported platforms; shows `Reporter`, `GraphRecord` and the changed signatures; gives every adapter the item of design §13 it arrives with; names `cargo make guard-adapter-isolation`; describes the four test layers that touch real I/O; and holds the four publishing rules.
- `Approver` and `InputSandbox` are named only as later work: the second sweep of task 32 prints every mention.
- The four crate READMEs and the root README say what each crate and the binary do today, name Linux and macOS as the supported platforms where the crate is Unix-only, and hold no intent or milestone name.
- `docs/roadmap.md` shows I9 done with its date, milestone 1 done, a milestone for `test`, `log`, `clean`, `gc`, `doctor`, `verify`, demand-driven and parallel loading, per-target grant narrowing and discovery targets, a row for the first publish, the chain's row in §4 with every fill-in written out, and §5 unchanged: no follow-up is open.
- `CLAUDE.md` says milestone 1 is done and names the adapter-isolation guard.
- Every `mermaid` block is still a `mermaid` block, each spec document keeps its header line, and no section was added, removed or renumbered: the last sweep of task 32 prints the counts.
