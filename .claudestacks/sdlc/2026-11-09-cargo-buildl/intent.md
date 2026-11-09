---
status: done
created: 2026-11-09
---

# Intent: cargo-buildl, buildl from cargo

## Problem

A Rust engineer who wants to build a Cargo project with buildl has to do by hand what Cargo
already knows. They install a second tool, write a `buildl.toml` whose ceiling and
`[plugins.rust]` table are the same lists twice plus a git source, add `out/` and `.buildl/` to
`.gitignore`, run `buildl plugins fetch`, and write a `build.lua` with one call per binary,
library and test, copying package and feature names out of `Cargo.toml`. They must know the
plugin's rules (a `Cargo.lock` must exist, which crate types get a call, how a `cdylib` is named).
When `Cargo.toml` changes, nothing tells them `build.lua` is now out of date.

Every step is mechanical, and each one is a reason a Rust engineer gives up before the first
build. The `rust` plugin made buildl able to build Cargo projects; nothing yet makes it easy to
start.

## Affected systems

- `crates/buildl-cli` — today holds the whole command line (`cli.rs`, `run.rs`, `exit.rs`,
  `doctor_text.rs`, `remote_switches.rs`, `terminal.rs`, `console.rs`) inside the binary.
- `crates/buildl` — the library both binaries would build on.
- A new crate `crates/buildl-cargo`.
- `plugins/rust` — what the generated `build.lua` calls; read, not changed.
- `book/src/ch11-03-four-crates.md`, `crates/expected-edges.txt`,
  `book/src/ch11-06-decision-records.md`, the user chapters near `ch08-05-the-rust-plugin.md`,
  `README.md`, and the `buildl` / `buildl-migrate` skills where they describe a Cargo setup.

## Desired outcome

From a Cargo project with no buildl files, one install and one command give a working buildl
setup, and the engineer runs buildl through `cargo`:

- `cargo install buildl-cargo` puts `cargo-buildl` on the machine; nothing else is needed.
- `cargo buildl init` leaves the project ready to build: `buildl.toml`, `build.lua`, the
  `.gitignore` entries and the fetched `rust` plugin, or says plainly what is missing, such as a
  `Cargo.lock`.
- `cargo buildl sync` brings `build.lua` back in line with `Cargo.toml`; `cargo buildl sync
  --check` says whether it is, with an exit status CI can use.
- `cargo buildl <command>` behaves exactly as `buildl <command>`: the same flags, output and
  exit codes.

## Constraints

Decided by the author, not to be reopened in design:

- The crate is `buildl-cargo`, the fifth workspace crate; its binary is `cargo-buildl`, so cargo
  finds it as the subcommand `buildl`.
- It links the `buildl` library: one install, no separate `buildl` binary needed.
- The command line moves out of `buildl-cli` into the `buildl` library, so `buildl-cli` and
  `buildl-cargo` share one implementation; `buildl-cli` keeps the `buildl` binary and little
  else.
- `build.lua` has a section `sync` owns and rewrites, and a section the engineer owns and
  `sync` never touches.
- What is generated follows Cargo's own defaults: no `profile` is written, so the plugin's
  default (`dev`) applies.
- One `rust.test` per package. A binary gets `rust.binary`, a `cdylib` or `staticlib` gets
  `rust.library`; an `rlib` or `proc-macro` gets no call, as the plugin's README says.
- The `rust` plugin stays a thin cargo wrapper; nothing in this work adds to it.
- The dependency rules of `ch11-03` and the determinism rules of `ch11-05` hold; the book and the
  edge lists change with the crates.

## Non-goals

- Caching compiled dependencies between builds (cargo-chef's prepare/cook split) and folder
  outputs in `buildl-core`.
- Changing the `rust` plugin or what its targets run.
- Projects that are not Cargo projects, and non-Rust targets inside a Cargo project.
- A roadmap milestone; this chain stands alone.
