---
status: approved
created: 2026-10-14
depends-on: [03]
built-on: 050ed7322c38
---

# Query documentation Implementation Plan

**Goal:** the repository's documents describe `query` as built and show milestone 2's intent ladder.

**Architecture:** Documentation only, no crate code changes. Each task updates one group of
documents:
- `design.md` and `README.md` carry the command as a user sees it;
- `architecture.md` and `architecture-building-blocks.md` carry how it is built;
- `CLAUDE.md` carries the status;
- `docs/roadmap.md` gets the milestone 2 ladder, and the last task closes I10 in it.

Mermaid stays Mermaid in `docs/`. No section is renumbered outside roadmap §3, and §3 keeps its
number, so every existing cross-reference still resolves.

**Tech Stack:** Markdown, Mermaid. Definition of Done: `cargo make dod` (the rustdoc build is
part of it) and `cargo deny check`.

**Content authority:** spec §4.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-query` (branch
`worktree-m2-query`). Plan `03` must be `done` first, so the documents describe what exists. One
commit per task, scope `repo`.

Every location below was read at commit `ce57760`. Where a task gives wording in quotes, that
wording is the deliverable: it is what readers will see. If a location no longer reads as
described, re-read the file instead of editing nearby text.

## Files that change

```
docs/design.md                        — [modify] §7 table row and built-commands paragraph; §13 item 2 heading
README.md                             — [modify] status line, tier paragraph, command table row, roadmap item 2
docs/architecture.md                  — [modify] status line; §3.2 TargetGraph paragraph; §6 graph.json and lock rows
docs/architecture-building-blocks.md  — [modify] §5.4 reach row and pipeline paragraph; failure-mode and wiring rows
CLAUDE.md                             — [modify] status paragraph, roadmap role, pipeline line
docs/roadmap.md                       — [modify] §1, §2 row 2, §3 split into 3.1 and 3.2; I10 closed in §3.2 and §4
```

## Order of work

### Task 1 — The command as a user sees it: `design.md` and `README.md`

**Files:**
- Modify `docs/design.md` — `:213`, `:216`, `:715`.
- Modify `README.md` — `:7`, `:123`, `:155`, `:183`.

**Change:**
- `design.md:213`: the §7 table row for `buildl query <expr>` cites `(§10.2, §10.3, §11.1)`
  instead of `(§10.1)`. §10.1 is about customization, and `rdeps` is used at `:570` (§10.2), in
  §10.3, and at `:606` (§11.1).
- `design.md:216`:
  - the opening list of built commands becomes "`check`, `graph`, `query`, `plan` and `build`
    are built";
  - the paragraph ends with a new sentence: "`query` takes one expression, `deps(<target>)` or
    `rdeps(<target>)`, and prints the targets the named one depends on, or that depend on it,
    any number of steps away and without the target itself: one label a line in label order, or
    with `--json` as canonical JSON. Like `check` and `graph` it takes no lock and writes
    nothing."
- `design.md:715`: §13 item 2's bold heading becomes "**`query` (`deps()`, `rdeps()`)**". The
  rest of the line is unchanged.
- `README.md:7`: the status line's list of commands the binary runs becomes "`check`, `graph`,
  `query`, `plan` and `build`".
- `README.md:123`: the `buildl query <expr>` row's last cell changes from `not yet` to `yes`.
- `README.md:155`: the list of commands that work without the isolation line becomes "`check`,
  `graph`, `query` and `plan` work either way".
- `README.md:183`: roadmap item 2's bold heading becomes "**`query` (`deps()`, `rdeps()`)**".

**Proof:**
- `grep -n '`query` / `rdeps`' docs/design.md README.md` prints nothing.
- `grep -n '(§10.1)' docs/design.md` prints nothing.
- `grep -n 'query' README.md` prints lines 7, 123, 155 and 183. At `ce57760` it printed only
  123 and 183.

**Done when:** the checks above hold and the Definition of Done is green. Commit
`docs(repo): query is a built command in the design and README`.

### Task 2 — How it is built: `architecture.md` and `architecture-building-blocks.md`

**Files:**
- Modify `docs/architecture.md` — `:3`, `:209`, `:374`, `:379`.
- Modify `docs/architecture-building-blocks.md` — after `:410`, and `:416`, `:532`, `:533`, `:585`.

**Change:**
- `architecture.md:3`: after "the `check` / `graph` / `plan` / `build` commands", add "; `query`,
  from the design doc's §13 item 2, is built over the same core".
- `architecture.md:209` (§3.2): before the closing parenthetical about petgraph, add
  "`query`'s `deps()` and `rdeps()` are reachability over `edges` and `reverse`:
  `resolve::reach` walks either from one target, with the walk `narrow` uses for a run's targets,
  and leaves the target itself and every alias out of the answer."
- `architecture.md:374`: "`plan` and `graph` record nothing" becomes "`plan`, `graph` and
  `query` record nothing".
- `architecture.md:379`: "`check` and `graph` take none" becomes "`check`, `graph` and `query`
  take none".
- `architecture-building-blocks.md` §5.4: a new row right after the `narrow` row (`:410`), in
  the table's three-column form. Name `reach`; describe it as "`TargetGraph` and a `Query` → the
  targets the query reaches from its target, following dependencies or the reverse edges any
  number of steps, without that target and without aliases, as a `BTreeSet<Label>`; a label
  nothing declares is `Error::UnknownTarget`"; ports `—`.
- `architecture-building-blocks.md:416`: the free-function sentence names three commands,
  "`check`, `graph` and `query` … `pipeline::check`, `pipeline::graph` and `pipeline::query`".
- `architecture-building-blocks.md:532` and `:533`: "`check` and `graph`" becomes "`check`,
  `graph` and `query`" in both cells.
- `architecture-building-blocks.md:585`: the bold decision ends "a source of build files alone
  for `check`, `graph` and `query`".

**Proof:**
- `grep -c '|reach|' docs/architecture-building-blocks.md` is `1`.
- `grep -n '`check` and `graph`' docs/architecture.md docs/architecture-building-blocks.md`
  prints nothing. At `ce57760` its hits were exactly the five lines this task edits.

**Done when:** the checks above hold and the Definition of Done is green. Commit
`docs(repo): the architecture documents describe query`.

### Task 3 — `CLAUDE.md`

**Files:**
- Modify `CLAUDE.md` — `:7`, `:17`, `:26`.

**Change:**
- `:7`: after "Milestone 1 is done: … at isolation tier 1.", add "Milestone 2 has begun: `query`
  answers `deps()` and `rdeps()` over the target graph." The pointer at the end of the paragraph
  becomes `docs/roadmap.md` §2–§3.
- `:17`: the roadmap's role names "the intent ladders (milestone 1: I1–I9; milestone 2:
  I10–I15)" instead of "the milestone 1 intent ladder (I1–I9)".
- `:26`: the pipeline line's command list becomes `check`/`graph`/`query`/`plan`/`build`.

**Proof:**
- `grep -n 'query' CLAUDE.md` prints exactly lines 7 and 26. Line 17's new wording does not hold
  the word.
- `grep -n 'I10–I15' CLAUDE.md` prints line 17.

**Done when:** the checks above hold. Commit `docs(repo): CLAUDE.md names query and the milestone 2 ladder`.

### Task 4 — The milestone 2 ladder in `docs/roadmap.md`

**Files:**
- Modify `docs/roadmap.md` — §1, §2 row 2, §3.

**Change:**
- §1: the sentence "Milestone 1 was cut into nine intents (§3). No later milestone has an intent
  yet; milestone 2 is next." becomes "Milestone 1 was cut into nine intents (§3.1). Milestone 2
  is cut into six intents (§3.2); no later milestone has an intent yet."
- §2 row 2: the bold heading becomes "**`query` (`deps()`, `rdeps()`)**", and the status changes
  from `not started` to "**in progress** (§3.2)".
- §3: the heading "Milestone 1 — the intent ladder" becomes "## 3. Intent ladders". The existing
  content, from its first paragraph to the last "Ground rules" bullet (airsl on `0.1`), sits under
  a new "### 3.1 Milestone 1".
- A new "### 3.2 Milestone 2" goes after that, before "## 4. Completed intents". It holds:
  - **One opening sentence:** milestone 2 is design §13 item 2 — `query`, the commands around a
    build, and the two changes to how build files are loaded — and its intents continue §3.1's
    numbering.
  - **A Mermaid `graph LR`**, with the comment line
    `%% Milestone 2 — intent ladder; arrows point from a slice to the slice that follows it`, and
    the chain I10 query (deps / rdeps) → I11 test → I12 log → I13 clean + gc → I14 parallel load
    → I15 demand-driven load.
  - **A table** with §3.1's columns (Intent, Crate, Scope, Done when, Status), one row per intent:

    | Intent | Crate | Scope | Done when | Status |
    |---|---|---|---|---|
    | I10 | `buildl-core`, `buildl`, `buildl-cli` | `buildl query` with `deps()` and `rdeps()` over the whole graph, the start target and aliases left out, no lock and no state | the binary answers both functions on a fixture workspace | **in progress** |
    | I11 | `—` | `buildl test`, building and running the test targets (design §7, §9.1) | `—` | not started |
    | I12 | `—` | `buildl log <target>`, what a target printed when it last ran (design §7, §8.7), which needs the build log read back | `—` | not started |
    | I13 | `—` | `clean` drops outputs; `gc` prunes the cas by mark and sweep (architecture §3.5) | `—` | not started |
    | I14 | `—` | build files evaluated across workers and merged to the order serial load gives (architecture §3.1) | `—` | not started |
    | I15 | `—` | a command that names targets loads only the directories those targets and their dependencies need (design §11.1) | `—` | not started |

  - **"Ground rules for the ladder:"** with four bullets:
    - the crate and done-when cells of I11–I15 are left to each slice's intent and spec, as
      §3.1's port assignment was;
    - I13 follows I12, so `gc` keeps the blobs `log` reads;
    - I15 covers only commands that name targets: `query`, `graph`, `check` and a command run
      without targets load the whole workspace, because `rdeps` needs every reverse edge,
      `graph.json` is the whole graph, and a build file is still evaluated only because an
      evaluated one named its directory (design §14);
    - the first publish to crates.io (§2) is not part of this ladder, and gets an intent of its
      own.

**Proof:**
- `grep -n '^## \|^### 3' docs/roadmap.md` lists, in order: `## 1.`, `## 2.`,
  `## 3. Intent ladders`, `### 3.1 Milestone 1`, `### 3.2 Milestone 2`, `## 4.`, `## 5.`,
  `## 6.`, `## References`.
- `grep -rn '`query` / `rdeps`' docs README.md CLAUDE.md` prints nothing.

**Done when:** the checks above hold. Commit `docs(repo): the milestone 2 intent ladder`.

### Task 5 — Close I10 in the roadmap

**Files:**
- Modify `docs/roadmap.md` — §3.2 I10 row, §4 table.

**Change:**
- Read the chain's range with `git log --reverse --format=%h main..HEAD`: the first and last
  hashes it prints. Read the last commit's date with `git log -1 --format=%cs <last>`.
- In §3.2, the I10 row's status becomes "**done**, <that date> (§4)".
- In §4, a row goes after I9's: I10 query; chain
  `.claudestacks/sdlc/2026-10-14-query/`; plans `01-core-query` (5 tasks), `02-facade-query`
  (2 tasks), `03-cli-query` (5 tasks), `04-documentation` (5 tasks); commits `<first>` …
  `<last>`.
- §2 row 2 stays **in progress**, because I11–I15 are not done.

**Proof:**
- `grep -n 'I10 query' docs/roadmap.md` shows the §3.2 row marked done and the §4 row.
- Both hashes in the §4 row resolve with `git cat-file -t <hash>` as `commit`.

**Done when:** the checks above hold. Commit `docs(repo): close the query slice in the roadmap`.
As with earlier slices, the range in §4 does not include the commit that writes it.

---

## Risks

- **Line numbers drift** as earlier tasks in this plan edit the same files. Each location is also
  named by its section and its text, so find it by text when a number is off.
- **Roadmap §3 restructuring** is the step most likely to break something. A subsection
  heading must not renumber §4–§6, and references elsewhere to "roadmap §3" must still land on
  the ladders. Task 4's heading check is the guard.
- **Whether the quoted wording counts as "a diff".** The reviewer raised this. The plan gives
  the words because, in documentation, they are the deliverable. It does not give old/new blocks.

## Proof

- `cargo make dod` and `cargo deny check` green.
- `grep -rn '`query` / `rdeps`' docs README.md CLAUDE.md` and `grep -n '(§10.1)' docs/design.md`
  print nothing.
- `grep -n '^### 3' docs/roadmap.md` shows `3.1` and `3.2`, and §3.2 shows I10 done.
