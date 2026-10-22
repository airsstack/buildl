---
status: done
created: 2026-10-22
depends-on: [02]
built-on: 5d10ef9b2f2e
---

# Approver Modes Implementation Plan

**Goal:** `buildl` answers the approver port in the mode `buildl.toml`'s `[approver]` table names: `manifest`, `interactive` or `deny`.

**Architecture:** All in the `buildl` adapter module `approver/` (made in plan 02), except
`name_of`, which moves into `state` so `approver` and `state/scratch.rs` share it (spec §8; an
adapter may name only `state` and `search_path`, `Makefile.toml:209-287`). `LocalApprover<C>`
is the one associated type (spec §4), so `LocalPorts` gains the parameter `C: Console`.
`Workspace::build/test/verify` take `&mut C` as they take `&mut R` for the reporter; `plan`
uses `NoTerminal`, a `Console` that is never a terminal, since plan never asks. Where approvals
are kept is resolved from the environment when first needed; `Workspace::with_data_dir`
replaces it, because a test cannot set environment variables in-process (`unsafe_code =
"forbid"`, and `set_var` is `unsafe` in edition 2024). The CLI adds `StdConsole`.

**Tech Stack:** Rust, `buildl`, `buildl-cli`, `std::io::IsTerminal`, `cargo make dod`,
`cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines` (static dispatch,
strong types, `mod.rs` export-only, colocated unit tests).

**Content authority:** spec §1 P6, P8, P11; §4 (Modes, Interactive, Approvals file); §5 rows
`NotApproved`, `CorruptState`, `Adapter { Approver }`; §8 (`buildl`, `buildl-cli` rows); §9
(`buildl` e2e row, CLI rows for `interactive` and `deny`).

---

## Files that change

```
crates/buildl/src/manifest/schema.rs        — [modify] Raw gains `approver: ApproverTable { default }`, deny_unknown_fields
crates/buildl/src/manifest/interpret.rs     — [modify] `approver` table → Workspace.approver
crates/buildl/src/manifest/toml_manifest.rs — [modify] tests: `[approver]` accepted; bad key/value refused with line:col
crates/buildl/src/state/name.rs             — [create] `name_of`, moved from state/scratch.rs:45
crates/buildl/src/state/scratch.rs          — [modify] use state's `name_of`
crates/buildl/src/state/mod.rs              — [modify] `mod name;`, `pub use name::name_of`
crates/buildl/src/approver/mod.rs           — [modify] declare and re-export the new items
crates/buildl/src/approver/data_dir.rs      — [create] DataDir: the user's data directory or one given
crates/buildl/src/approver/approvals.rs     — [create] Approvals: the remembered approvals file, read and written
crates/buildl/src/approver/console.rs       — [create] trait Console; NoTerminal
crates/buildl/src/approver/deny.rs          — [create] DenyApprover
crates/buildl/src/approver/interactive.rs   — [create] InteractiveApprover<C: Console>
crates/buildl/src/approver/local.rs         — [create] LocalApprover<C>: Manifest | Deny | Interactive
crates/buildl/src/local_ports.rs            — [modify] LocalPorts<R, C>; Wiring picks the approver from the mode
crates/buildl/src/workspace.rs              — [modify] build/test/verify take `&mut C`; with_data_dir; docs
crates/buildl/src/lib.rs                    — [modify] re-exports; crate docs
crates/buildl/tests/e2e/approver.rs         — [create] each mode through Workspace with a fake Console
crates/buildl/tests/e2e/main.rs             — [modify] `mod approver;`
crates/buildl/tests/e2e/common.rs           — [modify] Fixture build/test helpers pass a console and a data dir
crates/buildl/tests/e2e/*.rs                — [modify] only call sites of build/test/verify, if they call Workspace directly
crates/buildl-cli/src/console.rs            — [create] StdConsole over the real stdin and stderr
crates/buildl-cli/src/main.rs               — [modify] `mod console;`, hand StdConsole to run
crates/buildl-cli/src/run.rs                — [modify] build/test/verify pass the console
crates/buildl-cli/tests/cli/exit_codes.rs   — [modify] interactive under piped stdin, and deny, exit 4
crates/buildl-cli/tests/cli/common.rs       — [modify] a run helper that sets HOME / XDG_DATA_HOME for the child
```

## Order of work

### Task 1 — `[approver]` in `buildl.toml`

**Files:** `manifest/schema.rs`, `manifest/interpret.rs`, `manifest/toml_manifest.rs`.

**Change:** `Raw` (`schema.rs:51`) gains `approver: ApproverTable` (`#[serde(default,
deserialize_with = "table")]`, as the other four); `ApproverTable` is
`#[serde(deny_unknown_fields)]` with `default: Option<Spanned<String>>`. `interpret::workspace`
(`:60-68`) sets `approver` from `optional(text, "approver.default", ..., ApproverMode::parse)`
`.unwrap_or_default()`, the same shape as `execution.strategy` (`:136-137`), replacing plan 02's
fixed `ApproverMode::Manifest`. (Amended 2026-10-22, execution: no module doc lists the
tables, so none changes. `toml_manifest.rs`'s `DESIGN_EXAMPLE` gains the `[approver]` table,
which it left out only because the reader refused it.)

**Proof:** written first, seen failing, then passing (`toml_manifest.rs` tests) —
- `no_approver_table_means_manifest`
- `each_approver_mode_is_read` (`manifest`, `interactive`, `deny`)
- `an_unknown_approver_mode_is_refused_at_its_position` (`default = "ask"` → located error
  naming `approver.default`)
- `an_unknown_key_in_the_approver_table_is_refused_at_its_position`
- the existing `an_unknown_key_is_refused_at_its_position` loses its `[approver]` case
  (`toml_manifest.rs:321-322`), and its "expected one of" texts gain `approver`.

**Done when:** `cargo test -p buildl manifest::` passes and `cargo make dod` is green. Commit
`feat(buildl): read the approver mode from buildl.toml`.

### Task 2 — Where approvals are remembered

**Files:** `state/name.rs`, `state/scratch.rs`, `state/mod.rs`, `approver/data_dir.rs`,
`approver/approvals.rs`, `approver/mod.rs`.

**Change:**
- `name_of(bytes: &[u8]) -> String` moves from `state/scratch.rs:45` (with `NAME_DIGITS`,
  `:39`) to `state/name.rs`, `pub`, re-exported from `state`; `scratch.rs` uses it; no body
  change.
- `DataDir` — `User` (the default) or `At(PathBuf)`. `resolve` returns the directory:
  for `At`, that path; for `User`, `$XDG_DATA_HOME` when set and not empty, else
  `$HOME/.local/share`, else `Error::Adapter { port: PortName::Approver }` saying neither is
  set. The variable lookup is a parameter (a function from name to value), so tests give it a
  map; production passes `std::env::var_os`.
- `Approvals` — the file `<data>/buildl/approved/<name_of(root bytes)>.json`, the root's bytes
  taken as `Scratch::new` takes them (`root.as_os_str().as_encoded_bytes()`, `scratch.rs:75`),
  so one workspace has one name in both places.
  - `read(&self) -> Result<WantedSet>`: absent → empty set; read with
    `buildl_core::json::canonical::from_slice` (`json/canonical.rs:129`); bytes that do not
    parse or are not canonical → `Error::CorruptState { file, source }` (as
    `cache/journal.rs:88`) — its message names the file (`error.rs:89`), which is the "remove
    it" remedy of spec §4: no extra text, as for the other files buildl writes; any other I/O
    error → `Error::Adapter { port: Approver }` naming the
    file.
  - `write(&self, &WantedSet) -> Result<()>`: makes the directory, writes the canonical bytes
    with `state::write_whole` (`state/write.rs:57`) staged in that same directory
    (`Access::Rewritable`); any failure → `Error::Adapter { port: Approver }` naming the file.

**Proof:** written first, seen failing, then passing —
- `state/name.rs`: `name_of` gains tests of its own (amended 2026-10-22, execution: it had none
  to move; `scratch.rs` reached it only through `Scratch`); `scratch.rs` tests pass unchanged.
- `approver/data_dir.rs`: `xdg_data_home_wins`, `an_empty_xdg_data_home_is_ignored`,
  `home_gives_its_local_share`, `neither_set_is_an_approver_error`, `a_given_directory_is_used_as_is`.
- `approver/approvals.rs` (temp dirs): `the_file_is_named_for_the_workspace_root`,
  `two_roots_have_two_files`, `a_missing_file_reads_as_nothing_approved`,
  `what_is_written_reads_back`, `the_file_is_canonical_json`,
  `a_file_buildl_did_not_write_is_corrupt_state_naming_it`,
  `an_unwritable_directory_is_an_approver_error`.

**Done when:** `cargo test -p buildl -- state:: approver::` passes,
`cargo make guard-adapter-isolation` is OK and `cargo make dod` is green. Commit
`feat(buildl): remember approvals outside the workspace`.

### Task 3 — The deny and interactive approvers

**Files:** `approver/console.rs`, `approver/deny.rs`, `approver/interactive.rs`,
`approver/local.rs`, `approver/mod.rs`.

**Change:**
- `Console` — `fn is_terminal(&self) -> bool` (both the stream asked on and the one read from
  are terminals, spec §4) and `fn ask(&mut self, prompt: &str) -> io::Result<String>` (write
  the prompt, read one line). A blanket `impl Console for &mut C`. `NoTerminal` — never a
  terminal; `ask` returns an error and is never reached. Tests in `console.rs`:
  `no_terminal_is_never_a_terminal_and_cannot_ask`, `a_borrowed_console_is_the_console`.
- `DenyApprover` — `Denied { unapproved: wanted.clone(), reason: "the workspace's approver is
  deny" }`.
- `InteractiveApprover<C: Console>` — holds the console, `Approvals` and `DataDir` resolution
  (resolved on first `approve`, so a `manifest` workspace never needs `HOME`). `approve` follows
  spec §4's diagram exactly: remembered = `read`; new = `wanted.minus(&remembered)`; new empty →
  `Granted`, nothing asked; `!is_terminal()` → `Denied { unapproved: new, reason: "no terminal
  to ask on; run the build in a terminal once to approve, or set [approver] default =
  \"manifest\"" }`; else `ask` with `"this build wants to\n"`, then `new`'s `Display` (plan 02,
  task 1 — the one rendering), then `"allow? [y/N] "`; an `ask` I/O error →
  `Error::Adapter { port: Approver }`; the trimmed answer `y` or `yes` in any case →
  `write(&remembered.union(wanted))?` then `Granted`; anything else → `Denied { unapproved:
  new, reason: "not approved" }`.
- `LocalApprover<C: Console>` — `Manifest(ManifestApprover)`, `Deny(DenyApprover)`,
  `Interactive(InteractiveApprover<C>)`; `Approver` dispatches by match (static, as `Local`
  does for strategies, `exec/local.rs:32`).

**Proof:** written first, seen failing, then passing (a scripted fake console in the test
module: answers, terminal flag, prompts seen) —
- `deny.rs`: `deny_refuses_everything_naming_it`.
- `interactive.rs`: `nothing_new_is_granted_without_asking`,
  `a_yes_is_remembered_and_granted`, `yes_in_any_case_is_a_yes`,
  `anything_else_is_not_approved_and_nothing_is_remembered` (`""`, `n`, `yeah`),
  `the_prompt_lists_only_the_new_wants`, `the_prompt_is_the_same_text_every_time`,
  `no_terminal_refuses_with_the_remedy_and_never_asks`,
  `a_failed_write_after_yes_is_an_error_not_a_grant` (amended 2026-10-22, execution: its
  setup must let the read find no file and make only the write fail, so the prompt is shown
  and the write is reached), `a_yes_is_remembered_with_what_was_already_allowed` (added 2026-10-22: a yes keeps
  what was remembered before),
  `a_corrupt_file_is_an_error_before_anything_is_asked`.
- `local.rs`: `each_mode_answers_as_its_approver`.

**Done when:** `cargo test -p buildl approver::` passes,
`cargo make guard-adapter-isolation` is OK and `cargo make dod` is green. Commit
`feat(buildl): add the deny and interactive approvers`.

### Task 4 — Wire the approver into the workspace and the CLI

**Files:** `local_ports.rs`, `workspace.rs`, `lib.rs`, `tests/e2e/common.rs`,
`tests/e2e/*.rs` (call sites only), `tests/e2e/approver.rs`, `tests/e2e/main.rs`,
`buildl-cli/src/console.rs`, `buildl-cli/src/main.rs`, `buildl-cli/src/run.rs`.

**Change:**
- `LocalPorts<R, C>` (`local_ports.rs:43`), `type Approver = LocalApprover<C>`;
  `LocalPipeline<R, C>`. `Wiring` holds a `DataDir` (`User` from `read`). `Wiring::ports` and
  `verify_ports` (`:302`, `:321`) take the console and pass `assemble` the `LocalApprover` the
  workspace's `approver` mode names (`Interactive` built with the root and the `DataDir`).
- `Workspace::build`, `test` and `verify` (`workspace.rs:205`, `:341`, `:238`) gain
  `console: &mut C` with `C: Console` as their last parameter (`verify` has no reporter,
  `workspace.rs:238-243`); `plan` (`:181`) passes `NoTerminal`. (Amended 2026-10-22,
  execution: with `console` added, `test` passes clippy's `too_many_arguments` limit, so the
  plain value flags `build` and `test` take are grouped into one options value; reporter and
  console stay separate `&mut` parameters, as spec §8 words it.)
  `Workspace::with_data_dir(self, dir: impl Into<PathBuf>) -> Self` sets `DataDir::At`. Their
  `# Errors` docs add `Error::NotApproved` and the approvals file's errors. `lib.rs`
  re-exports `Console`, `NoTerminal` and `DataDir` if public API needs them, and its crate docs
  say the approver is chosen in `buildl.toml`.
- `tests/e2e/common.rs`: `Fixture`'s build/test/verify helpers pass a console (a fake that is
  no terminal unless a test says otherwise) and a data dir under the fixture's temp dir, so no
  test touches the real home.
- `buildl-cli/src/console.rs`: `StdConsole` — `is_terminal` is
  `io::stdin().is_terminal() && io::stderr().is_terminal()` (P8); `ask` writes the prompt to
  stderr, flushes, reads one line from stdin. `main.rs` makes it and hands it to `run` as it
  hands the writers (`main.rs:85-91`); `run.rs` passes it to `build`/`test`/`verify`
  (`:271`, `:298`, `:319`). The module carries a unit test only for logic it holds; reading the
  real streams is exempt as `main.rs` is.

**Proof:** written first, seen failing, then passing — `tests/e2e/approver.rs`, through
`Workspace` on a copy of the `chain` fixture with an `[approver]` table and a fake console:
- `manifest_builds_without_asking`
- `interactive_yes_builds_and_remembers` — the file is under the data dir, not under the
  workspace root, and a second build asks nothing
- `interactive_asks_only_about_new_wants` — a build file change adds `env = { "X" }` and the
  manifest's `env.read` gains `X` (else plan refuses it before the approver is asked); the
  second prompt lists `read variables: X` and nothing else
- `interactive_no_is_refused_and_builds_nothing` (`Error::NotApproved`; no output, no row)
- `interactive_with_no_terminal_is_refused_with_the_remedy`
- `a_corrupt_approvals_file_is_corrupt_state` (exit class 6 via `Error::CorruptState`)
- `an_unwritable_approvals_file_after_yes_refuses_and_builds_nothing`
- `deny_refuses_build_test_and_verify`
- `plan_asks_nothing_under_deny`

**Done when:** `cargo test -p buildl --test e2e` and `cargo test -p buildl-cli` pass,
`cargo make guard-adapter-isolation` is OK and `cargo make dod` is green. Commit
`feat(repo): ask the approver the workspace names before building` (scope `repo`: it edits
`buildl-cli` too).

### Task 5 — The refusals at the binary

**Files:** `buildl-cli/tests/cli/exit_codes.rs`, `buildl-cli/tests/cli/common.rs`.

**Change:** `common.rs` gains a run helper that sets `HOME` and `XDG_DATA_HOME` for the child
to a temp directory (child env only; nothing in-process).

**Proof:** written first, seen failing, then passing —
- `interactive_with_piped_stdin_exits_four_with_the_remedy` — stdin piped, stderr captured:
  exit 4; stderr holds `no terminal to ask on` and `[approver] default = "manifest"` and the
  wants listed in the prompt's layout.
- `deny_exits_four` — exit 4, stderr names `deny`.
- `a_corrupt_approvals_file_exits_six`.

**Done when:** `cargo test -p buildl-cli` passes and `cargo make dod` is green. Commit
`test(buildl-cli): exit codes of a build that was not approved`.

---

## Risks

- **Most risky: task 4's signature change** of `Workspace::build/test/verify` reaches every
  e2e helper and the CLI at once. `common.rs` helpers absorb most call sites; `cargo check
  --workspace --all-targets` finds the rest.
- The approvals file must never land in the workspace: `interactive_yes_builds_and_remembers`
  asserts the path is outside the root.
- A real `HOME` must never be written by tests: every e2e build goes through
  `Fixture`'s data dir, and the CLI tests set the child's `HOME`. A test that forgets would
  write `~/.local/share/buildl/approved/` only under `interactive`, which only these tests use.
- `StdConsole::is_terminal` reads both streams; under `cargo test` neither is a terminal, so a
  CLI test can never block on a prompt.
- Not done here: `doctor` (plan 04), documents (plan 05).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-adapter-isolation` OK: `approver` names only `state` and `buildl_core`.
- Every existing e2e and CLI test passes with only call-site changes.

## Review findings

Commits: `cdd481f` (task 1), `e401b52` (task 2), `92b53f6` (task 3), `0ad7c3c` (task 4),
`308bee9` (task 5). Tasks 1 and 2 ran in parallel and shared one review.

- security — `DataDir` accepted an empty `HOME` or a relative `XDG_DATA_HOME`, so the approvals
  file could resolve inside the workspace — task 2 — fixed: only absolute values count; a
  relative `XDG_DATA_HOME` falls back to `HOME`; tests for each case.
- test — the approvals file's "cannot be read" branch had no test — task 2 — fixed: a directory
  in the file's place gives `Adapter { Approver }` naming the file.
- doc-comment / modularity — `write`'s `# Errors` left out the serializer error; an unreachable
  `"."` fallback; an empty `WantedSet` built by hand (`WantedSet` now derives `Default`);
  `name_of`'s docs said "directory" only — task 2 — fixed.
- test — `DESIGN_EXAMPLE` left out the `[approver]` table the reader now accepts — task 1 —
  fixed, asserts `Interactive`; a duplicate unknown-key case removed.
- plan — task 1's module-doc line and task 2's "existing `name_of` tests" were wrong; plan 05
  did not name the design's "unknown key" sentence or `DESIGN_EXAMPLE` — amended.
- test — `a_failed_write_after_yes_is_an_error_not_a_grant` failed at the read (the reviewer's
  probe: `NotADirectory notfound=false`) and never reached the write — task 3 — fixed and
  verified: the `approved` directory is made read-only; with the write's error ignored the test
  failed; `cargo test -p buildl approver::interactive` → `test result: ok. 10 passed; 0 failed`.
- test — nothing checked that a yes keeps what was approved before — task 3 — fixed and
  verified: `a_yes_is_remembered_with_what_was_already_allowed`; writing only the new wants
  made it fail.
- doc-comment — "when the first build asks" — task 3 — fixed.
- design — the first cut of task 4 bundled reporter and console into a public `Lent` value to
  pass clippy's `too_many_arguments`; spec §8 says build, test and verify take the console as
  they take a reporter — task 4 — redesigned: a `RunOptions` value groups `on_failure` and
  `live`; reporter and console stay separate `&mut` parameters; plan amended.
- doc-comment — a garbled sentence in `console.rs`, a 125-column line in `run.rs`, the
  `buildl` README's Facade row, "`deny` refuses every build", three "wants" — task 4 — fixed.
- test — nothing guarded "both stdin and stderr are terminals" — task 4 — fixed:
  `both_terminals` and `one_terminal_alone_is_not_enough_to_ask`.
- test — nothing automated shows the CLI hands its console, not `NoTerminal`, to a build:
  every test run has no terminal — task 4 — accepted, unguarded.
- modularity — the e2e `Fake` console repeats the crate's test-only `Scripted` — task 4 —
  accepted: the crate boundary hides `Scripted`.
- test / doc — `run`'s doc said stdin was piped; `approved_by` spelled `PathBuf` in full;
  `deny_exits_four` matched only `deny` — task 5 — fixed: it matches spec §4's reason
  `the workspace's approver is deny`; `cargo test -p buildl-cli --test cli exit_codes` →
  `test result: ok. 40 passed; 0 failed`.

## Probe results

- Claim: the plan's sites exist — `name_of` at `state/scratch.rs:45`, `Raw` at
  `manifest/schema.rs:51`, the `[approver]`-refused case at `toml_manifest.rs:321`,
  `Workspace::plan`/`build`/`verify`/`test` at `workspace.rs:181`/`:205`/`:238`/`:341`,
  `write_whole` at `state/write.rs:57`, `canonical::from_slice` at `json/canonical.rs:129` —
  `grep -n` — held.
- Claim: `name_of` had tests to move — against the plan: it had none (`scratch.rs` reached it
  only through `Scratch`).

## Deviations

- 2026-10-22 — task 4: `build` and `test` take a `RunOptions` value (on failure, live output)
  in place of those two parameters, so adding the console stays within clippy's
  `too_many_arguments`; reporter and console are separate `&mut` parameters as planned.
- 2026-10-22 — task 3: a test was added, `a_yes_is_remembered_with_what_was_already_allowed`.
- 2026-10-22 — task 1: `DESIGN_EXAMPLE` gained the `[approver]` table.
