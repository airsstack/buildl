# What buildl Can and Cannot Promise

A build tool is only as useful as your knowledge of where its promises stop. Here they are.

## What it does promise

- **A build file can be read safely before you trust it.** Evaluating build files grants them
  nothing: no files, no processes, no environment, no clock.
- **You can see everything a build may do before it does it.** The programs, variables, network
  use and folders a build wants are worked out from the graph, held to the ceiling in
  `buildl.toml`, and shown to the approver before any action runs.
- **The same declared inputs run the same action.** That is what the action key is, and `plan`
  can always say what will rebuild and why.
- **A machine's own folders stay out of the key.** A variable that only finds programs is marked
  `unkeyed`, and its key holds the name alone. A command, a `key` variable, a setting or a discovery
  output that holds this machine's home, workspace or action's folder is refused, so users and CI can
  share a cache ([The Action Key](ch05-01-action-key.md#variables-that-only-find-files)).
- **A crash never leaves a cache row pointing at nothing.** Content goes into the cas first, the
  cache row after, and failures are never cached.
- **Nothing on the ledger is hidden.** Every input that can change an action's result is in the
  [explicitness ledger](ch11-05-determinism-rules.md#the-explicitness-ledger), and a difference
  between two builds that is not traceable to it is a bug in `buildl`, not in your build.

## Per-file C and C++ goes through the plugin

`buildl` compiles C and C++ one file at a time, with one target per object, through the `c`
plugin. [The C and C++ Plugin](ch08-04-the-c-plugin.md) shows how. The reason it needs discovery targets
is the *depfile gap*: a compiler reads headers nobody declared, Ninja and Make learn about them from
a *depfile* the compiler writes, and `buildl` reads no depfiles.
[Discovery Targets](ch07-00-discovery-targets.md#the-depfile-gap) draws the missing edge, and what
each strategy does with it: under `bare` an edited header is silently ignored, and under `sandbox`
the first compile fails because the header is not there.

The plugin closes the gap with a [discovery target](ch07-00-discovery-targets.md) per source. It is
an ordinary action, keyed, cached, sandboxed and approved, that runs the compiler with `-MM` and
whose output the next pass over the build files reads. Each object is then declared with the source
and the headers it read as inputs. The executor still reads no depfile, and none of this
changes what the sandbox or the ledger promise.

Its limits:

- **One build file per project.** A header outside the build file's folder cannot be declared as an
  input, so a source may include only headers inside the build file's folder and listed in
  `headers`, or ones another target generates and the call names in `header_deps`. Anything else is
  an error naming the header.
- **Linux and macOS, with `gcc` and `clang`.** Windows and MSVC are out, and so is
  cross-compiling. The programs the plugin may run are a fixed list, and a versioned name such as
  `gcc-13` is not on it.
- **A compiler update behind a launcher goes unseen.** A target's key records the program file, and
  on macOS `/usr/bin/cc` is one launcher file that stays the same when the compiler behind it
  changes. Unless a call sets `version_probe = true`, a new compiler does not rebuild the objects.
  With it, a probe runs `--version` and every object depends on its output. `plan` always lists
  those compiles as conditional, even right after a full build, because it never runs the probe; a
  build still takes them from the cache when the probe prints the same.

What is really left of the gap is narrower than "depfiles": finding an input *in the middle of one
action*, which a discovery target does not cover, because its output feeds the next declaration
pass and not the action running now. If that case is ever needed, it will extend what a discovery
target can do, rather than add a side channel to the executor. Ninja's depfile format would then be
one way to feed it, not the design.

## Rust goes through the plugin too

`buildl` builds a Rust crate through the `rust` plugin, which runs plain `cargo`. There is no depfile
gap here, since cargo tracks the sources it reads, and so there is no discovery per file: a call
declares one target for the crate, with every file below the build file's folder as an input.
[The Rust Plugin](ch08-05-the-rust-plugin.md) shows how. Its limits:

- **Dependencies compile again whenever an input changes.** Each build runs in a fresh sandbox, so
  cargo has no `target/` from the last one. Plain `cargo` keeps its own `target/` and is faster for
  an edit loop on one crate.
- **Crates live in cargo's cache, outside buildl's store.** Cargo finds them under `HOME`
  (`~/.cargo`), which is `unkeyed`, so a crate a machine already holds is not part of the key.
- **Programs cargo starts are unseen.** The ceiling sees the `sh` that started cargo, not the
  `rustc`, the linker or the build scripts cargo runs beneath it.
- **The dev profile does not verify.** Its debug info holds the absolute path of the folder the
  crate was built in, and `buildl verify` runs the action in a folder of its own, so it reports a
  `dev` binary as differing. The examples build with `profile = "release"`, which holds no path.
- **A toolchain update goes unseen**, with `rustup` or without. Each target runs `sh`, so its key
  records neither `cargo` nor `rustc`. Unless a call sets `version_probe = true`, a new toolchain
  does not rebuild the target; with it, `plan` lists the target as conditional even after a full
  build.

Linux and macOS only, and no cross-compiling.

## What each isolation tier does not guarantee

[The Sandbox](ch06-00-sandbox.md) shows the tiers in use. Here is what each one leaves open.

**Tier 1, `bare`, confines nothing.** It does promise five things: the command is started from an
argument list, never through a shell; the program run is the file the plan fingerprinted; the
environment holds exactly the declared variables that have a value; standard input is empty; and
the action's own process is stopped at the timeout. Beyond that, an action can:

- read any file you can, declared or not, so a hidden input is silent instead of an error;
- reach the network, whatever `network` says;
- write anywhere you can, another target's output included;
- leave the processes it started running, at a timeout or when it exits;
- leave a partial output at its path if it fails or is stopped. No cache row names such a file,
  and the next build clears the path before running the action again;
- damage the store, if it changes the mode of a dependency's read-only output and writes to it.

The machine's clock, time zone and locale are the action's own.

**Tier 2, `sandbox`, stops one thing: reading a workspace file the action did not declare.** An
action that names a workspace file by its absolute path, or climbs out of its directory with `..`,
still reaches the workspace. It reads `/usr`, your home folder and tool caches as at tier 1. It
reaches the network, and the processes it starts outlive it. When it does read an undeclared file,
it fails with the tool's own message, "file not found" or "cannot find module", and `buildl` does
not name the file. And because the strategy is not part of the action key, a target built under
`bare` is a cache hit under `sandbox`: switching does not re-check it. `buildl verify` is how a
cached target is checked.

**Tiers 3 and 4, `os-sandbox` and `container`, are not built.** They are the tiers meant to seal
the network and system files, and to pin the toolchain in an image. This version refuses them
rather than run your actions with less isolation than you asked for.

## The limits of the ledger

The ledger is finite and named, and some things are honestly outside it:

- **What the machine is.** The kernel version, the C library, the file system's behaviour, and the
  exact processor as it affects generated code are not in any key. Two machines with the same
  operating system and processor family share cache entries even when these differ. Containers,
  tier 4, and `verify` are what exist to contain them, and only `verify` is built.
- **A folder the check cannot see.** `buildl` searches for this machine's folders in the places a
  text enters a key. It does not search an `unkeyed` value, which is in no key, and it does not
  name the programs a command starts by itself, such as the linker a compiler runs. A tool that writes a folder into its own output is under
  *Where the workspace is*, below.
- **Whether a tool is reproducible.** `buildl` pins the inputs, the time zone, the locale and the
  build time, and an action's folder has the same path on every run of one workspace, as long as the
  temporary directory stays the same. A tool that still gives different bytes from the same inputs
  is the tool's property. `verify` finds it and names it; nothing fixes it.
- **Where the workspace is.** The action's folder has a different path in another workspace, a git
  worktree included, and on a machine whose workspace path or temporary directory differs. That is
  by design: no two workspaces share a folder or wait on one. A tool that writes the folder into its
  output, as rustc does for a file included from `OUT_DIR` or a compiler does for the build
  directory in debug information, gives other bytes there. That difference is explained; it is not
  a bug in `buildl`. The result is never wrong: a cache only reuses less
  (see [Who should upload](ch09-00-remote.md#who-should-upload)).
- **The settings left out on purpose.** `workers`, `timeout`, `strategy` and a rule's `desc` are
  in no local key. The `timeout` is in the remote key, not the local one: a remote server names an
  action by its timeout as well, so a copy with another `timeout` in `buildl.toml` fetches nothing
  the first uploaded ([Who should upload](ch09-00-remote.md#who-should-upload)). Only `strategy`
  can change an action's bytes: under `sandbox` the action runs in another directory with pinned
  values, so a tool that records its directory or reads the clock can give other bytes than under
  `bare`. A compile with debug information is one such tool.

Outside those, a difference between two builds that the ledger cannot explain is not your build's
fault. It is a bug in `buildl`.
