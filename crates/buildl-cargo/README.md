# buildl-cargo

The `cargo-buildl` binary — runs every buildl command as `cargo buildl <command>`.

[buildl](https://github.com/airsstack/buildl) is a sandboxed, deterministic build system whose
build files are written in Lua. Cargo runs any `cargo-<name>` program on the `PATH` as
`cargo <name>`, so with this crate installed, `cargo buildl build` does what `buildl build` does.
Linux and macOS.

```sh
cargo install --locked buildl-cargo
```

Commands are those of the `buildl` binary — see
[the command line](https://airsstack.github.io/buildl/appendix-01-cli.html). A command line that
is wrong ends with exit code 2, as it does there.

This crate depends on `buildl`.

[The buildl Book](https://airsstack.github.io/buildl/) · [Repository](https://github.com/airsstack/buildl)

## License

Apache-2.0
