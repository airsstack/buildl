# buildl

A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the framework crate. Its role is to bind concrete adapters — storage, hashing, execution, and the Lua evaluator from [`buildl-lua`](../buildl-lua) — to the pipeline defined in [`buildl-core`](../buildl-core). The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

**Status:** pre-release; the crate has no public API.

## License

Apache-2.0
