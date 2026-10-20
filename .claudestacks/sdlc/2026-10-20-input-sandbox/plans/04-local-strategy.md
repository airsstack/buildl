---
status: approved
created: 2026-10-20
depends-on: [01, 02, 03]
built-on: 5fb250597c90
---

# Local Strategy Implementation Plan

**Goal:** `build` and `test` run each action under the workspace's strategy, `bare` or `sandbox`.

**Architecture:** `exec/local.rs` holds `enum Local { Bare, Sandbox, Refused(Isolation) }`, the
one `ExecStrategy` the real ports name, delegating statically to its variant with
`LocalDir { Bare(()), Sandbox(ExecDir) }`. `Wiring::ports` builds the variant from the
manifest and never fails because of the strategy, so `plan` works under every strategy;
`Wiring::runs` accepts `bare` and `sandbox` and refuses the rest with the one refusal text that
`Local::Refused`'s `prepare` also returns.

**Tech Stack:** Rust (`buildl`, `buildl-cli`), `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2 rows Strategies run, Cache, Same results; §3.4; §3.5; §3.6; §5
rows `buildl` unit `exec/local.rs`, `buildl` e2e, `buildl-cli`.

---

## Files that change

```
crates/buildl/src/exec/local.rs              — [create] `Local`, `LocalDir`, the refusal, and their unit tests
crates/buildl/src/exec/mod.rs                — [modify] declare `local`; export `Local`, `LocalDir`
crates/buildl/src/local_ports.rs             — [modify] `Strategy = Local`; `runs()`; `ports()`; tests
crates/buildl/src/workspace.rs               — [modify] `build`/`test` docs name the strategies refused; two refusal tests use `os-sandbox`
crates/buildl-cli/src/run.rs                 — [modify] the one-error-line test uses `os-sandbox` (`:772`)
crates/buildl/src/lib.rs                     — [modify] crate docs: tier 2 runs, the default runs
crates/buildl/tests/fixtures/sandboxed/      — [create] fixture: buildl.toml, build.lua, a.txt, secret.txt, partial.sh, slow.sh
crates/buildl/tests/e2e/sandbox.rs           — [create] the end-to-end tests below
crates/buildl/tests/e2e/main.rs              — [modify] `mod sandbox;`
crates/buildl/tests/e2e/facade.rs            — [modify] the refusal test uses `os-sandbox` (`:71`)
crates/buildl-cli/tests/cli/exit_codes.rs    — [modify] the refusal test uses `os-sandbox` (`:522`); default builds
```

## Order of work

### Task 1 — Add `Local`

**Files:**
- Create `crates/buildl/src/exec/local.rs`
- Modify `crates/buildl/src/exec/mod.rs` — `mod local;`, `pub use local::{Local, LocalDir};`

**Change:**
- `pub enum Local { Bare(Bare), Sandbox(InputSandbox), Refused(Isolation) }` (`Debug`) and
  `pub enum LocalDir { Bare(()), Sandbox(ExecDir) }`; `impl ExecStrategy for Local` with
  `type Dir = LocalDir`, each method matching its variant and calling the inner strategy (static
  dispatch, rust guideline static-dispatch).
- the associated function `pub fn Local::refusal(strategy: Isolation) -> Error` (called as
  `Local::refusal` everywhere) — `Error::Adapter { port: PortName::Manifest }`
  with `"buildl.toml: [execution] strategy is \"{strategy}\", and this version of buildl runs
  \"bare\" and \"sandbox\" only: write strategy = \"sandbox\" under [execution]"`. The constant
  `MANIFEST_FILE` (`local_ports.rs:39`) moves here so the one text is written once.
- `Refused`'s `prepare` returns `Local::refusal(strategy)`; its `run` and `collect` return the same.
- A `LocalDir` of the other variant handed to `run` or `collect` is
  `fault(label, "running the action", "its directory was prepared by another strategy")`
  (`exec/outcome.rs`, plan 03), never a panic.

**Proof:** unit tests in `local.rs`:
- `bare_runs_in_the_workspace_and_sandbox_in_its_exec_dir` — `pwd` under each variant: the root,
  then a path under the scratch dir (both canonical, as plan 03's tests).
- `a_dir_of_the_other_variant_is_an_adapter_error` — `Local::Bare` handed `LocalDir::Sandbox`, and
  the reverse: `Error::Adapter { port: ExecStrategy }` from `run` and `collect`.
- `refused_prepares_nothing_and_returns_the_refusal` — `Local::Refused(OsSandbox).prepare` is
  `Error::Adapter { port: Manifest }` with the text above, and no `out/` path or scratch dir was
  made.

**Done when:** `cargo test -p buildl --lib exec::local` passes and `cargo make dod` is green.
Commit `feat(buildl): choose bare or the input sandbox per workspace`.

### Task 2 — Wire `Local` into the ports

**Files:**
- Modify `crates/buildl/src/local_ports.rs` — `type Strategy = Local` (`:58`); `runs()`
  (`:142-171`) returns `Ok` for `Bare` and `Sandbox` and `Local::refusal(strategy)` otherwise,
  its doc and `# Errors` updated; `ports()` (`:255-282`) builds `Local::Bare(Bare::new(…))`,
  `Local::Sandbox(InputSandbox::new(root, self.scratch.clone(), lock.owner(), timeout, search))`,
  or `Local::Refused(strategy)`; tests
- Modify `crates/buildl/src/workspace.rs` — `build` and `test` `# Errors` (`:195-199`,
  `:297-301`) say "`os-sandbox` or `container`" instead of "a strategy other than `bare`";
  tests `build_refuses_a_strategy_it_cannot_run_before_anything_is_made` (`:1020`) and
  `a_test_run_refuses_a_strategy_it_cannot_run_before_anything_is_made` (`:1052`) replace
  `strategy = "bare"` with `strategy = "os-sandbox"` instead of removing it, and expect the new
  refusal text (today `:1043-1045`)
- Modify `crates/buildl-cli/src/run.rs` — `a_build_that_ends_in_an_error_says_so_once` (`:772`)
  writes `[execution]\nstrategy = "os-sandbox"\n` as its manifest; its comment says the strategy
  is one this buildl cannot run; it still expects `Exit::Definition`, one stderr line starting
  `error: manifest: buildl.toml: [execution] strategy is `, and empty stdout
- Modify `crates/buildl/src/lib.rs` — crate doc lines `:40` (example manifest) and `:74-75`
  (only `bare` runs) describe `bare` and `sandbox` running, the default `sandbox`
- Modify `crates/buildl/tests/e2e/facade.rs` — `build_refuses_a_strategy_it_cannot_run_and_plan_does_not`
  (`:71`) writes `strategy = "os-sandbox"` instead of removing the line
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs` — `a_strategy_that_cannot_be_run_fails_build_and_not_plan`
  (`:522`) writes `[execution]\nstrategy = "os-sandbox"` and expects the new refusal text

**Change:** the default manifest builds under the sandbox; `bare` builds as before; `os-sandbox`
and `container` are refused by `build` and `test` before anything is made, and planned by `plan`.

**Proof:**
- `local_ports.rs` `only_bare_can_be_run` (`:418`) becomes `bare_and_sandbox_can_be_run` —
  `runs()` is `Ok` for `bare`, `sandbox` and no line; for `os-sandbox` and `container` it is the
  `Manifest` adapter error with exactly the refusal text.
- `local_ports.rs` `every_port_is_made_from_the_manifest_the_root_and_the_lock` (`:604`) — under
  `BARE`, `ports.strategy` renders as `Local::Bare(Bare::new(…))`; a second case with no
  strategy line renders as `Local::Sandbox(InputSandbox::new(…, lock.owner(), …))`; a third with
  `os-sandbox` gives `Local::Refused(OsSandbox)` and `ports()` is `Ok`.
- `facade.rs` refusal test — `plan` has 6 targets; `build` is the `Manifest` error; nothing made.
- `workspace.rs` the two refusal tests — `build` and `test` under `os-sandbox` are the `Manifest`
  error with the new text, and the root still holds only `a.txt`, `build.lua`, `buildl.toml`.
- `run.rs` `a_build_that_ends_in_an_error_says_so_once` — exit 5, one stderr line.
- `exit_codes.rs` refusal test — `plan` exits 0; `build` exits 5 with
  `error: manifest: buildl.toml: [execution] strategy is "os-sandbox", and this version of buildl
  runs "bare" and "sandbox" only: write strategy = "sandbox" under [execution]\n`.
- `exit_codes.rs` `a_workspace_without_a_strategy_line_builds` — the old test's workspace
  (`cp`, `a.txt`, no `[execution]`): `build` exits 0 and `out/copy` (the target's output) equals
  `a.txt`.

**Done when:** `cargo test -p buildl -p buildl-cli` passes and `cargo make dod` is green. Commit
`feat(buildl): run build and test under the input sandbox by default`.

### Task 3 — Prove the sandbox end to end

**Files:**
- Create `crates/buildl/tests/fixtures/sandboxed/` —
  - `buildl.toml`: `proc.run = ["cp", "sh"]`, `fs.read = ["."]`, `fs.write = ["out", ".buildl"]`,
    `[execution]` `workers = 4`, `timeout = "2s"`, and no `strategy` line
  - `build.lua`: `visible` (inputs `a.txt`, `cp $in $out`); `hidden` (inputs `a.txt`,
    `cp secret.txt $out` — reads an undeclared file); `partial` (inputs `partial.sh`,
    `sh $in $out`, the script writes `part` to `$1` then `exit 1`); `slow` (inputs `slow.sh`,
    `sh $in $out`, the script writes `part` to `$1` then `exec sleep 30`)
  - `a.txt`, `secret.txt`, `partial.sh`, `slow.sh`
- Create `crates/buildl/tests/e2e/sandbox.rs`; modify `crates/buildl/tests/e2e/main.rs` — `mod sandbox;`

**Change:** none in library code.

**Proof:** in `sandbox.rs`, using `Fixture` (`e2e/common.rs`) and `Fixture::scratch()` (plan 02):
- `the_default_manifest_builds_the_chain` — `chain` with its `strategy` line removed builds every
  target `Built` (the chain runs a `$deps` program, `out/greet`, so mode-kept copies are
  exercised).
- `an_undeclared_input_fails_under_sandbox_and_builds_under_bare` — `sandboxed`,
  `build_targets(&["hidden"])`: `ActionFailure { status }` (non-zero); with `strategy = "bare"` appended under
  `[execution]`, the same target is `Built`.
- `outputs_are_byte_identical_under_both_tiers` — two copies of `chain`, one with
  `strategy = "bare"` and one without: `snapshot()` of `out/` is equal (`first_difference` is
  `None`) — outputs with their execute bits, `cache.json`, `graph.json` and the cas
  (`common.rs:159-176`); this holds because `strategy` is no key component. The chain's actions are `cp`, `sort` and a `sh` script that reads and prints lines;
  none records its directory (spec §2 Same results; spec §5 names `cp` and `cat`, and `chain`
  is used because it already exercises deps, a directory output and a `$deps` program).
- `a_target_built_under_bare_is_cached_under_sandbox` — build `chain` under `bare`, remove the
  line, build: every outcome `Cached`.
- `a_failed_or_timed_out_action_leaves_nothing_in_out` — `sandboxed`, `build_targets(&["partial"])`
  then `(&["slow"])`: each fails (`ActionFailure { status: 1 }`, then `InfraFailure { kind: TimedOut }`), and nothing exists at
  `out/partial` or `out/slow`.
- `leftovers_are_removed_and_nothing_is_left_after_a_build` — create
  `scratch().dir()/stale/file` before `build_targets(&["visible"])`; after it, `scratch().dir()`
  does not exist.
- `tests_run_sandboxed` — `tested` with its strategy line removed:
  `test(&["same", "flagged"], Rerun::Off)` gives `same` passed and `flagged` failed (it reads
  `flag.txt` undeclared, `e2e/test.rs:11`); under `bare` both pass.
- `no_log_line_names_the_scratch_directory` — after the `hidden`, `partial` and `slow` failures,
  no line of `log_lines()` contains `scratch().dir()`'s text, in either its written form or its
  canonical form (`fs::canonicalize` of `std::env::temp_dir()` joined with the same name: on
  macOS `/var` is `/private/var`).

**Done when:** `cargo test -p buildl --test e2e sandbox` passes and `cargo make dod` is green.
Commit `test(buildl): prove the input sandbox end to end`.

---

## Risks

- **Task 2 flips the default.** Every e2e and CLI fixture that writes `strategy = "bare"` keeps
  running bare; one that writes none now runs sandboxed. `grep -rln 'strategy' crates/*/tests`
  before Task 2 lists the ones that keep `bare`. The tests that expect the default to be refused
  are the five Task 2 names (`facade.rs:71`, `exit_codes.rs:522`, `workspace.rs:1020`, `:1052`,
  `run.rs:772`); `grep -rn 'cannot_run\|cannot be run\|left to its default' crates` before Task 2
  re-checks there is no sixth.
- **`slow` depends on the timeout** (`2s`) against `sleep 30`; the test asserts the class, not a
  duration.
- **`partial` and `slow` fail the build;** they are only ever selected by name, never in a
  whole-workspace build of `sandboxed`.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` green: `local.rs` names only `exec`'s own files.
- By hand: copy `crates/buildl/tests/fixtures/chain` to a scratch directory, `chmod +x greet.sh`
  there (it is `100644` in git; `Fixture::copy_of` sets the bit, `common.rs:56-71`), remove its
  `strategy` line, and run the built binary from inside it,
  `cd <copy> && <repo>/target/debug/buildl build`: exit 0.
