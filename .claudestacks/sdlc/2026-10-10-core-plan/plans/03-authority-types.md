---
status: approved
created: 2026-10-11
depends-on: [01, 02]
---

# Authority Types Implementation Plan

**Goal:** What a workspace allows and what a build wants are values the pipeline can hold.

**Architecture:** Two new files under `types/`, type definitions only. `types/authority.rs` holds `Ceiling` — the four lists of a manifest's ceiling: programs, environment names, directories read, directories written — `WantedSet`, what a plan reports a build wants, and `Want`, the one want a refusal is about. `types/workspace.rs` holds `Workspace`: the two directories a build writes into and the ceiling. Every field is public, because none of these values has an invariant beyond its fields' own. No `Grants` type is introduced: with no approver the grant is the wanted set, once it is inside the ceiling. Deciding whether a want is inside a ceiling needs the graph as well, so it is not here.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D3, D17, D18), §3.5 (authority and the workspace).

**Checkpoints:** review once, when the task is done.

---

## Context an implementer needs

What plans `01` and `02` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| The `CacheRow` bullet in the module doc, and the eight names plan `02` added to the re-export list | `crates/buildl-core/src/types/mod.rs`, `crates/buildl-core/src/lib.rs` | the anchor task 1 inserts below, and part of the list it quotes |
| `Program` (`Ord`) | `crates/buildl-core/src/types/program.rs` | the entries of `Ceiling::run` and `WantedSet::programs` |
| `EnvName`, `Directory`, `Label`, `SourcePath` (all `Ord`, all serialize as their text) | `crates/buildl-core/src/types/` | the entries of the other sets, and the payloads of `Want` |

Gate facts that bite here:

- Neither file holds logic, so neither has a test module and the task has no failing-test step:
  each carries the note the crate's other definition-only files carry, and the gate's build and
  rustdoc steps are what check them.
- `WantedSet` derives `Serialize` because a plan writes it out as it is; its four field names are
  the keys of the serialized object. `Ceiling`, `Want` and `Workspace` are not serialized.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/authority.rs   — [create] Ceiling, WantedSet, Want
crates/buildl-core/src/types/workspace.rs   — [create] Workspace
crates/buildl-core/src/types/mod.rs         — [modify] declare and re-export the two modules
crates/buildl-core/src/lib.rs               — [modify] re-export the four types
```

### Task 1 — Add the ceiling, the wanted set and the workspace

**Files:**
- Create `crates/buildl-core/src/types/authority.rs`
- Create `crates/buildl-core/src/types/workspace.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/authority.rs`:

   ```rust
   //! Authority as values: what a workspace allows, and what a build wants.
   //!
   //! Its own file because nothing in a build is ambient: what may run, which variables may be
   //! read and which directories may be touched are written down once for a workspace, what a
   //! build needs is derived from its targets, and the two are compared before anything runs.
   //!
   //! Responsibilities: [`Ceiling`], [`WantedSet`] and [`Want`].
   //!
   //! Non-responsibilities: the comparison. Deciding whether a want is inside a ceiling needs both
   //! values and the graph, so it belongs to the phase that plans; nothing here grants or refuses.
   //!
   //! This file holds only type definitions, so it carries no logic to unit-test.

   use std::collections::BTreeSet;

   use serde::Serialize;

   use crate::types::{Directory, EnvName, Label, Program, SourcePath};

   /// The most a workspace allows any build to do.
   ///
   /// Four lists, each empty unless the workspace says otherwise: an empty ceiling allows nothing.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct Ceiling {
       /// The programs a command may run.
       pub run: BTreeSet<Program>,
       /// The environment variables a target may read.
       pub env: BTreeSet<EnvName>,
       /// The directories a target's inputs may lie in.
       pub read: BTreeSet<Directory>,
       /// The directories a build may write into.
       pub write: BTreeSet<Directory>,
   }

   /// What one build wants, derived from its targets and its workspace.
   ///
   /// The files a build reads are not copied here: they are each target's declared inputs, and stay
   /// in the graph.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct WantedSet {
       /// Every program a command names.
       pub programs: BTreeSet<Program>,
       /// Every environment variable a target reads.
       pub env: BTreeSet<EnvName>,
       /// Every directory the build writes into.
       pub write: BTreeSet<Directory>,
       /// Every target that declared it reaches the network.
       pub network: BTreeSet<Label>,
   }

   /// One thing a build wants that a ceiling is asked about.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Want {
       /// To run a program.
       Program(Program),
       /// To read an environment variable.
       Env(EnvName),
       /// To read a workspace file as an input.
       Read(SourcePath),
       /// To write into a directory.
       Write(Directory),
   }
   ```

2. Create `crates/buildl-core/src/types/workspace.rs`:

   ```rust
   //! What a workspace's manifest tells the pipeline.
   //!
   //! Its own file because these are the facts about a workspace that no build file states: where
   //! a build puts what it produces, where buildl keeps its own files, and what the workspace
   //! allows.
   //!
   //! Responsibilities: [`Workspace`].
   //!
   //! Non-responsibilities: reading a manifest, which is a port's.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   use crate::types::{Ceiling, Directory};

   /// A workspace as its manifest describes it.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct Workspace {
       /// The directory named outputs are linked into.
       pub out: Directory,
       /// The directory buildl keeps its own files in: its caches, its content store, its log.
       pub state: Directory,
       /// What the workspace allows a build to do.
       pub ceiling: Ceiling,
   }
   ```

3. Declare both modules. In `crates/buildl-core/src/types/mod.rs`, add `pub mod authority;` and `pub mod workspace;` to the `pub mod` lines, add `pub use authority::{Ceiling, Want, WantedSet};` and `pub use workspace::Workspace;` to the `pub use` lines, and add two bullets to the module doc, directly below the `CacheRow` bullet:

   ```rust
   //! - [`Ceiling`], [`WantedSet`] and [`Want`] — what a workspace allows, and what a build wants.
   //! - [`Workspace`] — what a workspace's manifest tells the pipeline.
   ```

4. Re-export them. In `crates/buildl-core/src/lib.rs`, add `Ceiling`, `Want`, `WantedSet` and `Workspace` to the `pub use types::{…}` list.

5. Run `cargo fmt --all`, which puts the additions in sorted order. The list in `crates/buildl-core/src/lib.rs` then reads:

   ```rust
   pub use types::{
       Action, ActionKey, Alias, Argument, BuildFile, CacheRow, Ceiling, Command, Declaration,
       DeclarationOrder, Declared, DepProducts, Description, Diagnostic, Digest, Directory, EntryName,
       EnvName, EnvValue, Evaluated, FieldName, FileStat, Freshness, InputDigest, KeyComponents,
       Label, NetworkAccess, Node, NodeId, OutputName, OwnComponents, Platform, Produced, Program,
       Provenance, Rule, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, Target, TargetGraph, TargetName, TargetRole, Timestamp, ToolIdentity,
       Want, WantedSet, Workspace, Written,
   };
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/types/authority.rs crates/buildl-core/src/types/workspace.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the ceiling, the wanted set and the workspace"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `Ceiling`, `WantedSet`, `Want` and `Workspace` are public, with public fields, and re-exported
  from the crate root.
- `types/authority.rs` and `types/workspace.rs` hold no function and no `impl` block.
- No type named `Grants` exists.
