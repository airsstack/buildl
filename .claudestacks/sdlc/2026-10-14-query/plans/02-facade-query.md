---
status: approved
created: 2026-10-14
depends-on: [01]
built-on: 050ed7322c38
---

# Facade query Implementation Plan

**Goal:** a caller holding a `buildl::Workspace` gets the answer to a query typed as text, with a text form of it for a person.

**Architecture:** `Workspace` gains two methods. `query_of` reads text into a `Query`, reading
its target the way `Workspace::label` does. `query` answers through
`buildl_core::pipeline::query`, the source-only entry point, so like `check` and `graph` it takes
no lock and opens no state file. The text form is a new rendering, `report::query_text`, beside
`plan_text` and `graph_dot`, because `report` holds every rendering of a value into text.

**Tech Stack:** Rust 2024, the `buildl` crate's public API, the checked-in `chain` fixture.
Definition of Done: `cargo make dod` and `cargo deny check`.

**Content authority:** spec §3.4, the rendering part of §3.5, §5, §6.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-query` (branch
`worktree-m2-query`). Plan `01` must be `done` first. It adds `buildl_core::{Query, Direction}`,
`Query::parse` and `pipeline::query`. One commit per task, scope `buildl`.

Facts the tasks rest on, read at commit `ce57760`:

| Fact | Where |
|---|---|
| `report` holds "every rendering of a value into text … so that a caller formats nothing itself"; its Responsibilities list holds `plan_text`, `graph_dot`, `check_line`; its Non-responsibilities sentence ends "the canonical JSON of a graph or a plan, which is the value's own serialized form", wrapped across two lines | `crates/buildl/src/report/mod.rs:1-6`, `:9-22`, `:24-26` |
| `Workspace::label(raw)` reads `//dir:name` as written and `:name` / `name` against the directory the workspace was opened from | `crates/buildl/src/workspace.rs:62-70` |
| `Workspace::graph` is `pipeline::graph(&self.wiring.source(), self.wiring.entry())`, with no lock | `workspace.rs:91-93` |
| `Workspace`'s module doc Responsibilities: "opening one, reading a target as a person names it, and the commands" | `workspace.rs:7-8` |
| `facade.rs`'s module doc: "What the facade does before a build starts: opening a workspace, reading a target, and refusing a build it must not begin." | `crates/buildl/tests/e2e/facade.rs:1-2` |
| The `chain` fixture: `//:greeting` → `//:greet`, `//lib:words`; `//:report` → `//:greeting`; `//:site` → `//lib:assets`; alias `//:text` → `//lib:words`; six targets | `buildl graph` on a copy of `crates/buildl/tests/fixtures/chain` (spec P12); `facade.rs:20` asserts `graph.len() == 6` |
| `Fixture::copy_of("chain", &["greet.sh"])`, `root()`, `state()`; `StateLock::acquire(chain.state())` holds the workspace | `crates/buildl/tests/e2e/common.rs:56`, `:75`, `:80`; `facade.rs:36-38` |
| `Label` orders by directory, then name: `//:z` < `//a:x` < `//a-b:x` < `//lib:a` | spec P9 (probe) |
| `guard-adapter-isolation`: an adapter module of `buildl` may name only `state` and `search_path`; `report` here names only `buildl_core` | `Makefile.toml` task `guard-adapter-isolation` |

## Files that change

```
crates/buildl/src/report/query_text.rs  — [create] query_text, with unit tests
crates/buildl/src/report/mod.rs         — [modify] declare, re-export and list query_text
crates/buildl/src/workspace.rs          — [modify] Workspace::query_of and Workspace::query; module doc
crates/buildl/tests/e2e/facade.rs       — [modify] the query scenarios on the chain fixture; module doc
```

## Order of work

### Task 1 — `report::query_text`

**Files:**
- Create `crates/buildl/src/report/query_text.rs` — the rendering and its tests.
- Modify `crates/buildl/src/report/mod.rs`:
  - add `mod query_text;` after `mod plan_text;`;
  - add `pub use query_text::query_text;` after `pub use plan_text::plan_text;`;
  - add a Responsibilities bullet after `plan_text`'s: "`query_text` — the answer to a query as
    lines for a person";
  - in the Non-responsibilities sentence, make the JSON clause read "the canonical JSON of a
    graph, a plan or a query's answer". That clause wraps across `:25-26`, so edit both lines.

**Change:** `#[must_use] pub fn query_text(labels: &BTreeSet<Label>) -> String` writes each label
on its own line, in the set's order, each line ending in `\n`. An empty set gives the empty
string. The module doc says why this is its own file: it renders a set of labels that no other
rendering shows, one per line, for a person or a shell loop. It also says that answering the
query, and the answer's JSON (the set's own serialized form), are not its job.

**Proof** (colocated tests):
- `an_empty_answer_is_no_text` — an empty set renders as `""`.
- `each_label_is_one_line_in_label_order` — the set `//lib:a`, `//a-b:x`, `//:z`, `//a:x`
  renders as `//:z\n//a:x\n//a-b:x\n//lib:a\n`, in label order rather than byte order.

**Done when:** `cargo test -p buildl --lib report::query_text` passes and the Definition of Done
is green, `guard-adapter-isolation` included. Commit `feat(buildl): the answer to a query as lines`.

### Task 2 — `Workspace::query_of` and `Workspace::query`

**Files:**
- Modify `crates/buildl/src/workspace.rs` — two methods after `graph`, imports, module doc.
- Modify `crates/buildl/tests/e2e/facade.rs` — five scenarios, a helper, the module doc.

**Change:**
- `pub fn query_of(&self, raw: &str) -> Result<Query>` is `Query::parse(raw, |target| self.label(target))`.
  Its rustdoc says it reads `deps(<target>)` or `rdeps(<target>)` with the target read as
  `Workspace::label` reads one. Its `# Errors` names `Error::InvalidQuery` and whatever `label`
  returns.
- `pub fn query(&self, query: &Query) -> Result<BTreeSet<Label>>` is
  `pipeline::query(&self.wiring.source(), self.wiring.entry(), query)`. Its rustdoc says it
  answers in label order without the named target, evaluates every build file once, and takes no
  lock and opens no state file, like `Workspace::check`. Its `# Errors` returns what
  `pipeline::query` returns.
- The imports gain `BTreeSet` and `Query`. The module doc's Responsibilities becomes "opening one,
  reading a target or a query as a person writes it, and the commands", wrapped at 100 columns by
  hand.
- `facade.rs`'s module doc becomes "What the facade does before a build starts: opening a
  workspace, reading a target, answering a query, and refusing a build it must not begin."

**Proof** (`tests/e2e/facade.rs`, each on a fresh `Fixture::copy_of("chain", &["greet.sh"])`; a
helper `answer(workspace, raw)` runs `query_of` then `query` and renders the labels):
- `a_query_answers_from_the_whole_graph_and_writes_nothing`:
  - `deps(//:report)` answers `//:greet`, `//:greeting`, `//lib:words`;
  - `rdeps(//:text)` answers `//:greeting`, `//:report`, reaching through the alias;
  - `rdeps(//:report)` answers nothing;
  - afterwards neither `.buildl/` nor `out/` exists.
- `a_query_reads_its_target_from_where_it_was_opened` — opened at `lib/`: `rdeps(words)` answers
  `//:greeting`, `//:report`, and `deps(:assets)` answers nothing.
- `a_query_in_neither_form_is_refused_before_anything_is_loaded` — `query_of("dep(//:report)")`
  is `Error::InvalidQuery` holding `dep(//:report)`.
- `a_query_for_a_target_nothing_declares_suggests_the_nearest` — `deps(//:reprt)` fails with the
  message `target //:reprt was asked for but is not declared (did you mean //:report?)`.
  `//:reprt` is one edit from `//:report` and at least three from every other label.
- `a_query_is_answered_while_another_buildl_holds_the_workspace` — with `StateLock` held on
  `chain.state()`, `deps(//:site)` still answers `//lib:assets`.

**Done when:** `cargo test -p buildl --test e2e facade` passes, the Definition of Done is green,
and `cargo deny check` is green. Commit `feat(buildl): the workspace answers a query without a lock`.

---

## Risks

- **Line wrapping in docs.** `rustfmt` does not wrap comments, so the two edited module-doc lines
  and the `report/mod.rs` clause must be kept under 100 columns by hand.
- **The lock scenario relies on `query` never calling `Wiring::lock`.** It holds as long as
  `query` goes through `pipeline::query` over `wiring.source()` alone, as `graph` does. If the
  test fails with "is in use by another buildl", the method was wired through `ports()` instead.
- **The label-order expectation in task 1** comes from a probe on `BTreeSet<Label>` (spec P9).
  The test re-proves it.

## Proof

- `cargo make dod` and `cargo deny check` green.
- `cargo test -p buildl --test e2e facade`: the facade answers `deps(//:report)` on the `chain`
  fixture with `//:greet`, `//:greeting`, `//lib:words` and writes no `.buildl/` and no `out/`.
