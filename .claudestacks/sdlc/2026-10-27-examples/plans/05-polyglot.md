---
status: done
created: 2026-10-27
depends-on: [03]
built-on: 5947af3e315f
---

# Polyglot monorepo example — Implementation Plan

**Goal:** one example workspace builds a Go service, a Rust CLI and a Python tool from one shared generated schema.

**Architecture:** `examples/polyglot/` holds `schema/` (a data file and one Python generator
target emitting Go, Rust and Python source as four outputs), and `go/`, `rust/`, `python/`
subdirectories, each with its own `build.lua` and its own project file, joined by
`buildl.subdir` and the cross-directory label `//schema:gen`. An input path never leaves its
build file's directory (`convert_input`, `crates/buildl-core/src/load/traversal.rs:183-190`), so
no project file sits at the example root. Because one target produces all the generated files,
`query 'rdeps(//schema:gen)'` reaches targets of all three languages (`query` takes a target,
`docs/design.md:307`). Editing a comment in the schema re-runs the generator, which writes
identical bytes, so the targets downstream are served from cache (early cutoff). Checked by
`examples/check.sh` (plan 03).

**Tech Stack:** Python 3.12 (generator and tool), Go 1.22, cargo, the `buildl` binary.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §2, §5. Gates for every task:
`cargo make dod`, `cargo deny check` and `cargo make examples` green; Conventional Commit, one per task.

---

## Files that change

```
examples/polyglot/{buildl.toml,build.lua,assert.sh,README.md}  — [create] root: subdirs, ceiling, checks
examples/polyglot/schema/{build.lua,schema.json,generate.py}  — [create] the data and the one generator target
examples/polyglot/go/{build.lua,go.mod,main.go}               — [create] Go service importing the generated package
examples/polyglot/rust/{build.lua,Cargo.toml,Cargo.lock,src/main.rs} — [create] Rust CLI including the generated file; own [workspace] table
examples/polyglot/python/{build.lua,pack.py,tool/__main__.py} — [create] Python tool packed with the generated module
examples/README.md                                            — [modify] a row for the example
```

## Order of work

### Task 1 — The schema and its generator

**Files:** Create `examples/polyglot/{buildl.toml,build.lua}`, `examples/polyglot/schema/*`

**Change:** `schema.json` holds a few named values (a version and a greeting) and a `"comment"`
key that the generator ignores. `generate.py` (named apart from its output `gen.py`) takes the
schema path and four output paths and writes them itself (no stdout capture, so this plan does
not need plan 01): the Go package `gogen/gen.go`, its `gogen/go.mod` (module
`example.com/polyglot/schema`, so Go can `replace` a module with it), `gen.rs` and `gen.py`.
`schema/build.lua` declares one target, `gen`, with
`inputs = { "generate.py", "schema.json" }`,
`outputs = { "gogen/gen.go", "gogen/go.mod", "gen.rs", "gen.py" }` and
`run = { "python3", "$in", "$out" }` (`$in` and `$out` expand to one argument per path, in the
order written, design §5). Root `build.lua` calls `buildl.subdir` for `schema` (Task 2 adds the
other three). `buildl.toml`: `proc.run = ["go", "cargo", "python3"]`,
`env.read = ["HOME", "PATH"]`, `fs.read = ["."]`, `fs.write = ["out", ".buildl"]`.

**Proof:** in a scratch copy, `buildl build //schema:gen` produces the four files; editing only
the `comment` and building again leaves the outputs' bytes unchanged.

**Done when:** `cargo make dod` green. Commit `feat(repo): polyglot example schema and generator`.

### Task 2 — The three programs and the example's checks

**Files:** Create `examples/polyglot/{go,rust,python}/*`, `examples/polyglot/assert.sh`,
`examples/polyglot/README.md`; modify `examples/polyglot/build.lua` (the three subdirs) and
`examples/README.md` (a row)

**Change:**
- A command reads its paths from the workspace root (in the sandbox, from an exec dir laid out
  as it), and a dependency's output reaches the action at its path under `out/` —
  `out/schema/gogen/`, `out/schema/gen.rs`, `out/schema/gen.py` (design §12.2). So:
  - `go/`: `go/go.mod` (module `example.com/polyglot/service`) requires
    `example.com/polyglot/schema v0.0.0` and replaces it with `../out/schema/gogen`;
    `go/main.go` imports it; the target runs `go build -C go -o ../$out .` with `go.mod` and
    `*.go` as inputs, `deps = { "//schema:gen" }`, `env = { "HOME" }`.
  - `rust/`: `src/main.rs` reads the generated file with `include!` by its path relative to
    `rust/src/main.rs` (`../../out/schema/gen.rs`); the target runs
    `cargo install --path rust --root $out --target-dir tt --no-track`; own `[workspace]` table.
  - `python/`: `tool/__main__.py` imports `gen`; a standard-library `pack.py` is given
    `$in $deps $out` and writes the `.py` files among them into one archive, every entry stamped
    1980-01-01 00:00 with mode 0644 and stored uncompressed, so the archive is the same bytes on
    Linux and macOS (the `zipapp` file-time risk of plan 03 Task 3); the archive runs with
    `python3`.
  First step, before the assertions: run each of the three through `buildl` once in a scratch
  copy.
- `assert.sh`: each program prints its expected line built from the schema; append to the
  schema's `comment`, `buildl build`, and the summary reads `1 built, 3 cached`; then
  `buildl plan --json` reports every target `Clean`; `buildl query 'rdeps(//schema:gen)'` lists
  exactly `//go:service`, `//python:tool.pyz` and `//rust:cli`.
- `README.md`: labels across directories, early cutoff, `query`, and where each language reads
  the generated file.

**Proof:** `cargo make examples` passes with `polyglot` included.

**Done when:** it passes. Commit `feat(repo): polyglot example programs and checks`.

## Risks

- **Most risky: where each toolchain reads the generated file.** Settled by Task 2's first step
  (see Probe results).
- **Chose not to:** use `protoc`; share one Cargo workspace with the Rust starter; one generator
  target per language (it would leave no target whose `rdeps` reach all three).

## Proof

- `cargo make examples` green locally and in the CI `examples` job on both operating systems.

## Review findings

- spec gap — the plan's text still named a root `go.mod`, `schema/gen.py`, three outputs, the
  `out/` Go import path and the old `pack.py` arguments; the plan is amended to the forms built.
  Fixed.
- false claim — `README.md` and `go/build.lua` said a command "runs at the workspace root",
  true only under `bare`; now "reads its paths from the workspace root". Same wording fixed in
  `examples/plugins/README.md`. Fixed.
- nit — the README showed `rdeps` output on one line; it prints one label a line. Fixed.
- nit — `examples/README.md` row missing from the file list. Fixed.
- Left as is — `assert.sh` repeats `check.sh`'s every-target-Clean block, so it also holds when
  run alone.

## Probe results

- Inputs are joined under the declaring directory and `../x` is refused
  (`crates/buildl-core/src/load/traversal.rs:183-190`, test at `:699-702`): a root `go.mod` cannot
  be an input of `//go:service` — against the plan's Go form; Task 2 amended.
- Scratch copy, macOS: `buildl build` → `4 built`; the three programs print
  `go service 1.0: hello from the schema`, `rust cli 1.0: …`, `python tool 1.0: …`; after the
  comment edit `1 built, 3 cached` with the generated bytes unchanged; `plan --json` all `Clean`;
  `rdeps(//schema:gen)` → `//go:service`, `//python:tool.pyz`, `//rust:cli`; `verify` →
  `4 verified, 0 mismatched`.
- `BUILDL=target/debug/buildl sh examples/check.sh examples/polyglot` → `polyglot: ok`.
- CI run 40632551313 (at `7431351`): `Examples (ubuntu-latest)` and `Examples (macos-latest)`
  success with `polyglot` included.

## Deviations

- 2026-10-28 — `go.mod` in `go/` with a `replace` onto the generated module, whose `go.mod` the
  generator writes as a fourth output; the target runs `go build -C go -o ../$out .` (probe above).
- 2026-10-28 — the generator is `generate.py`, not `gen.py`.
- 2026-10-28 — `pack.py` takes `$in $deps $out` and keeps the `.py` files.
- 2026-10-28 — `cargo make dod` and `cargo deny check` ran once for the plan (no Rust changed);
  the Task 1 commit holds a root `build.lua` naming only `schema`.
