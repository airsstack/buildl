# Migrating a Makefile

The `buildl` skill holds the build-file and `buildl.toml` rules. This file only maps what a
Makefile does to what buildl does.

## What `compare` reads

Check 1 of `compare` reads recipe names from the old build itself. For `tool: "make"` it reads
the first of `GNUmakefile`, `makefile`, `Makefile` that exists in the project folder, and only
that one. Its recipes are:

| Seen by check 1 | Example |
|---|---|
| a target at the start of a line, one name for each word before the colon | `app:`, and `a b: c` gives `a` and `b` |
| a pattern rule, with its `%` | `%.o: %.c` gives `%.o` |
| a phony target, such as `all`, `test`, `clean` | `test: app` |
| a target written as a variable, with a recipe | `$(BIN): $(OBJS)` gives `$(BIN)`, unexpanded. Map it to the target that builds the file |

| Not seen by check 1 | What to do |
|---|---|
| `.PHONY` and every other name starting with `.`, such as the old suffix rule `.c.o:` | nothing to map; still migrate what they do |
| variable assignments: `=`, `:=`, `::=`, `?=`, `+=` | they become Lua locals |
| a line starting with a space, a tab or `#` | |
| rules in a file brought in by `include` | read that file yourself and map its rules by hand |
| the Makefile of a folder called by `$(MAKE) -C dir` | map it with `subdir`, below |

Two quirks of that reader make a name appear that is not a recipe. Map it to a target or drop it
with a reason:

| Quirk | Listed as | Drop it with |
|---|---|---|
| a header line without a recipe, such as `$(OBJS): x.h` | the recipe `$(OBJS)` | `"dropped": "header dependency line, not a recipe"` |
| a line inside `define … endef` that holds a colon in column 0 | a rule, named by the words before the colon | `"dropped": "text inside define, not a rule"` |

## Mapping

| Make | buildl |
|---|---|
| a rule that builds a file | `b.target(file, { inputs, run })` |
| a phony recipe that runs the tests (`test`, `check`) | `b.test(name, { deps, run })` |
| `all:` listing other targets | `b.alias("all", ":x")`. The graph holds no aliases: map the recipe `all` in `parity.json` to the target the alias names |
| a pattern rule `%.txt: %.in` | `b.rule(name, { run })` once, then a Lua loop over `b.sources` declaring a target for each file |
| `$@` | `$out` |
| `$<`, `$^` | `$in`, and `$deps` for the outputs of other targets |
| `$(wildcard src/*.c)` | `b.sources("src/*.c")` |
| `$(VAR)` | a Lua `local` |
| `$(MAKE) -C dir`, or a Makefile in another folder | `b.subdir("dir")` and a `build.lua` in that folder. Its targets are `//dir:name` in `deps` and in `parity.json` |
| a recipe that downloads (`curl`, `wget`, `git clone`) | the same command with `network = true`, and the program in `proc.run` |
| an environment variable the recipe reads (`$$TOKEN`) | `env = { "TOKEN" }` on the target and `env.read = ["TOKEN"]` in the ceiling |
| `clean`, `deploy`, `release`, `tag`, `push` | the stop rule, last section |

A recipe line is not a shell script. buildl runs a program with arguments and no shell. A line
with `&&`, `>`, `$VAR` or a pipe becomes `{ "sh", "-c", "…" }` with `sh` in `proc.run`, or two
targets, or `stdout = true` for a command whose output is the file. `make` runs its recipes
through `sh`, so list `sh` in `old.programs` as well: a ceiling entry the old build does not use
fails check 6. For the build command of a language, see the table in the `buildl` skill.

## Example: fetch, derive, test

```make
.PHONY: all test clean

all: report.txt

data.json:
	curl -fsSL -H "Authorization: $$TOKEN" https://example.com/data.json -o $@

report.txt: data.json
	grep id data.json > $@

test: report.txt
	grep -q . report.txt

clean:
	rm -f data.json report.txt
```

```lua
local b = buildl

b.target("data.json", {
  run = { "sh", "-c", 'curl -fsSL -H "Authorization: $TOKEN" https://example.com/data.json -o "$0"', "$out" },
  env = { "TOKEN" },
  network = true,
})

b.target("report.txt", {
  deps = { ":data.json" },
  run = { "grep", "id", "$deps" },
  stdout = true,
})

b.test("test", {
  deps = { ":report.txt" },
  run = { "grep", "-q", ".", "$deps" },
})

b.alias("all", ":report.txt")
```

```toml
[workspace]
name   = "report"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["sh", "curl", "grep"]
env.read = ["TOKEN"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

The matching `parity.json`. Every name `compare` lists is in `recipes`: `all`, `clean`,
`data.json`, `report.txt` and `test`. A buildl output sits under `out/`. `rm` is in
`old.programs` and not in the ceiling, which is allowed: check 6 fails only for a ceiling entry
the old build does not use.

```json
{
  "tool": "make",
  "old": {
    "build": ["make", "report.txt"],
    "test": ["make", "test"],
    "programs": ["sh", "curl", "grep", "rm"],
    "env": ["TOKEN"],
    "network": true
  },
  "recipes": {
    "all": { "target": "//:report.txt" },
    "clean": { "dropped": "buildl clean removes the outputs" },
    "data.json": { "target": "//:data.json" },
    "report.txt": { "target": "//:report.txt" },
    "test": { "target": "//:test" }
  },
  "outputs": [ { "old": "report.txt", "new": "out/report.txt" } ],
  "skip": ["data.json"]
}
```

A pattern rule, as a `rule` and a loop:

```make
%.txt: %.in
	sed 's/foo/bar/' $< > $@
```

```lua
local b = buildl

b.rule("swap", { run = { "sed", "s/foo/bar/", "$in" }, desc = "swapping $in" })

for _, src in ipairs(b.sources("*.in")) do
  local name = (src:gsub("%.in$", ".txt"))
  b.target(name, { rule = ":swap", inputs = { src }, stdout = true })
end
```

In `parity.json`, `%.txt` maps to one target the loop declares, or is dropped with a reason that
names the rule.

## Folders

```make
all:
	$(MAKE) -C lib
```

The root `build.lua` names the folder, and `lib/build.lua` declares what `lib/Makefile` built.

```lua no-check
-- needs lib/build.lua beside it, which the checked blocks do not have
local b = buildl
b.subdir("lib")
```

## C and C++ compiled one file at a time

When the Makefile compiles each `.c` or `.cc` to its own `.o` and links them (`%.o: %.c`), write
no per-`.o` targets. buildl v1 does not support them: a compiler reads headers nobody declared,
so a per-file target could not be keyed on them.

Offer one target that compiles the whole program in one command, with every source and every
header declared as an input. Say what it costs before the user answers:

- any edit rebuilds the whole program
- the binary differs byte for byte from the per-file build's, so its parity output is judged by
  the tests, and a project with no test command cannot pass parity this way

If the user declines, stop with nothing changed.

```lua
local b = buildl

local sources = b.sources("src/*.c")
local headers = b.sources("src/*.h")

b.target("app", {
  inputs = { sources, headers },
  run = { "cc", "-o", "$out", sources },
})

b.test("self_test", {
  deps = { ":app" },
  run = { "$deps", "--self-test" },
})
```

The sources are named in `run`, not passed as `$in`: `$in` would hand the headers to the
compiler too, and a compiler given a header and `-o` refuses. The headers are still inputs, so
they are in the sandbox and in the action's key.

This block is for the build file in the project's root folder. `sources` names start at the
folder of the build file, but a path in `run` starts at the workspace root. In a build file in a
folder, such as `src/build.lua`, put the folder in front of each name in `run` (`"src/" .. f`).

On acceptance, mark the output in `parity.json`. `old.test` is required, and check 5 then needs
both test runs to pass:

```json
"outputs": [
  { "old": "app", "new": "out/app", "judge": "tests", "reason": "whole-program build differs from the per-file link byte for byte" }
]
```

Check 1 lists the `%.o: %.c` recipe, and `$(OBJS)` or `$(BIN)` lines if the Makefile has them.
Map the recipe that builds the program to the new target. Drop the per-file ones:
`"dropped": "per-file objects: one whole-program target replaces them"`.

List the `.o` files an earlier run left in `skip`, one path each (`"src/a.o"`, `"src/b.o"`), or
the folder that holds them. `skip` takes paths, never a pattern: `*.o` skips nothing.

## Recipes that are not build steps

A recipe that deploys, tags, pushes, or otherwise acts outside building files is neither dropped
silently nor forced into a target. List them, ask the user whether each stays in the Makefile or
is dropped, and record the answer as `dropped` with its reason, so check 1 adds up. `clean` is in
this group: the usual answer is to drop it, because `buildl clean` removes the output directory.
