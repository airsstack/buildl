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
| `buildl plugins fetch` | pins every git plugin `buildl.toml` names in `buildl.lock` and stores its files, so a build needs no network; says on stderr, per plugin, `fetched` or `unchanged`, its name and its commit |
| `buildl doctor` | prints on stdout the workspace root, its approver, its execution settings, its ceiling, and each want: allowed or refused, approved or not; writes nothing, and exits 0 whatever it reports |

A target is `//dir:name`, or `:name` and `name`, read against the current directory; with none, `plan`, `build` and `verify` take every target but the tests and `test` every test, and `log` takes exactly one. `plan` and `doctor` print each want with the plugins whose targets want it. `plan`, `build`, `test` and `verify` accept `--set NAME=VALUE`, repeated, to give a build setting a value for the run. `--color auto|always|never` goes with every command. The workspace is the nearest `buildl.toml` at or above the current directory.

| Exit code | Means |
|---|---|
| 0 | done |
| 1 | an action failed: fix your code |
| 2 | the command line is wrong |
| 3 | fix your builder, or run again: a missing tool, a timeout, a file that cannot be read or written, a workspace another buildl is using, a `git` that is missing or fails |
| 4 | the build wants more than the ceiling, or than a plugin's manifest or cap allows, or its approver did not approve it |
| 5 | a build file or `buildl.toml` is wrong, a plugin is not fetched or its files hold a link when fetched, or discovery targets declared differently on the second pass |
| 6 | a state file, or the remembered approvals file, is not what buildl writes, or a plugin changed since it was fetched, a link in its files included |
| 7 | buildl is at fault |

The binary builds on Unix targets only; the supported platforms are Linux and macOS.

**Status:** pre-release. The commands above are the whole surface.

## License

Apache-2.0
