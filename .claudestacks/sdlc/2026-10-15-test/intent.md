---
status: approved
created: 2026-10-15
milestone: buildl/02
---

# Intent: I11 test — run the test targets, cached by key

## Problem

`b.test` exists end to end as a declaration: `buildl-lua` stages it with `TargetRole::Test`, and
the role reaches `Target`, `TargetGraph` and `graph.json`. Nothing after Resolve reads it, so a
test is built exactly like any other target, and the command that runs tests does not exist.

- **A passing test fails.** `outputs` defaults to the target name for a test as for a build
  (`load/traversal.rs`), so a test that writes no file ends with "declared output … was not
  produced", exit 1. Shown on the real binary: `b.test("t", { deps = { "copy" }, inputs =
  { "expected.txt" }, run = { "cmp", "$deps", "$in" } })` fails although `cmp` exits 0; the same
  test with `outputs = {}` passes and is cached on the next run. Design §5's own example,
  `b.test("app_test", { deps = { "app" }, run = { "$deps", "--self-test" } })`, fails the same
  way. The core flow test that covers a test target hides it, because its fake strategy produces
  every output.
- **`buildl test` is promised and absent.** Design §7 lists `buildl test` ("run test targets");
  README marks it "not yet"; `buildl-cli` has no `test` subcommand.
- **`buildl build` runs the test suite.** With no targets it takes every target, tests included.
- **A test's result cannot be asked for on its own terms.** The summary counts every target
  (`1 failed, 3 built, 6 cached`), so how many tests passed is not visible; a passing test that
  prints (`go test`'s `ok …`) prints a block on every run, replayed even from the cache; and a
  cached pass cannot be rerun without editing the build file.

Second slice of milestone 2 (design §13 item 2): I10 query → **I11 test** → I12 log → I13 clean
+ gc → I14 parallel load → I15 demand-driven load. It shares a branch and one pull request with
I12, executed first.

Assumed, and overridable:

- **buildl is not a test framework.** A test is a target whose command is a test runner; its exit
  status is pass or fail. buildl does not parse a runner's output or count test cases.
- **One chain.** The role's meaning, the command and its reporting cannot be shown to work apart.

## Affected systems

- `buildl-core` — the default outputs of a test target (Load); which targets a run selects;
  Plan, for a rerun of a test whose key has not changed; the counts a test run reports.
- `buildl` — the `Workspace` facade entry for `test`; the terminal reporter's summary and which
  blocks it prints.
- `buildl-cli` — the `test` subcommand, its flags, its exit codes; `build`'s selection with no
  targets.
- `docs/design.md` §5, §7, §8.7, §9.1; `README.md`'s command table; `docs/architecture.md` and
  `docs/architecture-building-blocks.md` where they describe commands, outcomes and the reporter;
  the roadmap's milestone 2 ladder.

## Desired outcome

|Topic|Outcome|
|---|---|
|Outputs|A test declares no outputs by default. Its record is its exit status and its captured stdout and stderr, stored as for every action. `outputs = { … }` on a test overrides the default and holds the test to those files like any target.|
|Selection|`buildl test` with no targets runs every test target, building what each needs first. `buildl test <target>...` runs the named ones; an alias is followed to its target. Naming a target that is not a test is a usage error, exit 2. No directory patterns.|
|`build`|`buildl build` with no targets builds every target except tests. A test named explicitly, `buildl build //api:unit`, is still built, and so run.|
|Caching|A passing test is cached by its key, so unchanged code skips its tests (design §9.1); a failing one is never cached; `always = true` runs one every time. `buildl test --rerun` runs the selected tests even when their keys are unchanged, never their dependencies; the run says why an unchanged test ran.|
|Failure policy|`buildl test` keeps going by default, so every failing test is reported; `--stop` stops scheduling new work at the first failure. A failed dependency still skips the tests above it.|
|Summary|One line about tests: `tests: 3 passed (2 cached), 1 failed, 1 skipped`. A failing dependency prints its own failure report.|
|Printing|Only a failing test prints its block; `--verbose` prints every test's block. What is stored does not change. `build` keeps its printing as it is.|
|Exit codes|A failing test is an action failure, exit 1; the codes of design §8.7 otherwise hold unchanged.|
|Tests|Core flows against fake ports, adapter and end-to-end tests through `Workspace`, and CLI tests of the binary cover each row above, including a test with no outputs passing.|
|Documents|Design §5, §7, §8.7 and §9.1, README and both architecture documents describe `test` as built; README marks it available.|

## Constraints

- `buildl test` is the full pipeline over a selection: Load, Resolve, Plan, Execute, Record, with
  `build`'s lock, `--set` and `--stream`.
- The action key is unchanged in what it folds. `--rerun` is not a key component: a rerun that
  passes records the same key again.
- The determinism rules of `architecture.md` §5 hold: no `HashMap` iteration in output, JSON only
  through the canonical serializer, parallelism never observable in what is recorded.
- The structural rules hold: `buildl-core` stays free of I/O, phase modules do not import each
  other, adapter modules of `buildl` do not import each other (`cargo make` guards).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch, never
  `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Reading a test runner's output: per-case counts, JUnit parsing, test-case filters beyond what a
  build setting already gives (`--set test_filter=…`, design §10.2).
- `buildl log` (I12), which reads a passing test's stored output back.
- Directory or pattern selection (`//services/api/...`) for any command.
- Sharing a test helper across build files, which needs plugins (milestone 05).
- An input sandbox: at tier 1 a test that reads an undeclared file can stay cached after it
  changes, as a build can; milestone 03 closes that.
- `--rerun` on `build`, or forcing dependencies to rebuild (I13's `clean`).
