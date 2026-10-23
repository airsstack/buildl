---
status: done
created: 2026-10-22
milestone: buildl/05
---

# Intent: plugins and discovery targets

## Problem

A build file can only write primitives by hand, and nothing lets one person's knowledge of
how to wrap a toolchain be reused by another.

- **There are no plugins.** Design §10 promises `buildl.use("go")`: Lua that turns one call
  into the targets a stack needs (`go.binary` → build, test, vet), so a workspace does not
  restate `go.mod`, flags and input sets in every build file. `buildl.use` does not exist,
  and the build-file engine withholds `require`, so plugin code spread over several files
  cannot load either.
- **Nothing says where a plugin comes from.** Design §10 shows only a `plugins/go/`
  folder. A team needs its own plugins, kept in the repository, and shared ones kept in
  their own repositories. `buildl.toml` has no table for either, and an unknown table is
  refused (design §4).
- **A want cannot be traced to a plugin.** `plan`, `doctor` and the approver prompt name
  the target that wants a program, but not the plugin that generated the target. Design
  §10 promises "wants go — declared by plugin go@1.x for //services/api:server", and a
  workspace that caps what each plugin may ask for, so that a compromised plugin update
  wanting `curl` is caught. Milestone 4 left this attribution to milestone 5.
- **The manifest in design §10 does not match airsl's.** Design §10 writes
  `plugin.toml` with a `[plugin]` table. airsl's extension manifest, which milestone 5 is to
  consume (design §13 item 5), is `extension.toml` with an `[extension]` table holding
  `name`, `version`, `entry` and `api`, and refuses unknown keys. Its capabilities mean
  what the extension's own engine may reach; a buildl plugin's mean what the targets it
  generates may want, since plugin Lua itself reaches nothing.
- **Some graphs cannot be declared from source files alone.** "One test target per Go
  package" needs `go list`; a Rust workspace needs `cargo metadata`. Build files may not
  run tools (design §10), so today the graph cannot follow what only a tool knows. Design
  §10 names the answer, the discovery target, and §14 leaves its contract open.
- **Plugin API versioning is open** (design §14) and must be settled before a third-party
  plugin ships.

Agreed with the user, and overridable:

- **Manifest:** airsl's `extension.toml`, as airsl defines it; design §10 is corrected to
  match.
- **Sources:** a path in the filesystem, or a git repository on any host (a short form for
  GitHub). Both are listed in `buildl.toml`, never in Lua, and the workspace's cap on what
  each plugin may ask for sits beside its source.
- **Pinning:** a git source names a tag or a commit; it is pinned to a commit and a
  content hash in a generated `buildl.lock`, kept in the repository.
- **Network:** only an explicit `buildl plugins fetch` reaches the network. `check`,
  `build` and every other command read plugins from disk, and a plugin not fetched is an
  error that names the command to run.
- **Where fetched plugins live:** per workspace, under `.buildl/plugins/`. Each engineer
  fetches for their own checkout; nothing fetched is shared between workspaces or
  committed. A plugin's content is checked against `buildl.lock` each time it is loaded.
- **Authority:** what a generated target may want is its want ∩ the plugin's manifest
  capabilities ∩ the workspace's cap for that plugin ∩ the ceiling.
- **Versioning:** airsl's rule: within `api = 1`, the plugin API only gains names, and a
  manifest naming an unsupported `api` is refused before any plugin code runs.
- **Discovery:** one extra pass. Load, run the discovery targets, Load again with their
  output, then plan. A discovery target declared only in the second pass is an error, not
  a third pass.

## Affected systems

- `buildl-core` — attribution carried from declaration to the wanted set, the per-plugin
  intersection, and a pipeline that loads twice when discovery targets are declared.
- `buildl-lua` — `buildl.use`, loading a plugin's Lua under its own root with confined
  `require`, reading the plugin manifest, and handing a discovery target's output to the
  second pass.
- `buildl` — the `[plugins]` tables of the manifest reader, `buildl.lock`, the plugin
  store under `.buildl/plugins/`, fetching from git.
- `buildl-cli` — `buildl plugins fetch`; attribution in `plan`, `doctor` and the approver
  prompt.
- `docs/design.md` §4, §5, §7, §10, §13 item 5, §14; `docs/architecture.md`;
  `docs/architecture-building-blocks.md`; `.claudestacks/sdlc/roadmaps/buildl/05-plugins.md`;
  `README.md`; `CLAUDE.md` Status; the crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|`buildl.use(name)`|returns the plugin's module table; what the plugin declares is ordinary primitives, and `buildl graph` shows them. A plugin may use another plugin (§10.1 wrapping).|
|Path source|`[plugins.<name>] path = "…"` loads a plugin from a folder in the workspace.|
|Git source|`[plugins.<name>]` names a git URL (any host, short form for GitHub) and a tag or commit; `buildl plugins fetch` resolves it, stores it under `.buildl/plugins/` and writes `buildl.lock`.|
|No network elsewhere|every command but `buildl plugins fetch` works offline; a missing or changed fetched plugin is refused with the command to run.|
|Manifest|`extension.toml`; a bad manifest or an unsupported `api` is refused before the plugin's code runs, naming the file.|
|Attribution|`plan`, `doctor` and the approver prompt say which plugin (and version) declared the target behind each want.|
|Per-plugin cap|a generated target wanting more than its plugin's manifest or the workspace's cap allows refuses the plan, naming the want, the target and the plugin.|
|Discovery targets|a build file or plugin declares a discovery target and reads its output on the second pass; the discovery action is planned, approved, sandboxed and cached like any other target. A discovery target that appears only on the second pass is an error.|
|Open questions|design §14's discovery-target contract and plugin API versioning are recorded as resolved.|
|Tests|core tests against fake ports, end-to-end tests through `Workspace` and CLI tests of the binary cover each row above; git fetching is tested against a local repository, never the network.|
|Documents|the design, both architecture documents, README and `CLAUDE.md` describe plugins and discovery targets as built.|

## Constraints

- Nothing runs and nothing reaches the network while build files or plugins are read:
  the declaration phase stays a reader of files (design §10).
- airsl stays a dependency of `buildl-lua` only; no airsl type crosses into another crate.
- A plugin can only narrow authority: nothing a manifest or a plugin says lets a build
  exceed the ceiling, and failure is closed.
- Determinism (`architecture.md` §5): the same sources, plugins and settings declare the
  same graph and the same keys on every machine; no `HashMap` iteration reaches output;
  JSON through the one canonical serializer.
- A plugin's code reaches the action key only through what it declares; switching how a
  target was declared without changing it rebuilds nothing.
- The structural rules hold: logic in `buildl-core`, testable against fake ports; adapter
  modules of `buildl` do not import each other (`cargo make guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` green before every commit; a new dependency
  carries its reason in the workspace `Cargo.toml`; work on this branch, never `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- `b.fetch` and hermetic toolchains (design §9.6, level 3).
- The stack plugin catalog (`go`, `bun`, `rust`, `docker`, `proto`, `python`, design §10.3).
- Discovery that repeats until nothing new appears, and discovery in the middle of an
  action (design §14).
- A plugin registry, or plugin sources other than a path and git.
- Sharing fetched plugins between workspaces.

Candidate direction: three steps in one chain: path plugins with attribution and caps;
git sources with `buildl.lock` and `plugins fetch`; discovery targets.
