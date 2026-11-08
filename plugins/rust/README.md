# The `rust` plugin

Builds and tests Rust crates with the `cargo` you already have. The plugin runs plain `cargo`: it
changes nothing about how Rust builds, and it fills in the inputs, the environment and the network
setting that a build file would otherwise write by hand.

It works on Linux and macOS, for binaries, `staticlib` and `cdylib` libraries, and tests.

## Setting it up

Name the plugin in `buildl.toml`, give it the programs and the environment it may use, and let the
ceiling allow them too:

```toml
[ceiling]
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
fs.read     = ["."]
fs.write    = ["out", ".buildl"]

[plugins.rust]
github      = "airsstack/buildl"
rev         = "plugin-rust-v0.1.0"
dir         = "plugins/rust"
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
network     = true
```

`network = true` is in the plugin's table because cargo downloads the crates it does not have yet.
The ceiling has no network list. Allowing `sh` lets a target run any program through `sh -c`, so
the ceiling's program list then limits less.

Then run `buildl plugins fetch` once. A build file loads the plugin with `buildl.use`:

```lua
local rust = buildl.use("rust")

rust.binary("hello")
rust.test("unit")
```

The build file sits in the crate's folder, beside `Cargo.toml` and `Cargo.lock`.

## The functions

Each takes `(name, options)` and returns nothing; `options` may be left out. An option it does not
know is refused, and the message names it.

| Function | Declares |
|---|---|
| `rust.binary(name, o)` | a target that runs `cargo build --locked --bin <name>` and copies the binary to the output |
| `rust.library(name, o)` | a target that runs `cargo build --locked --lib` and copies the library to the output |
| `rust.test(name, o)` | a test target `name` that runs `cargo test --locked` |

Each runs `sh -c`, in the build file's folder (`cd <folder> && …`, no `cd` at the workspace root):

```text
cargo <build or test> --locked … --profile <profile> --target-dir target <args>
```

then, for `rust.binary` and `rust.library`, copies the file cargo wrote under
`<folder>/target/<profile folder>/` to the output. `rust.test` copies nothing.

| Option | Type | Meaning | Default |
|---|---|---|---|
| `args` | list of strings | arguments to `cargo`, after the plugin's own, as written on its command line | `{}` |
| `profile` | string | `--profile`; it also names the folder the output is copied from: `debug` for `dev` and `test`, `release` for `release` and `bench`, else the profile's own name | `"dev"` |
| `deps` | list of labels | targets whose outputs the build reads, such as a generated file | `{}` |
| `output` | string | the file the step writes | `rust.binary`: `name`; `rust.library`: `lib<name>.a` for a `staticlib`, `name` for a `cdylib` |
| `kind` | string | `rust.library` only, required: `"staticlib"` or `"cdylib"` | none |
| `version_probe` | boolean | see below | `false` |

`rust.test` has no `output`. `kind` given to `rust.binary` or `rust.test` is refused.

What the plugin fills in on every target, so you write none of it:

| Setting | Value |
|---|---|
| inputs | every file below the build file's folder (`buildl.sources("**/*")`), except a `target/` folder a plain `cargo build` left there: sources, manifests, `build.rs`, toolchain files, and files read by `include_str!` or a build script |
| env | `{ HOME = "unkeyed", PATH = "unkeyed" }`: cargo finds its crates and toolchain through `HOME` and the linker through `PATH`; only the names are in the key |
| network | `network = true` |

A `cdylib` is copied from `lib<name>.so` when cargo wrote that file, else from `lib<name>.dylib`.
Pass the platform's name as `output`, as in the example below. A library's `name` is the name of
the file cargo writes, so it is the crate's library name: `my-lib` becomes `my_lib`.

## The helpers

Each returns a value and declares nothing, except `rust.platform`. Put the value where you want it.

| Helper | Returns |
|---|---|
| `rust.package(name)` | `{ "-p", name }` |
| `rust.features(list)` | `{ "--features", "<the list joined with commas>" }`; `{}` for an empty list |
| `rust.platform()` | `"linux"` or `"macos"`, or `nil` before discovery has run (see below) |
| `rust.shared_name(name, platform)` | `lib<name>.so` or `lib<name>.dylib`; `nil` when `platform` is `nil` |

Helpers compose into `args`:

```lua
local rust = buildl.use("rust")

local args = rust.package("server")
for _, a in ipairs(rust.features({ "tls" })) do args[#args + 1] = a end
args[#args + 1] = "--no-default-features"

rust.binary("server", { args = args, profile = "release" })
```

A shared library:

```lua
local rust = buildl.use("rust")
local platform = rust.platform()

rust.library("ffi", {
  kind   = "cdylib",
  output = rust.shared_name("ffi", platform),
})
```

`rust.platform()` is `nil` on the first pass (see "How `rust.platform` works"), and so is
`rust.shared_name("ffi", platform)`. `output` is then `nil` and the library's default, `ffi`,
applies; the second pass has the platform's name.

## A Cargo workspace

A workspace has one build file at its root, beside `Cargo.lock`, and one call per artifact, each
naming its package with `rust.package`. Cargo reads `Cargo.toml` as always: members, profile
settings and feature definitions stay there.

```lua
local rust = buildl.use("rust")

rust.binary("server", { args = rust.package("server"), profile = "release" })
rust.binary("cli",    { args = rust.package("cli") })
rust.library("codec", { kind = "staticlib", args = rust.package("codec") })

rust.test("server-test", { args = rust.package("server") })
rust.test("codec-test",  { args = rust.package("codec") })
```

The inputs are every file below the root, so a change in any member rebuilds every target.

## Crate types

`rust.library` gives the two crate types other languages use, `staticlib` and `cdylib`. An `rlib`
(the default `lib`), a Rust `dylib` and a `proc-macro` are read only by rustc. Cargo builds them
itself for any `rust.binary` or `rust.test` that depends on them, as a plain `cargo build` does, so
they have no function of their own. To test one, use `rust.test`:

```lua
rust.test("codec-test", { args = rust.package("codec") })
```

## How `rust.platform` works

The first call in a build file declares a discovery target `:rust.platform` that runs
`rustc -vV`; later calls return the same value and declare nothing. The `host:` line of the output
holds the target triple. A triple holding `apple-darwin` gives `"macos"` and one holding `linux`
gives `"linux"`. A call returns `nil` until the discovery has run: on the first of the two
passes a build file is declared in.

## The version probe

`rustc` and `cargo` on `PATH` are links to the `rustup` launcher, and a key records the program
file, so a toolchain change would not change the key. With `version_probe = true` the call
declares `<name>.version` (`rustc -vV`), an always-run target, and the target depends on it, so a
new toolchain rebuilds it.

The cost is in `buildl plan`: an always-run target leaves every target that depends on it listed
as conditional, even right after a full build, because `plan` never runs the probe. A build still
takes the target from the cache when the probe's output is the same.

## The programs it may run

The plugin's manifest lists `sh` and `rustc`. `cargo` runs inside `sh`, as `ar` runs inside `sh` in
the `c` plugin, so it is not on the list; `rustc` is for `version_probe` and `rust.platform`. A
program outside the list is refused even when the workspace allows it, and the workspace's cap and
ceiling narrow the list further.

Programs cargo starts itself (rustc, the linker, build scripts) are not seen by the ceiling.

## Limits

- Each build runs in a fresh sandbox, so cargo compiles the dependencies again whenever an input
  changes. Plain `cargo` keeps its own `target/` and is faster for an edit loop on one crate.
- Crates live in cargo's cache under `HOME` (`~/.cargo`), outside buildl's store.
- Programs cargo starts are not seen by the ceiling.

## Errors

A message names the function and the target, such as `rust.binary(hello): the option bogus is not
known`, and carries the file and line of your call. Each stops the load with exit code 5.

| Case | Message |
|---|---|
| no `Cargo.toml` in the build file's folder | `rust.binary(hello): no Cargo.toml in <folder>` (`.` at the root) |
| no `Cargo.lock` | `rust.binary(hello): no Cargo.lock beside Cargo.toml; run cargo generate-lockfile` |
| options that are not a table | `rust.binary(hello): the options must be a table` |
| an unknown option | `rust.binary(hello): the option bogus is not known` |
| `kind` given to `rust.binary` or `rust.test` | `rust.binary(hello): the option kind is refused` |
| a wrong type | `rust.binary(hello): the option args must be a list of strings` |
| `rust.library` without `kind`, or another value | `rust.library(mylib): kind must be "staticlib" or "cdylib"` |
| `rust.platform` on a host that is neither | `rust.platform: the target triple <triple> is neither macos nor linux` |
| `rustc -vV` without a `host:` line | `rust.platform: the output of rustc -vV has no host line` |
| `rust.package` without a string | `rust.package: the name must be a string` |
| `rust.features` without a list of strings | `rust.features: the features must be a list of strings` |

What cargo itself refuses (a bad `Cargo.toml`, a path dependency outside the folder, a failed
download) fails the action with cargo's own message, as a build failure.
