---
status: approved
created: 2026-10-15
built-on: 82154c24d89d
---

# Spec: test — run the test targets, cached by key

`b.test` declares a target with `TargetRole::Test`, and nothing after Resolve reads the role. This
spec gives the role its meaning: a test declares no outputs unless it says so; `buildl test`
selects the test targets, builds what they need and runs them, cached by key; `build` and `plan`
with no targets leave tests out; `--rerun` runs an unchanged test again, and a failure under the
recorded key forgets the pass it contradicts; a test run reports in its own words. buildl stays a
runner of runners: a test passes when its command exits zero, and its output is never parsed.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-15 at commit `95a2b0a`.

|#|Premise|Evidence|
|---|---|---|
|P1|A test's outputs default to its name, as a build's do.|`crates/buildl-core/src/load/traversal.rs:205-221` (`convert_target`): `None => vec![OutputName::parse(label.name()…)]`, no role check.|
|P2|So a passing test that writes no file fails.|Probe, scratch workspace (`proc.run = ["cp", "cmp"]`): `b.test("default_outputs", { deps = { "copy" }, inputs = { "expected.txt" }, run = { "cmp", "$deps", "$in" } })` with equal files → `FAILED //:default_outputs: action failure (declared output default_outputs was not produced)`, `1 failed, 1 built`, exit 1.|
|P3|`outputs = {}` is accepted and such a test passes and is cached.|Same probe with `outputs = {}`: `1 built, 1 cached`, exit 0; again: `2 cached`, exit 0. With `expected.txt` changed: block `out/copy expected.txt differ: char 1, line 1`, `FAILED //:no_outputs: action failure (exit status 1)`, exit 1; its `log.jsonl` line holds `"outputs":[]` and both digests.|
|P4|`build` with no targets runs tests.|Probe: `buildl build` in the same workspace → `FAILED //:default_outputs …`, `1 failed, 1 skipped, 1 cached`, exit 1.|
|P5|Narrowing returns the whole graph for `Scope::Workspace`, and otherwise the closure of the named labels, aliases followed.|`crates/buildl-core/src/resolve/narrow.rs:31-41` (`let Scope::Targets(labels) = scope else { return Ok(Cow::Borrowed(graph)) }`; `graph.id_of(label)`; `closure(graph, starts, TargetGraph::deps)`). Callers: `pipeline/driver.rs:124` (`plan`), `:159` (`build`).|
|P6|`Scope` is `Workspace \| Targets(BTreeSet<Label>)`.|`crates/buildl-core/src/types/scope.rs:20-27`.|
|P7|A verdict is decided at Plan, and a row is checked again at Execute once dependencies finish; both read `freshness`.|`plan/verdict.rs:52` `decide(freshness, own, deps, row)`, `:86-95` (`Always` → `DirtyReason::Always`; no row → `NeverBuilt`), called at `:145` with `node.freshness`. `execute/settle.rs:57` `stands(freshness, key, row)`, used at `:136`; `settle` called from `execute/assembly.rs:404-411` with `target.node.freshness`.|
|P8|`DirtyReason` is `Always \| NeverBuilt \| Changed(Vec<Difference>)`, serialized in `plan --json`.|`types/plan.rs:57-64`; `Serialize` derive at `:56`; plan view at `:225-249`.|
|P9|The action cache can read and record a row, and cannot remove one.|`ports/action_cache.rs`: the trait's only methods are `row` and `record` (method listing of the file; `record` found by the same listing, the control).|
|P10|A journal line is `{label, row}`, read strictly.|`crates/buildl/src/cache/journal.rs:21-28`: `#[serde(deny_unknown_fields)] struct Entry { label: Label, row: CacheRow }`; design §3 (`docs/design.md:84`): a cache line not byte for byte what buildl writes is refused, exit 6.|
|P11|`ActionOutcome` is not serialized; the log writes an explicit view of twelve keys.|`types/outcome.rs:111-127`: `#[derive(Debug, Clone, PartialEq, Eq)]`, no `Serialize`. `types/outcome_event.rs:46-59` `EventView` lists `at, class, command, diagnostic, file, finished, key, label, outputs, started, stderr, stdout`; test `an_event_holds_exactly_the_twelve_keys_of_the_log_format` (`:243`).|
|P12|Outcomes are built in `execute/record.rs` helpers.|`record.rs:55` `built`, `:61` `cached`, `:68` `failed`, `:86` `unstored`, `:97` `unplaced`, `:109` `mismatched`, `:119` `skipped`, `:289` `succeeded` (grep `ActionOutcome {` / `-> ActionOutcome`).|
|P13|`Summary` is four counts over every outcome, made once per build.|`types/outcome.rs:131-141` (`failed, skipped, built, cached`), `Summary::of` `:146`; made at `execute/assembly.rs:598`; printed by `crates/buildl/src/report/block.rs:58-75` (`summary_line`) via `report/terminal.rs:216`. Every reader is counted in §3.6.|
|P14|The reporter learns of each target through `finished(event, printed)`, and the terminal reporter prints a block per finished target, replaying a cached target's output.|`ports/reporter.rs:38-62`; `report/terminal.rs:195-212`; `report/block.rs:174-187` (header and output only when something was printed, `(cached)` for a cached one, then the trailer); test `a_cached_build_replays_what_was_printed_byte_for_byte` (`terminal.rs:469`).|
|P15|The CLI's `build` arguments and their mapping.|`crates/buildl-cli/src/cli.rs:52-63` (`Command`: `Check, Graph, Query, Plan, Build`), `:98-105` `Selection` (`targets`, `--set`), `:119-147` `BuildArgs` (`-k/--keep-going` → `OnFailure`, `--stream` → `LiveOutput`); `run.rs:232-252` `build`; `exit.rs:79-80` (`UnknownTarget`, `InvalidQuery` → `Usage`).|
|P16|`Workspace::build` takes the lock, builds, folds the cache and joins the two results.|`crates/buildl/src/workspace.rs:155-172`.|
|P17|No file outside the chains holds the milestone 2 ladder.|`grep -rln 'I10\|I11' --include='*.md' .` minus chain directories → nothing; control, the same search without the exclusion → the query, test and log chain files.|

## 2. The command

```text
buildl test [TARGET]... [--set NAME=VALUE]... [--stop] [--stream] [--rerun] [--verbose]
```

|Aspect|Behaviour|
|---|---|
|Selection|No targets: every target declared with `b.test`, with what each needs. Targets named: each must name a test, directly or through an alias, else exit 2 with `Error::NotATest` (§3.3). A label nothing declares is exit 2 as today, with the nearest label.|
|Pipeline|Load, Resolve, the whole graph recorded to `graph.json`, narrow, Plan, Execute, Record — `build`'s pipeline with `build`'s lock.|
|Failure policy|Keeps going by default; `--stop` schedules nothing new after the first failure. A failed dependency skips the tests above it either way.|
|Caching|A passing test is cached by its key; a failing one never is; `always = true` runs one every time. `--rerun` runs the selected tests whatever their keys, and never forces a dependency.|
|Printing (stderr)|A failing or skipped test's block; every dependency's block exactly as `build` prints it; a passing test prints nothing unless `--verbose`. Under `--stream` the running action is shown live, a passing test included, and gets no further block.|
|Summary (stderr)|`tests: 3 passed (2 cached), 1 failed, 1 skipped`; zero counts left out; `no tests` when nothing was selected; `tests (rerun): …` under `--rerun`.|
|stdout|Nothing.|
|Exit|As `build` (design §8.7): a failing test is exit 1; no failure is 0, `no tests` included.|
|Not accepted|`-k`/`--keep-going` (keeping going is the default): a clap usage error, exit 2.|

`build` and `plan` change in one respect: with no targets they take every target but tests (and
the tests a taken target depends on). Named targets are taken as today, tests included. `plan`
follows `build` because design §7 requires it: "`plan` takes exactly what `build` takes, because
it is that build's dry run" (`docs/design.md:216`).

## 3. Components

### 3.1 Load — a test's default outputs

`load/traversal.rs` `convert_target` (P1): `outputs: None` gives `[name]` for `TargetRole::Build`
and `[]` for `TargetRole::Test`. A declared list is kept for either role, so a test that declares
`outputs = { "junit.xml" }` is held to it like any target (P3's output checks apply unchanged).

### 3.2 `types::Purpose` (new file `types/purpose.rs`)

```rust
/// What a run is for, which decides the targets a whole workspace means.
pub enum Purpose { Build, Test }
```

An enum and not a `bool` (rust guideline, strong types: no bool parameters for semantic flags).

### 3.3 Resolve — `narrow(graph, scope, purpose)`

|`purpose`|`Scope::Workspace`|`Scope::Targets(labels)`|
|---|---|---|
|`Build`|every `Build`-role target and its closure|the named targets and their closure (P5, unchanged)|
|`Test`|every `Test`-role target and its closure|each label must resolve (P5) to a `Test` node, else `Error::NotATest`; then the closure|

`Build` over the whole workspace now computes a closure rather than borrowing the graph (P5), so a
test that a build target depends on is still taken. An empty start set gives an empty graph.

`Error::NotATest { asked, target, provenance }` (new, `error.rs`), one text in every place that
quotes it:

```text
target <asked> was asked for as a test, but <target> is declared with b.target in <file>
```

`asked` is the label as given on the command line, `target` the target it resolved to (the same
label unless an alias was followed), `file` that target's build file. It has no provenance
prefix, like its neighbour `UnknownTarget` (`error.rs:286-289`, `target {label} was asked for but
is not declared…`), since both are about the command line. Example:
`target //:default was asked for as a test, but //:app is declared with b.target in build.lua`.

### 3.4 Plan and Execute — `Rerun` and forcing

`types::Rerun { Off, Tests }` (new file `types/rerun.rs`) is an invocation value, never a key
component.

Both decision sites of P7 take one value computed per target from `(freshness, role, rerun)`:

|`freshness`|`role`|`rerun`|Plan (`decide`)|Execute (`stands`)|
|---|---|---|---|---|
|`Always`|any|any|`Dirty { Always }`, as today|false, as today|
|`Cached`|`Test`|`Tests`|`Dirty { Rerun }`, never `Conditional`|false|
|`Cached`|any other|any other|as today|as today|

The value is an enum, `Forcing { None, Always, Rerun }` in `types/forcing.rs` (`pub(crate)`),
replacing the `freshness` argument of `decide` and `stands`. It lives in `types` because
`guard-module-edges` (`Makefile.toml:151-207`, `crates/expected-module-edges.txt:2-4,16-18`) lets
`plan` and `execute` reach only `error`, `ports` and `types`. Precedence is `Always`, then
`Rerun`, then the existing rules: a rerun test is `Dirty { Rerun }` whether or not it has a row or
its key changed. `DirtyReason` gains `Rerun`, serialized `"Rerun"` (P8). `plan` never passes
`Rerun::Tests`, so the reason `"Rerun"` never appears in `plan --json` (the targets `plan` lists
do change, §2).

`pipeline::plan(graph, overrides, ports)` and `execute(…)` take `rerun: Rerun`; `Pipeline::plan`
and `Pipeline::build` pass `Rerun::Off`.

### 3.5 The cache forgets a contradicted pass

`ActionCache` gains (P9):

```rust
/// Removes the row of the target `label` names, durably; no row is no change.
fn forget(&mut self, label: &Label) -> Result<()>;
```

**When.** In `Assembly::record` (`execute/assembly.rs:441`), the one place a performed action's
outcome is made — `Ended::Failed` through `failed`, `Ended::Produced` through `store` (which can
end as an output missing or refused, or unstored) — when that outcome's class is a failure and
the target's current row has the **same key digest** as `performed.key`, the row is forgotten.
Three cases reach it: `--rerun`; `always = true`; and a row that stood by key but could not be
restored (`execute/settle.rs:136-145`, `restorable` → `None` → `Settled::Runs(key)`), whose pass
cannot be served anyway. A failure after a key change keeps the old row, so reverting an edit is
still a hit. A target that never ran — `unplaced` (`record.rs:97`), a cached target whose restore
failed, or a skipped one — forgets nothing.

**Its failure.** A `forget` that returns `Err` turns the outcome into an infrastructure failure of
the store, as a failed `record` does in `store` (`record.rs:181-185`): exit 3, which outranks the
action failure (design §8.7).

**Ordering.** Design §8.5's invariant gains a clause: a forget is durable before the target's log
line is appended, as a row is.

`LocalCache` (P10): `Entry.row` becomes `Option<CacheRow>`, and a forget appends
`{"label":"//:t","row":null}` before removing the row in memory. Replaying the journal applies
`null` as a removal; `cache.json`'s format is unchanged (a forgotten label is absent after a fold);
a forget line is read under the same strict rule as a row line. Forgetting a label with no row
appends nothing. Every `ActionCache` implementation gains `forget` — five, by
`grep -rn 'impl ActionCache for' --include='*.rs' crates`: `buildl/src/cache/local_cache.rs:121`
(real), `buildl-core/tests/flows/common.rs:286` (`FakeCache`), `buildl-core/src/ports/bundle.rs:109`
(doc example), `buildl-core/src/ports/port_set.rs:176` (`Stub`), `buildl/src/report/plan_text.rs:201`
(`Rows`).

### 3.6 Outcomes and the summary

`ActionOutcome` gains `role: TargetRole`, set from the node by every helper of P12. It is not
written to the log: `EventView` (P11) is unchanged, so `log.jsonl` is byte-identical to today's.

`Summary` becomes `{ all: Tally, tests: Tally }` with `Tally { failed, skipped, built, cached }`
— one type for "counts of outcomes" (rust guideline, modularity: one canonical type per concept).
`Summary::of` fills `all` from every outcome and `tests` from `Test`-role ones. Every reader moves
to `summary.all`: `grep -rn 'Summary\b' --include='*.rs' crates` outside `types/outcome.rs` gives
76 lines in 14 files — `buildl-core`: `lib.rs`, `types/mod.rs`, `execute/assembly.rs`,
`ports/reporter.rs`, `ports/bundle.rs`, `ports/port_set.rs`, `tests/flows/{report,common,build}.rs`;
`buildl`: `report/{silent,terminal,block}.rs`, `workspace.rs`, `tests/e2e/failure.rs`.

The `role` field reaches every `ActionOutcome` literal: `grep -rn 'ActionOutcome {' --include='*.rs'
crates` (less the definition) gives 36 lines in 11 files — `buildl-core`: `types/outcome.rs`,
`types/outcome_event.rs`, `execute/record.rs`, `ports/reporter.rs`, `tests/flows/{build,common}.rs`;
`buildl`: `log/local_log.rs`, `report/{terminal,silent,block}.rs`; `buildl-cli`: `src/exit.rs`.

### 3.7 The reporter

`TerminalReporter::new(out, mode, paint, report)` with:

```rust
pub enum Report { Build, Test { blocks: TestBlocks, rerun: Rerun } }
pub enum TestBlocks { Failed, Every }
```

|`report`|Target|Block|
|---|---|---|
|`Build`|any|as today (P14)|
|`Test`|`Build` role (a dependency)|as `Build`|
|`Test`|`Test` role, failed or skipped|as `Build`|
|`Test { blocks: Failed }`|`Test` role, built or cached|none; the status line still advances|
|`Test { blocks: Every }`|`Test` role, built or cached|as `Build`, `(cached)` replay included|

The summary line is `summary_line(summary.all)` for `Build` and a new `test_line(summary.tests,
rerun)` for `Test` (§2 wording). `silent.rs` reports nothing and only follows the `Summary` change.
The reporter port's methods (P14) keep their signatures; `summary` takes the reshaped `Summary`.

### 3.8 Entries

- `Pipeline::test(entry, scope, overrides, on_failure, live, rerun) -> Result<Vec<ActionOutcome>>`
  in `pipeline/driver.rs`, beside `build`: load, resolve, record the whole graph, `narrow(…,
  Purpose::Test)`, plan and execute with `rerun`. `Pipeline::build` and `Pipeline::plan` keep their
  signatures and pass `Purpose::Build`, `Rerun::Off`. A free `pipeline::test` is not added: unlike
  `check`, `graph` and `query`, it needs every port.
- `Workspace::test(scope, overrides, on_failure, live, rerun, reporter)` in `workspace.rs`, the
  same lock, fold and `concluded` as `Workspace::build` (P16).
- `buildl-cli`: `Command::Test(TestArgs)` after `Build`; `TestArgs { selection: Selection, stop,
  stream, rerun, verbose }`, mapped to `OnFailure` (`KeepGoing` unless `--stop`), `LiveOutput`,
  `Rerun`, `TestBlocks`; `run.rs` `test` as `build` with `Report::Test`; `exit.rs` maps
  `Error::NotATest` to `Usage` beside `UnknownTarget` (P15); `main.rs` gains the command-table row.
  `Selection`'s help text (`cli.rs:96-100`: "What `plan` and `build` share", "every target when
  none is given") and `main.rs:19` ("naming none means every target") are rewritten: `test` shares
  it, and with none given `plan`/`build` take every target but tests and `test` every test.

## 4. Documentation

|File|Change|
|---|---|
|`docs/design.md` §3 (`:75`)|`cache.journal` holds a row, or the forgetting of one.|
|`docs/design.md` §5|The `b.test` bullet: a test declares no outputs by default; `outputs` overrides.|
|`docs/design.md` §7 (`:211`, `:216`)|The `test` row as built, its flags; `plan` and `build` with no targets take every target but tests; `test` among the built commands.|
|`docs/design.md` §8.4 (`:282`)|Stopping is `build`'s default; `test` keeps going by default and takes `--stop`.|
|`docs/design.md` §8.5 (`:284-308`)|The invariant's new clause: a forget is durable before the log line, as a row is.|
|`docs/design.md` §8.7 (`:326-366`)|What `test` prints and sums up, `--verbose`, `--stream` on a passing test; a failure under the recorded key forgets the row.|
|`docs/design.md` §9.1 (`:385`)|A failed rerun forgets the pass.|
|`docs/architecture.md` (`:3`, `:176-182`, `:231`, `:284`, `:376`)|`test` in the status line; `Purpose`, `Rerun`, `DirtyReason::Rerun`; `ActionCache::forget`; `Summary { all, tests }`; the journal's forget line.|
|`docs/architecture-building-blocks.md` (`:179`, `:411`, `:418`, `:434`, `:506`, `:532`)|`narrow`'s argument; `Pipeline::test`; the `ActionCache` port row gains `forget`; summary examples.|
|`README.md` (`:124`, `:128`)|`buildl test` available; its flags.|
|`CLAUDE.md` Status (`:7`)|Milestone 2: `query` and `test` built.|

No roadmap file changes (P17). Mermaid in `docs/` stays Mermaid; section numbers are not changed.
Line numbers are re-checked when the plan is written.

## 5. Determinism and rules

- No `HashMap` reaches any output; the narrowed graph, the summary and the reporter's order are
  label- and id-ordered as today (architecture §5).
- JSON — the journal's forget line, `"Rerun"` — goes only through `canonical`.
- `buildl-core` gains no I/O (`guard-core-purity`) and no dependency (`guard-crate-edges`); phase
  modules do not import each other (`guard-module-edges`, `crates/expected-module-edges.txt`
  unchanged): `Forcing`, `Purpose` and `Rerun` live in `types`.
  In `buildl`, `report` and `cache` name only `state` and `search_path` (`guard-adapter-isolation`).
- The action key's components are unchanged; `Rerun` is not one of them.

## 6. Testing

|Layer|File|Tests|
|---|---|---|
|unit|`load/traversal.rs`|`a_test_declares_no_outputs_by_default` · `a_test_keeps_the_outputs_it_declares` · `outputs_default_to_the_target_name` (existing)|
|unit|`resolve/narrow.rs`|`a_build_of_the_workspace_leaves_tests_out` · `a_build_of_the_workspace_keeps_a_test_a_target_depends_on` · `a_build_of_a_named_test_includes_it` · `a_test_run_of_the_workspace_takes_every_test_and_its_deps` · `a_test_run_of_named_tests_takes_them_and_their_deps` · `a_test_run_follows_an_alias_to_a_test` · `a_test_run_naming_a_build_target_is_not_a_test` · `a_test_run_of_a_workspace_without_tests_is_empty`|
|unit|`error.rs`|`not_a_test_names_the_target_and_its_file`|
|unit|`plan/verdict.rs`|`a_rerun_test_with_an_unchanged_key_is_dirty_for_a_rerun` · `a_rerun_test_waiting_on_a_dep_is_dirty_not_conditional` · `a_rerun_leaves_a_clean_build_target_clean` · `a_test_declared_always_keeps_always_under_a_rerun`|
|unit|`execute/settle.rs`|`a_rerun_test_row_does_not_stand` · `a_rerun_build_target_row_stands`|
|unit|`types/plan.rs`|`a_rerun_reason_serializes_as_rerun`|
|unit|`execute/assembly.rs`|`a_failure_under_the_recorded_key_forgets_the_row` · `a_failure_under_another_key_keeps_the_row` · `an_output_missing_under_the_recorded_key_forgets_the_row` · `an_unplaced_cached_target_forgets_nothing` · `a_forget_that_fails_is_a_store_failure`|
|unit|`types/outcome.rs`|`a_summary_tallies_tests_apart_from_every_target`|
|unit|`types/outcome_event.rs`|`an_event_holds_exactly_the_twelve_keys_of_the_log_format` (existing, stays green: `role` stays out of the log)|
|unit|`report/block.rs`|`a_test_line_counts_passes_with_their_cached_share` · `a_test_line_leaves_zero_counts_out` · `a_run_without_tests_says_no_tests` · `a_rerun_line_says_rerun`|
|unit|`report/terminal.rs`|`a_test_run_hides_a_passing_test_that_printed` · `a_test_run_hides_the_replay_of_a_cached_pass` · `a_test_run_shows_the_block_of_a_failing_test` · `a_test_run_shows_a_dependency_as_a_build_does` · `a_verbose_test_run_shows_every_test` · `a_test_run_ends_with_the_test_line` · `a_rerun_test_run_ends_with_the_rerun_line` · `a_live_test_run_still_advances_the_status_line_past_a_hidden_pass`; existing build tests under `Report::Build`|
|adapter|`cache/journal.rs`|`a_forget_line_is_canonical` · `a_forget_line_out_of_form_is_corrupt`|
|adapter|`cache/local_cache.rs`|`a_forgotten_row_is_gone_after_fold` · `a_forgotten_row_is_gone_after_reopen_from_the_journal` · `forgetting_a_label_with_no_row_writes_nothing`|
|core flow|`tests/flows/build.rs`|`a_plan_of_the_workspace_leaves_tests_out` · `a_test_with_no_outputs_that_exits_zero_is_built` (with a fake strategy that writes nothing — the existing flow at `build.rs:808` writes every output, which hid P2)|
|core flow|`tests/flows/test.rs` (new)|`a_test_run_records_the_whole_graph_and_runs_only_tests` · `a_test_run_keeps_going_past_a_failing_test` · `a_stopped_test_run_starts_nothing_new_after_a_failure` · `a_failed_dependency_skips_the_tests_above_it` · `a_rerun_runs_a_cached_test_again_and_records_it` · `a_rerun_does_not_rebuild_a_cached_dependency` · `a_failed_rerun_forgets_the_row_so_the_next_run_runs_it` · `a_test_run_returns_each_outcome_with_its_role`|
|e2e|`crates/buildl/tests/e2e/test.rs` (new), fixture `crates/buildl/tests/fixtures/tested/` (new: a `cp` target; a passing `cmp` test without outputs; a failing one; one declaring `outputs = { "report.txt" }`)|`a_passing_test_with_no_outputs_passes_and_is_cached_next_run` · `a_failing_test_is_never_cached` · `a_test_that_declares_an_output_is_held_to_it` · `a_build_of_the_workspace_runs_no_test` · `a_failed_rerun_runs_again_next_time`|
|CLI|`crates/buildl-cli/tests/cli/output.rs`|`test_prints_only_failing_tests_and_the_test_line` · `test_verbose_prints_every_test` · `test_rerun_line`|
|CLI|`crates/buildl-cli/tests/cli/exit_codes.rs`|`test_with_a_failing_test_exits_1` · `test_naming_a_build_target_exits_2` · `test_with_no_tests_exits_0`|
|CLI|`crates/buildl-cli/tests/cli/arguments.rs`|`test_rejects_keep_going` · `test_accepts_stop_stream_rerun_verbose`|
|unit|`crates/buildl-cli/src/exit.rs`|`not_a_test_is_a_usage_error`|

Tools in tests are system tools by argv (`cp`, `cmp`, `false`, `sh -c` in tests only), per
architecture-building-blocks §8.

## 7. Risks

- **The journal format changes.** A `cache.journal` written by this version holds a line an older
  buildl refuses (exit 6). A journal exists only while a build runs or after one died (design §3),
  so only a downgrade across a crashed build meets it; the remedy is design §3's (remove it, build
  again).
- **`Summary`'s and `ActionOutcome`'s shapes change for every reader** (§3.6: 14 and 11 files);
  the plan orders those changes before the reporter's.

## 8. Non-goals

- Reading a runner's output: per-case counts, JUnit parsing, filters beyond build settings.
- `buildl log` (I12, the next chain on this branch).
- Directory or pattern selection; `-k` on `test`; `--rerun` on `build` or `plan`; forcing
  dependencies (I13's `clean`).
- Sharing test helpers across build files (milestone 05).
- An input sandbox (milestone 03).
