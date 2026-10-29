# buildl.toml

`buildl.toml` sits at the workspace root and marks it. Every table and key is optional, and an empty
file is valid: the ceiling is then empty, which allows nothing. A key this version does not know is
an error (exit 5), reported on one line with the place it was written:

```text
error: manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`
```

| Table | What it sets |
|---|---|
| `[workspace]` | the workspace's name, build file name and output directory |
| `[ceiling]` | the most any build may be granted |
| `[declaration]` | the limits one build file is evaluated under |
| `[execution]` | how actions are run |
| `[approver]` | who approves what a build wants |
| `[plugins.<name>]` | where each plugin comes from, and what it may have |
| `[remote]` | the remote cache, and remote execution |

## `[workspace]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | none | a name for people. Not empty |
| `entry` | string | `"build.lua"` | the file name of the build file in every directory |
| `out` | string | `"out"` | the directory every output is placed under. Not the workspace root, and not `.buildl` or a directory holding or inside it |
| `config` | integer | none | the version of this file's format. When written, it must be `1` |

buildl keeps its own files in `.buildl`. No key changes that.

## `[ceiling]`

Three parts, written as `[ceiling.proc]`, `[ceiling.env]` and `[ceiling.fs]` or as dotted keys. A
list left out is empty, and an empty list allows nothing. The output directory and `.buildl` are not
added for you: a workspace that builds writes them into `fs.write` itself. There is no network key.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `proc.run` | list of strings | `[]` | the programs a command may run, by bare name such as `"cc"` |
| `env.read` | list of strings | `[]` | the environment variables a target may read |
| `fs.read` | list of strings | `[]` | the directories inputs may come from. `"."` is the workspace root |
| `fs.write` | list of strings | `[]` | the directories a build may write under |

```toml
[ceiling]
proc.run = ["go"]
env.read = ["HOME"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

A program a dependency built needs no `proc.run` entry.

## `[declaration]`

Limits on evaluating one build file. A build file that goes over one stops with an error (exit 5).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `memory` | string | `"16MB"` | a whole number followed by `B`, `KB`, `MB` or `GB`. Not zero |
| `instructions` | integer | `10000000` | how many Lua instructions one build file may run. At least 1 |
| `walk_entries` | integer | `100000` | how many directory entries its `sources` listings may visit. At least 1 |

```toml
[declaration]
walk_entries = 500000
```

## `[execution]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `workers` | integer | `0` | how many actions run at once. `0` is one per core |
| `timeout` | string | `"10m"` | how long one action may run: a whole number followed by `ms`, `s`, `m` or `h`. Not zero |
| `strategy` | string | `"sandbox"` | how strongly actions are kept apart (below) |

| `strategy` | What it does |
|---|---|
| `"sandbox"` | runs each action in a fresh directory outside the workspace, holding only its declared inputs, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned |
| `"bare"` | runs each action as a plain process in the workspace, with only the variables its target declared. Do not use it to get past a sandbox failure |
| `"os-sandbox"`, `"container"` | accepted, but `build`, `test` and `verify` refuse them |

```toml
[execution]
workers = 4
timeout = "5m"
```

## `[approver]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `default` | string | `"manifest"` | who answers whether a build may have what it wants |

| `default` | Means |
|---|---|
| `"manifest"` | the ceiling is all the approval there is |
| `"interactive"` | you are asked on the terminal about anything new, and a yes is remembered outside the workspace. With no terminal, the build is refused (exit 4) |
| `"deny"` | no build runs (exit 4) |

```toml
[approver]
default = "manifest"
```

## `[plugins.<name>]`

One table per plugin, under the name build files load it by with `buildl.use`. A plugin comes from
exactly one of `path`, `git` and `github`. What the plugin may have starts empty.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `path` | string | none | a directory of the workspace holding the plugin. Takes no `rev` |
| `git` | string | none | the URL of a git repository holding the plugin. Needs `rev` |
| `github` | string | none | `owner/repo`, the same as `git = "https://github.com/owner/repo"`. Needs `rev` |
| `rev` | string | none | the tag or full commit the repository is pinned to |
| `proc.run` | list of strings | `[]` | the programs the plugin's targets may run. `proc.run`, `env.read` and `network` here are the workspace's cap |
| `env.read` | list of strings | `[]` | the environment variables the plugin's targets may read |
| `network` | boolean | `false` | whether the plugin's targets may reach the network |

```toml
[plugins.gobin]
path     = "plugins/gobin"
proc.run = ["go"]
env.read = ["HOME"]
```

A git plugin is refused by every command that reads the workspace until `buildl plugins fetch` has pinned it. See
`plugins.md`.

## `[remote]`

A cache this workspace shares results with, which may also run actions. With no `[remote]` table
buildl uses no remote. When the table is there, `url` is required.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `url` | string | required | `grpc://` or `grpcs://`, a host, and an optional `:port` |
| `instance` | string | none | the instance name the cache's storage is addressed under |
| `ca` | string | none | a certificate authority file to trust: an absolute path, or one inside the workspace |
| `token.env` | string | none | the environment variable holding the token. Only over `grpcs://` |
| `token.file` | string | none | the file holding the token. Name only one of `token.env` and `token.file` |
| `header` | string | `"authorization"` | the request header the token is sent in |
| `timeout` | string | `"30s"` | how long one request may take, written as in `[execution]` |
| `execute` | boolean | `false` | whether the cache also runs the actions it has no result for |
| `jobs` | integer | `128` | how many actions may be with the remote at once, 1 to 1024 |
| `queue_timeout` | string | `"1h"` | how long a remote execution may wait in the queue |
| `retries` | integer | `5` | how many times a failed call is tried again, 0 to 10 |
| `properties` | table of strings | none | platform properties asked of the executor, such as `pool = "large"` |

```toml
[remote]
url = "grpcs://cache.example.com:443"
token.env = "BUILDL_TOKEN"
```
