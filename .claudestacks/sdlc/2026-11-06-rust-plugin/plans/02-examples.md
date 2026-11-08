---
status: approved
created: 2026-11-08
depends-on: [01]
built-on: fef1a1baa97b
---

# Rust Examples Implementation Plan

**Goal:** The Rust examples build and test through the rust plugin, fetched as a user fetches it.

**Architecture:** `examples/rust` and `examples/polyglot/rust` replace their hand-written cargo
targets with `rust.binary` and `rust.test`. Each `buildl.toml` names `[plugins.rust]` by GitHub
tag, and a `prepare.sh`, as `examples/c/prepare.sh`, makes this checkout's `plugins/rust`
fetchable under that tag in a temporary git repository, so `examples/check.sh` and CI run the
plugin as this checkout has it (spec §6, §7).

**Tech Stack:** Lua build files, TOML, POSIX `sh`; `cargo make examples`.

**Content authority:** `spec.md` §6, §7.

---

## Files that change

```
examples/rust/build.lua, buildl.toml, README.md, assert.sh   — modify: use the plugin
examples/rust/prepare.sh                                     — create: fetch the plugin from this checkout
examples/polyglot/rust/build.lua, examples/polyglot/buildl.toml, examples/polyglot/README.md, examples/polyglot/assert.sh — modify
examples/polyglot/prepare.sh                                 — create: as examples/rust/prepare.sh
```

## Order of work

Tasks 1 and 2 touch disjoint files and may run side by side.

### Task 1 — Build examples/rust through the plugin

**Files:** Modify `examples/rust/build.lua`, `buildl.toml`, `README.md`, `assert.sh`; create
`examples/rust/prepare.sh`.

**Change:** the build file is `local rust = buildl.use("rust")`, `rust.binary("hello")`,
`rust.test("unit")`. `buildl.toml`: the ceiling allows `sh` and `rustc`, `HOME` and `PATH` in
`env.read` and `env.unkeyed`; `[plugins.rust]` names `github = "airsstack/buildl"`,
`rev = "plugin-rust-v0.1.0"`, `dir = "plugins/rust"`, the same programs and variables, and
`network = true`. `prepare.sh` is `examples/c/prepare.sh` for `plugins/rust` and its tag. The
README, as `examples/c/README.md`, draws the targets, has a "What it uses" table and explains
`buildl.toml`. `assert.sh` checks what `out/hello` prints, that `buildl plan` after a source edit
runs `//:hello` and `//:unit`, and that `buildl test` passes.

**Proof:** written first, seen failing, then passing — `BUILDL=target/debug/buildl sh examples/check.sh examples/rust` fails on the new `assert.sh` before the build file changes, and passes after.

**Done when:** `cargo make examples`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): build the rust example through the rust plugin`.

### Task 2 — Build examples/polyglot's Rust part through the plugin

**Files:** Modify `examples/polyglot/rust/build.lua`, `examples/polyglot/buildl.toml`,
`examples/polyglot/README.md`, `examples/polyglot/assert.sh`; create `examples/polyglot/prepare.sh`.

**Change:** `rust/build.lua` is `rust.binary("cli", { deps = { "//schema:gen" } })` and
`rust.test("cli_test")`, the plugin running cargo in `rust/` (spec P8). The binary now lands at
`out/rust/cli`, not `out/rust/cli/bin/cli` as `cargo install --root` put it: `assert.sh:21` and
`README.md:44` change to the new path. `assert.sh` also checks that `buildl plan` after an edit to
`rust/src/main.rs` runs `//rust:cli` and `//rust:cli_test`, and that `buildl test` passes.
`buildl.toml` and `prepare.sh` as Task 1's, beside what the other parts already allow. The README's
Rust rows and its `buildl.toml` explanation name the plugin.

**Proof:** written first, seen failing, then passing — `BUILDL=target/debug/buildl sh examples/check.sh examples/polyglot` fails on the new path and checks before the build file changes, and passes after.

**Done when:** `cargo make examples`, `cargo make dod` and `cargo deny check` pass. Commit `feat(repo): build polyglot's rust part through the rust plugin`.

---

## Risks

- `cargo make examples` builds real toolchains; it is the slow check.
- The tag `plugin-rust-v0.1.0` exists only after release; until then the examples run through
  `prepare.sh`.

## Proof

- `cargo make examples` passes.
