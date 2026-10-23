# buildl-lua

Lua build-file evaluation for buildl, on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the only buildl crate that depends on a Lua runtime. It implements [`buildl-core`](../buildl-core)'s `DeclarationSource` port as `LuaSource`, which evaluates one build file per call on a fresh airsl engine and returns what the file staged, exactly as written.

## What it holds today

| | |
|---|---|
| Adapter | `LuaSource::new(root, limits, skipped, plugins)`: a missing build file is reported absent; every other failure is one `Error::Evaluation` naming the file, its kind and a diagnostic with the line where known. `skipped` is required: the workspace-relative directories `buildl.sources` stays out of, such as the workspace's output and state directories. An empty list skips nothing. `plugins` maps each plugin name a build file may use to its directory. Each `evaluate` call is given the `Discovered` results `buildl.discovered` reads, empty on a first load |
| Limits | `DeclarationLimits`: the memory, instruction and source-walk ceilings (16 MiB, 10 000 000 instructions and 100 000 directory entries by default); the memory ceiling is also the byte budget for what one file may stage, and the walk ceiling counts every entry `buildl.sources` visits across all its calls in one build file. Wall time is not bounded |
| Declaration API | the `buildl` table, bound as both `buildl` and `airsstack.buildl`: `target`, `test`, `discover`, `rule`, `alias`, `option`, `subdir`, `sources`, `discovered`, `use`. `buildl.use(name)` reads the plugin's `extension.toml` with airsl, refusing a manifest that asks for more than `proc.run` and `env.read` or whose name is not the one the workspace gave it, then loads the entry file in the same engine under an environment of its own (a new `buildl` table whose declarations carry the plugin as their origin, one-level copies of the curated modules and the language globals, and a `require` confined to the plugin's directory) and returns the entry's value; one instance is loaded per chain of plugins that led to it. `buildl.discover(name, options)` declares a discovery target: a target's fields less `outputs` and `always`, with the one output `<name>.json`. `buildl.discovered(label)` returns the bytes of the last result of that discovery target as a string, or `nil` when it has none; the build file decodes them. Option tables are closed, values are strictly typed, and list fields splice one level of nested lists |
| Sandbox | airsl's minimal Lua surface with no grants; only `json`, `path` (without `absolute`), `regex`, `hash` and `glob`; no `math.random`, `math.randomseed` or `print`; `tostring` and `string.format` refuse address-derived text (a table, function, thread or userdata without `__tostring`, and `%p`) with an ordinary Lua error. `buildl.sources` is the only filesystem read: regular files under the declaring directory, sorted, symlinks never followed, and nothing under a directory `LuaSource::new` was given to skip — matched by its whole path from the workspace root, neither listed, entered nor counted; a build file inside one gets an empty list |
| Refusals | a refused primitive call fails the whole file, even when the build file catches it with `pcall`; the one exception is a call made while another `buildl` call is in progress (for example from a finalizer), which raises an error without failing the file |

**Status:** pre-release. The adapter is complete for the declaration phase; [`buildl`](../buildl) composes it with the pipeline, and the `check`, `graph`, `plan` and `build` commands evaluate build files through it.

## License

Apache-2.0
