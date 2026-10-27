---
status: approved
created: 2026-10-27
milestone: buildl/09
---

# Intent: Examples for Go, Rust and Python

## Problem

Before the first publish, buildl needs examples from several technology stacks — Go, Rust
and Python to begin with — that between them show several use cases, so a new user can see
buildl working on a real project.

Today there are none. buildl's only workspaces are test fixtures inside the crates, written to
prove one behaviour each, not to show a user how to build a Go, Rust or Python project. Nothing
yet shows that buildl's defaults — the input sandbox above all — work with these toolchains as
people actually use them, so a first user would be the one to find out.

## Affected systems

- A new top-level `examples/` directory, outside every crate.
- The CI workflow, `.github/workflows/ci.yml`, which gains the toolchains the examples need and
  a run of each example.
- Any part of buildl an example shows to be wrong or missing: this chain fixes it (see
  Constraints).

## Desired outcome

Examples in Go, Rust and Python that, between them, show:

|Use case|Shown|
|---|---|
|build and test basics|a build and a test target per stack; `plan` explaining a rebuild; a second `test` served from cache; `log`|
|discovery target|inputs known only by running a tool, read back on the second pass|
|polyglot monorepo|Go, Rust and Python in one workspace, with cross-directory labels and `query`|
|plugins|a plugin loaded with `buildl.use`, its wants attributed in `plan` and `doctor`|
|remote cache and remote execution|an example that brings up a REAPI server and builds against it|

Each example is a workspace a user can copy and run with the `buildl` binary alone, plus the
stack's own toolchain. CI runs every example on Linux and macOS; the remote example runs where
its server can run (Linux, as the NativeLink job does today).

## Constraints

- **Anything an example needs is built here.** A bug, or a capability buildl lacks, that keeps
  an example from working is fixed or built inside this chain, under the repository's gates and
  structural rules (architecture-building-blocks §7), not left to a later chain.
- **The publish rules hold** (architecture-building-blocks §9): no fifth workspace member, no
  `[[example]]` in `buildl`, and no test reading another package's directory.
- **CI load.** The examples add Go, Python and a REAPI server to CI on two operating systems;
  they run once per CI run, not per test.
- The book that walks through the examples is the `2026-10-27-book` chain's, not this one's.

## Non-goals

- Stacks other than Go, Rust and Python.
- Fine-grained per-file C/C++ compilation (out of scope for v1, design §13).
- The book's chapters about the examples, the READMEs, and the publish itself — the other
  chains of milestone buildl/09.
