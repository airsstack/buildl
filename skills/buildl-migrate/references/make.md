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

When the Makefile compiles each `.c` or `.cc` to its own `.o` and links them (`%.o: %.c`), move it
to the `c` plugin. It compiles each source to its own object and finds the headers each one reads,
so a change to one source or header rebuilds only the objects that read it. Write no per-`.o`
targets by hand.

| In the Makefile | In buildl |
|---|---|
| the `SRCS` or `OBJS` list | `srcs`, the `.c`, `.cc`, `.cpp` or `.cxx` files; the `.o` names go |
| the header list, or `-MMD` and `-include *.d` | `headers`, the headers of the project; the plugin finds which source reads which |
| `CFLAGS`, `CPPFLAGS` | `cflags` (and `cxxflags` for C++ sources), with `c.includes` for `-I` folders and `c.defines` for `-D` |
| `LDFLAGS` | `ldflags` |
| `LDLIBS`, `-lm` | `ldlibs` |
| `%.o: %.c` | one object target of the call, `<name>.<source path, with / as .>.o` |
| `$(BIN): $(OBJS)` | the call's own target, `<name>` |
| `ar rcs lib.a $(OBJS)` | `c.library`; needs `sh` |
| a `-shared` link | `c.shared_library` |
| a test binary run by `make test` | `c.test` |

Add the `[plugins.c]` table to `buildl.toml` as the `buildl` skill shows, and run
`buildl plugins fetch`. The ceiling and the table both allow `PATH`, the compiler the old build
runs (`cc`, `c++`, `gcc`, `g++`, `clang` or `clang++`; a `CC = gcc` or GNU make's built-in
`CXX = g++` means `gcc` or `g++`, not `cc`) and `sh` for `c.library` or a `c.test` with `deps`. Set
the call's `cc` (and `cxx` for C++ sources) to that compiler, so the ceiling holds nothing the old
build did not run, which check 6 refuses. `make` runs through `sh`, so `sh` is in `old.programs`
anyway.

```lua no-check
-- needs the c plugin fetched: buildl.use("c") fails without it
local c = buildl.use("c")

c.binary("app", {
  srcs    = { "src/main.c", "src/util.c" },
  headers = { "src/util.h" },
  cflags  = c.includes({ "src" }),
})

c.test("self_test", {
  srcs    = { "src/self_test.c", "src/util.c" },
  headers = { "src/util.h" },
  cflags  = c.includes({ "src" }),
})
```

The sources and headers are named from the folder of the build file; paths inside `cflags` start
at the workspace root. Keep all of the project's C and C++ in one build file: a header outside the
build file's folder cannot be declared. A header from a folder outside the workspace is refused
unless that folder comes in through `-isystem` in `cflags`.

Say what happens to the output before the user answers, because the parity table depends on it:

- with `-g` in `CFLAGS`, the compiler records the folder it ran in, and buildl compiles in another
  one, so the binary differs from the old build's byte for byte. Mark it judged by the tests.
  `old.test` is required, and check 5 then needs both test runs to pass. A project with no test
  command cannot pass parity this way
- without `-g` the bytes match, and the output is compared as any other

If the Makefile uses `-g`, mark the output in `parity.json`:

```json
"outputs": [
  { "old": "app", "new": "out/app", "judge": "tests", "reason": "-g records the compile folder, which differs in buildl" }
]
```

Without `-g`, give the same pair with no `judge`:

```json
"outputs": [
  { "old": "app", "new": "out/app" }
]
```

Check 1 lists the `%.o: %.c` recipe, and `$(OBJS)` or `$(BIN)` lines if the Makefile has them. Map
the recipe that builds the program to the call's target (`//:app`) and the `%.o` recipe to one of
its objects (`//:app.src.main.c.o`).

List the `.o` files an earlier run left in `skip`, one path each (`"src/a.o"`, `"src/b.o"`), or
the folder that holds them. `skip` takes paths, never a pattern: `*.o` skips nothing.

## Recipes that are not build steps

A recipe that deploys, tags, pushes, or otherwise acts outside building files is neither dropped
silently nor forced into a target. List them, ask the user whether each stays in the Makefile or
is dropped, and record the answer as `dropped` with its reason, so check 1 adds up. `clean` is in
this group: the usual answer is to drop it, because `buildl clean` removes the output directory.
