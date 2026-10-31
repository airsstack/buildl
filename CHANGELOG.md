# Changelog

Notable changes to this workspace, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crates follow
[SemVer](https://semver.org/) — with the caveat that for a `0.1.z` version the *middle* number is
the slot Cargo treats as breaking, so a compatible change ships as a `z` bump.

## Unreleased

## 0.1.0 — 2026-10-31

First release. It publishes four crates — `buildl-core`, `buildl-lua`, `buildl` and `buildl-cli`
(the package that holds the `buildl` binary) — at 0.1.0, and tags the `c` plugin as
`plugin-c-v0.1.0`.

### Added

- **The build pipeline and its commands.** Build files are written in Lua and only declare targets;
  `buildl` loads them, plans, runs the actions and records the results. `check`, `graph`, `query`
  (`deps()` and `rdeps()`), `plan`, `build`, `test`, `log`, `clean` and `gc` work on a real
  workspace. A target is rebuilt only when its action key changes, and a dependency rebuilt to the
  same bytes leaves what depends on it alone. `plan` says what would run and why. Build files load
  in parallel.
- **The sandbox and `verify`.** `build` and `test` run each action in a directory outside the
  workspace that holds copies of its declared inputs, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH`
  pinned. `strategy = "bare"` runs actions as plain processes instead. `buildl verify` re-runs
  cached actions in the sandbox — all of them, or the `--sample N` lowest by key — and names those
  whose outputs differ from the cache.
- **The approver.** `buildl.toml` sets a ceiling on what a build may reach, and an approver decides
  within it after planning and before anything runs: `manifest` (the default), `interactive`, which
  asks once on a terminal and remembers a yes outside the workspace, or `deny`. `buildl doctor`
  prints the ceiling, the approver, the run settings and everything the build asks for, and
  refuses nothing.
- **Plugins and discovery targets.** A build file calls `buildl.use` to load a plugin kept in the
  workspace or fetched from git by `buildl plugins fetch` and pinned in `buildl.lock`. A plugin is described
  by an `extension.toml`, and what it declares carries its name through `plan`, `doctor`, the
  approver and `graph`. A discovery target (`buildl.discover`, read back with
  `buildl.discovered`) is a cached action whose output a second declaration pass consumes.
- **The remote cache and remote execution.** A `[remote]` table in `buildl.toml` names a server that
  speaks the Remote Execution API v2 over gRPC. `build` and `test` look up a target the local cache
  cannot serve there and report a hit as `fetched`; uploads happen only when asked for. With
  `execute = true`, a target neither cache can serve is sent to the server to run and reported as
  `remote`. Calls are retried on transient errors, `--no-remote` turns the remote off and
  `--no-remote-execute` turns off remote execution only, and asking for an upload or for execution under `strategy = "bare"` is refused.
- **Watch mode.** `buildl watch [target]...` takes `build`'s flags, builds, then builds again when a
  file the build reads changes or when the build files declare a different graph.
- **The `c` plugin for C and C++.** Each source compiles to its own object, and a discovery target
  finds the headers it reads, so editing one header rebuilds only the objects that read it. It
  builds programs, static libraries, shared libraries and tests on Linux and macOS with gcc or
  clang.
- **Agent skills.** The `buildl` skill guides an agent writing and running build files, and
  `buildl-migrate` one moving an existing build to buildl. `skills/install.sh` installs them.
- **Install routes.** Prebuilt archives for macOS (Apple Silicon and Intel) and Linux (x86_64 and
  ARM64, each as a glibc build and a static musl build) are published as a GitHub Release with a
  SHA-256 checksum and a GitHub artifact attestation per archive, along with a shell installer for
  macOS and Linux that installs `buildl` into `~/.local/bin`. Every archive is smoke-tested before
  the release is published. `cargo install --locked buildl-cli` builds it from crates.io.
