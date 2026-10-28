# Hello, buildl

A `buildl` workspace is a folder with two files at its top: `buildl.toml`, which says what builds
in this workspace may do, and `build.lua`, which declares what to build. Make a new folder and save
these three files in it.

The first is the input, a short list of names:

```text
zoe
ada
mia
```

Save it as `names.in`. The target will sort it with `sort`, a program every Unix machine has.

Next, `buildl.toml`:

```toml
[workspace]
name = "hello"

[ceiling]
proc.run = ["sort"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

The `[ceiling]` table is the most any build in this workspace may do. This one is as small as it
can be for our target, and every line is needed:

- `proc.run` lists the programs a build may run. Here that is `sort` and nothing else.
- `fs.read` lists the folders a build may read, relative to the workspace. `"."` is the whole
  workspace, which is where `names.in` lives.
- `fs.write` lists the folders a build may write. `out` is where finished outputs go, and `.buildl`
  is where `buildl` keeps its cache.

Leave any of them out and `buildl build` stops before running anything, naming what was missing.
[Asking Permission](ch04-00-asking-permission.md) covers the ceiling in full.

Last, `build.lua`:

```lua
local b = buildl

b.target("names.txt", {
  inputs = { "names.in" },
  run = { "sort", "-o", "$out", "$in" },
})
```

`buildl` is a table of functions that every build file can use, and the first line gives it a
shorter name. `b.target` declares one target, named `names.txt`. Its `inputs` are the files it
reads, and `run` is the command that makes it, as a list of arguments.

`$in` and `$out` are placeholders. Your build file never sees a real path: `buildl` puts the
paths in when it runs the command. `$in` becomes the target's inputs and `$out` becomes the file
it must make, which by default is named after the target.

## Check it

`buildl check` reads every build file and runs nothing. It is the quickest way to find a mistake
in a build file:

```console
$ buildl check
ok: 1 declaration in 1 build file
```

## Build it

```console
$ buildl build
1 built
```

The result is under `out/`, named after the target:

```console
$ ls out
names.txt
$ cat out/names.txt
ada
mia
zoe
```

`out/names.txt` is read-only. `buildl` keeps the bytes in its own store, under `.buildl`, and
the file in `out/` is a link to them. To change the output, change the input.

## Build it again

```console
$ buildl build
1 cached
```

Nothing ran. `buildl` saw that `names.in`, the command and everything else the target depends on
are the same as last time, so it already has the answer. The next page asks `buildl` how it knew.
