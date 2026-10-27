---
status: executing
created: 2026-10-27
built-on: 5947af3e315f
---

# Standard output as an output — Implementation Plan

**Goal:** a build target or discovery target that declares `stdout = true` has its command's
standard output stored as its one output, locally and through a remote cache or executor.

**Architecture:** a new enum `OutputSource { Command, Stdout }` follows the path `network`'s
`NetworkAccess` takes from the Lua table to the action key. Adapters change nothing about how
they run or collect: `buildl-core` decides, after `collect`, that a stdout target's path must
hold nothing and that its output is the captured stdout (architecture §6, `docs/architecture.md:674`).
Remote results are judged by the same rule, with the output taken from `stdout_digest`.
No backward compatibility: buildl is unpublished, so the field is required in `OwnComponents`
like `network`, and an older `.buildl/cache.json` is refused once by the existing strict reading.

**Tech Stack:** Rust 2024 workspace (`buildl-core`, `buildl-lua`, `buildl`, `buildl-cli`), serde
with the one canonical serializer, airsl/mlua for the Lua table, tonic/prost REAPI adapter.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §3, §6, §7. Gates for every task:
`cargo make dod` and `cargo deny check` green; Conventional Commit, one per task.

---

## Files that change

```
crates/buildl-core/src/types/declaration.rs     — [modify] OutputSource enum; Target gains output_source (:75, beside network :91)
crates/buildl-core/src/types/build_file.rs      — [modify] StagedTarget gains output_source (:131, beside network :149)
crates/buildl-core/src/types/mod.rs             — [modify] re-export OutputSource
crates/buildl-core/src/lib.rs                   — [modify] re-export OutputSource beside NetworkAccess (:197)
crates/buildl/src/report/plan_text.rs           — [modify] difference_text words Difference::Stdout (:17-35)
crates/buildl-core/tests/flows/golden/plan.json — [modify] keys gain the field
crates/buildl/tests/e2e/golden/chain.graph.json — [modify] nodes gain the field
crates/buildl/src/cache/journal.rs              — [modify] literal key JSON in tests (:159)
crates/buildl/src/cache/local_cache.rs          — [modify] literal key JSON in tests (:231)
(every other file constructing Target, Node, ReadyAction or OwnComponents) — [modify] the new field beside `network:`; found with `git grep -l "network: NetworkAccess::"` (about 40 files, mostly tests)
crates/buildl-core/tests/flows/verify.rs        — [modify] verify re-runs a stdout target
crates/buildl-core/src/load/traversal.rs        — [modify] convert_target carries it (:280); refuse more than one output with Stdout (beside refuse_discovery_overrides :194-210)
crates/buildl-core/src/error.rs                 — [modify] DeclarationField gains the stdout field (:923, Display :952)
crates/buildl-core/src/resolve/assembly.rs      — [modify] Node carries it (:168)
crates/buildl-core/src/types/target_graph.rs    — [modify] Node field (:27) and its JSON view (:172-230)
crates/buildl-core/src/execute/assembly.rs      — [modify] ReadyAction carries it (:297)
crates/buildl-core/src/types/ready_action.rs    — [modify] ReadyAction field (struct :26, beside network :47)
crates/buildl-core/src/plan/components.rs       — [modify] OwnComponents built with it (:96)
crates/buildl-core/src/types/action_key.rs      — [modify] OwnComponents field (:80-97), doctest at :113-137
crates/buildl-core/src/plan/difference.rs       — [modify] Difference::Stdout when it differs (:67, order doc :30-36)
crates/buildl-core/src/types/plan.rs            — [modify] Difference::Stdout variant (:30)
crates/buildl-core/src/types/outcome.rs         — [modify] OutcomeClass::OutputWritten { name } (:66-107); tally (:181)
crates/buildl-core/src/execute/perform.rs       — [modify] collected/examined judge a stdout target (:307-348); the output manifest is built from the stored stdout (beside kept :923, stored :934)
crates/buildl-core/src/execute/fetch.rs         — [modify] usable/unasked/fetch for a stdout target (:43, :66, :268)
crates/buildl-core/src/execute/upload.rs        — [modify] upload_of sends no output file for a stdout target (:34)
crates/buildl-core/src/execute/remote_action.rs — [modify] nothing to strip: the path stays in outputs (:46-47); doc says why
crates/buildl-core/tests/flows/common.rs        — [modify] fakes can return a RemoteHit/Executed carrying stdout for a stdout target (FakeRemote :1229, FakeStrategy :727)
crates/buildl-core/tests/flows/build.rs         — [modify] local stdout-output flows
crates/buildl-core/tests/flows/remote.rs        — [modify] remote stdout-output flows
crates/buildl-core/tests/flows/plan.rs          — [modify] Difference::Stdout flow
crates/buildl-core/tests/flows/golden/graph.json — [modify] every node gains the new field
crates/buildl-lua/src/primitives.rs             — [modify] `stdout` in DISCOVER_FIELDS and a target-only list; test refuses it (:24-62, :72)
crates/buildl/src/report/block.rs               — [modify] failure_text arm for OutputWritten (:150-168)
crates/buildl/src/graph/graph_file.rs           — [modify] fixture Target and expected JSON (:101-102, :140-144)
crates/buildl/src/remote/encode.rs              — [modify] tests only, unless the upload shape needs a change (:265-294, tests :525)
crates/buildl/src/remote/test_server.rs         — [modify] tests for a stdout target round trip (tests :1355)
crates/buildl/tests/fixtures/stdout_output/     — [create] fixture workspace: a target and a discovery target with stdout = true
crates/buildl/tests/e2e/stdout_output.rs        — [create] end-to-end tests over that fixture
crates/buildl/tests/e2e/main.rs                 — [modify] `mod stdout_output;`
crates/buildl-cli/src/exit.rs                   — [modify] OutputWritten → action failure (:47-58); every_class() (:540-570)
crates/buildl-cli/tests/cli/exit_codes.rs       — [modify] exit 1 and exit 5 cases
docs/design.md                                  — [modify] §5, §8.3, §8.7, §11.2, §12.4
docs/architecture.md                            — [modify] §2 sketches, :176 KeyComponents list, §3.8
```

## Order of work

### Task 1 — Declare `stdout` and carry it to the action key

**Files:**
- Modify `crates/buildl-core/src/types/declaration.rs`, `types/build_file.rs`, `types/mod.rs`,
  `load/traversal.rs`, `error.rs`, `resolve/assembly.rs`, `types/target_graph.rs`,
  `execute/assembly.rs`, `types/ready_action.rs`, `plan/components.rs`, `types/action_key.rs`,
  `plan/difference.rs`, `types/plan.rs`
- Modify `crates/buildl-core/src/lib.rs` (re-export, `:197`)
- Modify `crates/buildl-lua/src/primitives.rs`
- Modify `crates/buildl/src/graph/graph_file.rs`, `crates/buildl/src/report/plan_text.rs`
- Modify every other constructor of `Target`, `Node`, `ReadyAction` or `OwnComponents`
  (`git grep -l "network: NetworkAccess::"`), and the literal key JSON in
  `crates/buildl/src/cache/journal.rs:159` and `crates/buildl/src/cache/local_cache.rs:231`
- Test: colocated `#[cfg(test)]` modules of the files above; `crates/buildl-core/tests/flows/plan.rs`;
  the goldens `crates/buildl-core/tests/flows/golden/{graph,plan}.json` and
  `crates/buildl/tests/e2e/golden/chain.graph.json`

**Change:**
- `OutputSource { Command, Stdout }` (doc: where an action's one output comes from), serde-derived
  like `NetworkAccess` (`declaration.rs:48`).
- `StagedTarget`, `Target`, graph `Node`, `ReadyAction` and `OwnComponents` gain an
  `output_source` field, set from `stdout` exactly where `network` is set at each site of the
  `Files` list. In `OwnComponents` it is a required field (no `#[serde(default)]`), so it is an
  action-key component; `KeyComponents` keeps `deny_unknown_fields`.
- The graph JSON view (`target_graph.rs:172-230`) prints it beside `network`.
- `Difference::Stdout` is emitted when two keys' `output_source` differ, placed next to
  `Difference::Network` in the order `difference.rs:30-36` documents. `Difference` is
  `#[non_exhaustive]` (`types/plan.rs:27-29`) and `plan_text.rs:17-35` ends in a catch-all
  (`_ => "something this buildl cannot name changed"`, `:33`), so the new variant compiles
  without wording; `difference_text` gains an explicit arm, `its output now comes from stdout`
  or `its output now comes from the command`, by the new value.
- `convert_target` refuses a target whose `output_source` is `Stdout` and which declares other
  than exactly one output — none (`outputs = {}`) or more than one — as an invalid declaration
  field (a new `DeclarationField` variant for `stdout`), which maps to exit 5 like every
  declaration mistake. A discovery target's fixed single output always passes.
- Lua: `stdout` joins `DISCOVER_FIELDS` and the field list of `buildl.target`; `buildl.test`
  keeps a list without it, so `stdout` on a test is refused as an unknown field. The value is
  read with `flag()` (`values.rs:80`), so a non-boolean is refused as for `network`.

**Proof:** written first, seen failing, then passing —
- `buildl-lua` `primitives.rs` tests: `target_reads_stdout_as_the_output_source` — `stdout = true`
  stages `OutputSource::Stdout`; `target_defaults_every_absent_field` extended — absent is
  `Command`; `a_discovery_target_takes_stdout` — `buildl.discover` accepts it;
  `a_test_refuses_stdout` — `buildl.test` with `stdout = true` is an unknown-field refusal naming
  `stdout`; `stdout_must_be_a_boolean` — `stdout = "yes"` is refused.
- `buildl-core` `traversal.rs` tests: `a_stdout_target_with_two_outputs_is_refused` — the error
  names the target and the `stdout` field; `a_stdout_target_with_no_output_is_refused`;
  `a_stdout_target_with_one_output_is_accepted`.
- `types/plan.rs` per-variant JSON table test (`:501`) gains `Difference::Stdout`.
- `plan_text.rs` test: `a_stdout_difference_is_worded` — neither value renders the catch-all text.
- `action_key.rs` tests: `changing_any_one_component_changes_the_key` gains an output-source case —
  two components differing only in `output_source` have different digests; the doctest at `:113-137` builds `OwnComponents` with
  the field.
- `difference.rs` tests: `a_changed_output_source_is_a_stdout_difference`.
- `tests/flows/plan.rs`: `turning_stdout_on_explains_the_rebuild` — `plan` names
  `Difference::Stdout` for the target.
- `golden/graph.json` regenerated; `graph_file.rs` expected JSON includes the field.

**Done when:** `cargo test -p buildl-core -p buildl-lua -p buildl` passes, `cargo make dod` and
`cargo deny check` are green. Commit `feat(repo): declare stdout = true as a target's output source`.

### Task 2 — Store a local run's stdout as the output

**Files:**
- Modify `crates/buildl-core/src/types/outcome.rs`, `crates/buildl-core/src/execute/perform.rs`,
  `crates/buildl-core/src/execute/fetch.rs` (only the call of `collected` inside `unusable`,
  `:81-89`, which follows the new signature; the remote rule itself is Task 3's)
- Modify `crates/buildl/src/report/block.rs`, `crates/buildl-cli/src/exit.rs`
- Modify `crates/buildl-core/tests/flows/common.rs` (FakeStrategy can report a run's stdout and
  what stands at each path), `crates/buildl-core/tests/flows/build.rs`,
  `crates/buildl-core/tests/flows/verify.rs`
- Test: colocated tests of `perform.rs`, `block.rs`, `exit.rs`

**Change:**
- `OutcomeClass::OutputWritten { name }` (doc: a stdout target's command wrote its output path
  itself) — the action's fault. The tally at `outcome.rs:181` counts it as failed. `OutcomeClass`
  is `#[non_exhaustive]` and the matches in other crates end in catch-alls (`exit.rs:61`
  `_ => Self::Internal`, `block.rs:171`), so each needs an explicit arm, not just a compile:
  `exit.rs:47-58` maps it to the action-failure code (1) and `every_class()` (`:540-570`) gains
  it; `block.rs:150-168` renders `action failure (output {name} is written from stdout, and the command wrote it too)`.
  The task also lists every other `match` on `OutcomeClass` with a `_ =>` arm
  (`git grep -n "OutcomeClass::" crates/buildl crates/buildl-cli`) and gives `OutputWritten` the
  arm its neighbour `OutputMissing` has wherever that one is named.
- In `perform.rs`, `collected`/`examined` (`:307-348`) take the target's `OutputSource`. For
  `Stdout`: `Found::Nothing` at the path is the expected case; anything else is
  `Unkept { class: OutputWritten }`. The output's manifest is then a plain-mode file whose
  digest is that of the captured stdout bytes, which `stored` (`:934`) already puts in the cas,
  so the log row's stdout and the output share one blob. For `Command`, behaviour is unchanged.
- The order stays the crash-safety invariant: cas first, cache row after, log always.

**Proof:** written first, seen failing, then passing —
- `perform.rs` tests: `a_stdout_target_keeps_its_stdout_as_the_output` — collected `Found::Nothing`
  plus stdout `b"x"` gives a file manifest of `x`'s digest, mode plain;
  `a_stdout_target_whose_command_wrote_the_path_fails` — `Found::File` gives `OutputWritten`;
  `a_command_target_is_unchanged` — `Found::Nothing` still gives `OutputMissing`.
- `tests/flows/build.rs`: `a_stdout_target_builds_from_what_it_printed` — the cas holds the
  output under the stdout digest and the cache row's stdout digest equals the output's;
  `a_stdout_target_is_cached_on_the_second_build`; `a_failed_stdout_target_stores_nothing` — a
  non-zero exit records no row.
- `tests/flows/verify.rs`: `verify_reruns_a_stdout_target_and_finds_it_identical`;
  `verify_names_a_stdout_target_that_prints_differently`.
- `block.rs` test: `output_written_names_the_output`; `exit.rs` test:
  `output_written_is_an_action_failure`.

**Done when:** `cargo test -p buildl-core -p buildl -p buildl-cli` passes, `cargo make dod` and
`cargo deny check` are green. Commit `feat(repo): store a stdout target's output from what it printed`.

### Task 3 — Judge and send a stdout target through the remote cache and executor

**Files:**
- Modify `crates/buildl-core/src/execute/fetch.rs`, `crates/buildl-core/src/execute/upload.rs`,
  `crates/buildl-core/src/execute/remote_action.rs` (doc only)
- Modify `crates/buildl-core/tests/flows/common.rs` (FakeRemote returns a hit or an `Executed`
  with `stdout` set), `crates/buildl-core/tests/flows/remote.rs`
- Modify `crates/buildl/src/remote/test_server.rs` tests, `crates/buildl/src/remote/encode.rs`
  tests (and code only if a test shows the upload shape needs it)

**Change:**
- Task 2 kept `collected(role, names, found)` (it now means `OutputsFrom::Command`) and added
  `collected_from(.., &OutputsFrom, ..)` with `OutputsFrom { Command, Stdout(Digest) }` in
  `perform.rs`; the three calls in `fetch.rs` (`:88`, `:323`, `:908`) still use `collected`, and
  this task switches each to `collected_from` with `OutputsFrom::Stdout(digest)` for a stdout target.
- `remote_action` keeps the stdout target's path in `outputs` (`remote_action.rs:46-47`), so the
  REAPI `Command.output_paths` lists it and a server returns a file the command wrote there
  (vendored proto `remote_execution.proto:845-853`). A doc comment says why.
- `usable`, `unusable` and `fetch` (`fetch.rs:43`, `:81`, `:268`) for a stdout target —
  `unusable` is the check `judged` makes before `fetch` (`perform.rs:726`), and today it would
  judge an executed stdout result `OutputMissing`: a result whose `files` or
  `directories` name the path fails as `OutputWritten`, as a local run would; a result with no
  `stdout` blob (`RemoteHit.stdout`, `remote_hit.rs:49`) is unusable and handled as a failed
  lookup; otherwise the output is the blob `stdout` names, downloaded and checked against its
  digest like any fetched output, and stored as a plain-mode file. This holds for a lookup hit
  (`perform.rs:591`) and for `Executed::Ran` and `Executed::Cached` (`perform.rs:675-676` →
  `judged` `:707` → `fetch` `:731`).
- `upload_of` (`upload.rs:34`) sends no output file for a stdout target: the `ActionResult`
  carries the `stdout_digest` only (`encode.rs:265-294`).

**Proof:** written first, seen failing, then passing —
- `fetch.rs` tests: `a_stdout_hit_takes_its_output_from_stdout`;
  `a_stdout_hit_naming_the_path_is_output_written`; `a_stdout_hit_without_stdout_is_unusable`.
- `upload.rs` test: `a_stdout_upload_carries_no_output_file`.
- `tests/flows/remote.rs`: `a_remote_hit_for_a_stdout_target_is_fetched`;
  `an_executed_stdout_target_is_reported_remote` (`Executed::Ran`);
  `a_server_cached_stdout_target_is_reported_fetched` (`Executed::Cached`);
  `a_stdout_result_without_stdout_runs_locally` (without strict: falls back);
  `a_stdout_target_uploads_its_stdout_only`.
- `fetch.rs` test: `an_executed_stdout_result_is_not_judged_output_missing` (through `unusable`).
- `encode.rs` tests (module `:525`): `a_stdout_targets_command_lists_its_path_in_output_paths` —
  the `wire::Command` built at `:108-135` holds the path in `output_paths`;
  `a_stdout_upload_has_a_stdout_digest_and_no_output_files` — the `ActionResult` built at
  `:265-294` has `stdout_digest` set and empty `output_files` and `output_directories`.
- `test_server.rs` tests: `a_stdout_target_round_trips_through_upload_and_lookup`;
  `a_stdout_target_round_trips_through_execution`.

**Done when:** `cargo test -p buildl-core -p buildl --all-features` passes, `cargo make dod` and
`cargo deny check` are green. Commit `feat(repo): fetch, execute and upload a stdout target remotely`.

### Task 4 — Prove it end to end and at the binary

**Files:**
- Create `crates/buildl/tests/fixtures/stdout_output/` — `buildl.toml` (ceiling `proc.run`
  holding `printf`), `build.lua` with a target `printf`-ing text under `stdout = true` and a
  discovery target `printf`-ing two JSON objects under `stdout = true`
- Create `crates/buildl/tests/e2e/stdout_output.rs`; modify `crates/buildl/tests/e2e/main.rs`
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs`

**Change:** no production change; the tests drive the built adapters and binary. System tools
by argv only (`printf`, `sh` in tests only — architecture-building-blocks §8).

**Proof:** written first, seen failing, then passing —
- e2e `a_stdout_output_is_built_twice_byte_identical` — under `sandbox`, `out/<name>` holds the
  printed bytes, and a second checkout builds the same bytes; `the_same_under_bare`;
  `log_still_shows_what_a_stdout_target_printed`.
- CLI `a_stdout_target_with_two_outputs_exits_5`;
  `a_stdout_target_whose_command_writes_the_path_exits_1` — stderr names the output and says it is
  written from stdout; `log_shows_what_a_stdout_target_printed` — `buildl log <target>` prints
  the bytes the output holds (spec §6's `buildl-cli` row).

The fixture's discovery target prints two JSON objects; reading them back with
`buildl.decode_all` is plan 08's, once plan 02 has landed.

**Done when:** `cargo test -p buildl --test e2e` and `cargo test -p buildl-cli --test cli` pass,
`cargo make dod` and `cargo deny check` are green. Commit `test(repo): stdout outputs end to end`.

### Task 5 — Document the field

**Files:** Modify `docs/design.md`, `docs/architecture.md`

**Change:**
- design §5 (`:235`): `stdout = true` in `b.target`'s closed table — one output, from what the
  command prints; on `b.target` and `b.discover`, not `b.test`; an action-key component.
- design §8.3 (`:418-432`): the new component in the key's list and diagram.
- design §8.7 (`:501-538`): the output-capture text says a stdout target's captured stdout is
  also its output, stored once; the failure tables and the exit-code table gain the new exit-1
  cause (the command wrote a stdout target's path) and exit-5 cause (a stdout target with other
  than one output).
- design §11.2 (`:869-880`): a stdout target's path stays in `output_paths`; its output comes from
  `stdout_digest`; a result without it is unusable; an upload carries no output file.
- design §12.4 (`:994-1003`): the ledger gains the output source.
- architecture §2 (`:89`, `:116`, `:176`): `output_source: OutputSource` beside `network` in the
  sketches and the `KeyComponents` list; §3.8 (`:410-507`): the REAPI mapping.
Mermaid stays Mermaid; § cross-references are preserved.

**Proof:** `cargo make dod` green (doc-only); a reader can follow every rule of spec §3 in the docs.

**Done when:** committed `docs(repo): document stdout = true`.

## Risks

- **Most risky: Task 3.** `usable` today requires the hit to name exactly the declared paths
  (`fetch.rs:43-60`); a stdout target inverts that for its one path. A mistake there makes a
  correct remote result look unusable (silent local fallback) or accepts a wrong one. The
  named tests cover both directions.
- **Matches the compiler will not flag.** `OutcomeClass` and `Difference` are `#[non_exhaustive]`
  and every match on them outside `buildl-core` ends in a catch-all (`exit.rs:61`, `block.rs:171`,
  `plan_text.rs:33`), so a missing arm compiles and prints the wrong text or the wrong exit code.
  Tasks 1 and 2 add the arms explicitly and test the text; the `git grep` sweep in Task 2 covers
  `crates/buildl/src/report/{terminal,silent,verify_text,log_text}.rs`,
  `crates/buildl/src/log/local_log.rs`, `crates/buildl/src/workspace.rs` and `lib.rs`.
- **Files list is large.** About 40 files construct the four structs with `network:` (mostly
  tests); Task 1 touches each with one line.
- **Key digest change.** Every key changes once; the golden `graph.json`/`plan.json` and any test
  asserting a literal digest are updated in Task 1. An existing local `.buildl/cache.json` is
  refused once with its own "remove cache.json" instruction (spec §1) — accepted, not handled.
- **Chose not to:** add a new `Found` or adapter behaviour; change `OutputRefusal`; support
  `stdout` on `buildl.test`.

## Proof

- `cargo make dod` and `cargo deny check` green after every task.
- `cargo test -p buildl --test e2e stdout_output` and `cargo test -p buildl-cli --test cli` pass.
- Plan 04's `go-discovery` example builds on Linux and macOS in CI with `stdout = true`.
