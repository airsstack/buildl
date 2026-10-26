---
status: approved
created: 2026-10-26
depends-on: [01, 02]
built-on: a08d695c7505
---

# Workspace Watch Implementation Plan

**Goal:** `Workspace::watch` runs plan 01's loop with every cycle and check pass on the workspace opened again.

**Architecture:** An open `Workspace` reads `buildl.toml` once (`crates/buildl/src/workspace.rs:130-134`,
`crates/buildl/src/local_ports.rs:96-108`; spec P16), so watch keeps no open workspace
between cycles: a private type, `Reopening`, implements `Cycles` and `WatchReport` (plan 01's
loop takes one value bounded by both). It holds the first `Wiring` only as the recipe for
the next one — `Wiring::reopened` reads the manifest again from the root and carries over
the approvals directory (`Wiring::with_data_dir`, `local_ports.rs:113`) and the search path
(`SearchPath::from_process()`, `local_ports.rs:105`, the process `PATH`, which is the same
whenever it is read) — so between cycles nothing read from `buildl.toml` is kept, as spec §2
requires. It also holds the scope, overrides and `RunOptions`, and the caller's one reporter
and console; its `WatchReport` methods forward to that reporter. `Workspace::watch` is generic over plan 01's `Changes`,
so the CLI passes plan 03's `NotifyChanges` and the e2e tests a scripted one; it takes no
`notify` type itself. `Reopening` lives in `crates/buildl/src/workspace/watching.rs`,
declared from `workspace.rs` (a file module may have a same-named directory of children);
`cargo make guard-adapter-isolation` reads the module name with `basename … .rs`
(`Makefile.toml:259-260`), so both `workspace.rs` and `workspace/` count as the composition
module `workspace` (`Makefile.toml:234`). The layout of spec §3 comes from `Wiring`, which
owns the manifest (`local_ports.rs:76-82`).

**Tech Stack:** Rust, `buildl`, `cargo make dod`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2, §3 (`layout`), §4 (`Workspace::watch`), §6, §8 (e2e row),
P8, P11, P16, P18.

---

## Files that change

```
crates/buildl/src/local_ports.rs          — [modify] Wiring::reopened, Wiring::watch_layout; unit tests
crates/buildl/src/workspace.rs            — [modify] Workspace::watch; Workspace::build_watched (private), Workspace::build delegating; `mod watching;`
crates/buildl/src/workspace/watching.rs   — [create] Reopening: impl Cycles; unit tests
crates/buildl/tests/e2e/watch.rs          — [create] e2e rows of spec §8, with a scripted Changes and a recording reporter
crates/buildl/tests/e2e/main.rs           — [modify] `mod watch;`
```

---

### Task 1 — The layout and a reopened wiring

**Files:**
- Modify `crates/buildl/src/local_ports.rs`

**Change:** `Wiring::watch_layout(&self) -> Result<WatchLayout>`: `out` and `state` from the
workspace it read (`self.workspace.out`, `.state`; `types/workspace.rs:23-27`); `config` holds
`buildl.toml`, `buildl.lock`, and `<dir>/extension.toml` for each
`PluginSource::Path(dir)` of `self.manifest.plugins()` (`manifest/toml_manifest.rs:119`,
`types/plugin_source.rs:306-308`), each as a `ChangedPath`. `Wiring::reopened(&self) ->
Result<Wiring>`: `Wiring::read(self.root)` again, keeping `data` and `search`.

**Proof:** written first, seen failing, then passing — in `local_ports.rs`'s test module, on
temp workspaces as its neighbours build them:
- `watch_layout_names_out_state_and_the_two_root_files`
- `watch_layout_names_a_path_plugins_extension_toml` — `[plugins.ourgo] path = "plugins/ourgo"` gives `plugins/ourgo/extension.toml`; a git plugin adds nothing
- `watch_layout_follows_a_custom_out` — `out = "build"`
- `reopened_reads_the_manifest_again` — change `out` in `buildl.toml` after the first read; the reopened layout has the new one
- `reopened_keeps_the_data_dir`

**Done when:** `cargo test -p buildl local_ports` passes and the stack's Definition of Done
is green. Commit `feat(buildl): read a watch layout from the manifest`.

### Task 2 — `Workspace::build_watched`

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Change:** a private `Workspace::build_watched`, with `Workspace::build`'s parameters
(`workspace.rs:389-396`), doing what `build` does (`:397-421`: `runs`, `lock`, ports,
`set_remote`, fold, drop, `concluded`) over plan 02's `Pipeline::build_watched`, and returning
`Watched` whose `ended` is `concluded(watched.ended, folded)`; a `runs()` or `lock()` failure
is `Watched { built: None, ended: Err }`. `Workspace::build` returns
`self.build_watched(…).ended`; its docs and signature stay.

**Proof:** written first, seen failing, then passing — in `workspace.rs`'s test module:
- `build_watched_on_a_held_lock_has_no_graph` — hold `Wiring::lock` (`local_ports.rs:300`) of the same workspace; `build_watched` gives `built: None` and `ended: Err` naming the action cache (the lock-busy case of spec §6)
- `build_watched_returns_the_graph_it_built` — on a temp workspace, `built.graph` has its target
The existing e2e tests that call `Workspace::build` (`crates/buildl/tests/e2e/*.rs` through
`common.rs:219-260`) pass unchanged.

**Done when:** `cargo test -p buildl` passes and the stack's Definition of Done is green.
Commit `refactor(buildl): build through the watched pipeline entry`.

### Task 3 — `Reopening` and `Workspace::watch`

**Files:**
- Create `crates/buildl/src/workspace/watching.rs`
- Modify `crates/buildl/src/workspace.rs`

**Change:** `Reopening<'a, R, C>` implements `WatchReport` by forwarding each call to its
reporter, and `Cycles`:
- `build`: `Wiring::reopened` on the held wiring; on `Err`, `Cycle { layout: None, watched:
  Watched { built: None, ended: Err } }`; otherwise a `Workspace` over it, its
  `watch_layout` (an `Err` is treated as the open failing), then `build_watched` with the held
  scope, overrides, options, reporter and console.
- `graph`: `Wiring::reopened`, then the workspace's `graph()` (`workspace.rs:245`, no lock,
  P7), keeping the graph.

`Workspace::watch<Ch: Changes, R: Reporter + WatchReport, C: Console>(&self, changes: &mut Ch,
scope: &Scope, overrides: &BTreeMap<SettingName, SettingValue>, options: RunOptions,
reporter: &mut R, console: &mut C) -> Error`: its first layout from `self.wiring.watch_layout()`
(an error is returned at once — the first open already succeeded, spec §2), then
`buildl_core::watch(changes, &mut Reopening { …, reporter, console }, layout)`. The reporter is
both the build's `Reporter` and the loop's `WatchReport`, and is borrowed once, inside
`Reopening`: one value writes stderr, so the two kinds of line never interleave (plan 05
implements both on `TerminalReporter`).
Doc comment states: the caller starts `changes` before calling, so an edit during the first
cycle is seen (spec §2).

**Proof:** written first, seen failing, then passing — in `watching.rs`'s test module on a
temp workspace:
- `a_cycle_on_a_broken_manifest_has_no_layout_and_an_error` — write `buildl.toml` with an unknown table; `build` gives `layout: None`, `built: None`, `ended: Err` naming the manifest
- `a_cycle_reads_the_manifest_again` — change `out` between two `build` calls; the second cycle's layout has it
- `graph_takes_no_lock` — hold `Wiring::lock` of the same workspace; `graph` still succeeds

**Done when:** `cargo test -p buildl watching` passes, `cargo make guard-adapter-isolation`
passes, and the stack's Definition of Done is green. Commit `feat(buildl): watch a workspace,
opening it again for every cycle`.

### Task 4 — e2e rows

**Files:**
- Create `crates/buildl/tests/e2e/watch.rs`; modify `crates/buildl/tests/e2e/main.rs`

**Change:** tests only. A test `Changes`, `Script`, holds steps: each step edits the fixture
(through `Fixture::append`, `replace_line`, or a plain write, `common.rs:101-128`) and returns
the batch of the paths it touched; after the last step it returns
`Error::Adapter { port: PortName::Changes, … }`, which ends `Workspace::watch`. A test
reporter, `Recorded`, implements `Reporter` (every method recording nothing but the
`summary`) and `WatchReport` (recording each call). Fixtures by `Fixture::copy_of`
(`common.rs:62`): `copy_of("chain", &["greet.sh"])`, as `approver.rs:18` and `doctor.rs:13`
copy it, so every target builds (its `lib/build.lua` globs `words/*.txt`), and `discovery`.

**Proof:** written first, seen failing, then passing — one test per row of the intent's
Desired-outcome table, as spec §8 lists them:
- `start_builds_as_a_plain_build_does` — no steps; the first cycle's outcomes equal a plain `Fixture::build()` on a twin copy
- `a_source_edit_rebuilds_only_its_target` — append to `lib/words/<file>.txt`; the second cycle's `summary` counts one or more built and the rest cached, and only targets that read it or depend on it are built
- `a_build_file_edit_adding_a_target_builds_it` — append a `b.target(...)` to `lib/build.lua`; the new label is built in the next cycle
- `a_manifest_edit_is_read_again` — change `out = "out"` to `out = "build"` and `fs.write`'s `"out"` to `"build"` together in `buildl.toml` (the ceiling checks output roots against `fs.write`, `crates/buildl-core/src/grants/outside.rs:108`); the next cycle places outputs under `build/`; a further step touching a file there produces no `building` call
- `a_new_globbed_file_is_built` — create `lib/words/new.txt`; a cycle runs and the `words` target is built
- `a_readme_edit_adds_no_log_line` — write `README.md`; no `building` call; `Fixture::log_lines()` count unchanged
- `a_build_file_comment_edit_runs_no_cycle` — append `-- note` to `lib/build.lua`; no `building` call
- `a_save_of_the_same_bytes_runs_no_action` — rewrite an input with its own bytes; the cycle runs and no outcome is `Built` (each is `Cached`)
- `changes_during_a_cycle_make_one_more_cycle` — a step returns a batch of three paths from three edits; exactly one further `building` call
- `a_discovery_input_outside_the_scope_runs_a_cycle` — `discovery` fixture, scope a non-discovery target; edit the discovery target's input; a cycle runs
- `a_readme_edit_on_the_discovery_fixture_adds_no_log_line` — settles spec §10's equality risk
- `plan_and_log_work_between_cycles` — inside a step, `Fixture::open().plan(…)` and `.log(…)` succeed (the lock is free)
- `a_watch_records_what_plain_builds_record` — the same edits applied to a twin with `Fixture::build()` after each: `rows()`, `snapshot()` of `out/` and cas equal

**Done when:** `cargo test -p buildl --test e2e watch` passes and the stack's Definition of
Done is green. Commit `test(buildl): prove watch against the intent's outcomes`.

---

## Risks

- Most risky: Task 2 reshapes `Workspace::build`. The existing e2e suite is the guard and
  must pass untouched.
- Spec P6's `1 failed` came from copying `chain` without `greet.sh`'s executable bit; Task 4
  copies it with the bit. Rows that count outcomes still compare against a plain build of a
  twin rather than fixed numbers.
- `a_save_of_the_same_bytes_runs_no_action` meets the stat cache's two-second rule
  (`docs/architecture.md:252`): the input is hashed again, still giving the same key. Only
  "no outcome is `Built`" is asserted, never "no hash".
- An error raised by the run's own flags in every cycle (`Error::UploadUnsandboxed` under
  `strategy = "bare"`) is a cycle error under spec §6: printed each cycle, watching goes on,
  and a `buildl.toml` edit to the strategy is picked up by the next cycle.

## Proof

`cargo make dod` green, with `guard-adapter-isolation`; `cargo test -p buildl --test e2e watch`
covers every e2e row of spec §8.
