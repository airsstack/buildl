# Rust FFI

C and Rust calling each other, built through two plugins in one workspace. C calls Rust: the
[`rust` plugin](../../plugins/rust/README.md) builds a static and a shared library from a Cargo
workspace, and the [`c` plugin](../../plugins/c/README.md) builds the C programs that link them.
Rust calls C: the `c` plugin builds a static library, and a Rust binary links it.

```text
C calls Rust
  rust/mathlib ─► rust.library //rust:mathlib ──┬──► c.binary //c:app_static    out/c/app_static
                 (staticlib) out/rust/libmathlib.a └──► c.test   //c:mathlib_test
  rust/greeter ─► rust.library //rust:greeter ─────► c.binary //c:app_shared    out/c/app_shared
                 (cdylib) out/rust/libgreeter.so / .dylib   (found at run time by rpath)

Rust calls C
  c/lib/clamp.c ─► c.library //c:clamp ─────────────► rust.binary //rust:calc   out/rust/calc
                  out/c/libclamp.a                      (calc/build.rs links it)

  rust.test //rust:unit   cargo test, every crate (deps //c:clamp)
  c/include/*.h ─► every C compile
```

## What it uses

| Piece | Why |
|---|---|
| `buildl.subdir("rust")`, `buildl.subdir("c")` | the rust plugin takes every file of its folder as an input; with the C sources in a folder of their own, a C edit leaves cargo alone |
| `rust.library("mathlib", { kind = { "staticlib" }, args = rust.package("mathlib") })` | `cargo build --lib -p mathlib`, then `libmathlib.a` copied to `out/rust/`; the call's name is the crate's, so the plugin knows the file |
| `rust.library("greeter", { kind = { "cdylib" } })` | the shared library, `libgreeter.so` on Linux and `libgreeter.dylib` on macOS, as cargo names it |
| `rust.platform()` | nil on the first pass, so `rust/build.lua` declares the shared library on the second |
| `profile = "release"` | dev debug info keeps the absolute folder cargo ran in, and `buildl verify` would find the bytes differ |
| `rust/greeter/build.rs` | names the library `libgreeter.so` (soname) or `@rpath/libgreeter.dylib` (install name); left alone, it is named by the sandbox path it was linked in, and the binary would not start |
| `c.binary(…, { deps = { "//rust:mathlib" } })` | a target's output is linked in after the objects; the label reaches across folders |
| `ldlibs` on Linux | a Rust static library holds the Rust standard library, which needs `-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc` (`rustc --print native-static-libs`); macOS needs none |
| `c.rpath(platform)` with `/../rust` appended | the plugin's flag names the binary's own folder; the value is a list, so the build file points it at `out/rust/` |
| `c.library("clamp", …)` | the C side of Rust calling C: one object per source, then `ar rcs` makes `out/c/libclamp.a` |
| `rust.binary("calc", { deps = { "//c:clamp" } })` | the dependency puts `out/c/libclamp.a` in the sandbox, which mirrors the workspace, so `calc/build.rs` finds it at `$CARGO_MANIFEST_DIR/../../out/c` and tells rustc to link it (`rustc-link-search`, `rustc-link-lib=static=clamp`) |
| `greeter` and `calc` declared on the second pass | the c plugin declares `//c:clamp` on its second pass only, once its compiler discovery has run, so a target depending on it waits for the second pass as well |
| `c.platform()` | the C side asks `cc`; its calls are made on both passes, since the plugin declares its own discovery targets on the first, and only the link flags wait |
| `rust.test("unit", { deps = { "//c:clamp" } })`, `c.test("mathlib_test")` | the Rust tests through cargo, which builds every crate and so links the C library too, and the static library tested from C |

## The `buildl.toml`

```toml
[ceiling]
proc.run    = ["cc", "rustc", "sh"]
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

[plugins.c]
github      = "airsstack/buildl"
rev         = "plugin-c-v0.2.0"
dir         = "plugins/c"
proc.run    = ["cc", "sh"]
env.read    = ["PATH"]
env.unkeyed = ["PATH"]
```

The ceiling is what the two plugins allow together. Each plugin's table is held to its own
`extension.toml` before the ceiling: the rust plugin runs `sh` (cargo inside it) and `rustc` (for
`rust.platform`), the c plugin `cc` and `sh`. `HOME` and `PATH` are `unkeyed`, so a target's key
holds their names and not their values, and another machine's results serve this one.

## Run it

Needs the `buildl` binary, a Rust toolchain, a C compiler and `git`. With the published tags:

```sh
buildl plugins fetch     # fetches both plugins, writes buildl.lock
buildl build             # the platform discoveries, the libraries, then the programs
./out/c/app_static       # [rust-ffi] static: mathlib_add(2, 3) = 5, mathlib_mul(6, 7) = 42
./out/c/app_shared       # [rust-ffi] shared: hello from a Rust shared library
./out/rust/calc          # [rust-ffi] rust calls C: clamp(15, 0, 10) = 10, clamp(-3, 0, 10) = 0
buildl test              # tests: 2 passed
```

From `examples/`, `sh check.sh rust-ffi` runs the shared checks on a copy. The copy first runs
`prepare.sh`: it commits this checkout's `plugins/rust` and `plugins/c` to one temporary git
repository, tags it `plugin-rust-v0.2.0` and `plugin-c-v0.2.0`, points both plugins in the copy's
`buildl.toml` at it with `git = "file://…"`, and fetches. Then `assert.sh` checks what both programs
print, that a comment added to `c/include/clamp.h` reruns the object of `clamp.c` and leaves
`//rust:calc` cached (the object comes out the same bytes), that a comment added to
`c/include/greeter.h` runs one C object and no Rust target, that a comment added to
`rust/mathlib/src/lib.rs` runs `//rust:mathlib`, and that both tests pass.
