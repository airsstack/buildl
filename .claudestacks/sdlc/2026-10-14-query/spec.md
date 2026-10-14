---
status: approved
created: 2026-10-14
built-on: 2c3217be8603
---

# Spec: query — `deps()` and `rdeps()` over the target graph

This spec delivers `buildl query <expr>`. It answers two questions about a workspace's target
graph: what one target needs (`deps`) and what needs it (`rdeps`). The answer is the set of target
labels the question reaches, printed on stdout. The command stops after Resolve, as design §7 has
it, and it is the first slice of milestone 2 (design §13 item 2). It also makes the documents agree
on what `query` is, and opens the milestone 2 ladder in the roadmap.

Evidence notation: `path:line` is a line opened in this task, at commit `ce57760`. **Probe** marks
a command that was run in this task, with its real output quoted.

## 1. Premises (what exists today)

| # | Fact | Evidence |
|---|---|---|
| P1 | `TargetGraph` answers both directions, one step at a time. `deps(id)` gives a node's direct dependencies; `dependents(id)` gives the nodes that depend on it directly, in id order. | `crates/buildl-core/src/types/target_graph.rs:132`, `:138` |
| P2 | `TargetGraph::id_of(label)` finds a target by its label or by an alias. | `target_graph.rs:144-149` |
| P3 | `narrow` already walks the `deps` direction transitively with a stack and a `wanted: Vec<bool>`. Its private `unknown(graph, label)` builds `Error::UnknownTarget` with the nearest declared label or alias. | `crates/buildl-core/src/resolve/narrow.rs:31-49`, `:87-98` |
| P4 | `Error::UnknownTarget` maps to exit 2 (`Usage`). The real binary prints the suggestion. | `crates/buildl-cli/src/exit.rs:78`. **Probe** (copy of the `chain` fixture): `buildl plan //lib:wordz` → `error: target //lib:wordz was asked for but is not declared (did you mean //lib:words?)`, `exit=2` |
| P5 | `Error` is `#[non_exhaustive]`, and `Exit::of_error` sends any variant without a row to `Internal` (7). A new variant therefore needs its own row. | `crates/buildl-core/src/error.rs:41`; `exit.rs` `of_error`, final `\| _ => Self::Internal` arm |
| P6 | `check` and `graph` are free functions over a `DeclarationSource` alone. `Workspace::check` and `Workspace::graph` call them and take no lock. | `crates/buildl-core/src/pipeline/driver.rs:163`, `:175`; `pipeline/mod.rs:14`; `crates/buildl/src/workspace.rs:80`, `:91` |
| P7 | `Workspace::label(raw)` reads `//dir:name` as written. It reads `:name` and `name` against the directory the workspace was opened from. | `workspace.rs:62-70`; `crates/buildl-core/src/types/label.rs:76` |
| P8 | The CLI turns a bad target into a clap `ValueValidation` error, which exits 2. | `crates/buildl-cli/src/run.rs:145-161`, `told` at `run.rs:118` |
| P9 | `Label` orders by directory, then name. That differs from the byte order of the label's text. `BTreeSet<Label>` serializes through the canonical serializer as a JSON array of strings. | `label.rs:26-27` (`Ord`, `#[serde(into = "String")]`). **Probe** (throwaway test, deleted): set of `//lib:a //:z //a:x //a-b:x` → `ORDER ["//:z", "//a:x", "//a-b:x", "//lib:a"]`, `JSON ["//:z","//a:x","//a-b:x","//lib:a"]`. Byte order would put `//a-b:x` before `//a:x`. |
| P10 | No `query` or `rdeps` exists in any crate. | **Probe**: `grep -rniE 'query\|rdeps' crates \| grep -v /target/ \| wc -l` → `0`. Control, same method: `grep -rn 'fn graph_dot' crates \| wc -l` → `3` |
| P11 | buildl-core module edges allow `pipeline → resolve, types` and `resolve → error, types`. A new file inside `resolve/` or `types/` adds no edge. | `crates/expected-module-edges.txt` |
| P12 | Shape of the `chain` fixture's graph: `//:greeting` → `//:greet`, `//lib:words`; `//:report` → `//:greeting`; `//:site` → `//lib:assets`; the alias `//:text` → `//lib:words`. | **Probe**: `buildl graph` on a copy of `crates/buildl/tests/fixtures/chain`, exit 0 |
| P13 | `buildl-cli/tests/cli` runs the real binary (`CARGO_BIN_EXE_buildl`) against workspaces it writes. `buildl/tests/e2e` drives the `Workspace` facade over copies of the fixtures. | `crates/buildl-cli/tests/cli/common.rs:63-64`, `:12`; `crates/buildl/tests/e2e/common.rs:56` |
| P14 | Rendering belongs to `buildl::report`: it holds "every rendering of a value into text … so that a caller formats nothing itself", with `plan_text`, `graph_dot` and `check_line` beside each other. The CLI holds the argument surface and exit codes. Canonical JSON is the value's own serialized form and stays in `run.rs`, as `graph` and `plan` do today. | `crates/buildl/src/report/mod.rs:1-6`, `:9-22`, `:24-26`; `docs/architecture-building-blocks.md:130`, `:150`; `crates/buildl-cli/src/run.rs:12`, `:174-178`, `:190-194` |
| P15 | Two more documents give "`query` / `rdeps`" as a heading, as roadmap §2 does. | `docs/design.md:715` (§13 item 2), `README.md:183` |

## 2. The command

```text
buildl query <EXPR> [--json]

EXPR  :=  deps(<target>)  |  rdeps(<target>)
target := //dir:name | :name | name       (the last two are read against the start directory, P7)
```

| Rule | Behaviour |
|---|---|
| `deps(T)` | every target T depends on, transitively. T itself is not in the answer. |
| `rdeps(T)` | every target that depends on T, transitively. T itself is not in the answer. |
| T is an alias | the alias is followed to its target (P2). The answer lists only targets, never aliases. |
| Output (default) | one label per line in label order (P9), each line ending in `\n`; nothing when the answer is empty |
| Output `--json` | the same set as canonical JSON (P9) followed by `\n`; `[]` when empty |
| Streams | the answer goes to stdout and nothing else does (design §8.7); errors go to stderr |
| Lock and state | no lock, no state directory, nothing written: Load then Resolve, like `graph` (P6) |
| Spelling | function names are lowercase and exact. Whitespace around the whole expression and inside the parentheses is ignored. Anything else is malformed. |

Exit codes (design §8.7):

| Case | Exit |
|---|---|
| answered (even if empty) | 0 |
| malformed expression, or a target that fails the label grammar | 2, via the CLI's usage path (P8) |
| T names no target or alias | 2, `Error::UnknownTarget` with a suggestion (P3, P4) |
| build file or manifest error | 5, unchanged |
| stdout closed early (`\| head`) | 0, unchanged (`run.rs` BrokenPipe arm) |

## 3. Components

```text
buildl-cli   Command::Query(QueryArgs { expression, --json })
               │  workspace.query_of(raw)  ── error ─► clap ValueValidation ─► exit 2
               │  workspace.query(&q)      ── error ─► Exit::of_error
               ▼
buildl       Workspace::query_of(&self, raw) -> Result<Query>       Query::parse(raw, |t| self.label(t))
             Workspace::query(&self, &Query) -> Result<BTreeSet<Label>>   no lock (as graph)
               ▼
buildl-core  pipeline::query(source, entry, &Query)  = resolve::reach(&graph(source, entry)?, q)
             Pipeline::query(&self, entry, &Query)   = same, over self.ports.source
             types::Query        the expression, parsed
             resolve::reach      the transitive walk, in either direction, and
                                 unknown_target(graph, label): moved here from narrow.rs
```

### 3.1 `buildl-core::types::query` (new file `types/query.rs`)

```rust
/// Which way a query walks the graph.
pub enum Direction { Deps, Rdeps }

/// A parsed `deps(<target>)` or `rdeps(<target>)`.
pub struct Query { direction: Direction, target: Label }

impl Query {
    pub const fn new(direction: Direction, target: Label) -> Self;
    pub const fn direction(&self) -> Direction;
    pub const fn target(&self) -> &Label;
    /// Reads `raw`, reading the target through `label` (the caller decides what a relative
    /// target is relative to).
    pub fn parse(raw: &str, label: impl FnOnce(&str) -> Result<Label>) -> Result<Self>;
}
```

- The fields are private and only `parse` and `new` build a `Query`, following the
  strong-types guideline (parse, don't validate).
- `parse` returns the new `Error::InvalidQuery { expression: String }` when the text is not a
  known function with one parenthesised argument. When the argument fails the label grammar, it
  returns whatever `label` returns (`Error::InvalidName`). Its message is
  `` query `{expression}` is not deps(<target>) or rdeps(<target>) ``, so a malformed expression
  reaches stderr as `invalid value '<raw>' for '<EXPR>': query `<raw>` is not deps(<target>) or rdeps(<target>)`.
- Both `Direction` and `Query` derive `Debug, Clone, PartialEq, Eq`, and `Direction` derives
  `Copy`.
- Re-exported from `types/mod.rs`. The colocated unit tests cover: both functions, whitespace,
  the wrong case, a missing or extra parenthesis, an empty argument, two arguments, an unknown
  function, and a label error passed through.

### 3.2 `buildl-core::resolve::reach` (new file `resolve/reach.rs`)

- `pub fn reach(graph: &TargetGraph, query: &Query) -> Result<BTreeSet<Label>>` finds the start
  with `id_of` (P2). An unknown target gives `unknown_target(graph, label)`. It then walks
  `deps` or `dependents` (P1) transitively. The start node is left out of the answer, even when
  a cycle would reach it again; Resolve refuses cycles anyway. The answer is returned as labels.
- `pub(super) fn closure(graph, starts, step) -> Vec<bool>` is the stack walk lifted out of
  `narrow` (P3), with `step` being `TargetGraph::deps` or `TargetGraph::dependents`. `narrow`
  calls it with `deps` and starts included, so its behaviour is unchanged, and its existing tests
  prove that. One walk now serves both callers, instead of two copies.
- `unknown` moves from `narrow.rs` into `reach.rs` as `pub(super) fn unknown_target`, beside
  `closure`, so `narrow` and `reach` build the same error. It does not go into `suggest.rs`:
  that file compares rendered names only and lists "choosing the candidates" as a
  non-responsibility (`crates/buildl-core/src/resolve/suggest.rs:3-9`), and `unknown_target`
  chooses them (every target label and alias of the graph). `closure` is `pub(super)` too:
  only `narrow` and `reach` call it.
- `resolve/mod.rs` gains `pub mod reach;` and `pub use reach::reach;`, and its Responsibilities
  line (`resolve/mod.rs:5-6`) names [`reach`]. No module edge changes (P11).
- Unit tests in `reach.rs` cover: deps and rdeps on a diamond graph (no duplicates), a leaf
  (empty answer), an alias as the start, an unknown label with its suggestion, and the start
  left out.

### 3.3 `buildl-core::pipeline`

- A free `pub fn query<S: DeclarationSource>(source: &S, entry: &EntryName, query: &Query) ->
  Result<BTreeSet<Label>>` in `driver.rs`, defined as `reach(&graph(source, entry)?, query)`.
- `Pipeline::query(&self, entry, query)` calls it over `self.ports.source`, in the way
  `Pipeline::graph` mirrors `graph`.
- `pipeline/mod.rs` re-exports `query`, and its Responsibilities line (`pipeline/mod.rs:6`) names
  it; `driver.rs`'s module doc names `Pipeline::query` and `query` with the others.
- A flow test, `crates/buildl-core/tests/flows/query.rs` registered in `flows/main.rs`, covers
  deps and rdeps through `FakePorts`, an unknown target, and a build-file error that passes
  through unchanged.

### 3.4 `buildl::Workspace`

- `query_of(&self, raw: &str) -> Result<Query>` calls `Query::parse(raw, |t| self.label(t))`.
- `query(&self, query: &Query) -> Result<BTreeSet<Label>>` calls `pipeline::query(&self.wiring.source(),
  self.wiring.entry(), query)`.
- Its rustdoc says it takes no lock and opens no state file, as `graph` does.
- An e2e test in `crates/buildl/tests/e2e/facade.rs` runs on the `chain` fixture (P12):

| Query | Expected answer |
|---|---|
| `deps(//:report)` | `{//:greet, //:greeting, //lib:words}` |
| `rdeps(//:text)` | `{//:greeting, //:report}` |
| `rdeps(//:report)` | `{}` |

  The same test asserts that the state directory does not exist afterwards.

### 3.5 `buildl-cli`

- `cli.rs` gains `Command::Query(QueryArgs)`, with `QueryArgs { expression: String
  (value_name = "EXPR"), #[arg(long)] json: bool }` and a doc line "Ask what a target needs, or
  what needs it".
- `run.rs` gains a `query(start, args, stdout)` function:
  1. open the workspace;
  2. call `query_of(&args.expression)`, and turn any error into a clap `ValueValidation` error,
     "invalid value '{raw}' for '<EXPR>': {error}", the same pattern `scope` uses (P8);
  3. call `query`;
  4. render the answer and write it with `product`.
- The text form comes from `buildl::report::query_text(&BTreeSet<Label>) -> String`, a new file
  `crates/buildl/src/report/query_text.rs` with its own unit tests. It is re-exported from
  `report/mod.rs` and listed in that file's Responsibilities, beside `plan_text` (P14). JSON goes
  through `canonical::to_vec` followed by `\n` in `run.rs`, as `graph` and `plan` do (P14).
- `exit.rs` gets the row `Error::InvalidQuery { .. } => Self::Usage`, beside `UnknownTarget`
  (P5). It is unit-tested like the existing rows.
- The command table in the `main.rs` doc comment gains the row `` `buildl query <EXPR> [--json]` | the targets the
  expression reaches, one per line, or canonical JSON | nothing ``.
- Binary tests go in `crates/buildl-cli/tests/cli/`: `output.rs` runs both `deps()` and `rdeps()`
  through the binary, each in text and `--json`, plus an empty answer; `exit_codes.rs` covers a malformed expression → 2, an unknown target → 2 with
  `did you mean`; `arguments.rs` covers a missing `EXPR` → 2, the help of `query`, and a
  relative target read from a subdirectory, beside the test that finds the workspace from one. The malformed case asserts the stderr text given in §3.1.

## 4. Documentation

| File | Change |
|---|---|
| `docs/design.md:213` | change `(§10.1)` to `(§10.2, §10.3, §11.1)`. In the paragraph at `:216`, add `query` to the built commands and give its syntax. |
| `docs/design.md:715` | §13 item 2's heading "`query` / `rdeps`" becomes "`query` (`deps()`, `rdeps()`)" (P15) |
| `README.md:183` | the same heading change as `design.md:715` (P15) |
| `README.md:7` | the status line names `query` with the built commands |
| `README.md:123` | the `query` row: `not yet` becomes `yes`; the shipped-commands prose gains `query` where it lists them |
| `docs/architecture-building-blocks.md` | §5.4 pure-logic table (`:410`, the `narrow` row): a `reach` row, `TargetGraph` and a `Query` → the `BTreeSet<Label>` the query reaches, the start left out; a label nothing declares is `Error::UnknownTarget`; no port. `:416`, `:532-533`, `:585`: wherever `check` and `graph` are named as the lock-free, source-only commands, `query` joins them |
| `docs/architecture.md` | `:3` status line names `query`; `:374` "`plan` and `graph` record nothing" gains `query`; §3.2: `TargetGraph`'s reverse edges also serve `query`'s `rdeps`; §6 lock row `:379`: `query` takes no lock |
| `docs/roadmap.md` | §1: "No later milestone has an intent yet; milestone 2 is next." becomes "Milestone 2 is cut into six intents (§3.2); no later milestone has an intent yet." §2 row 2: "`query` (`deps()`, `rdeps()`)" instead of "`query` / `rdeps`", status **in progress (§3.2)**. §3 becomes "Intent ladders", with the existing content as §3.1 "Milestone 1" and a new §3.2 "Milestone 2" holding the agreed ladder (I10 query → I11 test → I12 log → I13 clean + gc → I14 parallel load → I15 demand-driven load, the last limited to commands that name targets) in a mermaid diagram and status table. Renumbering stays inside §3, so every existing `§3`/`§4`/`§5`/`§6` reference still holds. §4 gains I10's row when the chain is done. |
| `CLAUDE.md` | the Status paragraph (`:7`) names `query` with the built commands; the pipeline line (`:26`) lists `query` beside `graph` as the pipeline truncated after Resolve |

Each claim above is re-checked against the file when the plan is written, since line numbers can
move.

## 5. Determinism and rules

- The answer is a `BTreeSet<Label>`. No `HashMap` reaches the output, and JSON goes only through
  `canonical` (architecture §5).
- `buildl-core` gains no I/O (guard `guard-core-purity`), no new module edge (P11), and no new
  dependency (guard `guard-crate-edges`). In `buildl`, `workspace.rs` (allowed to meet
  adapters) and the new `report/query_text.rs` change; `report` names no other adapter module
  (guard `guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` pass before every commit. The work happens on
  `worktree-m2-query`.

## 6. Testing summary

| Layer | Where | Proves |
|---|---|---|
| unit | `types/query.rs`, `resolve/reach.rs`, `resolve/narrow.rs` (existing tests, unchanged), `buildl/src/report/query_text.rs`, `buildl-cli` `exit.rs` | parsing, the walk, the refactor leaves narrow unchanged, rendering, exit row |
| flow | `buildl-core/tests/flows/query.rs` | the pipeline answer over `FakePorts`, and errors |
| e2e facade | `buildl/tests/e2e/facade.rs` | a real Lua workspace (`chain`), aliases, no state written |
| binary | `buildl-cli/tests/cli/{output,exit_codes,arguments}.rs` | `deps()` and `rdeps()` stdout bytes in both forms, exit codes, stderr of a malformed expression, relative targets |

## 7. Non-goals

- Query functions other than `deps` and `rdeps`: set operators, depth limits, filters such as
  "tests only", several targets in one call, file-path arguments.
- Including the start target in the answer, and listing aliases.
- Demand-driven or parallel loading (I14, I15). `query` always loads the whole workspace.
- `test`, `log`, `clean`, `gc` (I11–I13).
- Running what `query` returns; that is a shell pipe over its output.
