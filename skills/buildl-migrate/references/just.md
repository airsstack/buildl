# Migrating a justfile

The `buildl` skill holds the build-file and `buildl.toml` rules. This file only maps what a
justfile does to what buildl does.

## What `compare` reads

Check 1 of `compare` reads recipe names from the old build itself. For `tool: "just"` it reads
the file named exactly `justfile` in the project folder; with no such file, `compare` stops with
an error and prints no table. Its recipes are the recipe headers at the start of a line:

| Seen by check 1 | Example |
|---|---|
| `name:` | `build:` |
| a header with parameters or dependencies | `test arg:`, `ship: build` |
| a header with a leading `@` | `@lint:` gives `lint` |

| Not seen by check 1 | What to do |
|---|---|
| `set`, `alias`, `export`, `import` and `mod` lines | `alias` is a second name, so use `b.alias`; `import` and `mod` bring in recipes you read by hand |
| `name := value` assignments | they become Lua locals |
| recipes of an imported file or of a module | read that file yourself and map its recipes by hand |
| a file named `Justfile` or `.justfile` | `compare` stops with an error, except on a disk that ignores case (the macOS default), where `Justfile` is read as `justfile`. Tell the user the check reads only `justfile`; do not rename their file |
| a recipe named `set`, `alias`, `export`, `import` or `mod` | still migrate it |
| indented lines, the bodies of recipes | their commands go in the target's `run` |

## Mapping

| just | buildl |
|---|---|
| a recipe that builds a file | `b.target(file, { inputs, run })` |
| a recipe that runs tests (`test`, `check`, `lint`) | `b.test(name, { deps, run })` |
| `ship: build`, a dependency | `deps = { ":build_output" }` on the target |
| a recipe parameter `test arg:` | `b.option("arg", { default = "…" })`, read as `$opt:arg` and changed with `--set arg=value` |
| `version := "1"` | a Lua `local` |
| `export NAME := "value"` | the value goes in the command, through the shell the old build already uses: `{ "sh", "-c", "NAME=value program" }` with `sh` in `proc.run`. Add no other program for it |
| `alias b := build` | `b.alias("b", ":build_output")`. The graph holds no aliases: map the recipe `build` in `parity.json` to the target |
| `{{arg}}` | `$opt:arg` |
| a recipe that calls `just other/…` or a script in another folder | `b.subdir("other")` and a `build.lua` in that folder. Its targets are `//other:name` |
| a recipe that downloads | the same command with `network = true`, and the program in `proc.run` |
| an environment variable the recipe reads (`$HOME`, `env_var("HOME")`) | `env = { "HOME" }` on the target and `env.read = ["HOME"]` in the ceiling |
| `deploy`, `release`, `publish`, `tag`, `push`, `clean` | the stop rule, last section |

A just recipe body is a script. buildl runs a program with arguments and no shell. A body with
`&&`, `>`, `$VAR` or a pipe becomes `{ "sh", "-c", "…" }` with `sh` in `proc.run`, or two targets,
or `stdout = true` for a command whose output is the file. just runs its recipes through the
shell the justfile names (`sh` by default), so list that shell in `old.programs`: a ceiling entry
the old build does not use fails check 6. For the build command of a language, see the table in
the `buildl` skill.

A recipe that only runs `cargo build` has no named output file. Give the target the file the
command makes: for a Rust binary see the `buildl` skill's Rust row, which installs it into `$out`.

## Example

```just
set shell := ["sh", "-c"]

greeting := "hello"

build:
    echo {{greeting}} > greeting.txt

test: build
    grep -q hello greeting.txt

deploy:
    scp greeting.txt host:/srv/
```

```lua
local b = buildl

local greeting = "hello"

b.target("greeting.txt", {
  run = { "echo", greeting },
  stdout = true,
})

b.test("test", {
  deps = { ":greeting.txt" },
  run = { "grep", "-q", "hello", "$deps" },
})
```

```toml
[workspace]
name   = "greeting"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["echo", "grep"]
env.read = []
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

`compare` lists three recipes here: `build`, `test` and `deploy`. `set` and `greeting :=` are
not recipes. `deploy` is not a build step, so the stop rule applies, and the user chose to keep
it in the justfile:

```json
{
  "tool": "just",
  "old": {
    "build": ["just", "build"],
    "test": ["just", "test"],
    "programs": ["sh", "echo", "grep"],
    "env": [],
    "network": false
  },
  "recipes": {
    "build": { "target": "//:greeting.txt" },
    "test": { "target": "//:test" },
    "deploy": { "dropped": "deploys to a host; stays in the justfile" }
  },
  "outputs": [ { "old": "greeting.txt", "new": "out/greeting.txt" } ],
  "skip": []
}
```

A parameter becomes a build setting:

```lua
local b = buildl

b.option("profile", { default = "debug" })

b.target("profile.txt", {
  run = { "echo", "$opt:profile" },
  stdout = true,
})
```

## Folders

```lua no-check
-- needs tools/build.lua beside it, which the checked blocks do not have
local b = buildl
b.subdir("tools")
```

## Recipes that are not build steps

A recipe that deploys, tags, pushes, or otherwise acts outside building files is neither dropped
silently nor forced into a target. List them, ask the user whether each stays in the justfile or
is dropped, and record the answer as `dropped` with its reason, so check 1 adds up. `clean` is in
this group: the usual answer is to drop it, because `buildl clean` removes the output directory.

## C and C++

If a recipe compiles each `.c` or `.cc` to its own `.o` and links them, the per-file stop rule
applies. See `make.md`, "C and C++ compiled one file at a time": one whole-program target, its
output judged by the tests.
