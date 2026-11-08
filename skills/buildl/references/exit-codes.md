# Exit codes

buildl ends with one of eight codes. When one build ends in several ways, the most serious wins: 7
over 3, and 3 over 1.

| Code | Name | Whose fault | Running again may help? |
|---|---|---|---|
| 0 | Success | nobody's | |
| 1 | ActionFailure | the code being built | no, fix the code |
| 2 | Usage | the command line | no, fix the command |
| 3 | Infrastructure | the builder or the machine | yes |
| 4 | Refused | the build wants more than the workspace allows, or than was approved | no, widen the ceiling or approve (the user's call) |
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
| the ceiling | `error: build.lua: //:version.txt: program python3 is outside the ceiling (proc.run allows: sh)`, or for a variable marked `"unkeyed"` that `env.unkeyed` does not list, `error: build.lua: //:app: unkeyed variable HOME is outside the ceiling (env.unkeyed allows: nothing)` |
| a plugin | `error: build.lua: //:bye (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)`, or `error: build.lua: //:bye (plugin gobin@0.1.0): unkeyed variable HOME is outside plugin gobin@0.1.0 (env.unkeyed allows: nothing)` |
| the approver | `error: no terminal to ask on; run the build in a terminal once to approve, or set [approver] default = "manifest"`, or `error: the workspace's approver is deny` |

`plan` gives the same refusals as `build`, before anything runs. Run `buildl doctor`; it differs by cause:

| Cause | What `doctor` shows |
|---|---|
| the ceiling | the want is `REFUSED` in the `ceiling` column |
| a plugin | the want is `allowed` in the `ceiling` column, and the refusal is listed under `plugin violations` |
| the approver | the want is `allowed` in the `ceiling` column and `no` in the `approved` column with `deny`. With `interactive` and no terminal, read the error |

Then tell the user and ask. Never widen the ceiling, a plugin cap or an `extension.toml`, or change the approver, without the
user's yes to that exact change.

## 5, Definition

A build file or `buildl.toml` is wrong, and editing it is the fix. The error names the line.

- a syntax error, a raised error, or a `[declaration]` limit passed;
- a folder named by `subdir` has no build file;
- a declaration is refused: an unknown option, both or neither of `run` and `rule`, a name declared
  twice, a label that names nothing, a dependency cycle, an input file that does not exist, two
  targets writing the same output, a command with no program;
- a target declares `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH`, which the sandbox sets itself;
- a target's `env` is wrong, or a file holds this machine's folder (the cases below);
- a git plugin has not been fetched: run `buildl plugins fetch`;
- `buildl.toml` is missing, does not parse, or asks for what this version cannot do.

These cases name the file that is wrong: a build file with a target or a declaration number, or `buildl.toml` or an `extension.toml` with a table. Fix the file, then run `check` again:

| Case | Error text | Fix |
|---|---|---|
| `env` written as a list, or a list mixed with marks | `build.lua: //:app: env must mark each variable "key" or "unkeyed", such as env = { HOME = "unkeyed" }` | mark each variable |
| a mark that is not `"key"` or `"unkeyed"` | `build.lua: //:app: env HOME is marked "pass"; mark it "key" or "unkeyed"` | write `"key"` or `"unkeyed"` |
| `env.unkeyed` lists a variable its table's `env.read` does not | `buildl.toml: ceiling env.unkeyed lists HOME, which env.read does not` | add the name to `env.read` of the same table (the user's decision), or take it out of `env.unkeyed`. When the refusal names a plugin's `extension.toml`, the manifest is the plugin author's: offer no edit to it |
| a command word holds this machine's folder | `build.lua: //:note: the command holds this machine's path /home/alice; it would give the target another key on every machine` | use a placeholder or a relative path |
| a `"key"` variable's value holds one | `build.lua: //:app: the variable RUSTFLAGS holds this machine's path /home/alice; mark it "unkeyed" if it only finds files` | mark it `"unkeyed"` if it only finds files, else give the variable a value without the folder |
| a discovery target's output holds one | `build.lua: //:cargo.metadata: its output holds this machine's path /home/alice/shop` | change the discovery's command so its output is relative. The refusal repeats until the command changes |
| a setting a command names holds one | `build.lua: //:setting: the setting prefix holds this machine's path /home/alice; it would give the target another key on every machine` | give the setting a value without the folder |
| a variable name that is not valid | `build.lua: declaration #0: invalid env: invalid environment variable name: "A-B" — may hold only ASCII letters, digits and '_'` | write a valid name |

The folders checked are the home folder, the workspace folder and the sandbox folder. Inputs and
outputs are workspace-relative, so they are not checked. A `"key"` variable whose value holds none
of them passes.

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
