# Polyglot

One workspace, three languages, one shared schema. A Python generator turns `schema.json` into
Go, Rust and Python source; a Go service, a Rust CLI and a Python tool each build from it.

```text
schema/schema.json ─► //schema:gen (python3 generate.py)
                         │ out/schema/gogen/gen.go, gogen/go.mod, gen.rs, gen.py
          ┌──────────────┼──────────────────┐
          ▼              ▼                  ▼
   //go:service     //rust:cli        //python:tool.pyz
   go build         rust.binary       pack.py (zip)
                    (+ //rust:cli_test)
```

## What it uses

| Piece | Why |
|---|---|
| `buildl.subdir` | the root build file names `schema`, `go`, `rust` and `python`; each directory declares its own targets, labelled `//<dir>:<name>` |
| labels across directories | each program lists `deps = { "//schema:gen" }`; a dependency's outputs reach the action at their place under `out/`, here `out/schema/` |
| one target, four outputs | `$out` expands to one argument per output, so `generate.py` is given all four paths |
| early cutoff | an edit to the schema's `comment` runs the generator again, but it writes the same bytes, so the three programs are served from the cache: `1 built, 3 cached` |
| the `rust` plugin | `rust/build.lua` loads it with `buildl.use("rust")` and declares the binary and its test with `rust.binary` and `rust.test`; the plugin runs cargo, so the build file holds no command |
| `buildl query` | `buildl query 'rdeps(//schema:gen)'` lists every target that depends on the generator: one per language, and the Rust test |

## Where each language reads the generated file

A command reads its paths from the workspace root (in the sandbox, from a directory laid out
as it), and an input path never leaves its build file's directory, so each language directory
holds its own project file:

| Language | How it finds `out/schema/` |
|---|---|
| Go | `go/go.mod` requires `example.com/polyglot/schema` and replaces it with `../out/schema/gogen`, whose `go.mod` the generator writes; the target runs `go build -C go` |
| Rust | `rust/src/main.rs` reads `../../out/schema/gen.rs` with `include!`; the `rust` plugin runs cargo in `rust/`, and the binary and the test both list `//schema:gen` in `deps` |
| Python | `pack.py` packs the tool and `out/schema/gen.py` into one archive, every entry stamped 1980-01-01, so the archive is the same bytes on every machine |

## Run it

Needs the `buildl` binary, Go 1.22, a Rust toolchain and Python 3.

`buildl.toml` names the `rust` plugin in `[plugins.rust]` and lets it run `sh` and `rustc`, read
`HOME` and `PATH`, and reach the network, which cargo uses to download crates. The plugin comes
from git, so run `buildl plugins fetch` once before the first build.

```sh
buildl plugins fetch                  # clones the plugin at plugin-rust-v0.2.1, writes buildl.lock
buildl build                          # 4 built
./out/go/service                      # go service 1.0: hello from the schema
./out/rust/cli                        # rust cli 1.0: hello from the schema
python3 out/python/tool.pyz           # python tool 1.0: hello from the schema
buildl test                           # runs //rust:cli_test
buildl query 'rdeps(//schema:gen)'    # one label a line: //go:service, //python:tool.pyz, //rust:cli, //rust:cli_test
```

From `examples/`, `sh check.sh polyglot` runs the shared checks on a copy, then `assert.sh`:
each program prints the schema's values, a comment edit rebuilds only the generator,
`rdeps(//schema:gen)` names one target per language and the Rust test, and an edit to
`rust/src/main.rs` makes the Rust binary and its test run again.
