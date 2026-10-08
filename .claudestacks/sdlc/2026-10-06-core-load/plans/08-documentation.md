---
status: done
created: 2026-10-06
depends-on: [07]
---

# Documentation Implementation Plan

**Goal:** Every document that describes `buildl-core`, Load or the declaration boundary states what
this chain shipped.

**Architecture:** Prose only — no Rust code changes, no new tests. Three tasks rewrite the crate's
own rustdoc and README; four amend the design documents this chain falsified (spec §9's table, row
by row) plus the repository README; one closes the slice in the roadmap. `docs/` section numbers and
Mermaid blocks are load-bearing and are preserved: other documents, `clippy.toml` and this chain's
artifacts cite them by number.

**Tech Stack:** rustdoc, Markdown, Mermaid, `cargo-make`.

**Content authority:** spec §9 (documentation deliverables and amendments), with the facts each
amendment states taken from spec §2 (D7), §3.3, §3.4, §4.2, §4.4, §5.1, §5.2, §5.3, §6.2 and §8.

---

## Context an implementer needs

Plans `01` through `07` are `done`. `crates/buildl-core` now holds, beyond I2's vocabulary:

```
src/types/{written,source_path,output_name,env_name,argument,description,setting,
           entry_name,field_name,diagnostic,declaration,build_file}.rs, grammar.rs (private)
src/error.rs              + Evaluation, MissingBuildFile, InvalidDeclaration, ActionConflict,
                            Nondeterministic; EvaluationFailure, EvaluationLimit, DeclarationField, ActionFound
src/ports/declaration_source.rs   DeclarationSource
src/ports/bundle.rs               Ports
src/load/{mod,traversal}.rs       load
src/pipeline/{mod,driver}.rs      Pipeline::new, Pipeline::check
tests/flows/{main,common,load,check}.rs   one integration binary; FakeSource, FakeFile, FakePorts
```

Every text block in tasks 1 and 2 was spliced into a copy of the gate-green prototype of this final
state and passed `cargo make dod` (rustdoc under `RUSTDOCFLAGS=-D warnings`, clippy `-D warnings`,
doctests) — so each intra-doc link in them resolves against the code plans 01–07 produce.

Rules that bind this plan:

- **No planning vocabulary in rustdoc or the crate README.** No spec, plan or `§` citation, no "I3",
  no chain path. The crate docs state the topology in their own words. `docs/` and `docs/roadmap.md`
  are the opposite: section cross-references there are load-bearing and stay.
- **`load` is both a function and a module at the crate root.** A bare ``[`load`]`` link is
  ambiguous and fails the doc build; the crate doc writes ``[`load`](mod@load)`` for the module and
  ``[`load`](fn@load)`` for the function.
- **Preserve every `docs/` heading and every Mermaid block.** Diagrams are edited in place as
  Mermaid, never converted. Each markdown task checks its headings against `HEAD` before committing.
- **Do not narrow the crate's stated responsibilities** (`lib.rs:8-13`): the three-item list stays
  exactly as it is.
- **Anchor on quoted text, not on line numbers.** Line numbers below are the worktree's before this
  plan runs (plans 01–07 touch no `docs/` file and not the crate README). If a quoted "current" text
  does not match, stop and report rather than improvising.

Two items plan `05` deliberately deferred here land in task 2: `DeclarationSource::evaluate`'s
`# Errors` link to `Error::Evaluation` (plan 05 wrote a code span because plan 04 might not have
landed), and `ports/mod.rs`'s "every trait of the crate lives here" sentence.

The repository `README.md` (task 7) is in spec §9's table: `CLAUDE.md` requires it to stay
consistent with the docs, and its status line says "no pipeline phase is implemented yet", which
this chain makes false. One amendment goes beyond the table: `architecture-building-blocks.md` §1.1
(task 6 step 1), whose "runs in `buildl-core`'s unit tests" the move of the fakes to integration
tests makes false.

Commits: `docs(buildl-core)` for tasks 1–3, `docs(repo)` for tasks 4–8, one per task.

### File map

```
crates/buildl-core/src/lib.rs                       — [modify] crate doc: module map, rule 1 and 5, status (task 1)
crates/buildl-core/src/types/mod.rs                 — [modify] module doc: index of every type (task 2)
crates/buildl-core/src/ports/mod.rs                 — [modify] module doc: every trait, the bundle included (task 2)
crates/buildl-core/src/ports/declaration_source.rs  — [modify] `# Errors` intra-doc link (task 2)
crates/buildl-core/README.md                        — [modify] what the crate holds, status (task 3)
docs/design.md                                      — [modify] §12.1 double run, §14 subdir (task 4)
docs/architecture.md                                — [modify] §2 diagram + Label bullet, §3.1, §5 rule 5, §7 (task 5)
docs/architecture-building-blocks.md                — [modify] §1.1 line, §5.2, §5.3, §8 (task 6)
README.md                                           — [modify] status line (task 7)
CLAUDE.md                                           — [modify] status section becomes a pointer to the roadmap (task 7)
docs/roadmap.md                                     — [modify] I3 row, §4 row, §5 follow-up 12 (task 8)
```

---

## Task 1 — Rewrite the crate-level rustdoc

**Files:**
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Confirm the current doc comment, `lib.rs:1-52`. Plans 06 and 07 add only the `pub mod load;`,
   `pub mod pipeline;` and `pub use` lines, so the module table still lacks the `load` and
   `pipeline` rows; this task adds both. The lines this task changes read:

   - `lib.rs:25` — ``//! | [`types`] | the validated domain values: names, identities, instants |``
   - `lib.rs:27` — ``//! | [`ports`] | the traits through which everything outside this crate is reached |``
   - `lib.rs:29-30` — `//! A later phase of the pipeline gets its own module beside these, named for the phase. Five`
     / `//! rules keep that arrangement navigable as it grows:`
   - `lib.rs:32-33` — `//! 1. No phase module names another. Phases communicate through values, and the pipeline owns`
     / `//!    the sequence, so a phase that imported the next would be bypassing it.`
   - `lib.rs:43-44` — `//! 5. Every phase module is exercised against in-memory implementations of the ports it uses,`
     / `//!    with its tests in the file under test.`
   - `lib.rs:48-49` — `//! Pre-release. The vocabulary, the error model, the canonical serializer and the clock port`
     / `//! exist; no pipeline phase is implemented yet.`

   ```
   $ sed -n '1,56p' crates/buildl-core/src/lib.rs
   ```

2. Replace every `//!` line at the top of the file (the whole doc comment, up to the blank line before
   `pub mod error;`) with the block below. Leave the `pub mod` and `pub use` lines untouched.

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
   //! | [`types`] | the validated domain values — names, identities, instants, declarations — and the as-written values a build file stages |
   //! | [`json`] | the one canonical serializer every byte of output goes through |
   //! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each |
   //! | [`load`](mod@load) | Load, the first phase: build files to validated, sorted declarations |
   //! | [`pipeline`] | the phases run in sequence over one bundle of ports, one method per command |
   //!
   //! Each phase has its own module, named for the phase. Five rules keep that arrangement
   //! navigable:
   //!
   //! 1. No phase module names another. Phases communicate through values, and [`pipeline`] owns
   //!    the sequence, so a phase that imported the next would be bypassing it.
   //! 2. [`types`] holds no decisions between *different* concepts. A type there may decide things
   //!    about its own kind — [`Timestamp::duration_since`] decides whether two instants of the
   //!    same type went backwards, [`Label::resolve`] decides which of three reference forms it was
   //!    handed — but logic needing two different concepts to decide something belongs to the
   //!    phase that decides it.
   //! 3. One concept, one type. [`types`] is the only place a domain value is declared, so nothing
   //!    downstream invents a second spelling of a name that already exists.
   //! 4. [`ports`] never gains a production implementation. A real implementation means doing I/O,
   //!    which this crate does not do; the only implementations here are test fakes.
   //! 5. Every phase module is exercised against in-memory implementations of the ports it uses.
   //!    Its pure helpers, which need no port, are tested in the file under test; its flows through
   //!    a port are tested in this crate's integration tests, against fakes built from the public
   //!    API alone, so anything a fake does an adapter crate can do too.
   //!
   //! # Status
   //!
   //! Pre-release. Load is implemented: [`load`](fn@load) walks the workspace's build files
   //! through the [`DeclarationSource`] port and returns their declarations validated and sorted,
   //! and [`Pipeline::check`] runs it twice to catch a build file that declares differently on
   //! each evaluation. Resolve, Plan, Execute and Record are not implemented yet.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.
   ```

   Rules 2, 3 and 4 and everything above the module table are unchanged; they are repeated so the
   block replaces the comment whole.

3. Verify:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ grep -c 'no pipeline phase is implemented yet\|with its tests in the file under test' crates/buildl-core/src/lib.rs
   0
   $ grep -c 'mod@load\|fn@load\|Pipeline::check\|integration tests' crates/buildl-core/src/lib.rs
   4
   ```

   Expected: `dod` exits 0 — rustdoc runs under `-D warnings`, so a wrong or ambiguous link fails it.

4. Commit `docs(buildl-core): map the load and pipeline modules in the crate doc`.

---

## Task 2 — Complete the `types` and `ports` module docs

**Files:**
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/ports/declaration_source.rs`

**Steps:**

1. `types/mod.rs`. Plans 01–03 add the new `pub mod`/`pub use` lines, and plans 01–02 add
   Responsibilities bullets. Its doc comment opens with
   `//! The domain vocabulary: the validated values the pipeline's phases hand to one another.`
   and its non-responsibility reads (anchor on the quoted text; plans 01–02 shift the line numbers):

   ```rust
   //! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
   //! two of them to decide something belongs to the module for the phase that decides it.
   ```

   That is narrower than the crate's rule 2, which `Written<Directory>::under` and `Label::resolve`
   already rely on. Replace every `//!` line at the top of the file with:

   ```rust
   //! The domain vocabulary: the validated values the pipeline's phases hand to one another, and the
   //! as-written values a build file stages before they are validated.
   //!
   //! Its own directory because these are the crate's values with a grammar to get wrong, and each
   //! is parsed once at the edge so no later phase re-checks it. Validation lives in the sibling
   //! file named for the type; this file is the index.
   //!
   //! Responsibilities:
   //!
   //! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
   //! - [`Directory`] — a workspace-relative directory path.
   //! - [`TargetName`] — the name half of a label.
   //! - [`Label`] — a target's absolute name.
   //! - [`NodeId`] — a target's handle inside the graph's arenas.
   //! - [`Provenance`] — the build file and directory a declaration came from.
   //! - [`Timestamp`] — an instant on the host wall clock.
   //! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
   //!   [`SettingName`], [`SettingValue`], [`EntryName`], [`FieldName`] — the validated values a
   //!   declaration's fields hold.
   //! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
   //! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
   //! - [`Declaration`] and its parts — what a build file declared, validated.
   //! - [`BuildFile`], [`Evaluated`] and the staged values — what crosses the build-file port.
   //!
   //! Non-responsibilities: decisions between different concepts. A type here validates and renders
   //! itself, and may combine values of its own kind; logic that needs two different concepts to
   //! decide something belongs to the module for the phase that decides it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.
   ```

2. `ports/mod.rs`. Plan 05 task 2 step 2 left its doc comment as:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //! - [`Ports`] — the bundle naming one implementation of each port.
   //!
   //! Non-responsibilities: implementations. A type implementing one of these traits does I/O by
   //! definition, so it cannot live in this crate.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.
   ```

   It lacks the sentence that every trait lives here, and its non-responsibility is false:
   `ports/clock.rs` holds a test `FixedClock` that implements `Clock` and does no I/O, and
   `tests/flows/common.rs` implements both traits the same way. Replace every `//!` line at the top
   of the file with:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API. Every
   //! trait of the crate lives here, the bundle included, so no logic module declares one.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
   //!   parameter instead of one per port.
   //!
   //! Non-responsibilities: production implementations. A real implementation does I/O, so it lives
   //! in an adapter crate; the only implementations in this crate are test fakes.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.
   ```

3. `ports/declaration_source.rs`. Plan 05 task 1 wrote the method's `# Errors` line as a code span:

   ```rust
       /// Returns `Error::Evaluation` when the file exists but cannot be evaluated.
   ```

   Plan 04 has landed, so replace that one line with the intra-doc link (two lines, to stay within
   the line width):

   ```rust
       /// Returns [`Error::Evaluation`](crate::Error::Evaluation) when the file exists but cannot be
       /// evaluated.
   ```

4. Verify:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ grep -c 'trait of the crate lives here, the bundle included' crates/buildl-core/src/ports/mod.rs
   1
   $ grep -c 'does I/O by$' crates/buildl-core/src/ports/mod.rs
   0
   $ grep -c '(crate::Error::Evaluation)' crates/buildl-core/src/ports/declaration_source.rs
   1
   $ grep -c 'decisions between different concepts' crates/buildl-core/src/types/mod.rs
   1
   ```

   Expected: `dod` exits 0; every bullet's link resolves under `-D warnings`.

5. Commit `docs(buildl-core): index every type and trait in the types and ports modules`.

---

## Task 3 — Rewrite the crate README

**Files:**
- Modify `crates/buildl-core/README.md`

**Steps:**

1. Current text, `crates/buildl-core/README.md:10-21`:

   ```markdown
   ## What it holds today

   | | |
   |---|---|
   | Names | `Directory`, `TargetName`, `Label` — a target's absolute name, `//dir:name`, not revalidated after construction |
   | Identity | `Digest` (SHA-256), `NodeId` (a graph arena index), `Provenance` (the declaring build file and the directory it was evaluated in) |
   | Time | `Timestamp`, and the `Clock` port that is the only way to obtain the current one |
   | Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text |
   | Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused |

   **Status:** pre-release. The vocabulary above is complete and unit-tested; no pipeline phase is
   implemented yet.
   ```

2. Replace those lines with (lines 1-9 and the License section stay):

   ```markdown
   ## What it holds today

   | | |
   |---|---|
   | Names | `Directory`, `TargetName`, `Label` — a target's absolute name, `//dir:name`, not revalidated after construction |
   | Identity | `Digest` (SHA-256), `NodeId` (a graph arena index), `Provenance` (the declaring build file and the directory it was evaluated in) |
   | Time | `Timestamp`, and the `Clock` port that is the only way to obtain the current one |
   | Declarations | `Declaration` — one `Target`, `Rule`, `Alias` or `Setting`, with the provenance of the build file that declared it — and the validated values its fields hold, such as `SourcePath`, `OutputName`, `EnvName`, `Command` and `SettingValue` |
   | Build-file port | `DeclarationSource` evaluates one build file into a `StagedFile`, or reports it absent: every value exactly as the file wrote it, a `Written<T>` typed by what it must become, plus the file's `subdir` requests |
   | Load | `load` walks the build files from the workspace root along their `subdir` requests and returns every declaration validated and sorted; `Pipeline::check`, over one `Ports` bundle, runs it twice and fails when the two runs differ |
   | Errors | one structured enum with a `Result` alias; callers branch on fields, never on message text; Load's errors name the build file and, where there is one, the declaration and field |
   | Serialization | one canonical JSON serializer: object keys sorted at every level, finite floating-point values refused |

   **Status:** pre-release. Load, the first pipeline phase, is implemented and tested against
   in-memory ports; Resolve, Plan, Execute and Record are not implemented yet.
   ```

3. Verify:

   ```
   $ grep -c 'no pipeline phase is' crates/buildl-core/README.md
   0
   $ grep -c '| Declarations |\|| Build-file port |\|| Load |\|Load, the first pipeline phase' crates/buildl-core/README.md
   4
   $ grep -n 'I3\|§\|spec\|plan ' crates/buildl-core/README.md
   ```

   Expected: the last command prints nothing — the README carries no planning reference.

4. Commit `docs(buildl-core): describe declarations, the build-file port and Load in the README`.

---

## Task 4 — Amend `design.md` §12.1 and §14

**Files:**
- Modify `docs/design.md`

**Steps:**

1. §12.1, `design.md:586`, current:

   ```markdown
   Because declaration is cheap it is also _checkable_: evaluate twice, compare staging-list hashes; a nondeterministic build file fails `buildl check` with the diff.
   ```

   Replace with (spec §5.2: by value, not by digest; the diff is the first differing pair):

   ```markdown
   Because declaration is cheap it is also _checkable_: evaluate twice and compare the two sorted staging lists by value; a nondeterministic build file fails `buildl check` with the diff — the first position at which the two runs disagree, with what each run declared there. A `pairs` loop does not fail the check, because both lists are sorted before they are compared.
   ```

2. §14's first bullet, `design.md:648`, current:

   ```markdown
   - **Label syntax details:** _partially resolved_ — `//dir:target` is adopted, and relative labels are too: a build file may write `:sibling` or a bare `name`, both resolved against the declaring directory, which is what §5's own `deps = { "main.o", "util.o", "//lib:text" }` already mixes. A directory-only `//dir` is rejected rather than read as `//dir:dir`. Whether `subdir` should be implicit via file discovery remains open.
   ```

   Replace with (spec D7):

   ```markdown
   - **Label syntax details:** _resolved_ — `//dir:target` is adopted, and relative labels are too: a build file may write `:sibling` or a bare `name`, both resolved against the declaring directory, which is what §5's own `deps = { "main.o", "util.o", "//lib:text" }` already mixes. A directory-only `//dir` is rejected rather than read as `//dir:dir`. `subdir` is explicit only, never implicit via file discovery: a build file is evaluated because an evaluated build file named its directory, never because it exists on disk, so the evaluated set is exactly the root plus the closure of `subdir` requests.
   ```

3. Verify:

   ```
   $ grep -c 'compare staging-list hashes\|Whether `subdir` should be implicit' docs/design.md
   0
   $ grep -c 'compare the two sorted staging lists by value\|`subdir` is explicit only, never implicit' docs/design.md
   2
   $ diff <(git show HEAD:docs/design.md | grep '^#\{2,4\} ') <(grep '^#\{2,4\} ' docs/design.md)
   ```

   Expected: the `diff` prints nothing — no heading moved.

4. Commit `docs(repo): record explicit subdir and the by-value double run in the design`.

---

## Task 5 — Amend `architecture.md` §2, §3.1, §5 and §7

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. §2 Mermaid class diagram. The `Declaration` class, `architecture.md:61-68`, current:

   ```
       class Declaration {
           label: Label
           provenance: Provenance
           action: ActionTemplate
           inputs: Vec~InputSpec~
           deps: Vec~Label~
           flags: TargetFlags
       }
   ```

   Replace with the shape of spec §3.3 (still inside the same `mermaid` block):

   ```
       class Declaration {
           provenance: Provenance
           item: Declared
       }
       class Declared {
           <<enumeration>>
           Target
           Rule
           Alias
           Setting
       }
       class Target {
           label: Label
           role: TargetRole
           action: Action
           inputs: Vec~SourcePath~
           deps: Vec~Label~
           outputs: Vec~OutputName~
           env: Vec~EnvName~
           network: NetworkAccess
           freshness: Freshness
       }
       class Rule {
           label: Label
           run: Command
           description: Option~Description~
       }
       class Alias {
           label: Label
           target: Label
       }
       class Setting {
           name: SettingName
           default: SettingValue
       }
   ```

   Then the relations, `architecture.md:94-95` before step 1's insertion, current:

   ```
       Declaration --> Label
       Declaration --> Provenance
   ```

   Replace with:

   ```
       Declaration --> Provenance
       Declaration --> Declared
       Declared --> Target
       Declared --> Rule
       Declared --> Alias
       Declared --> Setting
       Target --> Label
   ```

   Every other line of the diagram — `Label`, `Provenance`, `TargetGraph`, `ActionKey`, `Digest`,
   `Plan`, `ActionOutcome` and their relations — stays as it is.

2. §2's `Label` bullet, `architecture.md:103`, current:

   ```markdown
   - **`Label` is the universal name** (`//dir:name`), parsed and validated once at Resolve; everywhere downstream a target is a `NodeId` (`u32` index into the graph's arenas). Strings appear only at the edges — parsing and reporting.
   ```

   Replace with (spec D2):

   ```markdown
   - **`Label` is the universal name** (`//dir:name`), resolved and validated once at Load: every name and reference a build file writes — absolute, `:sibling` or bare — becomes an absolute `Label` against the declaring directory before Load hands its declarations on; from Resolve onward a target is a `NodeId` (`u32` index into the graph's arenas). Strings appear only at the edges — parsing and reporting.
   ```

   §2's `Provenance` bullet, `architecture.md:106`. After step 1 the diagram has a `Target` class
   for the declared record, which holds no provenance, so "into `Target`" would now name the wrong
   type. Current fragment:

   ```markdown
   Attached in `declare`, carried through `Declaration` into `Target`, surfaced in every error.
   ```

   Replace with:

   ```markdown
   Attached in `declare`, carried through `Declaration` into the graph, surfaced in every error.
   ```

3. §3.1, `architecture.md:148`, current:

   ```markdown
   The mechanics of collection: the `buildl` `HostModule`'s closures capture an `Arc<Mutex<Vec<Declaration>>>` staging buffer plus the current file's `Provenance`. `b.target(...)` validates its option table shape _immediately_ (unknown field → error naming file and field, airsl-refusal style) and pushes a `Declaration`. `b.subdir(dir)` pushes onto the directory queue owned by the Load driver — Lua never recurses.
   ```

   Replace with (spec §3.4, §4.2):

   ```markdown
   The mechanics of collection: the `buildl` `HostModule`'s closures capture an `Arc<Mutex<StagedFile>>` staging buffer for the file being evaluated. `b.target(...)` validates its option table shape _immediately_ (an unknown field, or a field of the wrong Lua type → `EvaluationFailure::UnknownField` / `WrongFieldType`, airsl-refusal style) and pushes a staged declaration: every value exactly as written, a `Written<T>` typed by what it must become, numbered with its `DeclarationOrder`. `b.subdir(dir)` pushes a `StagedSubdir` into the same buffer. Both return to `buildl-core` inside the `StagedFile` that `evaluate` hands back; Load attaches the `Provenance` (it built the `BuildFile`, so it already knows which file the result belongs to), validates every value, and resolves the requested directories, rejecting escapes, skipping repeats and queueing the rest — Lua never recurses, and the adapter never sees the queue.
   ```

4. §3.1, `architecture.md:150`, current:

   ```markdown
   Parallel load: the directory queue is processed by a small pool; each worker owns its engines, staging buffers merge at the end, and the merge sorts by (directory, declaration order) so parallel load yields the identical staging list as serial load. Determinism rule: parallelism must never be observable in any output (§6).
   ```

   Replace with (spec §4.4):

   ```markdown
   Parallel load: the directory queue is processed by a small pool; each worker owns its engines, staging buffers merge at the end, and the merge sorts by (directory, declared name, then the whole declaration) so parallel load yields the identical staging list as serial load. The key holds no declaration order: under `pairs` the same file issues its declarations in a different order on each evaluation. Determinism rule: parallelism must never be observable in any output (§6).
   ```

5. §5 rule 5, `architecture.md:227`, current:

   ```markdown
   5. **Double-run checks are CI, not doctrine.** `buildl check` runs declaration twice and diffs staging hashes; the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.json`, and cas contents.
   ```

   Replace with:

   ```markdown
   5. **Double-run checks are CI, not doctrine.** `buildl check` runs declaration twice and compares the two sorted staging lists by value; the test suite builds a fixture workspace twice and asserts byte-identical `graph.json`, `cache.json`, and cas contents.
   ```

6. §7, `architecture.md:250`, current:

   ```markdown
   - **`declare`** (`buildl-lua`): fixture build files → assert exact `Vec<Declaration>`; hostile fixtures (huge loops, `pairs` tricks, bad option tables) → assert refusal shape and instruction-ceiling stops.
   ```

   Replace with:

   ```markdown
   - **`declare`** (`buildl-lua`): fixture build files → assert exact `StagedFile`s, the adapter's whole output (turning them into `Vec<Declaration>` is Load's, tested in `buildl-core`); hostile fixtures (huge loops, `pairs` tricks, bad option tables) → assert refusal shape and instruction-ceiling stops.
   ```

7. Verify:

   ```
   $ grep -c 'flags: TargetFlags\|parsed and validated once at Resolve\|Arc<Mutex<Vec<Declaration>>>\|(directory, declaration order)\|diffs staging hashes\|assert exact `Vec<Declaration>`' docs/architecture.md
   0
   $ grep -c 'item: Declared\|resolved and validated once at Load\|Arc<Mutex<StagedFile>>\|(directory, declared name, then the whole declaration)\|compares the two sorted staging lists\|assert exact `StagedFile`s' docs/architecture.md
   6
   $ grep -c '^```mermaid' docs/architecture.md
   $ git show HEAD:docs/architecture.md | grep -c '^```mermaid'
   $ diff <(git show HEAD:docs/architecture.md | grep '^#\{2,4\} ') <(grep '^#\{2,4\} ' docs/architecture.md)
   ```

   Expected: the two Mermaid counts are equal; the `diff` prints nothing.

8. Commit `docs(repo): redraw Declaration and the declare boundary in the architecture`.

---

## Task 6 — Amend `architecture-building-blocks.md` §1.1, §5.2, §5.3 and §8

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. §1.1, `architecture-building-blocks.md:25`. The flows through a port now run in the integration
   tests, so "unit tests" is false. Current:

   ```markdown
   The consequence the structure exists for: **every flow of the pipeline runs in `buildl-core`'s unit tests against fake ports** — no Lua state, no filesystem, no process spawn, no thread (§8).
   ```

   Replace with:

   ```markdown
   The consequence the structure exists for: **every flow of the pipeline runs in `buildl-core`'s tests against fake ports** — no Lua state, no filesystem, no process spawn, no thread (§8).
   ```

2. §5.2, `architecture-building-blocks.md:155-159`, current:

   ```rust
   /// Evaluates one build file. The directory queue, `subdir` handling, and the
   /// sorted merge across files are Load logic in `buildl-core`, not the adapter's.
   pub trait DeclarationSource {
       fn evaluate(&self, file: &BuildFile) -> Result<StagedFile>;
   }
   ```

   Replace with (spec §3.4, §4.2):

   ```rust
   /// Evaluates one build file. The directory queue, `subdir` handling, and the
   /// sorted merge across files are Load logic in `buildl-core`, not the adapter's.
   /// A directory with no build file is `Evaluated::Absent`; whether that is an
   /// error is Load's decision.
   pub trait DeclarationSource {
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
   }
   ```

3. §5.2's closing paragraph, `architecture-building-blocks.md:189-192`, current:

   ```markdown
   Every fallible port method returns the crate's one `Result`, not an error type of its own:
   `architecture.md` §4 settles the error model as a single structured enum, so `Result<StagedFile>`
   above is `core::result::Result<StagedFile, buildl_core::Error>`. The enum is `#[non_exhaustive]`,
   and each phase adds the variants it earns.
   ```

   Replace with:

   ```markdown
   Every fallible port method returns the crate's one `Result`, not an error type of its own:
   `architecture.md` §4 settles the error model as a single structured enum, so `Result<Evaluated>`
   above is `core::result::Result<Evaluated, buildl_core::Error>`. The enum is `#[non_exhaustive]`,
   and each phase adds the variants it earns.
   ```

4. §5.3, `architecture-building-blocks.md:197`, current:

   ```markdown
   Static dispatch throughout [7]. One trait of associated types carries every port, so `Pipeline` takes a single type parameter instead of one per port.
   ```

   Replace with (spec §5.1):

   ```markdown
   Static dispatch throughout [7]. One trait of associated types carries every port, so `Pipeline` takes a single type parameter instead of one per port. The bundle is declared in `buildl-core`'s `ports` module, beside the traits it names, so every trait of the crate lives in one module; a composition root implements it.
   ```

5. §8, `architecture-building-blocks.md:305`, current:

   ```markdown
   The fakes live in `buildl-core` under `#[cfg(test)]`:
   ```

   Replace with (spec §5.3). The `Clock` sentence is needed because `ports/clock.rs` does keep a
   `#[cfg(test)]` fake:

   ```markdown
   The fakes live in `buildl-core`'s integration tests (`crates/buildl-core/tests/flows/common.rs`), not under `#[cfg(test)]`. They implement the public traits through the public API alone, so whatever a fake does an adapter crate can do too, and the compiler checks that on every test build. A port no flow reaches yet keeps a unit-test fake beside its trait, as `Clock` does:
   ```

6. §8's last scenario row, `architecture-building-blocks.md:324`, current:

   ```markdown
   |a build file enqueues subdirectories out of order|merged declarations sorted by directory, then declaration order|
   ```

   Replace with (spec §4.4):

   ```markdown
   |a build file enqueues subdirectories out of order|merged declarations sorted by directory, then declared name|
   ```

7. Verify:

   ```
   $ grep -c "unit tests against fake ports\|Result<StagedFile>\|under \`#\[cfg(test)\]\`:\|then declaration order" docs/architecture-building-blocks.md
   0
   $ grep -c "Result<Evaluated>\|declared in \`buildl-core\`'s \`ports\` module\|tests/flows/common.rs\|then declared name|" docs/architecture-building-blocks.md
   5
   $ diff <(git show HEAD:docs/architecture-building-blocks.md | grep '^#\{2,4\} ') <(grep '^#\{2,4\} ' docs/architecture-building-blocks.md)
   ```

   Expected: `Result<Evaluated>` matches twice (signature and paragraph), the other three once each;
   the `diff` prints nothing.

8. Commit `docs(repo): place Ports in ports and the fakes in integration tests`.

---

## Task 7 — Correct the repository README's status line and point `CLAUDE.md` at the roadmap

**Files:**
- Modify `README.md`
- Modify `CLAUDE.md`

**Steps:**

1. `README.md:7`, current:

   ```markdown
   > **Status: design settled, implementation under way.** `buildl-core` ships the validated name/identity/time vocabulary, the one error enum, the canonical JSON serializer and the `Clock` port, all unit-tested; no pipeline phase is implemented yet. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ```

   Replace with (only the second sentence changes):

   ```markdown
   > **Status: design settled, implementation under way.** `buildl-core` ships the validated domain vocabulary, the one error enum, the canonical JSON serializer, the `Clock` and `DeclarationSource` ports, and Load — the first pipeline phase, with `check` — all tested against in-memory fakes; Resolve, Plan, Execute and Record are not implemented yet, and no build file can be evaluated until the Lua adapter lands. The design and internal architecture are specified in [`design.md`](./docs/design.md) and [`architecture.md`](./docs/architecture.md). The one open correctness question — dynamic input discovery — is resolved: the mechanism is the discovery target ([design §10](./docs/design.md)), with fine-grained per-file C/C++ compilation out of scope for v1.
   ```

2. `CLAUDE.md:5-7`. Its status section restates implementation status that `docs/roadmap.md`
   already owns, so it goes stale with every slice; it is replaced by a pointer, which never does.
   Current:

   ```markdown
   ## Status: scaffold in place, implementation not started

   buildl is a sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime. The repository holds the design documents plus a workspace scaffold: all four crates — `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — exist as doc-comment stubs with their dependency edges wired per `architecture-building-blocks.md` §7 (airsl `0.1`, only in `buildl-lua`; toolchain `stable`, `rust-version` 1.94), and the `cargo make dod` gate, `cargo deny` check and CI matrix are live. No pipeline module is implemented.
   ```

   Replace with:

   ```markdown
   ## Status

   buildl is a sandboxed, deterministic build system whose build files are written in Lua and evaluated on the [airsl](https://github.com/airsstack/airsl) embedded runtime. The workspace holds four crates — `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli` — with their dependency edges wired per `architecture-building-blocks.md` §7 (airsl `0.1`, only in `buildl-lua`; toolchain `stable`, `rust-version` 1.94), and the `cargo make dod` gate, `cargo deny` check and CI matrix are live. What is implemented, in progress and next is tracked in `docs/roadmap.md` §2–§3; read it there rather than inferring a state from this file.
   ```

   The paragraph that follows (the resolved pre-development blocker) stays as it is.

3. Verify:

   ```
   $ grep -c 'no pipeline phase is implemented yet' README.md
   0
   $ grep -c 'and Load — the first pipeline phase, with `check`' README.md
   1
   $ grep -c 'No pipeline module is implemented' CLAUDE.md
   0
   $ grep -c 'tracked in `docs/roadmap.md` §2–§3' CLAUDE.md
   1
   ```

4. Commit `docs(repo): name Load as implemented and point CLAUDE.md's status at the roadmap`.

---

## Task 8 — Close the slice in the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. §3's I3 row, `roadmap.md:68`, current:

   ```markdown
   |**I3 Load**|`buildl-core`|`Declaration`, `BuildFile`, `StagedFile`, `DeclarationSource`; the `Pipeline` skeleton and `FakePorts`|a fake source's output becomes `Vec<Declaration>`|**in progress** — intent and spec approved 2026-10-06, `.claudestacks/sdlc/2026-10-06-core-load/`|
   ```

   Only the Status cell changes, to the completion form the I1, I2 and I-guard rows use. Print the
   row with today's commit date filled in and paste it over line 68 verbatim:

   ```
   $ printf '|**I3 Load**|`buildl-core`|`Declaration`, `BuildFile`, `StagedFile`, `DeclarationSource`; the `Pipeline` skeleton and `FakePorts`|a fake source'"'"'s output becomes `Vec<Declaration>`|**done**, %s (§4)|\n' "$(git log -1 --format=%cs)"
   ```

2. §4, after the I2 row (`roadmap.md:89`). Following the I2 precedent, the range runs from this
   chain's first implementation commit (the first commit touching `crates/` after the spec commit
   `f754c87`, which excludes the plan-set commit) to `HEAD`, which is task 7's commit; this task's
   own commit lands outside the range, as I2's and I-guard's closures did. The task counts are read
   from the plan files. Print the row and insert it as the new line 90 verbatim:

   ```
   $ first=$(git log --reverse --format=%h f754c87..HEAD -- crates/ | head -1)
   $ last=$(git log -1 --format=%h)
   $ plans=$(for f in .claudestacks/sdlc/2026-10-06-core-load/plans/0*.md; do n=$(grep -c '^## Task ' "$f"); [ "$n" = 1 ] && w=task || w=tasks; printf '`%s` (%s %s), ' "$(basename "$f" .md)" "$n" "$w"; done)
   $ printf '|I3 Load|`.claudestacks/sdlc/2026-10-06-core-load/`|%s|`%s` … `%s`|\n' "${plans%, }" "$first" "$last"
   ```

   Expected: eight plans listed, `01-value-types` through `08-documentation` (this plan: 8 tasks).

3. §5, after follow-up 11 (`roadmap.md:106`), add follow-up 12 — the next free identifier; existing
   numbers are never reused (spec §6.2, §9):

   ```markdown
   |12|Add a per-file staging cap to the `buildl-lua` adapter. It stages declarations in a Rust-side buffer that the Lua memory ceiling (`[declaration] memory`, design §4) does not count, and the instruction ceiling bounds a file's calls but not, verifiably, how many declarations they stage, since airsl's per-call cost is not measured. Declaration is meant to be safe on untrusted input (design §2), so the cap is refused in airsl's refusal style; `buildl-core` already carries that failure as `EvaluationFailure::LimitReached { limit: EvaluationLimit::Staging }`.|I3 spec|I-lua|
   ```

4. Verify:

   ```
   $ grep -c 'intent and spec approved 2026-10-06' docs/roadmap.md
   0
   $ grep -c '^|\*\*I3 Load\*\*|.*|\*\*done\*\*, \|^|I3 Load|`.claudestacks/sdlc/2026-10-06-core-load/`|\|^|12|Add a per-file staging cap' docs/roadmap.md
   3
   $ grep -n '`<\|<first>\|<last>' docs/roadmap.md
   $ diff <(git show HEAD:docs/roadmap.md | grep '^#\{2,4\} ') <(grep '^#\{2,4\} ' docs/roadmap.md)
   ```

   Expected: the third and fourth commands print nothing — no placeholder shipped, no heading moved.

5. Run the full gate over the chain's work:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ cargo deny check
   $ cargo make guard-crate-edges
   $ cargo make guard-core-purity
   ```

6. Commit `docs(repo): close the Load slice in the roadmap`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all --check
$ cargo make dod
$ cargo deny check
$ cargo make guard-crate-edges
$ cargo make guard-core-purity
$ for f in docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md; do diff <(git show f754c87:"$f" | grep '^#\{2,4\} ') <(grep '^#\{2,4\} ' "$f") || echo "$f headings moved"; done
$ for f in docs/design.md docs/architecture.md docs/architecture-building-blocks.md docs/roadmap.md; do [ "$(git show f754c87:"$f" | grep -c '^```mermaid')" = "$(grep -c '^```mermaid' "$f")" ] || echo "$f mermaid count changed"; done
$ grep -rn 'I3\|spec §\|plan 0' crates/buildl-core/src crates/buildl-core/README.md
```

The first five exit 0. The two loops print nothing: every heading and every Mermaid block in
`docs/` survives the plan. The last grep prints nothing: rustdoc and the crate README carry no
planning reference. Every row of spec §9's table is covered — lib.rs crate doc and README (tasks 1,
3), crate rule 5 (task 1), `design.md` §14 and §12.1 (task 4), `architecture.md` §2, §3.1, §5 rule 5
and §7 (task 5), `architecture-building-blocks.md` §5.2, §5.3 and both §8 rows (task 6), and the
roadmap (task 8), and the repository README (task 7) — plus `architecture-building-blocks.md` §1.1, which the table omits (task 6).

## Review findings

- doc accuracy 🟡 — `EntryName` and `FieldName` listed as "the validated values a declaration's fields hold"; neither is — `crates/buildl-core/src/types/mod.rs:17`. Fixed: own bullet for each. Verified: `cargo make dod` → lib `137 passed`, flows `13 passed`, doctests `2 passed`, "Build Done in 9.74 seconds."
- README consistency 🔵 — "all tested against in-memory fakes" also covered the vocabulary, error enum and serializer — `README.md:7`. Fixed: "whose flows run against in-memory fakes".
- doc accuracy 🔵 — "a different order on each evaluation" overclaimed — `docs/architecture.md` §3.1. Fixed: "may issue".
- doc accuracy 🔵 — fakes-table row "static declarations per file" predated the staged port — `docs/architecture-building-blocks.md` §8. Fixed: "a fixed staged file, absence or evaluation failure per directory".
- diagram fidelity 🔵 — §2 diagram adds `Target --> Label` but not the `Rule`/`Alias` label edges — `docs/architecture.md` §2. No change: the diagram follows the plan text.
- reversion guard 🔵 — prose amendments have no gate beyond rustdoc's intra-doc links. No change possible.
- spec drift 🔵 — spec §9 row 1 asks the crate README to describe `ports` as holding every trait; only `lib.rs` does (plan Task 3 text omits it). No change.
- spec amendment 🔵 — CLAUDE.md pointer and abb §1.1 go beyond spec §9's table; the plan grounds both. No change.
- amendment hygiene 🟡 — Task 8's quoted I3 Status cell did not match the file; recorded below under Deviations.

## Probe results

- Claim: `Timestamp::duration_since`, `Label::resolve`, `Written<Directory>::under`, `EvaluationFailure::{UnknownField, WrongFieldType}`, `EvaluationLimit::Staging` and the test `FixedClock` exist. `grep -rnE "fn duration_since|fn resolve|fn under|UnknownField|WrongFieldType|Staging|struct FixedClock" crates/buildl-core/src` → `types/timestamp.rs:50`, `types/label.rs:76`, `types/written.rs:65,80`, `error.rs:133,165,170`, `ports/clock.rs:31`. All present.

## Deviations

- 2026-10-08 — roadmap I3 Status cell read "**in progress** — intent, spec and 8 plans approved 2026-10-06, …", not the quoted "intent and spec approved". Only that cell changes; replaced with "**done**, 2026-10-08 (§4)" as the task intends.
- 2026-10-08 — Task 8's `$( )` and process-substitution commands are refused by the worktree guard; git queries ran separately and the §4 row was assembled by hand from the same values. Heading and Mermaid diffs ran against HEAD copies saved to scratch.
- 2026-10-08 — roadmap §4 I3 commit range ends at `4192376`, the documentation commit; the status-flip commit that follows it is bookkeeping.
- 2026-10-08 — four review fixes beyond the plan text (Review findings above).
