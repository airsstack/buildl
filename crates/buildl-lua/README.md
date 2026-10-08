# buildl-lua

Lua build-file evaluation for buildl, on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the only buildl crate that depends on a Lua runtime. It implements [`buildl-core`](../buildl-core)'s `DeclarationSource` port as `LuaSource`, which evaluates one build file per call on a fresh airsl engine and returns what the file staged, exactly as written.

## What it holds today

| | |
|---|---|
| Adapter | `LuaSource::new(root, limits)`: a missing build file is reported absent; every other failure is one `Error::Evaluation` naming the file, its kind and a diagnostic with the line where known |
| Limits | `DeclarationLimits`: the memory, instruction and source-walk ceilings (16 MiB, 10 000 000 instructions and 100 000 directory entries by default); the memory ceiling is also the byte budget for what one file may stage, and the walk ceiling counts every entry `buildl.sources` visits across all its calls in one build file. Wall time is not bounded |
| Declaration API | the `buildl` table, bound as both `buildl` and `airsstack.buildl`: `target`, `test`, `rule`, `alias`, `option`, `subdir`, `sources`. Option tables are closed, values are strictly typed, and list fields splice one level of nested lists |
| Sandbox | airsl's minimal Lua surface with no grants; only `json`, `path` (without `absolute`), `regex`, `hash` and `glob`; no `math.random`, `math.randomseed` or `print`; `tostring` and `string.format` refuse address-derived text (a table, function, thread or userdata without `__tostring`, and `%p`) with an ordinary Lua error. `buildl.sources` is the only filesystem read: regular files under the declaring directory, sorted, symlinks never followed |
| Refusals | a refused primitive call fails the whole file, even when the build file catches it with `pcall`; the one exception is a call made while another `buildl` call is in progress (for example from a finalizer), which raises an error without failing the file |

**Status:** pre-release. The adapter is complete for the declaration phase; nothing in buildl composes it into a command yet.

## License

Apache-2.0
