# D. Exit Codes

`buildl` ends with one of eight codes, so a script, a CI job or an editor can tell whose fault a
failure is without reading the message. They are defined in `crates/buildl-cli/src/exit.rs`.

| Code | Name | Whose fault | Running again may help? |
|---|---|---|---|
| 0 | Success | nobody's: the command did what it was asked | |
| 1 | ActionFailure | the code being built | no, fix the code |
| 2 | Usage | the command line | no, fix the command |
| 3 | Infrastructure | the builder or the machine | yes |
| 4 | Refused | the build wants more than the workspace allows, or than was approved | no, widen the ceiling or approve |
| 5 | Definition | a build file or `buildl.toml` | no, fix the file |
| 6 | CorruptState | a file `buildl` keeps its state in | no, the state was changed by something else |
| 7 | Internal | `buildl` itself: a bug | no, report it |

The codes stay clear of 126, 127 and everything above 128, which a shell uses for its own
meanings. The code a failed action exited with is never `buildl`'s own: it is in the report of the
failure, as `exit status 1` below.

When one build has targets that end in different ways, the most serious code wins: 7 over 3, and
3 over 1. A builder that failed means the run says nothing reliable about the code.

## 0, Success

Every target asked for was built, served from the cache, fetched from the remote cache or run
there. A target skipped because another failed does not change the code: that failure decides it.
A `verify` that checked nothing also ends with 0.

```console
$ buildl build hello.txt
1 built
$ echo $?
0
```

## 1, ActionFailure

An action ran and failed. Any of these ends with 1:

- it exited with a status other than 0;
- a signal ended it, which is how a timeout ends it;
- it exited 0, but left nothing, or something that cannot be kept, where a declared output should
  be;
- its output is what it prints (`stdout = true`), and it also wrote that output's file itself;
- `verify` ran it again and its outputs differ from the cache.

```lua
b.target("broken.txt", { run = { "sh", "-c", "echo no file for you; exit 1" } })
```

```console
$ buildl build broken.txt
--- //:broken.txt
no file for you
FAILED //:broken.txt: action failure (exit status 1)
    declared in build.lua
    command: sh -c "echo no file for you; exit 1"
1 failed
$ echo $?
1
```

## 2, Usage

The command line is wrong:

- an unknown flag or argument, or a missing one;
- a target no build file declares, or a `query` that is not `deps(...)` or `rdeps(...)`;
- `--set` for a setting nothing declares;
- `test` given a target that is not a test;
- `log` asked for a target that has never run;
- remote flags that contradict each other or the workspace, upload or remote execution asked for
  under `strategy = "bare"`, a `BUILDL_REMOTE_` variable that is neither `1` nor `0`, or a remote
  token that is named but cannot be read.

```console
$ buildl build --fast
error: unexpected argument '--fast' found

  tip: to pass '--fast' as a value, use '-- --fast'

Usage: buildl build [OPTIONS] [TARGET]...

For more information, try '--help'.
$ echo $?
2
```

## 3, Infrastructure

The builder or the machine failed, and nothing is known about the code. Running again may help:

- a program a command names is not found;
- the builder could not run an action at all;
- another `buildl` holds the workspace, or a file of `.buildl` cannot be read or written;
- a call to the remote cache failed under `--remote-strict`;
- fetching a plugin from git failed;
- the file watcher of `watch` failed;
- what a target printed when it last ran, or an output `verify` needs, is gone from the store.

With a `[remote]` table whose `url` points at a port nothing listens on, here
`grpc://127.0.0.1:1`, and `--remote-strict`, the first lookup fails after its five retries:

```console
$ buildl build --remote-strict
error: remote cache: lookup: grpc://127.0.0.1:1: ActionCache.GetActionResult: cannot connect: transport error: tcp connect error: tcp connect error: Connection refused (os error 61) (after 6 calls)
$ echo $?
3
```

The `os error` number is the operating system's: 61 on macOS, 111 on Linux. Without
`--remote-strict`, the same run warns and builds locally:

```console
$ buildl build
warning: remote cache lookup failed: grpc://127.0.0.1:1: ActionCache.GetActionResult: cannot connect: transport error: tcp connect error: tcp connect error: Connection refused (os error 61) (after 6 calls); the build goes on without it
1 built
$ echo $?
0
```

## 4, Refused

The build wants something it may not have:

- a program, variable or directory outside the ceiling in `buildl.toml`;
- a plugin's target wants something its plugin does not allow;
- the approver said no, or is `deny`.

The ceiling here allows only `sh`, and the target runs `python3`:

```lua
b.target("version.txt", { run = { "python3", "--version" }, stdout = true })
```

```console
$ buildl build version.txt
error: build.lua: //:version.txt: program python3 is outside the ceiling (proc.run allows: sh)
$ echo $?
4
```

## 5, Definition

A build file or `buildl.toml` is wrong, and editing it is the fix:

- a build file has a syntax error, raises an error, or goes over a `[declaration]` limit;
- a folder named by `subdir` has no build file;
- a declaration is refused: an unknown option, both or neither of `run` and `rule`, a name declared
  twice, a label or setting that names nothing, a dependency cycle, an input file that does not
  exist, two targets writing the same output, a command with no program;
- two evaluations of the build files declared different things;
- a target declares `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH`, which the sandbox sets itself;
- a git plugin has not been fetched;
- `buildl.toml` is missing, does not parse, or asks for what this version cannot do, such as
  `strategy = "container"`.

This `build.lua` is missing the `})` that closes the options and the call:

```lua
local b = buildl
b.target("hello.txt", { run = { "sh", "-c", "echo hello > $out" }
```

```console
$ buildl build
error: build.lua: syntax error: lua error in build.lua: syntax error: build.lua:3: '}' expected (to close '{' at line 2) near <eof>
$ echo $?
5
```

## 6, CorruptState

A file in `.buildl` does not hold what `buildl` writes there: it is not JSON, it does not describe
what it should, or its bytes are not the exact bytes `buildl` writes for that value. The same code
is given when a fetched plugin's files in the store are no longer the ones `buildl.lock` pinned.
`buildl` never makes such a file itself, so something else changed it.

## 7, Internal

`buildl` reached a state it has no answer for: an error that should never happen, or an error or
outcome this binary has no row for in its table of codes. This is a bug in `buildl`, never the user's fault.
