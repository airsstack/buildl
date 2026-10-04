---
status: approved
created: 2026-10-04
depends-on: [01, 02, 03, 04, 05]
---

# Documentation Implementation Plan

**Goal:** The crate's own documentation and the project's design documents describe what
`buildl-core` now holds.

**Architecture:** Prose only — no Rust changes, no new tests. Two tasks rewrite the crate's
rustdoc and README; four amend the design documents whose sentences this chain falsified, and the
roadmap. The documents are load-bearing: `clippy.toml` cites `architecture-building-blocks.md` §1.2,
and other documents cite `architecture.md` §2 and §5 by number, so the numbering survives every
edit here.

**Tech Stack:** Markdown, rustdoc, `cargo-make`.

---

## Context an implementer needs

Plans `01` through `05` are `done`. `crates/buildl-core` now holds:

```
src/lib.rs              export-only
src/error.rs            Error (InvalidName, CanonicalJson, FloatRejected), NameKind, Result
src/types/mod.rs        export-only
src/types/directory.rs  Directory
src/types/target_name.rs TargetName
src/types/label.rs      Label — parse, resolve
src/types/digest.rs     Digest
src/types/node_id.rs    NodeId
src/types/provenance.rs Provenance
src/types/timestamp.rs  Timestamp
src/json/mod.rs         export-only
src/json/canonical.rs   to_vec, to_string
src/ports/mod.rs        export-only
src/ports/clock.rs      Clock
```

Two traps in this plan, both easy to fall into:

- **Do not narrow the crate's stated responsibilities.** `crates/buildl-core/src/lib.rs:10-13`
  lists three: the domain values the phases exchange, the ports, and the pure logic of every phase.
  That list is `architecture-building-blocks.md` §5.4 and `docs/roadmap.md:47`, and it is still
  accurate — this chain shipped two of §5.4's rows (`Label::resolve`'s grammar and canonical JSON).
  The rewrite updates the status and the non-responsibilities; it must not reduce the crate to
  "domain types".
- **No internal planning vocabulary in source.** `claudestacks-guideline-rust:rust-guidelines` →
  `references/doc-comment-discipline` keeps plan identifiers, chain paths, workflow words and
  `docs/` section citations out of rustdoc and `//` comments. The crate doc states the module
  topology in its own words; it does not cite this plan or the spec.

`docs/` is the opposite case: section cross-references there are load-bearing and are cited by
number from other documents and from `clippy.toml:21`. Preserve every existing number; add
subsections rather than renumbering.

All work happens in the git worktree, on a branch, never on `main`. Commits follow Conventional
Commits; scope is `buildl-core` for tasks 1-2 and `repo` for tasks 3-6.

### File map

```
crates/buildl-core/src/lib.rs        — [modify] crate doc: what the crate holds, and the topology (task 1)
crates/buildl-core/README.md         — [modify] the public summary and the status line (task 2)
docs/architecture.md                 — [modify] §2 class diagram, §5 rule 2, §6 decision records (task 3)
docs/architecture-building-blocks.md — [modify] §5.1 type list, §5.2 port error types (task 4)
docs/design.md                       — [modify] §14's label-syntax bullet (task 5)
docs/roadmap.md                      — [modify] §3 status, §4 row, §5 follow-ups (task 6)
```

---

## Task 1 — Rewrite the crate's rustdoc

**Files:**
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Confirm what the doc comment claims today:

   ```
   $ sed -n '1,21p' crates/buildl-core/src/lib.rs
   ```

   The last two lines read "This file holds only module declarations and re-exports, so it carries
   no logic to unit-test." That is still true and stays.

2. Replace the doc comment, keeping the `pub mod` and `pub use` block below it untouched:

   ```rust
   //! Domain data, ports, and pure pipeline logic for the buildl build system.
   //!
   //! This crate is the centre buildl's other crates depend on. It depends on no other buildl
   //! crate, on no Lua runtime, and on no filesystem, process, thread, network, environment or
   //! clock API, so every pipeline flow can be exercised against in-memory implementations of its
   //! ports.
   //!
   //! # Responsibilities
   //!
   //! - The domain values the pipeline phases hand to one another.
   //! - The ports: traits through which the pipeline reaches build-file evaluation, storage,
   //!   hashing, and execution.
   //! - The pure logic of each phase: Load → Resolve → Plan → Execute → Record.
   //!
   //! # Non-responsibilities
   //!
   //! - Implementing any port. Concrete adapters live in the `buildl-lua` and `buildl` crates.
   //! - Reading or writing anything. A value arrives from a caller or through a port.
   //!
   //! # Where things live
   //!
   //! | Module | Holds |
   //! |---|---|
   //! | [`error`] | the one error enum and the crate-wide `Result` alias |
   //! | [`types`] | the validated domain values: names, identities, instants |
   //! | [`json`] | the one canonical serializer every byte of output goes through |
   //! | [`ports`] | the traits through which everything outside this crate is reached |
   //!
   //! A later phase of the pipeline gets its own module beside these, named for the phase. Five
   //! rules keep that arrangement navigable as it grows:
   //!
   //! 1. No phase module names another. Phases communicate through values, and the pipeline owns
   //!    the sequence, so a phase that imported the next would be bypassing it.
   //! 2. [`types`] holds no decisions. A type there validates and renders itself; logic needing
   //!    two of them to decide something belongs to the phase that decides it.
   //! 3. One concept, one type. [`types`] is the only place a domain value is declared, so nothing
   //!    downstream invents a second spelling of a name that already exists.
   //! 4. [`ports`] never gains an implementation. Implementing a port means doing I/O, which this
   //!    crate does not do.
   //! 5. Every phase module is exercised against in-memory implementations of the ports it uses,
   //!    with its tests in the file under test.
   //!
   //! # Status
   //!
   //! Pre-release. The vocabulary, the error model, the canonical serializer and the clock port
   //! exist; no pipeline phase is implemented yet.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.
   ```

   Every bracketed link is an intra-doc link to a module this crate now has, so
   `RUSTDOCFLAGS="-D warnings"` fails the build if one is wrong.

3. Confirm the documentation gate:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

4. Commit `docs(buildl-core): describe the vocabulary, serializer and port the crate now holds`.

---

## Task 2 — Rewrite the crate README

**Files:**
- Modify `crates/buildl-core/README.md`

**Steps:**

1. The file is eleven lines. Line 5 describes the crate, and line 7 reads
   `**Status:** pre-release; the crate has no public API.`, which is now false. Replace the body
   between the title and the License section:

   ```markdown
   # buildl-core

   Domain data, ports, and pure pipeline logic for the buildl build system.

   This crate performs no I/O and knows nothing about Lua: it is the home for the values the
   pipeline phases exchange, the traits (ports) through which the pipeline reaches the outside
   world, and the logic of every phase. Concrete implementations of the ports live in
   [`buildl-lua`](../buildl-lua) and [`buildl`](../buildl).

   ## What it holds today

   | | |
   |---|---|
   | Names | `Directory`, `TargetName`, `Label` — a target's absolute name, `//dir:name`, parsed once and never re-checked |
   | Identity | `Digest` (SHA-256), `NodeId` (a graph arena index), `Provenance` (the declaring build file) |
   | Time | `Timestamp`, and the `Clock` port that is the only way to obtain one |
   | Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text |
   | Serialization | one canonical JSON serializer: object keys sorted at every level, floating-point values refused |

   **Status:** pre-release. The vocabulary above is complete and unit-tested; no pipeline phase is
   implemented yet.

   ## License

   Apache-2.0
   ```

2. There is no gate over a README's prose, so verify by reading it back and checking the two
   relative links resolve:

   ```
   $ ls crates/buildl-lua crates/buildl
   ```

3. Commit `docs(buildl-core): describe the crate's public surface in its README`.

---

## Task 3 — Amend `architecture.md`

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. In the §2 class diagram, `Label` is declared at `:51-55` with `directory: String` and
   `name: String`, and `Provenance` at `:56-59` with `directory: String`. Change the three field
   types to the newtypes that now exist, leaving every other line of the diagram alone:

   ```
       class Label {
           directory: Directory
           name: TargetName
           parse(s) Result~Label~
           resolve(s, base) Result~Label~
       }
   ```

   ```
       class Provenance {
           file: PathBuf
           directory: Directory
       }
   ```

2. In §5 rule 2 (`:214`), the phrase "sorted keys, fixed float handling" names a float rule that
   was never specified. Replace that rule's text with what is now implemented:

   ```markdown
   2. **All JSON leaves through one canonical serializer** — object keys sorted at every level, and
      floating-point values refused outright. `graph.json`, `cache.json`, `log.jsonl`, and the bytes
      hashed into an `ActionKey` all use it, so "the key of X" and "the file of X" can never
      disagree. The float rule is enforced twice over, because one half cannot do it alone:
      `crates/buildl-core/clippy.toml` bans `f32` and `f64` in the crate, and the serializer refuses
      any float that reaches it through a dependency's `Serialize` impl. A post-serialization check
      alone would be insufficient — `serde_json` turns `NaN` and `±Infinity` into `null` before any
      inspection sees them.
   ```

3. Append four rows to the §6 decision-record table (`:221-230`), after the existing `Structure`
   row:

   ```markdown
   |Label syntax|**Absolute only, with a separate resolver**|A `Label` is `//dir:name` and nothing else, so no phase downstream asks whether the one it holds still needs resolving. `Label::resolve` is the single place the three forms a build file may write — `//dir:name`, `:sibling`, and a bare `name` — become one. Closes the relative-label half of design §14.|
   |Directory-only labels|**Rejected, not resolved to `//dir:dir`**|Bazel's shorthand is not adopted: the design document states no such rule, and one target with two spellings breaks the `Display` → `parse` round-trip and admits two cache keys meaning the same target.|
   |Floats in canonical JSON|**Refused outright, banned at compile time**|Nothing in the domain model is float-valued, and a non-finite float serializes as `null`, which would make an action key disagree with the value it was computed from. See §5 rule 2.|
   |`Timestamp` representation|**`u64` nanoseconds since the Unix epoch; no date dependency**|The clock feeds durations as well as event timestamps (§5 rule 4) and `Instant` is banned in `buildl-core`, so a duration is the difference of two timestamps — which second resolution could not express. Unrelated to `SOURCE_DATE_EPOCH`, a whole-second value the sandbox hands to an action. Formatting belongs to the adapter that displays it, where a date library may be taken.|
   ```

4. Confirm no section number moved:

   ```
   $ grep -n '^## \|^### ' docs/architecture.md
   ```

   The headings must be unchanged from before this task — other documents cite `architecture.md` §2,
   §4, §5 and §6 by number, and `crates/buildl-core/clippy.toml:133-135` cites §5 rule 4.

5. Commit `docs(repo): record the core-foundation types and decisions in the architecture`.

---

## Task 4 — Amend `architecture-building-blocks.md`

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. In the §5.1 domain-data table, replace line 132 and insert a new row after line 133, leaving
   the `Content` row's text as it stands:

   ```markdown
   |Names and identity|`Directory`, `TargetName`, `Label`, `NodeId`, `Provenance`|
   |Content|`Digest` — the single SHA-256 type for files, outputs, keys, and log blobs|
   |Time|`Timestamp` — nanoseconds since the Unix epoch, obtained only through the `Clock` port|
   ```

   The table's remaining rows — `Phase handoffs`, `Incrementality`, `Authority`, `Classification` —
   describe types later slices build and are untouched.

2. §5.2's port sketches (`:143-174`) name three error types — `LoadError` at `:145`, `StoreError`
   at `:150`, `:158` and `:162`, `InfraFailure` at `:168-169` — that do not exist and will not.
   §5.2's own preamble at `:124` already says "The signatures below are the shape, not the final
   API"; make the error half concrete by replacing each with the crate's one `Result`, and add a
   sentence beneath the code block:

   ```markdown
   Every port returns the crate's one `Result`, not an error type of its own: `architecture.md` §4
   settles the error model as a single structured enum, so `Result<StagedFile>` above is
   `core::result::Result<StagedFile, buildl_core::Error>`. The enum is `#[non_exhaustive]`, and each
   phase adds the variants it earns.
   ```

   Change the six signatures accordingly — `Result<StagedFile, LoadError>` becomes
   `Result<StagedFile>`, `Result<Digest, StoreError>` becomes `Result<Digest>`, and so on.

3. The §1.2 float paragraph was already added by plan `04` task 1. Confirm it is still there and
   reads correctly against the final `clippy.toml`:

   ```
   $ grep -n 'floating-point' docs/architecture-building-blocks.md crates/buildl-core/clippy.toml
   ```

4. Confirm no section number moved:

   ```
   $ grep -n '^## \|^### ' docs/architecture-building-blocks.md
   ```

   §1.2, §5.1, §5.2, §5.3, §5.4, §6 and §7 are cited by number from `architecture.md`,
   `crates/buildl-core/clippy.toml:21`, `docs/roadmap.md` and this chain's own artifacts.

5. Commit `docs(repo): name the core-foundation types and the one port error type`.

---

## Task 5 — Narrow `design.md` §14's label-syntax question

**Files:**
- Modify `docs/design.md`

**Steps:**

1. §14's first bullet (`:648`) currently reads:

   ```markdown
   - **Label syntax details:** `//dir:target` adopted here; relative labels (`:sibling`), and whether `subdir` should be implicit via file discovery, are open.
   ```

   Replace it with the half that is now settled plus the half that is not:

   ```markdown
   - **Label syntax details:** _partially resolved_ — `//dir:target` is adopted, and relative labels are too: a build file may write `:sibling` or a bare `name`, both resolved against the declaring directory, which is what §5's own `deps = { "main.o", "util.o", "//lib:text" }` already mixes. A directory-only `//dir` is rejected rather than read as `//dir:dir`. Whether `subdir` should be implicit via file discovery remains open.
   ```

   The bullet is narrowed, not deleted: the file-discovery half is untouched by this chain.

2. Confirm nothing else in `design.md` moved, and that §14 is still §14 — `architecture.md` and
   this chain's artifacts cite design sections by number:

   ```
   $ grep -n '^## \|^### ' docs/design.md
   ```

3. Commit `docs(repo): record that relative labels are settled`.

---

## Task 6 — Update the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. §3's I2 row (`:66`) has Status `**intent approved**, 2026-09-18; chain
   `2026-09-18-core-foundation``. Replace that cell with the completion form the I1 and I-guard rows
   use — `**done**, <YYYY-MM-DD> (§4)` — using the date of the last commit in this chain.

2. Add a §4 row beneath the I-guard row (`:88`), following its shape. The commit range runs from
   this chain's first commit to its last; the last is the commit this task itself creates, so write
   the range after committing and amend, or write it as the range through task 5's commit and note
   the documentation commits separately — the I-guard row faced the same problem and cites
   `1fa8421 … a718139`:

   ```markdown
   |I2 foundation|`.claudestacks/sdlc/2026-09-18-core-foundation/`|`01-error-model` (2 tasks), `02-label-types` (4 tasks), `03-identity-types` (4 tasks), `04-canonical-json` (4 tasks), `05-clock-port` (1 task), `06-documentation` (6 tasks)|`<first>` … `<last>`|
   ```

3. §5's follow-up 3 (`:98`) assigns a crate-doc refresh to "each crate's first code slice (I2,
   I-lua, adapters)". This chain closed the `buildl-core` share. Narrow the row rather than deleting
   it — `buildl-lua`, `buildl` and `buildl-cli` are still stale:

   ```markdown
   |3|Refresh the crate-level docs once real modules exist. `buildl-core` is done (§4). Still open: `buildl-lua`, `buildl` and `buildl-cli` — port lists that omit `Manifest`/`Approver`, `Reporter`, `StatCache`/`ToolResolver` and `Dispatcher`; "holds only module declarations and re-exports" in `lib.rs` files that hold neither; `buildl`'s adapter list, which disagrees between its README and `lib.rs`.|I1 review|each crate's first code slice (I-lua, adapters)|
   ```

   `Clock` leaves the omitted-ports list: `buildl-core` now has it.

4. Add a §5 row for the guard this chain deliberately did not build. Use the next free number —
   the `#` column is an identifier other documents cite, so survivors are never renumbered:

   ```markdown
   |11|No guard asserts that a phase module in `buildl-core` does not import another. The rule is `architecture.md`'s "no phase invokes the next"; the mechanism would be a golden file of permitted intra-crate module edges, diffed by a `cargo make` task, in the shape of `guard-crate-edges`. I2 shipped no phase module, so the guard would have asserted nothing.|I2 review|I4 Resolve, the first slice with two phase modules|
   ```

5. Confirm the row identifiers are intact and the counts moved as expected:

   ```
   $ grep -c '|I1 review|' docs/roadmap.md
   6
   ```

   Unchanged from before this task: follow-up 3 is narrowed, not removed, and the new row carries an
   `I2 review` origin.

6. Run the gate one last time over the whole chain's work:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ cargo make deny
   advisories ok, bans ok, licenses ok, sources ok
   ```

7. Commit `docs(repo): close the core-foundation slice in the roadmap`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
$ grep -n '^## \|^### ' docs/architecture.md docs/architecture-building-blocks.md docs/design.md
```

The first three exit 0. The fourth is the check that matters for this plan: every section heading
must be unchanged from before it ran, because `clippy.toml:21` and `:133-135`, the other design
documents, and this chain's own spec all cite those sections by number.

One count in the spec is overstated and worth correcting while editing these documents, or leaving
alone deliberately. Spec §9 says ten public items return `Result` and each needs a `# Errors`
section. Seven do: `Directory::parse`, `TargetName::parse`, `Label::parse`, `Label::resolve`,
`Digest::from_hex`, `canonical::to_vec`, `canonical::to_string`. The three `FromStr::from_str`
impls get none, and the gate stays green because `clippy::missing_errors_doc` does not fire on a
trait-impl method. Nothing is broken either way.

At the end of this plan the crate documents what it holds, the four design documents describe the
types that exist rather than the ones that were planned, `design.md` §14 records what this chain
settled, and the roadmap shows I2 done with its one deliberate follow-up assigned to I4.
