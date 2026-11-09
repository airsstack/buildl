---
status: approved
created: 2026-11-09
depends-on: [02]
built-on: 072796d9b51f
---

# cargo buildl sync Implementation Plan

**Goal:** Add `cargo buildl sync [--check]`, which keeps the generated section of `build.lua` in line with `Cargo.toml`.

**Architecture:** `buildl-cargo` gains four focused modules. `metadata` runs `cargo metadata` and reads the fields spec §5 lists. `generate` turns them into the section text of spec §6. `section` finds the markers and replaces the lines between them. `sync` is the command flow of spec §8. `args` gains `Sync { check }`, and `main` dispatches it. Names are checked with `buildl::buildl_core::TargetName::parse`.

**Tech Stack:** Rust 2024, serde + serde_json, clap 4 derive.

**Content authority:** spec §5, §6, §8, §9, §11 (generation, markers and sync rows).

---

## Files that change

```
crates/buildl-cargo/Cargo.toml                    — modify: depend on serde and serde_json (workspace)
crates/expected-edges.txt                         — modify: serde and serde_json in the `# buildl-cargo` block
crates/buildl-cargo/src/metadata.rs               — create: `CargoProject`, `Package`, `Target`; `read(cargo, dir)` runs `metadata --format-version 1 --no-deps`; `parse(json)`
crates/buildl-cargo/src/generate.rs               — create: `section(project) -> Result<String, Refusal>` per spec §6
crates/buildl-cargo/src/section.rs                — create: marker constants, `find(text)`, `replace(text, section)`
crates/buildl-cargo/src/sync.rs                   — create: `sync(start, check, stderr) -> Exit` per spec §8
crates/buildl-cargo/src/args.rs                   — modify: `CargoCommand::Sync(SyncArgs { check })`
crates/buildl-cargo/src/main.rs                   — modify: dispatch `Sync`; module list
crates/buildl-cargo/tests/fixtures/metadata/*.json — create: captured metadata (rust-workspace shape; my-lib shape with cdylib+staticlib+rlib, bins, example, test, build script; a bin with required-features; an example library; a non-ASCII name; two packages with the same bin name)
crates/buildl-cargo/tests/fixtures/workspace/     — create: a small Cargo workspace (two members: a lib, a bin) with Cargo.lock
crates/buildl-cargo/tests/sync.rs                 — create: runs the binary on a temp copy of the fixture workspace
```

## Order of work

### Task 1 — Read the Cargo project

**Files:**
- Create `crates/buildl-cargo/src/metadata.rs` and the `tests/fixtures/metadata/*.json` files.
- Modify `crates/buildl-cargo/Cargo.toml`, `crates/expected-edges.txt`.

**Change:** `read` runs the program `CARGO` names, or `cargo` when it is unset, as `metadata --format-version 1 --no-deps` in `dir`. `parse` keeps `workspace_root` and each package's `name` and targets (`name`, `kind`, `crate_types`, `required-features`). Failure to start cargo maps to exit 3. A non-zero cargo exit passes on cargo's stderr and maps to exit 5. Output that cannot be read as metadata maps to exit 3 (spec §9).

**Proof:**
- `parses_the_workspace_shape` — the rust-workspace fixture gives three packages with the kinds of spec F6.
- `parses_required_features` — a bin's `required-features` is read.
- `a_cargo_that_cannot_start_is_3` — `CARGO` naming a missing file gives exit 3.
- `a_refused_manifest_is_5` — cargo run on a folder with a broken `Cargo.toml` gives exit 5 and carries cargo's message.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): read the Cargo project through cargo metadata`.

### Task 2 — Generate the section

**Files:**
- Create `crates/buildl-cargo/src/generate.rs`.

**Change:** `section` returns the lines between the markers: `local rust = buildl.use("rust")`, a blank line, then per package (sorted by name) the library, the binaries (sorted by name) and the test, per the spec §6 table. A library target is one whose `kind` holds none of `bin`, `example`, `test`, `bench`, `custom-build`, and gets a call only when its `crate_types` holds `cdylib` and/or `staticlib`, with those in Cargo's order. No `profile` is written. A name that `TargetName::parse` refuses, or two generated targets with one name, is a `Refusal` (exit 5) naming the target and the package(s).

**Proof:**
- `rust_workspace_gives_three_tests_and_one_binary` — equals the example in spec §6, line for line.
- `a_library_lists_its_crate_types_in_cargo_order` — `["cdylib","staticlib","rlib"]` gives `kind = { "cdylib", "staticlib" }`.
- `rlib_proc_macro_example_and_build_script_get_no_call`.
- `an_example_library_gets_no_call` — `kind ["example"]`, `crate_types ["cdylib"]`.
- `a_bin_with_required_features_gets_no_call`.
- `a_name_outside_the_grammar_is_refused` and `two_targets_with_one_name_are_refused` — both name the package(s).
- `the_same_metadata_gives_the_same_bytes`.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): generate the build file section from Cargo metadata`.

### Task 3 — Find and replace the generated section

**Files:**
- Create `crates/buildl-cargo/src/section.rs`.

**Change:** A begin marker is a line starting with `-- cargo-buildl: begin generated`, and an end marker a line starting with `-- cargo-buildl: end generated`; a trailing `\r` is ignored. `find` gives the span between exactly one begin line and one later end line. It refuses no markers, more than one of either, a begin without an end, an end without a begin, and an end before the begin, each with the spec §8 message. `replace` keeps both marker lines and every byte outside them, and writes the new lines with `\n`. `BEGIN_LINE` and `END_LINE` are the full marker lines `init` writes (spec §6).

**Proof:**
- `replace_keeps_every_byte_outside_the_markers` — text before and after, with CRLF outside, is unchanged.
- `free_text_after_the_marker_prefix_is_a_marker`.
- `no_markers_is_refused`, `two_begins_is_refused`, `a_lone_begin_is_refused`, `a_lone_end_is_refused`, `end_before_begin_is_refused` — each with its spec §8 text.
- `same_section_is_up_to_date` — `replace` with the current lines reports no change.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): find and replace the generated section of build.lua`.

### Task 4 — Add `cargo buildl sync [--check]`

**Files:**
- Create `crates/buildl-cargo/src/sync.rs`, `crates/buildl-cargo/tests/fixtures/workspace/`, `crates/buildl-cargo/tests/sync.rs`.
- Modify `crates/buildl-cargo/src/args.rs`, `crates/buildl-cargo/src/main.rs`.

**Change:** `sync` reads the metadata, generates the section, and reads `build.lua` in `workspace_root`. A missing file gives `sync: no build.lua; run cargo buildl init` (exit 5). Without `--check`, it writes only when the section differs (`sync: build.lua updated`, else `sync: build.lua is up to date`). With `--check` it writes nothing: up to date is 0, out of date is 5 with `sync: build.lua is out of date with Cargo.toml; run cargo buildl sync`. Messages go to stderr in buildl's error style. `sync` never touches `buildl.toml`.

**Proof** (integration, on a temp copy of the fixture workspace with a `build.lua` holding markers and text of its own):
- `check_is_0_when_up_to_date`.
- `check_is_5_after_a_new_bin_then_0_after_sync` — a new `[[bin]]` makes `--check` 5; `sync` updates; `--check` is 0; the engineer's text is unchanged.
- `sync_from_a_member_folder_writes_at_the_root`.
- `no_build_lua_is_5` and `missing_markers_is_5` — exit 5 with the spec §8 messages.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): add cargo buildl sync`.

---

## Risks

- The integration tests need a real `cargo` on the test machine; CI has one. They run `cargo metadata --no-deps` only, which needs no network.
- Fixture JSON hand-trimmed from real output could drift from cargo's format. The fixtures are trimmed copies of real `cargo metadata` output, not hand-written.

## Proof

- `cargo make dod` and `cargo deny check` are green.
- Every row of spec §6's table and every message of spec §8 has a named test above.
