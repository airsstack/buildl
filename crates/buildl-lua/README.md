# buildl-lua

Lua build-file evaluation for buildl, on the airsl embedded runtime.

[buildl](https://github.com/airsstack/buildl) is a sandboxed, deterministic build system whose
build files are written in Lua. This crate evaluates them: `LuaSource` runs one build file per call
on a fresh [airsl](https://github.com/airsstack/airsl) engine, under a confined policy, and returns
what the file declared. It provides the `buildl` table a build file calls, and loads plugins. It is
the only buildl crate that depends on a Lua runtime.

buildl is four crates, published in this order: `buildl-core`, **`buildl-lua`**, `buildl`,
`buildl-cli`. This crate depends on `buildl-core` and airsl.

- [The buildl Book](https://airsstack.github.io/buildl/), its
  [build file API](https://airsstack.github.io/buildl/appendix-03-lua-api.html) and its page on
  [the four crates](https://airsstack.github.io/buildl/ch11-03-four-crates.html)
- [API documentation](https://docs.rs/buildl-lua)
- [Repository](https://github.com/airsstack/buildl)

## License

Apache-2.0
