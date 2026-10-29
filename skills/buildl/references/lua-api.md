# Build file API

A build file reaches buildl through one table, the global `buildl`. Start each file with
`local b = buildl`. The table holds eleven functions. Every options table refuses a key it does
not know: a misspelt key fails the build file (exit 5) with the line that called it.

| Function | Declares |
|---|---|
| `target(name, options)` | a target that builds files |
| `test(name, options)` | a test target |
| `discover(name, options)` | a discovery target |
| `rule(name, options)` | a command several targets share |
| `alias(name, target)` | a second name for a target |
| `option(name, options)` | a build setting |
| `subdir(path)` | another directory with a build file |
| `sources(pattern)` | returns a sorted list of file names |
| `discovered(label)` | returns a discovery target's output, or `nil` |
| `decode_all(text)` | returns a list of the JSON values in `text` |
| `use(name)` | returns a plugin's module |

## Rules every function shares

- A list option takes strings, and lists of strings one level deep, spliced in place:
  `{ "go.mod", b.sources("*.go") }` is one flat list.
- File names in `inputs` and patterns in `sources` are relative to the build file's folder.
- A `pcall` around a refused call does not undo it: the build file has still failed.

## Labels

`deps`, `rule`, an alias's target and `discovered` name a target in one of three ways:

| Form | Means |
|---|---|
| `"//dir:name"` | the target `name` in the build file of `dir`. `"//:name"` is the root's |
| `":name"` | the target `name` in this build file's folder |
| `"name"` | the same as `":name"` |

## Placeholders

A command never holds a real path. Inside each string of `run`:

| Placeholder | Becomes |
|---|---|
| `$in` | the target's `inputs`, in order |
| `$out` | the target's `outputs`, in order |
| `$deps` | every output of every dependency |
| `$opt:name` | the value of the build setting `name` |

A string that is exactly `$in`, `$out` or `$deps` becomes one argument per path (none when the list
is empty). Inside a longer string, such as `"$out/app"`, the paths are joined by single spaces.
`$input` is not a placeholder: the word after `$` must be exactly `in`, `out` or `deps`. Paths are
relative to the workspace root.

## `target(name, options)`

| Option | Type | Default | Meaning |
|---|---|---|---|
| `run` | list of strings | | the command: program first, then arguments |
| `rule` | label | | a rule whose `run` is the command instead. Exactly one of `run` and `rule` |
| `inputs` | list of strings | `{}` | the files the command reads |
| `deps` | list of labels | `{}` | the targets whose outputs the command reads |
| `outputs` | list of strings | `{ name }` | the files the command writes |
| `env` | list of strings | `{}` | the environment variables the command is given, read from yours |
| `network` | boolean | `false` | `true` says the command reaches the network |
| `stdout` | boolean | `false` | `true` makes what the command prints the output. Needs exactly one output |
| `always` | boolean | `false` | `true` runs the command on every build instead of caching it |

```lua
local b = buildl
b.target("hello.txt", { run = { "sh", "-c", "echo hello > $out" } })
```

## `test(name, options)`

Passes when its command exits 0. Takes the options of `target` except `stdout`. `outputs` defaults
to none.

```lua
local b = buildl
b.target("hello.txt", { run = { "sh", "-c", "echo hello > $out" } })
b.test("hello_test", { run = { "grep", "-q", "hello", "$deps" }, deps = { ":hello.txt" } })
```

## `discover(name, options)`

An action whose one output, `<name>.json`, a later pass reads with `discovered`. Takes the options
of `target` except `outputs` and `always`. `build` and `test` run discovery targets first, then
read the build files again.

```lua
local b = buildl
b.discover("pkgs", { run = { "go", "list", "-json=ImportPath", "./..." }, stdout = true, env = { "HOME" } })
```

## `rule(name, options)`

| Option | Type | Default | Meaning |
|---|---|---|---|
| `run` | list of strings | required | the command |
| `desc` | string | none | the line shown for each action that uses the rule. Placeholders are replaced in it |

```lua
local b = buildl
b.rule("join", { run = { "cat", "$in" }, desc = "joining $in" })
b.target("joined.txt", { rule = ":join", inputs = { "a.txt", "b.txt" }, stdout = true })
```

## `alias(name, target)`

```lua
local b = buildl
b.target("hello.txt", { run = { "sh", "-c", "echo hello > $out" } })
b.alias("default", ":hello.txt")
```

## `option(name, options)`

A build setting, read as `$opt:name` and changed per run by `--set name=value`.

| Option | Type | Default | Meaning |
|---|---|---|---|
| `default` | string | required | the value when no `--set` gives another |

```lua
local b = buildl
b.option("greeting", { default = "hello" })
b.target("greeting.txt", { run = { "echo", "$opt:greeting" }, stdout = true })
```

## `subdir(path)`

Adds the build file of the folder `path`, relative to this one. Its targets are named from the
workspace root, such as `//lib:name`. A path that leaves the workspace is refused, and so is a
folder with no build file.

```lua no-check
-- needs lib/build.lua beside it, which the checked blocks do not have
local b = buildl
b.subdir("lib")
```

## `sources(pattern)`

Returns the regular files below this build file's folder that match `pattern`, relative to the
folder, sorted. `*` matches within one folder name and stops at `/`. `**/` matches any number of
folders, none included. Links are not followed. The output directory and `.buildl` are never
searched.

```lua
local b = buildl
local go_files = b.sources("**/*.go")
b.target("app", { inputs = { "go.mod", go_files }, run = { "go", "build", "-o", "$out", "." }, env = { "HOME" } })
```

## `discovered(label)` and `decode_all(text)`

`discovered` returns the output of the discovery target's last successful run as a string, or
`nil` when it has never run.
`decode_all` returns a list of the JSON values in `text`, one after another. Empty text gives an
empty list. Its errors are plain Lua errors that `pcall` can catch.

```lua
local b = buildl
b.discover("pkgs", { run = { "go", "list", "-json=ImportPath", "./..." }, stdout = true, env = { "HOME" } })
local listed = b.discovered(":pkgs")
for _, pkg in ipairs(b.decode_all(listed or "")) do
  b.target("note_" .. pkg.ImportPath:gsub("/", "_"), { run = { "echo", pkg.ImportPath }, stdout = true })
end
```

## `use(name)`

Loads the plugin `buildl.toml` names under `[plugins.<name>]` and returns what its entry file
returns, usually a table of functions. See `plugins.md`.

```lua no-check
-- needs a [plugins.zip] table in buildl.toml and the plugin's files
local b = buildl
local zip = b.use("zip")
```

## What is taken away

| Removed | Why |
|---|---|
| `print` | a build file declares, it does not talk. Use `error("text", 0)` to show something |
| `math.random`, `math.randomseed` | a random number would declare something different on every run |

`tostring` and `string.format` raise an error for a table or function without its own `__tostring`.
