---
status: approved
created: 2026-11-09
depends-on: [03]
built-on: 072796d9b51f
---

# cargo buildl init Implementation Plan

**Goal:** Add `cargo buildl init`, which sets a Cargo project up for buildl in one command.

**Architecture:** `scaffold` renders `buildl.toml` and the `.gitignore` lines, and holds `RUST_PLUGIN_TAG`. `init` is the command flow of spec §7: every check before any write, the writes with rollback, then the plugin fetch, run as `buildl::cli::execute` with `Command::Plugins(PluginsArgs { command: PluginsCommand::Fetch })` in `workspace_root`, so its lines and exit code are `buildl plugins fetch`'s own (plan 01 makes those types public). `build.lua` is `section::BEGIN_LINE`, the section from `generate`, `section::END_LINE`, a blank line, and the comment line of spec §6.

**Tech Stack:** Rust 2024, clap 4 derive.

**Content authority:** spec §7, §9, §11 (init rows and the no-network note).

---

## Files that change

```
crates/buildl-cargo/src/scaffold.rs   — create: `RUST_PLUGIN_TAG`, `buildl_toml(name)`, `gitignore(existing) -> new text`, `build_lua(section)`
crates/buildl-cargo/src/init.rs       — create: `init(start, stderr, console…) -> Exit` per spec §7
crates/buildl-cargo/src/args.rs       — modify: `CargoCommand::Init`
crates/buildl-cargo/src/main.rs       — modify: dispatch `Init`; module list
crates/buildl-cargo/tests/init.rs     — create: runs the binary on a temp copy of tests/fixtures/workspace/
```

## Order of work

### Task 1 — Render the files `init` writes

**Files:**
- Create `crates/buildl-cargo/src/scaffold.rs`.

**Change:**
- `RUST_PLUGIN_TAG` is `"plugin-rust-v0.2.0"`.
- `buildl_toml(name)` gives the text of spec §7, with `name` as a TOML basic string, `"` and `\` escaped.
- `gitignore(existing)` adds `/out/` and `/.buildl/`, each only when that exact line is missing, and starts a new file when there is none.
- `build_lua(section)` gives the begin line, the section, the end line, a blank line, and `-- Your own targets go below. \`cargo buildl sync\` never changes them.`

**Proof:**
- `buildl_toml_matches_the_spec` — the rendered text for `name = "demo"` equals spec §7's, with the tag.
- `a_name_with_a_quote_or_backslash_is_escaped` — `a"b\c` is written `name   = "a\"b\\c"`.
- `gitignore_adds_only_missing_lines` — absent file, file with one line, file with both lines.
- `build_lua_holds_both_markers` — `section::find` finds the span in `build_lua`'s output.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): render buildl.toml, build.lua and .gitignore for init`.

### Task 2 — Add `cargo buildl init`

**Files:**
- Create `crates/buildl-cargo/src/init.rs`, `crates/buildl-cargo/tests/init.rs`.
- Modify `crates/buildl-cargo/src/args.rs`, `crates/buildl-cargo/src/main.rs`.

**Change:** The steps follow spec §7. (1) Read the metadata. (2) Refuse an existing `buildl.toml` or `build.lua` with `init: <file> already exists; this folder is set up already, or remove <file> to start over` (exit 5). (3) Refuse a missing `Cargo.lock` with `init: no Cargo.lock beside Cargo.toml; run cargo generate-lockfile` (exit 5). (4) Generate the section. (5) Write `build.lua`, then `buildl.toml`, then read any existing `.gitignore` and write the new one; on an I/O error, reading `.gitignore` included, remove the files this run created and exit 3. (6) Print `init: wrote build.lua, buildl.toml, .gitignore`, then run the fetch through `buildl::cli::execute`; a fetch failure gives that command's exit code and a line saying to run `cargo buildl plugins fetch`. The `name` is the last component of `workspace_root`, written lossily when it is not UTF-8. Messages go to stderr in buildl's error style.

**Proof** (integration; `CARGO` set to the real cargo, `PATH` holding no `git`, so the fetch ends with 3):
- `init_writes_the_three_files_then_fails_the_fetch_with_3` — the files equal the `scaffold` output; the stderr names `cargo buildl plugins fetch`.
- `a_second_init_is_5_and_writes_nothing` — the folder is byte-identical before and after.
- `no_cargo_lock_is_5_and_writes_nothing`.
- `a_failed_write_leaves_the_folder_as_it_was` — `.gitignore` made a directory, so step 5 fails after `build.lua` and `buildl.toml` were written; both are gone again; exit 3.
- `init_from_a_member_folder_writes_at_the_root`.
- `sync_check_is_0_right_after_init`.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): add cargo buildl init`.

---

## Risks

- The success path of the fetch is not run in tests, so they never reach the network. It is `buildl plugins fetch`'s own path, already covered by `crates/buildl/tests/e2e/fetch.rs`.
- Rollback removes only files this run created. `.gitignore` is written last, so a failure before it leaves the existing `.gitignore` untouched.

## Proof

- `cargo make dod` and `cargo deny check` are green.
- Every step and message of spec §7 has a named test above.
