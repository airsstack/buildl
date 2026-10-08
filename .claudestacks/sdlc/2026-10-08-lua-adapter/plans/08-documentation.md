---
status: approved
created: 2026-10-08
depends-on: [07]
---

# Documentation Implementation Plan

**Goal:** Every document that describes `buildl-lua`, the declaration sandbox or `b.sources()` matches the shipped adapter.

**Architecture:** Prose only, no behaviour. The crate's rustdoc and README describe what it now holds (roadmap follow-up #3 for `buildl-lua`). The design document's §2, §5 and §12.1 state that the declaration engine holds no grant, that `b.sources()` reads host-side, and what buildl strips and what it does not. The root README says the same. The building-blocks document moves the globbing question from open (§11) to decided (§10). `architecture.md` names the staging buffer it now is. The roadmap closes follow-ups #4 and #12 and #3's `buildl-lua` part, and records three new ones.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §12 (documentation deliverables and amendments), §5.3 (the §12.1 amendment), D2, D12.

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

Section cross-references between the documents are load-bearing (CLAUDE.md). None of these edits
renumbers a section. The design documents use Mermaid; none of these edits touches a diagram. The
root README is a condensed summary of the design and must stay consistent with it.

## File structure

```text
crates/buildl-lua/src/lib.rs           — [modify] crate rustdoc                                  (Task 1)
crates/buildl-lua/README.md            — [modify] what the crate holds                           (Task 1)
docs/design.md                         — [modify] §2 (:56), §5 (:148), §12.1 (:583)              (Task 2)
README.md                              — [modify] the declaration-safety sentence (:42)         (Task 2)
docs/architecture-building-blocks.md   — [modify] §10 decision row; §11 open question removed    (Task 3)
docs/architecture.md                   — [modify] the staging-buffer sentence (:180)            (Task 3)
docs/roadmap.md                        — [modify] §5 follow-ups                                   (Task 4)
```

### Task 1 — The crate's own documentation

**Files:**
- Modify `crates/buildl-lua/src/lib.rs`
- Modify `crates/buildl-lua/README.md`

**Steps:**

1. Replace the whole of `crates/buildl-lua/src/lib.rs` with this text. The `mod` and `pub use` lines
   are unchanged; the crate docs are rewritten and now end with the export-only sentence, which is
   true now that the file holds only declarations and re-exports:

   ```rust
   //! Lua build-file evaluation for buildl, on the airsl embedded runtime.
   //!
   //! This crate is the only part of buildl that depends on a Lua runtime, so the effect of an
   //! airsl upgrade stays within one crate. It implements `buildl-core`'s [`DeclarationSource`]
   //! port as [`LuaSource`].
   //!
   //! [`DeclarationSource`]: buildl_core::DeclarationSource
   //!
   //! # Responsibilities
   //!
   //! - Evaluating one build file per call on a fresh airsl engine, and returning what it staged
   //!   exactly as written, or the one failure that stopped it.
   //! - Installing the `buildl` module table, reachable from Lua both as `airsstack.buildl` and as
   //!   the global `buildl`: `target`, `test`, `rule`, `alias`, `option`, `subdir` and `sources`.
   //! - Keeping the declaration phase sandboxed: airsl's minimal language surface, no grant of any
   //!   kind, only the `json`, `path`, `regex`, `hash` and `glob` modules, and no `math.random`,
   //!   `math.randomseed`, `print` or `airsstack.path.absolute`. `buildl.sources` is the only
   //!   filesystem read, and it walks only the declaring directory.
   //! - Bounding every resource a build file can consume: the memory and instruction ceilings of
   //!   [`DeclarationLimits`], and a byte budget, equal to the memory ceiling, on what one file may
   //!   stage.
   //!
   //! # Non-responsibilities
   //!
   //! - Deciding which build files to evaluate, validating names, resolving labels or joining
   //!   paths. The caller does all of that with what a file staged.
   //! - Resolving, planning, scheduling, caching, or executing anything a build file declares.
   //!
   //! # Where things live
   //!
   //! | Module | Holds |
   //! |---|---|
   //! | `source` | [`LuaSource`]: read, evaluate, report |
   //! | `limits` | [`DeclarationLimits`] |
   //! | `engine` | the airsl engine configuration |
   //! | `curated` | airsl's `path` module without `absolute` |
   //! | `module` | the `buildl` host module and its sticky refusals |
   //! | `primitives` | each primitive's argument shape |
   //! | `values` | the Lua value readers every primitive shares |
   //! | `refusal` | why a primitive call was refused |
   //! | `sources` | the host-side walk behind `buildl.sources` |
   //! | `staging` | the staging buffer: order, byte budget, first refusal |
   //! | `classify` | airsl failures named as `EvaluationFailure` kinds |
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod classify;
   mod curated;
   mod engine;
   mod limits;
   mod module;
   mod primitives;
   mod refusal;
   mod source;
   mod sources;
   mod staging;
   mod values;

   pub use limits::DeclarationLimits;
   pub use source::LuaSource;
   ```

2. Replace the whole of `crates/buildl-lua/README.md` with:

   ```markdown
   # buildl-lua

   Lua build-file evaluation for buildl, on the [airsl](https://github.com/airsstack/airsl) embedded runtime.

   This is the only buildl crate that depends on a Lua runtime. It implements [`buildl-core`](../buildl-core)'s `DeclarationSource` port as `LuaSource`, which evaluates one build file per call on a fresh airsl engine and returns what the file staged, exactly as written.

   ## What it holds today

   | | |
   |---|---|
   | Adapter | `LuaSource::new(root, limits)`: a missing build file is reported absent; every other failure is one `Error::Evaluation` naming the file, its kind and a diagnostic with the line |
   | Limits | `DeclarationLimits`: the memory and instruction ceilings (16 MiB and 10 000 000 by default); the memory ceiling is also the byte budget for what one file may stage |
   | Declaration API | the `buildl` table, bound as both `buildl` and `airsstack.buildl`: `target`, `test`, `rule`, `alias`, `option`, `subdir`, `sources`. Option tables are closed, values are strictly typed, and list fields splice one level of nested lists |
   | Sandbox | airsl's minimal Lua surface with no grants; only `json`, `path` (without `absolute`), `regex`, `hash` and `glob`; no `math.random`, `math.randomseed` or `print`. `buildl.sources` is the only filesystem read: regular files under the declaring directory, sorted, symlinks never followed |
   | Refusals | a refused primitive call fails the whole file, even when the build file catches it with `pcall` |

   **Status:** pre-release. The adapter is complete for the declaration phase; nothing in buildl composes it into a command yet.

   ## License

   Apache-2.0
   ```

3. Confirm the rustdoc builds with no broken intra-doc link:

   ```bash
   RUSTDOCFLAGS="-D warnings" cargo doc -p buildl-lua --no-deps
   ```

   It exits `0`.
4. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
5. Commit: `docs(buildl-lua): describe the adapter, its sandbox and its limits`.

### Task 2 — The design document and the root README

**Files:**
- Modify `docs/design.md`
- Modify `README.md`

**Steps:**

1. In `docs/design.md` §2, replace the line

   ```markdown
   - **Declaration is safe on untrusted input.** The declaration engine holds one grant (filesystem read on the workspace, for source globbing) and nothing else.
   ```

   with

   ```markdown
   - **Declaration is safe on untrusted input.** The declaration engine holds no grant at all. Its one filesystem read, source globbing, is done host-side by `b.sources()` (§5) and confined to the declaring directory.
   ```

2. In `docs/design.md` §5, replace the line

   ```markdown
   - **`b.sources()` is the declaration phase's only filesystem read**, backed by the workspace-read grant. Results come back sorted (an airsl guarantee), so declaration is deterministic regardless of filesystem order.
   ```

   with

   ```markdown
   - **`b.sources()` is the declaration phase's only filesystem read**, done host-side rather than through an engine grant. It returns the regular files under the declaring directory that match the pattern, relative to that directory and sorted by the host, so declaration is deterministic regardless of filesystem order. Symlinks are never followed, and a directory that resolves outside the workspace is refused. Every list field accepts one level of nesting, spliced in order, so `inputs = { b.sources("src/*.c"), "go.mod" }` is one flat list.
   ```

3. In `docs/design.md` §12.1, replace item 1

   ```markdown
   1. **Drop `os` and curate the module set.** `os.time`, `os.clock`, and Lua 5.4's entropy-seeded `math.random` are nondeterministic; the declaration policy uses a custom language surface without `os`, and a `ModuleSet` installing `json`, `path`, `regex`, `hash`, `glob` while omitting `time`, `proc`, `env`, `stdio`. airsl lets the host choose both — configuration, not new machinery [1].
   ```

   with

   ```markdown
   1. **Drop `os`, curate the module set, and strip what remains.** `os.time`, `os.clock`, and Lua 5.4's entropy-seeded `math.random` are nondeterministic. The declaration policy uses airsl's minimal language surface, which has no `os`, and a `ModuleSet` installing `json`, `path`, `regex`, `hash`, `glob` while omitting `time`, `proc`, `env`, `stdio`. airsl lets the host choose both — configuration, not new machinery [1]. The minimal surface still loads `math` whole, so buildl removes `math.random` and `math.randomseed` itself. It also removes `airsstack.path.absolute`, which reads the process working directory, and `print`, because stdout belongs to the CLI. One source stays open: `tostring` or `string.format` of a table or function renders its address, which can differ between runs, and the double evaluation below catches it only when the two addresses differ.
   ```

4. In `README.md`, replace the sentence fragment

   ```markdown
   Declaration is safe on untrusted input (its only grant is filesystem read on the workspace, for source globbing);
   ```

   with

   ```markdown
   Declaration is safe on untrusted input (its engine holds no grant at all, and `b.sources()` reads the declaring directory host-side);
   ```

5. Confirm nothing else in either file still claims a declaration-phase grant:

   ```bash
   grep -n "workspace-read grant\|only grant is filesystem\|holds one grant\|an airsl guarantee" docs/design.md README.md
   ```

   It prints nothing.
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `docs(repo): the declaration engine holds no grant; b.sources reads host-side`.

### Task 3 — The architecture documents

**Files:**
- Modify `docs/architecture-building-blocks.md`
- Modify `docs/architecture.md`

**Steps:**

1. In `docs/architecture-building-blocks.md` §10, add this row directly after the `Load granularity`
   row (the row reading `|Load granularity|**`DeclarationSource` evaluates one file**…`):

   ```markdown
   |Source globbing|**Host-side in `buildl-lua`**|`b.sources()` walks with `globset` and `walkdir`, so the declaration engine holds no grant and `airsstack.glob.walk` is refused; sorting, symlink policy and the containment check are buildl's, in one place.|
   ```

2. In the same file's §11, delete the bullet

   ```markdown
   - Where `b.sources()` globbing runs: through `airsstack.glob.walk` inside `buildl-lua`, or host-side behind a port — the workspace `Cargo.toml` currently catalogs `globset` and `walkdir` for it.
   ```

3. In `docs/architecture.md` (the "mechanics of collection" paragraph), replace

   ```markdown
   the `buildl` `HostModule`'s closures capture an `Arc<Mutex<StagedFile>>` staging buffer for the file being evaluated.
   ```

   with

   ```markdown
   the `buildl` `HostModule`'s closures capture an `Arc<Mutex<Staging>>` for the file being evaluated: the staged records, the file's byte budget (its memory ceiling), and the first refusal, which stands even if the build file catches it with `pcall`.
   ```

4. Confirm both edits landed and nothing else names the old buffer:

   ```bash
   grep -rn "Arc<Mutex<StagedFile>>\|Where \`b.sources()\` globbing runs" docs/
   ```

   It prints nothing.
5. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
6. Commit: `docs(repo): record host-side globbing and the staging buffer`.

### Task 4 — The roadmap's follow-ups

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. In `docs/roadmap.md` §5, delete the rows numbered `4` (the airsl floor) and `12` (the per-file
   staging cap). This slice closes both.
2. In row `3`, replace the opening

   ```markdown
   |3|Refresh the crate-level docs once real modules exist. `buildl-core` is done (§4). Still open: `buildl-lua`, `buildl` and `buildl-cli` —
   ```

   with

   ```markdown
   |3|Refresh the crate-level docs once real modules exist. `buildl-core` and `buildl-lua` are done (§4). Still open: `buildl` and `buildl-cli` —
   ```

   and replace its last two cells

   ```markdown
   |I1 review|each crate's first code slice (I-lua, adapters)|
   ```

   with

   ```markdown
   |I1 review|each crate's first code slice (adapters)|
   ```

3. Append these three rows after row `11`, at the end of the table:

   ```markdown
   |13|Exclude the workspace's `out` and `.buildl` directories from `b.sources()`. It walks the declaring directory whole, so at the workspace root it also lists build outputs; which directories to skip is known only once `buildl.toml` is parsed.|I-lua spec §6.2|I4 Resolve (the `Manifest` port)|
   |14|Reword `EvaluationLimit::Staging`'s rustdoc ("the cap on how many declarations one file may stage") to the byte budget `buildl-lua` enforces: the in-memory size of each staged record plus its text, up to the memory ceiling.|I-lua spec D3|the next slice that touches `buildl-core`|
   |15|Address-derived text is still nondeterministic: `tostring` or `string.format` (`%s`, `%p`) of a table or function renders an address, and `check`'s double run sees it only when the two addresses differ. Closing it means replacing `tostring` and wrapping `string.format` in the `buildl` module.|I-lua spec D12|unassigned|
   ```

4. Verify the follow-up table now holds rows 3, 5–8, 10, 11 and 13–15, in that order:

   ```bash
   sed -n '/^## 5/,/^## 6/p' docs/roadmap.md | grep -oE '^[|][0-9]+[|]' | tr -d '|' | tr '\n' ' '
   ```

   ```text
   3 5 6 7 8 10 11 13 14 15
   ```

5. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
6. Commit: `docs(repo): close the I-lua follow-ups and record three new ones`.

The I-lua row in §3 and the §4 completed-intents table are updated when the chain is marked done,
not here.

---

## Verification summary (plan-level)

- `RUSTDOCFLAGS="-D warnings" cargo doc -p buildl-lua --no-deps` exits `0`.
- Both grep checks (Tasks 2 and 3) print nothing, and the roadmap check (Task 4) prints the expected rows.
- `cargo make dod` exits `0`.
