---
status: done
created: 2026-10-08
---

# buildl-lua Workspace Configuration Implementation Plan

**Goal:** The workspace manifests accept `buildl-lua`'s first code under every gate.

**Architecture:** Four manifest-only edits, no Rust source. The airsl requirement gains the floor the adapter is tested against. One nursery lint that contradicts rustc's `unreachable_pub` is allowed workspace-wide. `buildl-lua` gains `globset` and `walkdir` (host-side globbing) and the dev-dependency `tempfile` (symlink fixtures). The two supply-chain guards are told about those edges: the `guard-crate-edges` golden file, and the `deny.toml` wrapper lists that name which crates may depend on the three banned crates directly. Nothing here has behaviour, so the red step of each task is a failing gate rather than a failing unit test.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §10 (dependencies, the `deny.toml` and golden-file rows, the lint row), D11 (airsl floor), §1.1 C1–C3 (the gate runs these edits were proven against).

---

## Context an implementer needs

**Conventions every task follows.** The workspace lints are strict. `unwrap`, `expect` and `panic!`
are denied outside tests, pedantic and nursery clippy run at `-D warnings`, and `missing_docs` and
`unreachable_pub` are on. Crate-internal items are therefore `pub(crate)` inside private modules. A
test module opens with `#![expect(clippy::unwrap_used, reason = …)]`, adding `clippy::panic` where
it panics. `lib.rs` holds only module docs, `mod` declarations and `pub use` re-exports. Every file
opens with `//!` docs naming its responsibilities and non-responsibilities, and carries no internal
planning vocabulary. Every commit is Conventional Commits with scope `buildl-lua`, or `repo` for
workspace files. Every task ends green on `cargo make dod` before it commits.

**Where the code comes from.** Every code block below was compiled, linted and tested in a scratch
copy of the workspace at every task boundary of this plan set, on macOS with rustc 1.98 and clippy
1.98. The expected outputs are the outputs those runs printed.

| Fact | Where |
|---|---|
| The airsl requirement and its comment | `Cargo.toml:18-24` |
| The workspace clippy lint table; `unreachable_pub = "warn"` is in the rust table above it | `Cargo.toml:62-77` |
| `globset = "0.4"`, `walkdir = "2.5"`, `tempfile = "3.23.0"` are already catalogued | `Cargo.toml:44-52` |
| `Cargo.lock` already resolves airsl 0.1.4, globset 0.4.20, walkdir 2.5.0, tempfile 3.27.0 | `Cargo.lock` |
| The three bans with `wrappers = ["airsl"]` | `deny.toml:93-98` |
| The golden file of direct edges, which `cargo make guard-crate-edges` diffs against `cargo tree` | `crates/expected-edges.txt`, `Makefile.toml:121-143` |

The lint allowance is needed before any `buildl-lua` code exists. This toolchain's clippy fires the
nursery lint `redundant_pub_crate` on every `pub(crate)` item in a private module, and rustc's
`unreachable_pub` fires on `pub` there, so without the allowance no internal item can be written at
all. airsl writes crate-internal items the same way (airsl-0.1.4 `src/instruction_budget.rs:43-103`).

## File structure

```text
Cargo.toml                         — [modify] airsl floor 0.1.4; allow redundant_pub_crate      (Tasks 1, 2)
crates/buildl-lua/Cargo.toml       — [modify] globset, walkdir; dev-dependency tempfile         (Task 3)
crates/expected-edges.txt          — [modify] the three new buildl-lua edges                     (Task 3)
deny.toml                          — [modify] buildl-lua added to three ban wrapper lists        (Task 4)
```

### Task 1 — Raise the airsl floor to 0.1.4

**Files:**
- Modify `Cargo.toml`

**Steps:**

1. In `Cargo.toml`, replace

   ```toml
   # upgrade's blast radius stays one crate deep. The requirement accepts the newest
   # 0.1.x release; airsl 0.1 requires rustc 1.94, which sets this workspace's
   # rust-version. Pulls in mlua/lua54 vendored transitively, so a C compiler is
   # required to build this workspace.
   airsl      = "0.1"
   ```

   with

   ```toml
   # upgrade's blast radius stays one crate deep. The floor is the 0.1.x release the
   # adapter is tested against, and the requirement accepts any newer 0.1.x; airsl 0.1
   # requires rustc 1.94, which sets this workspace's rust-version. Pulls in mlua/lua54
   # vendored transitively, so a C compiler is required to build this workspace.
   airsl      = "0.1.4"
   ```

2. Confirm the lock file already satisfies it and nothing re-resolves:

   ```bash
   cargo tree -p buildl-lua --depth 1 --prefix none | grep airsl
   ```

   ```text
   airsl v0.1.4
   ```

3. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
4. Commit: `build(repo): raise the airsl floor to the tested 0.1.4`.

### Task 2 — Allow the nursery lint that contradicts `unreachable_pub`

**Files:**
- Modify `Cargo.toml`

**Steps:**

1. In `Cargo.toml`'s `[workspace.lints.clippy]` table, replace the line `missing_panics_doc = "warn"` with

   ```toml
   missing_panics_doc = "warn"
   # A nursery lint that contradicts rustc's `unreachable_pub` above: an item in a
   # private module must be `pub(crate)` for one and `pub` for the other. The rustc
   # lint wins, so crate-internal items are written `pub(crate)`.
   redundant_pub_crate = "allow"
   ```

2. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
3. Commit: `build(repo): allow redundant_pub_crate, which contradicts unreachable_pub`.

### Task 3 — Give `buildl-lua` its globbing and fixture dependencies

**Files:**
- Modify `crates/buildl-lua/Cargo.toml`
- Modify `crates/expected-edges.txt`

**Steps:**

1. In `crates/buildl-lua/Cargo.toml`, replace the whole `[dependencies]` table (its header and the
   four lines under it, ending at `buildl-core = { workspace = true }`) with

   ```toml
   [dependencies]
   # The only buildl crate that depends on a Lua runtime.
   airsl       = { workspace = true }
   # The ports this crate implements.
   buildl-core = { workspace = true }
   # `buildl.sources()` matching, with the glob semantics of airsl's own `glob` module.
   globset     = { workspace = true }
   # `buildl.sources()` traversal of the declaring directory, host-side.
   walkdir     = { workspace = true }

   [dev-dependencies]
   # Fixture trees that need a symlink, built at test time rather than checked in.
   tempfile    = { workspace = true }
   ```

2. Run the edge guard and confirm it fails on the three new edges:

   ```bash
   cargo make guard-crate-edges
   ```

   ```text
   @@ -6,6 +6,9 @@
    # buildl-lua
    airsl
    buildl_core
   +globset
   +walkdir
   +tempfile
    # buildl
    buildl_core
    buildl_lua
   guard: crate edges do not match crates/expected-edges.txt
          fix the manifest, or update the golden file if the change is intended
   ```

3. In `crates/expected-edges.txt`, add the lines `globset`, `walkdir` and `tempfile`, in that order,
   after `buildl_core` in the `# buildl-lua` block. This is `cargo tree`'s order: normal
   dependencies first, then dev-dependencies. The block then reads:

   ```text
   # buildl-lua
   airsl
   buildl_core
   globset
   walkdir
   tempfile
   ```

4. Run `cargo make guard-crate-edges` again. It exits `0`.
5. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.

   `cargo deny check` fails at this point, on the bans Task 4 lifts. `cargo deny` is not part of
   `cargo make dod` (`Makefile.toml:102-110`), so this commit is gate-clean.
6. Commit: `build(buildl-lua): depend on globset and walkdir, and tempfile for tests`.

### Task 4 — Let `buildl-lua` depend on the three banned crates

**Files:**
- Modify `deny.toml`

**Steps:**

1. Run the supply-chain check and confirm the bans fail:

   ```bash
   cargo deny check bans
   ```

   ```text
   error[banned]: crate 'globset = 0.4.20' is explicitly banned
   error[banned]: crate 'tempfile = 3.27.0' is explicitly banned
   error[banned]: crate 'walkdir = 2.5.0' is explicitly banned
   bans FAILED
   ```

2. In `deny.toml`, replace

   ```toml
       { crate = "walkdir", wrappers = ["airsl"], reason = """\
           ABB §1.2: buildl-core does no filesystem traversal""" },
       { crate = "tempfile", wrappers = ["airsl"], reason = """\
           ABB §1.2: temp-file-then-rename is a storage-adapter concern""" },
       { crate = "globset", wrappers = ["airsl"], reason = """\
           ABB §1.2: b.sources() globbing is a filesystem read, not core logic""" },
   ```

   with

   ```toml
       { crate = "walkdir", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: buildl-core does no filesystem traversal; buildl-lua walks the declaring \
           directory for b.sources()""" },
       { crate = "tempfile", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: temp-file-then-rename is a storage-adapter concern; buildl-lua uses it only \
           to build symlink fixtures in its tests""" },
       { crate = "globset", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: b.sources() globbing is a filesystem read, not core logic; buildl-lua \
           matches it host-side""" },
   ```

3. Run the full check:

   ```bash
   cargo deny check
   ```

   ```text
   advisories ok, bans ok, licenses ok, sources ok
   ```

4. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
5. Commit: `build(repo): let buildl-lua depend on globset, walkdir and tempfile`.

---

## Verification summary (plan-level)

- `cargo make dod` exits `0`, including `guard-crate-edges` against the updated golden file.
- `cargo deny check` prints `advisories ok, bans ok, licenses ok, sources ok`.
- `cargo tree -p buildl-lua --depth 1 --edges normal,dev --prefix none --format '{lib}'` lists `airsl`, `buildl_core`, `globset`, `walkdir`, `tempfile`.

## Review findings

- strict-quality — the `0.1.4` floor has no reversion guard; the lock already pins 0.1.4 and there is no minimal-versions build — `Cargo.toml:24`
- strict-quality — `redundant_pub_crate` is a workspace-wide `allow`, where airsl uses a per-file `expect`; spec §10 authorises the workspace form — `Cargo.toml:78-81`
- consistency — the deny reasons spell the API `b.sources()` while the manifest comments spell it `buildl.sources()` — `deny.toml:93-101`

Blocking set: none. The reviewer re-ran `cargo make dod` (exit 0) and `cargo deny check -D unused-wrapper` (`advisories ok, bans ok, licenses ok, sources ok`).

## Probe results

- Claim: `Cargo.lock` already resolves airsl 0.1.4 and holds globset, walkdir and tempfile. Command: `grep -n -A1 'name = "airsl"' Cargo.lock` gave `version = "0.1.4"`; globset is at `Cargo.lock:247`, tempfile at `:702`, walkdir at `:793`. Holds.
- T1: `cargo tree -p buildl-lua --depth 1 --prefix none | grep airsl` gave `airsl v0.1.4`. Holds.
- T3 red: `cargo make guard-crate-edges` failed before the golden-file edit; cargo-make exited 105 and the script exited 1, with diff `+globset +walkdir +tempfile`. After the edit it exits 0. Holds.
- T4 red: `cargo deny check bans` gave three `explicitly banned` errors (globset 0.4.20, tempfile 3.27.0, walkdir 2.5.0) and `bans FAILED`. After the edit, `cargo deny check` gave `advisories ok, bans ok, licenses ok, sources ok`. Holds.

## Deviations

- 2026-10-08 — One coder ran Tasks 1–4 in order instead of one coder per task. The tasks are sequential manifest edits over shared files, so batching them cannot parallelise anything. `cargo make dod` ran once at the end, not after each task. Commits were left to the user and not made per task.
