---
status: done
created: 2026-11-10
depends-on: [01, 02]
built-on: 6643cfeb82a2
---

# Load Refusals Implementation Plan

**Goal:** Loading refuses an `inputs` entry or a `subdir` that the workspace's ignore rules report ignored.

**Architecture:** `Ports` gains `type Ignore: IgnoreRules`. `load` (`crates/buildl-core/src/load/traversal.rs:57`)
gains a `rules: &I` parameter, `I: IgnoreRules`, carried by `first_pass`/`second_pass`/`load_twice`
(`discovery/passes.rs:78,101,127`) and by every pipeline entry that loads: `Pipeline::declared`
(`pipeline/driver.rs:211`) from its `PortSet`, and the free functions `check` (`driver.rs:619`),
`graph` (`:671`), `query` (`:701`), `doctor` (`pipeline/doctor.rs:43`) and `gc` (`pipeline/gc.rs:68`),
which take per-port generics and gain one more. In the per-file evaluation (`load/evaluation.rs:50-70`)
the rules reach `convert_input` (`traversal.rs:186`, called at `:234`) and `resolve_subdirs`
(`traversal.rs:128`, called at `evaluation.rs:63`): after an input parses it is asked as a `File`;
after a subdir resolves it is asked as a `Folder`. `Some` becomes `Error::Ignored` with the same
`Site` fields `InvalidDeclaration` takes (`traversal.rs:104-112`); `Err` passes through. The
composition root's `LocalPorts` (`crates/buildl/src/local_ports.rs:56`) sets `type Ignore =
BuildlIgnore` and builds one instance per workspace open, kept on the wiring so plan 05 hands the
same instance to the walk. Test bundles and direct `load` callers that are not about ignoring pass
`NothingIgnored` (plan 01).

**Tech Stack:** Rust 1.94, `buildl-core` (no I/O, no threads in tests: `crates/buildl-core/clippy.toml`),
`buildl` composition, the CLI test harness.

**Content authority:** `spec.md` §3 (`Ports::Ignore`, load checks, composition rows), §4 (load lines),
§5 (all rows, load route), §6 (root build file and manifest always read; grammar first; no
`.buildlignore` anywhere), §8 (`buildl-core` flows, `buildl-cli` rows for the load route).

---

## Files that change

```
crates/buildl-core/src/ports/bundle.rs           — modify: type Ignore: IgnoreRules in Ports (:277-313); the doc-test bundle (:258) uses NothingIgnored
crates/buildl-core/src/ports/port_set.rs         — modify: PortSet holds and lends the Ignore port; its test bundle (:325)
crates/buildl-core/src/load/traversal.rs         — modify: load gains rules; convert_input, then resolve_subdirs ask it
crates/buildl-core/src/load/evaluation.rs        — modify: pass rules into the per-file evaluation (:50-70)
crates/buildl-core/src/discovery/passes.rs       — modify: first_pass, second_pass, load_twice carry rules (:78-127)
crates/buildl-core/src/pipeline/driver.rs        — modify: declared, check, graph, query take or lend rules
crates/buildl-core/src/pipeline/doctor.rs        — modify: doctor carries rules (:43-59)
crates/buildl-core/src/pipeline/gc.rs            — modify: gc carries rules (:68-86)
crates/buildl-core/tests/flows/common.rs         — modify: FakeIgnore and its slot in FakePorts (:1505-1555)
crates/buildl-core/tests/flows/load.rs           — modify: its direct load calls pass NothingIgnored (:28-581)
crates/buildl-core/tests/flows/ignore.rs         — create: the load flows of this plan
crates/buildl-core/tests/flows/main.rs           — modify: declare the ignore module
crates/buildl-lua/tests/flows/load.rs            — modify: its direct load call passes NothingIgnored (:68)
crates/buildl/src/local_ports.rs                 — modify: type Ignore = BuildlIgnore; one instance per open
crates/buildl/src/workspace.rs                   — modify: pass the instance to check, graph, query, doctor, gc (:254-579)
crates/buildl/src/report/plan_text.rs            — modify: test bundle Planning gains type Ignore = NothingIgnored (:382)
crates/buildl/tests/e2e/remote_gates.rs          — modify: test bundle Gated gains type Ignore = NothingIgnored (:379)
crates/buildl/tests/e2e/parallel_load.rs         — modify: direct load calls pass NothingIgnored (:98,:106,:123)
crates/buildl-cli/tests/cli/exit_codes.rs        — modify: exit code and message of each spec §5 row on the load route
```

Every other caller of the changed signatures the compiler names is changed in the same task, so the
workspace builds after each task.

## Order of work

### Task 1 — Carry the rules to the inputs and refuse an ignored one

**Files:**
- Modify `crates/buildl-core/src/ports/bundle.rs`, `ports/port_set.rs` — the associated type and its
  slot in `PortSet`
- Modify `crates/buildl-core/src/load/traversal.rs`, `load/evaluation.rs`, `discovery/passes.rs`,
  `pipeline/driver.rs`, `pipeline/doctor.rs`, `pipeline/gc.rs` — the `rules` parameter, down to
  `convert_input`
- Modify `crates/buildl-core/tests/flows/common.rs` — `FakeIgnore`: built with the paths it ignores
  (each with a file and pattern) or built to fail with an `IgnoreFile`; records every `(path, kind)`
  it is asked; `FakePorts` uses it, ignoring nothing unless a test says otherwise
- Modify `crates/buildl-core/tests/flows/load.rs`, `crates/buildl-lua/tests/flows/load.rs`,
  `crates/buildl/tests/e2e/parallel_load.rs`, `crates/buildl/src/report/plan_text.rs`,
  `crates/buildl/tests/e2e/remote_gates.rs` — `NothingIgnored`
- Modify `crates/buildl/src/local_ports.rs`, `crates/buildl/src/workspace.rs` — `type Ignore =
  BuildlIgnore`, one `BuildlIgnore::new(root)` per workspace open, passed where the pipeline asks
- Create `crates/buildl-core/tests/flows/ignore.rs`, modify `tests/flows/main.rs` — the flows

**Change:** after `SourcePath::parse` succeeds in `convert_input`, ask `rules.ignored(input, File)`;
`Some(ignoring)` is `Error::Ignored { path: Input(input), ignoring, … }`; `Err` is returned as it
is. The grammar check stays first, so a path it refuses never reaches the port. The root build file
is read without asking.

**Proof:**
- `an_ignored_input_is_refused` — a root build file whose target's inputs hold `notes/a.md`, with the
  fake ignoring `notes/a.md` by `.buildlignore` `notes/`, fails `check` with `Error::Ignored`
  naming the path, file and pattern.
- `an_input_in_a_subdirectory_is_asked_with_its_full_path` — `lib/build.lua` with input `x.txt`: the
  fake records `lib/x.txt` asked as `File`.
- `a_plugin_written_input_is_checked_like_any_other` — a target declared through a test plugin with
  an ignored input fails the same way.
- `a_failing_rule_set_fails_the_load` — the fake built to fail makes `check` return its `IgnoreFile`.
- `a_grammar_refusal_comes_first` — an input `../x` fails with `InvalidDeclaration`, and the fake
  records no question.
- `the_root_build_file_is_read_whatever_the_rules_say` — a fake that ignores `build.lua` leaves
  `check` succeeding.
- every existing test of the workspace passes unchanged (spec §6, no `.buildlignore` anywhere).

**Done when:** `cargo make dod`, `cargo deny check -D unused-wrapper` and `cargo make
guard-module-edges` pass. Commit `feat(buildl-core): refuse an ignored input`.

### Task 2 — Refuse an ignored subdir

**Files:**
- Modify `crates/buildl-core/src/load/traversal.rs`, `load/evaluation.rs` — the rules reach
  `resolve_subdirs`, which asks
- Modify `crates/buildl-core/tests/flows/ignore.rs` — the flows
- Modify `crates/buildl-core/tests/flows/common.rs` — the fake source reports which build files it
  evaluated, if it does not already

**Change:** each subdir `resolve_subdirs` resolves is asked `rules.ignored(subdir, Folder)`; `Some`
is `Error::Ignored { path: Subdir(subdir), … }`; `Err` is returned. The build file of an ignored
subdir is never evaluated.

**Proof:**
- `an_ignored_subdir_is_refused` — the root calls `subdir("examples")`, the fake ignores the folder
  `examples` by `examples/`: `check` fails with `Error::Ignored` for `Subdir`, and the fake source
  evaluated only the root build file.
- `a_subdir_that_is_not_ignored_loads` — the same with the fake ignoring nothing loads both files.

**Done when:** `cargo test -p buildl-core --all-targets --all-features` passes and `cargo make dod`
and `cargo deny check -D unused-wrapper` are green. Commit `feat(buildl-core): refuse an ignored subdir`.

### Task 3 — Show the load-route refusals on the command line

**Files:**
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs` — the cases, through the real binary

**Change:** none in source: Tasks 1-2 and plan 02 make these pass end to end.

**Proof:** each workspace has a root `build.lua` with one target whose `inputs` hold `src/a.c` (and
`notes/a.md` where named); each case runs `buildl check` and asserts exit code 5 and the stderr line,
with the provenance prefix the harness's other definition errors print:
- `an_ignored_input_exits_5` — `.buildlignore` `notes/`, input `notes/a.md`: stderr holds
  `build.lua: inputs: notes/a.md is ignored by .buildlignore (notes/)`.
- `an_ignored_subdir_exits_5` — `.buildlignore` `examples/`, the root calls `subdir("examples")`:
  stderr holds `subdir: examples is ignored by .buildlignore (examples/)`.
- `a_bad_pattern_exits_5` — `src/.buildlignore` `[z-a]`: stderr holds
  `src/.buildlignore:1: error parsing glob '[z-a]': invalid range; 'z' > 'a'`.
- `a_non_utf8_buildlignore_exits_5` — `src/.buildlignore` with invalid bytes on line 1: stderr holds
  `src/.buildlignore:1: not UTF-8`.
- `a_symlinked_buildlignore_exits_5` — `src/.buildlignore` a link to a real file: stderr holds
  `src/.buildlignore: a symlink, not a file`.
- `a_workspace_without_buildlignore_is_unchanged` — the same workspace without any file: exit 0.

**Done when:** `cargo test -p buildl-cli --all-targets --all-features` passes and `cargo make dod`
and `cargo deny check -D unused-wrapper` are green. Commit `test(buildl-cli): exit codes of ignored paths`.

---

## Risks

- Task 1 touches every pipeline entry and every `Ports` implementation at once; it is the riskiest
  step. The compiler names every site, and the whole existing suite is its regression test.
- `load` runs build files in parallel through the `Dispatcher`; the rule set is shared by reference,
  which `IgnoreRules: Sync` allows. `BuildlIgnore`'s cache is behind a `Mutex` (plan 02), so two
  build files asking at once only serialize on it.
- `buildl-core` tests may not start threads (`crates/buildl-core/clippy.toml`); `FakeIgnore` records
  its questions in a `Mutex`-guarded list, no thread.
- spec §8's `buildl` end-to-end row "an `inputs` entry under `notes/` exits 5" is proved here through
  the `buildl-cli` binary, which runs the same code path with the exit code as well.

## Proof

- `cargo make dod` and `cargo deny check -D unused-wrapper` green after each task;
  `cargo make guard-module-edges`, `cargo make guard-adapter-isolation` and `cargo make
  guard-core-purity` pass.

## Review findings

One reviewer pass over 98b2a72, 46369d6, 18e49ae: 0 blocking. All fixed in e076560 (`cargo make dod`, `cargo deny check -D unused-wrapper` green).

- 🟡 `graph`, `query`, `doctor`, `gc` in `workspace.rs` had no test with a `.buildlignore` — fixed: four tests each return `Error::Ignored`; with the call sites swapped to `&NothingIgnored` all four failed (Ok instead of Err), then restored.
- 🔵 no test that a reopened wiring sees a changed `.buildlignore` — fixed: test in `local_ports.rs`.
- 🔵 `an_ignored_subdir_exits_5` left out the provenance prefix — fixed: asserts `error: build.lua: subdir: examples is ignored by .buildlignore (examples/)`.
- 🔵 `FileResult::Failed` doc named only an ignored input — fixed: also an ignored subdir and an unusable ignore file.
- 🔵 ragged doc lines in `evaluation.rs`, `traversal.rs`, `bundle.rs`; doubled "and" in `common.rs` — fixed.

## Deviations

- 2026-11-10 — Task 1: `doctor` and `query` now take 8 arguments and carry `#[expect(clippy::too_many_arguments, reason = …)]`, as `gc` already did; `check_graph_query_and_gc_load_in_any_order` carries `#[expect(clippy::too_many_lines, reason = …)]`.
- 2026-11-10 — Task 1: `crates/buildl-core/src/load/loading.rs` also changed (a caller of `load`), not in the file list.
- 2026-11-10 — Task 1: `Wiring` exposes the instance as `Wiring::ignore()`; `reopened()` reads afresh, so each watch cycle gets a new one.
- 2026-11-10 — Task 2: `common.rs` needed no change (`FakeSource::evaluated()` existed); `a_subdir_that_is_not_ignored_loads` checks with `contains` because `check` loads twice. Added `a_failing_rule_set_fails_a_subdir_question`.
- 2026-11-10 — Tasks 2 and 3 ran in one coder with one gate run (disjoint files), committed separately.
