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
- **A crash never leaves a cache row pointing at nothing.** Content goes into the cas first, the
  cache row after, and failures are never cached.
- **Nothing on the ledger is hidden.** Every input that can change an action's result is in the
  [explicitness ledger](ch11-05-determinism-rules.md#the-explicitness-ledger), and a difference
  between two builds that is not traceable to it is a bug in `buildl`, not in your build.

## No per-file C and C++ in this version

`buildl` does not support compiling C or C++ one file at a time, with one target per `.o` file.
The reason is the *depfile gap*: a compiler reads headers nobody declared, Ninja and Make learn
about them from a *depfile* the compiler writes, and `buildl` reads no depfiles.
[Discovery Targets](ch07-00-discovery-targets.md#the-depfile-gap) draws the missing edge, and what
each strategy does with it: under `bare` an edited header is silently ignored, and under `sandbox`
the first compile fails because the header is not there.

Two findings settled this, and neither is a gap in general:

1. **It is one build pattern in one stack.** Every other toolchain tracks its own dependencies
   inside itself, which is why `go build`, `cargo build` and their like are each wrapped as one
   action over a full glob of their sources. That shape is sound, and needs no depfile.
2. **`buildl` already has an answer to finding inputs at build time.** A
   [discovery target](ch07-00-discovery-targets.md) is an ordinary action, keyed, cached,
   sandboxed and approved, whose output the next pass over the build files reads. A depfile is the
   executor reading a file a compiler happened to drop; a discovery target is a node in the graph
   with a key, a grant and a line in `plan`, held to the same ledger as everything else.

What is really left is narrower than "depfiles": finding an input *in the middle of one action*,
which a discovery target does not cover, because its output feeds the next declaration pass and
not the action running now. If that case is ever needed, it will extend what a discovery target
can do, rather than add a side channel to the executor. Ninja's depfile format would then be one
way to feed it, not the design. No branch of this decision changes what a build file can write.

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
- **Whether a tool is reproducible.** `buildl` pins the inputs, the time zone, the locale, the
  build time and the sandbox's directory. A tool that still gives different bytes from the same
  inputs is the tool's property. `verify` finds it and names it; nothing fixes it.
- **The settings left out on purpose.** `workers`, `timeout`, `strategy` and a rule's `desc` are
  in no key. Only `strategy` can change an action's bytes: under `sandbox` the action runs in
  another directory with pinned values, so a tool that records its directory or reads the clock
  can give other bytes than under `bare`. A compile with debug information is one such tool.

Outside those, a difference between two builds that the ledger cannot explain is not your build's
fault. It is a bug in `buildl`.
