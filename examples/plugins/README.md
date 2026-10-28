# Plugins

A Go module with two programs, `hello/` and `bye/`, built through a plugin kept in the
workspace. The build file calls the plugin's helper instead of writing each target out.

```text
build.lua ── buildl.use("gobin") ─► plugins/gobin/main.lua ─► module.binary(name, dir)
                                                                 │ buildl.target(...)
                                                                 ▼
                                                     //:hello    //:bye
                                                     go build -o $out ./<dir>
```

## What it uses

| Piece | Why |
|---|---|
| `buildl.use("gobin")` | loads the plugin and returns what its entry returns: here a table with one helper, `binary` |
| `plugins/gobin/extension.toml` | the plugin's manifest: `name`, `version`, `entry` (`main.lua`), `api = 1`, and `[capabilities]`, the most its targets may want |
| `[plugins.gobin]` in `buildl.toml` | names the plugin and where it lives (`path`), and caps what its targets may want for this workspace; a want must fit the plugin's manifest, then this cap, then the ceiling |
| attribution | `buildl plan`, `buildl doctor` and the approver show a want the plugin's targets declared as `declared by plugin gobin@0.1.0 for //:…`, and `buildl graph --json` gives each target the plugin as its origin |

The helper declares nothing a build file could not: a plugin calls the same `buildl.target`.
Both programs share the one `go.mod` because a command reads its paths from the workspace
root, where `go build ./hello` finds it.

## Plugins fetched from git

A plugin may live in a git repository instead: `[plugins.<name>]` names its `git` URL and a
`rev`, `buildl plugins fetch` downloads it into the store and records it in `buildl.lock`, and
`buildl.use` loads it from there. This example keeps its plugin in the workspace, so it needs
neither the network nor a fetch.

## Run it

Needs the `buildl` binary and Go 1.22.

```sh
buildl plan       # go: declared by plugin gobin@0.1.0 for //:bye
buildl build      # 2 built
./out/hello       # hello from a plugin
buildl doctor     # every want, with the plugin that declared it
```

From `examples/`, `sh check.sh plugins` runs the shared checks on a copy, then `assert.sh`:
both programs print their lines, `doctor` and `plan` name the plugin, and `graph --json` gives
both targets its origin.
