---
status: done
created: 2026-11-08
built-on: fef1a1baa97b
---

# The Rust Plugin Implementation Plan

**Goal:** A `plugins/rust` folder whose functions declare plain cargo targets and whose helpers return values.

**Architecture:** Laid out as `plugins/c` (spec P13): `options.lua` checks and fills the options
(pure), `graph.lua` declares the targets through the `buildl` table it is given, `api.lua` is the
public table (functions that declare, helpers that return a value, errors re-raised at the build
file's call), `main.lua` returns `require("api").new(buildl)`. Tests under `plugins/rust/tests` run
with a fake `buildl`, through `cargo make plugins`, which stages each plugin folder apart.

**Tech Stack:** Lua (airsl `airsl test`), TOML, `Makefile.toml`.

**Content authority:** `spec.md` §2 (functions, options, helpers), §3 (manifest), §4 (errors), §6, §7.

---

## Files that change

```
plugins/rust/extension.toml          — create: name, version 0.1.0, entry, api, capabilities (spec §3)
plugins/rust/main.lua                — create: the entry, as plugins/c/main.lua
plugins/rust/options.lua             — create: check and fill the options of one call (pure)
plugins/rust/graph.lua               — create: declare one call's targets through buildl
plugins/rust/api.lua                 — create: the public table, the helpers, error relocation
plugins/rust/tests/fake_buildl.lua   — create: plugins/c/tests/fake_buildl.lua plus sources(pattern)
plugins/rust/tests/options_test.lua  — test: options and their errors
plugins/rust/tests/graph_test.lua    — test: the declared targets
plugins/rust/tests/api_test.lua      — test: the public table and the helpers
plugins/rust/README.md               — create: shaped as plugins/c/README.md
Makefile.toml                        — modify: the plugins task stages each plugin folder apart
```

## Order of work

Task 1 first (the task the tests run through); Tasks 2 and 3 then; Task 4 needs 2 and 3; Task 5 last.

### Task 1 — Test each plugin folder apart

**Files:** Modify `Makefile.toml` (task `plugins`, `:387-412`).

**Change:** the task loops over each folder under `plugins/` holding a `tests/` folder, copies that
folder's `*.lua` and `tests/*.lua` into a temporary folder of its own and runs `airsl test` there,
so files of the same name in two plugins do not clash (spec §6). The same-name check stays within
one plugin.

**Proof:** `cargo make plugins` passes with `plugins/c` alone (73 passed, as before: `grep -hE '^    [a-z_0-9]+ = function\(' plugins/c/tests/*_test.lua | wc -l` → 73, and the last run printed `73 passed, 0 failed`), and later with `plugins/rust` beside it.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `build(repo): test each plugin folder apart`.

### Task 2 — Check and fill the options

**Files:** Create `plugins/rust/options.lua`, `plugins/rust/tests/options_test.lua`.

**Change:** `options.check(kind, name, given)` for `kind` `binary`, `library`, `test`: `given` may
be nil (an empty table); options `args` (list of strings), `profile` (string), `deps` (list of
strings), `output` (string), `kind` (string, `library` only, `"staticlib"` or `"cdylib"`),
`version_probe` (boolean), checked in sorted order; returns the filled table with `args` and `deps`
`{}`, `profile` `"dev"`, `version_probe` false, `output` `name` for a binary and a cdylib and
`lib<name>.a` for a staticlib, and the profile folder (`debug` for `dev` and `test`, `release` for
`release` and `bench`, else the profile's name; spec P10). Messages are those of spec §4, prefixed
`rust.<kind>(<name>): `.

**Proof:** written first, seen failing, then passing —
- `options_may_be_left_out` — `check("binary", "hello", nil)` gives the defaults.
- `options_must_be_a_table` — a string raises `the options must be a table`.
- `an_unknown_option_is_refused` — `bogus` raises `the option bogus is not known`.
- `a_wrong_type_is_refused` — `args = "x"` raises `the option args must be a list of strings`.
- `kind_is_refused_outside_library` — raises `the option kind is refused` for binary and test.
- `library_needs_a_kind` — none, or `"rlib"`, raises `kind must be "staticlib" or "cdylib"`.
- `outputs_default_by_kind` — `hello`, `libmylib.a`, cdylib `ffi`.
- `profile_folders` — `dev`/`test` → `debug`, `release`/`bench` → `release`, `fast` → `fast`.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): check the rust plugin's options`.

### Task 3 — Declare the targets of a call

**Files:** Create `plugins/rust/graph.lua`, `plugins/rust/tests/fake_buildl.lua`,
`plugins/rust/tests/graph_test.lua`.

**Change:** `graph.declare(buildl, kind, name, options)` declares, for `binary` and `library`, a
target `name` running `sh -c` with `cd <folder> && cargo build --locked <--bin name | --lib>
--profile <profile> --target-dir target <args>`, then a copy of
`<folder>/target/<profile folder>/<file>` to `$out`, the file being `name` for a binary,
`lib<name>.a` for a staticlib, and for a cdylib whichever of `lib<name>.so` and `lib<name>.dylib`
cargo wrote (the copy tries one then the other), whatever the output is named; for `test`, a
test `name` running `cargo test --locked --profile <profile> --target-dir target <args>` in the
folder. `<folder>` is `buildl.folder()`, and nothing at the root (spec P8). Every declaration has
`inputs` = every file `buildl.sources("**/*")` lists but those under `target/` and `.git/` (amended in execution, see Deviations), `deps` =
`options.deps`, `env = { HOME = "unkeyed", PATH = "unkeyed" }`, `network = true`, `outputs = {
options.output }` for a build. Before declaring, it raises `no Cargo.lock beside Cargo.toml; run
cargo generate-lockfile` or `no Cargo.toml in <folder>` (spec §4, P5). With `version_probe` it
declares `<name>.version` (`rustc -vV`, `stdout`, `always`) and adds it to `deps`.
`tests/fake_buildl.lua` is `plugins/c/tests/fake_buildl.lua` with `sources(pattern)` serving a
canned file list.

**Proof:** written first, seen failing, then passing —
- `binary_runs_cargo_build_in_the_folder` — at the root and in `rust/`, the command, the copy and `outputs`.
- `library_copies_the_named_file` — staticlib `libmylib.a`; a cdylib's command copies `libffi.so` or else `libffi.dylib`, with `output` `libffi.dylib` and with the default `ffi`.
- `test_runs_cargo_test` — a test, no output.
- `args_and_profile_reach_cargo` — `args` after the plugin's own; `--profile release` and the `release` folder.
- `inputs_leave_out_target` — `target/debug/x.rs` is not an input; `src/a/b.rs`, `data.txt`, `build.rs` are.
- `env_and_network` — `HOME` and `PATH` unkeyed, `network = true`.
- `missing_lockfile_or_manifest_is_refused` — both messages.
- `version_probe_adds_a_dependency` — `<name>.version` declared and in `deps`.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): declare the rust plugin's targets`.

### Task 4 — The public table and the helpers

**Files:** Create `plugins/rust/api.lua`, `plugins/rust/main.lua`, `plugins/rust/extension.toml`,
`plugins/rust/tests/api_test.lua`.

**Change:** `api.new(buildl)` returns the table with `binary`, `library`, `test` (each runs
`graph.declare(buildl, kind, name, options.check(kind, name, given))` and returns nothing, an error
re-raised at the caller as `plugins/c/api.lua:9-25` does), and the helpers of spec §2.2:
`package(name)` → `{ "-p", name }`; `features(list)` → `{ "--features", "a,b" }` or `{}`;
`platform()` — the first call declares the discovery `rust.platform` (`rustc -vV`, `stdout`), every
call returns `"linux"` or `"macos"` from its `host:` line, or nil before it has run;
`shared_name(name, platform)` → `lib<name>.so`, `lib<name>.dylib`, or nil. Their errors are those
of spec §4. `main.lua` and `extension.toml` as spec §3.

**Proof:** written first, seen failing, then passing —
- `the_public_table_holds_every_function` — the seven names.
- `a_function_returns_nothing` — each declaring function.
- `an_error_names_the_build_files_line` — a bad option's message carries the caller's location.
- `package_and_features_return_lists` — values, `{}` for no features, errors for wrong types.
- `platform_is_nil_then_known` — nil with no result; `macos` for `host: aarch64-apple-darwin`, `linux` for `host: x86_64-unknown-linux-gnu`; declared once; another triple raises.
- `shared_name_by_platform` — `.so`, `.dylib`, nil.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): add the rust plugin's public table and helpers`.

### Task 5 — Describe the plugin

**Files:** Create `plugins/rust/README.md`.

**Change:** shaped as `plugins/c/README.md`: setting it up (`buildl.toml` ceiling and
`[plugins.rust]`, `network = true` in the cap), the functions and their options, the helpers, a
workspace example (spec §2.2), the programs it may run, the limits (spec §5), errors (spec §4).
Every example matches the code of Tasks 2–4.

**Proof:** the README's examples use only the functions, options and helpers the tests of Tasks 2–4
name.

**Done when:** `cargo make plugins`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): describe the rust plugin`.

---

## Risks

- `buildl.sources("**/*")` walks a large folder; a plain-cargo `target/` there counts against
  `[declaration] walk_entries` before it is left out.
- Task 1 changes how the plugin tests run for `plugins/c` too; its 73 tests must still pass.

## Proof

- `cargo make plugins` passes for `plugins/c` and `plugins/rust`.
- `cargo make dod` and `cargo deny check` pass.

## Review findings

- 🟡 graph.lua inputs took `.git/**` for a crate at its git root (keys split per clone) — fixed in 8d178eb: `without_generated` leaves out `target/` and `.git/`; `inputs_leave_out_target` checks `.git/HEAD` out, `.github/ci.yml` in. `cargo make plugins` → c 73 passed, rust 24 passed.
- 🟡 options.lua accepted `output` for `rust.test` and dropped it — fixed in 8d178eb: refused with `the option output is refused`; test `output_is_refused_for_test`.
- 🔵 api.lua `features` used a looser list check than options.lua — fixed in 8d178eb (`options.is_list`).
- 🔵 api.lua `shared_name` did not check `name` — fixed in 8d178eb (`rust.shared_name: the name must be a string`).
- 🔵 no test for the `no host line` error — fixed in 8d178eb (`platform_refuses_output_without_a_host_line`).
- 🔵 api.lua:11 comment (tail call) — fixed in 8d178eb.
- 🔵 README: subshell `(cd <folder> && …)`, version-probe reason, duplicate line — fixed in 8d178eb.

## Deviations

- 2026-11-08 — Task 1 done inline by the orchestrator (a 15-line Makefile edit), not by a coder.
- 2026-11-08 — `cargo make dod` and `cargo deny check` ran once after Task 1 and once at the end of the chain, not before each commit: the changes are Lua, TOML and Markdown, and no Rust test reads `plugins/` (grep of `crates/` for `plugins/c`: test fixtures only). Both green: `dod exit 0`; `advisories ok, bans ok, licenses ok, sources ok`.
- 2026-11-08 — Task 3 inputs also leave out `.git/` (review finding above); the Task 3 text is amended.
- 2026-11-08 — `rust.platform` also raises `rust.platform: the output of rustc -vV has no host line`, and `rust.shared_name` raises on a non-string name; both are listed in the README and the book chapter.
- 2026-11-08 — the manifest has no network key (`crates/buildl-lua/src/plugin/manifest.rs` has none); `network = true` goes in the workspace's `[plugins.rust]` cap.
