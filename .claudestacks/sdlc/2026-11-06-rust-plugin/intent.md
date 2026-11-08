---
status: done
created: 2026-11-06
---

# Intent: Rust through an official `rust` plugin

## Problem

A Rust project in buildl today is a hand-written `buildl.target` that runs cargo
(`examples/rust`, `examples/polyglot/rust`), and that shape has these gaps:

| Gap | Today |
|---|---|
| every change recompiles every dependency | each action starts in a fresh sandbox with no cargo build directory, so editing one line rebuilds all crates, a `-sys` crate's C/C++ included (RocksDB: minutes per edit) |
| inputs are written by hand | `b.sources("src/*.rs")` misses nested modules, workspace members, `build.rs`, files read by `include_str!` |
| outputs need a trick | `cargo install --path . --root $out --no-track`, binaries only; no `staticlib` or `cdylib` another target can use |
| one test result | one `cargo test` action, cached as a whole |
| network use is hidden | cargo fetches crates inside an action that declares no network |
| a toolchain update goes unseen | `~/.cargo/bin/rustc` and `cargo` are links to the `rustup` launcher, whose file does not change when the toolchain does |

Many real projects depend on `-sys` crates that build C or C++ through FFI (`rocksdb`, `openssl`,
`zstd`), and mixed-language repositories with Rust in them are where buildl's single graph,
shared cache and approval matter most. buildl now builds C and C++ through the `c` plugin; Rust
is the next compiled stack its users will bring.

Findings of the 2026-11-06 analysis that the design must carry:

- **Rejected shapes.** Running `rustc` once per crate, with crates declared in Lua, covers almost
  no real project: nearly every dependency tree has a crate with `build.rs` (`libc`,
  `proc-macro2`, `serde`), and buildl allows one discovery round, so a build script's output
  cannot shape declarations. Reimplementing Cargo (features, build scripts, `links`, proc-macros,
  several versions of one crate) is Bazel `rules_rust` sized, with upkeep on every Cargo release.
  Early cutoff between Rust crates almost never fires: a comment-only edit changes the rlib's
  bytes.
- **The probed shape works.** A layer that builds only the dependencies from `Cargo.toml` and
  `Cargo.lock` (the cargo-chef pattern) and a layer that builds the project on top of it were
  probed on macOS (Apple, cargo 1.99.0) and in GitHub Actions on `ubuntu-latest`,
  `ubuntu-24.04-arm` and `macos-latest` (cargo 1.99.1, run 42221321359) with `serde` (derive)
  and `zstd` (`zstd-sys` compiles C in `build.rs`), 19 crates:

  | Check | Result |
  |---|---|
  | dependencies reused in a new sandbox folder | 16 of 16 `Fresh` on all systems |
  | edit a source | only the project recompiles; 13.4 s cold, 2.1 s after the edit (macOS) |
  | `buildl verify` on the same machine | 2 verified on all systems |
  | same bytes in another workspace or machine | no, for crates that `include!` from `OUT_DIR`: the sandbox folder differs per workspace by design (sibling chain `2026-11-06-where-actions-run`) |
  | size of the dependency layer | 68 MB for this small project |

- **What identical bytes needed, as found.** Fixed file times, owner and order in the packed
  layer, with GNU tar and bsdtar flags told apart. The sandbox path appears in cargo's text files
  (`*.d`, a build script's `output` and `root-output`) and is swapped out on packing and back on
  unpacking, in those files only: editing a binary that holds the path (a macOS proc-macro dylib)
  corrupted it and rustc was killed. All unpacked files get one identical time, or cargo judges
  the dependencies stale. The project's own artifacts are removed from the layer (`cargo clean -p`),
  or the binary built from the placeholder `main` survives as stale. Third-party crates were built
  without debug info and the project binary stripped of its debug symbols: on macOS debug info is
  kept in `.o` files referenced by absolute paths into the sandbox, which is deleted. Cargo's
  `trim-paths` would be the clean fix but is not stable in 1.99. The same cargo settings must be
  given to both layers, or cargo rebuilds everything.

The value is for teams and CI with a shared cache, mixed-language repositories, heavy `-sys`
dependencies and anyone who wants build scripts seen and approved. A single developer's edit loop
on one crate stays faster with plain `cargo`, which keeps rustc's incremental state.

## Affected systems

- A new plugin folder, `plugins/rust/`, published by git tag from this repository, beside
  `plugins/c/`.
- `Makefile.toml`: the `plugins` task, which runs only `plugins/c`'s Lua tests today.
- `examples/rust` and `examples/polyglot/rust`, checked by `examples/check.sh` in CI.
- The book: a new chapter beside `ch08-04-the-c-plugin.md`, `SUMMARY.md`, `ch11-09` (limits),
  `ch07-00` where it speaks of Cargo tracking its own sources.
- The `buildl` and `buildl-migrate` skills and their references.
- `README.md` and `CLAUDE.md` where they describe Rust or the plugins.

## Desired outcome

- The plugin is a thin `cargo` wrapper, as the `c` plugin is a thin `cc` wrapper: helpers that
  save the engineer the boilerplate `examples/rust` writes by hand (inputs, the `cargo install`
  trick, `HOME` and `PATH`), and run plain `cargo`. It changes nothing about how Rust builds.
- A workspace builds a Cargo project by calling an official `rust` plugin it fetches from this
  repository at a tag.
- The project's binaries, static libraries and C-ABI dynamic libraries land in `out/`, named,
  for other targets to use.
- Tests run as buildl tests, cached.
- Network use is declared and shown by `plan` and the approver.
- A toolchain update can be made to rebuild, as the `c` plugin's `version_probe` does for a
  compiler.
- Linux and macOS.
- The examples show the plugin and CI checks them through a real git fetch; the book, skills and
  README describe it, its value, its costs and its limits.

## Constraints

- No change to buildl's core in this chain.
- No depfile read by the executor, and no side channel into an action's inputs.
- Cargo 1.99 stable features only: nothing behind `-Z` or `cargo-features`.
- The programs the plugin may run are listed in its manifest, as in `c`. Programs cargo starts
  itself (rustc, the linker, build scripts and what they run) are not seen by the ceiling; the
  book says so plainly.
- One build file per Cargo project, at its root: an input cannot name a file outside its build
  file's folder.
- No vendoring, no cached dependency layer, no rewritten files: the 2026-11-06 layer findings in
  the Problem section are history, not the design.
- Candidate direction: helpers `rust.binary`, `rust.library`, `rust.test`, shaped like `c.binary`.

## Non-goals

- Running `rustc` per crate, or reimplementing Cargo's resolution, features or build scripts.
- Hand-written overrides that replace a `-sys` crate's `build.rs` with buildl targets.
- Caching each workspace member on its own.
- Keeping rustc's incremental compilation between builds.
- A faster local edit loop than plain `cargo`.
- Cross-compiling (`--target`).
- Windows.
- A sandbox path shared by every workspace or machine (rejected in `2026-11-06-where-actions-run`).
- The book's general pages on where actions run and who should upload
  (`2026-11-06-where-actions-run`).
