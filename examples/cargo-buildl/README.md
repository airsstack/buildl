# cargo buildl

A Cargo workspace set up and kept in line by [`cargo buildl`](../../book/src/ch08-06-cargo-buildl.md).
`buildl.toml`, `build.lua` and `.gitignore` here are what `cargo buildl init` wrote, byte for byte,
not hand-written files; only the Cargo project is the example's own.

```text
Cargo workspace (cargo buildl init wrote build.lua, buildl.toml, .gitignore)
  greet/  rlib          ◄── used by ──┐   (cargo builds it on the way, no call)
  ffi/    staticlib  ───────────────────┼─► rust.library //:ffi    out/libffi.a
  app/    bin  ─────────────────────────┴─► rust.binary  //:app     out/app

  rust.test //:app-test  //:ffi-test  //:greet-test      one per package
```

## The generated `build.lua`

```lua
-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.
local rust = buildl.use("rust")

rust.binary("app", { args = rust.package("app") })
rust.test("app-test", { args = rust.package("app") })
rust.library("ffi", { kind = { "staticlib" }, args = rust.package("ffi") })
rust.test("ffi-test", { args = rust.package("ffi") })
rust.test("greet-test", { args = rust.package("greet") })
-- cargo-buildl: end generated

-- Your own targets go below. `cargo buildl sync` never changes them.
```

The section follows from the `Cargo.toml` files alone. The rlib `greet` gets a test and no other
call: only rustc reads it. Anything of your own, a `release` binary or C code, goes below the end
marker, where `sync` leaves it alone.

The `buildl.toml` is the one `init` writes too: the ceiling and `[plugins.rust]`, the same tables
as in [`rust-workspace`](../rust-workspace/README.md). The build uses cargo's `dev` profile, so
the workspace `Cargo.toml` turns off `debug` and `incremental` for it: without that, `buildl
verify` finds a different binary or static library in each sandbox.

## Run it

```sh
cargo install --locked buildl-cargo    # installs cargo-buildl, which cargo runs as `cargo buildl`
cargo buildl init                      # in a Cargo project with a Cargo.lock: writes the three files, fetches the plugin
cargo buildl build                     # same as `buildl build`
./out/app                              # [cargo-buildl] hello, buildl!
cargo buildl test                      # one test target per package
cargo buildl sync                      # after a package or binary comes or goes: updates the generated lines
cargo buildl sync --check              # in CI: exit code 5 when build.lua is behind Cargo.toml
```

`init` is not run again here: the files are already written, and it refuses a folder that has a
`buildl.toml`. Run it in a copy of the Cargo project alone to see it write them.

From `examples/`, `sh check.sh cargo-buildl` runs the shared checks on a copy; it needs `git`, a
Rust toolchain and `cargo-buildl` beside `buildl` (`cargo build -p buildl-cargo`, or
`CARGO_BUILDL`). The copy first runs `prepare.sh`: it commits this checkout's `plugins/rust` to a
temporary git repository, tags it `plugin-rust-v0.2.0`, points the copy's `buildl.toml` at it with
`git = "file://…"`, and fetches. Then `assert.sh` checks what `out/app` prints, that `out/` holds
the binary and `libffi.a` alone, and that `sync --check` passes. It adds a second binary,
`app/src/bin/extra.rs`, and checks that `sync --check` exits 5, that `sync` adds
`rust.binary("extra", …)` and leaves everything from the end marker on as it was, that `sync
--check` passes again and `plan` runs `//:extra`, and that `cargo buildl check` reaches buildl.
