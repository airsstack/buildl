# buildl-core

Domain data, ports, and pure pipeline logic for the buildl build system — no I/O, no Lua.

[buildl](https://github.com/airsstack/buildl) is a sandboxed, deterministic build system whose
build files are written in Lua. This crate holds the values the pipeline's phases hand each other,
the traits (ports) through which a build reaches the outside world, and the logic of every phase:
Load, Resolve, Plan, Execute and Record. It performs no I/O and knows nothing of Lua, so every flow
of a build is tested here against fake ports.

buildl is four crates, published in this order: **`buildl-core`**, `buildl-lua`, `buildl`,
`buildl-cli`. This crate depends on none of the others; all three depend on it.

- [The buildl Book](https://airsstack.github.io/buildl/), and its page on
  [the four crates](https://airsstack.github.io/buildl/ch11-03-four-crates.html)
- [API documentation](https://docs.rs/buildl-core)
- [Repository](https://github.com/airsstack/buildl)

## License

Apache-2.0
