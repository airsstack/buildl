# `buildl.toml`, the Ceiling

`buildl.toml` sits at the root of the workspace. buildl reads it before any build file, and it is
plain TOML, not Lua, so nothing in it can run. Its `[ceiling]` table is the most any build in the
workspace may ever do. A build file cannot raise it; only you can, by editing this file.

The file is also what makes a directory a workspace. buildl looks for it in the current directory
and the ones above, and a directory with no `buildl.toml` in or above it is an error, never a
default.

This page explains each table with one example. [Appendix B](appendix-02-buildl-toml.md) lists
every key.

## Read strictly

Every table and every key is optional. A key buildl does not know, though, is an error with its
line and column, so a typo cannot quietly drop a permission:

```toml
fs.wirte = ["out", ".buildl"]
```

```console
$ buildl check
error: manifest: buildl.toml:9:4: unknown field `wirte`, expected `read` or `write`
```

## `[workspace]`

```toml
[workspace]
name   = "ambient"
entry  = "build.lua"
out    = "out"
config = 1
```

`name` is for people. `entry` is the file name of every directory's build file, `build.lua` unless
you say otherwise. `out` is where outputs go, `out` by default. `config` is the version of this
file's format, and must be `1`.

## `[ceiling]`

```toml
[ceiling]
proc.run = ["go"]
env.read = ["HOME"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

| Key | Lists |
|---|---|
| `proc.run` | the programs from your machine a command may run |
| `env.read` | the environment variables a target may name in `env` |
| `fs.read` | the directories inputs may come from; `"."` is the whole workspace |
| `fs.write` | the directories a build may write into |

Directories are relative to the workspace root, and buildl, never Lua, works out what they mean.
Each list starts empty. An empty ceiling, even an empty `buildl.toml`, still lets `check`, `graph`
and `query` work, since they run nothing, but every plan is refused:

```console
$ buildl plan
error: directory .buildl is outside the ceiling (fs.write allows: nothing)
```

A build always writes into `.buildl`, where buildl keeps its state, and `out`, so `fs.write` needs
both. An input outside `fs.read` is refused the same way, naming the target. Here the ceiling
writes `fs.read = ["lib"]` and the root build file reads `hello.txt`:

```console
$ buildl plan
error: build.lua: //:greeting: input hello.txt is outside the ceiling (fs.read allows: lib)
```

There is no network list. A target that reaches the network says so with `network = true`, and
the approver and `buildl doctor` show it.

## `[declaration]`

```toml
[declaration]
memory       = "16MB"
instructions = 10_000_000
walk_entries = 100_000
```

The limits one build file is evaluated under: how much memory it may use, how many Lua
instructions it may run, and how many directory entries `buildl.sources` may visit. The values
above are the defaults. A build file that goes over a limit fails.

## `[execution]`

```toml
[execution]
workers  = 4
timeout  = "2m"
strategy = "sandbox"
```

| Key | Means | When left out |
|---|---|---|
| `workers` | how many actions run at once; `0` is one per core | `0` |
| `timeout` | how long one action may run: `500ms`, `90s`, `10m`, `2h` | `"10m"` |
| `strategy` | how actions are isolated: `bare`, `sandbox`, `os-sandbox` or `container` | `"sandbox"` |

`build` and `test` run `bare` and `sandbox`, and refuse the other two rather than run with less
isolation than you asked for. [The Sandbox](ch06-00-sandbox.md) explains the difference. None of
these change what an action produces, so none of them is part of a target's action key.

## `[approver]`

```toml
[approver]
default = "manifest"
```

Who has the last word once a build fits under the ceiling: `manifest`, `interactive` or `deny`.
[Approvers](ch04-03-approvers.md) covers all three.

## `[plugins.<name>]`

```toml
[plugins.gobin]
path     = "plugins/gobin"
proc.run = ["go"]
env.read = ["HOME"]
```

One table per plugin: where its files come from (`path` in the workspace, or `git` or `github` with
a `rev`), and the most its targets may want. A plugin's targets are held to its own lists first,
then to the ceiling, so these lists can narrow what a plugin gets but never widen it.
[Plugins](ch08-00-plugins.md) covers them.

## `[remote]`

```toml
[remote]
url      = "grpcs://cache.example.com:443"
instance = "ci"
token    = { env = "BUILDL_REMOTE_TOKEN" }
```

A shared cache, and optionally machines to build on, that speak the Remote Execution API. The
token itself is never written here, since this file is committed: `token` names the environment
variable or the file to read it from. A workspace without this table never uses a remote.
[Remote Cache and Execution](ch09-00-remote.md) covers it.

## Seeing what you wrote

`buildl doctor` prints the manifest as buildl understood it, with every default filled in, and
connects to nothing. For the manifest built from the examples on this page:

```console
$ buildl doctor
workspace  /private/tmp/buildl-book/full
approver   manifest
execution  workers 4, timeout 2m, strategy sandbox
remote
  url            grpcs://cache.example.com:443
  instance       ci
  ca             -
  header         authorization
  timeout        30s
  token          not set
  upload         off
  strict         off
  execute        off in buildl.toml, BUILDL_REMOTE_EXECUTE not set
  jobs           128
  queue_timeout  1h
  retries        5
  properties: none
  --remote-upload and --remote-strict turn either on for one run
  --remote-execute and --no-remote-execute change execute, and --remote-jobs changes jobs, for one run
ceiling
  proc.run  go
  env.read  HOME
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run go         allowed  yes
  read HOME      allowed  yes
  write .buildl  allowed  yes
  write out      allowed  yes
```

The last part, the wants, is the subject of the next page.
