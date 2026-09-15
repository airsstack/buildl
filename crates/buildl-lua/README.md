# buildl-lua

Lua build-file evaluation for buildl, on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the only buildl crate that depends on a Lua runtime. Its role is to evaluate build files inside an airsl sandbox and to install the `buildl` module table that build files declare targets through, implementing a port defined in [`buildl-core`](../buildl-core).

**Status:** pre-release; the crate has no public API.

## License

Apache-2.0
