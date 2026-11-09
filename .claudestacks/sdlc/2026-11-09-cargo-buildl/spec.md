---
status: approved
created: 2026-11-09
built-on: b443fcba6dc0
---

# Spec: cargo-buildl, buildl from cargo

A new crate, `buildl-cargo`, ships the binary `cargo-buildl`. Cargo runs it as `cargo buildl`. It
sets up a Cargo project for buildl (`init`), keeps the build file in line with `Cargo.toml`
(`sync`), and runs every buildl command with buildl's own flags, output and exit codes. To share
one command line, the command-line code moves from the `buildl-cli` binary into the `buildl`
library without changing what it does. `buildl-core`, `buildl-lua` and the `rust` plugin are not
changed.

## 1. Facts the design rests on

| # | Fact | Proof |
|---|---|---|
| F1 | Cargo runs `cargo-<name>` from `$PATH` for `cargo <name>`. The first argument is the file name, the second is `<name>`, and the rest are passed on unchanged | `external-tools.md:276-293`, fetched from `rust-lang/cargo` `doc/book/src/reference/external-tools.md`: "the first argument to the subcommand will be the filename … The second argument will be the subcommand name itself … Any additional arguments on the command line will be forwarded unchanged" |
| F2 | `cargo help <name>` runs `cargo-<name> <name> --help` | same file `:295-298` |
| F3 | Cargo sets `CARGO` (the path of the running cargo) for a third-party subcommand, which should drive cargo through its CLI, `cargo metadata` included | `environment-variables.md:505-509` (saved as `env.md`; "`CARGO` --- Path to the `cargo` binary performing the build"); `external-tools.md:300-309` |
| F4 | `cargo metadata --no-deps` reports only the workspace members | `cargo-metadata.md:352-354`, fetched from `doc/book/src/commands/cargo-metadata.md`: "Output information only about the workspace members and don't fetch dependencies" |
| F5 | Each target in that output has `name`, `kind`, `crate_types` and `required-features`, and the top level has `workspace_root` | `cargo-metadata.md:147,156,172,330` |
| F6 | A library target's `name` is the lib name with `-` turned into `_`. A lib's `kind` is its crate-type list in Cargo's order. Bins, examples, tests and build scripts have kinds `bin`, `example`, `test` and `custom-build`; an rlib is `lib` and a proc-macro is `proc-macro` | probe: `cargo metadata --format-version 1 --no-deps` on a package `my-lib` with `crate-type = ["cdylib","staticlib","rlib"]` printed `my_lib ['cdylib','staticlib','rlib']`, `extra ['bin']`, `tool-a ['bin']`, `ex ['example']`, `it ['test']`, `build-script-build ['custom-build']`. On `examples/rust-workspace` it printed `greeting ['lib']`, `answer ['proc-macro']`, `cli ['bin']` |
| F7 | `cargo metadata --no-deps` writes no `Cargo.lock` | same probe: after it ran, `ls` listed `build.rs Cargo.toml examples src` and no `Cargo.lock` |
| F8 | `cargo build --locked` without a `Cargo.lock` fails | probe: `cargo build --locked --lib -p my-lib` printed `error: cannot create the lock file … because --locked was passed to prevent this` |
| F9 | `-p <package>` works in a single-package project | probe: after `cargo generate-lockfile`, `cargo build --locked --bin tool-a -p my-lib` printed `Finished dev profile` |
| F10 | `rust` plugin 0.2.0: `rust.library(name, { kind = { … } })` takes the crate types `"staticlib"` and `"cdylib"` as a list, and copies each file cargo built under cargo's name; `rust.binary(name)` builds `--bin name`; `rust.test` runs `cargo test`; `rust.package(p)` is `{ "-p", p }`; an rlib, a dylib and a proc-macro get no call | `plugins/rust/README.md` (options table and "Crate types"); `plugins/rust/extension.toml:3` `version = "0.2.0"`; the probe `rust.library("both", { kind = { "cdylib", "staticlib" } })` built `out/libboth.dylib` and `out/libboth.a`, and `examples/check.sh` passed for `rust-ffi`, `rust`, `rust-workspace` and `polyglot` |
| F11 | A buildl target name is 1–256 bytes of ASCII letters, digits, `.`, `-` and `_`, and is neither `.` nor `..` | `crates/buildl-core/src/types/target_name.rs:33-34` |
| F12 | buildl finds its workspace in the nearest directory, at or above the start, that holds `buildl.toml` | `crates/buildl/src/manifest/root.rs:46-75` (`WorkspaceRoot::find`, `canonical.ancestors().find(…)`) |
| F13 | `[workspace]` in `buildl.toml` takes `name` (not empty), `entry` (default `"build.lua"`), `out` (default `"out"`) and `config` (must be `1`) | `book/src/appendix-02-buildl-toml.md:29-32` |
| F14 | The command line is `crates/buildl-cli/src/{cli,run,exit,doctor_text,remote_switches,terminal,console}.rs`, and `main.rs` only reads the process (`env`, terminal, streams) and calls `run::run` | `ls crates/buildl-cli/src`; `crates/buildl-cli/src/main.rs:109-134`; `cli.rs:33-86` (`Cli`, `Command`); `run.rs:84,99` (`run`, `Cli::try_parse_from`) |
| F15 | The exit codes are `Exit` 0–7 in `crates/buildl-cli/src/exit.rs` | `exit.rs:23-40`; `book/src/appendix-04-exit-codes.md:6-15` |
| F16 | `buildl` exposes `Workspace::open` and `Workspace::fetch_plugins` | `crates/buildl/src/lib.rs:16-17`; `run.rs:252` (`fetch_plugins`) |
| F17 | The guards list the members (`guard-crate-edges`) and the composition modules of `buildl` (`guard-adapter-isolation`) | `Makefile.toml:142` (`for member in buildl-core buildl-lua buildl buildl-cli`); `Makefile.toml:236` (`composition="local_ports workspace"`) |
| F18 | `deny.toml` lets only `buildl-cli` depend on `clap` and on `buildl` | `deny.toml:93,114` |
| F19 | cargo-dist builds every package of the workspace (`members = ["cargo:."]`), and `dist = false` on a package leaves it out | `dist-workspace.toml`; `dist-config.md:197`, fetched from `axodotdev/cargo-dist` `book/src/reference/config.md`: "`dist = false` on a package can be used to force dist to ignore it"; `:11`: the `[dist]` keys may be written as `[package.metadata.dist]` in `Cargo.toml` |
| F20 | `clap` is a workspace dependency with `derive` | root `Cargo.toml:76` |
| F21 | A build file comment is a Lua `--` line | `examples/rust-workspace/build.lua:3` |
| F22 | clap's derive lets a subcommand enum `flatten` another one | `clap-4.6.7/src/_derive/mod.rs:190`: "`flatten`: Delegates to the variant for more subcommands" |
| F23 | buildl's plugin fetch without `git` on `PATH` ends with exit 3 | `crates/buildl/tests/e2e/fetch.rs:266` (`a_fetch_without_git_is_a_fetch_error`); `crates/buildl-cli/src/exit.rs:110` maps `Error::PluginFetch` |
| F24 | Fixtures live inside the package whose tests read them; no test reads another package's folder | `book/src/ch11-03-four-crates.md:499-500` |
| F25 | A module file named like its parent module fails clippy's `module_inception` under `-D warnings` | review probe in a scratch crate: `error: module has the same name as its containing module --> src/cli/mod.rs:2:1` |

## 2. Crates

```text
 buildl-core ──► buildl-lua ──► buildl ──┬──► buildl-cli     [[bin]] buildl         main.rs only
                                (+ cli   │
                                 module, └──► buildl-cargo   [[bin]] cargo-buildl   init / sync /
                                 + clap)                                            everything else
```

| Crate | Change |
|---|---|
| `buildl-core`, `buildl-lua`, `plugins/rust` | none |
| `buildl` | gains the folder module `crates/buildl/src/cli/`: the files of F14 except `main.rs`, with their unit tests, `cli.rs` renamed `args.rs` (F25); and the dependency `clap` (F20) |
| `buildl-cli` | keeps `main.rs` and its integration tests under `tests/`; `main` calls `buildl::cli::run`; its normal dependencies become `buildl` alone |
| `buildl-cargo` (new) | `[[bin]] name = "cargo-buildl"`; depends on `buildl`, `clap`, `serde`, `serde_json`; dev-dependency `tempfile`; `[package.metadata.dist] dist = false`, so it ships through `cargo install buildl-cargo` alone (F19) |

## 3. The shared command line in `buildl`

The files move unchanged, apart from visibility and paths. `buildl` binary behaviour is
byte-for-byte the same. Public items of `buildl::cli`:

| Item | Purpose |
|---|---|
| `Cli` | the `buildl` parser, as today |
| `Command` | the buildl commands enum, public so another parser can flatten it; the argument types its variants carry (`BuildArgs`, `PluginsArgs`, `PluginsCommand` and the rest) are public with it, since a public enum cannot carry private types |
| `Color` | `--color auto\|always\|never` |
| `Exit` | the exit codes of F15 |
| `Terminal`, `RemoteSwitches`, `StdConsole` | what `main` reads from the process, as today. `cargo-buildl`'s `main` reads exactly what `buildl-cli`'s does: `is_terminal` on stderr, `TERM`, `NO_COLOR`, `BUILDL_REMOTE_UPLOAD`, `BUILDL_REMOTE_STRICT`, `BUILDL_REMOTE_EXECUTE` (F14), so a pass-through command behaves as `buildl`'s |
| `run(args, start, terminal, remote, stdout, stderr, console) -> Exit` | parse with `Cli` and run: what `buildl-cli`'s `main` calls |
| `execute(color, command, start, terminal, remote, stdout, stderr, console) -> Exit` | run one already-parsed `Command`: what `buildl-cargo` calls; `run` calls it after parsing |

`cli` imports adapters (`approver`, `clock`, `plugins`, `remote`, `report`, `watch`), so it joins
`local_ports` and `workspace` as a composition module in `guard-adapter-isolation` (F17).

## 4. The `cargo buildl` command line

```text
 cargo buildl <args>  ─► cargo-buildl buildl <args>      (F1)
                          │ drop argv[1] when it is exactly "buildl"
                          ▼
                 clap parser "cargo buildl"
                 ├─ init                 → §6
                 ├─ sync [--check]       → §7
                 └─ <buildl Command>     → buildl::cli::execute   (flatten of buildl::cli::Command, F22)
                 global: --color, --help, --version
```

- When argv[1] is not `buildl` (the binary run directly), nothing is dropped.
- `cargo buildl build -k`, `cargo buildl test --stop` and every other buildl command take buildl's
  flags and give buildl's output and exit codes, because the same `execute` runs them.
- `--version` prints `buildl-cargo`'s version. `cargo help buildl` gives the help, as F2 says.
- A usage error is clap's, with exit code 2, as in buildl. Parse errors and `--help` name
  `cargo buildl`. An error raised inside `execute`, such as a target that is not a label, keeps
  buildl's text, which names `buildl`.

## 5. Reading the Cargo project

`buildl-cargo` runs `$CARGO`, or `cargo` when `CARGO` is unset (F3), as
`metadata --format-version 1 --no-deps` in the current directory, and reads `workspace_root`
and, for each package, `name` and its targets' `name`, `kind`, `crate_types` and
`required-features` (F4, F5) with `serde_json`. This writes no `Cargo.lock` (F7).

## 6. What goes in `build.lua`

The **generated section** follows from the metadata alone, so the same `Cargo.toml` always gives
the same bytes:

| In the metadata | Generated |
|---|---|
| a library target, one whose `kind` holds none of `bin`, `example`, `test`, `bench`, `custom-build`, and whose `crate_types` holds `cdylib` and/or `staticlib` | `rust.library("<target name>", { kind = { <those of the two it holds, in Cargo's order> }, args = rust.package("<package>") })` |
| a `bin` target with no `required-features` | `rust.binary("<target name>", { args = rust.package("<package>") })` |
| every package | `rust.test("<package>-test", { args = rust.package("<package>") })` |
| `lib` (rlib), `dylib`, `proc-macro`, `example`, `test`, `bench`, `custom-build`, and a bin with `required-features` | no call: cargo builds them itself, or a plain `cargo build` skips them (F10) |

- No `profile` is written, so the plugin's default `dev` applies.
- Packages are sorted by name. Within a package the library comes first, then the binaries sorted
  by name, then the test.
- Example, for `examples/rust-workspace` (F6):

```lua
-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.
local rust = buildl.use("rust")

rust.test("answer-test", { args = rust.package("answer") })
rust.binary("cli", { args = rust.package("cli") })
rust.test("cli-test", { args = rust.package("cli") })
rust.test("greeting-test", { args = rust.package("greeting") })
-- cargo-buildl: end generated

-- Your own targets go below. `cargo buildl sync` never changes them.
```

- The two marker lines are Lua comments (F21), so the file is an ordinary build file. Everything
  outside them belongs to the engineer.
- A begin marker is any line that starts with `-- cargo-buildl: begin generated`, and an end
  marker any line that starts with `-- cargo-buildl: end generated`; the rest of the line is free,
  and a trailing `\r` is ignored. `sync` keeps both marker lines as they are and compares and
  rewrites only the lines between them, which it writes with `\n` endings.
- A name outside buildl's target-name rule (F11), or two generated targets with the same name, is
  refused with exit code 5. The message names the target and the package(s), and nothing is
  written.

## 7. `cargo buildl init`

All checks run before anything is written:

| Step | On failure |
|---|---|
| 1. read the metadata (§5) | cargo cannot be started: exit 3. cargo exits non-zero: its stderr is shown, exit 5 |
| 2. `buildl.toml` or `build.lua` already in `workspace_root` | exit 5: `init: <file> already exists; this folder is set up already, or remove <file> to start over`. Nothing is written |
| 3. no `Cargo.lock` in `workspace_root` | exit 5: `init: no Cargo.lock beside Cargo.toml; run cargo generate-lockfile`. Nothing is written. The plugin's `--locked` would fail without it (F8) |
| 4. generate `build.lua` (§6) | exit 5 as in §6. Nothing is written |
| 5. write `build.lua`, then `buildl.toml`, then `.gitignore` | an I/O error: exit 3, and the files this run created are removed, so the folder is as it was |
| 6. fetch the plugin by running `buildl::cli::execute` with `Command::Plugins(PluginsArgs { command: PluginsCommand::Fetch })` in `workspace_root` (F16), so the output and exit code are `buildl plugins fetch`'s own | that command's exit code (3 for `git`, F23). The files stay; the message says to run `cargo buildl plugins fetch` |

`buildl.toml` is written as:

```toml
[workspace]
name   = "<the last component of workspace_root>"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
fs.read     = ["."]
fs.write    = ["out", ".buildl"]

[plugins.rust]
github      = "airsstack/buildl"
rev         = "<RUST_PLUGIN_TAG>"
dir         = "plugins/rust"
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
network     = true
```

This is `examples/rust-workspace/buildl.toml` without its comments (F13). `name` is written as a
TOML basic string, with `"` and `\` escaped; a folder name that is not UTF-8 is written lossily.
`RUST_PLUGIN_TAG` is a constant of `buildl-cargo`, today `plugin-rust-v0.2.0`: each release pins
the plugin it was tested with. The release pushes that tag before it publishes `buildl-cargo`. `.gitignore` in `workspace_root` gets `/out/` and `/.buildl/`,
each added only when that exact line is missing, and is created when absent.

On success `init` prints `init: wrote build.lua, buildl.toml, .gitignore` before the fetch lines.
Every message of `init` and `sync` goes to standard error, an error in buildl's error style, as
`buildl` prints its own.

Run from a member's folder, `init` still writes at `workspace_root`. Every buildl command then
finds that workspace from any folder below it (F12).

## 8. `cargo buildl sync [--check]`

1. Read the metadata (§5) and generate the section (§6), with the same failures.
2. Read `build.lua` in `workspace_root`. If it is missing: exit 5,
   `sync: no build.lua; run cargo buildl init`.
3. Find the markers (§6). With none: exit 5, `sync: build.lua has no generated section; add the
   lines "-- cargo-buildl: begin generated" and "-- cargo-buildl: end generated" where it goes`.
   With more than one of either, a begin without an end, an end without a begin, or the end
   before the begin: exit 5,
   `sync: build.lua must hold one begin line and, after it, one end line`.
4. Without `--check`: when the section differs, replace the lines from begin to end, keeping every
   other byte, and print `sync: build.lua updated`. Otherwise print
   `sync: build.lua is up to date` and write nothing.
5. With `--check`: write nothing. When up to date, exit 0. When not, exit 5 with
   `sync: build.lua is out of date with Cargo.toml; run cargo buildl sync`.

`sync` never touches `buildl.toml`.

## 9. Exit codes

`cargo buildl` uses buildl's codes (F15) and adds no new one:

| Code | `init` / `sync` cause |
|---|---|
| 0 | done; `sync --check` found it up to date |
| 2 | a usage error |
| 3 | cargo cannot be started, cargo's output cannot be read as metadata, a file cannot be written, the plugin fetch failed |
| 5 | cargo refused the manifest; files already exist (`init`); no `Cargo.lock`; a name refused or claimed twice; no `build.lua` or broken markers (`sync`); out of date (`sync --check`) |

## 10. Guards, docs and release

| File | Change |
|---|---|
| `Makefile.toml` | `guard-crate-edges` lists `buildl-cargo` (F17); `guard-adapter-isolation` adds `cli` to `composition` |
| `crates/expected-edges.txt` | `clap` under `buildl`; `buildl-cli` loses `clap`; a `# buildl-cargo` block |
| `deny.toml` | `clap` wrappers `buildl`, `buildl-cargo`; `buildl` wrappers `buildl-cli`, `buildl-cargo`; `tempfile` wrappers add `buildl-cargo` (F18); the `reason` texts of those three bans say the same |
| `book/src/ch11-03-four-crates.md` | five crates: the tree, the diagrams, the crate table, the publish order with `buildl-cargo` last; dependency rule 4 and publishing rule 2 ("no fifth member") reworded for the second binary. Numbered rules keep their numbers |
| `book/src/ch11-04-errors.md:171`, `book/src/ch11-07-testing-layers.md:27` | the `crates/buildl-cli/src` paths move to `crates/buildl/src/cli` |
| `book/src/ch11-06-decision-records.md` | a record: the command line lives in `buildl` so a second binary shares it |
| `book/src/appendix-04-exit-codes.md` | `exit.rs` path moves to `crates/buildl/src/cli/` |
| book user chapter, `SUMMARY.md` | a page `ch08-06-cargo-buildl.md` after the rust plugin page |
| `README.md`, `skills/buildl`, `skills/buildl-migrate` | point a Cargo project at `cargo install buildl-cargo` and `cargo buildl init` |
| `CHANGELOG.md` | Unreleased: Added `buildl-cargo` |

## 11. Testing

| Layer | What it proves |
|---|---|
| unit, `buildl-cargo` generation | the §6 table row by row, the order, `required-features`, a lib with both crate types, an example library getting no call, the name refusals; input is metadata JSON kept in `buildl-cargo`'s own fixtures (F24) |
| unit, markers | sync's replacement keeps every byte outside the markers; missing, doubled and reversed markers are refused; up to date writes nothing |
| unit, argv | `buildl` dropped only as argv[1]; `init`, `sync --check` and a buildl command parse |
| integration, `buildl-cargo/tests` | in a temp copy of a small Cargo workspace kept in `buildl-cargo/tests/fixtures/` (F24): `init` writes the three files; a second `init` exits 5 and writes nothing; no `Cargo.lock` exits 5; a write failure leaves the folder as it was; `sync --check` is 0, then 5 after a new bin is added, then 0 after `sync`; missing markers exit 5; the pass-through `clean` prints buildl's `clean:` line |
| existing, `buildl-cli/tests` | unchanged and still green: the move changed no behaviour |

The integration tests never reach the network. `init` runs with `CARGO` set and nothing that
fetches on `PATH`, so step 6 ends with exit 3 after the files are written (F23). `sync` and
`clean` load no plugin, so no fetch is needed after that.

## 12. Non-goals

- Caching compiled dependencies between builds, and folder outputs in `buildl-core`.
- Any change to `buildl-core`, `buildl-lua` or the `rust` plugin.
- Non-Cargo projects, and non-Rust targets in a Cargo project (the engineer's section holds those).
- `sync` updating `buildl.toml`.
- Prebuilt `cargo-buildl` binaries through cargo-dist.
