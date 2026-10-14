---
status: approved
created: 2026-10-14
built-on: 050ed7322c38
---

# Core query Implementation Plan

**Goal:** `buildl-core` answers a `deps()` or `rdeps()` query over a workspace's target graph.

**Architecture:** A query is a validated value, `types::Query`, read once from text. Answering it is
a transitive walk over `TargetGraph`'s forward or reverse edges. The walk is lifted out of `narrow`
into a new `resolve/reach.rs`, so narrowing and querying share one walk and one unknown-target error.
`pipeline::query` and `Pipeline::query` run Load and Resolve, then that walk. No state port is
involved, so no new module edge or dependency is needed.

**Tech Stack:** Rust 2024 (`rust-version` 1.94), `thiserror`, the crate's canonical serializer.
Definition of Done: `cargo make dod` and `cargo deny check`.

**Content authority:** spec §2 (the command's rules), §3.1–§3.3, §5, §6.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-query` (branch
`worktree-m2-query`), never on `main`. One commit per task, Conventional Commits, scope
`buildl-core`.

Facts the tasks rest on, each read at commit `ce57760`:

| Fact | Where |
|---|---|
| `TargetGraph::deps(id)` and `dependents(id)` give one step as `Option<&[NodeId]>`; `id_of(label)` finds a target or an alias; `nodes()` is indexed by `NodeId::index()`; `Node.label` is public | `crates/buildl-core/src/types/target_graph.rs:117`, `:132`, `:138`, `:144` |
| `narrow`'s stack walk and its private `unknown` | `crates/buildl-core/src/resolve/narrow.rs:35-47`, `:87-98` |
| `narrow`'s rustdoc links `[`Error::UnknownTarget`]`, so `narrow.rs` must keep `Error` in scope | `narrow.rs:29` |
| `suggest.rs` compares rendered names only; choosing candidates is its stated non-responsibility | `crates/buildl-core/src/resolve/suggest.rs:3-9` |
| `nearest` allows at most `len / 3` edits (at least one) | `suggest.rs:24` |
| `Error` is `#[non_exhaustive]`; `UnknownTarget`'s doc reads "A run was asked for a target, and no target or alias has that label." | `crates/buildl-core/src/error.rs:41`, `:285` |
| `Label::parse(&str)`, `Label::resolve(&str, &Directory)`, `Directory::parse(impl Into<String>)` | `types/label.rs:46`, `:76`; `types/directory.rs:45` |
| Root re-exports are explicit lists | `crates/buildl-core/src/lib.rs:106-135` |
| Module edges `resolve → error, types` and `pipeline → resolve, types` are already allowed | `crates/expected-module-edges.txt` |
| Test fixtures: `resolve/fixtures.rs` has `target(raw, deps)`, `alias(raw, target)`, `label(raw)`; the flows harness has `FakeSource`, `workspace`, `file(items, subdirs)`, `target(name, deps)`, `alias(name, target)`, `ports(source)`; a directory with no file evaluates to `Absent` | `resolve/fixtures.rs:20`, `:75`, `:100`; `tests/flows/common.rs:95`, `:984`, `:1034`, `:1060`, `:1097`, `:1115` |
| This repo's clippy wants emptiness asserted with `assert_eq!` against an empty value, not `assert!(… .is_empty())` | commit `166dcc6` |
| `cargo test` takes one test-name filter; a second one is refused (`error: unexpected argument … found`) | cargo 1.99 |

## Files that change

```
crates/buildl-core/src/error.rs            — [modify] Error::InvalidQuery; UnknownTarget's doc names a query too
crates/buildl-core/src/types/query.rs      — [create] Direction, Query, Query::parse, with unit tests
crates/buildl-core/src/types/mod.rs        — [modify] declare and re-export query; index line
crates/buildl-core/src/resolve/reach.rs    — [create] closure, unknown_target, reach, with unit tests
crates/buildl-core/src/resolve/narrow.rs   — [modify] walk and unknown-target error come from reach.rs
crates/buildl-core/src/resolve/mod.rs      — [modify] declare and re-export reach; Responsibilities line
crates/buildl-core/src/pipeline/driver.rs  — [modify] free query, Pipeline::query, module doc
crates/buildl-core/src/pipeline/mod.rs     — [modify] re-export query; Responsibilities line
crates/buildl-core/src/lib.rs              — [modify] root re-exports and crate doc
crates/buildl-core/tests/flows/query.rs    — [create] pipeline flows over FakeSource
crates/buildl-core/tests/flows/main.rs     — [modify] register the query flow module
```

## Order of work

### Task 1 — An error for a query in neither form

**Files:**
- Modify `crates/buildl-core/src/error.rs` — the new variant beside `UnknownTarget`; `UnknownTarget`'s doc line.

**Change:** `Error::InvalidQuery { expression: String }` is added right after `UnknownTarget`.
Its doc reads "A query was written in neither of the two forms a query takes", and its field
holds the query as it was written. Its message is exactly
`` query `{expression}` is not deps(<target>) or rdeps(<target>) ``. `UnknownTarget`'s doc line
becomes "A run or a query was asked for a target, and no target or alias has that label." The
CLI's `Exit::of_error` keeps compiling, because its last arm is `_ => Self::Internal`. Plan `03`
gives the variant its row.

**Proof:**
- `invalid_query_names_the_query_and_the_two_forms` in `error.rs`'s tests — `InvalidQuery`
  holding `dep(//:a)` renders as `` query `dep(//:a)` is not deps(<target>) or rdeps(<target>) ``.

**Done when:** `cargo test -p buildl-core --lib error::tests` passes and the Definition of Done is
green. Commit `feat(buildl-core): an error for a query in neither form`.

### Task 2 — The `Query` value and its parser

**Files:**
- Create `crates/buildl-core/src/types/query.rs` — the two types, the parser, colocated tests.
- Modify `crates/buildl-core/src/types/mod.rs` — `pub mod query;` after `provenance`;
  `pub use query::{Direction, Query};`; a Responsibilities bullet after the `Scope` /
  `LiveOutput` one: "`Query` and `Direction` — a question about the target graph: what one target
  needs, or what needs it."
- Modify `crates/buildl-core/src/lib.rs` — add `Direction` (between `Digest` and `Directory`)
  and `Query` (between `Provenance` and `RawOutcome`) to the root `pub use types::{…}` list.

**Change:** `query.rs` holds these items.
- `Direction` is `Deps` or `Rdeps`. It derives `Debug, Clone, Copy, PartialEq, Eq` and has a
  private `const fn name` that returns `"deps"` or `"rdeps"`.
- `Query { direction, target: Label }` has private fields and derives
  `Debug, Clone, PartialEq, Eq`. It has `const fn new(direction, target)`, and the accessors
  `direction()` and `target()`, each `#[must_use]`.
- `Query::parse(raw: &str, label: impl FnOnce(&str) -> Result<Label>) -> Result<Query>` follows
  spec §2:
  - Trim the whole text.
  - Match an exact lowercase function name, directly followed by `(`, with `)` as the last
    character.
  - Trim the inside of the parentheses.
  - Refuse an empty target, or one that still holds whitespace, `(`, `)` or `,`, with
    `InvalidQuery` holding `raw` exactly as given.
  - Hand the target text to `label`, and pass `label`'s error through unchanged.

  The module doc follows the house shape: why its own file, Responsibilities, Non-responsibilities.
  It says that answering a query, and deciding what a relative target is relative to, are not this
  file's work.

**Proof** (in `query.rs`'s tests, reading targets with `Label::parse` unless said otherwise):
- `each_function_reads_its_direction_and_target` — `deps(//:app)` is `Query::new(Deps, //:app)`;
  `rdeps(//lib:text)` is `Query::new(Rdeps, //lib:text)`.
- `whitespace_around_the_query_and_inside_the_parentheses_is_ignored` — `"  rdeps(  //lib:text \t)\n"`
  reads as `rdeps(//lib:text)`.
- `the_target_is_read_by_the_callers_reader` — with a reader that is
  `Label::resolve(raw, &Directory::parse("lib"))`, `deps(:text)` has target `//lib:text` and
  direction `Deps`.
- `a_query_in_neither_form_is_refused_as_written` — each of these is `InvalidQuery` whose
  `expression` equals the input exactly: `""`, `deps`, `deps()`, `deps(  )`, `deps(//:a`,
  `deps//:a)`, `deps(//:a))`, `deps((//:a))`, `deps(//:a)x`, `deps (//:a)`, `DEPS(//:a)`,
  `dep(//:a)`, `reverse(//:a)`, `deps(//:a,//:b)`, `deps(//:a //:b)`.
- `a_target_that_fails_its_grammar_is_the_readers_error` — `deps(//lib)` is `Error::InvalidName`,
  since `Label::parse` rejects a directory-only reference (`label.rs:23-25`).

**Done when:** `cargo test -p buildl-core --lib types::query` passes and the Definition of Done
is green. Commit `feat(buildl-core): a query value read from deps() or rdeps()`.

### Task 3 — Lift `narrow`'s walk into `resolve/reach.rs`

**Files:**
- Create `crates/buildl-core/src/resolve/reach.rs` — `closure` and `unknown_target`, with tests
  for `closure`.
- Modify `crates/buildl-core/src/resolve/narrow.rs` — uses both from `reach.rs`.
- Modify `crates/buildl-core/src/resolve/mod.rs` — `mod reach;` (private for now) after
  `pub mod narrow;`.

**Change:** This task is a refactor, and `narrow`'s behaviour does not change.
- `reach.rs` gets `pub(super) fn closure(graph, starts: Vec<NodeId>, step) -> Vec<bool>`. It is
  the stack walk from `narrow.rs:35-47`, unchanged. `step` is `TargetGraph::deps` or
  `TargetGraph::dependents`, taken as
  `fn(&TargetGraph, NodeId) -> Option<&[NodeId]>`, as spec §3.2 has it. The result is one flag
  per node, indexed by `NodeId::index()`, and the starts are flagged.
- `reach.rs` also gets `pub(super) fn unknown_target(graph, label) -> Error`. This is
  `narrow.rs`'s private `unknown`, moved here unchanged: the candidates are every node label plus
  every alias, ranked by `suggest::nearest`. It does not go into `suggest.rs`, for the reason in
  the facts table.
- `narrow` builds its starts with `unknown_target` for a missing label, then calls
  `closure(graph, starts, TargetGraph::deps)`. Its private `unknown` is deleted.
- `narrow.rs` keeps `Error` imported, because its rustdoc links `Error::UnknownTarget`
  (`narrow.rs:29`). It drops the imports it no longer uses (`suggest::nearest`, `Label`).
- `reach.rs`'s module doc says this file holds the walk that narrowing and querying share, and
  the error for a label naming nothing. Ranking names is `suggest`'s job. Because `closure` and
  `unknown_target` are not public, the doc names them in plain backticks, not intra-doc links.

**Proof:**
- `narrow`'s six existing tests in `narrow.rs` pass unchanged, the unknown-label suggestion
  included. That shows the refactor kept its behaviour.
- New tests in `reach.rs` use a diamond fixture: `//:top` → `//:left`, `//:right`; both →
  `//:base`; `//:solo` alone; alias `//:all` → `//:top`.
  - `deps_reach_everything_below_once_with_the_start` — `closure` from `//:top` over `deps` flags
    `//:base`, `//:left`, `//:right`, `//:top`, each once.
  - `rdeps_reach_everything_above_once_with_the_start` — `closure` from `//:base` over
    `dependents` flags the same four.
  - `no_start_reaches_nothing` — `closure` with no start flags nothing (asserted with
    `assert_eq!` against an empty list).

**Done when:** `cargo test -p buildl-core --lib resolve::` passes and the Definition of Done is
green, `guard-module-edges` included. Commit
`refactor(buildl-core): one graph walk for narrowing and reach`.

### Task 4 — `reach`: the targets a query reaches

**Files:**
- Modify `crates/buildl-core/src/resolve/reach.rs` — `reach` and its tests; module doc.
- Modify `crates/buildl-core/src/resolve/mod.rs` — `pub mod reach;`, `pub use reach::reach;`,
  and a Responsibilities line that also names `reach` as "the targets a query reaches".
- Modify `crates/buildl-core/src/lib.rs` — `pub use resolve::{narrow, reach, resolve};`.

**Change:** `pub fn reach(graph: &TargetGraph, query: &Query) -> Result<BTreeSet<Label>>` works
in four steps:
1. Find the query's target with `id_of`. A target that is not there is
   `unknown_target(graph, target)`.
2. Walk the query's direction with `closure` (`deps` for `Deps`, `dependents` for `Rdeps`),
   starting from that target's one-step neighbours.
3. Clear the target's own flag, so it is out of the answer even if a cycle could reach it again.
4. Collect the flagged nodes' labels.

Aliases are never in the answer, since they are not nodes. The rustdoc states all of this and has
an `# Errors` section for `UnknownTarget`. The module doc's Responsibilities line gains `reach`.

**Proof** (new tests in `reach.rs`, over the diamond fixture):
- `deps_answer_everything_the_target_needs_without_the_target` — `deps(//:top)` answers
  `//:base`, `//:left`, `//:right`, and `deps(//:left)` answers `//:base`.
- `rdeps_answer_everything_that_needs_the_target_without_the_target` — `rdeps(//:base)` answers
  `//:left`, `//:right`, `//:top`, and `rdeps(//:left)` answers `//:top`.
- `a_leaf_answers_nothing` — `deps(//:base)`, `rdeps(//:top)` and `rdeps(//:solo)` each answer
  an empty set.
- `an_alias_is_followed_and_never_answered` — `deps(//:all)` answers what `deps(//:top)` does;
  `rdeps(//:all)` is empty; `rdeps(//:base)` does not hold `//:all`.
- `a_target_naming_nothing_is_refused_with_the_nearest_one` — `deps(//:tpo)` is `UnknownTarget`
  for `//:tpo` with suggestion `//:top`. It is 2 edits away, within `6 / 3 = 2`, and every other
  label is further.

**Done when:** `cargo test -p buildl-core --lib resolve::reach` passes and the Definition of Done
is green. Commit `feat(buildl-core): reach answers a query over the graph`.

### Task 5 — `pipeline::query` and `Pipeline::query`

**Files:**
- Create `crates/buildl-core/tests/flows/query.rs` — flows over `FakeSource`.
- Modify `crates/buildl-core/tests/flows/main.rs` — `mod query;` after `mod plan;`.
- Modify `crates/buildl-core/src/pipeline/driver.rs` — the free function, the method, imports,
  module doc.
- Modify `crates/buildl-core/src/pipeline/mod.rs` — `pub use driver::{Pipeline, check, graph, query};`
  and a Responsibilities line naming `query`.
- Modify `crates/buildl-core/src/lib.rs` — crate doc.

**Change:**
- In `driver.rs`, a free `pub fn query<S: DeclarationSource>(source, entry, asked: &Query) ->
  Result<BTreeSet<Label>>` is `reach(&graph(source, entry)?, asked)`. It sits after the free
  `graph`.
- `Pipeline::query(&self, entry, asked)` calls it over `self.ports.source` and sits after
  `Pipeline::graph`. The parameter is named `asked`, so that inside the method `query` still
  names the free function.
- Both rustdocs say they load once and resolve, give the answer in label order without the
  named target, and in `# Errors` return what `graph` returns plus `UnknownTarget`.
- `driver.rs`'s module doc names `Pipeline::query` and says there are three commands that reach
  only build files (`check`, `graph`, `query`).
- `lib.rs`'s crate doc: the `resolve` row of "Where things live" adds "and the targets a query
  reaches"; the `pipeline` row says "the three commands that reach only build files". The closing
  sentence about `pipeline::check` and `pipeline::graph` becomes one that also names
  `reach` / `Query` / `Direction` and `pipeline::query`.

**Proof** (`tests/flows/query.rs`). The fixture: root declares `//:app` with deps `main.o` and
`//lib:text`, `//:main.o`, and alias `//:default` → `//:app`, and names subdir `lib`; `lib`
declares `//lib:text`.
- `deps_answer_what_a_target_needs_across_build_files` — `deps(//:app)` and `deps(//:default)`
  both answer `//:main.o`, `//lib:text`.
- `rdeps_answer_what_needs_a_target_across_build_files` — `rdeps(//lib:text)` answers `//:app`;
  `rdeps(//:app)` answers an empty set.
- `a_pipeline_answers_what_the_free_function_answers` — `rdeps(//:main.o)` gives the same set
  from `pipeline::query` and from `Pipeline::<FakePorts>::query`.
- `a_target_naming_nothing_is_refused_with_the_nearest_one` — `deps(//lib:txt)` is
  `UnknownTarget` with suggestion `//lib:text`.
- `a_missing_build_file_stops_the_query_before_it_is_answered` — a root that names subdir `lib`
  with no `lib` file gives `Error::MissingBuildFile`.

**Done when:** `cargo test -p buildl-core --test flows query` passes, the Definition of Done is
green, and `cargo deny check` is green. Commit
`feat(buildl-core): pipeline query over a source of build files`.

---

## Risks

- **Task 3 is the riskiest step**, because it changes working code that `plan` and `build` run
  through. The guard is that `narrow`'s six tests stay unchanged and pass. If any one has to be
  edited to pass, the refactor changed behaviour: stop and record a deviation.
- **Passing `TargetGraph::deps` as `step`.** It relies on a method coercing to the
  higher-ranked fn pointer `fn(&TargetGraph, NodeId) -> Option<&[NodeId]>` under lifetime
  elision. Only compiling settles this. If it does not coerce, a closure `|g, id| g.deps(id)`
  does the same job, and the spec's shape still holds.
- **Rustdoc links.** `-D warnings` fails on a broken or private intra-doc link. This covers
  `narrow`'s `Error::UnknownTarget` link (the reason `Error` stays imported) and any public doc
  that would link the `pub(super)` helpers.
- **Root re-export order.** `rustfmt` reflows the `lib.rs` lists. Run `cargo fmt --all` after
  editing them.
- **Not taken.** A `Direction` argument to `closure` instead of a `step` function, and putting
  `unknown_target` in `suggest.rs`. The spec fixed both (§3.2).

## Proof

- `cargo make dod` green: fmt, clippy `-D warnings`, rustdoc `-D warnings`, tests, doctests,
  and the guards `guard-crate-edges`, `guard-module-edges`, `guard-core-purity`.
- `cargo deny check` green.
- `cargo test -p buildl-core --test flows query` shows the pipeline answering `deps(//:app)`
  with `//:main.o`, `//lib:text`.
- `git diff --stat main -- crates/expected-module-edges.txt crates/expected-edges.txt` is empty:
  no new edge.
