# Build Files

A build file is a Lua file named `build.lua`. It says what can be built: which files go in, which
command runs, and what comes out. It never runs that command itself. buildl reads every build file
first, and only then decides what to run.

This chapter covers:

- [Targets and Labels](ch03-01-targets-and-labels.md): the four ways to declare something, and how
  targets name each other.
- [Inputs, Outputs and Dependencies](ch03-02-inputs-outputs-deps.md): what a command reads and
  writes, and the `$in`, `$out` and `$deps` placeholders.
- [The Toolbox](ch03-03-toolbox.md): the helper modules a build file can call.
- [Many Directories](ch03-04-many-directories.md): one build file per directory, joined by labels.
- [Build Settings](ch03-05-build-settings.md): values you choose when you run a build.

## What a build file sees

A build file starts with two globals:

| Global | What it holds |
|---|---|
| `buildl` | the declaration functions: `buildl.target`, `buildl.test`, `buildl.rule` and the rest |
| `airsstack` | the helper modules `json`, `path`, `regex`, `hash` and `glob`, and `buildl` again |

They are not two copies. `buildl` and `airsstack.buildl` are one table with two names, so most build
files begin with a short local name for it:

```lua
local b = buildl
```

## What a build file does not see

A build file must declare the same targets every time it is read, on every machine. So the parts of
Lua that could make it answer differently are not there:

- `print` is gone. A build file has nowhere to write to.
- `math.random` and `math.randomseed` are gone. A random name would change on every run.
- `os`, `io`, `require`, `load` and `dofile` are gone. A build file cannot read the clock, the
  environment or a file, and cannot load another build file's code.

Calling one of them is an ordinary Lua error, and the build file fails:

```lua
-- build.lua
print(buildl == airsstack.buildl)
```

```console
$ buildl check
error: build.lua: runtime error: lua error in build.lua: runtime error: build.lua:1: attempt to call a nil value (global 'print')
stack traceback:
	[C]: in global 'print'
	build.lua:1: in main chunk
```

`buildl check` reads every build file and runs nothing, which makes it the quickest way to see
whether your build files are well formed. A build file that fails exits with code 5.

The one way a build file looks at the disk is `buildl.sources`, which lists files in its own
directory. [Inputs, Outputs and Dependencies](ch03-02-inputs-outputs-deps.md) covers it.
