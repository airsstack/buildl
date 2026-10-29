# Plugins

A plugin is a folder with a manifest, `extension.toml`, and a Lua entry script. A build file loads it
with `buildl.use`. What the plugin declares carries its name to `plan`, `doctor`, the approver and
`graph`.

## The entry script

An ordinary Lua module that returns a table of functions. Each function calls `buildl.target`, the
only way a plugin makes a target. A plugin cannot run anything itself.

```lua no-check
-- a plugin's own main.lua: it is a module, not a build file
local module = {}

function module.binary(name, dir)
  buildl.target(name, {
    inputs = { "go.mod", buildl.sources(dir .. "/*.go") },
    run    = { "go", "build", "-o", "$out", "./" .. dir },
    env    = { "HOME" },
  })
end

return module
```

## The manifest

```toml
# plugins/gobin/extension.toml
[extension]
name    = "gobin"
version = "0.1.0"
entry   = "main.lua"
api     = 1

[capabilities]
proc.run = ["go"]
env.read = ["HOME"]
```

| Key | Says |
|---|---|
| `name` | the plugin's name. It must be the name `buildl.toml` gives it |
| `version` | text to show. Never compared |
| `entry` | the file `buildl.use` runs, inside the plugin's folder |
| `api` | the plugin API version it was written for. This version supports `1` |
| `proc.run` | the programs its targets may run |
| `env.read` | the variables its targets may read |

A manifest holding `fs.read`, `fs.write`, a module flag, `[capabilities.optional]` or `[limits]` is
refused, naming the file. The manifest is its author's: an agent offers no edit to it.

## The workspace names it

`buildl.toml` names each plugin, and its cap is the most the workspace gives it:

```toml
[ceiling]
proc.run = ["go"]
env.read = ["HOME"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]

[plugins.gobin]
path     = "plugins/gobin"
proc.run = ["go"]
env.read = ["HOME"]
```

A target a plugin declares may have a program or variable only when its manifest, the workspace's
cap and the ceiling all list it. A plugin's network use needs `network = true` in its cap. A cap
left out allows nothing.

## Using it

```lua no-check
-- needs [plugins.gobin] in buildl.toml and the plugin's files, which the checked blocks do not have
local gobin = buildl.use("gobin")
gobin.binary("hello", "hello")
```

## Plugins from git

Give `git` (or `github = "owner/repo"`) and `rev`, a tag or a full commit, instead of `path`:

```toml
[plugins.gobin]
git      = "https://example.com/plugins/gobin.git"
rev      = "v0.1.0"
proc.run = ["go"]
env.read = ["HOME"]
```

| Step | What happens |
|---|---|
| any command that reads build files, before a fetch | refused with exit 5: ``error: plugin gobin is not fetched: buildl.lock does not pin <url> at v0.1.0; run `buildl plugins fetch` `` |
| `buildl plugins fetch` | pins each git plugin in `buildl.lock` and stores its files in `.buildl`. Prints `fetched gobin <commit>`, or `unchanged gobin <commit>`. A failed fetch is exit 3: report it, do not retry |
| every later command | checks the stored files against `buildl.lock`, not the network. A changed file is exit 6 |
| a moved tag | the fetch is refused. Moving a plugin on is a new `rev` in `buildl.toml`, then a fetch |

Commit `buildl.lock` with the workspace.

## Refusals that name a plugin

Exit 4, with the plugin in brackets:

```text
error: build.lua: //:bye (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)
error: build.lua: //:bye (plugin gobin@0.1.0): program go is outside plugin gobin@0.1.0 (proc.run allows: nothing)
```

The line alone does not say which stops it. Compare the want with the plugin's `extension.toml` and
with its `[plugins.<name>]` table: if the cap lacks it, ask the user about the cap. If the manifest
lacks it, tell the user, and offer no edit to the manifest. A wrong `api` is exit 5.
