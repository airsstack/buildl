---
status: done
created: 2026-10-28
depends-on: [01]
built-on: 136d5b098d7e
---

# Under the Hood Implementation Plan

**Goal:** Chapter 11 of the book carries buildl's design and architecture, rewritten from
`docs/`, for contributors and the curious.

**Architecture:** This plan fills stubs that plan 01 created and changes no other file. It
follows the writing rules every chapter plan shares:
- **Voice:** plain words, in the voice of airsl's book. `/Users/hiraq/Projects/airsstack/airsl/book/src/ch09-00-under-the-hood.md` and its three sections are the model.
- **Real output:** shell output, where shown, is captured from the debug binary at the branch head.
- **Code wins:** where `docs/` disagrees with the code, the book follows the code, cited by `file:line`.
- **Diagrams.** This plan owns the `docs/` Mermaid diagrams listed under its `## Proof`; each is carried into its chapter as a ` ```mermaid ` block, redrawn where the binary has moved on.
- **A command that disagrees with its `--help`** is raised to the user, not written around.
- **Links:** relative `.md` links, and no `§` sign anywhere in the book.

Two pages are cited by name from code comments after plan 08, so their numbering is fixed:
- `ch11-03-four-crates.md` keeps the dependency rules numbered exactly as `docs/architecture-building-blocks.md:551-565` numbers them, and keeps the derivation and per-family counts of `:27-49`.
- `ch11-05-determinism-rules.md` keeps the rules numbered exactly as `docs/architecture.md:631-656` numbers them.

**Tech Stack:** Markdown, mdBook 0.4.52.

---

## Files that change

```
book/src/ch11-00-under-the-hood.md       — [modify] opener: for contributors; not needed to use buildl
book/src/ch11-01-declare-then-execute.md — [modify] the pipeline, core types, phases, scheduler, parallel load
book/src/ch11-02-crash-safety.md         — [modify] one action's lifecycle; cas → row → log; capture
book/src/ch11-03-four-crates.md          — [modify] crates, inversion, core's allowed deps, C4, ports, rules, publishing
book/src/ch11-04-errors.md               — [modify] error architecture
book/src/ch11-05-determinism-rules.md    — [modify] the numbered rules, declaration determinism, ledger
book/src/ch11-06-decision-records.md     — [modify] every decision record of both documents
book/src/ch11-07-testing-layers.md       — [modify] the testing layers and where each lives
book/src/ch11-08-where-buildl-sits.md    — [modify] the theory walkthrough
book/src/ch11-09-what-it-promises.md     — [modify] scope note; what each tier does not guarantee
```

## Order of work

### Task 1 — The pipeline and crash safety

**Files:** Modify `book/src/ch11-00-under-the-hood.md`, `book/src/ch11-01-declare-then-execute.md`, `book/src/ch11-02-crash-safety.md`.

**Change:**
- **Opener:** says the chapter is for the curious and for contributors, and lists its sections.
- **Declare, Then Execute:**
  - Load → Resolve → Plan → Execute → Record, with the value each phase hands to the next (`Vec<Declaration>`, `TargetGraph`, `Plan`, `Vec<ActionOutcome>`).
  - Each CLI command is the pipeline cut off at one of those handoffs.
  - The core types and the target state machine.
  - The scheduler loop.
  - Parallel loading, for big monorepos.
- **Crash Safety and the Store:**
  - one action's lifecycle;
  - the invariant: cas first, cache row after, log always; failures are never cached;
  - output capture and failure semantics.

Sources:
- `docs/design.md`: `:33-59` (§2), `:391-451` (§8.1–8.4), `:452-477` and `:503-550` (§8.5, §8.7), `:846-854` (§11, §11.1).
- `docs/architecture.md`: `:44-358` (§2, §2.1, §3, §3.1–3.5) and `:359-398` (§3.6, the second load as a pipeline step).

**Proof:**
- Each type name the pages use is found in `crates/buildl-core/src` (`grep -rn "pub struct <Name>\|pub enum <Name>"`).
- The task's report lists the grep hit for each name.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book chapter 11, the pipeline and crash safety`.

### Task 2 — Four crates

**Files:** Modify `book/src/ch11-03-four-crates.md`.

**Change:**
- the four crates, with their dependency graph;
- dependency inversion;
- what `buildl-core` may depend on, with its derivation and per-family counts. `crates/buildl-core/clippy.toml:4,21` cites this content;
- the name `buildl`;
- the C4 views (context, containers, components, code: domain data, ports, `Pipeline` and `Ports`, pure logic);
- the port catalog;
- the dependency rules, numbered as today;
- the publishing order.

Sources: `docs/architecture-building-blocks.md:9-565` (§1–§7) and `:698-732` (§9), and `docs/architecture.md:9-43` (§1, §1.1). The C4 Mermaid diagrams carry over.

**Proof:**
- The dependency rules on the page are the same in number and order as `docs/architecture-building-blocks.md:551-565`.
- Every port the catalog names is a trait in `crates/buildl-core/src`. The task's report lists each grep hit.
- The per-family counts still hold against `crates/buildl-core/clippy.toml`.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book chapter 11, four crates, ports and adapters`.

### Task 3 — Errors, determinism, decisions, testing

**Files:** Modify `book/src/ch11-04-errors.md`, `book/src/ch11-05-determinism-rules.md`, `book/src/ch11-06-decision-records.md`, `book/src/ch11-07-testing-layers.md`.

**Change:**

| Page | What it holds | Sources |
|---|---|---|
| Errors | the error architecture | `docs/architecture.md:557-630` (§4) |
| Determinism Rules | the engineering rules, numbered as today; declaration determinism; guarantee, mitigate, verify; the explicitness ledger | `docs/architecture.md:631-656` (§5); `docs/design.md:938-946` (§12.1), `:979-1000` (§12.3) and `:1001-1020` (§12.4) |
| Decision Records | every decision record of both documents, each with its decision and reason, and the Lua/airsl choice from design §1.1 | `docs/architecture.md:657-717` (§6); `docs/architecture-building-blocks.md:733-756` (§10) |
| Testing Layers | which layer is tested where, and which crate holds it | `docs/architecture-building-blocks.md:566-697` (§8); `docs/architecture.md:718-730` (§7) |

**Proof:**
- The determinism rules match `docs/architecture.md:631-656` in number and order.
- The decision records page holds as many records as the two source sections combined. The task's report states the count.
- `mdbook build book` exits 0.

**Done when:** all three hold. Commit `docs(repo): book chapter 11, errors, determinism, decisions and tests`.

### Task 4 — Where buildl sits, and what it promises

**Files:** Modify `book/src/ch11-08-where-buildl-sits.md`, `book/src/ch11-09-what-it-promises.md`.

**Change:**

Where buildl Sits covers:
- the à la Carte taxonomy;
- the scheduler and the rebuilder;
- buildl as a build system first, with task-runner use as a degenerate case (design §13's second scope decision);
- the capability gradient;
- the summary card.

What buildl Can and Cannot Promise covers:
- no per-file C/C++ compilation in v1, and discovery targets instead of depfiles (design §13's first scope decision);
- what each isolation tier does not guarantee;
- the explicitness ledger's limits.

Sources: `docs/fundamental-walkthrough.md` `:7-62` (§1, §2), `:157-208` (§6, the depfile gap, for What It Promises) and `:209-289` (§7–§9); `docs/design.md` `:1021-1037` (§13) and `:1001-1020` (§12.4). The diagrams carry over.

**Proof:** `mdbook build book` exits 0.

**Done when:** it holds. Commit `docs(repo): book chapter 11, where buildl sits and what it promises`.

## Risks

- Parts of `docs/architecture.md` describe types or modules that later milestones renamed. Each name the book uses is checked by grep (Task 1, Task 2). A name that is gone is rewritten to what the code holds now.
- This plan carries the largest volume of rewritten text. Its tasks are cut by page so that each commit stays readable.

## Proof

- `cargo make book-build` exits 0.
- The 19 `docs/` Mermaid diagrams this plan owns each appear as a ` ```mermaid ` block in the chapter named, and each such page renders `<pre class="mermaid">` in `book/book/`; open each page once in a browser (`cargo make book`) and confirm the diagram draws, not an error box: `docs/architecture.md:48`, `:188`, `docs/design.md:283`, `:397`, `:440` → `ch11-01`; `docs/design.md:454` → `ch11-02`; `docs/architecture.md:29`, `docs/architecture-building-blocks.md:15`, `:62`, `:84`, `:104`, `:455`, `:702` → `ch11-03`; `docs/architecture-building-blocks.md:568` → `ch11-07`; `docs/fundamental-walkthrough.md:11`, `:39`, `:223`, `:246`, `:263` → `ch11-08`.
- The spec §3 rows this plan lands, marked in the execution record:

  | Source | Sections | Destination |
  |---|---|---|
  | `docs/architecture-building-blocks.md` | §1–§7 and §9 | `ch11-03` |
  | | §8 | `ch11-07` |
  | | §10 | `ch11-06` |
  | `docs/architecture.md` | §1 | `ch11-03` |
  | | §2, §3.1–§3.4 and §3.6 | `ch11-01` |
  | | §3.5 | `ch11-02` |
  | | §4 | `ch11-04` |
  | | §5 | `ch11-05` |
  | | §6 | `ch11-06` |
  | | §7 | `ch11-07` |
  | `docs/design.md` | §1.1 (the decision) | `ch11-06` |
  | | §2, §8.1–§8.4, §11 and §11.1 | `ch11-01` |
  | | §8.5 and §8.7 | `ch11-02` |
  | | §12.1 and §12.3 | `ch11-05` |
  | | §12.4 | `ch11-05`, `ch11-09` |
  | | §13's scope decisions | `ch11-08`, `ch11-09` |
  | `docs/fundamental-walkthrough.md` | §1, §2 and §7–§9 | `ch11-08` |
  | | §6 (the depfile gap; its ch07 half is plan 04) | `ch11-09` |

## Review findings

- wrong fact — `ch11-04:151` said `PortName` has one variant per port; `error.rs:798-833` has none
  for `DeclarationSource`, `Clock`, `Reporter`, `Cycles` or `WatchReport`, and adds `PluginStore`,
  which is not a port. Now "one for each port whose adapter can fail, plus `PluginStore`". Fixed.
- nit — `ch11-01:232` gave 136 µs to make an engine; 128 µs is the make cost (136 µs is make plus
  one evaluation, kept at `:257`), matching `ch11-06`. Fixed.
- nit — `ch11-03:374` said the narrow commands are free functions "instead of" `Pipeline` methods;
  `Pipeline` also has `check`, `graph`, `query` (`pipeline/driver.rs:114,137,155`). Now "beside".
  Fixed.
- nit — `ch11-04:156` led with the `UnreadableJson`/`NonCanonicalJson` pair; now leads with
  `Adapter` vs `CorruptState`. Fixed.
- nit — `ch11-06` row labels "(building blocks)" / "(architecture)" named source files the reader
  never sees; tags dropped, the rows' own titles keep them apart. Fixed.
- nit — `ch11-08:127` quoted `1 failed, 1 skipped, 1 built`, which lacks *cached*; now quotes
  `1 failed, 2 skipped, 2 built, 9 cached` from `ch11-01`. Fixed.

## Probe results

- Names: every type, port, error variant and module the pages give was grepped against `crates/`
  (198 names on ch11-01..03; the ch11-04..09 set incl. `OutcomeClass`, `InfraKind`, `PortName`,
  `Exit::of_*`, `Fake*` in `tests/flows/common.rs`); only std and airsl names lack a hit. The
  reviewer re-checked crate edges (`crates/expected-edges.txt`), the 22 port traits and `Ports`
  (`ports/bundle.rs:272-307`), exit codes (`exit.rs:23-155`) and the cas → row → log order
  (`execute/record.rs:241`, `assembly.rs:561,629`).
- Numbered rules: `ch11-03` keeps dependency rules 1–5 and `ch11-05` determinism rules 1–5 in the
  old order; every "rule N" cited from `Cargo.toml`, `deny.toml`, `Makefile.toml` and
  `crates/buildl-core/clippy.toml` matches its page (reviewer).
- `clippy.toml` holds 130 bans; the family counts on `ch11-03` sum to it.
- `ch11-04`'s transcript (head debug binary): `build --keep-going` → `1 failed, 1 skipped, 1 built`
  (exit 1), a ceiling refusal (exit 4), an unknown target (exit 2).
- Diagrams: ch11-01 ×5, ch11-02 ×1, ch11-03 ×7, ch11-07 ×1, ch11-08 ×5 (19) render
  `<pre class="mermaid">`; opened in Chrome from a local build, each is an SVG with no error box.
- Decision records: 56 from architecture §6 + 19 from building blocks §10 + the Lua/airsl record
  = 76 rows.
- Spec §3 rows landed: building blocks §1–§7, §9 → ch11-03, §8 → ch11-07, §10 → ch11-06;
  architecture §1 → ch11-03, §2, §3.1–§3.4, §3.6 → ch11-01, §3.5 → ch11-02, §4 → ch11-04,
  §5 → ch11-05, §6 → ch11-06, §7 → ch11-07; design §1.1 → ch11-06, §2, §8.1–§8.4, §11, §11.1 →
  ch11-01, §8.5, §8.7 → ch11-02, §12.1, §12.3 → ch11-05, §12.4 → ch11-05, ch11-09, §13 scope →
  ch11-08, ch11-09; walkthrough §1, §2, §7–§9 → ch11-08, §6 → ch11-09.

## Deviations

- 2026-10-28 — run as two parallel writers (tasks 1–2, tasks 3–4); file sets were disjoint.
- 2026-10-28 — diagrams redrawn where the code moved on: the executor loop (design:440) with the
  settle step and cas, row, log, counters; the action lifecycle (design:454) with its own
  "place into out/" step; abb:15 as logic, ports, adapters and root (it was a copy of arch:29);
  C4 level 1 names "Remote cache and executor"; level 4 adds `build_watched()` and `test()`; the
  summary mindmap moves discovery, remote and parallel load to covered.
- 2026-10-28 — rewritten to the code: `ActionOutcome.role`, `Error::Refused`/`DependencyCycle`/
  `DuplicateLabel`, `FakeClock`, `Dispatcher::dispatch(work, step)` with `Turn`, the ledger's
  network row (no `b.fetch`), the sandbox's pinned modification times.
- 2026-10-28 — "Nothing is published yet" and the crates.io name-claim date were dropped; both
  go stale at the first publish.
- 2026-10-28 — `ch11-09` links `ch07`'s depfile-gap section, which already holds those diagrams,
  rather than repeating them.
