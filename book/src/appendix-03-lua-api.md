# C. Build File API

A build file reaches `buildl` through one table, bound to two names: the global `buildl`, and
`airsstack.buildl` beside the other `airsstack` modules (`json`, `path`, `regex`, `hash` and
`glob`). The book's examples start with `local b = buildl`. The table holds twelve functions:

| Function | Returns | Declares |
|---|---|---|
| [`target(name, options)`](#targetname-options) | nothing | a target that builds files |
| [`test(name, options)`](#testname-options) | nothing | a test target |
| [`discover(name, options)`](#discovername-options) | nothing | a discovery target |
| [`rule(name, options)`](#rulename-options) | nothing | a command several targets share |
| [`alias(name, target)`](#aliasname-target) | nothing | a second name for a target |
| [`option(name, options)`](#optionname-options) | nothing | a build setting |
| [`subdir(path)`](#subdirpath) | nothing | another directory with a build file |
| [`sources(pattern)`](#sourcespattern) | a list of file names | |
| [`folder()`](#folder) | a string | |
| [`discovered(label)`](#discoveredlabel) | a string, or `nil` | |
| [`decode_all(text)`](#decode_alltext) | a list of values | |
| [`use(name)`](#usename) | the plugin's module | |

The names come from `crates/buildl-lua/src/module.rs`, where the table is filled. This build file
asks the table for its own keys and shows them in an error, since a build file cannot print:

```lua
local names = {}
for k in pairs(buildl) do names[#names + 1] = k end
table.sort(names)
error("buildl holds: " .. table.concat(names, " "), 0)
```

```console
$ buildl check
error: build.lua: runtime error: lua error in build.lua: runtime error: buildl holds: alias decode_all discover discovered folder option rule sources subdir target test use
stack traceback:
	[C]: in ?
	[C]: in function 'error'
	build.lua:4: in main chunk
```

## How arguments are read

The rules every function shares:

- An options table accepts only the keys listed for its function. Any other key is an error, so a
  misspelt key never passes silently.
- A list option takes strings, and may hold lists of strings one level deep, which are spliced in
  place: `{ "go.mod", b.sources("*.go") }` is one flat list.
- A name, a path and a label are plain strings.
- A function that refuses its arguments fails the build file with the line that called it. A
  `pcall` around the call does not undo that: the build file has still failed.
- File names in `inputs` and patterns in `sources` are relative to the folder of the build file.

## Labels

`deps`, `rule`, an alias's target and `discovered` name a target in one of three ways:

| Form | Means |
|---|---|
| `"//dir:name"` | the target `name` declared in the build file of `dir`; `"//:name"` is the root's |
| `":name"` | the target `name` in this build file's folder |
| `"name"` | the same as `":name"` |

## Placeholders in commands

A command never holds a real path. Inside each string of `run`, these words are replaced just
before the action runs:

| Placeholder | Becomes |
|---|---|
| `$in` | the target's `inputs`, in the order written |
| `$out` | the target's `outputs`, in the order written |
| `$deps` | every output of every dependency |
| `$opt:name` | the value of the build setting `name` |

A string that is exactly `$in`, `$out` or `$deps` becomes one argument per path, and none when the
list is empty. Inside a longer string, such as `"$out/app"` or `"--src=$in"`, the paths are joined
by single spaces. `$input` is not a placeholder: the word after `$` must be exactly `in`, `out` or
`deps`. The paths are relative to the workspace root.

## `target(name, options)`

Declares a target that builds files. Returns nothing.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `run` | list of strings | | the command: program first, then its arguments |
| `rule` | label | | a rule whose `run` is the command instead; exactly one of `run` and `rule` |
| `inputs` | list of strings | `{}` | the files the command reads |
| `deps` | list of labels | `{}` | the targets whose outputs the command reads |
| `outputs` | list of strings | `{ name }` | the files the command writes |
| `env` | list of strings | `{}` | the environment variables the command is given, read from yours |
| `network` | boolean | `false` | `true` says the command reaches the network; `plan` and the approver show it |
| `stdout` | boolean | `false` | `true` makes what the command prints the output; needs exactly one output |
| `always` | boolean | `false` | `true` runs the command on every build instead of caching it |

```lua
b.target("hello.txt", { run = { "sh", "-c", "echo hello > $out" } })
```

## `test(name, options)`

Declares a test, run by `buildl test` and passed when its command exits 0. Returns nothing. It
takes the options of `target` except `stdout`, and `outputs` defaults to none: a test is a record of
its exit status and what it printed.

| Option | Type | Default |
|---|---|---|
| `run` | list of strings | |
| `rule` | label | |
| `inputs` | list of strings | `{}` |
| `deps` | list of labels | `{}` |
| `outputs` | list of strings | `{}` |
| `env` | list of strings | `{}` |
| `network` | boolean | `false` |
| `always` | boolean | `false` |

```lua
b.test("hello_test", { run = { "grep", "-q", "hello", "$deps" }, deps = { ":hello.txt" } })
```

## `discover(name, options)`

Declares a discovery target: an action whose one output, `<name>.json`, a later pass of the build
files reads with [`discovered`](#discoveredlabel). Returns nothing. It takes the options of
`target` except `outputs` and `always`, since its output is fixed and it is always cached.

| Option | Type | Default |
|---|---|---|
| `run` | list of strings | |
| `rule` | label | |
| `inputs` | list of strings | `{}` |
| `deps` | list of labels | `{}` |
| `env` | list of strings | `{}` |
| `network` | boolean | `false` |
| `stdout` | boolean | `false` |

```lua
b.discover("pkgs", { run = { "go", "list", "-json=ImportPath", "./..." }, stdout = true })
```

## `rule(name, options)`

Declares a command that targets name with `rule` instead of writing their own `run`. Its
placeholders take the values of the target that uses it. Returns nothing.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `run` | list of strings | required | the command |
| `desc` | string | none | the line shown for each action that uses the rule; placeholders are replaced in it too |

```lua
b.rule("join", { run = { "cat", "$in" }, desc = "joining $in" })
```

## `alias(name, target)`

Declares `name` as a second name for `target`, a label. Returns nothing.

```lua
b.alias("default", ":hello.txt")
```

## `option(name, options)`

Declares a build setting, which a command reads as `$opt:name` and a run changes with
`--set name=value`. Returns nothing.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `default` | string | required | the value when no `--set` gives another |

```lua
b.option("greeting", { default = "hello" })
```

## `subdir(path)`

Adds the build file of the folder `path`, relative to this one, to the workspace. It is read too,
and its targets are named by its folder from the workspace root, such as `//lib:name`. A path that
leaves the workspace is refused. Returns nothing.

```lua
b.subdir("lib")
```

## `sources(pattern)`

Returns the files below this build file's folder whose path matches `pattern`, as a list of
strings relative to the folder, sorted. `*` matches within one folder name and stops at `/`;
`**/` matches any number of folders, none included. Only regular files are listed, links are not
followed, and the output directory and `.buildl` are never searched. Every entry visited counts
against `[declaration] walk_entries`.

```lua
local go_files = b.sources("**/*.go")
```

## `folder()`

Returns the folder of the build file that is being read, as a string relative to the workspace root:
`""` in the root build file, `"lib"` in `lib/build.lua`, `"lib/net"` in `lib/net/build.lua`. It takes
no arguments. Inside a plugin it is the folder of the build file that called the plugin, not the
plugin's own.

```lua
local dir = b.folder()
```

## `discovered(label)`

Returns the output of the discovery target `label` as a string, or `nil` when it has not run yet.
`build` and `test` run discovery targets first, then read the build files again, so in that second
pass the string is there.

```lua
local listed = b.discovered(":pkgs")
```

## `decode_all(text)`

Returns a list of the JSON values in `text`, one after another, as `airsstack.json.decode` would
give each. Values are separated by white space, which may be left out beside an object, an array
or a string. Empty text gives an empty list. Unlike the other functions, its errors are plain Lua
errors that `pcall` can catch.

```lua
for _, pkg in ipairs(b.decode_all(listed or "")) do --[[ one target per package ]] end
```

## `use(name)`

Loads the plugin `buildl.toml` names under `[plugins.<name>]` and returns what its entry file
returns, usually a table of functions. A second `use` of the same name in the same build file
returns the same value without loading the plugin again.

```lua
local zip = b.use("zip")
```

## What is taken away

A build file runs with no way to reach the world or to differ from one run to the next. Three
standard Lua functions are removed, set to `nil`:

| Removed | Why |
|---|---|
| `print` | a build file declares, it does not talk; an error is the way to show something |
| `math.random` | a random number would declare something different on every run |
| `math.randomseed` | goes with `math.random` |

`tostring` and `string.format` are also changed: for a table or function without its own
`__tostring` they raise an error instead of giving text with a memory address in it, which would
differ on every run.
