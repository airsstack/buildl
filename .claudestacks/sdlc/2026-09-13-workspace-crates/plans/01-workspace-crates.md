---
status: approved
created: 2026-09-13
---

# Workspace Crates Implementation Plan

**Goal:** Bring the workspace to the four-crate dependency-inverted layout.

**Architecture:** Four members, listed in dependency order: `buildl-core` (no dependencies) ← `buildl-lua` (`buildl-core`, `airsl`) ← `buildl` (`buildl-core`, `buildl-lua`) ← `buildl-cli` (`buildl`). The edges are declared now, before any code uses them, so the compiler enforces the dependency rules from the first line of the `buildl-core` foundation chain onward. Every crate is a documented stub. Each `lib.rs` is export-only and carries the four-question `//!` block, with no items and no tests; each states in its own `//!` block that it is export-only, without naming an internal rule. No rustdoc or crate README references planning documents. The toolchain follows the `stable` channel, so the gate always runs on the latest stable release; `rust-version` moves to 1.94 because every airsl `0.1.x` release requires it.

**Tech Stack:** Rust stable (1.98.1 when checked; `rust-version` 1.94, edition 2024, resolver 3), Cargo workspaces, cargo-make (`cargo make dod`), cargo-deny 0.20, airsl `0.1` (resolves to 0.1.4 today).

**Content authority:** `.claudestacks/sdlc/2026-09-13-workspace-crates/intent.md` (approved, `spec: skipped`). The layout comes from `docs/architecture-building-blocks.md` §4 and the dependency rules from §7.

**Checkpoint:** stop for review after Task 4, when the crate graph is wired, before the wording and tooling cleanup.

---

## File structure

```
rust-toolchain.toml                 — modify  channel 1.91 → stable, comment for the stable channel
Cargo.toml                          — modify  rust-version, members, airsl "0.1", internal member entries, catalog comments
Cargo.lock                          — modify  new member entries and airsl's dependency graph
crates/buildl-core/Cargo.toml       — create  manifest, no dependencies
crates/buildl-core/README.md        — create  crate README
crates/buildl-core/src/lib.rs       — create  crate doc, export-only
crates/buildl-lua/Cargo.toml        — create  manifest: airsl + buildl-core
crates/buildl-lua/README.md         — create  crate README
crates/buildl-lua/src/lib.rs        — create  crate doc, export-only
crates/buildl/Cargo.toml            — modify  depend on buildl-core + buildl-lua; drop scaffold comment
crates/buildl/README.md             — modify  framework-crate description, no docs/ links
crates/buildl/src/lib.rs            — modify  composition-root crate doc
crates/buildl-cli/Cargo.toml        — modify  dependency comment without architecture.md reference
crates/buildl-cli/README.md         — modify  no design-phase status, no docs/ links
crates/buildl-cli/src/main.rs       — modify  binary doc without scaffold narration
Makefile.toml                       — modify  "neither crate" → "no crate"
.github/workflows/ci.yml            — modify  toolchain steps install or update stable; matrix comment names adapters, not exec/store modules
deny.toml                           — modify  drop unused-allowed-license override; comments for four members
CLAUDE.md                           — modify  status paragraph, workspace-shape heading, adapter dependency rule
```

Every command runs from the repository root. Facts these tasks depend on, the airsl facts checked on 2026-09-13 and the toolchain facts and dry run re-checked on 2026-09-15:

- crates.io lists airsl versions `0.1.0`–`0.1.4`, and every one declares `rust_version` `1.94` (`https://crates.io/api/v1/crates/airsl`).
- On rustc 1.91.1, a crate depending on `airsl = "0.1"` fails with `airsl@0.1.4 requires rustc 1.94`. On 1.94.1 it builds.
- `rustup check` reports the latest stable release as `1.98.1 (48a229cea 2026-09-01)`.
- `rustup toolchain install --help` (rustup 1.29.1) reads: "Install or update the given toolchains, or by default the active toolchain". Run with no argument, it installs or updates the toolchain `rust-toolchain.toml` names.
- `rust-version = "stable"` is rejected by cargo with `error: expected a version like "1.32"`, so `rust-version` carries a number.
- With this plan's final tree applied to a scratch copy of the repository on rustc 1.98.1, `cargo make dod` passes, `cargo deny check` prints `advisories ok, bans ok, licenses ok, sources ok` with the `unused-allowed-license` override removed, and `cargo tree -d` finds no duplicate versions for the host platform (`--target all` shows `syn` 2.0.119 beside 3.0.5, which `cargo deny check` accepts).

### Task 1 — Follow stable Rust with a 1.94 minimum

**Files:**
- Modify `rust-toolchain.toml`
- Modify `Cargo.toml`
- Modify `.github/workflows/ci.yml`

**Steps:**

1. Confirm the current settings:
   ```
   $ grep -n 'channel' rust-toolchain.toml
   $ grep -n 'rust-version' Cargo.toml
   7:rust-version = "1.91"
   $ grep -n 'pinned' .github/workflows/ci.yml
   ```
   Expected: `channel` reads `"1.91"` (or `"stable"`, if the channel was already switched in the working tree); `rust-version` reads `"1.91"`; `ci.yml` has three `pinned` matches (two step names, one comment).
2. Replace the whole of `rust-toolchain.toml` with:
   ```toml
   [toolchain]
   # Tracks the latest stable release, so every local build and CI run shows that
   # buildl compiles and passes the gate on current stable Rust. The oldest compiler
   # buildl supports is recorded separately, as `rust-version` in the workspace
   # manifest.
   channel = "stable"
   components = ["rustfmt", "clippy"]
   profile = "minimal"
   ```
3. In `Cargo.toml` under `[workspace.package]`, replace `rust-version = "1.91"` with:
   ```toml
   rust-version = "1.94"
   ```
4. In `.github/workflows/ci.yml` (the `dod` job), replace:
   ```yaml
         # `rustup show` resolves the override in rust-toolchain.toml and installs it.
         # The versions printed below are the record of which toolchain the gate ran on.
         - name: Install the pinned Rust toolchain
           run: |
             rustup show
   ```
   with:
   ```yaml
         # `rustup toolchain install` with no argument installs the toolchain
         # rust-toolchain.toml names, or updates it when the runner image carries an
         # older stable. The versions printed below are the record of which toolchain
         # the gate ran on.
         - name: Install the latest stable Rust toolchain
           run: |
             rustup toolchain install
             rustup show
   ```
5. In `.github/workflows/ci.yml` (the `deny` job), replace:
   ```yaml
         # cargo-deny shells out to `cargo metadata`, and `cargo` here is a rustup
         # shim that resolves rust-toolchain.toml, so the pinned toolchain gets
         # fetched either way. Doing it in its own step keeps that cost out of the
         # check's log.
         - name: Install the pinned Rust toolchain
           run: rustup show
   ```
   with:
   ```yaml
         # cargo-deny shells out to `cargo metadata`, and `cargo` here is a rustup
         # shim that resolves rust-toolchain.toml, so the toolchain it names is
         # needed either way. Installing it in its own step keeps that cost out of
         # the check's log.
         - name: Install the latest stable Rust toolchain
           run: rustup toolchain install
   ```
6. Verify:
   ```
   $ rustup toolchain install
   info: the active toolchain `stable-aarch64-apple-darwin` has been installed
   $ rustc --version
   rustc 1.98.1 (48a229cea 2026-09-01)
   $ grep -n 'pinned' .github/workflows/ci.yml
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: rustc reports the latest stable release (1.98.1 when checked; a newer stable is equally correct), grep prints nothing, and the gate exits 0. The host triple in the `info:` line varies by machine.
7. Commit `build(repo): follow stable Rust with a 1.94 minimum`.

### Task 2 — Add the buildl-core crate

**Files:**
- Create `crates/buildl-core/Cargo.toml`
- Create `crates/buildl-core/README.md`
- Create `crates/buildl-core/src/lib.rs`
- Modify `Cargo.toml`
- Modify `Cargo.lock`

**Steps:**

1. Confirm the crate does not exist yet:
   ```
   $ cargo tree -p buildl-core
   error: package ID specification `buildl-core` did not match any packages

   help: a package with a similar name exists: `buildl-cli`
   ```
2. Create `crates/buildl-core/Cargo.toml`:
   ```toml
   [package]
   name         = "buildl-core"
   version      = "0.1.0"
   description  = "Domain data, ports, and pure pipeline logic for the buildl build system — no I/O, no Lua."
   readme       = "README.md"
   edition.workspace      = true
   rust-version.workspace = true
   license.workspace      = true
   repository.workspace   = true
   authors.workspace      = true

   # No buildl crate, no airsl, no I/O library — ever. Pure-computation crates from
   # the workspace catalog (serde, serde_json, thiserror, sha2) are the only
   # dependencies this crate may take, added as the code that needs them lands.
   [dependencies]

   [lints]
   workspace = true
   ```
3. Create `crates/buildl-core/src/lib.rs`:
   ```rust
   //! Domain data, ports, and pure pipeline logic for the buildl build system.
   //!
   //! This crate is the center buildl's other crates depend on. It depends on no
   //! other buildl crate, on no Lua runtime, and on no filesystem, process,
   //! thread, environment, or clock API, so every pipeline flow can be exercised
   //! against in-memory implementations of its ports.
   //!
   //! # Responsibilities
   //!
   //! - The domain values the pipeline phases hand to one another.
   //! - The ports: traits through which the pipeline reaches build-file
   //!   evaluation, storage, hashing, and execution.
   //! - The pure logic of each phase: Load → Resolve → Plan → Execute → Record.
   //!
   //! # Non-responsibilities
   //!
   //! - Implementing any port. Concrete adapters live in the `buildl-lua` and
   //!   `buildl` crates.
   //!
   //! This file holds only module declarations and re-exports, so it carries no
   //! logic to unit-test.
   ```
4. Create `crates/buildl-core/README.md`:
   ```markdown
   # buildl-core

   Domain data, ports, and pure pipeline logic for the buildl build system.

   This crate performs no I/O and knows nothing about Lua: it is the home for the values the pipeline phases exchange, the traits (ports) through which the pipeline reaches the outside world, and the logic of every phase. Concrete implementations of the ports live in [`buildl-lua`](../buildl-lua) and [`buildl`](../buildl).

   **Status:** pre-release; the crate has no public API.

   ## License

   Apache-2.0
   ```
5. In `Cargo.toml`, replace the `members` line `members  = ["crates/buildl", "crates/buildl-cli"]` with:
   ```toml
   members  = [
       "crates/buildl-core",
       "crates/buildl",
       "crates/buildl-cli",
   ]
   ```
6. In `Cargo.toml`, replace the internal-member block at the end of `[workspace.dependencies]`:
   ```toml
   # Internal workspace member — path for local builds, version for publish-readiness.
   # `buildl-cli` inherits this with `{ workspace = true }`. Cargo strips the path and
   # keeps the version when packaging, so the published crate depends on crates.io.
   buildl     = { version = "0.1.0", path = "crates/buildl" }
   ```
   with:
   ```toml
   # Internal workspace members, in dependency order — path for local builds, version
   # for publish-readiness. Members inherit these with `{ workspace = true }`. Cargo
   # strips the path and keeps the version when packaging, so a published crate
   # depends on crates.io.
   buildl-core = { version = "0.1.0", path = "crates/buildl-core" }
   buildl      = { version = "0.1.0", path = "crates/buildl" }
   ```
7. Verify:
   ```
   $ cargo tree -p buildl-core -e normal --depth 1 --prefix none
   buildl-core v0.1.0 (<repo>/crates/buildl-core)
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: `buildl-core` resolves with no dependencies, `Cargo.lock` gains a `buildl-core` entry, and the gate exits 0.
8. Commit `feat(buildl-core): add the buildl-core crate`, including `Cargo.lock`.

### Task 3 — Add the buildl-lua crate on airsl 0.1

**Files:**
- Create `crates/buildl-lua/Cargo.toml`
- Create `crates/buildl-lua/README.md`
- Create `crates/buildl-lua/src/lib.rs`
- Modify `Cargo.toml`
- Modify `Cargo.lock`

**Steps:**

1. Confirm the crate does not exist yet:
   ```
   $ cargo tree -p buildl-lua
   error: package ID specification `buildl-lua` did not match any packages

   help: a package with a similar name exists: `buildl-cli`
   ```
2. In `Cargo.toml`, replace the airsl block:
   ```toml
   # The embedded Lua runtime buildl declares its build files on. A dependency of the
   # `declare` module ONLY (architecture.md §1.1) — no other module may know Lua exists,
   # so an airsl upgrade's blast radius stays one module deep. Pulls in mlua/lua54
   # vendored transitively, so a C compiler is required to build this workspace.
   airsl      = "0.1.3"
   ```
   with:
   ```toml
   # The embedded Lua runtime buildl declares its build files on. A dependency of the
   # `buildl-lua` crate ONLY — no other crate may know Lua exists, so an airsl
   # upgrade's blast radius stays one crate deep. The requirement accepts the newest
   # 0.1.x release; airsl 0.1 requires rustc 1.94, which sets this workspace's
   # rust-version. Pulls in mlua/lua54 vendored transitively, so a C compiler is
   # required to build this workspace.
   airsl      = "0.1"
   ```
3. In `Cargo.toml`, replace the `members` array with:
   ```toml
   members  = [
       "crates/buildl-core",
       "crates/buildl-lua",
       "crates/buildl",
       "crates/buildl-cli",
   ]
   ```
4. In `Cargo.toml`, insert this line between the `buildl-core` and `buildl` internal-member entries:
   ```toml
   buildl-lua  = { version = "0.1.0", path = "crates/buildl-lua" }
   ```
5. Create `crates/buildl-lua/Cargo.toml`:
   ```toml
   [package]
   name         = "buildl-lua"
   version      = "0.1.0"
   description  = "Lua build-file evaluation for buildl, on the airsl embedded runtime."
   readme       = "README.md"
   edition.workspace      = true
   rust-version.workspace = true
   license.workspace      = true
   repository.workspace   = true
   authors.workspace      = true

   [dependencies]
   # The only buildl crate that depends on a Lua runtime.
   airsl       = { workspace = true }
   # The ports this crate implements.
   buildl-core = { workspace = true }

   [lints]
   workspace = true
   ```
6. Create `crates/buildl-lua/src/lib.rs`:
   ```rust
   //! Lua build-file evaluation for buildl, on the airsl embedded runtime.
   //!
   //! This crate is the only part of buildl that depends on a Lua runtime, so
   //! the effect of an airsl upgrade stays within one crate.
   //!
   //! # Responsibilities
   //!
   //! - Evaluating one build file per call inside an airsl sandbox and returning
   //!   the declarations it staged.
   //! - Installing the `buildl` module table, reachable from Lua both as
   //!   `airsstack.buildl` and as the global `buildl`.
   //!
   //! # Non-responsibilities
   //!
   //! - Resolving, planning, scheduling, caching, or executing anything a build
   //!   file declares.
   //!
   //! This file holds only module declarations and re-exports, so it carries no
   //! logic to unit-test.
   ```
7. Create `crates/buildl-lua/README.md`:
   ```markdown
   # buildl-lua

   Lua build-file evaluation for buildl, on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

   This is the only buildl crate that depends on a Lua runtime. Its role is to evaluate build files inside an airsl sandbox and to install the `buildl` module table that build files declare targets through, implementing a port defined in [`buildl-core`](../buildl-core).

   **Status:** pre-release; the crate has no public API.

   ## License

   Apache-2.0
   ```
8. Verify:
   ```
   $ cargo tree -p buildl-lua -e normal --depth 1 --prefix none
   buildl-lua v0.1.0 (<repo>/crates/buildl-lua)
   airsl v0.1.4
   buildl-core v0.1.0 (<repo>/crates/buildl-core)
   $ cargo tree -d
   warning: nothing to print.

   To find dependencies that require specific target platforms, try to use option `--target all` first, and then narrow your search scope accordingly.
   $ cargo update -p airsl --dry-run
       Updating crates.io index
        Locking 0 packages to latest Rust 1.94 compatible versions
   note: pass `--verbose` to see 2 unchanged dependencies behind latest
   warning: not updating lockfile due to dry run
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: `buildl-lua` depends on exactly `airsl` (0.1.4, or a newer 0.1.x if one has been published) and `buildl-core`. `cargo tree -d` finds no duplicate versions for the host platform. With `--target all` it lists one: `syn` 2.0.119 (via `jiff-static`, a proc-macro under airsl's `jiff`) alongside `syn` 3.0.5, and Task 7's `cargo deny check` accepts it. `Locking 0 packages` means airsl already resolves to the newest compatible 0.1.x; the `note:` count may differ. The gate exits 0, and the first build compiles vendored Lua from C. `Cargo.lock` now contains airsl's dependency graph.
9. Commit `feat(buildl-lua): add the buildl-lua crate on airsl 0.1`, including `Cargo.lock`.

### Task 4 — Wire buildl as the composition root

**Files:**
- Modify `crates/buildl/Cargo.toml`
- Modify `crates/buildl/src/lib.rs`
- Modify `crates/buildl/README.md`
- Modify `Cargo.lock`

**Steps:**

1. Confirm `buildl` has no library dependencies yet:
   ```
   $ cargo tree -p buildl -e normal --depth 1 --prefix none
   buildl v0.1.0 (<repo>/crates/buildl)
   ```
2. In `crates/buildl/Cargo.toml`, replace:
   ```toml
   # Empty scaffold — the design phase is not closed (the depfile question,
   # docs/fundamental-walkthrough.md §6). Dependencies are added from the
   # workspace catalog as modules land; none are used yet.
   [dependencies]
   ```
   with:
   ```toml
   [dependencies]
   # The pipeline, its ports, and the domain values this crate's adapters serve.
   buildl-core = { workspace = true }
   # The Lua build-file adapter, bound to the pipeline here alongside the others.
   buildl-lua  = { workspace = true }
   ```
3. Replace the whole of `crates/buildl/src/lib.rs` with:
   ```rust
   //! buildl — a sandboxed, deterministic build system whose build files are
   //! written in Lua and evaluated on the airsl embedded runtime.
   //!
   //! Build files *declare* targets; buildl schedules, sandboxes, parallelizes,
   //! and caches their execution. The pipeline is
   //! Load → Resolve → Plan → Execute → Record, with each phase handing the next
   //! a serializable value.
   //!
   //! This crate is the composition root: its role is to choose a concrete
   //! implementation for every port the `buildl-core` pipeline needs.
   //!
   //! # Responsibilities
   //!
   //! - The filesystem, process, thread, and clock adapters for the pipeline's
   //!   ports.
   //! - Binding those adapters, together with the Lua adapter from `buildl-lua`,
   //!   to the `buildl-core` pipeline.
   //!
   //! # Non-responsibilities
   //!
   //! - Pipeline logic, which lives in `buildl-core`.
   //! - Lua evaluation, which lives in `buildl-lua`.
   //!
   //! This file holds only module declarations and re-exports, so it carries no
   //! logic to unit-test.
   ```
4. Replace the whole of `crates/buildl/README.md` with:
   ```markdown
   # buildl

   A sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

   This is the framework crate. Its role is to bind concrete adapters — storage, hashing, execution, and the Lua evaluator from [`buildl-lua`](../buildl-lua) — to the pipeline defined in [`buildl-core`](../buildl-core). The `buildl` binary in [`buildl-cli`](../buildl-cli) is a thin shell over it.

   **Status:** pre-release; the crate has no public API.

   ## License

   Apache-2.0
   ```
5. Verify:
   ```
   $ cargo tree -p buildl -e normal --depth 1 --prefix none
   buildl v0.1.0 (<repo>/crates/buildl)
   buildl-core v0.1.0 (<repo>/crates/buildl-core)
   buildl-lua v0.1.0 (<repo>/crates/buildl-lua)
   $ cargo tree -p buildl-cli -e normal --depth 1 --prefix none
   buildl-cli v0.1.0 (<repo>/crates/buildl-cli)
   buildl v0.1.0 (<repo>/crates/buildl)
   $ cargo tree -p buildl-core -e normal --depth 1 --prefix none
   buildl-core v0.1.0 (<repo>/crates/buildl-core)
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: the four direct-dependency sets match the Architecture line exactly, `Cargo.lock` lists `buildl-core` and `buildl-lua` under the `buildl` entry, and the gate exits 0.
6. Commit `build(buildl): depend on buildl-core and buildl-lua`, including `Cargo.lock`.

**Checkpoint:** stop here for review before Task 5.

### Task 5 — Describe buildl-cli as it stands

**Files:**
- Modify `crates/buildl-cli/Cargo.toml`
- Modify `crates/buildl-cli/src/main.rs`
- Modify `crates/buildl-cli/README.md`

**Steps:**

1. Confirm the stale wording is present:
   ```
   $ grep -rn 'scaffold\|design phase\|architecture\.md\|docs/' crates/buildl-cli
   ```
   Expected: matches in `Cargo.toml` (`architecture.md §1`), `src/main.rs` (`scaffold`) and `README.md` (`design phase`, `docs/`).
2. In `crates/buildl-cli/Cargo.toml`, replace:
   ```toml
   # The library carries all logic; this crate is argument parsing and phase
   # orchestration only (architecture.md §1).
   buildl = { workspace = true }
   ```
   with:
   ```toml
   # The binary's only library dependency. All build logic lives in the library
   # crates, and core types reach the binary through `buildl`, never directly.
   buildl = { workspace = true }
   ```
3. Replace the whole of `crates/buildl-cli/src/main.rs` with:
   ```rust
   //! The `buildl` binary — the command-line entry point to the `buildl` library.
   //!
   //! All build logic lives in the library crates; this binary depends on the
   //! `buildl` crate alone.

   const fn main() {}
   ```
4. Replace the whole of `crates/buildl-cli/README.md` with:
   ```markdown
   # buildl-cli

   The `buildl` binary — the command-line entry point to the [`buildl`](../buildl) build system library.

   **Status:** pre-release; the binary provides no commands.

   ## License

   Apache-2.0
   ```
5. Verify:
   ```
   $ grep -rn 'scaffold\|design phase\|architecture\.md\|docs/' crates/buildl-cli
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: grep prints nothing and exits 1; the gate exits 0.
6. Commit `docs(buildl-cli): describe the binary as it stands`.

### Task 6 — Name crates and adapters in tooling comments

**Files:**
- Modify `Cargo.toml`
- Modify `Makefile.toml`
- Modify `.github/workflows/ci.yml`

**Steps:**

1. Confirm the single-crate and two-crate wording is present:
   ```
   $ grep -n 'enum in the library\|`plan` module\|in `manifest`\|in `store`\|neither crate\|exec/store' Cargo.toml Makefile.toml .github/workflows/ci.yml
   ```
   Expected: six matches. `Cargo.toml` has four (thiserror, rayon, toml, tempfile comments). `Makefile.toml` has one (`neither crate`). `ci.yml` has one (`exec/store`).
2. In `Cargo.toml`, replace:
   ```toml
   # Errors. One structured enum in the library, airsl-style — structured fields,
   ```
   with:
   ```toml
   # Errors. One structured enum in `buildl-core`, airsl-style — structured fields,
   ```
3. In `Cargo.toml`, replace:
   ```toml
   # Parallel file hashing in the Plan phase. Confined to the `plan` module (decision
   # record §6) so the executor's plain-threads model stays singular.
   ```
   with:
   ```toml
   # Parallel file hashing in the Plan phase. Confined to the `Digester` adapter in
   # `buildl` (decision record §6) so the executor's plain-threads model stays singular.
   ```
4. In `Cargo.toml`, replace:
   ```toml
   # buildl.toml (the ceiling) parsing in `manifest`. Read-only, like airsl's
   # extension manifests: the `display` (serialization) default feature is dropped.
   ```
   with:
   ```toml
   # buildl.toml (the ceiling) parsing in the `Manifest` adapter. Read-only, like
   # airsl's extension manifests: the `display` (serialization) default feature is dropped.
   ```
5. In `Cargo.toml`, replace:
   ```toml
   # atomic writes in `store` (architecture.md §3.5).
   ```
   with:
   ```toml
   # atomic writes in the storage adapters (architecture-building-blocks.md §6).
   ```
6. In `Makefile.toml`, replace:
   ```toml
   # `--all-features` below is carried for forward-safety: neither crate in this
   ```
   with:
   ```toml
   # `--all-features` below is carried for forward-safety: no crate in this
   ```
7. In `.github/workflows/ci.yml`, replace:
   ```yaml
       # The matrix mirrors airsl's: once `airsl` lands in the dependency graph it
       # compiles vendored Lua 5.4 from C, and the exec/store modules touch paths,
       # processes and filesystems in platform-specific ways. A break there is
       # invisible on a single runner, so the matrix is in place from day one.
   ```
   with:
   ```yaml
       # The matrix mirrors airsl's: `airsl` compiles vendored Lua 5.4 from C, and
       # the execution and storage adapters touch paths, processes and filesystems
       # in platform-specific ways. A break there is invisible on a single runner.
   ```
8. Verify:
   ```
   $ grep -n 'enum in the library\|`plan` module\|in `manifest`\|in `store`\|neither crate\|exec/store' Cargo.toml Makefile.toml .github/workflows/ci.yml
   $ cargo metadata --format-version 1 > /dev/null && echo metadata-ok
   metadata-ok
   $ cargo make dod
   [cargo-make] INFO - Build Done in <n> seconds.
   ```
   Expected: grep prints nothing; the workspace metadata still resolves; the gate exits 0.
9. Commit `docs(repo): name crates and adapters in tooling comments`.

### Task 7 — Enforce unused license allowances in cargo-deny

**Files:**
- Modify `deny.toml`

**Steps:**

1. Confirm the scaffold-era override is present:
   ```
   $ grep -n 'unused-allowed-license\|empty scaffolds\|planned\|Both workspace members\|pins every direct one' deny.toml
   ```
   Expected: matches for all five patterns.
2. In `deny.toml`, replace:
   ```toml
   # Permissive licenses only, seeded from airsl's graph — the set buildl's planned
   # dependency catalog resolves into.
   ```
   with:
   ```toml
   # Permissive licenses only, seeded from airsl's graph — the set buildl's
   # dependency catalog resolves into.
   ```
3. In `deny.toml`, delete these lines, which directly follow `exceptions = []`:
   ```toml
   # The member crates are still empty scaffolds, so most of the allow list is not
   # yet exercised by the graph. Without this, every unmatched allowance warns on
   # each run until the dependency catalog is actually consumed. Remove once the
   # first real dependencies land.
   unused-allowed-license = "allow"
   ```
4. In `deny.toml`, replace:
   ```toml
   # Both workspace members are published to crates.io under Apache-2.0, so their
   # license text is a distribution question and is checked like any other crate.
   ```
   with:
   ```toml
   # Every workspace member is published to crates.io under Apache-2.0, so its
   # license text is a distribution question and is checked like any other crate.
   ```
5. In `deny.toml`, replace:
   ```toml
   # No `version = "*"` dependencies. The root manifest pins every direct one.
   ```
   with:
   ```toml
   # No `version = "*"` dependencies. The root manifest gives every direct one an
   # explicit version requirement.
   ```
6. Verify:
   ```
   $ grep -n 'unused-allowed-license\|empty scaffolds\|planned\|Both workspace members\|pins every direct one' deny.toml
   $ cargo deny check
   advisories ok, bans ok, licenses ok, sources ok
   ```
   Expected: grep prints nothing. cargo-deny passes with no `license-not-encountered` warning, which shows that airsl's graph uses every allowed license.
7. Commit `build(repo): enforce unused license allowances in cargo-deny`.

### Task 8 — Record the four-crate scaffold in CLAUDE.md

**Files:**
- Modify `CLAUDE.md`

**Steps:**

1. Confirm the stale status is present:
   ```
   $ grep -n 'planned, not yet created\|## Planned workspace shape\|never on each other' CLAUDE.md
   ```
   Expected: three matches.
2. In `CLAUDE.md`, replace the sentence:
   ```markdown
   The repository holds the design documents plus a workspace scaffold: the `buildl` and `buildl-cli` crates exist as doc-comment stubs with no dependencies wired (`buildl-core` and `buildl-lua` are planned, not yet created), and the `cargo make dod` gate and CI matrix are live. No pipeline module is implemented.
   ```
   with:
   ```markdown
   The repository holds the design documents plus a workspace scaffold: all four crates — `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — exist as doc-comment stubs with their dependency edges wired per `architecture-building-blocks.md` §7 (airsl `0.1`, only in `buildl-lua`; toolchain `stable`, `rust-version` 1.94), and the `cargo make dod` gate, `cargo deny` check and CI matrix are live. No pipeline module is implemented.
   ```
3. In `CLAUDE.md`, replace the heading `## Planned workspace shape` with:
   ```markdown
   ## Workspace shape
   ```
4. In `CLAUDE.md`, in the `Dependency inversion` bullet under the structural rules, replace:
   ```markdown
   Adapters (`buildl-lua`, `buildl`) depend on `buildl-core` only, never on each other. `buildl` is the single composition root.
   ```
   with:
   ```markdown
   Adapters depend on `buildl-core`; `buildl-lua` never depends on `buildl`, and adapter modules inside `buildl` never import each other. `buildl` is the single composition root, and the only crate that depends on another adapter crate (`buildl-lua`).
   ```
5. Verify:
   ```
   $ grep -n 'planned, not yet created\|## Planned workspace shape\|never on each other' CLAUDE.md
   $ grep -n '## Workspace shape\|single composition root, and the only crate' CLAUDE.md
   ```
   Expected: the first grep prints nothing; the second prints two lines.
6. Commit `docs(repo): record the four-crate scaffold in CLAUDE.md`.

---

## Verification summary (plan-level)

- `cargo make dod` exits 0 on the stable toolchain (1.98.1 when checked), with `rust-version` 1.94.
- `cargo deny check` prints `advisories ok, bans ok, licenses ok, sources ok`.
- `cargo tree -p <crate> -e normal --depth 1 --prefix none` gives exactly these direct dependencies: `buildl-core` → none; `buildl-lua` → `airsl`, `buildl-core`; `buildl` → `buildl-core`, `buildl-lua`; `buildl-cli` → `buildl`.
- `cargo tree -d` prints `warning: nothing to print.` for the host platform. With `--target all`, the only duplicate is `syn` (2.0.119 via `jiff-static`, and 3.0.5), which `cargo deny check` accepts.
- `cargo update -p airsl --dry-run` reports `Locking 0 packages`: airsl is at the newest compatible 0.1.x.
- `grep -rn 'scaffold\|design phase\|docs/\|depfile\|architecture\.md\|design\.md' crates` prints nothing; no rustdoc or crate README references planning documents.
- `git diff --stat <commit before Task 1> HEAD -- docs/` is empty: no design document changed.
