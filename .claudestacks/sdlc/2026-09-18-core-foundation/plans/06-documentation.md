---
status: done
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

---

## Review findings

Every finding in this plan was a documentation-truth defect — a sentence claiming more than the shipped code supports. Four of the seven blocking items were prescribed verbatim by this plan's own task text, so the defect was the plan's rather than the implementer's.

- bug (blocking) — rule 4 read "[`ports`] never gains an implementation. Implementing a port means doing I/O, which this crate does not do." False of the shipped tree and self-contradictory: `ports/clock.rs:33` holds `impl Clock for FixedClock` under `#[cfg(test)]`, and rule 5 one line below asserts the opposite, as does the crate doc's own opening paragraph. Prescribed verbatim by task 1 step 2. Fixed to "never gains a production implementation … the only implementations here are test fakes". Verified by `cargo make dod` → `[cargo-make] INFO - Build Done in 8.82 seconds.`, exit 0, which runs rustdoc under `RUSTDOCFLAGS="-D warnings"` — `crates/buildl-core/src/lib.rs:38`.
- bug (blocking) — the rewritten §5 rule 2 overstated the guard and misattributed its own hole: it claimed the serializer "refuses any float that reaches it through a dependency's `Serialize` impl" and that "a post-serialization check alone would be insufficient", when `reject_floats` runs over the `serde_json::Value` tree produced by `serde_json::to_value` and therefore *is* a post-serialization check, carrying exactly the weakness the paragraph assigned to the hypothetical alternative. `json/canonical.rs`'s own module doc states this correctly. Prescribed by task 3 step 2. Rewritten to state that the walk catches every finite float, cannot catch `NaN`/`±Infinity` (which `serde_json` turns into `null` before the walk sees them), and that the real guarantee is `clippy.toml`'s `f32`/`f64` ban asserted by `cargo make guard-core-purity`. Third instance of this pattern in this chain — `docs/architecture.md:219`.
- risk (blocking) — "the `Clock` port that is the only way to obtain one" is false: `Timestamp::from_unix_nanos` is a `pub const fn` (`types/timestamp.rs:35`) and `Timestamp::UNIX_EPOCH` a `pub const` (`:31`), so any caller constructs a `Timestamp` with no port, as this chain's own plan 05 tests do. This ships to crates.io as published prose. Prescribed by task 2. Fixed to "the only way to obtain the current one" — `crates/buildl-core/README.md:16`.
- risk (blocking) — the same false claim in the §5.1 `Time` row, prescribed independently by task 4. Fixed identically; the nanosecond half was correct and kept — `docs/architecture-building-blocks.md:146`.
- risk (blocking) — "Every port returns the crate's one `Result`, not an error type of its own" is falsified by the code block immediately above it: `Clock::now(&self) -> Timestamp` returns no `Result` at all, and `ContentStore::contains -> bool`, `ActionCache::row -> Option<&CacheRow>` and `ActionCache::upsert -> ()` are also non-`Result`. Prescribed by task 4 step 2. Fixed to "Every fallible port method returns the crate's one `Result`". The rest of the paragraph was verified correct and kept — `docs/architecture-building-blocks.md:190`.
- risk (blocking) — the new §4 row shipped the literal placeholder `` `<last>` `` as its closing SHA, and its range *start* used a different convention from the row above it. Refusing to fabricate a hash was correct, but a placeholder in a tracked design document is not shippable either. The start was corrected from `8a5b22f` (a plan-set commit, the category the I-guard row deliberately excludes) to `70184be`, this chain's first implementation commit, matching I-guard's `1fa8421`. The closing element remains open at the commit gate — see Deviations — `docs/roadmap.md:89`.
- missing (blocking) — the repository root `README.md` still read "Nothing described here is implemented yet", which is false now that `buildl-core` ships the vocabulary, the error model, the canonical serializer and the `Clock` port, all unit-tested, and the roadmap records I2 as done. `CLAUDE.md` requires the root README stay consistent with the docs when either changes, and the root README is absent from this plan's six-file map — a hole in the plan, not implementer drift. Fixed with a minimal one-sentence status correction — `README.md:7`.
- nit — rule 2 ("[`types`] holds no decisions") was already bent by shipped code: `Timestamp::duration_since` takes two `Timestamp`s and decides that a backwards clock yields `None` rather than zero, and `Label::resolve` decides which of three reference forms it was handed while combining a `base: &Directory` with a raw string. Fixed with a same-kind carve-out citing both via intra-doc links, which the `-D warnings` doc build confirms resolve — `crates/buildl-core/src/lib.rs:34`.
- nit — "parsed once and never re-checked" is false: `Label` is `#[serde(try_from = "String")]` (`label.rs:27`), so every deserialize re-runs `Label::parse`. Fixed to "not revalidated after construction", the construction-time guarantee that is actually true — `crates/buildl-core/README.md:14`.
- nit — `Provenance` was glossed as "the declaring build file", dropping half the type: it carries the file *and* the directory it was evaluated in, with an accessor for each. Fixed to name both — `crates/buildl-core/README.md:15`.
- nit — "floating-point values refused" overstates in the same direction as the §5 rule 2 finding, since a non-finite float becomes JSON `null` before the serializer's walk can see it. Fixed to "finite floating-point values refused", matching what `json/canonical.rs`'s module doc guarantees — `crates/buildl-core/README.md:18`.
- nit — the two relative crate links resolve on GitHub but 404 on crates.io, where this README renders standalone (`readme = "README.md"` in the manifest). Pre-existing line, only reflowed by this diff; left alone — `crates/buildl-core/README.md:7`.
- nit — the table ships an empty `| | |` header row, which GitHub and crates.io both render as a blank header band. Prescribed by the plan's own replacement body; left as written — `crates/buildl-core/README.md:12`.
- nit — rules 1 and 5 govern phase modules that do not exist, and the lead-in says "A later phase of the pipeline gets its own module beside these", which `doc-comment-discipline` discourages as a forward qualifier in rustdoc; the same layout policy is already stated in `architecture-building-blocks.md` §5 and §7 where it can rot cheaply. Plan-prescribed content, and the reviewer itself called it a judgement call; left as written — `crates/buildl-core/src/lib.rs:28-45`.
- nit — the §2 diagram now types `Label.directory`/`Provenance.directory` as `Directory` and `Label.name` as `TargetName`, all three verified correct, but declares no `class Directory` or `class TargetName`, so both newtypes appear only as field types with no grammar anywhere in §2. `Timestamp` is absent from §2 entirely while §5.1 now lists it, and `Label::new` — a public infallible constructor that skips parsing — is unshown. Outside this plan's scope; left for a later docs pass — `docs/architecture.md:51-58`.
- nit — the §3 I2 row reads `**done**, 2026-10-04 (§4)` while both plan files were still `status: executing`. The plan prescribed this flip in task 6; resolved by flipping both plan statuses in the same commit series — `docs/roadmap.md:66`.
- verified — `resolve(s, base) Result~Label~` is fact, not forecast: `Label::resolve(raw: &str, base: &Directory) -> Result<Self>` exists at `crates/buildl-core/src/types/label.rs:76` and handles all three reference forms the §6 row and `design.md` §14 describe. The directory-only rejection is real too, since `parse` requires a `:` — `docs/architecture.md:55`.
- verified — all four new §6 decision rows check out. The float row's rationale matches `clippy.toml`'s own `f32`/`f64` reason text verbatim, and the `Timestamp` row's "no date dependency" claim holds because `crates/buildl-core/Cargo.toml` has exactly four dependencies: `serde`, `serde_json`, `sha2`, `thiserror` — `docs/architecture.md:239-242`.
- verified — all six plan task counts in the new §4 row are correct, counted from `^## Task ` in each plan file; `grep -c '|I1 review|' docs/roadmap.md` → `6` as the plan expected; follow-up 3 was narrowed rather than deleted; follow-up 11 uses the next free identifier with no renumbering of survivors — `docs/roadmap.md`.

## Probe results

- Claim: `crates/buildl-core/README.md` is eleven lines, line 5 describes the crate, and line 7 reads `**Status:** pre-release; the crate has no public API.` Command: `wc -l crates/buildl-core/README.md` and `cat -n crates/buildl-core/README.md`. Output: `11`, with line 7 matching verbatim. Holds.
- Claim: `architecture.md` §2 declares `Label` at :51-55 and `Provenance` at :56-59; §5 rule 2 is :214; the §6 table runs :221-230 with the `Structure` row last. Command: `sed -n '48,60p'`, `sed -n '214p'`, `sed -n '219,232p' docs/architecture.md`. Output: `class Label {` at 51 with `directory: String`/`name: String`; `class Provenance {` at 56 with `file: PathBuf`/`directory: String`; rule 2 at 214 containing `sorted keys, fixed float handling`; `|Structure|` last at 230. All hold.
- Claim: `design.md` §14's first bullet is :648 and the heading is §14. Command: `sed -n '644,652p' docs/design.md` and `grep -n '^## 14' docs/design.md`. Output: `646:## 14. Open questions`, with :648 matching the quoted bullet verbatim. Holds.
- Claim: roadmap's I2 row is :66, the I-guard §4 row :88, follow-up 3 :98, and `grep -c '|I1 review|'` is 6 with 11 the next free follow-up number. Commands: `sed -n '60,70p'`, `sed -n '82,100p'`, `grep -c '|I1 review|' docs/roadmap.md`, and a scan of the follow-up row numbers. Output: all three line cites correct; `6`; existing numbers `3 4 5 6 7 8 10`, so 11 is free. All hold.
- **Claim came out AGAINST the plan.** Task 4's line cites for `architecture-building-blocks.md` are stale by +12. Commands: `grep -n '^## \|^### ' docs/architecture-building-blocks.md` and `grep -n 'LoadError\|StoreError\|InfraFailure' docs/architecture-building-blocks.md`. Output: `### 5.1 Domain data` at `:138` and `### 5.2 Ports` at `:151`, not the implied `:132`/`:143`; the error types at `157`, `162`, `170`, `174`, `180`, `181`, against the plan's `145`, `150`, `158`, `162`, `168-169`. The §5.2 preamble "The signatures below are the shape, not the final API." is at `:136`, not `:124`. Cause: plan 04 task 1 inserted a twelve-line paragraph into §1.2 after plan 06 was written. Section numbers and all quoted text were unaffected. See Deviations.
- **Claim came out AGAINST the plan.** Task 4's verification `grep -n 'floating-point' docs/architecture-building-blocks.md crates/buildl-core/clippy.toml` cannot match in the second file. Command: that grep, then `grep -n 'f32\|f64' crates/buildl-core/clippy.toml`. Output: one hit in `architecture-building-blocks.md:43` and none in `clippy.toml`, because the ban's reason strings read "no float reaches canonical JSON"; the entries themselves are present at `clippy.toml:149` and `:150`. The command is wrong; the content it was meant to confirm is right.
- Claim: `Label::resolve` exists, so §2's new diagram entry is not forward-looking. Command: `grep -rn 'fn resolve' crates/buildl-core/src/`. Output: `crates/buildl-core/src/types/label.rs:76:    pub fn resolve(raw: &str, base: &Directory) -> Result<Self> {`. Holds.
- Claim (this one disproved the README and §5.1 rows): a `Timestamp` can only be obtained through the `Clock` port. Command: `grep -n 'pub const fn\|pub fn' crates/buildl-core/src/types/timestamp.rs` and `grep -n 'UNIX_EPOCH' crates/buildl-core/src/types/timestamp.rs`. Output: `31:    pub const UNIX_EPOCH: Self = Self(0);`, `35:    pub const fn from_unix_nanos(nanos: u64) -> Self {`, `41:    pub const fn as_unix_nanos(self) -> u64 {`, `50:    pub fn duration_since(self, earlier: Self) -> Option<Duration> {`. Both constructors are public, so the claim is false. Fixed in both files.
- Claim: `lib.rs`'s addition of "network" to the ban list is supported. Command: `grep -n 'TcpStream\|UdpSocket\|TcpListener\|SocketAddr' crates/buildl-core/clippy.toml`. Output: `139:    { path = "std::net::TcpListener", …`, `140: … TcpStream …`, `141: … UdpSocket …` — three entries, matching the "network (3)" family in §1.2. Holds.
- Claim: section numbering survives every edit. Method: `grep -o '^#\{2,3\} [0-9.]*'` captured for all four documents *before* any task ran, diffed after the fix round. Output: `architecture.md: numbering IDENTICAL`, `architecture-building-blocks.md: numbering IDENTICAL`, `design.md: numbering IDENTICAL`, `roadmap.md: numbering IDENTICAL`. The reviewer independently re-verified against `git show HEAD:docs/<file>` on `^#{1,4} ` with the same result, and confirmed `clippy.toml:21` (§1.2) and `clippy.toml:133-135` (`architecture.md` §5 rule 4) still resolve. Holds.
- Claim: §1.2's count paragraph, written during plan 04, is accurate — this plan's task 4 step 3 asked for confirmation rather than a rewrite. Method: the reviewer recounted `clippy.toml` from scratch. Output: 130 entries total (71 methods + 53 types + 6 macros), partitioning as filesystem 41, environment 23, threads 20, processes 15, platform filesystem extensions 13, standard streams 10, clock 3, network 3, floats 2 — summing to 130. Holds, now on a second independent count rather than the original derivation alone.
- Claim: `design.md:648`'s citation of "§5's own `deps = { "main.o", "util.o", "//lib:text" }`" is real. Method: reviewer located the literal. Output: present at `design.md:136`, inside §5 (lines 114-152), mixing bare names with an absolute label exactly as the bullet claims. Holds.

## Deviations

- 2026-10-04 — Task 4's line cites were stale by +12 throughout, because plan 04 task 1 inserted a twelve-line paragraph into §1.2 of the same file after this plan was written. The coder was briefed with the verified true positions and instructed to anchor on the quoted text rather than on line numbers, and to stop and report if any quoted text failed to match. Section numbering was unaffected and verified unchanged before and after.
- 2026-10-04 — Task 4's verification command greps for `floating-point` in `crates/buildl-core/clippy.toml`, which can never match, since the ban entries' reason strings read "no float reaches canonical JSON". The entries' presence was confirmed directly at `clippy.toml:149-150` instead. The command is defective; the content is correct.
- 2026-10-04 — Task 1's prescribed rule 4 text was false of the shipped tree and was narrowed in the fix round, deviating from the plan's verbatim block. Rule 2 was given a same-kind carve-out for the same reason: as prescribed, it was contradicted by `Timestamp::duration_since` and `Label::resolve`.
- 2026-10-04 — Tasks 2 and 4 independently prescribed the claim that the `Clock` port is the only way to obtain a `Timestamp`. It is false, because `Timestamp::from_unix_nanos` and `Timestamp::UNIX_EPOCH` are both public. Both files were narrowed to "the current one", deviating from the prescribed text in each.
- 2026-10-04 — Task 3's prescribed §5 rule 2 replacement misdescribed the mechanism it was documenting, attributing the non-finite-float hole to a hypothetical post-serialization check when the shipped serializer is itself one. Rewritten in the fix round to match `json/canonical.rs`.
- 2026-10-04 — Task 6 step 2 offered two exits for the closing commit SHA, which cannot exist when the row is written: commit and then amend, or cite the range through task 5's commit and note the documentation commits separately. Resolved by a third route the plan did not list but the I-guard row sets a precedent for: that chain's roadmap closure (`8a546e2`) is its own commit landing after its last work commit (`a718139`), and sits outside the range it cites. This chain followed the same shape — the row ships `` `70184be` … `13cc89c` ``, closing on the documentation commit, and the closure commit carrying that line is itself outside the range. A placeholder reached the working tree in the interim and a plausible hash was never fabricated. The range *start* was additionally corrected from the plan's `8a5b22f`, a plan-set commit, to `70184be`, the chain's first implementation commit, matching the convention the I-guard row above it uses.
- 2026-10-04 — The repository root `README.md` was edited although it is absent from this plan's six-file map, because `CLAUDE.md` requires the root README stay consistent with the docs and the docs changed in this plan. The edit was held to the one false status sentence; the README was not restructured.
- 2026-10-04 — `CLAUDE.md`'s own header, "Status: scaffold in place, implementation not started", is now stale for the same reason the root README's was. It was deliberately **not** edited: it is project configuration, and a reviewer's report carries no authority to change it. Surfaced to the author for decision.
