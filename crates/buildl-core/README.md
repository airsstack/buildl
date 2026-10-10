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
| Declarations | `Declaration` — one `Target`, `Rule`, `Alias` or `Setting`, with the provenance of the build file that declared it — and the validated values its fields hold, such as `SourcePath`, `OutputName`, `EnvName`, `Command` and `SettingValue` |
| Build-file port | `DeclarationSource` evaluates one build file into a `StagedFile`, or reports it absent: every value exactly as the file wrote it, a `Written<T>` typed by what it must become, plus the file's `subdir` requests |
| Load | `load` walks the build files from the workspace root along their `subdir` requests and returns every declaration validated and sorted; `Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ |
| Graph | `TargetGraph` — the runnable targets as `Node`s numbered in label order, their dependencies as edges in written order, with aliases and build settings beside them; it serializes keyed by label, with no id in the output |
| Resolve | `resolve` turns declarations into a `TargetGraph`, refusing a name declared twice, a reference that names nothing or the wrong kind of thing, an undeclared `$opt:` setting, and a dependency cycle; `Pipeline::graph` runs Load once, then Resolve |
| Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved |
| Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused |

**Status:** pre-release. Load and Resolve, the first two pipeline phases, are implemented and
tested against in-memory ports; Plan, Execute and Record are not implemented yet.

## License

Apache-2.0
