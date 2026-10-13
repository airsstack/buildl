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
| Resolve | `resolve` turns declarations into a `TargetGraph`, refusing a name declared twice, a reference that names nothing or the wrong kind of thing, a `$opt:` in a command or in a rule's description that names no declared setting, and a dependency cycle; `Pipeline::graph` runs Load once, then Resolve |
| Keys | `ActionKey` — the SHA-256 of a target's `KeyComponents`: its command, input digests, output names, environment values, tool fingerprint, settings, network access and platform, with what each dependency produced — and `CacheRow`, what the last build of a target recorded. Both read back from canonical JSON: a key whose digest is not the digest of its components is refused, and so is a row that does not hold one digest per output its key declares |
| Outputs | `OutputManifest` — what one output is: a regular file (`StoredFile`: the digest of its bytes and a `FileMode`, plain or executable) or a `Tree`, a directory's files by `TreePath` in path order. Its canonical JSON is stored like any content, and the digest of those bytes is the output's one digest in a row, a log event and every dependent's key |
| Authority | `Ceiling` — what a workspace allows — `WantedSet` and `Want` — what a build asks for — and `Workspace`, what the manifest says: its output and state directories, its ceiling, and `Execution`, how it wants its actions run — `Workers`, `Timeout` and `Isolation`, each validated at construction and none of them part of a key |
| Plan ports | `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`, held with `DeclarationSource` in one `PortSet` |
| Plan | `plan` fixes the value of every build setting for the run, refuses a build whose wants exceed the ceiling, keys every target — a tool named through a build setting is wanted, looked up and keyed like one written out — and returns a `Plan`: a `Verdict` per target — clean, dirty with the components that differ, or conditional on a dependency's output — the wanted set, and the settings it was made with, which Execute reads from it; `Pipeline::plan` runs Load once, Resolve, then Plan |
| Build values | `OutputPath` — where one output lives under the workspace's output directory — `ReadyAction` — one action with its command expanded, and its rule's description expanded with it as an `ExpandedDescription`, the line to show for the action — `RawOutcome` — how a run ended and what it printed — `Found` — what a strategy reports at one output path: nothing, a file, a directory with everything below it, a symbolic link or something special — `ActionOutcome` with its `OutcomeClass`, `Summary`, and `OutcomeEvent`, one line of the build log |
| Build ports | `ContentStore` — holds bytes by digest, reads them back, clears an output's path and places a file there with its mode — `EventLog`, `ExecStrategy` — runs one action, then reports what it left at each output and stores every regular file of it — and `Dispatcher`, with `ActionCache::record` and a `Clock` a worker may read: twelve ports in one `PortSet` |
| Execute and Record | `execute` gives every output a path, expands every command and every rule description, and holds each command's program to the ceiling before anything runs, then runs the plan: a target whose key still stands, and whose every output can be put back whole from the store, is cached, any other runs, and each result is recorded as the last step of its action — content store, then cache row, then log — so a failure leaves no row. An output the store cannot place whole is removed from its path again, and the target fails. An action may leave a regular file or a directory of regular files at an output; a symbolic link, a special file, an empty directory or a name that is not Unicode there fails the target as `OutputRefused`. A dispatcher that loses a task or its result fails the build instead of leaving a target without an outcome; `Pipeline::build` runs Load once, Resolve, Plan, then Execute, and returns one `ActionOutcome` per target |
| Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field; Resolve's name every declaration involved; Plan's name the target and build file behind a refused want, a missing tool or a missing input, and the port behind an adapter's own failure; Execute's name the two targets claiming one output path, the target whose command names no program once expanded, and the targets a dispatcher left without an outcome; JSON that cannot be read back is told apart as not readable or not canonical, and from an adapter's own failure |
| Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused; and one reader, `canonical::from_slice`, which takes back only the bytes the serializer writes |

**Status:** pre-release. Every pipeline phase — Load, Resolve, Plan, Execute and Record — is
implemented and tested against in-memory ports. The adapters that run a real build belong to
other crates and are not written yet.

## License

Apache-2.0
