---
status: approved
created: 2026-10-22
built-on: db9d07ade9e6
---

# Spec: plugins and discovery targets

Delivers milestone 5 (design §13 item 5). A build file calls `buildl.use("go")` and gets a
plugin's functions; the plugin is Lua kept in the workspace or in a git repository on any host,
named in `buildl.toml`, and fetched only by `buildl plugins fetch`. Everything a plugin
declares carries the plugin's name to `plan`, `doctor`, the approver prompt, refusals and
`buildl graph`, and what it may want is narrowed by its own manifest and by the workspace's cap
for it, before the ceiling. A discovery target is an ordinary cached action whose output the
build files read on a second pass. Design §14's discovery contract and plugin API versioning
are settled. Everything is rooted in this chain's intent.

## 1. Premises (proven in this task)

Probes were compiled against airsl 0.1.4 (the version in `Cargo.lock`) in a scratch crate and
run on 2026-10-22; their output is quoted. Every other claim cites a file opened in this task.

|#|Claim|Evidence|
|---|---|---|
|P1|airsl parses an `extension.toml` holding `[extension] name, version, entry, api` and `[capabilities] proc.run, env.read`.|probe: `manifest ok: name=go version=1.2.0 entry="main.lua" api=ApiVersion(1) proc=["go"] env=["GOCACHE", "HOME"]`|
|P2|Design §10's `[plugin]` table is refused, and `version` is required.|probe: ``unknown field `plugin`, expected one of `extension`, `capabilities`, `limits` ``; with `[extension] name, api` only: ``missing field `version` ``|
|P3|airsl refuses an `api` it does not support, before any code runs.|probe: `manifest declares api 2; this runtime supports api 1`; `Manifest::validate` (`airsl-0.1.4/src/extension/manifest.rs:212`)|
|P4|airsl's capability block also accepts `fs.read`, `fs.write`, `[capabilities.optional]` and `<module> = true` keys, and a `[limits]` table.|`airsl-0.1.4/src/extension/manifest.rs:425-437` (`RawCapabilities`), `:108-123` (`LimitRequest`)|
|P5|The build-file engine runs airsl's `Minimal` surface with no grant (`crates/buildl-lua/src/engine.rs:34-37`), and on `Minimal` there is no `require` even for a script with a directory.|probe, `Script::from_file` of a file calling `require("lib.util")`: `attempt to call a nil value (global 'require')`; `os,os.time,os.clock,coroutine = "nil,nil,nil,nil"`|
|P6|airsl's confined `require` comes only with the `Restricted` surface, which also exposes `os.time` and `os.clock`, which differ from run to run.|probe on `Restricted`: `require -> Ok("ok")`; `os,os.time,os.clock,coroutine = "table,function,function,table"`; `RequireDisposition::decide` (`airsl-0.1.4/src/require_loader.rs:68-76`)|
|P7|`buildl-lua` reaches mlua through airsl's re-export (`crates/buildl-lua/src/module.rs:17`, `use airsl::mlua::{self, Lua, Table, Value}`), and mlua runs a chunk under an environment table the host gives it.|probe: `lua.load("return marker").set_environment(env)` → `set_environment: 42`|
|P8|Reading the Lua call stack cannot reliably say who called a primitive: a tail call removes the caller's frame.|probe: `local function inner() return who() end return inner()` under chunk name `=plugin:go/main.lua` → `stack: Some("=[C]") <- Some("=plugin:go/main.lua")` (no frame for `inner`)|
|P9|A declaration records only the build file that made it: `Declaration { provenance, item }` (`crates/buildl-core/src/types/declaration.rs:138-141`); a graph node carries `provenance` and no plugin (`types/target_graph.rs:27-49`); `Provenance` is a file and a directory (`types/provenance.rs:25-28`).|file:line opened|
|P10|A build file's evaluation crosses one port, `DeclarationSource::evaluate(&self, file: &BuildFile) -> Result<Evaluated>` (`crates/buildl-core/src/ports/declaration_source.rs:67`); every evaluation gets a fresh engine (`crates/buildl-lua/src/source.rs:124-138`).|file:line opened|
|P11|The wanted set is four sets — programs, env, write, network — with no attribution (`crates/buildl-core/src/types/authority.rs`, `WantedSet`); a ceiling refusal reads `<file>: <label>: program X is outside the ceiling (proc.run allows: …)` (`crates/buildl-core/src/error.rs:503-530`); `Error::Refused` exits 4 (`crates/buildl-cli/src/exit.rs`, policy refusal).|file:line opened|
|P12|`buildl.toml` is read strictly: `Raw` is `#[serde(deny_unknown_fields)]` with tables `workspace`, `ceiling`, `declaration`, `execution`, `approver` (`crates/buildl/src/manifest/schema.rs:49-66`), so a `[plugins]` table is refused today.|file:line opened|
|P13|`check`, `graph` and `query` are free functions over a `DeclarationSource` and a `Dispatcher` alone (`crates/buildl-core/src/pipeline/driver.rs:290-311`); they take no lock and need no state file, and `doctor` plans and hashes nothing (`docs/architecture-building-blocks.md:446,640`; `docs/design.md:244`). `log` reads the log and the cas without the lock (`docs/design.md:244`).|file:line opened|
|P14|A log event holds the target's outcome (`types/outcome_event.rs:24-40`), and a succeeded outcome holds what each output produced (`types/outcome.rs:126-127`, `outputs: Vec<Produced>`, "Empty unless the target succeeded"); `RunHistory` reads the log's lines (`ports/run_history.rs:18-24`).|file:line opened|
|P15|An output directory's one digest is the digest of its `OutputManifest::Tree`'s canonical JSON, sorted by path (`docs/architecture.md:289`); a directory output holds regular files only, and a symbolic link, a special file, an empty directory or a non-Unicode name is refused (`docs/architecture.md:278,393`).|file:line opened|
|P16|`git` is on the development machine (`git version 2.39.3 (Apple Git-146)`); CI runs on `ubuntu-latest` and `macos-latest` only (`.github/workflows/ci.yml:36`).|command run; file:line opened|
|P17|`.buildl/` is documented as gitignored (`docs/design.md:71`); buildl writes no `.gitignore` itself (`git grep -n gitignore -- crates docs README.md` finds only that line; the same search finds the design line, the control).|search run|
|P18|Exit codes: 3 infrastructure, 4 policy refusal, 5 build files or `buildl.toml`, 6 a state file not what buildl writes (`docs/design.md`, §8.7 table).|file:line opened|
|P19|`airsstack.buildl` and the global `buildl` are one table (`crates/buildl-lua/src/module.rs:4,130`): a plugin that can reach either reaches the build file's own, unstamped primitives.|file:line opened|
|P20|`check` loads twice and compares (`crates/buildl-core/src/pipeline/driver.rs:290-298`); `gc` loads the build files once, as `build` does, and drops the rows of undeclared targets (`docs/architecture-building-blocks.md:446`, `docs/design.md:239`).|file:line opened|
|P21|airsl subtracts required capabilities from the optional block (`airsl-0.1.4/src/extension/manifest.rs:232`, `.without(&required)`), so only a non-empty optional block or a set limit leaves a trace in `Manifest`.|file:line opened|
|P22|git resolves a tag and fetches one commit over `file://` at depth 1: `git ls-remote file://…/src refs/tags/v1.0.0` → `6ea2de50…ab0 refs/tags/v1.0.0`; `git fetch --depth 1 file://…/src 6ea2de50…ab0` then `checkout FETCH_HEAD` → `rev-parse HEAD` = `6ea2de50…ab0`; `git fetch --depth 1 … refs/tags/v1.0.0` → the same commit. Fetching a bare commit from another host depends on that server's settings: not verified beyond `file://`.|probe script run with git 2.39.3, 2026-10-22|

## 2. Configuration: `[plugins]`, `buildl.lock` and the plugin store

```toml
# buildl.toml
[plugins.go]
git      = "https://github.com/acme/buildl-go"   # any git URL; or github = "acme/buildl-go"
rev      = "v1.2.0"                              # a tag or a full commit
proc.run = ["go"]                                # the workspace's cap for this plugin
env.read = ["HOME", "GOCACHE"]
network  = false                                 # the default

[plugins.ourgo]
path = "plugins/ourgo"                           # workspace-relative
```

|Key|Rule|
|---|---|
|table name|`[plugins.<name>]`; `<name>` is what `buildl.use` is called with, and is a valid target name. The plugin's `extension.toml` must give the same `[extension] name`, or loading it is refused (exit 5). Attribution prints `<name>@<version>`, the version from `extension.toml`.|
|source|exactly one of `path`, `git`, `github`. `github = "owner/repo"` is `git = "https://github.com/owner/repo"`, and everything after reads the expanded URL.|
|`path`|a workspace-relative directory, like a ceiling directory (design §4); it may not be, contain, or lie in `out` or `.buildl`.|
|`rev`|required with `git`/`github`, refused with `path`: a tag, or a full 40-hex commit. Branches are not accepted: a lock pins what `rev` names, and a branch moves.|
|`proc.run`, `env.read`|the cap: lists, absent means empty. An empty cap allows the plugin's targets nothing of that kind.|
|`network`|`true` lets the plugin's targets declare `network = true`; absent means `false`.|
|anything else|refused with its line and column, as every unknown key is (P12).|

**`buildl.lock`** sits beside `buildl.toml` and is committed. It is canonical JSON written
through the one serializer (`architecture.md` §5): one entry per git plugin,
`{ "commit", "git", "rev", "sha256" }`, where `git` is the expanded URL and `rev` is as
`buildl.toml` wrote it,
`commit` is the full commit `rev` resolved to, and `sha256` is the plugin's tree digest (P15:
the digest of its `OutputManifest::Tree`, every regular file of the checkout except `.git/`).
A path plugin has no entry.

**The store** is `.buildl/plugins/<name>/<commit>/`, per workspace, outside git (P17). Each
engineer fetches into their own checkout.

**Every command that loads build files** checks every plugin `buildl.toml` names, once,
before the first build file is evaluated, in the `buildl` adapter, and finds each git plugin
through `buildl.lock`, never the network. The composition root hands `LuaSource` the checked
plugin directories; `buildl-lua` never reads the lock or hashes a store.

|State|Result|
|---|---|
|no lock entry, or its `git`/`rev` differ from `buildl.toml`|refused: the plugin is not fetched; the message names `buildl plugins fetch` (exit 5)|
|entry, but the store directory is missing|same refusal (exit 5)|
|store directory's tree digest ≠ `sha256`|refused: the plugin was changed after it was fetched (exit 6)|
|digest matches|handed to `LuaSource`|

A path plugin's directory must exist (exit 5 otherwise). A workspace that names a plugin must
have it fetched even when no build file uses it: the check is once per command, not per file.

## 3. `buildl plugins fetch`

The only command that reaches the network. It runs the `git` on buildl's `PATH` (P16), so
any host and the engineer's own credentials work, and buildl gains no dependency.

```text
for each git plugin, in name order:
  lock entry matches buildl.toml (git, rev) and its store dir digests to sha256? → nothing to do
  else → git init a temp dir under .buildl/plugins/
         git fetch --depth 1 <git> <rev>   (a tag ref, or the commit itself)      (P22)
         checkout FETCH_HEAD; commit = its commit; drop .git/; digest the tree
         lock entry matches (git, rev) but commit or digest differs → refused:
           the tag moved or the content changed since it was locked (exit 6)
         else → rename into .buildl/plugins/<name>/<commit>/, new or same entry
write buildl.lock (only when an entry changed), remove store dirs no entry names
```

Fetching a tag never needs the server to serve a bare commit. A `rev` that is a commit does,
which `file://` does (P22); whether another host does is that host's setting, and a refusal is
git's error, passed on (exit 3).

It holds the state lock while it writes. A `git` that cannot be started, or fails, is an
infrastructure failure (exit 3) naming the plugin and git's stderr. A tree holding a
symbolic link or a special file is refused (exit 5), as an output holding one is (P15).
`fetch` loads no build file and asks no approver.

## 4. Loading a plugin: `buildl.use`

```text
build.lua engine (one per file, P10)
  buildl.use("go")
    ├─ [plugins.go] → plugin dir (path, or the checked store dir)
    ├─ read extension.toml with airsl (P1), refusing any fs.read, fs.write or
    │  <module> = true capability, a non-empty [capabilities.optional], and any set
    │  [limits] value (P4, P21)
    ├─ load `entry` as a chunk in the SAME engine, under its own environment (P7)
    └─ return the chunk's value (the plugin's module table)
```

**The plugin's environment** is a new table built per instance, never a read-through to the
file's globals, because a plugin that reaches the file's `buildl` — directly or as
`airsstack.buildl`, the same table (P19) — would declare unstamped and skip its cap:

|Name|Value|
|---|---|
|`buildl`|a new `buildl` table stamped with this instance's origin|
|`airsstack`|a new table holding the file's curated modules (`json`, `path`, `regex`, `hash`, `glob`) and, as `buildl`, the stamped table above|
|`require`|loads `<plugin dir>/a/b.lua` for `require("a.b")`, plugin dir only|
|`_G`|the environment itself|
|the language's own globals (`string`, `table`, `math`, `utf8`, `pairs`, `tostring`, …)|the same values the file sees, with buildl's replacements (`tostring`, `string.format`) and withholdings (`print`, `math.random`)|
|anything else|absent|

A build file may still pass its own `buildl` table to a plugin function as an argument; what
the plugin then declares through it is the build file's, written in the build file.

**Instances are per origin chain.** `buildl.use` and `require` cache by the full chain, not by
plugin name: in one evaluation, `go` used directly is the instance `[go]`, and `go` used by
`ourgo` is a second instance `[ourgo, go]` with its own modules. Each is loaded once per
evaluation.

|Rule|Detail|
|---|---|
|why buildl's own `require`|airsl's is only on `Restricted`, which brings `os.time`/`os.clock` (P5, P6); determinism forbids that.|
|`require` confinement|a module name is dot-separated names; no `/`, `\`, `..` or empty part; the file is resolved inside the plugin dir and a path that resolves outside it, through a link or otherwise, is refused; a module loading itself is refused; a module runs under its instance's environment and loads once per instance.|
|attribution|every primitive on a plugin's `buildl` table stamps its declarations with that table's origin. Not the call stack: a tail call loses frames (P8).|
|wrapping|`buildl.use` called from a plugin returns the instance for the caller's chain plus the new name: `ourgo` using `go` gives `[ourgo, go]`. Names resolve only through the workspace's `[plugins]`; a plugin cannot bring its own.|
|`buildl.use` of an unknown name, or a use cycle|refused, naming the file and the plugin (exit 5).|
|`buildl.sources` inside a plugin|walks the calling build file's directory, as for the build file: a plugin generates targets for the directory that used it.|
|errors in plugin code|an evaluation failure of the calling build file, its diagnostic naming the plugin file and line.|
|limits|the plugin's Lua counts toward the calling build file's `[declaration]` limits; there are no separate ones.|
|versioning (§14)|`api` is airsl's and airsl checks it (P3). The `buildl` table follows airsl's rule within `api = 1`: it only gains names. buildl has no second version number.|

## 5. Origin and narrowing

```text
StagedDeclaration ─► Declaration { provenance, origin, item } ─► Node { …, origin }
origin = Origin::BuildFile | Origin::Plugins(Vec<PluginRef>)       (outermost first)
PluginRef = { name, version (from extension.toml), manifest caps (proc.run, env.read) }
```

`buildl-lua` turns airsl's manifest into `PluginRef` before it crosses the port: no airsl type
leaves `buildl-lua`. `Workspace` gains `plugins: BTreeMap<PluginName, PluginCap>` from
`buildl.toml` (`proc.run`, `env.read`, `network`).

**The check.** A node's plugins are the members of its target's chain together with those of
the chain of the rule it names, if any: a hand-written `b.target{ rule = … }` naming a plugin's
rule is held to that plugin. Before the ceiling check, every want of a node with plugins is held
to **each** of them, both its manifest caps and the workspace's cap for it:

|Want|Allowed when|
|---|---|
|program|in every chain member's manifest `proc.run` and workspace `proc.run`|
|env name|in every chain member's manifest `env.read` and workspace `env.read`|
|`network = true`|every chain member's workspace cap has `network = true`|

A program a dependency built is no want (design §6) and is never held to a plugin. Inputs and
the write directories are not narrowed per plugin. The first want refused gives
`Error::PluginRefused { want, site, plugin, allowed }` (exit 4), e.g.
`build.lua: //api:server: program curl is outside plugin go@1.3.0 (proc.run allows: go)`.
Wants pass the plugin check, then the ceiling check as today: request ∩ plugin ∩ ceiling.

**Shown.** Origin is display and policy only: it is not an action-key component, so a
target moved from a plugin to a hand-written `b.target` with the same fields stays cached.

**Attribution of a want**: for each program, variable and network target the wanted set holds,
every plugin chain whose targets want it, each with the lowest-labelled such target. A write
directory is the workspace's own and is never attributed. A want only hand-written targets
want has none.

|Where|What|
|---|---|
|`plan`, the approver prompt|each want, then its attribution, one line per chain: `run programs: go, protoc` followed by `  go: declared by plugin go@1.2.0 for //services/api:server`|
|`doctor`|each want with its attribution, and each want outside a plugin's manifest or cap, reported as a want outside the ceiling is, refusing nothing|
|`buildl graph --json`|each node's `origin`|
|refusals and errors|the plugin chain beside the file and label|

The remembered approvals file and `WantedSet` keep their shape: attribution is a separate map
beside the set, used only for display.

## 6. Discovery targets

```lua
b.discover("pkgs", { run = { "go", "list", "-json", "./..." }, inputs = {...}, deps = {...},
                     env = {...}, network = true })
local text = b.discovered("//svc:pkgs")       -- the output's bytes as a string, or nil
local pkgs = text and airsstack.json.decode(text) or {}
```

|Rule|Detail|
|---|---|
|declaration|`b.discover` takes `b.target`'s table without `outputs` and `always`; it has one output, `<name>.json`, placed by buildl as any output is. Role `Discovery`, a third `TargetRole`. The output must be a file: an action that leaves a directory there fails as an action failure (exit 1).|
|reading|`b.discovered(label)` takes a label (relative labels resolve as `deps` do) and returns the output as text, or `nil` when no result is available. Decoding is the build file's.|
|no discovery declared|one pass, exactly as today.|
|pass 2 consistency|after the second load, the discovery targets declared must equal pass 1's, field for field; one added, removed or changed is refused (exit 5): discovery may not depend on discovery.|
|key, cache, sandbox|a discovery target is a target: keyed, cached, sandboxed, held to the ceiling and its plugins, logged.|
|`build`, `test`|its own default scope excludes discovery targets; `buildl build //svc:pkgs` builds one.|

**Where the result comes from**: the newest logged run of that target that succeeded (P14),
its one output read from the cas (`RunHistory` + `ContentStore`, as `log` reads, P13).

|Command|Flow|
|---|---|
|`build`, `test`|Load #1 → Resolve → narrow to the discovery targets and their dependencies → Plan → approve → Execute → Record → Load #2 with the results → the command as today (Resolve, narrow, Plan, approve, Execute, Record). Two approvals; `interactive` asks again only about wants the first yes did not cover. A failed discovery stops the command with that failure.|
|`plan`, `verify`|Load #1 → read last results → Load #2 → as today. `plan` lists each discovery target that is not clean and says the targets declared from it may change.|
|`gc`|Load #1 → read last results → Load #2 → as today, so the rows of targets declared from discovery are kept.|
|`check`, `graph`, `query`, `doctor`|Load #1 → read last results without the state lock → Load #2 → as today. `check`'s double load and comparison (P20) apply to the second pass; the first runs once. A result that cannot be read counts as none: a damaged log never stops `check`. A discovery with no result is noted on stderr: "//svc:pkgs has not run; run buildl build".|

With no discovery target declared, every command is exactly as today.

**Decision recorded.** Only `build` and `test` run discovery, as the intent's flow says. Every
other command declares from the last result this checkout recorded, a choice made in the
design dialogue so that commands that run nothing never run anything. Determinism holds over
what the declaration reads: the same sources, plugins, settings and discovery results declare
the same graph on every machine; a fresh clone has no results until its first `build`, and
those commands then show the graph without the targets discovery adds, and say so.

The second load reads the results through a new argument of the port:
`DeclarationSource::evaluate(&self, file, discovered: &Discovered)`, where `Discovered` maps a
discovery target's label to its bytes. Load #1 passes an empty map.

## 7. Errors and exit codes

|Failure|Error|Exit|
|---|---|---|
|`[plugins]` table invalid|manifest error, line and column|5|
|plugin not fetched, not in `buildl.lock`, or lock out of date|`PluginNotFetched`, names `buildl plugins fetch`|5|
|bad `extension.toml`, unsupported `api`, a capability buildl does not read, a `name` not matching `[plugins.<name>]`|evaluation failure of the using file, naming the manifest|5|
|path plugin directory missing|`PluginNotFetched`, naming the path|5|
|discovery output left as a directory|action failure|1|
|unknown plugin name, `use` cycle, `require` outside the plugin, plugin Lua error|evaluation failure of the using file|5|
|pass 2 discovery differs from pass 1|`DiscoveryChanged`, naming the target|5|
|plugin tree holds a link or special file|`fetch` refusal|5|
|a want outside a plugin's manifest or cap|`PluginRefused`|4|
|`git` missing or failing, store unwritable|infrastructure|3|
|fetched plugin's digest ≠ `buildl.lock`; or, in `fetch`, a locked tag now at another commit or content|`PluginChanged`|6|

## 8. Code shape (guideline-driven)

|Crate|Gains|
|---|---|
|`buildl-core`|types `PluginName`, `PluginRef`, `PluginCap`, `Origin`, `Discovered`, `TargetRole::Discovery`; `Declaration`, `StagedDeclaration` and `Node` gain `origin`; `Workspace` gains `plugins`; `grants` gains the per-plugin check and the attribution map; `load`/`pipeline` gain the two-pass flow; the three new errors and their exit classes. All testable against fake ports.|
|`buildl-lua`|`buildl.use`, the plugin loader and its `require`, the origin-stamped `buildl` table, `b.discover`/`b.discovered`, the manifest read (airsl stays here).|
|`buildl`|the `[plugins]` tables of the manifest reader, a `plugins` adapter module (lock read/write, store check, fetch through `git`), handing plugin dirs to `LuaSource`.|
|`buildl-cli`|`buildl plugins fetch`; attribution in `plan`, `doctor` and the prompt; exit mapping.|

Guideline rules applied: newtypes parsed at construction (`PluginName`, the lock's commit as a
40-hex type); no `bool` for `network` (an enum, as `NetworkAccess`); `mod.rs` export-only;
static dispatch (no new `dyn` beyond airsl's module seam); a colocated unit-test module in every
logic file; the `plugins` adapter module imports no other adapter module
(`cargo make guard-adapter-isolation`); new types join the canonical set, no duplicates.

## 9. Testing

|Layer|Covers|
|---|---|
|`buildl-core` flows (fake ports)|origin carried to nodes; chain narrowing and `PluginRefused`, including a hand-written target naming a plugin's rule; attribution in the wanted-set display and in `doctor`; the two-pass flow, its consistency refusal, results read from a fake log; `gc` keeps the rows of discovered targets; no discovery → one pass.|
|`buildl-lua` unit|`buildl.use` with fixture plugins; `require` confinement (`..`, `/`, a link out, a self-require); origin stamping incl. `[ourgo, go]` beside `[go]` in one file; a plugin reaching for `airsstack.buildl`, `_G.buildl` or any file global gets its own stamped table or nothing; manifest refusals (P2, P3, P4 keys, a mismatched name); `b.discover`/`b.discovered` on both passes.|
|`buildl` end to end (`Workspace`)|path plugin build; `plugins fetch` from a local `file://` repository made by the test (never the network) on Linux and macOS (P16, P22): a tag, a commit, a moved tag refused; lock written; changed store refused; offline commands; discovery building, cached, then read by `check`, `gc` keeping it.|
|`buildl-cli`|`plugins fetch` exit codes; the prompt and `doctor` text with attribution; every row of §7.|

## 10. Documents

`docs/design.md` §4 (`[plugins]`, `buildl.lock`), §5 (`buildl.use`, `b.discover`,
`b.discovered`), §7 (`plugins fetch`, the two-pass flow per command), §8.7 (new failures),
§10 (manifest as `extension.toml`, sources, narrowing), §13 item 5 (built), §14 (discovery
and versioning resolved); `docs/architecture.md`; `docs/architecture-building-blocks.md`
(adapter table, port change, types); `.claudestacks/sdlc/roadmaps/buildl/05-plugins.md`;
`README.md`; `CLAUDE.md` Status; the crate docs.

## 11. Non-goals

- `b.fetch` and hermetic toolchains (design §9.6, level 3).
- The stack plugin catalog (`go`, `bun`, `rust`, `docker`, `proto`, `python`).
- Discovery repeated until nothing changes; discovery in the middle of an action.
- A plugin registry; sources other than a path and git; sharing fetched plugins between
  workspaces; a path plugin outside the workspace.
- Plugin capabilities other than `proc.run` and `env.read`; per-plugin declaration limits.
- Origin in the action key.
