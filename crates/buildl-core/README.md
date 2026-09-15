# buildl-core

Domain data, ports, and pure pipeline logic for the buildl build system.

This crate performs no I/O and knows nothing about Lua: it is the home for the values the pipeline phases exchange, the traits (ports) through which the pipeline reaches the outside world, and the logic of every phase. Concrete implementations of the ports live in [`buildl-lua`](../buildl-lua) and [`buildl`](../buildl).

**Status:** pre-release; the crate has no public API.

## License

Apache-2.0
