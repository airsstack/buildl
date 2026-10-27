---
status: executing
created: 2026-10-27
built-on: 5947af3e315f
---

# `buildl.decode_all` — Implementation Plan

**Goal:** build files and plugins can decode a stream of JSON values with `buildl.decode_all(text)`.

**Architecture:** a pure function in `buildl-lua`, set on the table `primitives_table` builds
(`crates/buildl-lua/src/module.rs:127-218`), which is both the build file's `buildl` table and
each plugin's (`crates/buildl-lua/src/plugin/environment.rs:60,74`). It splits the text at each
value's byte span with `serde_json`'s stream reader and decodes each span with airsl's own
`json.decode`, read from the copy recorded in the registry before any build file ran
(`CURATED_REGISTRY_KEY`, `curated.rs:26`; `Json` recorded at `engine.rs:29`). airsl and the
`airsstack` root table are not changed.

**Tech Stack:** Rust, mlua via airsl 0.1.4, `serde_json` 1.0.151 and `serde` 1.0.229 (locked).

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §4, §6, §7. Gates for every task:
`cargo make dod` and `cargo deny check` green; Conventional Commit, one per task.

---

## Files that change

```
crates/buildl-lua/Cargo.toml          — [modify] serde and serde_json as dependencies, each with its reason comment
crates/expected-edges.txt             — [modify] serde, serde_json under `# buildl-lua`
crates/buildl-lua/src/decode_all.rs   — [create] the function and its unit tests
crates/buildl-lua/src/lib.rs          — [modify] `mod decode_all;` (export-only, rust guideline mod-rs-export-only)
crates/buildl-lua/src/module.rs       — [modify] primitives_table sets `decode_all` (:127-218)
crates/buildl-lua/tests/flows/plugins.rs — [modify] a plugin reaches buildl.decode_all
crates/buildl-lua/tests/flows/declarations.rs — [modify] a build file uses it and declares from the result
crates/buildl-lua/README.md           — [modify] the declaration API row lists `decode_all`
docs/design.md                        — [modify] §5 bullet
```

## Order of work

### Task 1 — Add `buildl.decode_all`

**Files:**
- Modify `crates/buildl-lua/Cargo.toml`, `crates/expected-edges.txt`, `crates/buildl-lua/src/lib.rs`,
  `crates/buildl-lua/src/module.rs`, `crates/buildl-lua/README.md`
- Create `crates/buildl-lua/src/decode_all.rs`
- Test: `decode_all.rs` `#[cfg(test)] mod tests`; `crates/buildl-lua/tests/flows/plugins.rs`;
  `crates/buildl-lua/tests/flows/declarations.rs`

**Change:**
- `buildl-lua` takes `serde_json = { workspace = true }` (the stream reader) and
  `serde = { workspace = true }` (`serde::de::IgnoredAny`), each commented with its reason, as the
  workspace policy requires. `crates/expected-edges.txt` lists both under `# buildl-lua`
  (checked by `cargo make guard-crate-edges`, `Makefile.toml:126-152`).
- `decode_all(lua, text)` (one line: a list of the JSON values in `text`, in order): the
  argument must be a string, else a Lua error naming the argument. It walks
  `serde_json::Deserializer::from_str(text).into_iter::<IgnoredAny>()`, taking each value's span
  from `byte_offset()`; each span is decoded by calling the `decode` function of the `json` table
  in the `CURATED_REGISTRY_KEY` registry table (as `stable_text.rs:22-35` calls a Lua function
  from Rust). The result is a sequence table. A stream error becomes a Lua error carrying
  serde_json's message (`EOF while parsing a value at line 2 column 5`,
  `expected value at line 1 column 9`), catchable with `pcall`; uncaught, it fails the build file
  like any runtime error (exit 5).
- `primitives_table` sets it as `decode_all`; it stages nothing, so it needs neither `declare`
  (`module.rs:230`) nor `guarded` (`:252`).

**Proof:** written first, seen failing, then passing — each in `decode_all.rs` tests, with real
airsl (the cases spec §4 probed):
- `two_objects_on_two_lines_are_two_values` — `{"a":1}\n{"a":2}\n`
- `adjacent_objects_are_two_values` — `{"a":1}{"a":2}`
- `one_array_is_a_list_of_one_value` — `[1,2]` gives `{ {1,2} }`
- `spaced_numbers_are_three_values` — `1 2 3`
- `strings_null_and_true_are_three_values` — `"x" null true`, the null equal to what
  `airsstack.json.decode("null")` returns
- `a_number_then_an_object_are_two_values` — `1{"a":2}`
- `empty_text_is_an_empty_list` and `whitespace_only_is_an_empty_list`
- `unicode_survives` — `é` and `😀` intact
- `a_truncated_stream_raises_a_catchable_error` — `pcall` returns false with the EOF message
- `garbage_between_values_raises` — `{"a":1} xx {"a":2}`
- `a_non_string_raises_naming_the_argument`
- `values_equal_airsstack_json_decode` — each value deep-equals `airsstack.json.decode` of its span
- `a_build_file_replacing_airsstack_json_does_not_change_decode_all` — the registry copy is used
- flows `plugins.rs`: `a_plugin_can_call_decode_all`; flows `declarations.rs`:
  `a_build_file_declares_one_target_per_decoded_value`

**Done when:** `cargo test -p buildl-lua` passes, `cargo make dod` (which runs the guards) and
`cargo deny check` are green. Commit `feat(buildl-lua): decode a stream of JSON values with buildl.decode_all`.

### Task 2 — Document it

**Files:** Modify `docs/design.md`

**Change:** design §5 (after the `b.discover` bullet, `:239`): `buildl.decode_all(text)` returns
the list of JSON values in `text`; it reads a stream such as `go list -json` prints, where
`airsstack.json.decode` reads one value; errors as `decode`'s. The `b.discover` bullet's
"decoding it is the build file's, with `airsstack.json.decode`" becomes "with
`airsstack.json.decode`, or `buildl.decode_all` for a stream of values", so the two bullets agree.

**Proof:** `cargo make dod` green.

**Done when:** committed `docs(repo): document buildl.decode_all`.

## Risks

- **The registry copy's shape.** `Recorded` stores each module's table under its name in the
  `CURATED_REGISTRY_KEY` table (`curated.rs:28-63`); the task reads `json.decode` from it. If a
  test shows the copy is shallow and shares the function with the live table, the
  `…_does_not_change_decode_all` test still holds, because the function value is copied before
  the build file runs.
- **Memory.** A large stream is decoded inside the declaration engine and bounded by its memory
  ceiling, as `decode` is; no new limit.
- **Chose not to:** change airsl; put the function under `airsstack.json`; add a JSON Lines or
  array-wrapping mode.

## Proof

- `cargo make dod` and `cargo deny check` green.
- Plan 08 reads a stdout discovery output back with it end to end; plan 04's `go-discovery`
  example decodes real `go list -json ./...` output in CI.
