# What Did It Do?

`buildl` keeps no secrets about what it does. This page asks it three questions about the
workspace from the last page: what a build would run, what a target printed, and what the
workspace allows.

## What would a build run?

`buildl plan` works out everything a build would, then stops before running anything:

```console
$ buildl plan
clean  //:names.txt
  run programs: sort
  write into:   .buildl, out
0 to run, 0 conditional, 1 clean
```

`//:names.txt` is the target's full name, its *label*: `//` is the top of the workspace, and
`names.txt` is the name after the colon. `clean` means nothing it depends on has changed since it
was built, so a build would leave it alone. The two indented lines are what the build would be
allowed to do: run `sort`, and write into `.buildl` and `out`.

Now change the input, and ask again:

```console
$ echo "bob" >> names.in
$ buildl plan
run    //:names.txt: input names.in changed
  run programs: sort
  write into:   .buildl, out
1 to run, 0 conditional, 0 clean
```

The target would run, and `plan` says why: `names.in` changed. `buildl` noticed by hashing the
contents of every input, not by looking at file times, so saving a file without changing it does
not cause a rebuild. *Conditional* counts targets that depend on one that is about to run, and that
may or may not need to run themselves; [Caching](ch05-00-caching.md) explains them.

Build, and the plan is clean again:

```console
$ buildl build
1 built
$ cat out/names.txt
ada
bob
mia
zoe
$ buildl plan
clean  //:names.txt
  run programs: sort
  write into:   .buildl, out
0 to run, 0 conditional, 1 clean
```

`plan` takes the same targets `build` does, because it is that build's dry run.

## What did it print?

`buildl log` shows what a target printed the last time it ran:

```console
$ buildl log names.txt
//:names.txt: built at 2026-10-28 13:25:04 UTC
```

`sort -o` writes its result to the file and prints nothing, so all there is to show is when it
ran. The time on your machine will be your own. For a target that does print, such as a test, the
lines it printed follow; [A First Project](ch02-00-first-project.md) shows one.

## What may the workspace do?

`buildl doctor` prints the workspace's settings, and what its build wants next to what
`buildl.toml` allows:

```console
$ buildl doctor
workspace  /private/tmp/buildl-book/hello
approver   manifest
execution  workers 0 (one per core), timeout 10m, strategy sandbox
remote: none
ceiling
  proc.run  sort
  env.read  -
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run sort       allowed  yes
  write .buildl  allowed  yes
  write out      allowed  yes
```

The first line is the folder the workspace is in, so on your machine it names your own folder. The
next three are settings our short `buildl.toml` left at their defaults: who approves a build
(`manifest`, which approves whatever the ceiling allows), how actions run (as many at once as there are
cores, at most ten minutes each, in a sandbox), and no remote cache. Then comes the ceiling, and last the
*wants*: everything the declared targets need, each checked against the ceiling. `doctor` only
reports. It runs nothing and refuses nothing, so it is the command to reach for when a build is
refused and you want to see why. [Approvers](ch04-03-approvers.md) covers the approved column.

## Each command stops somewhere

Every command that reads the build files runs the same pipeline, and stops at a different point in it:

| Step | What happens | Commands that stop here |
|---|---|---|
| Load | Lua evaluates every build file | `check` |
| Resolve | the declarations become one graph of targets | `graph`, `query`, `doctor` |
| Plan | every input is hashed, and each target is marked clean or to run | `plan` |
| Execute | the targets to run are run, in parallel | |
| Record | outputs, the cache and the log are written | `build`, `test` |

So `check` is quick and runs nothing on any workspace, and `plan` tells you exactly what `build`
would do without running any of it.
