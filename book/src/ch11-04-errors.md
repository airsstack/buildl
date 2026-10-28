# Errors

A build can go wrong in two different ways, and `buildl` keeps them apart.

- **A target fails.** Its action exits with an error, or leaves the wrong thing behind, or the
  machine cannot run it. That is an *outcome*: one target ended badly, and the build still knows
  what every other target did.
- **The build cannot go on.** A build file has a mistake, the build wants a program the ceiling
  does not allow, a state file is damaged. That is an *error*: nothing useful can be said about
  the targets, so no outcome is made for any of them.

Here are both, in a workspace that allows `grep`, `sort` and `wc`:

```lua
local b = buildl

b.target("names.txt", {
  inputs = { "names.in" },
  run = { "sort", "-o", "$out", "$in" },
})

b.target("bob.txt", {
  inputs = { "names.in" },
  run = { "grep", "bob", "$in" },
  stdout = true,
})

b.target("count.txt", {
  deps = { "bob.txt" },
  run = { "wc", "-l", "$deps" },
  stdout = true,
})
```

`names.in` has no `bob` in it, so `grep` exits 1:

```console
$ buildl build --keep-going
FAILED //:bob.txt: action failure (exit status 1)
    declared in build.lua
    command: grep bob names.in
1 failed, 1 skipped, 1 built
$ echo $?
1
$ python3 -c 'import json; [print(e["label"], json.dumps(e["class"])) for e in map(json.loads, open(".buildl/log.jsonl"))]'
//:bob.txt {"ActionFailure": {"status": 1}}
//:count.txt {"Skipped": {"because": "//:bob.txt"}}
//:names.txt "Built"
```

Three outcomes, one per target, and the log holds each class as data. Now add a fourth target that
runs `tr`, which the ceiling leaves out, and ask for a target nobody declared:

```console
$ buildl build
error: build.lua: //:shout.txt: program tr is outside the ceiling (proc.run allows: grep, sort, wc)
$ echo $?
4
$ buildl build nope
error: target //:nope was asked for but is not declared
$ echo $?
2
```

No target ran in either. The first is refused before anything starts, and the second before the
build files are even planned.

## Outcomes

Every target a build reaches gets one `ActionOutcome`, and its `class` field says how it ended. The
class is an `OutcomeClass`, from `crates/buildl-core/src/types/outcome.rs`:

```rust
#[non_exhaustive]
pub enum OutcomeClass {
    Cached,                                  // not run: the last build's result stands
    Built,                                   // ran, exited 0, every output stored
    Fetched,                                 // not run: the remote cache held the result
    Remote,                                  // not run here: the remote server ran it
    ActionFailure { status: i32 },           // exited with another status
    Killed { signal: i32 },                  // a signal ended it: no exit status
    OutputMissing { name: OutputName },      // exited 0, a declared output absent
    OutputRefused { name: OutputName, refusal: OutputRefusal },
    OutputWritten { name: OutputName },      // exited 0, a stdout target wrote its own path
    InfraFailure { kind: InfraKind },        // the builder could not build it
    Skipped { because: Label },              // never begun: another target failed
}
```

| Family | Classes | What it means |
|---|---|---|
| success | `Cached`, `Built`, `Fetched`, `Remote` | the target's outputs are in place (`OutcomeClass::succeeded` is true for these four) |
| the code is at fault | `ActionFailure`, `Killed`, `OutputMissing`, `OutputRefused`, `OutputWritten` | the action ran and did not do its job |
| the builder is at fault | `InfraFailure` | `buildl` could not prepare the action's directory (`Prepare`), start it (`Run`), let it finish in time (`TimedOut`) or store what it made (`Store`) |
| never begun | `Skipped` | a target it needs failed; `because` names that target by label |

A few of these need a word.

- `Killed` is its own class because a process a signal ended has no exit status, so
  `ActionFailure { status }` would have nothing true to hold. A timeout is not read off the
  signal: the worker that stopped the action knows it did, and says `InfraFailure { kind: TimedOut }`.
- `OutputRefused` is an action that exited 0 and left, where an output should be, something that
  cannot be put back exactly by placing files. `refusal` says what: a `Symlink`, something
  `Special` such as a socket, an `EmptyDirectory`, a name that is `NotText`, or, for a discovery
  target, whose output must be a file, a directory (`NotAFile`). `name` is the first such output
  in declared order.
- `OutputWritten` belongs to a target with `stdout = true`. Its output is what the command
  printed, so a command that also wrote that path itself has done something no one asked for.
- `Skipped` names the failed target by its `Label`, never by the graph's internal node number, so
  the log reads the same whatever order the graph was built in.

Classes are decided where the facts are known: whether the process could be started, what status
it exited with, whether the worker's timeout flag is set. Nothing downstream ever reads a message
to work out what happened. No failure is cached: a failed target leaves no cache row, so it runs
again next time.

## Errors

Everything that is not an outcome is an `Error`. There is one error type for the whole of
`buildl-core`, in `crates/buildl-core/src/error.rs`, written with `thiserror`. One type, not one
per module, because a caller branches on a variant and its fields, never on the text of a message,
and every module returns the same `Result` without a conversion at its edge.

`Error` is `#[non_exhaustive]`, so adding a variant breaks nobody downstream. Each variant carries
what a caller needs as data. A few of the ones you are most likely to meet:

| Variant | Raised when | Carries |
|---|---|---|
| `Refused` | a want is outside the ceiling, at Plan or at the last program check before a build | the want, the lowest-labelled target wanting it with its build file, and the ceiling |
| `PluginRefused` | a plugin's target wants something that plugin is not allowed, checked before the ceiling | the want, the target and its plugin, and what that plugin does allow |
| `NotApproved` | the approver said no to the wanted set | the wants it did not approve, and the reason in words |
| `PinnedEnv` | a target declares `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH` under a strategy that pins them | the variable, the value the sandbox sets, and the first target declaring it |
| `Nondeterministic` | `check` evaluated the build files twice and got two different lists | the build file, and what each run declared at the first difference |
| `DiscoveryChanged` | the load that read discovery results declared different discovery targets | the first such target and its build file |
| `UnknownTarget` | a command named a target nobody declared | the label, and the nearest declared one as a suggestion |
| `NoRun` | `log` was asked for a target with no logged run | the label, a suggestion, and the number of every damaged log line |
| `CorruptState` | a state file does not hold what `buildl` writes there | the file, and the JSON error that says why |
| `DispatcherFault` | the dispatcher answered success and left targets without an outcome | every such target, in label order |
| `LostBuildFiles` | the dispatcher answered success and left build files unevaluated | every such directory, in directory order |
| `PluginNotFetched`, `PluginChanged`, `PluginFetch` | a plugin is not there to use, its files are not the ones `buildl.lock` pinned, or `git` could not fetch it | the plugin, and what is missing or different |
| `UploadUnsandboxed` | uploading to the remote, or remote execution, was asked for while actions run `bare` | which of the two was asked for |
| `RemoteToken`, `RemoteSwitch`, `RemoteFlags` | the remote token cannot be used, a `BUILDL_REMOTE_*` variable is not `1` or `0`, or the remote flags contradict each other | the token's source (never the token), the variable and its value, or the conflict |

Policy refusals, `Refused`, `PluginRefused` and `NotApproved`, are errors and not outcomes on
purpose. They are raised while the build is planned, or by the approver right after, and always
before any action runs, so there is no target whose outcome could hold one.

### `Adapter`, and the port that failed

One variant is different: `Error::Adapter { port, diagnostic }`. It carries an adapter's own
failure as text, for display only, and `port` is a `PortName` saying which port's adapter failed:
`Manifest`, `ContentStore`, `ActionCache`, `RemoteCache`, `Approver`, `PluginStore`, `Changes` and
so on, one for each port. Every variant above is kept out of `Adapter` because a caller branches on
its fields; `Adapter` is only ever shown.

The pair `UnreadableJson` and `NonCanonicalJson` shows why the line matters. A storage adapter that
cannot read `cache.json` at all returns `Error::Adapter`. One that read it and found bytes the
canonical reader refuses returns `CorruptState`, naming the file and wrapping the JSON error. The
first is the machine's trouble, the second a damaged file, and they end with different exit codes.

### What a message looks like

Every error that can name a target carries its label and its *provenance*, the build file that
declared it. A plugin's target also carries its origin, the chain of plugins that declared it. The
`Display` of an error renders that the way airsl renders a refusal: what was wanted, what was
allowed or found, and where it was declared, as in the `tr` example above.

## Exit codes

`buildl-cli` turns all of this into one of eight exit codes, in `crates/buildl-cli/src/exit.rs`.
Four functions pick the code: `Exit::of_class` for one outcome, `Exit::of_outcomes` for a whole
build, `Exit::of_verification` for `verify`, and `Exit::of_error` for an error. Each one matches on
the variant, never on text.

| Code | Name | What lands there |
|---|---|---|
| 0 | `Success` | every outcome succeeded, or was skipped because of a failure that decides the code itself |
| 1 | `ActionFailure` | an action failed: `ActionFailure`, `Killed`, `OutputMissing`, `OutputRefused`, `OutputWritten`; a `verify` mismatch |
| 2 | `Usage` | the command line: an unknown target or override, a bad query, `log` with no run, a test run naming a target that is not a test, the remote switches and flags |
| 3 | `Infrastructure` | the builder or the machine: `InfraFailure`, a missing program, a missing blob, a failed plugin fetch, and `Adapter` for every port but `Manifest` |
| 4 | `Refused` | `Refused`, `PluginRefused`, `NotApproved` |
| 5 | `Definition` | a build file or `buildl.toml` is wrong, including `Adapter` for the `Manifest` port |
| 6 | `CorruptState` | `CorruptState`, the two JSON variants, `PluginChanged` |
| 7 | `Internal` | `buildl` itself is at fault: a dispatcher that lost work, and any class or variant this binary has no row for |

When a build has outcomes of several kinds, the most severe decides: a fault of `buildl` (7)
outranks one of the builder (3), which outranks a failed action (1). A sick builder means the run
says nothing reliable about the code. The numbers stay clear of 126, 127 and everything above 128,
which a shell gives meanings of its own, and the status an action exited with is never the
process's own: it is in the report of the failure.

[Appendix D](appendix-04-exit-codes.md) lists every code with every variant that leads to it.
