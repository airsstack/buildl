# Rust workspace

A Cargo workspace with one crate of each kind cargo builds for Rust alone, an rlib, a
proc-macro and a binary, built and tested through the [`rust` plugin](../../plugins/rust/README.md).

```text
Cargo workspace (one build.lua at its root)
  greeting/  rlib        ◄── used by ──┐
  answer/    proc-macro  ◄── used by ──┤   (cargo builds both on the way, nothing copied out)
  cli/       bin  ─────────────────────┴─► rust.binary //:cli   out/cli

  rust.test //:unit             cargo test --workspace     every crate
  rust.test //:greeting_shout   cargo test -p greeting --features shout
```

## What it uses

| Piece | Why |
|---|---|
| no call for `greeting` (rlib) or `answer` (proc-macro) | only rustc reads them: an rlib is passed from crate to crate by cargo, and a proc-macro runs inside rustc while it compiles. Copied to `out/`, neither would have a reader, so the plugin has no function for them; cargo builds them for the binary and the tests |
| `rust.binary("cli", { args = args })` | `cargo build --bin cli -p cli --features greeting/shout`; the binary lands at `out/cli` |
| `rust.package("cli")`, `rust.features({ "greeting/shout" })` | each returns a list (`{ "-p", "cli" }`, `{ "--features", "greeting/shout" }`); the build file joins them into one `args` |
| `rust.test("unit", { args = { "--workspace" } })` | every crate's tests, the rlib's and the proc-macro's expansion included |
| `rust.test("greeting_shout", …)` | one crate's tests with a feature on, `-p greeting --features shout`: one target per way you want it tested |
| `profile = "release"` on the binary | dev debug info keeps the absolute folder cargo ran in, and `buildl verify` would find the bytes differ |

An edit to any crate is an input of every target of the folder: the plugin takes every file of
the build file's folder but `target/` and `.git/`. So an edit to `greeting/src/lib.rs` runs `cli`,
`unit` and `greeting_shout`, and cargo, in each fresh sandbox, builds what it needs again.

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
rev         = "plugin-rust-v0.2.1"
dir         = "plugins/rust"
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
network     = true
```

The same as `examples/rust`: `sh` runs cargo, `rustc` is for `rust.platform` and `version_probe`,
and `HOME` and `PATH` are `unkeyed`, so a target's key holds their names and not their values.

## Run it

Needs the `buildl` binary, a Rust toolchain and `git`. With the published tag:

```sh
buildl plugins fetch     # fetches the rust plugin, writes buildl.lock
buildl build             # cargo builds greeting, answer, then cli
./out/cli                # [rust-workspace] HELLO, BUILDL! answer = 42
buildl test              # tests: 2 passed
buildl log //:unit       # cargo's test output, the rlib's and the proc-macro's tests included
```

From `examples/`, `sh check.sh rust-workspace` runs the shared checks on a copy. The copy first runs
`prepare.sh`: it commits this checkout's `plugins/rust` to a temporary git repository, tags it
`plugin-rust-v0.2.1`, points the copy's `buildl.toml` at it with `git = "file://…"`, and fetches.
Then `assert.sh` checks what `out/cli` prints, that `out/` holds the binary alone, that an edit to
`greeting/src/lib.rs` runs all three targets, and that both tests pass and tested the rlib and the
proc-macro.
