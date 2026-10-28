# buildl

A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the
airsl embedded runtime.

This is the framework crate. It holds every adapter but the Lua one, among them the manifest, the
content store and action cache, the sandbox that runs each action, the approver, the remote cache
and remote execution, and watch mode, and it holds the plugins module. It binds them to the pipeline in `buildl-core` and exposes
the result as the `Workspace` facade. It builds on Unix only; Linux and macOS are supported.

buildl is four crates, published in this order: `buildl-core`, `buildl-lua`, **`buildl`**,
`buildl-cli`. This crate depends on `buildl-core` and `buildl-lua`; the `buildl` binary in
`buildl-cli` is a thin shell over it.

- [The buildl Book](https://airsstack.github.io/buildl/), and its page on
  [the four crates](https://airsstack.github.io/buildl/ch11-03-four-crates.html)
- [API documentation](https://docs.rs/buildl)
- [Repository](https://github.com/airsstack/buildl)

## License

Apache-2.0
