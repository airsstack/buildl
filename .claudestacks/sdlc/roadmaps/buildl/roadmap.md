---
status: approved
created: 2026-10-15
---

# Roadmap: buildl v1

## Vision

buildl is a build system whose build files are written in Lua and evaluated on the airsl
embedded runtime, under one founding constraint: nothing is ambient (`book/src/ch04-01-nothing-is-ambient.md`). Three
properties follow from it and are the product: build files that are safe to evaluate
before any trust decision, incrementality that is deterministic and explains itself, and
authority that is derived from the build graph and checked against a workspace ceiling
before anything runs.

This roadmap delivers v1 in the order its milestones are numbered, with the airsl extension work
interleaved: the core pipeline first, then the commands around a build, the input
sandbox, grant negotiation, plugins, a remote cache, watch mode and remote execution, and
the first publish of the four crates to crates.io.

## Non-goals

- Depfile parsing: dynamic input discovery is the discovery target
  (`book/src/ch07-00-discovery-targets.md`), not a side channel in the executor.
