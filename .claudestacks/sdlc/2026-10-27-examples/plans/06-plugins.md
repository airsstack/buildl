---
status: done
created: 2026-10-27
depends-on: [03]
built-on: 5947af3e315f
---

# Plugin example — Implementation Plan

**Goal:** an example workspace builds its Go programs through a workspace-local plugin whose wants `plan` and `doctor` attribute to it.

**Architecture:** `examples/plugins/` holds `plugins/gobin/` — an `extension.toml` and its Lua
entry returning a helper that declares a Go binary target. `buildl.toml` names it as
`[plugins.gobin] path = "plugins/gobin"` with a cap, and the build file calls
`buildl.use("gobin")`. The workspace is one Go module with two programs under `hello/` and
`bye/`, so each target runs `go build -o $out ./<dir>` from the module root, which the
action reads its paths from (the workspace root, or the exec dir laid out as it). Checked by `examples/check.sh` (plan 03).

**Tech Stack:** Lua (airsl), Go 1.22, the `buildl` binary.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §2, §5. Gates: `cargo make dod`,
`cargo deny check` and `cargo make examples` green; Conventional Commit.

---

## Files that change

```
examples/plugins/{buildl.toml,build.lua,go.mod,assert.sh,README.md} — [create] the workspace, one Go module
examples/plugins/plugins/gobin/{extension.toml,<entry>.lua}         — [create] the plugin
examples/plugins/{hello,bye}/main.go                                — [create] two small programs the helper builds
```

## Order of work

### Task 1 — The plugin and its workspace

**Files:** all of the above

**Change:**
- First step: read `crates/buildl/tests/fixtures/plugin_path/` for the manifest keys and entry
  shape the binary accepts today (`[extension]` `name`, `version`, `entry`, `api`;
  `[capabilities]` `proc.run`, `env.read`, design §10) and follow it; then run one
  `go build -o $out ./hello` target through `buildl` in a scratch copy before writing the helper.
- The plugin's `extension.toml`: `name = "gobin"`, a `version`, its `entry`, `api = 1`;
  `[capabilities]` `proc.run = ["go"]`, `env.read = ["HOME"]`.
- The helper takes a target name and a package directory and declares one target running
  `go build -o $out ./<dir>` with `go.mod` and the directory's `.go` files as inputs and `HOME`
  in `env`; it declares only primitives (design §10).
- `buildl.toml`: the ceiling (`proc.run = ["go"]`, `env.read = ["HOME"]`, `fs.read`, `fs.write`)
  and `[plugins.gobin]` with `path = "plugins/gobin"`, `proc.run = ["go"]`, `env.read = ["HOME"]`.
- `assert.sh`: both programs print their expected lines; `buildl doctor` and `buildl plan` print
  `declared by plugin gobin@<version> for //…` (`crates/buildl-core/src/types/attribution.rs:30-35`);
  `buildl graph --json` gives both targets the plugin's origin.
- `README.md`: `buildl.use`, the manifest, the cap, attribution; `git` plugins and
  `buildl plugins fetch` described, not run.

**Proof:** `cargo make examples` passes with `plugins` included.

**Done when:** it passes, with `cargo make dod`. Commit `feat(repo): plugin example`.

## Risks

- **Manifest details** (`entry` file name, `api` value) — taken from the existing fixture, not guessed.
- **Chose not to:** a `git` plugin (needs a hosted repository and the network); one Go module per
  program (the action reads its paths from the workspace root, where `.` would be neither module).

## Proof

- `cargo make examples` green locally and in the CI `examples` job on both operating systems.

## Review findings

- false claim — the README and this plan said the action "runs at the workspace root", true only
  under `bare`; now "reads its paths from the workspace root". Fixed at `3b6737a` and here.
- robustness — `assert.sh` would stop with a Python traceback, not its named failure, if a
  target's origin were not a plugin; it now prints the origin and fails by name. Fixed.

## Probe results

- `plugin_path` fixture read for the manifest shape (`[extension]` `name`, `version`, `entry`,
  `api = 1`; `[capabilities]`); followed as is.
- Scratch copy, macOS: `plan` prints `go: declared by plugin gobin@0.1.0 for //:bye` and the same
  for `HOME`; `build` → `2 built`; `doctor` lists both wants with that line; `graph --json` gives
  `//:bye` and `//:hello` the origin `{"Plugins":[{"name":"gobin","version":"0.1.0",…}]}`.
- `BUILDL=target/debug/buildl sh examples/check.sh examples/plugins` → `plugins: ok`.
- CI run 40632551313 (at `7431351`): `Examples (ubuntu-latest)` and `Examples (macos-latest)`
  success with `plugins` included; again in run 40633614431 (at `b1cc0f0`), after the
  `assert.sh` fix.

## Deviations

- 2026-10-28 — `plan` and `doctor` name one target in the attribution line (`for //:bye`), so
  `assert.sh` matches `declared by plugin gobin@0.1.0 for //:`.
- 2026-10-28 — Task 1 also added a row to `examples/README.md`.
