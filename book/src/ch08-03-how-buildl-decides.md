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
| a variable marked `"key"` | its manifest's `env.read` and its cap's `env.read` both list it |
| a variable marked `"unkeyed"` | its manifest's `env.unkeyed` and its cap's `env.unkeyed` both list it |
| the network | its cap says `network = true` |

`env.unkeyed` narrows `env.read` at each of the three levels, the plugin's manifest, its cap and the
ceiling: a name in an `env.unkeyed` that the `env.read` beside it does not list is a fault in the
file, found when `buildl` reads it, exit code 5. A list left out is empty, so a plugin's targets may
leave a variable out of the key only when the manifest, the cap and the ceiling all say so.

A plugin whose table gives no cap allows its targets nothing. Inputs and the folders a build writes
into are not narrowed per plugin, and a program a dependency built is no want at all (see
[Nothing Is Ambient](ch04-01-nothing-is-ambient.md)).

When a plugin uses another plugin, a target is held to every plugin in the chain. A wrapper `ourgo`
that calls `go.binary` needs both `ourgo` and `go` to allow what the target runs.

## The workspace narrows

Take the `examples/plugins` workspace from [Writing a Plugin](ch08-01-writing-a-plugin.md), and
give the plugin no variables in its cap. The ceiling still allows `HOME`. The plugin's targets mark
`HOME` `"unkeyed"`, so the cap has to say so in `env.unkeyed`, and `env.unkeyed` may not name what
`env.read` leaves out. Taking `HOME` out of `env.read` alone is therefore a fault in the file:

```toml
[plugins.gobin]
path        = "plugins/gobin"
proc.run    = ["go"]
env.read    = []
env.unkeyed = ["HOME"]
```

```console
$ buildl build
error: buildl.toml: [plugins.gobin] env.unkeyed lists HOME, which env.read does not
```

That is exit code 5, the same as for the ceiling's own pair of lists. Take `HOME` out of both:

```toml
[plugins.gobin]
path        = "plugins/gobin"
proc.run    = ["go"]
env.read    = []
env.unkeyed = []
```

```console
$ buildl build
error: build.lua: //:bye (plugin gobin@0.1.0): unkeyed variable HOME is outside plugin gobin@0.1.0 (env.unkeyed allows: nothing)
```

The build exits with code 4, before anything runs, as a want outside the ceiling would. The message
names the build file, the first target refused, the plugin chain in brackets, and what the plugin
does allow. A `"key"` variable the cap does not list is refused the same way, as `environment
variable X is outside plugin …` with `env.read allows`.

## The manifest narrows

Now put the cap back and take `go` out of the plugin's own manifest:

```toml
# plugins/gobin/extension.toml
[capabilities]
proc.run    = []
env.read    = ["HOME"]
env.unkeyed = ["HOME"]
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
  run programs:      go
  unkeyed variables: HOME
  write into:        .buildl, out
  go: declared by plugin gobin@0.1.0 for //:bye
  HOME (unkeyed): declared by plugin gobin@0.1.0 for //:bye
2 to run, 0 conditional, 0 clean
```

**`doctor`** puts the same line under each want's row, and ends with a `plugin violations` section
worded as the build's refusal would be. Here it is in the workspace whose cap gives no variables;
the lines above `wants` are left off:

```console
$ buildl doctor | sed -n '/^wants/,$p'
wants                ceiling  approved
  run go             allowed  yes
    declared by plugin gobin@0.1.0 for //:bye
  read HOME unkeyed  allowed  yes
    declared by plugin gobin@0.1.0 for //:bye
  write .buildl      allowed  yes
  write out          allowed  yes
plugin violations
  build.lua: //:bye (plugin gobin@0.1.0): unkeyed variable HOME is outside plugin gobin@0.1.0 (env.unkeyed allows: nothing)
  build.lua: //:hello (plugin gobin@0.1.0): unkeyed variable HOME is outside plugin gobin@0.1.0 (env.unkeyed allows: nothing)
```

`read HOME unkeyed` is `allowed` in the ceiling column, because the ceiling does allow it; the
violation is the plugin's. Like the rest of `doctor`, it refuses nothing and exits 0.

**The approver.** The `interactive` approver puts a line under its question naming the plugin
behind a new want, as `plan` does (see [Approvers](ch04-03-approvers.md)).

**`graph`** gives every target its origin. A target a build file wrote has `"origin":"BuildFile"`;
these two have the plugin:

```console
$ buildl graph | python3 -c 'import json,sys; g=json.load(sys.stdin)["targets"]; [print(l, json.dumps(t["origin"])) for l,t in sorted(g.items())]'
//:bye {"Plugins": [{"env": ["HOME"], "name": "gobin", "run": ["go"], "unkeyed": ["HOME"], "version": "0.1.0"}]}
//:hello {"Plugins": [{"env": ["HOME"], "name": "gobin", "run": ["go"], "unkeyed": ["HOME"], "version": "0.1.0"}]}
```

## Where it is checked

The order of the checks is the order of the commands' work:

| When | What is checked | Failure |
|---|---|---|
| before any build file opens | each `[plugins.<name>]` is a folder of the workspace, or is pinned in `buildl.lock` and digests to the pin; its `env.unkeyed` is inside its `env.read` | exit 5 if not fetched or the lists disagree, exit 6 if changed |
| when `buildl.use` runs | the manifest: its `name`, `api`, `entry`, and that it asks for nothing but `proc.run`, `env.read` and `env.unkeyed`, and that `env.unkeyed` is inside `env.read` | exit 5 |
| at plan | every want of every plugin target, against each plugin in its chain, then the ceiling | exit 4 |

No plugin can do anything a build file cannot. It holds no permission of its own: it can only
narrow what the workspace already allows.
