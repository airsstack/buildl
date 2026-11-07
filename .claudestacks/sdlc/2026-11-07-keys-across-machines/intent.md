---
status: approved
created: 2026-11-07
---

# Intent: The same target has the same key on every machine

## Problem

A target that reads an environment variable lists it in `env`, and the action key holds that
variable's value (`crates/buildl-core/src/types/action_key.rs:87-88`: "Each environment variable it
reads, with its value"). Some variables are read only to find programs or their files, not to
change what is built: `PATH` finds `cargo`, `rustc`, `cc`, `go`, `sh`; `HOME` lets `rustup` and
`cargo` find `~/.rustup` and `~/.cargo`. Their values hold the user's name and the machine's
layout, so they differ between people, CI runners and machines even when the build is the same.

| Machine | `HOME` | Key of `//:app` |
|---|---|---|
| CI | `/home/runner` | K1 |
| an engineer | `/home/alice` | K2 |

The engineer's build asks the remote cache for K2; CI stored K1; nothing is served, and the
engineer builds every target again. The advice the book gives for a team ("CI uploads, everyone
fetches", `book/src/ch09-00-remote.md`, section "Who should upload") then works only between
machines with the same values.

Evidence, 2026-11-07, buildl 0.1.0, one machine: in a workspace whose two targets declare
`env = { "HOME", "PATH" }` and are cached, `buildl plan` prints `clean` for both; with one extra
folder put in front of `PATH` it prints
`run //:app: environment variable PATH changed` and `run //:deps.tar: environment variable PATH changed`.
An earlier remote cache probe (chain `2026-11-06-where-actions-run`) fetched across workspaces only
because all of them ran as one user, with one `HOME` and one `PATH`.

The remote cache names an action by a second digest, of the REAPI `Action`, which holds the
`Command` (its words, each variable with its value, the output paths), the input root, the
platform, a salt of the action key, and the action's timeout
(`crates/buildl/src/remote/encode.rs:105-133`, `178-188`), so `HOME` and `PATH` split it too.
Evidence, 2026-11-07, bazel-remote 2.6.4 on localhost, three copies of one workspace in other
folders with their own `TMPDIR`: the first built with `--remote-upload`; the second, same
`buildl.toml`, printed `2 fetched`, so the workspace's folder and the temporary directory are in
neither key.

The third copy, with only `timeout = "20m"` added to `[execution]`, printed `2 built` (the cache
logging `AC GET … NOT FOUND`). That one stays, by the author's decision: REAPI puts the timeout in
the `Action` on purpose ("two `Actions` with different timeouts are different … hiding the fact
that the timeout is too short", `remote_execution.proto:706-714`), and the value comes only from
the committed `buildl.toml` (`crates/buildl/src/manifest/interpret.rs:327`; no command-line flag
or `BUILDL_` variable sets it), so every clone of a repository has the same one. The book says the
timeout is in no key (`book/src/ch11-09-what-it-promises.md`, "The settings left out on purpose"),
which is true of the local key only.

A build file cannot read the environment, but a discovery target's output reaches it: a tool that
prints an absolute path of the machine (a home folder, the sandbox folder) into what a plugin then
declares puts that path into keys, and nothing tells the plugin's author.

Every target that runs a toolchain reads `PATH`: the `c` plugin (`plugins/c/extension.toml`), the
examples `go`, `go-discovery`, `rust`, `polyglot`, `plugins`, `c`, `cpp`, and the planned `rust`
plugin (`2026-11-06-rust-plugin`), whose value for teams rests on sharing its dependency layer
through the remote cache.

## Affected systems

- `buildl-core`: the action key and what `plan` says changed.
- `buildl-lua`: how a build file and a plugin declare the variables a target reads.
- `buildl`: the input sandbox (which variables a command is given), the remote cache and remote
  execution (the variables sent with an action).
- `buildl.toml`'s ceiling and plugin tables (`env.read`), plugin manifests (`extension.toml`), the
  approver and `doctor` where they list variables.
- `plugins/c`, the examples, the book (`ch05-01-action-key.md`, `ch06-00-sandbox.md`,
  `ch09-00-remote.md`, `ch11-05-determinism-rules.md`, `ch11-09-what-it-promises.md`,
  `appendix-02-buildl-toml.md`, `appendix-03-lua-api.md`), the skills, `README.md`.

## Desired outcome

- Two machines of the same operating system and processor that build the same target from the same
  inputs with the same tools compute the same key, whoever the user is and wherever their home and
  tools are installed; so a result uploaded by one is fetched by the other.
- A result is never served for another tool: a change of the program that runs, or of what a
  variable that does change the output holds, still changes the key.
- The remote cache's name for an action follows the same rule for those variables; the timeout
  stays in it, and the book says so.
- `plan` still explains every change of key.
- It cannot come back unseen: a test holds the key, and the remote name, of one target equal across
  two machines that differ in user, home folder, `PATH`, workspace folder and temporary directory,
  and differ when a tool, an input or the timeout changes.
- A plugin author learns when a machine's absolute path reaches what a target declares.
- The `c` plugin and the examples get the sharing without losing correctness.

## Constraints

- Determinism rules hold (`book/src/ch11-05-determinism-rules.md`); nothing ambient enters a
  command: a command still sees only variables its target names.
- The ceiling and the approver still show and limit every variable a target reads.
- buildl is unpublished: the key, the cache format and the build file API may change without
  keeping an earlier form.
- Every example the book prints is captured from a real run.

## Non-goals

- The sandbox folder written into some outputs (settled in `2026-11-06-where-actions-run`: each
  workspace keeps its own folder; one builder uploads).
- Sharing between different operating systems or processors.
- The `rust` plugin itself (`2026-11-06-rust-plugin`), which builds on this chain.
