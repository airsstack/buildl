---
status: done
created: 2026-11-11
depends-on: [01]
built-on: ba5384fc9684
---

# Release plugin-rust-v0.2.1 Implementation Plan

**Goal:** `plugin-rust-v0.2.1` and `buildl-cargo` 0.1.2 are published.

**Architecture:** Two outward steps after plan 01's pull request is merged to `main`: push the tag
at the merge commit, then publish the crate. Both are irreversible and outward-facing; each runs
on the author's word, never by a coder, as the 0.3.0 release did (PR #31: "Publishing to
crates.io can't be undone", run after the author said so).

**Tech Stack:** git, `cargo publish`.

**Content authority:** `intent.md` (spec skipped), "Desired outcome" and the constraint that the
published `plugin-rust-v0.2.0` tag is not moved.

---

## Files that change

```
(none in the repository)
origin        — tag plugin-rust-v0.2.1
crates.io     — buildl-cargo 0.1.2
```

## Order of work

### Task 1 — Push plugin-rust-v0.2.1

**Files:** none.

**Change:** On `main` at the merge commit of plan 01's pull request:
`git tag plugin-rust-v0.2.1 <merge commit>` and `git push origin plugin-rust-v0.2.1`.
`plugin-rust-v0.2.0` is not touched.

**Proof:** `git ls-remote --tags origin 'plugin-rust-*'` lists `plugin-rust-v0.2.1` at the merge
commit and `plugin-rust-v0.2.0` still at `d0717135…`;
`git show plugin-rust-v0.2.1:plugins/rust/extension.toml` → `version = "0.2.1"`;
`git diff --quiet 60f2d45 plugin-rust-v0.2.1 -- plugins/rust ':!plugins/rust/extension.toml'
':!plugins/rust/README.md'` exits 0 (the 0.2 plugin's code, unchanged); in a scratch copy of
`examples/cargo-buildl`, whose `build.lua` calls `rust.library` with `kind = { "staticlib" }`,
pointed at GitHub (`github = "airsstack/buildl"`, `rev = "plugin-rust-v0.2.1"`, no `prepare.sh`),
`buildl plugins fetch` then `buildl plan` succeeds and names `plugin rust@0.2.1`.

**Done when:** the three checks above hold. No commit.

### Task 2 — Publish buildl-cargo 0.1.2

**Files:** none.

**Change:** From `main` at the same commit: `cargo publish --dry-run -p buildl-cargo`, then
`cargo publish -p buildl-cargo`. No other crate is published (intent constraint).

**Proof:** `cargo info buildl-cargo@0.1.2` resolves from crates.io.

**Done when:** the check above holds. No commit.

---

## Risks

- Order matters: the tag goes first, so a `cargo buildl init` from the published 0.1.2 never pins
  a tag that does not exist.
- Both steps are irreversible: a pushed tag is read by anyone who fetches, a published version
  cannot be replaced (only yanked).

## Proof

- Task 1 and Task 2 checks; the board row for this chain reads `chain complete` once both plans
  are `done`.

## Deviations

- 2026-11-11: the author ran both steps from `main` by hand; the checks below were run after.
  Before the release: `cargo publish --dry-run -p buildl-cargo` packaged and compiled 0.1.2
  ("warning: aborting upload due to dry run").
  - `git ls-remote --tags origin 'plugin-rust-*'` → `plugin-rust-v0.2.1` at `43d7a542…` (the
    merge commit of PR #32), `plugin-rust-v0.2.0` still at `d0717135…`.
  - `git show 43d7a54:plugins/rust/extension.toml` → `version = "0.2.1"`; `git diff --quiet 60f2d45
    43d7a54 -- plugins/rust ':!plugins/rust/extension.toml' ':!plugins/rust/README.md'` → rc 0.
  - A scratch copy of `examples/cargo-buildl` (no `prepare.sh`, `github = "airsstack/buildl"`,
    `rev = "plugin-rust-v0.2.1"`): `buildl plugins fetch` → "fetched rust
    43d7a542023880fe37657ac8ba0564a25146f72a"; `buildl plan` → "declared by plugin rust@0.2.1
    for //:app", rc 0.
  - `cargo info buildl-cargo@0.1.2 --registry crates-io`, outside the workspace → "version: 0.1.2".
