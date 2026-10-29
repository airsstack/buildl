# Migrating shell build scripts

The `buildl` skill holds the build-file and `buildl.toml` rules. This file only maps what a
build script does to what buildl does.

## What `compare` reads

Check 1 of `compare` reads recipe names from the old build itself. For `tool: "shell"` a recipe
is a script: every file whose name ends in `.sh`, in the project folder or any folder below it,
that is not under a path in `skip`. In `parity.json` its key is its path relative to the
project, such as `build.sh` or `tools/gen.sh`.

| Seen by check 1 | Example |
|---|---|
| `build.sh` | the key is `build.sh` |
| `tools/gen.sh`, in any folder | the key is `tools/gen.sh` |

| Not seen by check 1 | What to do |
|---|---|
| a script without the `.sh` ending (`build`, `configure`, `run-tests`) | read it from `old.build` and map its work; it needs no key |
| a script under a path in `skip` | it is left out of the copies too, so it cannot run there |
| the commands inside a script | map them by hand, below |

A script that only another script sources or calls still has a key. Map it to the target that
replaces its work, or drop it with a reason such as `"dropped": "sourced helper; its commands
are inlined"`.

## Mapping

A script is a list of commands, and a build file is a list of targets. Split the script by the
files it makes.

| Shell | buildl |
|---|---|
| a command that makes a file | `b.target(file, { inputs, run })`, with `$in` and `$out` for the files |
| a command whose output is the file (`cmd > file`) | `stdout = true`, and no `>` |
| a command that checks something (`grep -q`, `test -f`, running a test binary) | `b.test(name, { deps, run })` |
| a command that uses another command's file | `deps = { ":other" }` on the target, read as `$deps` |
| `for f in src/*.c; do …; done` | a Lua loop over `b.sources("src/*.c")` declaring a target for each file |
| a repeated command shape | `b.rule(name, { run })` once, then targets with `rule = ":name"` |
| `sh other/script.sh`, or `cd other && ./build.sh` | `b.subdir("other")` and a `build.lua` in that folder. Its targets are `//other:name` |
| `curl`, `wget`, `git clone`, `npm install` | the same command with `network = true`, and the program in `proc.run` |
| `$TOKEN`, `${HOME}` | `env = { "TOKEN" }` on the target and `env.read = ["TOKEN"]` in the ceiling |
| `mkdir -p out`, `cd`, `set -e` | nothing: buildl places `$out` and runs from the workspace root, and a failed command stops the target |
| `export NAME=value` | the value goes in the command: `{ "sh", "-c", "NAME=value program" }` with `sh` in `proc.run`. An export of a variable the script reads (`export PATH=$HOME/bin:$PATH`) is an `env` entry, as in the `$TOKEN` row |
| `deploy`, `scp`, `git push`, `docker push`, `git tag` | the stop rule, last section |

A command with `&&`, `|`, `>` or `$VAR` in one line becomes `{ "sh", "-c", "…" }` with `sh` in
`proc.run`, or two targets. List `sh` in `old.programs` too: the scripts ran through it, and a
ceiling entry the old build does not use fails check 6. For the build command of a language, see
the table in the `buildl` skill.

## Example: a script that calls a script in another folder

```sh
#!/bin/sh
# build.sh
set -e
sh tools/upper.sh > upper.txt
sort upper.txt > sorted.txt
```

```sh
#!/bin/sh
# tools/upper.sh
sed 's/a/A/g' tools/names.txt
```

The folder `tools/` gets a build file of its own, and `tools/names.txt` is read from there.

```lua
local b = buildl

b.target("upper.txt", {
  inputs = { "names.txt" },
  run = { "sed", "s/a/A/g", "$in" },
  stdout = true,
})
```

The root names the folder and builds on its target. It is a fragment of a workspace, because it
needs `tools/build.lua`, so it is not checked by itself.

```lua no-check
-- needs tools/build.lua beside it, which the checked blocks do not have
local b = buildl

b.subdir("tools")

b.target("sorted.txt", {
  deps = { "//tools:upper.txt" },
  run = { "sort", "$deps" },
  stdout = true,
})
```

```toml
[workspace]
name   = "scripts"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["sed", "sort"]
env.read = []
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

`compare` lists two scripts here, `build.sh` and `tools/upper.sh`. A buildl output sits under
`out/`, and a folder's output under `out/<folder>/`:

```json
{
  "tool": "shell",
  "old": {
    "build": ["sh", "build.sh"],
    "programs": ["sh", "sed", "sort"],
    "env": [],
    "network": false
  },
  "recipes": {
    "build.sh": { "target": "//:sorted.txt" },
    "tools/upper.sh": { "target": "//tools:upper.txt" }
  },
  "outputs": [ { "old": "sorted.txt", "new": "out/sorted.txt" } ],
  "skip": ["upper.txt"]
}
```

A script that reads a variable:

```sh
echo "$GREETING" > greeting.txt
```

```lua
local b = buildl

b.target("greeting.txt", {
  run = { "sh", "-c", 'echo "$GREETING"' },
  env = { "GREETING" },
  stdout = true,
})
```

with `env.read = ["GREETING"]` in the ceiling and `"env": ["GREETING"]` in `old`.

## Recipes that are not build steps

A script that deploys, tags, pushes, or otherwise acts outside building files is neither
dropped silently nor forced into a target. List them, ask the user whether each stays where it
is or is dropped, and record the answer as `dropped` with its reason, so check 1 adds up.

## C and C++

If a script compiles each `.c` or `.cc` to its own `.o` and links them, the per-file stop rule
applies. See `make.md`, "C and C++ compiled one file at a time": one whole-program target, its
output judged by the tests.
