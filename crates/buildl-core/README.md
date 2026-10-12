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
| Keys | `ActionKey` — the SHA-256 of a target's `KeyComponents`: its command, input digests, output names, environment values, tool fingerprint, settings, network access and platform, with what each dependency produced — and `CacheRow`, what the last build of a target recorded |
| Authority | `Ceiling` — what a workspace allows — `WantedSet` and `Want` — what a build asks for — and `Workspace`, what the manifest says |
| Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`, held with `DeclarationSource` in one `PortSet` |
| Plan | `plan` refuses a build whose wants exceed the ceiling, keys every target, and returns a `Plan`: a `Verdict` per target — clean, dirty with the components that differ, or conditional on a dependency's output — and the wanted set; `Pipeline::plan` runs Load once, Resolve, then Plan |
| Build values | `OutputPath` — where one output lives under the workspace's output directory — `ReadyAction` — one action with its command expanded — `RawOutcome` — how a run ended and what it printed — `ActionOutcome` with its `OutcomeClass`, `Summary`, and `OutcomeEvent`, one line of the build log |
| Build ports | `ContentStore`, `EventLog`, `ExecStrategy` and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in one `PortSet` |
| Execute and Record | `execute` gives every output a path, expands every command and holds its program to the ceiling before anything runs, then runs the plan: a target whose key and stored outputs still stand is cached, any other runs, and each result is recorded as the last step of its action — content store, then cache row, then log — so a failure leaves no row; `Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` per target |
| Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved; Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure; Execute's name the two targets claiming one output path, and the target whose command names no program once expanded |
| Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused |

**Status:** pre-release. Every pipeline phase — Load, Resolve, Plan, Execute and Record — is
implemented and tested against in-memory ports. The adapters that run a real build belong to
other crates and are not written yet.

## License

Apache-2.0
