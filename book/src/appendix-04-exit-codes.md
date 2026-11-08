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

- a program, variable or directory outside the ceiling in `buildl.toml`, or a variable marked
  `"unkeyed"` that `env.unkeyed` does not list;
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

A variable marked `"unkeyed"` is held to `env.unkeyed` at every level, as a variable marked `"key"`
is held to `env.read`. Here the ceiling lists `HOME` in `env.read` but not in `env.unkeyed`, and the
target marks it `"unkeyed"`:

```lua
b.target("app", { inputs = { "a.txt" }, env = { HOME = "unkeyed" }, run = { "cp", "$in", "$out" } })
```

```console
$ buildl plan
error: build.lua: //:app: unkeyed variable HOME is outside the ceiling (env.unkeyed allows: nothing)
$ echo $?
4
```

When the plugin's table in `buildl.toml` is what leaves `HOME` out of `env.unkeyed`, the message
names the plugin and its version:

```console
$ buildl plan
error: build.lua: //:bye (plugin gobin@0.1.0): unkeyed variable HOME is outside plugin gobin@0.1.0 (env.unkeyed allows: nothing)
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
- a target's `env` is wrong, or a target or a discovery output holds one of this machine's
  folders, below;
- a git plugin has not been fetched;
- `buildl.toml` is missing, does not parse, lists a name in `env.unkeyed` that `env.read` does not,
  or asks for what this version cannot do, such as `strategy = "container"`.

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

### `env` and this machine's folders

These cases are also exit 5. Each message starts with the file that is wrong: a build file with the
target, or `buildl.toml`. Where a message shows the home folder, it is `/home/alice`, the `HOME` of
the run it was captured from.

| Case | Message |
|---|---|
| `env` written as a list, or a list mixed with marks | `error: build.lua: //:app: env must mark each variable "key" or "unkeyed", such as env = { HOME = "unkeyed" }` |
| a mark that is not `"key"` or `"unkeyed"` | `error: build.lua: //:app: env HOME is marked "pass"; mark it "key" or "unkeyed"` |
| a variable name that is not valid | `error: build.lua: declaration #0: invalid env: invalid environment variable name: "A-B" — may hold only ASCII letters, digits and '_'` |
| `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH`, with either mark | `error: build.lua: //:app: env TZ is set to UTC by the sandbox and cannot be declared` |
| `env.unkeyed` lists a name its table's `env.read` does not | `error: buildl.toml: ceiling env.unkeyed lists HOME, which env.read does not` |
| a word of a command holds this machine's folder | `error: build.lua: //:note: the command holds this machine's path /home/alice; it would give the target another key on every machine` |
| a `"key"` variable's value holds one | `error: build.lua: //:app: the variable RUSTFLAGS holds this machine's path /home/alice; mark it "unkeyed" if it only finds files` |
| a setting a command names holds one | `error: build.lua: //:app: the setting prefix holds this machine's path /home/alice; it would give the target another key on every machine` |
| a discovery target's output holds one | `error: build.lua: //:cargo.metadata: its output holds this machine's path /home/alice` |

A name in `env.unkeyed` that `env.read` does not list is refused in the same way in a
`[plugins.<name>]` table and in a plugin's `extension.toml`, and the message names that table or
file.

The folders checked are the home folder, the workspace folder and the action's folder. Inputs and outputs are workspace-relative, so they are not checked, and neither is
the value of an `"unkeyed"` variable, which is in no key. Each check runs at its own moment:

| What holds the folder | When it is refused |
|---|---|
| a word of a command | by every command that reads the build files, before anything runs |
| a `"key"` variable's value, or a setting's value | when the action's key is gathered |
| a discovery target's output | when the second pass reads it |

The discovery's own run finishes, as the `1 built` line shows, and its output stays in the cache, so
the refusal comes again on every run until the discovery's command changes. `tail -n 2` leaves out
anything the discovery printed, and `/home/alice` stands for your own home folder. The command
exits with code 5:

```console
$ buildl build 2>&1 | tail -n 2
1 built
error: build.lua: //:cargo.metadata: its output holds this machine's path /home/alice
```

## 6, CorruptState

A file in `.buildl` does not hold what `buildl` writes there: it is not JSON, it does not describe
what it should, or its bytes are not the exact bytes `buildl` writes for that value. The same code
is given when a fetched plugin's files in the store are no longer the ones `buildl.lock` pinned.
`buildl` never makes such a file itself, so something else changed it.

## 7, Internal

`buildl` reached a state it has no answer for: an error that should never happen, or an error or
outcome this binary has no row for in its table of codes. This is a bug in `buildl`, never the user's fault.
