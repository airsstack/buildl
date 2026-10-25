---
status: approved
created: 2026-10-25
depends-on: [01, 04, 07]
built-on: 3d9113128fe5
---

# Remote CLI Implementation Plan

**Goal:** `buildl build` and `buildl test` use the remote cache `buildl.toml` names, under the switches spec §6 defines.

**Architecture:** `LocalPorts` (`local_ports.rs:50`) names `type Remote = remote::Remote`, an
enum of the `buildl` adapter that is `Off` (behaving as `NoRemote`) or `Reapi(Reapi)`, built by
`Wiring::ports` (`local_ports.rs:364`) from `Workspace.remote` (plan 01) when the policy reads,
with the channel connecting lazily so a server that is down is a lenient failure at its first
call, not a startup error. The switches are resolved in `buildl-cli`: `main.rs` reads
`BUILDL_REMOTE_UPLOAD` and `BUILDL_REMOTE_STRICT` beside `TERM` and `NO_COLOR` (`main.rs:96-101`),
a new `remote_switches.rs` combines them with the flags into a `RemotePolicy`, and `RunOptions`
(`workspace.rs:38`) carries it to the pipeline. `doctor` prints the settings and the switches.
The last task runs one real server.

**Tech Stack:** Rust, `buildl`, `buildl-cli` (clap derive), `cargo make dod`,
`cargo deny check`; `bazel-remote` 2.6.4 from Homebrew for Task 4 only. Guideline:
`claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §6 (switch table, usage errors, token missing, `doctor`, commands
that never connect), §7, §10 (CLI row, real-server row), P29.

---

## Files that change

```
crates/buildl/src/remote/configured.rs        — [create] Remote: Off | Reapi(Reapi), implementing RemoteCache; Remote::of(settings, policy, root)
crates/buildl/src/remote/mod.rs               — [modify] declare and export
crates/buildl/src/local_ports.rs              — [modify] `type Remote = remote::Remote`; ports() builds it from Workspace.remote and the policy
crates/buildl/src/workspace.rs                — [modify] RunOptions gains the RemotePolicy; build (:353) and test (:545) pass it; the token is read before anything runs
crates/buildl/tests/e2e/remote.rs             — [create] the facade against TestServer
crates/buildl/tests/e2e/main.rs               — [modify] mod remote
crates/buildl-cli/src/remote_switches.rs      — [create] RemoteSwitches from the two variables and the flags → RemotePolicy or a usage error; unit tests
crates/buildl-cli/src/cli.rs                  — [modify] RemoteArgs (`--remote-upload`, `--remote-strict`, `--no-remote`) flattened into BuildArgs (:153) and TestArgs (:171)
crates/buildl-cli/src/main.rs                 — [modify] read the two variables (:96-101)
crates/buildl-cli/src/run.rs                  — [modify] build (:315), test (:345) resolve the switches; doctor (:288) passes them to the text
crates/buildl-cli/src/doctor_text.rs          — [modify] the remote section
crates/buildl-core/src/error.rs               — [modify] Error::RemoteSwitch { name, value } (a variable neither `1` nor `0`); Error::RemoteFlags { reason } (a flag combination spec §6 refuses)
crates/buildl-cli/src/exit.rs                 — [modify] RemoteSwitch, RemoteFlags and RemoteToken (plan 06) → the usage exit, beside :97-102
crates/buildl-cli/Cargo.toml                  — [modify] dev-dependency `buildl` with feature `test-server`
crates/buildl-cli/tests/cli/remote.rs         — [create] the binary against TestServer
crates/buildl-cli/tests/cli/main.rs           — [modify] mod remote
crates/expected-edges.txt                     — [modify] only if `cargo make guard-crate-edges` reports a change
```

## Order of work

### Task 1 — The switches

**Files:** `remote_switches.rs`, `cli.rs`, `main.rs`, `buildl-core/src/error.rs`, `exit.rs`.

**Change:** `RemoteArgs` holds the three flags, flattened into `BuildArgs` and `TestArgs`.
`RemoteSwitches::resolve(upload_var, strict_var, args, has_remote)` → `RemotePolicy`: each
variable `1` or `0`, anything else `Error::RemoteSwitch` naming the variable; a flag turns its
switch on whatever its variable says; `--no-remote` → `RemotePolicy::OFF` whatever the variables
say; no `[remote]` table → `OFF`, but either flag without one, or `--no-remote` with either
flag, is `Error::RemoteFlags` (spec §6). Reading with upload → `ReadWrite`; strict → `Strict`.
`exit.rs` maps both errors, and plan 06's `Error::RemoteToken`, to the usage exit (spec §7).

**Proof:** written first, seen failing, then passing —
- `no_switch_reads_leniently` — `Read`, `Lenient`
- `the_upload_variable_turns_upload_on` — `BUILDL_REMOTE_UPLOAD=1` → `ReadWrite`
- `the_upload_flag_overrides_a_zero_variable` — `=0` plus `--remote-upload` → `ReadWrite`
- `no_remote_overrides_the_variables` — both variables `1`, `--no-remote` → `OFF`
- `no_remote_with_an_upload_flag_is_refused` — `Error::RemoteFlags`
- `a_flag_without_a_remote_table_is_refused` — `Error::RemoteFlags`
- `a_variable_other_than_one_or_zero_is_refused_naming_it` — `yes` → `Error::RemoteSwitch` naming `BUILDL_REMOTE_UPLOAD`
- `the_three_errors_exit_as_usage` (exit.rs)
- `the_flags_parse_on_build_and_test` (cli.rs unit, beside `:562`) — each flag sets its field on both commands

**Done when:** `cargo test -p buildl-cli` passes; `cargo make dod` green. Commit
`feat(buildl-cli): remote cache switches`.

### Task 2 — Wiring the adapter

**Files:** `remote/configured.rs`, `remote/mod.rs`, `local_ports.rs`, `workspace.rs`, `run.rs`,
`tests/e2e/remote.rs`, `tests/e2e/main.rs`.

**Change:** `Remote::of` returns `Off` when the policy is `OFF` or the workspace names no
remote, and otherwise connects a `Client` lazily and reads the token, so a missing token is an
error before anything runs (spec §6). `RunOptions` carries the policy; `build`/`test` hand it
to the pipeline with `Pipeline::set_remote(policy)` before running (`RemotePolicy::OFF` until now). `plan`, `verify`,
`query`, `graph`, `check`, `log`, `clean`, `gc` and `doctor` keep `RemotePolicy::OFF` and never
build a `Reapi`.

**Proof:** written first, seen failing, then passing (e2e through `Workspace`, `--all-features`) —
- `a_second_checkout_fetches_what_the_first_uploaded` — workspace A builds with `ReadWrite` against `TestServer`; workspace B (same sources, empty `.buildl`) builds with `Read`: every target `Fetched`, no action run, outputs byte-identical
- `a_server_that_is_down_warns_and_builds_locally` — url of a closed port: one notice; every target `Built`
- `a_missing_token_fails_before_anything_runs` — `Error::RemoteToken`; no outcome logged
- `verify_never_connects` — `TestServer` records no call during `verify`

**Done when:** `cargo test -p buildl --all-features --test e2e remote` passes; `cargo make dod`,
`cargo make guard-adapter-isolation` green. Commit `feat(buildl): use the remote cache buildl.toml names`.

### Task 3 — `doctor` and the binary

**Files:** `doctor_text.rs`, `run.rs`, `buildl-cli/Cargo.toml`, `tests/cli/remote.rs`,
`tests/cli/main.rs`.

**Change:** `doctor` prints a `remote` section: `url`, `instance`, `ca`, `header`, `timeout`,
`token: set | not set` (the variable present, or the file readable — never its content), and
`upload` and `strict` as this machine's variables set them, with a note that
`--remote-upload` and `--remote-strict` turn either on for one run; a workspace with no table
prints `remote: none`. `doctor` connects to nothing.

**Proof:** written first, seen failing, then passing (the binary, `tests/cli/common.rs:86`
`run`, against `TestServer`) —
- `build_reports_fetched_targets_in_the_summary` — second checkout: the summary counts every target fetched
- `remote_upload_seeds_the_server` / `the_upload_variable_seeds_the_server` — the server holds an action result afterwards
- `no_remote_builds_without_connecting` — the server records no call
- `a_strict_failure_exits_as_infrastructure` — closed port, `--remote-strict`: infrastructure exit code
- `a_missing_token_exits_as_usage`
- `uploading_under_bare_is_refused_with_the_usage_exit`
- `a_token_with_a_grpc_url_is_refused` — manifest error exit, key and position named
- `doctor_prints_the_remote_section` — every field; token shown only as `set`
- `the_token_appears_in_no_output_and_not_in_the_log` — stdout, stderr and `.buildl` log grepped for the token after a build, a strict failure and `doctor`
- `a_bad_upload_variable_exits_as_usage` — `BUILDL_REMOTE_UPLOAD=yes`: usage exit, the variable named

**Done when:** `cargo test -p buildl-cli --all-features` passes; `cargo make dod`,
`cargo deny check`, `cargo make guard-crate-edges` green. Commit
`feat(buildl-cli): remote cache in build, test and doctor`.

### Task 4 — One real server

**Files:** none in the repository; the evidence goes in this plan's execution record.

**Change:** none. With the author's go-ahead to install it, `brew install bazel-remote`
(2.6.4, P29); start it on loopback with gRPC on and a temporary directory, its flags read from
`bazel-remote --help` at the time; in a scratch workspace with `[remote] url = "grpc://127.0.0.1:<port>"`,
run `buildl build --remote-upload`, then in a second checkout of the same sources
`buildl build`; then stop the server.

**Proof:** the second build's summary shows every target fetched and none built, the outputs
are byte-identical to the first checkout's, and bazel-remote's log shows no rejected call. Any
refusal found is fixed in this chain (spec §12) before the plan is done.

**Done when:** the evidence is recorded. No commit.

## Risks

- **Most risky: Task 4**, the first contact with a server buildl did not write; a misread of the
  protocol shows here, and is closed in this chain.
- `BuildArgs` gains three bools; `TestArgs` already carries `clippy::struct_excessive_bools`
  (`cli.rs:166-169`); grouping the flags in `RemoteArgs` keeps the count down.
- Installing bazel-remote changes the author's machine: Task 4 asks first.

## Proof

`cargo make dod`, `cargo deny check` and every `guard-*` task green; through the binary, a
second checkout builds entirely from what the first uploaded, against the test server and once
against bazel-remote.
