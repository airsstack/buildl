# Polyglot

One workspace, three languages, one shared schema. A Python generator turns `schema.json` into
Go, Rust and Python source; a Go service, a Rust CLI and a Python tool each build from it.

```text
schema/schema.json ─► //schema:gen (python3 generate.py)
                         │ out/schema/gogen/gen.go, gogen/go.mod, gen.rs, gen.py
          ┌──────────────┼──────────────────┐
          ▼              ▼                  ▼
   //go:service     //rust:cli        //python:tool.pyz
   go build         cargo install     pack.py (zip)
```

## What it uses

| Piece | Why |
|---|---|
| `buildl.subdir` | the root build file names `schema`, `go`, `rust` and `python`; each directory declares its own targets, labelled `//<dir>:<name>` |
| labels across directories | each program lists `deps = { "//schema:gen" }`; a dependency's outputs reach the action at their place under `out/`, here `out/schema/` |
| one target, four outputs | `$out` expands to one argument per output, so `generate.py` is given all four paths |
| early cutoff | an edit to the schema's `comment` runs the generator again, but it writes the same bytes, so the three programs are served from the cache: `1 built, 3 cached` |
| `buildl query` | `buildl query 'rdeps(//schema:gen)'` lists every target that depends on the generator: one per language |

## Where each language reads the generated file

A command runs at the workspace root, and an input path never leaves its build file's
directory, so each language directory holds its own project file:

| Language | How it finds `out/schema/` |
|---|---|
| Go | `go/go.mod` requires `example.com/polyglot/schema` and replaces it with `../out/schema/gogen`, whose `go.mod` the generator writes; the target runs `go build -C go` |
| Rust | `rust/src/main.rs` reads `../../out/schema/gen.rs` with `include!`; the target runs `cargo install --path rust` |
| Python | `pack.py` packs the tool and `out/schema/gen.py` into one archive, every entry stamped 1980-01-01, so the archive is the same bytes on every machine |

## Run it

Needs the `buildl` binary, Go 1.22, a Rust toolchain and Python 3.

```sh
buildl build                          # 4 built
./out/go/service                      # go service 1.0: hello from the schema
./out/rust/cli/bin/cli                # rust cli 1.0: hello from the schema
python3 out/python/tool.pyz           # python tool 1.0: hello from the schema
buildl query 'rdeps(//schema:gen)'    # //go:service //python:tool.pyz //rust:cli
```

From `examples/`, `sh check.sh polyglot` runs the shared checks on a copy, then `assert.sh`:
each program prints the schema's values, a comment edit rebuilds only the generator, and
`rdeps(//schema:gen)` names one target per language.
