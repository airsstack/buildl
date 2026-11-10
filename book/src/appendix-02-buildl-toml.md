# B. `buildl.toml` Reference

`buildl.toml` sits at the workspace root and marks it. Every table and every key in it is optional,
and an empty file is valid: every setting takes its default, and the ceiling is empty, which allows
nothing. A key this version does not know is an error wherever it stands, and so is a value of the
wrong type or one its key refuses. Each is reported on one line with the place it was written:

```text
error: manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`
```

The **Line** column below names the line of `crates/buildl/src/manifest/schema.rs` that declares
the key, so you can check this page against the code.

| Table | What it sets | Line |
|---|---|---|
| [`[workspace]`](#workspace) | the workspace's name, its build file name and its output directory | 66 |
| [`[ceiling]`](#ceiling) | the most any build may be granted | 69 |
| [`[declaration]`](#declaration) | the limits one build file is evaluated under | 72 |
| [`[execution]`](#execution) | how actions are run | 75 |
| [`[approver]`](#approver) | who approves what a build wants | 78 |
| [`[plugins.<name>]`](#pluginsname) | where each plugin comes from, and what it may have | 81 |
| [`[remote]`](#remote) | the remote cache, and remote execution | 85 |

## `[workspace]`

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `name` | string | none | a name for people; must not be empty | 93 |
| `entry` | string | `"build.lua"` | the file name of the build file in every directory | 95 |
| `out` | string | `"out"` | the directory every output is placed under; not the workspace root, and not `.buildl`, a directory holding it or one inside it | 97 |
| `config` | integer | none | the version of this file's format; when written, it must be `1` | 99 |

`buildl` keeps its own files, the cache, the store and the log, in `.buildl`. No key changes that.

## `[ceiling]`

The ceiling has three parts, written as sub-tables `[ceiling.proc]`, `[ceiling.env]` and
`[ceiling.fs]`, or as dotted keys such as `proc.run = [...]` under `[ceiling]`. A list left out is
empty, and an empty list allows nothing. The output directory and `.buildl` are not added for you:
a workspace that builds writes them into `fs.write` itself.

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `proc` | table | empty | the programs, below | 108 |
| `env` | table | empty | the environment variables, below | 111 |
| `fs` | table | empty | the directories, below | 114 |
| `proc.run` | list of strings | `[]` | the programs a command may run, by bare name such as `"cc"` | 123 |
| `env.read` | list of strings | `[]` | the environment variables a target may read | 132 |
| `env.unkeyed` | list of strings | `[]` | the variables a target may mark `"unkeyed"`: given to the command, but only the name enters the action key and nothing is sent to a remote server. Every name must also be in `env.read`, or the file is refused | 135 |
| `fs.read` | list of strings | `[]` | the directories inputs may come from; `"."` is the workspace root | 144 |
| `fs.write` | list of strings | `[]` | the directories a build may write under; `"."` is the workspace root | 147 |

`proc` and `env` are also the names of the same lists in a plugin's table, below. A variable may
be marked `"unkeyed"` only when every level lists it: the ceiling's `env.unkeyed` and, for a
plugin's target, the plugin's table and its manifest too. The
[Build File API](appendix-03-lua-api.md#targetname-options) says what the two marks do.

## `[declaration]`

Limits on evaluating one build file. A build file that goes over one stops with an error.

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `memory` | string | `"16MB"` | the memory one build file may use: a whole number followed by `B`, `KB`, `MB` or `GB`, each 1024 times the one before; not zero | 155 |
| `instructions` | integer | `10000000` | how many Lua instructions one build file may run; at least 1 | 157 |
| `walk_entries` | integer | `100000` | how many directory entries one build file's source listings may visit; at least 1. A folder a `.buildlignore` ignores is not entered and an ignored path is not counted | 159 |

## `[execution]`

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `workers` | integer | `0` | how many actions run at once; `0` is one per core of the machine | 167 |
| `timeout` | string | `"10m"` | how long one action may run before it is stopped: a whole number followed by `ms`, `s`, `m` or `h`; not zero | 169 |
| `strategy` | string | `"sandbox"` | how strongly actions are kept apart, one of the four below | 171 |

| `strategy` | What this version does with it |
|---|---|
| `"bare"` | runs each action as a plain process in the workspace, with only the variables its target declared |
| `"sandbox"` | runs each action in a fresh directory outside the workspace, holding only its declared inputs, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned |
| `"os-sandbox"` | accepted, but `build`, `test` and `verify` refuse it |
| `"container"` | accepted, but `build`, `test` and `verify` refuse it |

## `[approver]`

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `default` | string | `"manifest"` | who answers whether a build may have what it wants: `"manifest"`, `"interactive"` or `"deny"` | 179 |

| `default` | Means |
|---|---|
| `"manifest"` | the ceiling is all the approval there is |
| `"interactive"` | you are asked on the terminal about anything new, and a yes is remembered outside the workspace |
| `"deny"` | no build runs |

## `[plugins.<name>]`

One table per plugin, under the name build files load it by with `buildl.use`. A plugin comes from
exactly one of `path`, `git` and `github`. What the plugin may have starts empty: a list or flag
left out allows nothing.

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `path` | string | none | a directory of the workspace holding the plugin; not the output directory or `.buildl`, nor one holding or inside them; takes no `rev` | 226 |
| `git` | string | none | the URL of a git repository holding the plugin; needs `rev` | 228 |
| `github` | string | none | a GitHub repository written `owner/repo`, the same as `git = "https://github.com/owner/repo"`; needs `rev` | 230 |
| `rev` | string | none | the tag or full commit the repository is pinned to | 232 |
| `dir` | string | the repository root | the folder of the repository that holds the plugin, relative to its root, with no `..`, no leading `/` and no empty part; only that folder is stored and digested; takes `git` or `github`, not `path` | 234 |
| `proc` | table | empty | holds `run`, the programs the plugin's targets may run | 237 |
| `env` | table | empty | holds `read` and `unkeyed`, the environment variables the plugin's targets may read, and may mark `"unkeyed"` | 240 |
| `network` | boolean | `false` | whether the plugin's targets may reach the network | 242 |
| `proc.run` | list of strings | `[]` | as in `[ceiling]` | 123 |
| `env.read` | list of strings | `[]` | as in `[ceiling]` | 132 |
| `env.unkeyed` | list of strings | `[]` | as in `[ceiling]`; every name must also be in this table's `env.read` | 135 |

## `[remote]`

The remote cache this workspace shares results with, which may also run actions. With no
`[remote]` table, `buildl` uses no remote. When the table is there, `url` is required.

| Key | Type | Default | Meaning | Line |
|---|---|---|---|---|
| `url` | string | required | the cache's address: `grpc://` or `grpcs://`, a host, and an optional `:port` | 187 |
| `instance` | string | none | the instance name the cache's storage is addressed under | 189 |
| `ca` | string | none | a certificate authority file to trust: an absolute path, or one inside the workspace | 191 |
| `token` | table | none | where the token sent with every request comes from, below; only over `grpcs://` | 193 |
| `header` | string | `"authorization"` | the request header the token is sent in | 195 |
| `timeout` | string | `"30s"` | how long one request may take, written as in `[execution]` | 197 |
| `execute` | boolean | `false` | whether the cache also runs the actions it has no result for | 199 |
| `jobs` | integer | `128` | how many actions may be with the remote at once, from 1 to 1024 | 201 |
| `queue_timeout` | string | `"1h"` | how long a remote execution may wait in the cache's queue, written as in `[execution]` | 203 |
| `retries` | integer | `5` | how many times a failed call is tried again after the first, from 0 to 10 | 205 |
| `properties` | table of strings | none | the platform properties asked of the remote executor, such as `pool = "large"`; `OSFamily` and `ISA` are set by `buildl` and cannot be written | 208 |

`token` names exactly one of its two keys:

| Key | Type | Meaning | Line |
|---|---|---|---|
| `token.env` | string | the environment variable holding the token | 216 |
| `token.file` | string | the file holding the token: an absolute path, or one inside the workspace | 218 |

## Every table at once

This `buildl.toml` writes every table, and every key but the four that fetch a plugin from git.
Beside it are a `build.lua` declaring one target and an empty folder `plugins/local`. It passes
`buildl check`:

```toml
[workspace]
name = "everything"
entry = "build.lua"
out = "out"
config = 1

[ceiling]
proc.run = ["sh"]
env.read = ["CC"]
env.unkeyed = ["CC"]
fs.read = ["."]
fs.write = ["out", ".buildl"]

[declaration]
memory = "16MB"
instructions = 10000000
walk_entries = 100000

[execution]
workers = 0
timeout = "10m"
strategy = "sandbox"

[approver]
default = "manifest"

[plugins.local]
path = "plugins/local"
proc.run = ["sh"]
env.read = ["CC"]
env.unkeyed = ["CC"]
network = false

[remote]
url = "grpcs://cache.example.com:443"
instance = "main"
ca = "certs/ca.pem"
token.env = "BUILDL_TOKEN"
header = "authorization"
timeout = "30s"
execute = false
jobs = 128
queue_timeout = "1h"
retries = 5
properties = { pool = "large" }
```

```console
$ buildl check
ok: 1 declaration in 1 build file
```

A plugin from git is written with `git` or `github`, and `rev`, and with `dir` when the plugin is
one folder of the repository:

```toml
[plugins.fromgit]
git = "https://example.com/plugins/fromgit.git"
rev = "v1.0.0"

[plugins.fromgithub]
github = "example/fromgithub"
rev = "v1.0.0"
dir = "plugins/fromgithub"
```

Every command that reads the workspace, `check` included, refuses such a plugin until
`buildl plugins fetch` has pinned it in `buildl.lock`:

```console
$ buildl check
error: plugin fromgit is not fetched: buildl.lock does not pin https://example.com/plugins/fromgit.git at v1.0.0; run `buildl plugins fetch`
```

[Plugins from Git](ch08-02-plugins-from-git.md) shows the whole round.
