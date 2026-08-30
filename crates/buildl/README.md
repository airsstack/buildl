# buildl

A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the library crate — all logic lives here; the `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

**Status: design phase.** This crate is an empty scaffold. See the repository's [`docs/design.md`](../../docs/design.md) and [`docs/architecture.md`](../../docs/architecture.md) for what is planned.

## License

Apache-2.0
