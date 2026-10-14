---
status: done
created: 2026-10-14
depends-on: [02]
built-on: 050ed7322c38
---

# CLI query Implementation Plan

**Goal:** the `buildl` binary runs `buildl query <EXPR> [--json]` and prints the answer on standard output.

**Architecture:** `buildl-cli` gains a `query` subcommand and one command function in `run.rs`.
The function opens the workspace and reads the expression with `Workspace::query_of`. A refused
expression becomes a clap `ValueValidation` error, the path a bad target already takes in `plan`
and `build`, so it exits 2. The function answers with `Workspace::query`. It writes either
`report::query_text` or the set's canonical JSON plus a newline, as `graph` and `plan` write
JSON. `Exit::of_error` gives `Error::InvalidQuery` the usage status. The CLI formats no text
itself.

**Tech Stack:** Rust 2024, `clap` (derive), the binary's test harness (`CARGO_BIN_EXE_buildl`).
Definition of Done: `cargo make dod` and `cargo deny check`.

**Content authority:** spec §2 (output, streams, exit codes), §3.5, §6.

---

Work in `/Users/hiraq/Projects/airsstack/buildl/.claude/worktrees/m2-query` (branch
`worktree-m2-query`). Plan `02` must be `done` first. It adds `Workspace::query_of`,
`Workspace::query` and `buildl::report::query_text`; plan `01` adds `Error::InvalidQuery`. One
commit per task, scope `buildl-cli`.

Facts the tasks rest on, read at commit `ce57760`:

| Fact | Where |
|---|---|
| `of_error`'s first arm maps `UnknownOverride` and `UnknownTarget` to `Usage`; a variant with no row falls to `_ => Self::Internal` | `crates/buildl-cli/src/exit.rs:78`, `:109-112` |
| `every_error_variant_has_its_code` counts `[2, 2, 1, 16, 3, 6]` per row and `30` distinct variants; `usage_errors()` lists the usage ones | `exit.rs:186-197`, `:424-447` |
| `scope()` turns a target error into `Cli::command().error(ErrorKind::ValueValidation, format!("invalid value '{raw}' for '[TARGET]...': {error}"))`; `told` writes it to stderr with status 2 | `crates/buildl-cli/src/run.rs:145-161`, `:118-128` |
| `graph` writes `canonical::to_vec(&value)` plus `\n` through `product` | `run.rs:172-184` |
| `run.rs` unit tests drive `run` over values: `copying()` is a workspace with one target `//:copy`; `ran(start, args)` returns exit, stdout and stderr | `run.rs:268-274`, `:302-321` |
| `cli.rs`'s module doc lists "`GraphArgs`, `PlanArgs`, `BuildArgs` and `Selection` — the arguments of a command" | `crates/buildl-cli/src/cli.rs:11` |
| The help, short help and refusals are pinned byte for byte | `crates/buildl-cli/tests/cli/arguments.rs:79-262` |
| `output.rs`'s `Shop`: `//:app` → `//:main.o`, `//lib:text`; `//:docs` alone; alias `//:default` → `//:app`; `shop.run(args)` | `crates/buildl-cli/tests/cli/output.rs:15-24`, `:82-84` |
| `exit_codes.rs`'s `failed(root, args)` asserts empty stdout and returns status and stderr | `crates/buildl-cli/tests/cli/exit_codes.rs:17-21` |
| `the_workspace_is_found_from_a_subdirectory` has a workspace whose only target is `//lib:words` | `arguments.rs:384-409` |
| `cargo test` takes one test-name filter | cargo 1.99 refuses a second (`error: unexpected argument … found`) |

## Files that change

```
crates/buildl-cli/src/exit.rs               — [modify] InvalidQuery is a usage error; its test row
crates/buildl-cli/src/cli.rs                — [modify] Command::Query and QueryArgs; module doc
crates/buildl-cli/src/run.rs                — [modify] the query command function; unit tests
crates/buildl-cli/tests/cli/arguments.rs    — [modify] help, refusal, relative-target tests
crates/buildl-cli/tests/cli/output.rs       — [modify] what query prints, both functions, both forms
crates/buildl-cli/tests/cli/exit_codes.rs   — [modify] malformed and unknown-target statuses
crates/buildl-cli/src/main.rs               — [modify] the command table and the expression paragraph
```

## Order of work

### Task 1 — `Error::InvalidQuery` is a usage error

**Files:**
- Modify `crates/buildl-cli/src/exit.rs` — the first arm of `of_error`; `usage_errors()` and the
  counts in `every_error_variant_has_its_code`.

**Change:** `of_error`'s first arm also matches `Error::InvalidQuery { .. }` and maps it to
`Usage`.

**Proof:**
- `every_error_variant_has_its_code`:
  - `usage_errors()` gains an `InvalidQuery` holding `dep(//:app)`;
  - the row counts become `[3, 2, 1, 16, 3, 6]`;
  - the distinct-variant count becomes `31`;
  - every listed error gets its row's status.

  Written first, it fails with `InvalidQuery` getting `Internal`.

**Done when:** `cargo test -p buildl-cli --bin buildl exit::tests` passes and the Definition of
Done is green. Commit `feat(buildl-cli): a query in neither form is a usage error`.

### Task 2 — The `query` subcommand's argument surface

**Files:**
- Modify `crates/buildl-cli/src/cli.rs` — the variant, the args struct, the module doc.
- Modify `crates/buildl-cli/src/run.rs` — a placeholder match arm returning `Ok(Exit::Internal)`.
  It keeps `run`'s match exhaustive and is replaced in task 3.
- Modify `crates/buildl-cli/tests/cli/arguments.rs` — the pinned help texts.

**Change:**
- `Command` gains `Query(QueryArgs)` between `Graph` and `Plan`. Its doc line is the help
  summary: `Ask what a target needs, or what needs it`.
- `QueryArgs` is `#[derive(Debug, Args)]` and placed after `GraphFormat`. It has:
  - `expression: String`, with `value_name = "EXPR"` and the doc line `deps(TARGET) for what
    TARGET needs, or rdeps(TARGET) for what needs it; TARGET as //dir:name, :name or name`;
  - `json: bool`, a `--json` flag, with the doc line `Print the answer as canonical JSON`.

  Clap prints these doc lines as the help, so the wording is exact.
- `cli.rs`'s module doc names `QueryArgs` among the arguments of a command.

**Proof** (`tests/cli/arguments.rs`):
- `help_names_every_command_and_flag` and `no_command_prints_the_short_help_on_stderr` — the
  Commands list has `  query  Ask what a target needs, or what needs it`, between `graph` and
  `plan`. The column does not move, because `query` is five letters like `check`, `graph` and
  `build`.
- `each_command_has_its_help` — a new `query` case, in this order:
  - the summary line;
  - `Usage: buildl query [OPTIONS] <EXPR>`;
  - an `Arguments:` block with `  <EXPR>` and the expression doc line, indented ten spaces;
  - `Options:` with the shared colour block, then `      --json` /
    `          Print the answer as canonical JSON`;
  - the shared help and version block.

  These are laid out as the existing `graph` case is.
- `a_refused_command_line_says_why_and_how_to_get_help` — a new case: `query` with no argument is
  refused with `error: the following required arguments were not provided:`, then `  <EXPR>`, a
  blank line, `Usage: buildl query <EXPR>`, a blank line, `For more information, try '--help'.`

**Done when:** `cargo test -p buildl-cli --test cli arguments` passes and the Definition of Done
is green. Commit `feat(buildl-cli): the query command's arguments and help`.

### Task 3 — The `query` command function

**Files:**
- Modify `crates/buildl-cli/src/run.rs` — the function, the match arm, the imports, unit tests.

**Change:** The placeholder arm becomes `Command::Query(args) => query(&start, &args, stdout)`.
`fn query<O: Write>(start, args: &QueryArgs, stdout) -> Result<Exit, Stop>` sits after `graph`
and does five things:
1. Open the workspace.
2. Call `workspace.query_of(&args.expression)`, mapping any error to
   `Cli::command().error(ErrorKind::ValueValidation, format!("invalid value '{expression}' for '<EXPR>': {error}"))`,
   the `scope()` pattern.
3. Call `workspace.query(&asked)`.
4. Render `query_text(&answer)`, or `canonical::to_vec(&answer)` plus `\n` under `--json`.
5. Write through `product` and return `Exit::Success`.

The imports gain `query_text` and `QueryArgs`. The rustdoc says an empty answer is a success, and
that a malformed expression is the command line's mistake.

**Proof** (`run.rs` unit tests, over `copying()`):
- `query_writes_the_answer_to_stdout_in_either_form` — `query deps(//:copy)` gives exit
  `Success`, empty stdout and empty stderr. `query --json rdeps(copy)` gives `Success` and stdout
  `[]\n`. No `.buildl/` is made.
- `a_query_in_neither_form_is_a_usage_error` — `query dep(//:copy)` gives exit `Usage`, empty
  stdout, and this stderr: `error: invalid value 'dep(//:copy)' for '<EXPR>': query `dep(//:copy)`
  is not deps(<target>) or rdeps(<target>)`, a blank line, `Usage: buildl [OPTIONS] <COMMAND>`, a
  blank line, `For more information, try '--help'.`
- `a_query_for_an_undeclared_target_is_a_usage_error` — `query rdeps(//:cpy)` gives exit
  `Usage`, empty stdout, and stderr
  `error: target //:cpy was asked for but is not declared (did you mean //:copy?)` with a newline.

**Done when:** `cargo test -p buildl-cli --bin buildl run::tests` passes and the Definition of
Done is green. Commit `feat(buildl-cli): buildl query prints what a target needs or what needs it`.

### Task 4 — Binary tests: both functions, both forms, statuses, a relative target

**Files:**
- Modify `crates/buildl-cli/tests/cli/output.rs` — two tests after `graph_json_equals_the_graph_file`.
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs` — one test after `an_unknown_target_is_a_usage_error`.
- Modify `crates/buildl-cli/tests/cli/arguments.rs` — extend `the_workspace_is_found_from_a_subdirectory`.

**Change:** No production code changes. These tests run the real binary over behaviour that
tasks 1–3 built and drove red then green.

**Proof:**
- `output.rs`, `query_prints_one_label_a_line_for_deps_and_rdeps` — over `Shop`:
  - `deps(//:app)` and `deps(default)` print `//:main.o\n//lib:text\n`;
  - `rdeps(//lib:text)` prints `//:app\n`;
  - `rdeps(//:docs)` prints nothing;
  - each exits 0 with empty stderr;
  - neither `out/` nor `.buildl/` exists afterwards.
- `output.rs`, `query_json_is_the_canonical_array_and_one_newline` — over `Shop`:
  - `--json deps(//:app)` prints `["//:main.o","//lib:text"]` and a newline;
  - `--json rdeps(//:main.o)` prints `["//:app"]` and a newline;
  - `--json deps(//:docs)` prints `[]` and a newline;
  - each exits 0 with empty stderr.
- `exit_codes.rs`, `a_query_in_neither_form_or_for_an_unknown_target_is_a_usage_error` — in a
  workspace declaring `//:lib`:
  - `query dep(//:lib)` is `(2, <the invalid-value text as in task 3, with dep(//:lib)>)`;
  - `query deps(//:libs)` is `(2, "error: target //:libs was asked for but is not declared (did you mean //:lib?)\n")`.
- `arguments.rs`, `the_workspace_is_found_from_a_subdirectory`, extended:
  - from `lib/`, `query deps(words)` exits 0 with empty stdout, since `//lib:words` depends on
    nothing;
  - from the root, `query rdeps(//lib:words)` exits 0 with empty stdout;
  - from the root, `query deps(words)` exits 2 with stderr
    `error: target //:words was asked for but is not declared` and a newline, and no suggestion.
    `//lib:words` is 3 edits from `//:words`, over the limit of `8 / 3 = 2`
    (`crates/buildl-core/src/resolve/suggest.rs:24`).

**Done when:** each of these passes, run one filter at a time:

```
cargo test -p buildl-cli --test cli output::
cargo test -p buildl-cli --test cli exit_codes::
cargo test -p buildl-cli --test cli arguments::
```

The Definition of Done must also be green. Commit `test(buildl-cli): query through the binary`.

### Task 5 — The binary's command table

**Files:**
- Modify `crates/buildl-cli/src/main.rs` — the `# Commands` table and one paragraph.

**Change:**
- The `# Commands` table gains a row after `buildl graph`: `` `buildl query <EXPR> [--json]` ``,
  writing to stdout "the targets the expression reaches, one per line, or as canonical JSON", and
  to stderr "nothing".
- A paragraph goes after the one about how a target is written, before `# Exit status`. It says:
  - an expression is `deps(TARGET)`, every target TARGET depends on, or `rdeps(TARGET)`, every
    target that depends on TARGET;
  - either reaches any number of steps away, and TARGET itself is not in the answer;
  - `query`, like `check` and `graph`, takes no lock and writes nothing.

**Proof:** documentation only. The rustdoc build under `-D warnings` is the check.

**Done when:** `RUSTDOCFLAGS="-D warnings" cargo doc -p buildl-cli --no-deps` passes, the
Definition of Done is green, and `cargo deny check` is green. Commit
`docs(buildl-cli): the query command in the binary's documentation`.

---

## Risks

- **Exact help text.** The pinned help in task 2 is what clap renders from the two doc lines. It
  was observed on a throwaway build of this argument surface before plans were code-free
  (`buildl --help`, `buildl query --help`, `buildl query`, `buildl query 'dep(x)'`). That build
  was restored, and nothing of it remains. Run the task 2 tests first, before any code change; if
  clap renders differently, the binary is the authority. Record a deviation and pin what it
  prints.
- **The refusal's usage line** is `Usage: buildl [OPTIONS] <COMMAND>`, not the subcommand's usage,
  because `Cli::command().error(…)` is raised on the top-level command. `plan` and `build`
  already behave this way (`run.rs` test `a_target_that_fails_its_grammar_is_a_usage_error`).
  This keeps the two paths alike.
- **Order of failure.** A malformed expression in a directory with no workspace fails on the
  missing `buildl.toml` first (exit 5), because the workspace is opened before the expression is
  read. That matches `plan` and `build`. No test depends on the other order.
- **The placeholder arm in task 2** returns `Internal` for one commit. Task 3 removes it, and no
  release is cut in between.

## Proof

- `cargo make dod` and `cargo deny check` green.
- `cargo test -p buildl-cli` green.
- On a copy of `crates/buildl/tests/fixtures/chain`, `./target/debug/buildl query "deps(//:report)"`
  prints `//:greet`, `//:greeting`, `//lib:words`, one per line, and exits 0.

## Review findings

- unit-test-mandate / reversion guard — no test covered a `query` whose target fails the label grammar (spec §2: exit 2); narrowing the `map_err` in `query` to `InvalidQuery` alone would have sent `InvalidName` to `Internal` (exit 7) with no test failing — `crates/buildl-cli/src/run.rs` — fixed: new test `a_query_whose_target_fails_its_grammar_is_a_usage_error` (`query deps(//:)` → `Exit::Usage`, the clap usage block, `invalid target name: "" — must not be empty`); `cargo test -p buildl-cli --bin buildl run::tests` → `test result: ok. 23 passed; 0 failed`
- doc-comment-discipline — `query`'s rustdoc named only the malformed-function case as the command line's mistake — `crates/buildl-cli/src/run.rs:190` — fixed: it now also names a target that fails the label grammar; `cargo fmt --all -- --check` clean

## Probe results

No throwaway probe was needed: each behavioural claim sits under a test the plan names, and each red-green run was the probe.

- Pinned help text (Risks) — the task 2 `arguments` tests passed against what clap renders, byte for byte. Holds.
- The refusal's usage line is the top-level `Usage: buildl [OPTIONS] <COMMAND>` (Risks) — `a_query_in_neither_form_is_a_usage_error` passes. Holds.
- Proof — on a copy of the `chain` fixture, `buildl query "deps(//:report)"` printed `//:greet`, `//:greeting`, `//lib:words`, one per line, exit 0, and made no `.buildl/` or `out/`. Holds.
- Full gate after task 5 (reviewer's independent run): `cargo make dod` → exit 0 (fmt, the four guards, clippy, rustdoc, tests, doctests); `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`.

## Deviations

- 2026-10-15 — Review fix: the grammar-failure test uses `deps(//:)`, not a target with a space; `Query::parse` refuses a space in the expression as `InvalidQuery` before the target is read, so `deps(a b)` never reaches the label grammar.
- 2026-10-15 — Commits: the plan asks for one commit per task. Nothing was committed during execution; committing is left to the author.
