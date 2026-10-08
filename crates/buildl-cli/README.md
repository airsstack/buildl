# buildl-cli

The `buildl` binary — the command-line entry point to the [`buildl`](../buildl) build system library.

It is a thin shell: it will parse arguments for the `check`, `graph`, `plan` and `build` commands, call into `buildl`, and map the outcome class to an exit code. All build logic lives in the library crates.

**Status:** pre-release; the binary provides no commands.

## License

Apache-2.0
