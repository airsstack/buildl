---
name: buildl
description: Write, edit and debug buildl build files (build.lua) and buildl.toml, and read
  what buildl reports. Use when a project has a build.lua or buildl.toml, when the user asks
  to build, test or set up a project with buildl, or when a buildl command fails or refuses.
license: Apache-2.0
compatibility: Requires the buildl command on PATH.
---

# buildl

## A. How buildl works

`build.lua` only declares targets. buildl runs them. Nothing is allowed by default:

| Rule | Where it is set |
|---|---|
| a command sees only the environment variables its target lists in `env` | `build.lua` |
| every program a command runs must be in `proc.run`, except a program a dependency built | `buildl.toml` `[ceiling]` |
| what a command reads and writes must lie within `fs.read` and `fs.write` | `buildl.toml` `[ceiling]` |
| a plugin's targets are also held to the plugin's own `extension.toml` and the workspace's `[plugins.<name>]` cap | `extension.toml`, `buildl.toml` |
| then the approver agrees to what the build wants | `buildl.toml` `[approver]` |

A command never holds a real path. It uses placeholders: `$in`, `$out`, `$deps`, `$opt:name`.

## B. The working loop

```text
 edit build.lua / buildl.toml
        |
        v
 buildl check ---- exit 5 --> fix the file at the line the error names, check again
        | 0       (check reads declarations only: it does not apply the ceiling)
        v
 buildl plan       what would rebuild, and why
        |
        v
 buildl build ---- exit 1 --> buildl log <target>, then fix the code being built
        | 0        exit 4 --> buildl doctor, then explain and ask the user (D)
        v
 buildl test       handled the same way
```

`buildl plan --json` holds, per target, its `verdict`, its `reason` and its key's `components`,
and for the build as a whole `wanted`. A plain `plan` leaves tests out; name a test to plan it.

## C. What to do for each exit code

| Code | Name | The agent |
|---|---|---|
| 0 | Success | is done |
| 1 | ActionFailure | runs `buildl log <target>` and fixes the code being built |
| 2 | Usage | fixes the command line it typed |
| 3 | Infrastructure | reads the error. A missing program or a failed plugin fetch is reported to the user: a retry cannot help. Any other cause: retry once, then report |
| 4 | Refused | runs `buildl doctor`, tells the user which of the three refusals it is (D), and asks |
| 5 | Definition | fixes `build.lua` or `buildl.toml` at the line the error names, then runs `check` again |
| 6 | CorruptState | stops and reports. Does not delete `.buildl` without asking |
| 7 | Internal | stops and reports a buildl bug |

The error text of each code is in `references/exit-codes.md`.

## D. Rules never broken

| Never | Why |
|---|---|
| add to `[ceiling]`, to a `[plugins.<name>]` cap or to a plugin's `extension.toml` without the user's yes to that exact change | what a build may touch is the user's decision |
| change `[approver]` to get past a refusal | approval is the user's |
| set `strategy = "bare"` to get past a sandbox failure | it drops the sandbox. The fix is a missing declared input |
| delete an old build, `out` or `.buildl` without asking | it cannot be undone |

Exit 4 has three causes. Tell them apart by the error line and `buildl doctor`:

| Cause | How it shows | What to tell the user |
|---|---|---|
| the ceiling | `doctor` lists a want as `REFUSED` in its `ceiling` column. The error names the program, variable or directory and the ceiling key | the quoted line, the key and value that would allow it, and the question below |
| a plugin | the error names a plugin in brackets, `(plugin <name>@<version>)`, and says the want is outside that plugin | the quoted line. Say whether the plugin's own manifest or the workspace's `[plugins.<name>]` cap stops it. Ask about the cap. The manifest is the plugin author's: offer no edit to it |
| the approver | the ceiling and the plugins allow every want, and the error is the approver's, such as `no terminal to ask on`, or `[approver]` is `deny` | the approver refused. With `interactive`, the user runs the build once in their own terminal to approve it. With `deny`, the choice of approver is theirs |

For the ceiling or a cap, quote buildl's line and ask. On a yes, make exactly that change and nothing
more. Without a yes, change nothing. The question looks like this:

```text
buildl refused the build:
  //:x.txt: program sh is outside the ceiling (proc.run allows: python3)
To allow it, buildl.toml's [ceiling] proc.run needs "sh". Should I add it?
```

## E. Common mistakes

| Mistake | What buildl does |
|---|---|
| a misspelt option key, `input` for `inputs` | exit 5, `unknown field 'input'`, with the line |
| `$input` for `$in` | not a placeholder: the command gets the text `$input` unchanged |
| a variable used but not listed in `env` | the command does not see it |
| a program used but not in `proc.run` | `check` passes. `build` exits 4 |
| an `inputs` file that does not exist | `check` passes. `build` exits 5, `input <file> does not exist` |
| `print` in `build.lua` | `print` is removed. `error("text", 0)` shows a value |

A mistake shown on purpose is fenced `lua no-check`, because it is not a good build file:

```lua no-check
-- wrong on purpose: "input" is not an option key, "inputs" is
local b = buildl
b.target("x.txt", { input = { "x.in" }, run = { "cp", "$input", "$out" } })
```

## F. Starting a project

A first `build.lua` and `buildl.toml` that build one file and test it:

```lua
local b = buildl

local sources = b.sources("src/*.txt")

b.target("all.txt", {
  inputs = sources,
  run = { "cat", "$in" },
  stdout = true,
})

b.test("has_text", {
  deps = { ":all.txt" },
  run = { "grep", "-q", "text", "$deps" },
})
```

```toml
[workspace]
name   = "starter"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["cat", "grep"]
env.read = []
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

The ceiling lists every program the targets run, every variable they read, and always
`fs.write = ["out", ".buildl"]`. Swap the target's `run` for the language:

| Language | `run`, and what else the target needs |
|---|---|
| Go | `{ "go", "build", "-o", "$out", "." }` with `env = { "HOME" }` |
| Rust | `{ "cargo", "install", "--path", ".", "--root", "$out", "--target-dir", "tt", "--no-track" }` with `env = { "HOME", "PATH" }`. Without `PATH` the build stops with ``error: linker `cc` not found`` |
| Python | `{ "python3", "-m", "zipapp", "app", "-o", "$out" }` |

Every language has one target over all its sources. Do not make one target per `.o` file.

## References

| File | Read it when |
|---|---|
| `references/lua-api.md` | you write or change a function call in `build.lua` |
| `references/buildl-toml.md` | you write or change `buildl.toml` |
| `references/exit-codes.md` | a command fails and you need the error text of its code |
| `references/plugins.md` | the project uses `buildl.use`, an `extension.toml` or `buildl plugins fetch` |
