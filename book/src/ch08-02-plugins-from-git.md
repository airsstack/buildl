# Plugins from Git

A plugin you share between repositories lives in a git repository of its own. The workspace names
the repository and a version of it, `buildl plugins fetch` downloads it once, and from then on no
command reaches the network to load it.

## Naming the repository

Instead of `path`, the plugin's table in `buildl.toml` gives `git` and `rev`:

```toml
[plugins.gobin]
git      = "file:///tmp/buildl-book/p05/git/gobin.git"
rev      = "v0.1.0"
proc.run = ["go"]
env.read = ["HOME"]
```

| Key | Says |
|---|---|
| `git` | any URL `git` can clone: `https://`, `ssh://`, or `file://` as here |
| `github` | instead of `git`: `owner/repo`, short for the GitHub URL |
| `rev` | a tag or a full commit |

The cap, `proc.run` and `env.read`, works as it does for a plugin kept in the workspace. For this
page the repository is a bare one made on the same machine, holding the `gobin` plugin from
[Writing a Plugin](ch08-01-writing-a-plugin.md) under the tag `v0.1.0`, so nothing here needs the
network.

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
fetched gobin c6a408f28f821a67d8393db4018f2178bb813f6c
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
{"gobin":{"commit":"c6a408f28f821a67d8393db4018f2178bb813f6c","git":"file:///tmp/buildl-book/p05/git/gobin.git","rev":"v0.1.0","sha256":"e49c02079e363a98f1fb3001eaff256db2a59c48c02d5205e5e18bf0990166f4"}}
```

| Field | Records |
|---|---|
| `git`, `rev` | what `buildl.toml` asked for when it was fetched |
| `commit` | the commit `rev` named then |
| `sha256` | the digest of the plugin's files |

Commit it with the workspace. Someone who clones the repository then fetches the very commit and
files you did, or is refused.

## Kept in the store

The files go under the state directory, one folder per plugin and commit:

```console
$ ls .buildl/plugins/gobin
c6a408f28f821a67d8393db4018f2178bb813f6c
$ ls .buildl/plugins/gobin/c6a408f28f821a67d8393db4018f2178bb813f6c
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
unchanged gobin c6a408f28f821a67d8393db4018f2178bb813f6c
```

## Checked every time

Every command that reads build files first checks each git plugin against `buildl.lock` and the
store, and never against the network. Add a line to the stored `main.lua`, and the digest no
longer matches:

```console
$ buildl build
error: plugin gobin was changed after it was fetched: its files digest to 918aa0f1e0deb3f25eab81f9fd230ce2ff883bec7a73ddf2c571fb30b5401c76 and buildl.lock pins e49c02079e363a98f1fb3001eaff256db2a59c48c02d5205e5e18bf0990166f4
$ buildl plugins fetch
fetched gobin c6a408f28f821a67d8393db4018f2178bb813f6c
$ buildl build
2 cached
```

The build exits with code 6: what is on disk is not what buildl wrote. Fetching again brought back
the files the lock pins.

A tag that moves is caught the same way. When the lock already pins a `git` and `rev`, and a fetch
of them brings another commit or other files, the fetch is refused rather than quietly pinning the
new ones. Moving a plugin on is a change you make: a new `rev` in `buildl.toml`, then a fetch.
