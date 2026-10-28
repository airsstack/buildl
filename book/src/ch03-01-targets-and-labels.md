# Targets and Labels

This page builds a two-directory workspace that joins two text files, and uses it to show the four
things a build file declares and how they name each other.

```text
hello/
  buildl.toml
  build.lua
  hello.txt        "hello"
  lib/
    build.lua
    name.txt       "world"
```

```lua
-- lib/build.lua
local b = buildl

b.target("name", {
  inputs = { "name.txt" },
  run    = { "cp", "$in", "$out" },
})
```

```lua
-- build.lua
local b = buildl

b.subdir("lib")

b.rule("join", {
  run  = { "cat", "$in", "$deps" },
  desc = "join $in and $deps",
})

b.target("greeting", {
  rule   = "join",
  inputs = { "hello.txt" },
  deps   = { "//lib:name" },
  stdout = true,
})

b.alias("default", "greeting")

b.test("greeting_test", {
  deps = { "greeting" },
  run  = { "grep", "-q", "world", "$deps" },
})
```

`buildl.toml` lets the build run `cat`, `cp` and `grep`;
[`buildl.toml`, the Ceiling](ch04-02-the-ceiling.md) explains why it has to.

## The four declarations

| Call | Declares |
|---|---|
| `buildl.target(name, { … })` | something to build: a command, what it reads, what it writes |
| `buildl.test(name, { … })` | a target that `buildl test` runs; it passes when its command exits 0 |
| `buildl.rule(name, { run, desc })` | a command shared by many targets, which name it with `rule = "…"` |
| `buildl.alias(name, target)` | a second name for a target |

A target and a test take the same table. Every field is optional:

| Field | Meaning |
|---|---|
| `run` | the command, a list of arguments; or `rule`, the name of a rule whose `run` is used |
| `inputs` | the files the command reads, relative to the build file's directory |
| `deps` | the targets whose outputs the command reads, by label |
| `outputs` | what the command writes; a target defaults to its own name, a test to nothing |
| `env` | the environment variables the command may see |
| `network` | `true` says the command reaches the network; `plan` and the approver show it |
| `stdout` | `true` makes what the command prints the target's one output (not for tests) |
| `always` | `true` runs the command every time, never from the cache |

A rule takes `run` and `desc`. `desc` is the line a terminal shows for the action while it runs, so
`join hello.txt and out/lib/name` here.

The table is closed. A field buildl does not know is an error that names the line, so a misspelling
cannot quietly drop an input:

```console
$ buildl check
error: build.lua: unknown field "srcs": line 10: buildl.target 'greeting': unknown field 'srcs'
```

`buildl.discover`, `buildl.option`, `buildl.subdir` and `buildl.use` are declarations too. They
have their own pages: [Discovery Targets](ch07-00-discovery-targets.md),
[Build Settings](ch03-05-build-settings.md), [Many Directories](ch03-04-many-directories.md) and
[Plugins](ch08-00-plugins.md).

## Labels

Every target has a label: `//`, the directory of its build file, `:`, and its name. The target
`name` in `lib/build.lua` is `//lib:name`. The root directory is empty, so `greeting` is
`//:greeting`.

Inside one build file you can write a label two shorter ways, both read against the build file's
own directory:

| Written | In `lib/build.lua` means | In `build.lua` means |
|---|---|---|
| `//lib:name` | `//lib:name` | `//lib:name` |
| `:name` | `//lib:name` | `//:name` |
| `name` | `//lib:name` | `//:name` |

That is why the test above can write `deps = { "greeting" }`. On the command line, `plan`, `build`,
`test` and `query` take the same three forms.

A label always names one target. `//lib` names a directory, and buildl will not guess that you
meant `//lib:lib`:

```console
$ buildl check
error: build.lua: declaration #2: invalid dep: invalid label: "//lib" — must hold ':' and a target name
```

The command exits with code 5, the code for a mistake in a build file.

## Looking at the graph

The targets and their `deps` form a graph. `buildl graph` prints it. Its default is canonical JSON,
the same bytes buildl keeps in `.buildl/graph.json`; `--dot` prints it for Graphviz instead:

```console
$ buildl graph --dot
digraph buildl {
  "//:greeting";
  "//:greeting_test";
  "//lib:name";
  "//:greeting" -> "//lib:name";
  "//:greeting_test" -> "//:greeting";
  "//:default" [shape=plaintext];
  "//:default" -> "//:greeting" [style=dashed];
}
```

The alias is the dashed line: `//:default` is not a target of its own, only another way to say
`//:greeting`.

`buildl query` answers one question about the graph. `deps(…)` lists what a target needs, all the
way down, and `rdeps(…)` lists everything that needs it:

```console
$ buildl query 'deps(greeting)'
//lib:name
$ buildl query 'rdeps(//lib:name)'
//:greeting
//:greeting_test
```

`rdeps` is the one to reach for before an edit: it lists every target the edit can affect.
Neither command runs anything, and neither does `check`. `build` and `test` do:

```console
$ buildl build
--- //:greeting
hello
world
2 built
$ buildl test
--- //:greeting (cached)
hello
world
tests: 1 passed
```

`buildl build` with no targets builds everything but tests, and `buildl test` runs every test,
building what they need first. Under `---` and a label, buildl shows what that command printed. For
a `stdout = true` target that is its output, which is why `hello` and `world` appear. The next page looks at what `$in`, `$out` and `$deps` turned into.
