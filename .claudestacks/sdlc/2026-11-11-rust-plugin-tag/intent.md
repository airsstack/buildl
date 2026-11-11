---
status: approved
created: 2026-11-11
spec: skipped
---

# Intent: the rust plugin's 0.2 tag names the 0.1.0 plugin

## Problem

The published tag `plugin-rust-v0.2.0` points at commit `d0717135`, the same commit as
`plugin-rust-v0.1.0` (`git ls-remote --tags origin 'plugin-*'`). At that commit
`plugins/rust/extension.toml` says `version = "0.1.0"`; the commit that made the plugin 0.2.0,
`60f2d45` ("bump the rust plugin to 0.2.0, kind as the crate-type list"), is not in the tag.

So every workspace that pins `plugin-rust-v0.2.0` runs the 0.1.0 plugin: `rust.library` takes
`kind` as a string and accepts `output`, while the README, the book and the CHANGELOG describe
0.2.0, where `kind` is a list and `output` is refused. `cargo buildl init` (buildl-cargo 0.1.0 and
0.1.1) pins that tag for every new project, and `plan` and `doctor` name the plugin `rust@0.1.0`.
The examples did not show it: each `prepare.sh` tags the working tree in a local repository.

Found 2026-11-11 by the spec review of `2026-11-11-buildl-builds-buildl`, which pins the rust
plugin and waits on this fix.

## Affected systems

- `plugins/rust/extension.toml` (the version).
- `crates/buildl-cargo`: `RUST_PLUGIN_TAG` in `src/scaffold.rs` and the tests that name it.
- Every file that names `plugin-rust-v0.2.0`: `plugins/rust/README.md`, `book/src/ch08-05`,
  `book/src/ch08-06`, `skills/buildl/SKILL.md`, `skills/buildl/references/plugins.md`, the
  `rust`, `rust-workspace`, `rust-ffi`, `polyglot` and `cargo-buildl` examples (`buildl.toml`,
  `README.md`, `prepare.sh`), and `CHANGELOG.md`.
- The release: a new plugin tag and a `buildl-cargo` publish.

## Desired outcome

- A new tag, `plugin-rust-v0.2.1`, at a commit whose `plugins/rust` is the 0.2 plugin with
  `version = "0.2.1"`; `buildl plugins fetch` of it gives a plugin that `plan` names `rust@0.2.1`
  and whose `rust.library` takes `kind` as a list.
- `cargo buildl init` pins `plugin-rust-v0.2.1`, released as `buildl-cargo` 0.1.2.
- Every file above names `plugin-rust-v0.2.1`; the CHANGELOG says what was wrong with
  `plugin-rust-v0.2.0` and that 0.2.1 is the plugin 0.2.0 described.

## Constraints

- The published `plugin-rust-v0.2.0` tag is not moved or deleted (decided 2026-11-11).
- The plugin's code does not change; only its version.
- `buildl-core`, `buildl-lua`, `buildl` and `buildl-cli` are not released.

## Non-goals

- Changing what the rust plugin does.
- The `c` plugin: `plugin-c-v0.2.0` is correct (`git diff --stat d0717135 HEAD -- plugins/c` →
  no change).
- Building buildl with buildl: the `2026-11-11-buildl-builds-buildl` chain, which pins
  `plugin-rust-v0.2.1` once it exists.

Spec skipped: a version bump, a tag, a constant and the files that name it; no design question.
