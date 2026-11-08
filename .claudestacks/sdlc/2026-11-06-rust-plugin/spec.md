---
status: approved
created: 2026-11-08
built-on: 8d117dc037f3
---

# Spec: Rust through an official `rust` plugin

A `rust` plugin, fetched from this repository at a tag, gives build files helpers that run plain
`cargo`, the way the `c` plugin's helpers run plain `cc`. An engineer writes `rust.binary("hello")`
instead of the inputs, the `cargo install` trick and the environment that `examples/rust` writes by
hand today. Cargo does all the work; the plugin changes nothing about how Rust builds.

## 1. Premises

| # | Premise | Proof |
|---|---|---|
| P1 | Today a Rust target is written by hand: inputs `Cargo.toml`, `Cargo.lock`, `b.sources("src/*.rs")`; `cargo install --path . --root $out --target-dir tt --no-track`; `env = { HOME = "unkeyed", PATH = "unkeyed" }`; a test is `cargo test --target-dir tt` | `examples/rust/build.lua:1-18`; `examples/polyglot/rust/build.lua:1-10` |
| P2 | The `c` plugin is the model: functions taking `(name, options)`, plain compiler commands, unknown options refused naming the function, `PATH` unkeyed, `version_probe` opt-in | `plugins/c/README.md` ("The functions", "The version probe", "Errors"); `plugins/c/graph.lua:118-216` |
| P3 | `buildl.sources(pattern)` lists files below the build file's folder, `**/` across folders, skipping only `out/` and `.buildl` | `book/src/appendix-03-lua-api.md:218-222` |
| P4 | `cargo build` writes a binary to `target/<profile folder>/<name>`, a staticlib to `lib<name>.a`, a cdylib to `lib<name>.dylib` (macOS) or `lib<name>.so` (Linux) | probe 2026-11-08 (macOS), a workspace with a bin `app` and a lib `mylib` of crate types `rlib, staticlib, cdylib`: `"executable":"./target/debug/app"`; files copied out: `app`, `libmylib.a`, `libmylib.dylib`. Linux: the Rust Reference, `src/linkage.md:32` (fetched 2026-11-08): cdylib "will create `*.so` files on Linux, `*.dylib` files on macOS" |
| P8 | An action starts at the sandbox root, which mirrors the workspace, so a build file in a subfolder names its folder: `examples/polyglot/rust/build.lua` passes `--path rust` | `examples/polyglot/rust/build.lua:8` |
| P10 | The profile folder is `debug` for `dev` and `test`, `release` for `release` and `bench`, the name for a custom profile | probe 2026-11-08: `cargo build --profile test`, `--profile bench`, `--profile fast` (`inherits = "release"`) → `ls target`: `debug`, `fast`, `release` |
| P11 | In `c`, helpers return values and declare nothing (`c.includes`, `c.defines`, `c.shared_name`), except `c.platform`, a discovery of `cc -dumpmachine` that is `nil` on the first pass; the build file composes them, and passes a shared library's name as `output` | `plugins/c/README.md` ("The helpers");  `plugins/c/api.lua:72-104`; `examples/cpp/build.lua:6-20`; `plugins/c/README.md` ("`c.platform`") |
| P12 | `rustc -vV` prints a `host:` line holding the target triple | `rustc +1.99.0 -vV` → `host: aarch64-apple-darwin` (probe 2026-11-08; `release: 1.99.0` from the same run) |
| P13 | The `c` plugin is three modules and their tests: `api.lua` (the public table; `relocate` re-raises an error at the build file's call), `options.lua` (pure: checks and fills the options, names in sorted order), `graph.lua` (declares through the `buildl` table it is given); `tests/` hold a `fake_buildl.lua` | `plugins/c/api.lua:1-30`; `plugins/c/options.lua:1-25`; `plugins/c/graph.lua:1-6`; `ls plugins/c/tests` |
| P9 | The `c` plugin refuses an unknown option with `the option %s is not known` and a wrong type with `the option %s must be %s` | `plugins/c/README.md` (Errors); `plugins/c/options.lua:121` |
| P5 | A missing `Cargo.lock` gives `buildl.sources("Cargo.lock")` an empty list, and a Lua `error()` stops the load with exit 5 | probe 2026-11-08: `build.lua:1: Cargo.lock matches: 0`, `exit 5` |
| P6 | A target that reaches the network says `network = true`; the ceiling has no network list; the approver and `doctor` show it; a plugin's targets need `network = true` in its cap | `book/src/ch04-02-the-ceiling.md:79-80`; `book/src/ch08-03-how-buildl-decides.md:30` |
| P7 | `rustc` and `cargo` on `PATH` are links to the `rustup` launcher; a toolchain change makes cargo recompile everything | `ls -la ~/.cargo/bin/rustc ~/.cargo/bin/cargo` → `cargo -> rustup`, `rustc -> rustup`; probe 2026-11-08: `cargo +1.94` then `cargo +1.99.0 build -v` → `compiling after the switch: 17` |

## 2. The helpers


The plugin has two kinds of function, as `c` has (P2, P11): functions that declare a target and
return nothing, and helpers that return a value and declare nothing. The engineer composes the
values as a build file of `c` composes `c.includes` and `c.defines` into `cflags`
(`examples/c/build.lua:11-15`); no helper tries to cover every case.

```lua
local rust = buildl.use("rust")

rust.binary("hello")
rust.test("unit")

local args = rust.package("server")
for _, a in ipairs(rust.features({ "tls" })) do args[#args + 1] = a end
args[#args + 1] = "--no-default-features"
rust.binary("server", { args = args, profile = "release" })

local platform = rust.platform() -- nil on the first pass
rust.library("ffi", { kind = "cdylib", output = rust.shared_name("ffi", platform) })
```

### 2.1 The functions that declare

Each takes `(name, options)` and returns nothing, as `c.binary` does; `options` may be left out.

| Function | Runs | Output |
|---|---|---|
| `rust.binary(name, o)` | `cargo build --locked --bin <name>` + `o.args` | the binary, named `o.output`, default `name` |
| `rust.library(name, o)` | `cargo build --locked --lib` + `o.args` | named `o.output`, default `lib<name>.a` for a staticlib and `name` for a cdylib, whose platform name the build file gives with `rust.shared_name`, as `examples/cpp/build.lua:6-20` does with `c.shared_name` (P4, P11) |
| `rust.test(name, o)` | `cargo test --locked` + `o.args` | a test target |

Each runs `sh -c 'cd <folder> && cargo … --target-dir target'`, `<folder>` being the build file's
folder (`buildl.folder()`, nothing at the root), since an action starts at the sandbox root (P8);
then it copies the file cargo wrote under `<folder>/target/<profile folder>/` to `$out` (P4).
`rust.test` copies nothing.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `args` | list of strings | `{}` | arguments to `cargo` after the plugin's own, as written on its command line, as `cflags` are to `cc` in `c` |
| `profile` | string | `"dev"` | `--profile`; an option, not an argument, because it names the folder the output is copied from: `debug` for `dev` and `test`, `release` for `release` and `bench`, else the profile's name (P10) |
| `deps` | list of labels | `{}` | targets whose outputs the build reads, such as a generated file (`examples/polyglot`) |
| `output` | string | see above | the file the step writes, as in `c` |
| `kind` | string | — | `rust.library` only, required: `"staticlib"` or `"cdylib"` |
| `version_probe` | boolean | `false` | as in `c`: an always-run `rustc -vV` target each step depends on, so a toolchain change rebuilds (P7) |

### 2.2 The helpers that return a value

Each returns a value and declares nothing, except `rust.platform`, as in `c` (P11).

| Helper | Returns |
|---|---|
| `rust.package(name)` | `{ "-p", name }` |
| `rust.features(list)` | `{ "--features", "<the list joined with commas>" }`; `{}` for an empty list |
| `rust.platform()` | `"linux"` or `"macos"` once a discovery target `:rust.platform` (`rustc -vV`, its `host:` line, P12) has run, else `nil`; the first call declares it, later calls declare nothing |
| `rust.shared_name(name, platform)` | `lib<name>.so` or `lib<name>.dylib`; `nil` when `platform` is `nil` |

A Cargo workspace has one build file at its root, beside `Cargo.lock`, and one call per artifact,
each naming its package with `rust.package`, as `examples/c/build.lua` has one `c.library` or
`c.binary` per artifact. Cargo reads `Cargo.toml` as always: members, profile settings and feature
definitions stay there.

`rust.library` gives the two crate types other languages use, `staticlib` and `cdylib`. An `rlib`
(the default `lib`), a Rust `dylib` and a `proc-macro` are read only by rustc, and cargo builds them
itself for any `rust.binary` or `rust.test` that depends on them, as plain `cargo build` does; such
a library is tested with `rust.test(name, { args = rust.package("<it>") })`, and has no function of
its own.


What every helper fills in, so the engineer writes none of it:

- **inputs:** every file below the build file's folder (`buildl.sources("**/*")`, P3), leaving out
  a `target/` folder a plain `cargo build` left there, so sources, manifests, `build.rs`, toolchain
  files and files read by `include_str!` or a build script are all there;
- **env:** `{ HOME = "unkeyed", PATH = "unkeyed" }` (P1): cargo finds its crates and toolchain
  through `HOME` and the linker through `PATH`;
- **network:** `network = true`, since cargo downloads crates it does not have yet (P6).

## 3. Permissions

`plugins/rust/extension.toml`:

```toml
[extension]
name    = "rust"
version = "0.1.0"
entry   = "main.lua"
api     = 1

[capabilities]
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
```

The workspace allows the same in its ceiling and in `[plugins.rust]`, with `network = true` in the
cap (P6). `cargo` runs inside `sh`, as `ar` does in `c` (P2), so it is not on the list; `rustc`
is, for `version_probe` and `rust.platform`. Programs cargo starts (rustc, the linker, build scripts) are not seen by
the ceiling.

## 4. Errors

A message names the function and the target, as in `c` (P2, P9); each stops the load with exit 5 (P5).

| Case | Message |
|---|---|
| no `Cargo.lock` | `rust.binary(hello): no Cargo.lock beside Cargo.toml; run cargo generate-lockfile` |
| no `Cargo.toml` | `rust.binary(hello): no Cargo.toml in <folder>` |
| options that are not a table | `rust.binary(hello): the options must be a table` |
| an unknown option | `rust.binary(hello): the option bogus is not known` |
| `kind` given to `rust.binary` or `rust.test` | `rust.binary(hello): the option kind is refused` |
| a wrong type | `rust.binary(hello): the option args must be a list of strings` |
| `rust.library` without `kind`, or another value | `rust.library(mylib): kind must be "staticlib" or "cdylib"` |
| `rust.platform` on a host that is neither | `rust.platform: the target triple <triple> is neither macos nor linux`, as `c.platform` (P11) |
| `rust.package` without a string, `rust.features` without a list of strings | `rust.package: the name must be a string`; `rust.features: the features must be a list of strings`, as `c.includes` (P13) |

What cargo itself refuses (a bad `Cargo.toml`, a path dependency outside the folder, a failed
download) fails the action with cargo's own message, as a build failure.

## 5. Limits, for the book

- Each build runs in a fresh sandbox, so cargo compiles the dependencies again whenever an input
  changes, as `examples/rust` does today (P1). Plain `cargo` keeps its own `target/` and is faster
  for a one-crate edit loop.
- Crates live in cargo's cache under `HOME` (`~/.cargo`), outside buildl's store.
- Programs cargo starts are unseen (§3).

## 6. What else changes

| Where | Change |
|---|---|
| `plugins/rust/` | new, laid out as `plugins/c` (P13): `extension.toml`, `main.lua` (`return require("api").new(buildl)`), `api.lua` (the public table; errors re-raised at the build file's call), `options.lua` (pure: check and fill), `graph.lua` (declares through `buildl`), `tests/*_test.lua` with `fake_buildl.lua`, `README.md` shaped as `plugins/c/README.md` (setting it up, the functions, the helpers, the programs it may run, errors) |
| `Makefile.toml` `plugins` | stages and tests each plugin folder in a temporary folder of its own, so `plugins/c` and `plugins/rust` files of the same name do not clash (`Makefile.toml:404-412`) |
| `examples/rust`, `examples/polyglot/rust` | use `rust.binary` and `rust.test`, the plugin fetched through a local git tag by `prepare.sh`, as `examples/c/prepare.sh` does |
| book | new `ch08-05-the-rust-plugin.md` (with the limits of §5), `SUMMARY.md`; `ch11-09-what-it-promises.md` gains the limits of §5; `ch07-00-discovery-targets.md:306-307`, where it speaks of Cargo tracking its own sources, points to the plugin |
| skills | `buildl` and `buildl-migrate` and their references name the plugin |
| `README.md`, `CLAUDE.md`, `CHANGELOG.md` | name the plugin |
| release | tag `plugin-rust-v0.1.0` when the branch reaches `main` |

## 7. Testing

- Lua unit tests in `plugins/rust/tests`: each helper's command, inputs, env, output and every
  error of §4; `cargo make plugins`.
- `examples/rust` and `examples/polyglot/rust` build and test through the plugin in
  `examples/check.sh`, in CI's Examples jobs. As `examples/c` does (`examples/c/README.md`,
  `examples/c/assert.sh`), each README draws the targets and has a "What it uses" table and its
  `buildl.toml` explained, and each `assert.sh` checks what the binary prints, which targets
  `buildl plan` runs after a source edit, and that `buildl test` passes.

## 8. Non-goals

- Changing how cargo builds: no vendoring, no cached dependency layer, no rewritten files.
- Targets declared without a helper call; running `rustc` per crate.
- Cross-compiling, Windows.
- Any change to buildl's core.
