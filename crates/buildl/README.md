# buildl

A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

This is the framework crate and the composition root. It binds concrete adapters to the pipeline defined in [`buildl-core`](../buildl-core):

- an adapter for every port a local build uses except `DeclarationSource` — `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ContentStore`, `ActionCache`, `EventLog`, `GraphRecord`, `ExecStrategy`, `Dispatcher`, `Clock`, `Reporter` and `Approver`;
- the Lua evaluator from [`buildl-lua`](../buildl-lua), which implements `DeclarationSource`.

It assembles them as `LocalPorts`, exposes the result as the `Workspace` facade, and re-exports `buildl-core` as `buildl::buildl_core`. The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

## What it holds today

| | |
|---|---|
| Facade | `Workspace::open(start)` finds the workspace at the nearest `buildl.toml` at or above `start` and reads it, writing nothing. `check`, `graph`, `plan` and `build` run the pipeline that far; `label` reads a target as a person names it — `//dir:name`, or `:name` and `name` in the directory the workspace was opened from. `build` and `test` are lent the caller's `Reporter` and `Console` for that one run, and `verify` the `Console`; `with_data_dir` says where the approver remembers a yes, in place of the user's data directory; `doctor` reports the ceiling, the approver, the execution settings and every want as a `Doctor`, with the wants not yet approved, and writes nothing; `fetch_plugins` pins every git plugin `buildl.toml` names and stores its files. `build` and `test` run a workspace's discovery targets first, and every command that loads build files loads them again with the last result of each |
| Plugins | `plugins` — `buildl.lock` (`Lock`, `LockEntry`), `tree_digest`, the SHA-256 of a plugin's files, `check`, which finds every plugin `buildl.toml` names ready to use from the lock and the store before any build file is evaluated, and `fetch`, which runs the system `git` to pin one revision of a git plugin. It implements no port, and `Wiring` hands the directories `check` found to the Lua evaluator |
| Approving | `approver` — `ManifestApprover` (yes), `DenyApprover` (no) and `InteractiveApprover`, which asks once on a `Console` and remembers a yes in `Approvals`, a file in the user's data directory outside the workspace; `LocalApprover` is the one `[approver]` names, and `NoTerminal` the console when there is no one to ask |
| Composition | `LocalPorts<R, C>`, the bundle of real adapters, generic over the reporter and the console the approver asks on; `LocalPipeline<R, C>`; and `Wiring`, which makes every port for one `plan` or one `build`. `check` and `graph` make a source of build files and nothing else, so neither needs a state directory, takes the lock or is stopped by a state file |
| Reading inputs | `manifest` — `buildl.toml`, read strictly: a key it does not know is an error with its line and column, `[plugins.<name>]` tables included — and the workspace root; `host` — the platform and environment values; `digest` — what stands at an input's path, and file hashes in parallel; `tools` — a program's fingerprint, the SHA-256 of the file its name leads to |
| State on disk | `cas` — `.buildl/cas`, read-only blobs with an executable twin where one is needed, and the files placed under `out/`; `cache` — `cache.json` and its journal; `log` — `log.jsonl`; `statcache` — `statcache.json`; `graph` — `graph.json`, written by every build |
| Running actions | `exec` — `Bare`, isolation tier 1: a plain process in the workspace root, started without a shell, with exactly its declared environment, an empty standard input and a timeout; `dispatch` — worker threads, started as tasks need them; `clock` — the system clock |
| Reporting | `report` — `TerminalReporter`, with a live status line or plain blocks; `Silent`; and the text of a plan, a graph as DOT, and an error |
| Shared | `state` — where each state file is, the lock that keeps a second buildl out, writing a file whole, clearing a path — and `search_path`, the one lookup of a program name. Neither implements a port, and adapter modules name no other module of the crate; `plugins` names only `state` and `search_path` |

The crate builds on Unix targets only; the supported platforms are Linux and macOS.

**Status:** pre-release. Every port a local build uses has its adapter, and a workspace is built end to end through `Workspace`. Actions run at isolation tier 2, the input sandbox, by default, and at tier 1 under `strategy = "bare"`; `os-sandbox` and `container` are refused. A build, a test run or a verify asks the approver the manifest names before anything runs. Plugins are checked against the lock before any build file is evaluated, and only `fetch_plugins` uses the network.

## License

Apache-2.0
