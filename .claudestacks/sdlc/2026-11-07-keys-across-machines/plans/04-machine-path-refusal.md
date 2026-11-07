---
status: approved
created: 2026-11-07
depends-on: [01, 02]
built-on: 55ab2847b726
---

# Machine Path Refusal Implementation Plan

**Goal:** buildl refuses a command, a `key` variable's value or a discovery output that holds this machine's own folders.

**Architecture:** A core type `MachineFolders` holds this machine's home, workspace and sandbox
folders and finds one in a text by spec §6's match rule. The `Host` port gains
`machine_folders()`; `LocalHost` builds it from an injected `HOME` value, the workspace root and
the sandbox folder, each as given and canonical. The shared load functions of
`crates/buildl-core/src/pipeline/driver.rs` (`check` `:606`, `graph` `:655`, `query` `:684`, and
`load_twice`/`results`, which `plan`, `build`, `test`, `verify`, `watch` and `gc`
(`pipeline/gc.rs:78-79`) also go through) and `pipeline/doctor.rs` (`:41`) take a
`&MachineFolders`; they check every command word after loading (E5) and every discovery output as
it is read back for the second pass (E7). So every command that loads the build files refuses,
`gc` included. `plan/components.rs` checks each `key` variable's value as it gathers it (E6).

**Tech Stack:** Rust 1.94, `buildl-core` (pure), `buildl` (the `LocalHost` adapter, the workspace
composition), `buildl-cli` (exit codes).

**Content authority:** `spec.md` §6, §7 (E5, E6, E7).

---

## Files that change

```
crates/buildl-core/src/types/machine_folders.rs — create: MachineFolders, the match rule, longest match
crates/buildl-core/src/types/mod.rs             — modify: export it
crates/buildl-core/src/ports/host.rs            — modify: Host::machine_folders()
crates/buildl-core/src/ports/port_set.rs        — modify: test stub returns no folders (:156)
crates/buildl-core/src/ports/bundle.rs          — modify: the doctest's `impl Host for Nothing` (:76)
crates/buildl-core/tests/flows/common.rs        — modify: FakeHost returns given folders (:182)
crates/buildl/src/report/plan_text.rs           — modify: its test `Machine` host (:193)
crates/buildl-core/src/load/machine_paths.rs    — create: check declarations' commands and discovered outputs
crates/buildl-core/src/load/mod.rs              — modify: declare it
crates/buildl-core/src/pipeline/driver.rs       — modify: check/graph/query/load_twice/results take &MachineFolders
crates/buildl-core/src/pipeline/doctor.rs       — modify: doctor takes &MachineFolders (:41)
crates/buildl-core/src/pipeline/gc.rs           — modify: pass the folders to its load (:78-79)
crates/buildl-core/src/plan/components.rs       — modify: check key variable values (E6)
crates/buildl-core/src/error.rs                 — modify: E5 MachinePathInCommand, E6 MachinePathInVariable, E7 MachinePathInDiscovery, exit 5
crates/buildl/src/host/local_host.rs            — modify: build MachineFolders from an injected HOME, root, sandbox folder
crates/buildl/src/local_ports.rs                — modify: hand root and sandbox folder to LocalHost (:104)
crates/buildl/src/workspace.rs                  — modify: pass the host's folders to check/graph/query/doctor (:252-254)
crates/buildl-cli/tests/cli/exit_codes.rs       — test: E5, E6, E7 exit 5 through the binary
```

## Order of work

### Task 1 — Add `MachineFolders` and its match rule

**Files:** Create `crates/buildl-core/src/types/machine_folders.rs`; modify `types/mod.rs`.

**Change:** `MachineFolders::new(home, workspace, sandbox)`, each an optional list of forms (as given
and canonical). A home with a trailing `/` has it taken off; a home that is unset, empty or `/` is
left out. `find_in(text)` returns the longest folder that matches: it starts the text or follows a
character other than a letter, digit, `.`, `_`, `-`, `/`; and it ends the text or is followed by a
character other than a letter, digit, `.`, `_`, `-`.

**Proof:** written first, seen failing, then passing — one test per row of spec §6's match table,
with `HOME=/home/al`:
- `matches_followed_by_slash` — `cd /home/al/shop && make`.
- `matches_the_whole_text` — `/home/al`.
- `matches_followed_by_space` — `cd /home/al && make`.
- `matches_followed_by_colon` — `/home/al:/usr/lib`.
- `matches_after_a_quote` — `"workspace_root":"/home/al/tcapp"`.
- `does_not_match_a_longer_name` — `cd /home/alice/shop`.
- `does_not_match_inside_another_path` — `/data/home/al/x`.
- `does_not_match_absent` — `cd shop && make`.
- `skips_a_root_home` — `HOME=/` with `/usr/bin/env`.
- `trailing_slash_is_taken_off` — `HOME=/home/al/` matches `/home/al/x`.
- `names_the_longest_match` — workspace `/home/al/shop` and home `/home/al`: the workspace is named.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): find this machine's folders in a text`.

### Task 2 — Report this machine's folders through the `Host` port

**Files:** Modify `crates/buildl-core/src/ports/host.rs`, `ports/port_set.rs:156`,
`ports/bundle.rs:76`, `crates/buildl-core/tests/flows/common.rs:182`,
`crates/buildl/src/report/plan_text.rs:193`, `crates/buildl/src/host/local_host.rs`,
`crates/buildl/src/local_ports.rs:104`.

**Change:** `Host::machine_folders(&self) -> MachineFolders`, a required method; every `Host` above
implements it (stubs return no folders; `FakeHost` returns the folders a test gives it).
`LocalHost` is built with the workspace root and the sandbox folder (`Scratch` root,
`crates/buildl/src/state/scratch.rs:56-65`) and a `HOME` value read once by its constructor from
the process and passed into a constructor that takes it as an argument, the seam the adapter uses
for values today (`value(name, held)`, `local_host.rs:66`), so a test can give it an unset `HOME`
without `remove_var` (which `unsafe_code = "forbid"` rules out, `Cargo.toml:109`). Each folder is
given in its written and canonical forms (`/tmp` and `/private/tmp` on macOS).

**Proof:** written first, seen failing, then passing —
- `local_host_reports_home_workspace_and_sandbox` — a temp workspace's three folders, both forms.
- `local_host_leaves_out_an_unset_home` — built with no `HOME`, no home folder.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl): report this machine's folders from the host`.

### Task 3 — Refuse commands, key values and discovery outputs that hold them

**Files:** Create `crates/buildl-core/src/load/machine_paths.rs`; modify `load/mod.rs`,
`pipeline/driver.rs`, `pipeline/doctor.rs`, `pipeline/gc.rs`, `plan/components.rs`, `error.rs`,
`crates/buildl/src/workspace.rs:252-254`; test in `crates/buildl-cli/tests/cli/exit_codes.rs`.

**Change:** `check`, `graph`, `query`, `load_twice`, `results` and `doctor` take a `&MachineFolders`,
which the workspace passes from `Host::machine_folders()`. After the declarations are loaded, each
command word (placeholders unexpanded) is searched; a hit is E5: `build.lua: //:note: the command
holds this machine's path /Users/hiraq; it would give the target another key on every machine`.
Each discovery output, as `results` reads it from the cache (just run, cached or fetched alike), is
searched before the second pass; a hit is E7: `build.lua: //:cargo.metadata: its output holds this
machine's path /home/alice/shop`; the output stays recorded. `components::environment` searches
each `key` variable's value; a hit is E6: `build.lua: //:app: the variable RUSTFLAGS holds this
machine's path /home/alice; mark it "unkeyed" if it only finds files`. `unkeyed` values are not
searched. All exit 5; under `watch` they are reported like any load error and the watch goes on
(spec §6, `book/src/ch10-00-watch.md:63`).

**Proof:** written first, seen failing, then passing —
- `a_command_holding_home_is_refused` — E5 with its message.
- `a_discovery_output_holding_the_workspace_is_refused` — E7, the output still in the cache.
- `a_cached_discovery_output_is_checked_too` — the same output served from the cache is refused again.
- `a_key_value_holding_home_is_refused` — E6.
- `an_unkeyed_value_holding_home_is_allowed` — no refusal.
- `gc_refuses_a_machine_path_too` — `gc`'s load gives E5.
- in `exit_codes.rs`: `machine_path_in_command_exits_5` (spec P10's workspace), `machine_path_in_variable_exits_5`, `machine_path_in_discovery_exits_5`.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-core): refuse targets that hold this machine's folders`.

---

## Risks

- Task 3 is the riskiest: the check runs on every load, so a false match would block a workspace.
  The match table's tests and `cargo make examples` (plan 05) guard it; today's examples and the
  `c` plugin hold no such path (spec P19).
- Task 3 changes public signatures of the pipeline functions; the compiler names every caller.
- Not done here: the book's account of the refusal (plan 07).

## Proof

- `cargo make dod` and `cargo deny check` pass.
