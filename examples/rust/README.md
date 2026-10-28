# Rust starter

A Rust binary and its test, built and run by buildl.

| Target | What it does |
|---|---|
| `//:hello` | `cargo install --path . --root $out --target-dir tt --no-track`, giving `out/hello/bin/hello` |
| `//:unit` | `cargo test --target-dir tt`, a test target |

Both actions read `HOME`, where cargo keeps its registry and toolchains, and `PATH`, where it
finds the linker: without `PATH` the build stops with ``error: linker `cc` not found``.

## Why `cargo install --root $out`

buildl keeps a target's output under `out/`, at the path `$out` names. Of the ways to build a
Rust binary, this one alone puts it there:

| Form | Result |
|---|---|
| `cargo install --path . --root $out --no-track` | `out/hello/bin/hello`, a directory holding regular files only |
| `cargo build --release` with `outputs = { "target/release/hello" }` | `declared output target/release/hello was not produced`: cargo writes under `target/`, and an output is looked for under `out/` |
| `cargo build --target-dir $out` | refused: cargo's build directory holds empty directories, and a directory output holds regular files only |

`--target-dir tt` keeps cargo's build directory inside the action's sandbox, and `--no-track`
stops cargo writing its install records into `$out`. `Cargo.toml` holds an empty `[workspace]`
table so cargo treats the example as a workspace of its own, wherever it is copied. `Cargo.lock`
is committed and is an input of both targets.

## Run it

Needs the `buildl` binary and a Rust toolchain.

```sh
buildl build                  # builds out/hello/bin/hello
./out/hello/bin/hello         # hello from rust
buildl test                   # runs //:unit
echo "// edit" >> src/main.rs
buildl plan                   # run    //:hello: input src/main.rs changed
buildl test                   # runs //:unit again: src/main.rs is one of its inputs
buildl test                   # tests: 1 passed (1 cached)
buildl log //:unit            # what cargo test printed when it last ran
```

From `examples/`, `sh check.sh rust` runs these steps on a copy and checks each result (`assert.sh`).
