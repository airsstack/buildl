# Approvers

The ceiling says what a build may want. Once a build's wants all fit under it, one more question is
left: should this build run now? The **approver** answers it, with yes or no. It cannot grant
anything the ceiling does not, and it is never asked about a build the ceiling already refused.

`build`, `test` and `verify` ask the approver after planning and before anything runs. `plan` never
asks, so you can always look before you leap.

## The three approvers

`buildl.toml` picks one:

```toml
[approver]
default = "interactive"
```

| `default` | Answer |
|---|---|
| `manifest` (when left out) | yes: the ceiling is the whole decision |
| `interactive` | asks you on the terminal once, and remembers a yes |
| `deny` | no, always |

### `manifest`

The default. If the wants fit under the ceiling, the build runs. This suits a workspace whose
ceiling you keep short and review like code, and it suits CI, where nobody is there to ask.

### `deny`

Nothing runs, ever, even when every target could come from the cache. `plan`, `check`, `graph` and
`query` still work, so a workspace can be read and studied without any chance of running it:

```console
$ buildl plan
run    //:report: never built
run    //tools:hello: never built
  run programs:   go
  read variables: HOME
  write into:     .buildl, out
2 to run, 0 conditional, 0 clean
$ buildl build
error: the workspace's approver is deny
  run programs:   go
  read variables: HOME
  write into:     .buildl, out
```

The build exits with code 4, as a want outside the ceiling does. The error lists what the build
wanted, so you can see what you turned down.

### `interactive`

The first time a build runs, buildl writes the question on standard error: `this build wants to`,
then the same list `plan` prints, then `allow? [y/N]`. It reads one line from standard input, and
`y` or `yes`, in any case, is a yes. Anything else, an empty line included, is a no, and the build
is refused with exit code 4.

A yes is remembered, and the next build whose wants are all covered by remembered yeses asks
nothing. If a later build wants something new, say a build file starts running `protoc`, buildl
asks again about the new wants only. When a plugin's targets are behind a new want, a line under
the list names the plugin.

The remembered yeses live outside the workspace, in
`$XDG_DATA_HOME/buildl/approved/`, in a JSON file named for the workspace's location, or under
`$HOME/.local/share` when
`XDG_DATA_HOME` is not set to an absolute path. That is on purpose: a repository can ship its own
`.buildl/` directory, and an approval kept inside the workspace would arrive with the code it is
meant to approve.

`interactive` only asks when both standard input and standard error are terminals. In a pipe or
in CI there is nobody to answer, so the build is refused, never quietly allowed:

```console
$ buildl build
error: no terminal to ask on; run the build in a terminal once to approve, or set [approver] default = "manifest"
  run programs:   go
  read variables: HOME
  write into:     .buildl, out
```

Run the build once in a terminal and answer yes, and later runs in a pipe go through, as long as
they want nothing new.

## `buildl doctor`

`buildl doctor` shows the whole picture in one place: the ceiling, the approver, the execution
settings, the remote, and every want of the build with what the ceiling and the approver say about
it. It refuses nothing and runs nothing, so it is safe in any workspace:

```console
$ buildl doctor
workspace  /private/tmp/buildl-book/ambient
approver   manifest
execution  workers 0 (one per core), timeout 10m, strategy sandbox
remote: none
ceiling
  proc.run  go
  env.read  HOME
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run go         allowed  yes
  read HOME      allowed  yes
  write .buildl  allowed  yes
  write out      allowed  yes
```

The same workspace under `deny` says no to every want, though each is inside the ceiling:

```console
$ buildl doctor
workspace  /private/tmp/buildl-book/denied
approver   deny
execution  workers 0 (one per core), timeout 10m, strategy sandbox
remote: none
ceiling
  proc.run  go
  env.read  HOME
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run go         allowed  no
  read HOME      allowed  no
  write .buildl  allowed  no
  write out      allowed  no
```

And the untrusted clone from [Nothing Is Ambient](ch04-01-nothing-is-ambient.md), whose `plan` is
refused, shows the refused want beside the others:

```console
$ buildl doctor
workspace  /private/tmp/buildl-book/untrusted
approver   manifest
execution  workers 0 (one per core), timeout 10m, strategy sandbox
remote: none
ceiling
  proc.run  go
  env.read  HOME
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run curl       REFUSED  -
  run go         allowed  yes
  read HOME      allowed  yes
  write .buildl  allowed  yes
  write out      allowed  yes
```

`doctor` exits 0 in all three. It reports; `plan` and `build` decide.
