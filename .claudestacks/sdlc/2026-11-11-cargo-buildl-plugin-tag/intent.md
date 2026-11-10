---
status: draft
created: 2026-11-11
---

# Intent: cargo buildl init should not need a release for each rust plugin tag

## Problem

`buildl-cargo` holds the rust plugin's tag as a constant (`RUST_PLUGIN_TAG` in
`crates/buildl-cargo/src/scaffold.rs`), and `cargo buildl init` writes it into every new
`buildl.toml`. Each new plugin tag therefore needs a new `buildl-cargo` release before new
projects get it. The author, 2026-11-11: "I think we should not hardcode the tag name like that,
each time there is a new tag we have to update this crate, it's not effective" — and, keeping the
constant for the `2026-11-11-rust-plugin-tag` fix: "lets just keep like current, but later we
need to solve it".

The constant was decided with `cargo-buildl` (2026-11-09) so that each release pins the plugin it
was tested with: the generated section of `build.lua` calls the plugin's API.

Options raised on 2026-11-11, none chosen: look up the newest tag in a compatible line
(`plugin-rust-v0.2.*`) when `init` runs and write it out in full; look up the newest tag of any
version; keep the constant.

## Affected systems

Not explored yet.

## Desired outcome

Not explored yet.

## Constraints

Not explored yet.

## Non-goals

Not explored yet.
