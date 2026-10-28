# A. Command-Line Reference

Everything here is what `buildl --help` and each command's `--help` print in this version. Run
`buildl` from the workspace root, the folder holding `buildl.toml`, or from any folder below it.

```console
$ buildl --help
A sandboxed, deterministic build system

Usage: buildl [OPTIONS] <COMMAND>
```

| Command | What it does |
|---|---|
| [`check`](#buildl-check) | Evaluate every build file twice and run nothing |
| [`graph`](#buildl-graph) | Print the target graph |
| [`query`](#buildl-query) | Ask what a target needs, or what needs it |
| [`plan`](#buildl-plan) | Say what a build would run, and why |
| [`build`](#buildl-build) | Build targets and what they need |
| [`watch`](#buildl-watch) | Build targets, then build again when a file they read changes |
| [`test`](#buildl-test) | Run test targets and what they need |
| [`verify`](#buildl-verify) | Re-run cached actions and name those whose outputs differ |
| [`doctor`](#buildl-doctor) | Print the workspace's ceiling, approver, run settings and what its build wants |
| [`log`](#buildl-log) | Show what a target printed when it last ran |
| [`clean`](#buildl-clean) | Remove the output directory |
| [`gc`](#buildl-gc) | Drop rows of targets no longer declared and remove what nothing needs |
| [`plugins`](#buildl-plugins) | Work with the plugins `buildl.toml` names |

`buildl help` prints the same list, and `buildl help <COMMAND>` the help of one command.

## Options every command takes

These three are accepted before the command and after it, by every command and subcommand.

| Flag | What it does |
|---|---|
| `--color <WHEN>` | When to colour what is printed: `auto` (the default) colours when standard error is a terminal and `NO_COLOR` is unset, `always` always does, `never` never does |
| `-h`, `--help` | Print help; `-h` prints a shorter summary |
| `-V`, `--version` | Print the version, such as `buildl 0.1.0` |

## Naming targets

Every command that takes a target accepts it in three forms:

| Form | Means |
|---|---|
| `//dir:name` | the target `name` declared in the build file of `dir` |
| `:name` | the target `name` declared in the build file of the folder you are in |
| `name` | the same as `:name` |

## `buildl check`

```text
buildl check [OPTIONS]
```

Evaluates every build file twice and runs nothing. Two evaluations that declare different targets
mean a build file is not deterministic, and `check` says so. No flags of its own.

## `buildl graph`

```text
buildl graph [OPTIONS]
```

Prints the target graph.

| Flag | What it does |
|---|---|
| `--json` | Print the graph as canonical JSON, the bytes of `.buildl/graph.json` (the default) |
| `--dot` | Print the graph in Graphviz DOT |

## `buildl query`

```text
buildl query [OPTIONS] <EXPR>
```

| Argument | Means |
|---|---|
| `<EXPR>` | `deps(TARGET)` for what `TARGET` needs, or `rdeps(TARGET)` for what needs it |

| Flag | What it does |
|---|---|
| `--json` | Print the answer as canonical JSON |

## `buildl plan`

```text
buildl plan [OPTIONS] [TARGET]...
```

Says what a build would run, and why, without running it. With no target, it plans every target but
tests.

| Flag | What it does |
|---|---|
| `--set <NAME=VALUE>` | Give a build setting a value for this run; may be repeated |
| `--json` | Print the plan as canonical JSON |

## `buildl build`

```text
buildl build [OPTIONS] [TARGET]...
```

Builds targets and what they need. With no target, it builds every target but tests.

| Flag | What it does |
|---|---|
| `--set <NAME=VALUE>` | Give a build setting a value for this run; may be repeated |
| `-k`, `--keep-going` | After a failure, still build everything that does not depend on it |
| `--stream` | Show one running action's output as it is printed |
| `--remote-upload` | Upload what this run produced to the remote cache |
| `--remote-strict` | Fail the run when a call to the remote cache fails, instead of warning and building locally |
| `--no-remote` | Do not use the remote cache in this run, and run nothing on the remote, whatever the manifest and variables say |
| `--remote-execute` | Run the actions the remote cache has no result for on the remote, whatever the manifest and variables say |
| `--no-remote-execute` | Run every action here, and keep using the remote cache, whatever the manifest and variables say |
| `--remote-jobs <N>` | How many actions may be with the remote at once, from 1 to 1024, for this run |

The six remote flags are refused, with exit code 2, when they contradict each other or the
workspace:

| Refused | Why |
|---|---|
| `--no-remote` with `--remote-upload`, `--remote-strict`, `--remote-execute` or `--remote-jobs` | the run uses no remote |
| `--remote-execute` with `--no-remote-execute` | they say opposite things |
| `--remote-upload`, `--remote-strict`, `--remote-execute`, `--no-remote-execute` or `--remote-jobs` in a workspace with no `[remote]` table | there is no remote to use |
| `--remote-jobs` in a run where remote execution is off | there is nothing to count |
| `--remote-upload` or `--remote-execute` under `strategy = "bare"` | a result from an unconfined action is not shared |

## `buildl watch`

```text
buildl watch [OPTIONS] [TARGET]...
```

Builds the targets, then builds them again whenever a file the build reads changes, or the build
files declare a different graph. It takes exactly the flags of [`build`](#buildl-build):

| Flag | What it does |
|---|---|
| `--set <NAME=VALUE>` | Give a build setting a value for this run; may be repeated |
| `-k`, `--keep-going` | After a failure, still build everything that does not depend on it |
| `--stream` | Show one running action's output as it is printed |
| `--remote-upload` | Upload what this run produced to the remote cache |
| `--remote-strict` | Fail the run when a call to the remote cache fails, instead of warning and building locally |
| `--no-remote` | Do not use the remote cache in this run, and run nothing on the remote, whatever the manifest and variables say |
| `--remote-execute` | Run the actions the remote cache has no result for on the remote, whatever the manifest and variables say |
| `--no-remote-execute` | Run every action here, and keep using the remote cache, whatever the manifest and variables say |
| `--remote-jobs <N>` | How many actions may be with the remote at once, from 1 to 1024, for this run |

Changes under the output directory, under `.buildl` and under `.git` are ignored. `watch` runs
until Ctrl-C, a usage error (exit code 2) or a failure of the file watcher (exit code 3).

## `buildl test`

```text
buildl test [OPTIONS] [TARGET]...
```

Runs test targets and what they need. With no target, it runs every test.

| Flag | What it does |
|---|---|
| `--set <NAME=VALUE>` | Give a build setting a value for this run; may be repeated |
| `--stop` | Start nothing new after the first failure |
| `--stream` | Show one running action's output as it is printed |
| `--rerun` | Run the selected tests even when nothing they depend on changed |
| `--verbose` | Show what every test printed, not only what a failing one did |
| `--remote-upload` | Upload what this run produced to the remote cache |
| `--remote-strict` | Fail the run when a call to the remote cache fails, instead of warning and building locally |
| `--no-remote` | Do not use the remote cache in this run, and run nothing on the remote, whatever the manifest and variables say |
| `--remote-execute` | Run the actions the remote cache has no result for on the remote, whatever the manifest and variables say |
| `--no-remote-execute` | Run every action here, and keep using the remote cache, whatever the manifest and variables say |
| `--remote-jobs <N>` | How many actions may be with the remote at once, from 1 to 1024, for this run |

The remote flags are refused in the same cases as for [`build`](#buildl-build).

## `buildl verify`

```text
buildl verify [OPTIONS] [TARGET]...
```

Runs cached actions again in the input sandbox and names those whose outputs differ from the cache.
It records nothing. With no target, it takes every target but tests. Under `strategy = "bare"` it is
refused.

| Flag | What it does |
|---|---|
| `--set <NAME=VALUE>` | Give a build setting a value for this run; may be repeated |
| `--sample <N>` | Re-run only the N cached actions whose keys sort lowest: the same ones while their keys stay the same |

## `buildl doctor`

```text
buildl doctor [OPTIONS]
```

Prints the workspace's ceiling, its approver, its run settings, its remote settings and everything
its build wants. It refuses nothing and connects to nothing. No flags of its own.

## `buildl log`

```text
buildl log [OPTIONS] <TARGET>
```

Shows what a target printed when it last ran.

| Argument | Means |
|---|---|
| `<TARGET>` | The target, as `//dir:name`, `:name` or `name` |

| Flag | What it does |
|---|---|
| `--stdout` | Print only what the run wrote to standard output |
| `--stderr` | Print only what the run wrote to standard error, on standard output |

## `buildl clean`

```text
buildl clean [OPTIONS]
```

Removes the output directory, `out` unless `buildl.toml` names another. The cache in `.buildl` is
kept. No flags of its own.

## `buildl gc`

```text
buildl gc [OPTIONS]
```

Drops the cache rows of targets no longer declared, and removes from the store what nothing needs.
No flags of its own.

## `buildl plugins`

```text
buildl plugins [OPTIONS] <COMMAND>
```

Works with the plugins `buildl.toml` names. It has one subcommand.

### `buildl plugins fetch`

```text
buildl plugins fetch [OPTIONS]
```

Pins every git plugin in `buildl.lock` and stores its files, so a build needs no network. No flags
of its own.

## Environment variables

`buildl` reads these from the environment it is started in. None of them reaches an action: an
action gets only the variables its target declares.

| Variable | Read by | What it does |
|---|---|---|
| `NO_COLOR` | every command | when set and not empty, `--color auto` does not colour |
| `TERM` | every command | when unset, empty or `dumb`, output is plain lines: no live status line and no colour under `--color auto` |
| `BUILDL_REMOTE_UPLOAD` | `build`, `watch`, `test` | `1` is the same as `--remote-upload`; `0` or unset is off |
| `BUILDL_REMOTE_STRICT` | `build`, `watch`, `test` | `1` is the same as `--remote-strict`; `0` or unset is off |
| `BUILDL_REMOTE_EXECUTE` | `build`, `watch`, `test` | `1` turns remote execution on and `0` turns it off, over what `[remote] execute` says; the flags win over it |
| `XDG_DATA_HOME` | the `interactive` approver | the folder approvals are kept in, as `buildl/approved/<name>.json` below it, when it is an absolute path |
| `HOME` | the `interactive` approver | when `XDG_DATA_HOME` is not usable, approvals are kept below `$HOME/.local/share` instead |
| `PATH` | every command that reads the workspace | the folders searched for a program named by its bare name, such as `cc`, and for `git` when fetching plugins |

For the three `BUILDL_REMOTE_` variables, any value other than `1` or `0` is refused.
