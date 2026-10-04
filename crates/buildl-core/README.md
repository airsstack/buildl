# buildl-core

Domain data, ports, and pure pipeline logic for the buildl build system.

This crate performs no I/O and knows nothing about Lua: it is the home for the values the
pipeline phases exchange, the traits (ports) through which the pipeline reaches the outside
world, and the logic of every phase. Concrete implementations of the ports live in
[`buildl-lua`](../buildl-lua) and [`buildl`](../buildl).

## What it holds today

| | |
|---|---|
| Names | `Directory`, `TargetName`, `Label` — a target's absolute name, `//dir:name`, not revalidated after construction |
| Identity | `Digest` (SHA-256), `NodeId` (a graph arena index), `Provenance` (the declaring build file and the directory it was evaluated in) |
| Time | `Timestamp`, and the `Clock` port that is the only way to obtain the current one |
| Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text |
| Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused |

**Status:** pre-release. The vocabulary above is complete and unit-tested; no pipeline phase is
implemented yet.

## License

Apache-2.0
