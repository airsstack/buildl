---
status: approved
created: 2026-10-26
built-on: a08d695c7505
---

# Watch Core Implementation Plan

**Goal:** `buildl-core` decides, against fake ports, when a batch of file changes starts a watch cycle.

**Architecture:** The values go one per file under `types/`, and the three ports one per file
under `ports/`, as the crate keeps them (`crates/buildl-core/src/ports/mod.rs`,
`crates/buildl-core/src/types/mod.rs:104`). Spec §4's diagram groups them under one `watch/`
box; the table beside it names only the crate, and this plan follows the crate's layout.
`types` holds no decision between different concepts (`crates/buildl-core/src/lib.rs:48-52`,
rule 2), so every comparison of a changed path with a directory or a source path, and the
renewal of the reference from a cycle, live in the new `watch/` module beside `react` (spec
§3's table) and the `watch` loop (spec §4). Nothing in `buildl-core` reads a clock (spec
P14): the 50 ms window belongs to the adapter (plan 03). Plain generics, no `dyn`
(static-dispatch rule). The loop takes one value that is both `Cycles` and `WatchReport`, because
plan 04's cycles hold the one reporter that writes stderr, and a second `&mut` to it could
not coexist. Nothing calls the loop yet; plan 04 wires it.

**Tech Stack:** Rust, `buildl-core`, `cargo make dod`. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §3 (reference, batch, `react`), §4 (units), §6 (which failures
continue), §8 (core unit row).

---

## Files that change

```
crates/buildl-core/src/types/changed_path.rs    — [create] ChangedPath: one workspace-relative path an event named; unit tests
crates/buildl-core/src/types/change_batch.rs    — [create] ChangeBatch: a non-empty sorted set of ChangedPath; unit tests
crates/buildl-core/src/types/watch_reference.rs — [create] WatchLayout, Built, Reference
crates/buildl-core/src/types/reaction.rs        — [create] Reaction
crates/buildl-core/src/types/trigger.rs         — [create] Trigger
crates/buildl-core/src/types/cycle.rs           — [create] Watched, Cycle
crates/buildl-core/src/types/mod.rs             — [modify] declare and re-export the six files
crates/buildl-core/src/ports/changes.rs         — [create] trait Changes
crates/buildl-core/src/ports/cycles.rs          — [create] trait Cycles
crates/buildl-core/src/ports/watch_report.rs    — [create] trait WatchReport
crates/buildl-core/src/ports/mod.rs             — [modify] declare and re-export the three ports
crates/buildl-core/src/watch/mod.rs             — [create] module docs and re-exports only
crates/buildl-core/src/watch/react.rs           — [create] react() and its path comparisons; unit tests
crates/buildl-core/src/watch/run.rs             — [create] watch(), the loop, and renew(); unit tests over fakes
crates/buildl-core/src/lib.rs                   — [modify] `pub mod watch;`, re-export the new types, ports and `watch`; module table and rule 1
crates/buildl-core/src/error.rs                 — [modify] NameKind gains ChangedPath (Task 1); PortName gains Changes (Task 3)
crates/buildl-cli/src/exit.rs                   — [modify] the PortName list of `only_the_manifests_adapter_is_a_definition_error` (:733-746) gains Changes
crates/expected-module-edges.txt                — [modify] add the `watch -> …` edges `cargo make guard-module-edges` reports
```

---

### Task 1 — The change values

**Files:**
- Create `crates/buildl-core/src/types/changed_path.rs`, `change_batch.rs`
- Modify `crates/buildl-core/src/types/mod.rs`, `crates/buildl-core/src/lib.rs`,
  `crates/buildl-core/src/error.rs`

**Change:** add `ChangedPath`, a workspace-relative path as a file event named it:
non-empty, `/`-separated, no leading or trailing `/`, no empty, `.` or `..` segment, and any
other character allowed. It is deliberately looser than `SourcePath`
(`types/source_path.rs:14-17` allows only ASCII letters, digits, `.`, `-`, `_`), because an
editor's swap file (`.#a.c`, `a.c~`) or a README with a space must still reach `react`, which
sends it to a check pass. Its module docs argue why it is not a duplicate of `SourcePath`,
`Directory` or `TreePath` (modularity rule's duplicate-type gate). `ChangedPath::parse`
refuses with `Error::InvalidName { kind: NameKind::ChangedPath, … }`: add the variant to
`NameKind` (`error.rs:1039-1101`), its phrase `changed path` in `NameKind`'s `Display`, and its
row in the `name_kind_renders_a_human_phrase` test (`error.rs`, near `:2145`). The reasons are
this type's own, not `grammar::path`'s. `ChangedPath::as_str`, `Display`. Add `ChangeBatch`, a
non-empty `BTreeSet<ChangedPath>`: `ChangeBatch::new(set)` returns `None` for an empty set;
`first()` (the least path), `len()`, `iter()`.

**Proof:** written first, seen failing, then passing —
- `a_relative_path_with_a_space_or_tilde_is_accepted` — `docs/read me.md`, `a.c~`, `.#a.c` parse
- `an_absolute_empty_dotted_or_doubled_path_is_refused` — `/a`, `""`, `a/../b`, `./a`, `a//b`, `a/` are each `Error::InvalidName` with `NameKind::ChangedPath`
- `an_empty_set_makes_no_batch` — `ChangeBatch::new(BTreeSet::new())` is `None`
- `a_batch_is_ordered_and_first_is_the_least_path` — paths inserted `b`, `a` iterate `a`, `b`; `first()` is `a`; `len()` is 2
- `name_kind_renders_a_human_phrase` gains `(NameKind::ChangedPath, "changed path")`

**Done when:** `cargo test -p buildl-core -- changed_path change_batch name_kind` passes and the
stack's Definition of Done is green. Commit `feat(buildl-core): add the values a file change is told in`.

### Task 2 — Reference, reaction, trigger and cycle values

**Files:**
- Create `crates/buildl-core/src/types/watch_reference.rs`, `reaction.rs`, `trigger.rs`, `cycle.rs`
- Modify `crates/buildl-core/src/types/mod.rs`, `crates/buildl-core/src/lib.rs`

**Change:** add plain values, as spec §3 defines them, with no methods that decide between
concepts:
- `WatchLayout { out: Directory, state: Directory, config: BTreeSet<ChangedPath> }` — what the
  last open of the workspace read; `config` holds `buildl.toml`, `buildl.lock` and each
  workspace plugin's `<path>/extension.toml` (filled by plan 04).
- `Built { graph: TargetGraph, inputs: BTreeSet<SourcePath> }` — what a cycle's build ran on
  (filled by plan 02).
- `Reference { layout: WatchLayout, built: Option<Built> }`.
- `Reaction { Build, Check, Ignore }` in `reaction.rs`.
- `Trigger { Start, Changed(ChangeBatch) }` in `trigger.rs` — what a cycle is told it was
  started by.
- `Watched { built: Option<Built>, ended: Result<Vec<ActionOutcome>> }` — one build and the
  graph it ran on (returned by plan 02's pipeline entry).
- `Cycle { layout: Option<WatchLayout>, watched: Watched }` — `layout` is `None` when the
  workspace could not be opened.

These files hold data only, with no logic; each module doc cites the unit-test mandate's
exemption for logic-free files by name.

**Proof:** they compile and are re-exported; Tasks 4 and 5 exercise every field.

**Done when:** the stack's Definition of Done is green. Commit
`feat(buildl-core): add the watch reference and cycle values`.

### Task 3 — The three ports

**Files:**
- Create `crates/buildl-core/src/ports/changes.rs`, `cycles.rs`, `watch_report.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`, `crates/buildl-core/src/lib.rs`,
  `crates/buildl-core/src/error.rs`, `crates/buildl-cli/src/exit.rs`

**Change:**
- `trait Changes { fn next(&mut self) -> Result<ChangeBatch>; }` — blocks until a debounced
  batch exists; an error ends watching.
- `trait Cycles { fn build(&mut self) -> Cycle; fn graph(&mut self) -> Result<TargetGraph>; }` —
  one build cycle on the workspace opened again; one check pass, a load of the workspace
  opened again.
- `trait WatchReport { fn building(&mut self, trigger: &Trigger); fn failed(&mut self, error:
  &Error); fn waiting(&mut self); }` — the three things the loop says (spec §7).
- `PortName::Changes` (`error.rs:786`), displayed `file watcher` (display match near `:839`),
  so the adapter's failure names its port; `Exit::of_error` already maps every non-manifest
  port to `Infrastructure` (`crates/buildl-cli/src/exit.rs:113-116`), which is spec §6's exit
  3. Add `Changes` to the port list of `only_the_manifests_adapter_is_a_definition_error`
  (`exit.rs:733-746`) and to the display table test of `error.rs` (near `:2126`).

Port files hold only trait definitions with their docs, in the house form of
`ports/declaration_source.rs`, which carries the trait-only exemption of the unit-test
mandate in its module docs.

**Proof:** written first, seen failing, then passing —
- `error.rs` display table test gains `(PortName::Changes, "file watcher")`
- `exit.rs::only_the_manifests_adapter_is_a_definition_error` lists `PortName::Changes` and still holds

**Done when:** `cargo test -p buildl-core -p buildl-cli` passes and the stack's Definition of
Done is green. Commit `feat(buildl-core): add the change, cycle and watch-report ports`.

### Task 4 — `react`

**Files:**
- Create `crates/buildl-core/src/watch/mod.rs`, `crates/buildl-core/src/watch/react.rs`
- Modify `crates/buildl-core/src/lib.rs`, `crates/expected-module-edges.txt`

**Change:** `react(batch: &ChangeBatch, reference: &Reference) -> Reaction`, spec §3's table,
first match wins: drop every path under `layout.out`, `layout.state` or `.git`; nothing left →
`Ignore`; `built` absent → `Build`; a kept path whose text equals the text of an input in
`built.inputs` (`ChangedPath::as_str` against `SourcePath::as_str`, `types/source_path.rs:50`)
→ `Build`; a kept path in `layout.config` → `Build`; otherwise `Check`. "Under" is a private
function of this file: the path equals the directory's text or starts with it followed by
`/`; the root directory (`Directory::is_root`, `types/directory.rs:68`) holds every path. Event kinds play no part (spec P5). `watch/mod.rs` holds module docs
and re-exports only (mod-rs-export-only rule). In `lib.rs`, add `watch` to the module table
and to rule 1's list of non-phase modules that name phases (`lib.rs:43-47`), since plan 02's
`watch/inputs.rs` names `resolve::narrow`.

**Proof:** written first, seen failing, then passing —
- `under_holds_for_the_directory_and_below_it_only` — `out/x.o` and `out` are under `out`; `outer/x` is not; every path is under the root
- `only_ignored_directories_is_ignore` — a batch of `out/x.o`, `.buildl/log.jsonl`, `.git/index` is `Ignore`
- `a_changed_out_directory_moves_the_drop` — with `layout.out = build`, `build/x.o` is `Ignore` and `out/x.o` is `Check`
- `no_graph_builds` — `built: None`, batch `README.md` is `Build`
- `an_input_builds` — `lib/words/a.txt` in `built.inputs` is `Build`
- `a_config_file_builds` — `buildl.toml`, `buildl.lock`, `plugins/go/extension.toml` (in `config`) each `Build`
- `anything_else_checks` — `README.md`, `.#a.c` with a graph present are `Check`
- `an_input_beside_ignored_paths_still_builds` — `out/x.o` plus an input is `Build`

**Done when:** `cargo test -p buildl-core -- react` passes, `cargo make guard-module-edges`
passes with the edges it reports added to `crates/expected-module-edges.txt`, and the stack's
Definition of Done is green. Commit `feat(buildl-core): decide what a batch of changes starts`.

### Task 5 — The `watch` loop

**Files:**
- Create `crates/buildl-core/src/watch/run.rs`
- Modify `crates/buildl-core/src/watch/mod.rs`, `crates/buildl-core/src/lib.rs`,
  `crates/expected-module-edges.txt` (only if new edges appear)

**Change:** `watch<C: Changes, Y: Cycles + WatchReport>(changes: &mut C, cycles: &mut Y,
layout: WatchLayout) -> Error`. One value is both the cycles and the report, so the
reporter it holds is borrowed once. It says `building(&Trigger::Start)` and runs a first
`build`, then loops: `next` → `react` → for `Check`, `graph()`: an error or a graph unequal
to `built.graph` is `Build`, an equal one is `Ignore` → for `Build`, `building(&Changed(batch))`
and `build()`. After each cycle: an `Err` in `watched.ended` goes to `failed`; the reference
becomes `renew(reference, cycle)`; then `waiting()`. An `Ignore` says nothing. It returns only
the error `next` returned. Changes arriving during a cycle are the adapter's to queue; the
loop takes one batch per `next`. `renew(reference: Reference, cycle: Cycle) -> Reference`, a
private function of this file, takes the cycle by value after `failed` has seen its `ended`,
so no graph is cloned: the cycle's layout when it has one, else the old layout; the cycle's
`built` as it is (absent stays absent).

**Proof:** written first, seen failing, then passing — fakes in the test module: a scripted
`Changes` that yields given batches then an error; one fake implementing `Cycles` and
`WatchReport` that returns given `Cycle`s and graphs and records every call in order.
- `the_first_cycle_runs_before_any_change` — `building(Start)`, one build, `waiting`
- `an_ignored_batch_runs_nothing_and_says_nothing`
- `a_check_with_an_unchanged_graph_runs_no_build` — one `graph` call, no second `build`, nothing reported
- `a_check_with_a_changed_graph_builds`
- `a_check_that_fails_builds`
- `a_failed_cycle_is_reported_and_watching_goes_on` — the `Err` reaches `failed`, then `waiting`, then the next batch is taken
- `after_a_cycle_without_a_graph_any_change_builds` — `built: None` then batch `README.md` builds without a check
- `a_cycle_that_could_not_open_keeps_the_old_layout` — `layout: None`, then a batch under the old `out` is ignored
- `a_cycle_with_a_new_layout_moves_the_drop` — the cycle's layout has `out = build`; a batch under `build/` is ignored
- `the_trigger_names_the_batch` — `building(Changed(b))` carries the batch whose `first()` and `len()` the CLI prints
- `a_changes_error_ends_the_loop_with_that_error`

**Done when:** `cargo test -p buildl-core -- watch::` passes and the stack's Definition of Done
is green. Commit `feat(buildl-core): add the watch loop`.

---

## Risks

- `TargetGraph` equality is derived (`types/target_graph.rs:63`,
  `#[derive(Debug, Clone, PartialEq, Eq)]`), so `Check` compares by value as spec §3
  requires; nothing here relies on graph JSON.
- The loop never returns normally; Ctrl-C ends the process (spec §6), so no stop signal is
  modelled.
- An error raised inside every cycle by the flags themselves — `Error::UploadUnsandboxed`,
  raised during execution (`crates/buildl-core/src/execute/assembly.rs:54`) — is a cycle
  error under spec §6's row ("cycle error … continues"): it is printed each cycle until
  `buildl.toml`'s strategy changes, which a full reload then picks up.

## Proof

`cargo make dod` green, including `guard-core-purity` (no clock in `watch/`) and
`guard-module-edges`. `cargo test -p buildl-core -- watch:: react changed_path change_batch`
covers every row of spec §3's table and every `buildl-core` row of spec §8.
