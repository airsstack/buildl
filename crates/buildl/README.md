# buildl

A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the framework crate and the composition root. Its role is to bind concrete adapters to the pipeline defined in [`buildl-core`](../buildl-core):

- the adapters for every port except `DeclarationSource` — `Manifest`, `Digester`, `StatCache`, `ToolResolver`, `ContentStore`, `ActionCache`, `EventLog`, `ExecStrategy`, `Dispatcher`, `Approver`, `Clock` and `Reporter`;
- the Lua evaluator from [`buildl-lua`](../buildl-lua), which implements `DeclarationSource`.

It will assemble them as `LocalPorts`, expose the result as the `Workspace` facade, and re-export `buildl-core`. The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

**Status:** pre-release; none of the adapters or the assembly exist yet, and the crate has no public API.

## License

Apache-2.0
