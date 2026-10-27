---
status: approved
created: 2026-10-27
depends-on: [01, 02]
built-on: 5947af3e315f
---

# A stdout stream read back end to end — Implementation Plan

**Goal:** an end-to-end test proves that a discovery target's stdout stream, read back with `buildl.decode_all`, declares targets through the real adapters.

**Architecture:** extends plan 01's fixture `crates/buildl/tests/fixtures/stdout_output/`, whose
discovery target `printf`s two JSON objects under `stdout = true`: its `build.lua` decodes
`buildl.discovered(...)` with `buildl.decode_all` (plan 02) and declares one target per value.
The test drives `buildl`'s public API through the `Workspace` facade, as every e2e test does
(architecture-building-blocks §8). This is spec §6's e2e row for `buildl.decode_all`.

**Tech Stack:** Rust integration tests in `crates/buildl/tests/e2e`; system tools by argv (`printf`).

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §6. Gates: `cargo make dod` and
`cargo deny check` green; Conventional Commit.

---

## Files that change

```
crates/buildl/tests/fixtures/stdout_output/build.lua — [modify] decode the discovery output with buildl.decode_all; one target per value
crates/buildl/tests/e2e/stdout_output.rs             — [modify] the read-back tests
```

## Order of work

### Task 1 — Read the stream back

**Files:** Modify the two files above.

**Change:** the fixture's build file, when `buildl.discovered` is not `nil`, declares one target
per value of `buildl.decode_all(...)`, named from a field of each object and `printf`-ing it to
its output. No production change.

**Proof:** written first, seen failing, then passing —
- `a_stdout_stream_declares_one_target_per_value` — after `build`, the graph holds the two
  declared targets and their outputs hold the values' text.
- `the_read_back_is_byte_identical_in_a_second_checkout` — building a copy gives the same bytes.

**Done when:** `cargo test -p buildl --test e2e stdout_output` passes, `cargo make dod` and
`cargo deny check` are green. Commit `test(buildl): read a stdout stream back with decode_all end to end`.

## Risks

- None beyond plan 01's and plan 02's: both features are already proven at their own layers.

## Proof

- `cargo test -p buildl --test e2e` green.
