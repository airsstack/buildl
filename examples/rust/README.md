# Rust

A Rust binary and its test, built with the [`rust` plugin](../../plugins/rust/README.md).

```text
Cargo.toml ─┐
Cargo.lock ─┼──► rust.binary //:hello   (out/hello)
src/main.rs ┤
            └──► rust.test //:unit
```

## What it uses

| Piece | Why |
|---|---|
| `rust.binary("hello", { profile = "release" })` | runs `cargo build --locked --bin hello --profile release --target-dir target` and copies the file cargo wrote to `out/hello` |
| `rust.test("unit")` | runs `cargo test --locked --profile dev --target-dir target`, a test target |
| `profile = "release"` | the default profile, `dev`, keeps the absolute path of the folder it was built in as debug info, and `buildl verify` runs the action in a folder of its own, so it would find the binary differing; `release` holds no path |
| `Cargo.lock` | both targets use `--locked`, and the plugin refuses a folder with a `Cargo.toml` and no `Cargo.lock` |

Both targets take every file in the folder, except `target/` and `.git/`, as inputs, so an edit to
`src/main.rs` runs both. `Cargo.toml` holds an empty `[workspace]` table so cargo treats the
example as a workspace of its own, wherever it is copied.

## The `buildl.toml`

```toml
[ceiling]
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
fs.read     = ["."]
fs.write    = ["out", ".buildl"]

[plugins.rust]
github      = "airsstack/buildl"
rev         = "plugin-rust-v0.2.0"
dir         = "plugins/rust"
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
network     = true
```

`sh` runs cargo: each target is `sh -c 'cargo …'`, so `cargo` needs no entry of its own, and for the
binary the same script copies the file cargo wrote to `$out`. `rustc` is the program the plugin's
extension allows beside `sh`. Allowing `sh` lets a target run any program through `sh -c`, so the
ceiling's program list limits less.

`HOME` is where cargo keeps its registry and toolchains, and `PATH` is where it finds the linker:
without `PATH` the build stops with ``error: linker `cc` not found``. Both are `unkeyed`, since a
value that is a folder on this machine would split the cache between machines: the command gets
the value, only the name enters the key, and `env.unkeyed` allows it in the ceiling and the plugin
table.

`network = true` is the plugin's cap on reaching the network, which cargo needs to download crates.

## Run it

Needs the `buildl` binary, a Rust toolchain and `git`. With the published tag:

```sh
buildl plugins fetch     # clones the repository at plugin-rust-v0.2.0, keeps plugins/rust, writes buildl.lock
buildl build             # builds out/hello
./out/hello              # hello from rust
buildl test              # tests: 1 passed
```

From `examples/`, `sh check.sh rust` runs the shared checks on a copy. The published tag is the
normal route; to run the plugin as this checkout holds it instead, as CI does, the copy first runs
`prepare.sh`: it commits this checkout's `plugins/rust` to a temporary git repository, tags it
`plugin-rust-v0.2.0`, points the copy's `buildl.toml` at that repository with `git = "file://…"`
instead of `github`, and fetches. Then `assert.sh` checks the program's output, that `buildl plan`
after an edit to `src/main.rs` runs `//:hello` and `//:unit`, that the test passes, that a second
`buildl test` is served from the cache, and that `buildl log //:unit` shows what `cargo test`
printed.
