# Inputs, Outputs and Dependencies

A build file never handles a real path to a built file. It writes placeholders in its commands, and
buildl puts the paths in just before the command runs. This page shows what each placeholder turns
into, using a workspace whose commands only `echo` and `touch`:

```lua
-- build.lua
local b = buildl

b.target("pair", {
  outputs = { "left.txt", "right.txt" },
  run     = { "touch", "$out" },
})

b.target("show", {
  inputs = { b.sources("notes/*.txt"), "README" },
  deps   = { "pair" },
  run    = { "echo", "in:", "$in", "deps:", "$deps", "joined: $in" },
  stdout = true,
})
```

```console
$ buildl build
--- //:show
in: notes/a.txt notes/b.txt README deps: out/left.txt out/right.txt joined: notes/a.txt notes/b.txt README
2 built
```

## The three placeholders

| Placeholder | Becomes |
|---|---|
| `$in` | the target's `inputs` |
| `$out` | the target's own `outputs`, under `out/` |
| `$deps` | every output of every target in `deps` |

Each one is a list of paths, and the paths are read from the workspace root. An argument that is
exactly one placeholder becomes one argument per path, or no argument when the list is empty. So
`{ "touch", "$out" }` above ran `touch out/left.txt out/right.txt`, two arguments. Inside longer text
the paths are joined by one space instead, as `"joined: $in"` shows: that was a single argument.

Lists keep the order you wrote them in, with repeats dropped. `$in` lists the two notes, then
`README`, because that is the order of `inputs`.

## Inputs, and `buildl.sources`

An input is a file in your workspace, written relative to the build file's directory. buildl reads
it, hashes it, and rebuilds the target when its bytes change.

A build file cannot look at the disk to find its inputs. `buildl.sources(pattern)` does that for it:
buildl walks the build file's directory, and returns the regular files that match, relative to that
directory and sorted, so every machine gets the same list. It never enters `out/` or `.buildl/`,
and it never follows a symbolic link. `*` stays inside one directory and `**/` crosses any number
of them.

A list field takes one level of nesting, spliced in where it stands. That is why
`{ b.sources("notes/*.txt"), "README" }` above is one flat list of three files.

An input stays inside its build file's directory. A build file in `lib/` cannot reach up with `..`
to read a file that belongs to the directory above:

```lua
-- lib/build.lua
local b = buildl

b.target("readme", {
  inputs = { "../README" },
  run    = { "echo", "$in" },
  stdout = true,
})
```

```console
$ buildl check
error: lib/build.lua: declaration #0: invalid input: invalid source path: "lib/../README" — must not contain a '.' or '..' segment
```

If a target needs a file from another directory, the build file there declares a target for it,
and this one lists that target in `deps`. [Many Directories](ch03-04-many-directories.md) shows how.

## Outputs, and `out/`

Every output lands at `out/<directory>/<name>`: the directory of the target's label, then the
output's name. `//:show` writes `out/show`, and a target `//lib:text` would write `out/lib/text`.
An output may be one file or a whole directory of files.

```console
$ ls -R out
left.txt
right.txt
show
```

`outputs` defaults to the target's own name, which is why `show` needs no `outputs` line. When the
command starts, the directory each output goes into exists and the output itself does not: the
command creates it. Two targets may not claim the same path; buildl refuses that before anything
runs.

`out/` is buildl's to write. Each file in it is read-only. A file that may be run is a copy; any
other is a link into buildl's store under `.buildl/` where linking is possible. Delete `out/` whenever you like: `buildl clean` does exactly
that, and the next build puts it back from the store without running anything:

```console
$ buildl clean
clean: removed out
$ buildl build
--- //:show (cached)
in: notes/a.txt notes/b.txt README deps: out/left.txt out/right.txt joined: notes/a.txt notes/b.txt README
2 cached
```

## Dependencies

`deps` names targets, and does two things at once:

- it orders the build: `pair` runs before `show`;
- it hands over files: `$deps` in `show` became the outputs of `pair`.

A target can only read another target's outputs through `deps`. Under the default sandbox, a
command sees copies of its declared inputs and its dependencies' outputs and nothing else from the
workspace, so a forgotten dependency fails the build instead of working by luck.
[The Sandbox](ch06-00-sandbox.md) explains how.

What a dependency produced, its outputs' bytes, is part of what decides whether a target is up to
date. [Caching](ch05-00-caching.md) takes that up.
