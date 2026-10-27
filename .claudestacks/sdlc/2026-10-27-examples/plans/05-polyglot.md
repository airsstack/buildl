---
status: approved
created: 2026-10-27
depends-on: [03]
built-on: 5947af3e315f
---

# Polyglot monorepo example — Implementation Plan

**Goal:** one example workspace builds a Go service, a Rust CLI and a Python tool from one shared generated schema.

**Architecture:** `examples/polyglot/` holds `schema/` (a data file and one Python generator
target emitting Go, Rust and Python source as three outputs), and `go/`, `rust/`, `python/`
subdirectories, each with its own `build.lua`, joined by `buildl.subdir` and the
cross-directory label `//schema:gen`. Because one target produces all three files,
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
examples/polyglot/{buildl.toml,build.lua,go.mod,assert.sh,README.md} — [create] root: subdirs, ceiling, the Go module root
examples/polyglot/schema/{build.lua,schema.json,gen.py}      — [create] the data and the one generator target
examples/polyglot/go/{build.lua,main.go}                     — [create] Go service importing the generated package
examples/polyglot/rust/{build.lua,Cargo.toml,Cargo.lock,src/main.rs} — [create] Rust CLI including the generated file; own [workspace] table
examples/polyglot/python/{build.lua,pack.py,tool/…}          — [create] Python tool packed with the generated module
```

## Order of work

### Task 1 — The schema and its generator

**Files:** Create `examples/polyglot/{buildl.toml,build.lua}`, `examples/polyglot/schema/*`

**Change:** `schema.json` holds a few named values (a version and a greeting) and a `"comment"`
key that the generator ignores. `gen.py` takes the schema path and three output paths and
writes the Go, Rust and Python source to them itself (no stdout capture, so this plan does not
need plan 01). `schema/build.lua` declares one target, `gen`, with
`outputs = { "gogen/gen.go", "gen.rs", "gen.py" }` and `run = { "python3", "gen.py", "schema.json", "$out" }`
(`$out` expands to one argument per output, design §5). Root `build.lua` calls `buildl.subdir`
for `schema`, `go`, `rust`, `python`. `buildl.toml`: `proc.run = ["go", "cargo", "python3"]`,
`env.read = ["HOME", "PATH"]`, `fs.read = ["."]`, `fs.write = ["out", ".buildl"]`.

**Proof:** in a scratch copy, `buildl build //schema:gen` produces three files; editing only the
`comment` and building again leaves the three outputs' bytes unchanged.

**Done when:** `cargo make dod` green. Commit `feat(repo): polyglot example schema and generator`.

### Task 2 — The three programs and the example's checks

**Files:** Create `examples/polyglot/{go,rust,python}/*`, `examples/polyglot/assert.sh`,
`examples/polyglot/README.md`

**Change:**
- Where the generated files are read: a dependency's output reaches the action at its path under
  `out/` — `out/schema/gogen/gen.go`, `out/schema/gen.rs`, `out/schema/gen.py` — in the exec dir
  and, under `bare`, in the workspace (design §12.2). So Task 1's generator names its Go output
  `gogen/gen.go`, and:
  - `go/`: `go.mod` sits at `examples/polyglot/` (the module root is the action's working
    directory); `go/main.go` imports the generated package as `<module>/out/schema/gogen`; the
    target runs `go build -o $out ./go` with `go.mod`, `go/main.go` as inputs and
    `deps = { "//schema:gen" }`.
  - `rust/`: `src/main.rs` reads the generated file with `include!` by its path relative to
    `rust/src/main.rs` (`../../out/schema/gen.rs`); the target runs
    `cargo install --path rust --root $out --target-dir tt --no-track`; own `[workspace]` table.
  - `python/`: `tool/__main__.py` imports `gen`; a standard-library `pack.py` writes one archive
    from the tool's files and `out/schema/gen.py`, every entry stamped 1980-01-01 00:00 so the
    archive is the same bytes on Linux and macOS (the `zipapp` file-time risk of plan 03 Task 3);
    the archive runs with `python3`.
  First step, before the assertions: run each of the three through `buildl` once in a scratch
  copy. If one of these forms fails, the task changes that language's form inside this plan and
  records the working form in the commit message and the example's `README.md`.
- `assert.sh`: each program prints its expected line built from the schema; append to the
  schema's `comment`, `buildl build`, and the summary shows `gen` ran while all three programs
  were served from cache; then `buildl plan --json` reports every target `Clean`;
  `buildl query 'rdeps(//schema:gen)'` lists targets under `//go`, `//rust` and `//python`.
- `README.md`: labels across directories, early cutoff, `query`.

**Proof:** `cargo make examples` passes with `polyglot` included.

**Done when:** it passes. Commit `feat(repo): polyglot example programs and checks`.

## Risks

- **Most risky: where each toolchain reads the generated file.** The three forms above follow
  from design §12.2's placement but were not probed; Task 2's first step runs each before the
  assertions are written.
- **A Go import path through `out/`** is unusual for Go readers; the example's `README.md` says
  it is where buildl places a dependency's output.
- **Chose not to:** use `protoc`; share one Cargo workspace with the Rust starter; one generator
  target per language (it would leave no target whose `rdeps` reach all three).

## Proof

- `cargo make examples` green locally and in the CI `examples` job on both operating systems.
