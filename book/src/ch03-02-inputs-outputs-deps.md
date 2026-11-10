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
never lists a path a `.buildlignore` leaves out ([below](#leaving-paths-out-buildlignore)), and
never follows a symbolic link. `*` stays inside one directory and `**/` crosses any number
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

## Leaving paths out: `.buildlignore`

Some folders hold files no target should read: notes, drafts, generated documentation. A file named
`.buildlignore` marks such paths as outside the build. It is written the way a `.gitignore` is, and
it may sit in any folder of the workspace: its patterns apply to that folder and everything below
it. Where a nested file and one above it disagree, the deeper file wins.

Here the workspace keeps its notes in `notes/`, and the root `.buildlignore` leaves them out. It
holds one line:

```text
notes/
```

```lua
-- build.lua
local b = buildl

b.target("bundle", {
  inputs = { b.sources("**/*.txt") },
  run    = { "cat", "$in" },
  stdout = true,
})
```

The workspace holds `src/a.txt`, `src/b.txt` and `notes/todo.txt`, and `buildl.toml` lets the build
run `cat`. The target reads the two files in `src/`. Editing a note changes nothing it reads, so
the target stays clean; editing a source does not:

```console
$ buildl build
--- //:bundle
a
b
1 built
$ echo more >> notes/todo.txt
$ buildl plan
clean  //:bundle
  run programs: cat
  write into:   .buildl, out
0 to run, 0 conditional, 1 clean
$ echo more >> src/a.txt
$ buildl plan
run    //:bundle: input src/a.txt changed
  run programs: cat
  write into:   .buildl, out
1 to run, 0 conditional, 0 clean
```

Three parts of buildl read the files, and all three give the same answer for a path:

| Where | What an ignored path does |
|---|---|
| `buildl.sources` | is not listed. An ignored folder is not entered, so it costs nothing against `[declaration] walk_entries` |
| `inputs` and `subdir` | is refused: a target cannot read a path the workspace has left out, and a build file cannot add an ignored folder. The exit code is 5, as [Exit Codes](appendix-04-exit-codes.md#5-definition) shows |
| [`buildl watch`](ch10-00-watch.md) | a change there starts no build |

### The syntax

The lines mean what they mean in a `.gitignore`:

| Line | Meaning |
|---|---|
| `# text` | a comment |
| an empty line | skipped |
| `notes/` | a folder named `notes`, at any depth, and everything in it; a trailing `/` matches folders only |
| `*.log` | a pattern with no `/` matches at any depth below the file |
| `/a.log` | `a.log` in the folder that holds this `.buildlignore`, and not deeper: a leading `/` anchors the pattern |
| `gen/*` | a `/` in the middle anchors the pattern the same way; `*` stays inside one folder name |
| `!keep.log` | puts back a path an earlier line left out; within a file the last matching line decides |

Spaces at the end of a line are dropped unless escaped with a backslash, and matching is
case-sensitive on every platform. A byte-order mark at the start of the file is dropped. `{` and
`}` are plain characters, as in git: `{a,b}` names a file called `{a,b}`, and is not two
alternatives as it would be in a pattern given to `buildl.sources`.

A path is decided from the workspace root down. buildl asks about each folder on the way to the
path, then about the path itself, and the first one a pattern ignores ends the question: nothing
below an ignored folder can be put back. With this root `.buildlignore`:

```text
notes/
!notes/keep/
```

`notes/keep/b.md` is ignored. `notes/` already left out the folder it sits in, and the
`!notes/keep/` line is never reached. To keep a file inside a folder, ignore the folder's contents
(`notes/*`) and not the folder.

Here is that root file in a workspace where `src/.buildlignore` holds `gen/*` and `!gen/keep.rs`,
beside `notes/a.md`, `notes/keep/b.md`, `src/main.rs`, `src/gen/g.rs` and `src/gen/keep.rs`. A
target whose inputs are `buildl.sources("**/*")` reads:

```console
$ buildl build
--- //:files
.buildlignore build.lua buildl.toml src/.buildlignore src/gen/keep.rs src/main.rs
1 built
```

Both files of `notes/` are gone, `src/gen/g.rs` is gone, and `src/gen/keep.rs` was put back by the
nested file. A `.buildlignore` is an ordinary file in every other way: it is listed, as above, and
a target that reads one is dirty when it changes. A pattern that ignores a `.buildlignore` hides it
from the listing, and the rules it holds still apply.

### What is and is not read

- Only a `.buildlignore` at or below the workspace root is read. One above the root never applies.
- No other ignore file is read: not `.gitignore`, not `.ignore`, not git's global file.
- `out/` and `.buildl/` stay out of the build whatever the patterns say, so a `!out/` line changes
  nothing.
- A pattern that names `build.lua` or `buildl.toml` does not stop buildl reading it. The
  rules apply only to `inputs`, `subdir`, `sources` and `watch`.
- A `.buildlignore` buildl cannot use is refused, never skipped: a pattern that does not parse, a
  file that is not UTF-8, a file that cannot be read, or one that is a symbolic link. The exit code
  is 5 and the message names the file, with the line where there is one.

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
