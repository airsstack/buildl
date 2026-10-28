# Build Settings

Sometimes you want to change one thing for one run: a different flag, a test filter, another
compiler. A build file cannot read the environment to find out, on purpose. Instead it declares a
**build setting**: a named value with a default, which you can override on the command line.

```lua
-- build.lua
local b = buildl

b.option("greeting", { default = "hello" })

b.target("message", {
  run    = { "echo", "$opt:greeting", "world" },
  stdout = true,
})
```

`buildl.option(name, { default = "…" })` declares the setting. `default` is required, and is the
only field. A command uses the value by writing `$opt:greeting`.

## Overriding a setting

`--set NAME=VALUE` gives a setting another value for one run. `plan`, `build`, `test`, `verify` and
`watch` all take it, and it may be repeated.

```console
$ buildl build
--- //:message
hello world
1 built
$ buildl plan --set greeting=goodbye
run    //:message: setting greeting changed
  run programs: echo
  write into:   .buildl, out
1 to run, 0 conditional, 0 clean
$ buildl build --set greeting=goodbye
--- //:message
goodbye world
1 built
```

`plan` names the reason: the setting changed. A setting's value is part of what decides whether a
target is up to date, but only for the targets whose commands name it. Change a setting no command
uses, and nothing runs again.

The value of every setting is fixed when the plan is made, so the run is the one that was planned.
Going back to the default is a change too:

```console
$ buildl plan
run    //:message: setting greeting changed
  run programs: echo
  write into:   .buildl, out
1 to run, 0 conditional, 0 clean
```

## Only declared settings

An override must name a setting some build file declared. A misspelt name is refused rather than
quietly ignored:

```console
$ buildl plan --set colour=blue
error: setting colour is overridden but not declared
```

That exits with code 2, the code for a mistake on the command line. Every setting and its default
appear in `buildl graph`, under `"settings"`:

```console
$ buildl graph
{"aliases":{},"settings":{"greeting":"hello"},"targets":{"//:message":{"deps":[],"description":null,"env":[],"freshness":"Cached","inputs":[],"network":"Sealed","origin":"BuildFile","output_source":"Stdout","outputs":["message"],"provenance":{"directory":"","file":"build.lua"},"role":"Build","run":["echo","$opt:greeting","world"]}}}
```

So everything that shapes a build is written down: in a build file, or on the command line that ran
it.

## A setting can choose the program

A setting may stand for the whole first argument:

```lua
b.option("compiler", { default = "cc" })
b.target("main.o", { inputs = { "main.c" }, run = { "$opt:compiler", "-c", "$in", "-o", "$out" } })
```

Once the value is put in, buildl treats the program exactly as if it were written out. Running
with `--set compiler=clang` means `clang` must be allowed by the workspace's ceiling
([`buildl.toml`, the Ceiling](ch04-02-the-ceiling.md)), must be found on `PATH`, and its contents
become part of what decides whether `main.o` is up to date.

## When to reach for one

A setting is for a choice made per run. If you find yourself setting the same value every time,
it belongs in the build file. And if you want to run part of a test suite, consider one test
target per package instead: then `buildl test //services/api:auth_test` runs just that part, and
`buildl query 'rdeps(…)'` can tell you which tests an edit touches.
