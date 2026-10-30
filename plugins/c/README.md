# The `c` plugin

Builds C and C++ programs, static libraries, shared libraries and tests with the compiler you
already have. Each source is compiled to its own object, so a change to one source or one header
rebuilds only the objects that read it.

It works on Linux and macOS, with `cc`, `c++`, `gcc`, `g++`, `clang` and `clang++`, and with `ar` for libraries.

## Setting it up

Name the plugin in `buildl.toml`, give it the program and the environment it may use, and let the
ceiling allow them too:

```toml
[ceiling]
proc.run = ["cc"]
env.read = ["PATH"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]

[plugins.c]
github   = "airsstack/buildl"
rev      = "plugin-c-v0.1.0"
dir      = "plugins/c"
proc.run = ["cc"]
env.read = ["PATH"]
```

`sh` is needed only for `c.library` and for a `c.test` with `deps`; add it to both `proc.run` lists
when you use either. Allowing `sh` lets a target run any program through `sh -c`, so the ceiling's
program list then limits less.

Then run `buildl plugins fetch` once. A build file loads the plugin with `buildl.use`:

```lua
local c = buildl.use("c")

c.binary("app", {
  srcs    = { "src/main.c", "src/util.c" },
  headers = { "include/util.h" },
  cflags  = c.includes({ "include" }),
})
```

The sources and headers are written relative to the build file. Paths inside `cflags`, `cxxflags`,
`ldflags` and `ldlibs` are written from the workspace root; `c.includes` does that for include folders.

## The functions

Each takes `(name, options)` and returns nothing. An option it does not know is refused, and the
message names it.

| Function | Declares |
|---|---|
| `c.binary(name, o)` | an executable |
| `c.library(name, o)` | a static library, `ar rcs` over the objects, run through `sh` |
| `c.shared_library(name, o)` | a shared library; `-shared` is the one flag the plugin adds |
| `c.test(name, o)` | a test binary `<name>.bin`, then a test target `name` that runs it |

| Option | Type | Meaning | Default |
|---|---|---|---|
| `srcs` | list of strings | the sources; `.c` is C, `.cc`, `.cpp` and `.cxx` are C++ | required, not empty |
| `headers` | list of strings | the headers a source may include | `{}` |
| `header_deps` | list of labels | targets whose outputs the compiles read, such as a generated `config.h` | `{}` |
| `cflags` | list of strings | arguments of C compiles, and of the discovery of C sources | `{}` |
| `cxxflags` | list of strings | arguments of C++ compiles, and of the discovery of C++ sources | `{}` |
| `ldflags` | list of strings | link arguments before the objects | `{}` |
| `ldlibs` | list of strings | link arguments after the objects, such as `-lm` | `{}` |
| `deps` | list of labels | targets linked in, after the objects; for a test, also present when it runs | `{}` |
| `cc` | string | the C compiler | `"cc"` |
| `cxx` | string | the C++ compiler | `"c++"` |
| `linker` | string | the program that links | `cxx` when a source is C++, else `cc` |
| `output` | string | the file the last step writes | `name` (`c.test`: `name .. ".bin"`) |
| `version_probe` | boolean | see below | `false` |

`c.library` refuses `ldflags`, `ldlibs`, `linker` and `deps`: an archive is not linked, and a
library another library needs is given to the binary that links them both.

A test lists in `deps` every shared library its binary loads, since the sandbox holds only the
outputs of the targets a step names. With `deps`, the test runs through `sh -c 'exec "$0"'` so the
binary runs alone, without the libraries as arguments.

## The helpers

Each returns a value and declares nothing, except `c.platform`. Put the value where you want it.

| Helper | Returns |
|---|---|
| `c.includes(dirs)` | `-I<folder>/<dir>` for each directory, `folder` being the build file's; `-I<dir>` at the root |
| `c.defines(map)` | sorted by key: `-D<key>` for `true`, nothing for `false`, `-D<key>=<value>` for a string or a number |
| `c.platform(options?)` | `"linux"` or `"macos"`, or `nil` before discovery has run (see below) |
| `c.shared_name(name, platform)` | `lib<name>.so` or `lib<name>.dylib`; `nil` when `platform` is `nil` |
| `c.soname(file, platform)` | the flags that give a shared library its runtime name; `{}` when `platform` is `nil` |
| `c.rpath(platform)` | the flags that make a binary look for shared libraries in its own folder; `{}` when `platform` is `nil` |
| `c.depfile.parse(text)` | the rules of Make-style output, for a discovery target of your own |

A shared library and a binary that uses it:

```lua
local c = buildl.use("c")
local platform = c.platform()
local file = c.shared_name("util", platform)
local include = c.includes({ "include" })

c.shared_library("util", {
  srcs    = { "src/util.c" },
  headers = { "include/util.h" },
  cflags  = { "-fPIC", include[1] },
  ldflags = c.soname(file, platform),
  output  = file,
})

c.binary("app", {
  srcs    = { "src/main.c" },
  headers = { "include/util.h" },
  cflags  = include,
  ldflags = c.rpath(platform),
  deps    = { ":util" },
})
```

## How the headers are found

A call declares its targets in two passes, because the headers a source reads are known only once
the compiler has said so.

1. The first pass declares one discovery target per source, `<name>.<source path with / as .>.deps`,
   which runs the compiler with `-MM`. If `version_probe` is set it also declares the version
   targets.
2. Once every source of the call has a result, the second pass declares an object per source, whose
   inputs are the source and the headers it read, and then the last step.

Each header the compiler names is placed by these rules, in order:

| The header | Becomes |
|---|---|
| an absolute path | an error naming it: a header outside the workspace belongs in an `-isystem` folder, whose headers `-MM` leaves out |
| inside the build file's folder and in `headers` | an input of the object |
| anything else, when the call has `header_deps` | nothing: it is an output of one of them, and the compile depends on them already |
| anything else | an error naming it, such as `…; list it in headers, or reach it through header_deps (one build file per project)`: a header outside the build file's folder cannot be declared, and a header inside the folder is refused when it is missing from `headers` |

A path is first tidied: `.` parts go and each `..` removes the part before it. One that climbs
above the workspace root is an error naming it.

The discovery targets must be the same on both passes, or buildl refuses the build with exit code
5. So `name`, `srcs`, `headers`, `header_deps`, `cflags`, `cxxflags`, `cc` and `cxx` must not depend
on anything discovered, such as `c.platform()`. `ldflags`, `ldlibs`, `deps`, `output` and `linker`
reach only the last step and may. The platform helpers return a value that is safe on the first
pass: `c.platform()` is `nil` there, and so `c.shared_name`, `c.soname` and `c.rpath` give `nil` or
`{}`.

## `c.platform`

The first call in a build file declares a discovery target `:c.platform` that runs
`cc -dumpmachine`; later calls return the same value and declare nothing. `options.cc` names another
compiler for the first call; a later call whose `cc` (default `"cc"`) differs from the first is an
error, so a plain `c.platform()` after `c.platform({ cc = "clang" })` is one. A triple holding
`apple-darwin` gives `"macos"` and one holding `linux` gives `"linux"`; any other is an error naming
it.

## The version probe

A target's key records the program file, and `/usr/bin/cc` is a launcher that stays the same when
the compiler behind it changes. With `version_probe = true` the call declares `<name>.version.c`
(the call's `cc` with `--version`) for its C sources and `<name>.version.cpp` (its `cxx` with
`--version`) for its C++ sources,
each an always-run target, and every object of that language depends on its probe, so a new
compiler rebuilds the objects.

The cost is in `buildl plan`: an always-run target leaves every compile that depends on it listed as
conditional until the probe's output is known. A build still takes those objects from the cache when
the probe's output is the same.

## The programs it may run

The plugin's manifest lists the programs it may run: `cc`, `c++`, `gcc`, `g++`, `clang`,
`clang++` and `sh`. `ar` is not on it because `c.library` runs it through `sh`. A program outside
the list is refused even when the workspace allows it, and the workspace's cap and ceiling narrow
the list further. A versioned tool, such as `gcc-13` or `clang-18`, is not on it.

To use one, keep a copy of this folder in your workspace, add the program to the `proc.run` of its
`extension.toml`, and name it as a `path` plugin; the calls name the program in their `cc` option:

```toml
[plugins.c]
path     = "plugins/c"
proc.run = ["gcc-13"]
env.read = ["PATH"]
```

Every step that links declares `env = { "PATH" }`, since the compiler driver finds the linker
through it, so `PATH`'s value is part of each link's key.

`c.library` runs `ar` through `sh` as `ZERO_AR_DATE=1 exec ar rcs …`: macOS `ar` stamps an archive
with the time it ran unless `ZERO_AR_DATE` is set, and the archive would differ on every run. GNU
`ar` as Debian and Ubuntu ship it is deterministic already; `ZERO_AR_DATE` does no harm there. So a
workspace with a library allows `sh`, and the archive step declares `env = { "PATH" }` too, for `sh`
to find `ar`. The archiver is fixed: another one, such as `llvm-ar`, means editing `graph.lua` in the
copied folder.

## Errors

A message names the function and the target, such as `c.binary(app): the option bogus is not
known`, and carries the file and line of your call.
