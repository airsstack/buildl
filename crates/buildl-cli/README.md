# buildl-cli

The `buildl` binary — the command-line entry point to the buildl library.

[buildl](https://github.com/airsstack/buildl) is a sandboxed, deterministic build system whose
build files are written in Lua. This crate is its command line: it parses arguments, calls the
`buildl` crate and maps how a command ended to an
[exit code](https://airsstack.github.io/buildl/appendix-04-exit-codes.html). Linux and macOS.

```sh
cargo install --git https://github.com/airsstack/buildl buildl-cli --locked
```

Commands: `check`, `graph`, `query`, `plan`, `build`, `test`, `verify`, `watch`, `log`, `clean`,
`gc`, `doctor`, `plugins fetch` — each described in
[the command line](https://airsstack.github.io/buildl/appendix-01-cli.html).

buildl is four crates, published in this order: `buildl-core`, `buildl-lua`, `buildl`,
**`buildl-cli`**. This crate depends on `buildl`.

[The buildl Book](https://airsstack.github.io/buildl/) · [Repository](https://github.com/airsstack/buildl)

## License

Apache-2.0
