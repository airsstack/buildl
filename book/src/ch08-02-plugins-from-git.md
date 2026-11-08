# Plugins from Git

A plugin you share between repositories lives in a git repository of its own. The workspace names
the repository and a version of it, `buildl plugins fetch` downloads it once, and from then on no
command reaches the network to load it.

## Naming the repository

Instead of `path`, the plugin's table in `buildl.toml` gives `git` and `rev`:

```toml
[plugins.gobin]
git         = "file:///tmp/buildl-book/p05/git/gobin.git"
rev         = "v0.1.0"
proc.run    = ["go"]
env.read    = ["HOME"]
env.unkeyed = ["HOME"]
```

| Key | Says |
|---|---|
| `git` | any URL `git` can clone: `https://`, `ssh://`, or `file://` as here |
| `github` | instead of `git`: `owner/repo`, short for the GitHub URL |
| `rev` | a tag or a full commit |
| `dir` | optional: the folder of the repository that holds the plugin, such as `plugins/gobin`; left out, the plugin is the repository root |

`dir` is for a repository that holds more than one plugin, or a plugin beside other files. It is
written relative to the repository root, with `/` between folders, and takes no `..`, no leading
`/` and no empty part. Only that folder is stored and digested: the plugin's `extension.toml` sits
directly in it, and the rest of the repository plays no part. A plugin that comes from a `path` has
no `dir`.

The cap, `proc.run`, `env.read` and `env.unkeyed`, works as it does for a plugin kept in the
workspace. For this page the repository is a bare one made on the same machine, holding the `gobin`
plugin from [Writing a Plugin](ch08-01-writing-a-plugin.md) under the tag `v0.1.0`, so nothing here
needs the network.

## Nothing loads until it is fetched

A build does not fetch. It refuses, and says what to run:

```console
$ buildl build
error: plugin gobin is not fetched: buildl.lock does not pin file:///tmp/buildl-book/p05/git/gobin.git at v0.1.0; run `buildl plugins fetch`
```

That exits with code 5.

## `buildl plugins fetch`

```console
$ buildl plugins fetch
fetched gobin b9d7c9afdd7177719d6d1fca8d65a4f4bca46137
```

It takes no flags of its own beyond `--color`. For each plugin that comes from git, in name order,
it runs your own `git` to fetch the one commit the tag names, at depth 1, drops the `.git` folder,
and works out one digest over the files. It reads no build file and asks no approver: fetching runs
no plugin code.

The commit shown is the one the tag named in this capture; yours will be another.

## `buildl.lock`

The fetch writes `buildl.lock` in the workspace root:

```console
$ cat buildl.lock
{"gobin":{"commit":"b9d7c9afdd7177719d6d1fca8d65a4f4bca46137","dir":"","git":"file:///tmp/buildl-book/p05/git/gobin.git","rev":"v0.1.0","sha256":"232d8abc6a17a467574ea0f57bd5b5cd76e6b05084afc76a02fc77a2b989cb4a"}}
```

| Field | Records |
|---|---|
| `git`, `rev` | what `buildl.toml` asked for when it was fetched |
| `dir` | the folder `buildl.toml` named, or `""` for the repository root |
| `commit` | the commit `rev` named then |
| `sha256` | the digest of the plugin's files: those of `dir`, when it is set |

Commit it with the workspace. Someone who clones the repository then fetches the very commit and
files you did, or is refused.

## Kept in the store

The files go under the state directory, one folder per plugin and commit:

```console
$ ls .buildl/plugins/gobin
b9d7c9afdd7177719d6d1fca8d65a4f4bca46137
$ ls .buildl/plugins/gobin/b9d7c9afdd7177719d6d1fca8d65a4f4bca46137
extension.toml
main.lua
```

Now the build loads it from there:

```console
$ buildl build
2 built
$ ./out/hello
hello from a plugin
```

A second fetch with nothing changed runs no `git` at all:

```console
$ buildl plugins fetch
unchanged gobin b9d7c9afdd7177719d6d1fca8d65a4f4bca46137
```

## Checked every time

Every command that reads build files first checks each git plugin against `buildl.lock` and the
store, and never against the network. Add a line to the stored `main.lua`, and the digest no
longer matches:

```console
$ buildl build
error: plugin gobin was changed after it was fetched: its files digest to 4f44f261367f027f0988af25deb94e3bc5db2ba2f3c46f0f4ca4f633e7a5ce06 and buildl.lock pins 232d8abc6a17a467574ea0f57bd5b5cd76e6b05084afc76a02fc77a2b989cb4a
$ buildl plugins fetch
fetched gobin b9d7c9afdd7177719d6d1fca8d65a4f4bca46137
$ buildl build
2 cached
```

The build exits with code 6: what is on disk is not what buildl wrote. Fetching again brought back
the files the lock pins.

A tag that moves is caught the same way. When the lock already pins a `git` and `rev`, and a fetch
of them brings another commit or other files, the fetch is refused rather than quietly pinning the
new ones. Moving a plugin on is a change you make: a new `rev` in `buildl.toml`, then a fetch.
