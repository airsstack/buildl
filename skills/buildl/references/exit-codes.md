# Exit codes

buildl ends with one of eight codes. When one build ends in several ways, the most serious wins: 7
over 3, and 3 over 1.

| Code | Name | Whose fault | Running again may help? |
|---|---|---|---|
| 0 | Success | nobody's | |
| 1 | ActionFailure | the code being built | no, fix the code |
| 2 | Usage | the command line | no, fix the command |
| 3 | Infrastructure | the builder or the machine | yes |
| 4 | Refused | the build wants more than the workspace allows, or than was approved | no, ask the user |
| 5 | Definition | a build file or `buildl.toml` | no, fix the file |
| 6 | CorruptState | a file buildl keeps its state in | no |
| 7 | Internal | buildl itself: a bug | no, report it |

## 0, Success

Every target asked for was built, served from the cache, fetched from the remote cache or run there.

```text
$ buildl build hello.txt
1 built
```

## 1, ActionFailure

An action exited with a status other than 0, was ended by a signal (a timeout is one), left a
declared output missing, or `verify` found its outputs differ from the cache. Run
`buildl log <target>`.

```text
FAILED //:broken.txt: action failure (exit status 1)
    declared in build.lua
    command: sh -c "echo no file for you; exit 1"
1 failed
```

## 2, Usage

The command line is wrong: an unknown flag, a target no build file declares, `--set` for a setting
nothing declares, `test` given a target that is not a test, `log` for a target that has never run,
or remote flags that contradict each other or the workspace.

```text
error: unexpected argument '--fast' found

Usage: buildl build [OPTIONS] [TARGET]...
```

## 3, Infrastructure

The builder or the machine failed. A program a command names is not found; another buildl holds the
workspace; a file of `.buildl` cannot be read or written; a remote call failed under
`--remote-strict`; fetching a plugin from git failed; what a target printed is gone from the store.
A missing program and a failed plugin fetch are reported to the user, not retried.

```text
error: remote cache: lookup: grpc://127.0.0.1:1: ActionCache.GetActionResult: cannot connect: transport error: tcp connect error: tcp connect error: Connection refused (os error 61) (after 6 calls)
```

## 4, Refused

The build wants something it may not have. Three causes:

| Cause | Error text |
|---|---|
| the ceiling | `error: build.lua: //:version.txt: program python3 is outside the ceiling (proc.run allows: sh)` |
| a plugin | `error: build.lua: //:bye (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)` |
| the approver | `error: no terminal to ask on; run the build in a terminal once to approve, or set [approver] default = "manifest"`, or `error: the workspace's approver is deny` |

Run `buildl doctor`: a refused want shows as `REFUSED` in the `ceiling` column. Then tell the user
and ask. Never widen the ceiling, a plugin cap or the approver without the user's yes to that exact
change.

## 5, Definition

A build file or `buildl.toml` is wrong, and editing it is the fix. The error names the line.

- a syntax error, a raised error, or a `[declaration]` limit passed;
- a folder named by `subdir` has no build file;
- a declaration is refused: an unknown option, both or neither of `run` and `rule`, a name declared
  twice, a label that names nothing, a dependency cycle, an input file that does not exist, two
  targets writing the same output, a command with no program;
- a target declares `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH`, which the sandbox sets itself;
- a git plugin has not been fetched: run `buildl plugins fetch`;
- `buildl.toml` is missing, does not parse, or asks for what this version cannot do.

```text
error: build.lua: syntax error: lua error in build.lua: syntax error: build.lua:3: '}' expected (to close '{' at line 2) near <eof>
```

## 6, CorruptState

A file in `.buildl` is not what buildl writes there, or a fetched plugin's files no longer match
`buildl.lock`. Something else changed it. Stop and report. Do not delete `.buildl` without asking. For
a changed plugin, the error says so and `buildl plugins fetch` brings back the pinned files.

```text
error: plugin gobin was changed after it was fetched: its files digest to 918aa0f1... and buildl.lock pins e49c0207...
```

## 7, Internal

buildl reached a state it has no answer for. It is a bug in buildl, never the user's fault. Stop and
report it.
