# buildl-cli

The `buildl` binary — the command-line entry point to the [`buildl`](../buildl) build system library.

It is a thin shell: it parses arguments, calls into `buildl`, writes what the library rendered, and maps how the command ended to an exit code. All build logic lives in the library crates.

## What it holds today

| Command | Does |
|---|---|
| `buildl check` | evaluates every build file twice and compares the two; runs nothing and writes nothing |
| `buildl graph` | prints the target graph on stdout: canonical JSON with `--json`, the default, or DOT with `--dot` |
| `buildl plan [TARGET]...` | prints on stdout what would run, and why; `--json` gives the plan as canonical JSON |
| `buildl build [TARGET]...` | builds the targets and what they need; blocks, a status line on a terminal and a summary go to stderr. `-k`, `--keep-going` carries on past a failure; `--stream` shows one running action's output as it is printed |
| `buildl test [TARGET]...` | runs the test targets and what they need |
| `buildl verify [TARGET]...` | re-runs cached actions in the sandbox and names those whose outputs differ |
| `buildl query <EXPR>` | prints the targets `deps(TARGET)` or `rdeps(TARGET)` reaches |
| `buildl log <TARGET>` | shows what a target printed when it last ran |
| `buildl clean`, `buildl gc` | remove the output directory; drop rows of undeclared targets and blobs nothing needs |
| `buildl doctor` | prints on stdout the workspace root, its approver, its execution settings, its ceiling, and each want: allowed or refused, approved or not; writes nothing, and exits 0 whatever it reports |

A target is `//dir:name`, or `:name` and `name`, read against the current directory; with none, every target is taken. `plan` and `build` accept `--set NAME=VALUE`, repeated, to give a build setting a value for the run. `--color auto|always|never` goes with every command. The workspace is the nearest `buildl.toml` at or above the current directory.

| Exit code | Means |
|---|---|
| 0 | done |
| 1 | an action failed: fix your code |
| 2 | the command line is wrong |
| 3 | fix your builder, or run again: a missing tool, a timeout, a file that cannot be read or written, a workspace another buildl is using |
| 4 | the build wants more than the ceiling, or its approver did not approve it |
| 5 | a build file or `buildl.toml` is wrong |
| 6 | a state file is not what buildl writes |
| 7 | buildl is at fault |

The binary builds on Unix targets only; the supported platforms are Linux and macOS.

**Status:** pre-release. The commands above are the whole surface.

## License

Apache-2.0
