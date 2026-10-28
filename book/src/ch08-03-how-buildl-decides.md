# How buildl Decides

A plugin is code someone else wrote, and a new release of it might want more than the last. So
before the ceiling ever sees a want of a plugin's target, buildl holds it to two lists that
belong to the plugin.

## Three lists, in order

```text
  what the target wants
     │
     ▼
  the plugin's extension.toml   ── not listed? refused, exit 4
     │
     ▼
  [plugins.<name>] in buildl.toml, the workspace's cap   ── not listed? refused, exit 4
     │
     ▼
  [ceiling] in buildl.toml   ── not listed? refused, exit 4
     │
     ▼
  the approver
```

| Want | Allowed by the plugin when |
|---|---|
| a program | its manifest's `proc.run` and its cap's `proc.run` both list it |
| a variable | its manifest's `env.read` and its cap's `env.read` both list it |
| the network | its cap says `network = true` |

A plugin whose table gives no cap allows its targets nothing. Inputs and the folders a build writes
into are not narrowed per plugin, and a program a dependency built is no want at all (see
[Nothing Is Ambient](ch04-01-nothing-is-ambient.md)).

When a plugin uses another plugin, a target is held to every plugin in the chain. A wrapper `ourgo`
that calls `go.binary` needs both `ourgo` and `go` to allow what the target runs.

## The workspace narrows

Take the `examples/plugins` workspace from [Writing a Plugin](ch08-01-writing-a-plugin.md), and
give the plugin no variables in its cap. The ceiling still allows `HOME`:

```toml
[plugins.gobin]
path     = "plugins/gobin"
proc.run = ["go"]
env.read = []
```

```console
$ buildl build
error: build.lua: //:bye (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)
```

The build exits with code 4, before anything runs, as a want outside the ceiling would. The message
names the build file, the first target refused, the plugin chain in brackets, and what the plugin
does allow.

## The manifest narrows

Now put the cap back and take `go` out of the plugin's own manifest:

```toml
# plugins/gobin/extension.toml
[capabilities]
proc.run = []
env.read = ["HOME"]
```

```console
$ buildl build
error: build.lua: //:bye (plugin gobin@0.1.0): program go is outside plugin gobin@0.1.0 (proc.run allows: nothing)
```

Exit code 4 again. The workspace and the ceiling would both allow `go`; the plugin did not ask for
it, so its targets cannot have it. This is the case that matters: a plugin update that starts
running `curl` is stopped by its own manifest, then by your cap, then by the ceiling.

## Every view names the plugin

A declaration made through a plugin carries its **origin**: the chain of plugins that made it,
each as `name@version` with what its manifest allows. The origin is for showing and for checking.
It is not part of the [action key](ch05-01-action-key.md), so a target moved out of a plugin into a
plain `buildl.target` with the same fields stays cached.

**`plan`** prints, after the wanted set, each want a plugin's targets have, with the lowest label
that wants it:

```console
$ buildl plan
run    //:bye: never built
run    //:hello: never built
  run programs:   go
  read variables: HOME
  write into:     .buildl, out
  go: declared by plugin gobin@0.1.0 for //:bye
  HOME: declared by plugin gobin@0.1.0 for //:bye
2 to run, 0 conditional, 0 clean
```

**`doctor`** puts the same line under each want's row, and ends with a `plugin violations` section
worded as the build's refusal would be. Here it is in the workspace whose cap gives no variables;
the lines above `wants` are left off:

```console
$ buildl doctor | sed -n '/^wants/,$p'
wants            ceiling  approved
  run go         allowed  yes
    declared by plugin gobin@0.1.0 for //:bye
  read HOME      allowed  yes
    declared by plugin gobin@0.1.0 for //:bye
  write .buildl  allowed  yes
  write out      allowed  yes
plugin violations
  build.lua: //:bye (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)
  build.lua: //:hello (plugin gobin@0.1.0): environment variable HOME is outside plugin gobin@0.1.0 (env.read allows: nothing)
```

`read HOME` is `allowed` in the ceiling column, because the ceiling does allow it; the violation
is the plugin's. Like the rest of `doctor`, it refuses nothing and exits 0.

**The approver.** The `interactive` approver puts a line under its question naming the plugin
behind a new want, as `plan` does (see [Approvers](ch04-03-approvers.md)).

**`graph`** gives every target its origin. A target a build file wrote has `"origin":"BuildFile"`;
these two have the plugin:

```console
$ buildl graph | python3 -c 'import json,sys; g=json.load(sys.stdin)["targets"]; [print(l, json.dumps(t["origin"])) for l,t in sorted(g.items())]'
//:bye {"Plugins": [{"env": ["HOME"], "name": "gobin", "run": ["go"], "version": "0.1.0"}]}
//:hello {"Plugins": [{"env": ["HOME"], "name": "gobin", "run": ["go"], "version": "0.1.0"}]}
```

## Where it is checked

The order of the checks is the order of the commands' work:

| When | What is checked | Failure |
|---|---|---|
| before any build file opens | each `[plugins.<name>]` is a folder of the workspace, or is pinned in `buildl.lock` and digests to the pin | exit 5 if not fetched, exit 6 if changed |
| when `buildl.use` runs | the manifest: its `name`, `api`, `entry`, and that it asks for nothing but `proc.run` and `env.read` | exit 5 |
| at plan | every want of every plugin target, against each plugin in its chain, then the ceiling | exit 4 |

No plugin can do anything a build file cannot. It holds no permission of its own: it can only
narrow what the workspace already allows.
