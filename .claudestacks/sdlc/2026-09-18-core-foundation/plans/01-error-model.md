---
status: done
created: 2026-10-04
---

# Error Model Implementation Plan

**Goal:** `buildl-core` reports every failure through one structured enum.

**Architecture:** One `thiserror` enum in `crates/buildl-core/src/error.rs`, `#[non_exhaustive]`, with
a crate-wide `Result<T>` alias beside it. The variant this plan writes carries a `NameKind` field
rather than a classifier string, so a caller branches on a field and never on message text. This
plan also wires the crate's four permitted dependencies and the crate-edge golden file in one move,
because three later plans run in parallel and none of them can edit those two shared files.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `thiserror` 2.0, `cargo-make`, `cargo-deny`.

---

## Context an implementer needs

`crates/buildl-core/src/lib.rs` is a 21-line doc comment with no items; the crate compiles because
there is nothing in it. `crates/buildl-core/Cargo.toml` has an empty `[dependencies]` table and a
comment naming the only four crates it may ever take: `serde`, `serde_json`, `thiserror`, `sha2`.
All four are already catalogued with their reasons in the workspace root `Cargo.toml:25-37` and are
already in `Cargo.lock`, so no network access is needed.

Two guards will react to this plan, both wired into `cargo make clippy` and therefore into
`cargo make dod` (`Makefile.toml:52-73`):

- `guard-crate-edges` (`Makefile.toml:121-145`) dumps every member's direct dependencies with
  `cargo tree --depth 1 --edges normal,dev --prefix none --format '{lib}'` and diffs the result
  against `crates/expected-edges.txt`. That file currently records **zero** direct dependencies for
  `buildl-core` — lines 1 and 2 are `# buildl-core` and `# buildl-lua` back to back. Adding a
  dependency without updating the golden file fails the gate.
- `guard-core-purity` (`Makefile.toml:146-216`) asserts `crates/buildl-core/clippy.toml`'s bans
  resolve and are not suppressed. This plan adds no banned API, so it stays green.

`cargo-deny`'s ban list (`deny.toml`, `[bans].deny`) names `airsl`, `buildl-core`, `buildl-lua`,
`buildl`, `mlua`, `toml`, `walkdir`, `tempfile` and `globset`. None of the four crates added here
appears in it, and all four are already in the graph, so `multiple-versions = "deny"` is unaffected.

The workspace enables these `rust` lints and no others: `unsafe_code = "forbid"`,
`missing_docs = "warn"`, `unreachable_pub = "warn"`, `unused_must_use = "deny"`
(`Cargo.toml:62-66`). `unused_crate_dependencies` is **not** among them, so the three dependencies
this plan adds without yet using produce no warning. Clippy runs `pedantic` and `nursery` at warn
with `-D warnings`, so a pedantic lint is a gate failure.

Run `cargo fmt` before every `cargo make dod` in this plan. `fmt-check` is the gate's first step
(`Makefile.toml:46-50`), and every Rust block below is given in rustfmt's canonical form — but
retyping or re-wrapping one can drift, and a drifted block turns the gate red before a single test
runs.

All work happens in the worktree, on a branch, never on `main`. Commits follow Conventional
Commits: `type(scope): summary`, scope `buildl-core`. One commit per task.

### File map

```
crates/buildl-core/Cargo.toml   — [modify] take the four permitted dependencies (task 1)
crates/expected-edges.txt       — [modify] record buildl-core's new direct edges (task 1)
Cargo.lock                      — [modify] cargo writes buildl-core's dependency list (task 1)
crates/buildl-core/src/error.rs — [create] Error, NameKind, Result, and their tests (task 2)
crates/buildl-core/src/lib.rs   — [modify] declare and re-export the error module (task 2)
```

---

## Task 1 — Take the four permitted dependencies

There is no behavioural change here, so the red step is the guard rejecting an undeclared edge
rather than a failing test. That is the cycle this task runs.

**Files:**
- Modify `crates/buildl-core/Cargo.toml`
- Modify `crates/expected-edges.txt`
- Modify `Cargo.lock` (written by cargo, committed)

**Steps:**

1. Replace the empty `[dependencies]` table in `crates/buildl-core/Cargo.toml`. Keep the comment
   block above it exactly as it is:

   ```toml
   [dependencies]
   serde      = { workspace = true }
   serde_json = { workspace = true }
   sha2       = { workspace = true }
   thiserror  = { workspace = true }
   ```

   All four are added now rather than one plan at a time: plans `02`, `03` and `04` are independent
   of each other and meant to run concurrently, and each would otherwise have to edit this file and
   the golden file below.

2. Run the guard and confirm it fails, naming exactly the four edges:

   ```
   $ cargo make guard-crate-edges
   [cargo-make] INFO - Running Task: guard-crate-edges
   --- crates/expected-edges.txt
   +++ /var/folders/…/tmp.vG1jJuzelM
   @@ -1,4 +1,8 @@
    # buildl-core
   +serde
   +serde_json
   +sha2
   +thiserror
    # buildl-lua
    airsl
    buildl_core
   guard: crate edges do not match crates/expected-edges.txt
          fix the manifest, or update the golden file if the change is intended
   Error while executing command, exit code: 1
   ```

3. Update `crates/expected-edges.txt` to the live graph. The whole file afterwards:

   ```
   # buildl-core
   serde
   serde_json
   sha2
   thiserror
   # buildl-lua
   airsl
   buildl_core
   # buildl
   buildl_core
   buildl_lua
   # buildl-cli
   buildl
   ```

   The names are cargo's lib names, so `buildl_core` keeps its underscore while the four new ones
   have none. They are in the order `cargo tree` emits, which is alphabetical.

4. Confirm the guard is green:

   ```
   $ cargo make guard-crate-edges
   [cargo-make] INFO - Running Task: guard-crate-edges
   [cargo-make] INFO - Build Done in …
   ```

5. Confirm the gate and the supply chain are untouched by the new edges:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ cargo make deny
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. `git diff --stat` should show exactly three files — `crates/buildl-core/Cargo.toml`,
   `crates/expected-edges.txt` and `Cargo.lock`. Cargo adds a `dependencies = [...]` block to
   `Cargo.lock`'s `buildl-core` package entry; commit it.

7. Commit `build(buildl-core): take the four permitted pure-computation dependencies`.

---

## Task 2 — Write the one error enum

**Files:**
- Create `crates/buildl-core/src/error.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test first. Create `crates/buildl-core/src/error.rs` holding **only** the test
   module:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       use super::{Error, NameKind};

       #[test]
       fn name_kind_renders_a_human_phrase() {
           assert_eq!(NameKind::Directory.to_string(), "directory");
           assert_eq!(NameKind::TargetName.to_string(), "target name");
           assert_eq!(NameKind::Label.to_string(), "label");
           assert_eq!(NameKind::Digest.to_string(), "digest");
       }

       #[test]
       fn invalid_name_states_what_was_found() {
           let err = Error::InvalidName {
               kind: NameKind::Label,
               value: "//lib".to_owned(),
               reason: "must hold ':' and a target name",
           };
           assert_eq!(
               err.to_string(),
               r#"invalid label: "//lib" — must hold ':' and a target name"#
           );
       }
   }
   ```

   and add `mod error;` to `crates/buildl-core/src/lib.rs` so the file is compiled.

2. Run it and confirm it fails to resolve the names:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `super::Error`, `super::NameKind`
    --> crates/buildl-core/src/error.rs:5:17
   ```

3. Replace the whole of `crates/buildl-core/src/error.rs` with the implementation, keeping the test
   module from step 1 at the bottom:

   ```rust
   //! The crate's one error type and the `Result` alias every fallible call returns.
   //!
   //! A single file rather than one error type per module: a caller branches on a field, never on
   //! the text of a message, and every module in this crate returns the same `Result` without a
   //! conversion at the boundary.
   //!
   //! Responsibilities:
   //!
   //! - [`Error`] — every way a call into this crate can fail.
   //! - [`NameKind`] — which domain name an [`Error::InvalidName`] is about.
   //! - [`Result`] — the crate-wide alias.
   //!
   //! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic line; how a
   //! command surfaces it belongs to the reporting adapter.

   use core::fmt;

   /// The result of any fallible call in this crate.
   pub type Result<T> = core::result::Result<T, Error>;

   /// Every way a call into this crate can fail.
   ///
   /// Non-exhaustive: later work adds the variants its phases earn without that being a breaking
   /// change for downstream crates.
   #[derive(Debug, thiserror::Error)]
   #[non_exhaustive]
   pub enum Error {
       /// A domain name did not satisfy its grammar.
       #[error("invalid {kind}: {value:?} — {reason}")]
       InvalidName {
           /// Which kind of name was being built.
           kind: NameKind,
           /// The rejected input, as given.
           value: String,
           /// Why the grammar rejected it.
           reason: &'static str,
       },
   }

   /// Which domain name an [`Error::InvalidName`] is about.
   ///
   /// A type rather than a string so classification is read as a field. Non-exhaustive for the same
   /// reason [`Error`] is.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum NameKind {
       /// A workspace-relative directory path.
       Directory,
       /// The name half of a label.
       TargetName,
       /// A whole label.
       Label,
       /// A content digest.
       Digest,
   }

   impl fmt::Display for NameKind {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(match self {
               Self::Directory => "directory",
               Self::TargetName => "target name",
               Self::Label => "label",
               Self::Digest => "digest",
           })
       }
   }
   ```

   `Digest` and the two name variants are declared now even though the types that raise them arrive
   in plans `02` and `03`: `NameKind` is the classifier for a grammar failure, and splitting it
   across plans would mean three plans editing this file.

4. Run the tests and confirm green:

   ```
   $ cargo test -p buildl-core
   running 2 tests
   ..
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Make the module public and re-export its items from `crates/buildl-core/src/lib.rs`. Change the
   `mod error;` added in step 1 to the following, placed after the existing crate doc comment:

   ```rust
   pub mod error;

   pub use error::{Error, NameKind, Result};
   ```

   `lib.rs` stays export-only — module docs, `mod`, `pub use` and nothing else — so the final line
   of its doc comment ("This file holds only module declarations and re-exports, so it carries no
   logic to unit-test") remains true.

6. Run the full gate:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

   `error.rs` as written trips neither of the two pedantic lints that bite the later plans — it
   names no crate in a doc comment and has no field-reading method — so a green gate here is the
   expected result rather than a lucky one.

7. Commit `feat(buildl-core): report every failure through one structured enum`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
```

All three exit 0. The third is the MSRV floor the `msrv` CI job runs; it is CI-only by design, but
running it locally here costs nothing and the code in this plan is known to compile on 1.94.

At the end of this plan `buildl-core` exports `Error`, `NameKind` and `Result`, holds four
dependencies, and has one `InvalidName` variant with no type yet raising it.

Plans `02` and `04` depend on this plan and on nothing else. They are independent in content and
neither touches `crates/buildl-core/Cargo.toml` or `crates/expected-edges.txt`, which is what this
plan's task 1 front-loads — but they are **not** file-disjoint: both append a line to
`crates/buildl-core/src/lib.rs` (`pub mod types;` and `pub mod json;` respectively), as do plans
`03` and `05`. Run them in either order, or concurrently only if you reconcile that one file by
hand. Plan `03` additionally depends on `02`, because its `Provenance` holds a `Directory`.

---

## Review findings

- doc-comment accuracy (risk) — the manifest comment still read "added as the code that needs them lands" while the table beside it declared all four edges; a later reader could have "fixed" it by deleting three. Restated as front-loaded, naming the guard as the reason — `crates/buildl-core/Cargo.toml:12-16`. Fixed and verified: `cargo make dod` → `[cargo-make] INFO - Build Done in 5.46 seconds.`; `cargo make deny` → `advisories ok, bans ok, licenses ok, sources ok`.
- doc-comment-discipline (nit) — shipped README still reads "**Status:** pre-release; the crate has no public API.", false once `pub use error::{Error, NameKind, Result}` lands — `crates/buildl-core/README.md:7`. Not fixed here: plan `06` task 2 owns the README rewrite, so this is signed-off carry-over rather than drift.
- doc-comment-discipline (nit) — "belongs to the reporting adapter" named a component that does not exist in the tree; the architecture's name is the `Reporter` port, which `crates/buildl-core/clippy.toml` already uses. Reworded — `crates/buildl-core/src/error.rs:14`. Fixed and verified by the same `cargo make dod` run above.
- doc-comment-discipline, leakage rule (nit) — "later work adds the variants its phases earn" was forward-work narration; the contract-bearing half stands alone — `crates/buildl-core/src/error.rs:23`. Fixed and verified by the same `cargo make dod` run above.
- reversion guard (nit) — nothing in the gate catches removal of `#[non_exhaustive]` from either enum: in-crate matching is unaffected and no downstream crate matches exhaustively yet — `crates/buildl-core/src/error.rs:26` and `:45`. No action taken; the crate is unpublished and the cheap guard is a compile-fail fixture, which has no home until a downstream consumer exists.

## Probe results

- **Claim: `guard-crate-edges` rejects the four undeclared edges, with the hunk the plan quotes.** `cargo make guard-crate-edges` after the manifest edit. Real output — matched the plan exactly, hunk header included:
  ```
  --- crates/expected-edges.txt	2026-09-21 08:13:43
  +++ /var/folders/8w/4ccj7cw90nx4d32s72tqf5nw0000gn/T/tmp.rKdOnftT95	2026-10-04 09:24:23
  @@ -1,4 +1,8 @@
   # buildl-core
  +serde
  +serde_json
  +sha2
  +thiserror
   # buildl-lua
   airsl
   buildl_core
  guard: crate edges do not match crates/expected-edges.txt
         fix the manifest, or update the golden file if the change is intended
  Error while executing command, exit code: 1
  ```
- **Claim: the golden-file rewrite turns the guard green.** `cargo make guard-crate-edges` → `[cargo-make] INFO - Build Done in 0.70 seconds.`
- **Claim: four declared-but-unused dependencies do not trip the gate.** This rests on `unused_crate_dependencies` not being enabled in the workspace lint table. `cargo make dod` → `[cargo-make] INFO - Build Done in 6.82 seconds.` with zero warnings. Confirmed.
- **Claim: `cargo deny` accepts the four new edges.** `cargo make deny` → `advisories ok, bans ok, licenses ok, sources ok` — the plan's predicted line verbatim.
- **Claim: the step-1 placeholder fails with `error[E0432]` at `crates/buildl-core/src/error.rs:5:17`.** Run as part of the task's red step; the failure matched the cited error and location exactly.
- **Structural check: `lib.rs`'s doc block ends with "This file holds only module declarations and re-exports, so it carries no logic to unit-test."** Read at `crates/buildl-core/src/lib.rs:20-21` before the task began. Present, and still true after the task.
- **Claim the plan does *not* make, checked because the spec does:** spec §6 names three `Error` variants, and this plan ships only `InvalidName`. Verified against `plans/04-canonical-json.md:319-332`, which declares `CanonicalJson` and `FloatRejected`. The staging is deliberate and `#[non_exhaustive]` is what makes it non-breaking — not a dropped requirement.

## Deviations

- **2026-10-04 — no commits.** Each task's final step names a commit. None was run: the commit gate belongs to the author, and no agent in this flow runs a commit. Messages held for the author: `build(buildl-core): take the four permitted pure-computation dependencies` (task 1) and `feat(buildl-core): report every failure through one structured enum` (task 2).
- **2026-10-04 — task 1 ran inline rather than through a coder subagent.** It is a four-line manifest edit plus a golden-file rewrite, with no red-green code cycle of its own; its red step is the guard, which the orchestrator ran directly. Delegating it would have cost a spawn and bought nothing.
- **2026-10-04 — the three applied review findings were fixed inline rather than routed to a fresh coder spawn.** All three were single-line doc or comment edits. The gate was re-run over the result rather than the reviewer being re-spawned, per the one-fix-round budget.
- **2026-10-04 — `crates/buildl-core/src/error.rs` was registered intent-to-add** so the reviewer's diff against HEAD would include the new file. Nothing was committed.
