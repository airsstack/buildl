---
status: approved
created: 2026-11-07
built-on: 2a7ab3b52357
---

# Spec: The same target has the same key on every machine

A target marks each environment variable it reads `key` or `unkeyed`. A `key` variable works as
every variable works today: its value is given to the command and folded into the action key. An
`unkeyed` variable's value is given to the command but not to the key, and is not sent to a remote
server; only its name is in the key. `buildl.toml` and a plugin's manifest must allow every
`unkeyed` name, and `plan`, `doctor` and the approver list them apart. So `HOME` and `PATH`, which
only find programs, stop splitting the key between users and machines. buildl also refuses a target
whose command, `key` variable or discovery output holds this machine's own folders, the other way
a machine reaches a key. The remote timeout stays in the remote key, and the book says so.

## 1. Premises, as the code stands

| # | Fact | Proof |
|---|---|---|
| P1 | The key holds each declared variable with its value | `crates/buildl-core/src/types/action_key.rs:87-88` ("Each environment variable it reads, with its value"); gathered by `crates/buildl-core/src/plan/components.rs:28-37` through `Host::env` (`crates/buildl-core/src/ports/host.rs:29`, adapter `crates/buildl/src/host/local_host.rs:60-61`) |
| P2 | A different `PATH` makes every target that declares it run again | probe 2026-11-07, buildl 0.1.0, a cached workspace whose two targets declare `env = { "HOME", "PATH" }`: `buildl plan` prints `clean  //:app`, `clean  //:deps.tar`; with `PATH="/opt/other/bin:$PATH"` it prints `run    //:app: environment variable PATH changed`, `run    //:deps.tar: environment variable PATH changed` |
| P3 | The remote `Command` holds each variable with its value, and the remote `Action` is salted with the action key | `crates/buildl/src/remote/encode.rs:105-125` (`environment_variables` from `action.env`), `:173-188` ("The salt is the 32 bytes of the target's action key"); `RemoteAction.env` is the ready action's env plus the pinned three, `crates/buildl-core/src/execute/remote_action.rs:46-47` |
| P4 | The workspace's folder and the temporary directory are in neither key | probe 2026-11-07, bazel-remote 2.6.4 on localhost: copy 1 of a workspace built with `--remote-upload` (`1 built, 1 fetched`); copy 2, another folder and `TMPDIR`, same `buildl.toml`, printed `2 fetched` |
| P5 | The remote `Action` holds the timeout, on purpose | `crates/buildl/proto/build/bazel/remote/execution/v2/remote_execution.proto:706-714` ("two `Actions` with different timeouts are different … hiding the fact that the timeout is too short"); encoded at `crates/buildl/src/remote/encode.rs:184`; probe: copy 3 with `timeout = "20m"` printed `2 built`, cache log `AC GET … NOT FOUND` |
| P6 | The timeout comes only from `buildl.toml` | read at `crates/buildl/src/manifest/interpret.rs:327` (`"execution.timeout"`); `buildl build`, `test`, `verify`, `watch --help` each have 0 lines with `timeout` (control: `build --help` has 12 with `remote`); the `BUILDL_` variables read are `REMOTE_EXECUTE`, `REMOTE_STRICT`, `REMOTE_TOKEN`, `REMOTE_UPLOAD` (grep of `crates/buildl-cli/src`, `crates/buildl/src`) |
| P7 | A tool is identified by its file's bytes, not its path | `crates/buildl/src/tools/path_tools.rs:3-5` ("the bytes of the file its name leads to, links followed. No path, size or time is part of it") |
| P8 | A command is given only the variables its target declares, plus the pinned three under the sandbox | `crates/buildl/src/exec/run.rs:61-68` (`env_clear().envs(action.env)`), `crates/buildl/src/exec/sandbox.rs:182-186`, `crates/buildl-core/src/types/isolation.rs:84-90` |
| P9 | A remote server runs a command with no `PATH` in its `Command` | `examples/remote/buildl.toml:9` (`env.read = []`), `examples/remote/build.lua:8` runs `python3`; `examples/remote/assert.sh:57` runs it with `--remote-execute`; CI run 41185308111 on `main`, job "Remote example on NativeLink": success. REAPI: "If the `environment_variables` list contains an entry for the PATH environment variable, it SHOULD be respected. If not, the resolution process is implementation-defined" (`remote_execution.proto:766-769`) |
| P10 | Today a command may hold this machine's home folder | probe 2026-11-07: a target `run = { "sh", "-c", "echo /Users/hiraq/x > $out" }`: `buildl check` prints `ok: 1 declaration in 1 build file`, exit 0; `buildl plan` prints `run    //:note: never built` |
| P11 | `cargo metadata` prints absolute paths | `cargo metadata --no-deps --format-version 1` on a project under the scratchpad: `manifest_path: /private/tmp/claude-501/…/tc/app/Cargo.toml`, `src_path: …/tc/app/src/main.rs`; its `workspace_root` field is the folder followed by `"` |
| P12 | Inputs and outputs are workspace-relative by type | `crates/buildl-core/src/types/source_path.rs:1` ("A workspace-relative path to a source file, validated at construction"); outputs are `OutputName` (`action_key.rs:86`) |
| P13 | The env list is parsed in one place for `target`, `test` and `discover`; `rule` takes no `env` | `crates/buildl-lua/src/primitives.rs:24-36` (field lists; `RULE_FIELDS` = `run`, `desc`), `:77` (`optional_list(…, "env")`) into `Declaration.env: Vec<EnvName>` (`crates/buildl-core/src/types/declaration.rs:98`) |
| P14 | `env.read` exists at three levels, intersected | ceiling `crates/buildl/src/manifest/interpret.rs:311-313`, plugin table `:118-120`, `extension.toml` `crates/buildl-lua/src/plugin/manifest.rs:98-107`; checked in `crates/buildl-core/src/grants/plugins.rs:64-68` and `crates/buildl-core/src/plan/authority.rs:82` |
| P15 | A build file's or `buildl.toml`'s fault exits 5, a refusal 4 | `book/src/appendix-04-exit-codes.md` table (`4 Refused`, `5 Definition`) |
| P16 | The interactive approver asks only for wants it has not remembered, then remembers their union; the manifest approver approves everything inside the ceiling | `crates/buildl/src/approver/interactive.rs:71-72` (`wanted.minus(&remembered)`), `:100` (`remembered.union(wanted)`); `crates/buildl/src/approver/manifest.rs:3-5` ("every build inside it is approved") |
| P17 | `watch` reports a broken build file and goes on waiting | `book/src/ch10-00-watch.md:63` ("`build.lua` broken: the build fails and prints what `build` prints; the watch goes on waiting") |
| P18 | The commands that load the build files | `crates/buildl-cli/src/main.rs:13-20` (the command table: `check`, `graph`, `query`, `plan`, `build`, `test`, `verify`, `watch`, `doctor`) |
| P19 | Today's discovery outputs and build files hold no machine folder | probe 2026-11-07: `cc -MM -MT obj -Iinclude lib/util.c` in `examples/c` prints `obj: lib/util.c include/util.h` (the c plugin passes the relative path, `plugins/c/graph.lua:150-155`); `cc -dumpmachine` prints `arm64-apple-darwin25.3.0` (`plugins/c/api.lua:90`); `go list -json=ImportPath ./...` in `examples/go-discovery` prints `{"ImportPath": "example.com/discovery/greet"}` and `…/mathx`; a search of `examples/*.lua` and `plugins/*.lua` (tests left out) for `"/Users`, `"/home`, `"/tmp`, `"/private`, `"/var` finds nothing (exit 1), while the same search for `"sh"` in `plugins/c/graph.lua` finds 2 |
| P20 | A result is stored before its cache row, and failures are never cached | `CLAUDE.md`, "Core architecture": "cas first, cache row after, log always — failures are never cached" |
| P21 | The test port stub cannot model two machines | `crates/buildl-core/src/ports/port_set.rs:156` is a `#[cfg(test)]` stub whose `env` returns `None` for every name; `Host` has only `platform` and `env` (`crates/buildl-core/src/ports/host.rs:29`) |

## 2. Declaring a variable

`env` on `target`, `test` and `discover` becomes a table from a variable's name to its mark:

```lua
b.target("app", {
  run = { "cargo", "build" },
  env = { RUSTFLAGS = "key", HOME = "unkeyed", PATH = "unkeyed" },
})
```

| Mark | Given to the command | In the action key | Sent to a remote server |
|---|---|---|---|
| `"key"` | its value | name and value | name and value |
| `"unkeyed"` | its value | name only | no |

- Every variable is marked; there is no default mark.
- `env = {}`, or `env` left out, declares no variable, as today.
- A list, `env = { "HOME" }`, and a table that mixes the two, `env = { "HOME", PATH = "unkeyed" }`,
  are refused (E1).
- A mark that is not the string `"key"` or `"unkeyed"`, such as `"pass"` or `true`, is refused (E2).
- A name that is not a string, or not a valid variable name, is refused as a bad name is today (the
  `EnvName` check, `crates/buildl-core/src/types/env_name.rs:43`) (E9).
- A pinned name (`TZ`, `LC_ALL`, `SOURCE_DATE_EPOCH`) is refused with either mark, as today
  (`crates/buildl-core/src/plan/pinned.rs:26-37`) (E8).
- The names are held sorted, so the order a table is written in reaches no output
  (`book/src/ch11-05-determinism-rules.md`).
- An `unkeyed` variable unset on this machine is not given to the command, and the key is the same
  as when it is set: it holds the name only. A `key` variable that is unset is in the key as unset,
  as today (`action_key.rs:87`, "`None` when the variable is unset").

**Where the mark travels.** `Declaration.env` (P13) becomes a map from name to mark, and so do the
graph node's `env` (`crates/buildl-core/src/types/target_graph.rs:50`) and the ready action's
(`crates/buildl-core/src/types/ready_action.rs:38`), which holds every variable's value, both
marks, for the local executors (P8). `RemoteAction.env` is built from the `key` variables and the
pinned three only (today all of `ready.env`, `crates/buildl-core/src/execute/remote_action.rs:46-47`),
so `encode::command` never sees an `unkeyed` value.

## 3. Allowing a variable to be unkeyed

Beside each `env.read` there is an `env.unkeyed`, the names that may be marked `unkeyed`. A list
left out is empty, so nothing may be unkeyed unless every level says so.

```toml
[ceiling]
env.read    = ["RUSTFLAGS", "HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]

[plugins.c]
env.read    = ["PATH"]
env.unkeyed = ["PATH"]
```

```toml
# plugins/c/extension.toml
[capabilities]
env.read    = ["PATH"]
env.unkeyed = ["PATH"]
```

- At each level `env.unkeyed` narrows `env.read`: a name in `env.unkeyed` that is not in the same
  table's `env.read` is refused when the file is read (E4).
- **Wants.** The wanted set gains `unkeyed`, the names some target reads marked `unkeyed`, beside
  `env` (`crates/buildl-core/src/types/authority.rs:28-29`), which keeps the names read marked
  `key`. A new `Want::Unkeyed` (cap key `env.unkeyed`) sits beside `Want::Env`
  (`crates/buildl-core/src/types/plugin.rs:311`).
- **Checks.** Each name in `env` must be in the ceiling's `env.read`, as today; each in `unkeyed`
  must be in the ceiling's `env.unkeyed`, so also in its `env.read`. A plugin's target is held to the
  plugin table's and the manifest's lists the same way, the intersection `env.read` has today
  (`grants/plugins.rs:64-68`) (E3).
- A variable marked `key` in one target and `unkeyed` in another is in both sets, and each is
  checked against its own list.
- `env.unkeyed` permits leaving a value out of the key; it does not require it: a name in both lists
  may still be marked `key`.
- **Approvers.** The interactive approver asks for wants it has not remembered (P16), so a
  remembered yes for reading `HOME` does not cover `HOME` unkeyed: it asks once more, naming it as
  an unkeyed variable, and remembers it. The manifest approver approves everything inside the
  ceiling, unchanged (P16).

## 4. The key and what `plan` says

- `OwnComponents.env` holds each `key` variable with its value, as today, and a new field holds the
  sorted names of the `unkeyed` variables.
- So any change to the set of names or to their marks changes the key, and `plan` names it:

| Change | `plan` prints |
|---|---|
| a `key` variable's value | `environment variable X changed` (today, `crates/buildl/src/report/plan_text.rs:26`) |
| a `key` variable added or removed | `environment variable X changed`, as today |
| an `unkeyed` variable added | `unkeyed variable X added` |
| an `unkeyed` variable removed | `unkeyed variable X removed` |
| `key` → `unkeyed` | `variable X is now unkeyed` |
| `unkeyed` → `key` | `variable X is now keyed` |
| an `unkeyed` variable's value | nothing: no key changes |

- The listings of `plan`, the approver and `doctor` (`crates/buildl-core/src/types/authority.rs:109-110`,
  `crates/buildl/src/approver/interactive.rs:276`, `crates/buildl-cli/src/doctor_text.rs:94`) print
  `read variables:` for the `key` names and a new line `unkeyed variables:` for the `unkeyed` ones;
  `doctor` prints `env.unkeyed` for the ceiling and each plugin.
- buildl is unpublished: the key's form changes, cache rows written before it no longer match and
  are rebuilt; nothing reads the old form.

## 5. Running

| Where | `key` variables | `unkeyed` variables |
|---|---|---|
| input sandbox, `bare` | given, with their values (P8) | given, with their values |
| remote cache: the `Command` looked up and uploaded | in `environment_variables` (P3) | left out |
| remote execution | in `environment_variables` | left out: the server uses its own environment (P9) |

Leaving them out of the remote `Command` is what makes the remote name the same on two machines:
with them in, the `Command` digest, part of the `Action`'s, would differ (P3). The `Action`'s
timeout is kept (P5, the author's decision): it comes only from the committed `buildl.toml` (P6).

## 6. Refusing this machine's folders

buildl refuses a target, and a discovery output, that holds one of this machine's folders:

| Folder | Value |
|---|---|
| the home folder | buildl's own `HOME`, a trailing `/` taken off; not checked when unset, empty or `/` |
| the workspace folder | the workspace root, both as given and canonical (`/tmp` and `/private/tmp` on macOS) |
| the sandbox folder | `<temp>/buildl-<digest of the workspace's path>` (`crates/buildl/src/state/scratch.rs:56-65`), as given and canonical |

| Checked | When |
|---|---|
| each word of a command, placeholders unexpanded | by every command that loads the build files (P18), before anything runs |
| the value of each `key` variable | when the key is gathered |
| each discovery target's output | when the second pass reads it, whether it was just run, served from the local cache or fetched |

- Inputs and outputs are not checked: they are workspace-relative by type (P12). An `unkeyed`
  variable's value is not checked: it is in no key.
- A discovery output is recorded as any output is (P20), and the check is on reading it: a refused
  output stays in the cache and is refused again on every run, until the discovery's command
  changes, and so its key.
- Under `watch` a refusal is reported and the watch goes on waiting, as for a broken build file
  (P17).
- When several folders match one text, the message names the longest.
- Today's examples and the `c` plugin pass the check: their build files hold no such path and their
  discovery outputs are relative (P19).

**The match.** A folder matches where it starts the text or follows a character that cannot be part
of a path (anything but a letter, a digit, `.`, `_`, `-`, `/`), and where it ends the text or is
followed by a character that cannot continue its last name (anything but a letter, a digit, `.`,
`_`, `-`). Checked against texts like the real ones, with `HOME=/home/al`:

| Text | Refused? | Why |
|---|---|---|
| `cd /home/al/shop && make` | yes | followed by `/` |
| `/home/al` | yes | the whole text |
| `cd /home/al && make` | yes | followed by a space |
| `/home/al:/usr/lib` | yes | followed by `:` |
| `"workspace_root":"/home/al/tcapp"` (as `cargo metadata` writes, P11) | yes | after `"`, followed by `/` |
| `cd /home/alice/shop` | no | followed by `i`, which continues the name |
| `/data/home/al/x` | no | after `a`, inside another path |
| `cd shop && make` | no | not there |
| `/usr/bin/env` with `HOME=/` | no | a home of `/` is not checked |

## 7. Errors

Messages start with the build file, as errors do today (`build.lua: //:app: …`,
`crates/buildl-core/src/error.rs:1758`).

| # | Case | Where | Exit | Message |
|---|---|---|---|---|
| E1 | `env` written as a list, or mixed | load | 5 | `build.lua: //:app: env must mark each variable "key" or "unkeyed", such as env = { HOME = "unkeyed" }` |
| E2 | a mark that is neither | load | 5 | `build.lua: //:app: env HOME is marked "pass"; mark it "key" or "unkeyed"` |
| E3 | `unkeyed` not allowed by a level | plan | 4 | as the `env.read` refusal today (`crates/buildl-core/src/error.rs:1755-1783`), naming `env.unkeyed` and the level |
| E4 | `env.unkeyed` names a variable its table's `env.read` does not | reading `buildl.toml` or `extension.toml` | 5 | `buildl.toml: ceiling env.unkeyed lists HOME, which env.read does not` |
| E5 | a command holds this machine's folder | before running | 5 | `build.lua: //:note: the command holds this machine's path /Users/hiraq; it would give the target another key on every machine` |
| E6 | a `key` variable's value holds one | when the key is gathered | 5 | `build.lua: //:app: the variable RUSTFLAGS holds this machine's path /home/alice; mark it "unkeyed" if it only finds files` |
| E7 | a discovery output holds one | when the second pass reads it | 5 | `build.lua: //:cargo.metadata: its output holds this machine's path /home/alice/shop` |
| E8 | a pinned name, either mark | plan | 5 | as today (`crates/buildl-core/src/error.rs:1563-1569`) |
| E9 | a bad variable name | load | 5 | as today, the `EnvName` refusal |

## 8. What else changes

The files that declare `env`, list `env.read` or print `read variables` were found with
`git grep -l -E 'env *= *\{|env\.read|read variables' -- book skills examples plugins crates README.md CLAUDE.md`
(2026-11-07). Each moves to the map form, gains `env.unkeyed`, or prints the two listing lines:

| Place | Files |
|---|---|
| `plugins/c` | `graph.lua:191,201` (`env = { PATH = "unkeyed" }`), `extension.toml` (`env.unkeyed = ["PATH"]`), `README.md:191,197`, its tests |
| examples | `go`, `go-discovery`, `plugins` (with `gobin/main.lua`, `gobin/extension.toml`): `HOME`; `polyglot`: `HOME` and `PATH`; `rust`: `HOME`, `PATH`; `c`, `cpp`: `PATH`; the `buildl.toml` of each, and of `python` and `remote`; `c/README.md`, `cpp/README.md` |
| skills | `skills/buildl/SKILL.md`, `references/lua-api.md`, `plugins.md`, `buildl-toml.md`, `exit-codes.md`; `skills/buildl-migrate/SKILL.md`, `references/make.md`, `npm.md`, `just.md`, `shell.md`, `scripts/parity_lib.lua`; `skills/tests/parity_test.lua`, `snippets.lua`, `fixtures/compare/**` |
| book | `appendix-02-buildl-toml.md`, `appendix-03-lua-api.md:92`, `appendix-04-exit-codes.md` (E1-E9 under 4 and 5), `ch01-03`, `ch02-00`, `ch03-01:72`, `ch03-04`, `ch04-01`, `ch04-02`, `ch04-03`, `ch05-01:150`, `ch05-02`, `ch05-03`, `ch06-00:200`, `ch07-00`, `ch08-01`, `ch08-02`, `ch08-03`, `ch08-04`, `ch09-00` ("Who should upload": users share only through `unkeyed`; the server's own environment under execution; the timeout is in the remote key), `ch11-05`, `ch11-09` (the timeout: in the remote key, not the local one); every captured example that shows `env` or a listing is captured again from a real run |
| crates | the fixtures and golden texts the search lists under `crates/*/tests/`, and the in-file tests of the files named in §2-§4 |

`README.md` shows no `env` declaration (its one mention is prose, `README.md:9`) and does not change.

## 9. Testing

| Level | Test | Asserts |
|---|---|---|
| core | a new fake `Host` for two machines (the test stub cannot, P21) differing in `HOME` and `PATH`, one target marking both `unkeyed` | the same key; a different tool fingerprint, input digest or `key` variable value gives a different key; each `plan` row of §4 |
| remote | the same two machines through `crates/buildl/src/remote/encode.rs` | the same `Command` and `Action` digests; `unkeyed` names absent from `environment_variables`; a different timeout gives a different `Action` digest |
| binary | `crates/buildl-cli/tests`: one workspace copied to two folders, run with two `TMPDIR` values and two `HOME` and `PATH` values, its targets marking `HOME` and `PATH` `unkeyed` | `buildl plan --json` gives the same key for every target |
| CI end-to-end | a new step in `examples/remote/assert.sh`, run by the job "Remote example on NativeLink" (`.github/workflows/ci.yml`, job `examples-remote`); the example's target marks `HOME` and `PATH` `unkeyed`: build with `--remote-upload` in one folder, then build a copy in another folder with another `TMPDIR`, another `HOME` and a `PATH` with one more folder in front | the copy prints `1 fetched` |
| units | E1-E9 each, and every row of the match table in §6 | the refusal, its exit code and message |

## 10. Non-goals

- The sandbox folder written into some outputs (settled in `2026-11-06-where-actions-run`).
- Removing the timeout from the remote key (§5; author's decision, P5).
- Sharing between different operating systems or processors.
- Identifying programs a command starts by itself (cargo's `rustc`, a compiler's linker); a plugin
  covers them with a version check (`plugins/c`'s `version_probe`; the planned `rust` plugin's).
- Checking `unkeyed` values, build settings (`$opt:`), or anything already relative by type.
- The `rust` plugin (`2026-11-06-rust-plugin`).
