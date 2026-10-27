---
status: approved
created: 2026-10-26
depends-on: [01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11]
built-on: 75312efc6398
---

# Documentation Implementation Plan

**Goal:** Every document spec §11 names describes remote execution as built.

**Architecture:** Documents only, after the code is done, so they describe what exists. Section
numbers and the cross-references between the docs stay as they are (CLAUDE.md: they are
load-bearing); new diagrams in `docs/` are Mermaid.

**Tech Stack:** Markdown, Mermaid.

**Content authority:** spec §11 (the document table), and every section the table points at.

---

## Files that change

```
docs/design.md                        — [modify] §4, §7, §8.7, §11.2, §13 item 8
docs/architecture.md                  — [modify] status line (:3), §3.4, §3.8, §6 async record
docs/architecture-building-blocks.md  — [modify] status line (:3), the RemoteCache row of the port table (:541), the remote module row (:154)
crates/buildl/proto/README.md         — [modify] checked: plan 02 already names error_details.proto
README.md                             — [modify] milestone 8 built
CLAUDE.md                             — [modify] the Status paragraph: milestone 8 built
```

## Order of work

### Task 1 — The design document

**Files:** `docs/design.md`.

**Change:**
- §4: the `[remote]` keys table gains `execute`, `jobs`, `queue_timeout`, `retries`, `properties`
  (spec §3); the sentence "Whether a run reads from the cache, writes to it, or fails when it cannot
  be reached is the machine's and the run's to say, not the manifest's" (`design.md:191`) says that
  `execute` is the one switch the manifest may hold (spec §2).
- §7: the switches table (`:307-311`) gains execution's three sources and `--no-remote-execute`;
  the usage errors of spec §2; `--remote-jobs`; the `remote` word, block marker and summary of
  spec §9; `doctor`'s new lines.
- §8.7: the exit-code table's row 2 (`:490`) gains the remote execution usage errors.
- §11.2: "Remote execution is not built" (`design.md:844`) becomes the built client:
  the flow of spec §1 and §4, retries (§5), failure handling (§6); the toolchain-container
  paragraph is dropped (milestone 8 non-goal); the Mermaid sequence diagram is kept and matched to
  what is built.
- §13 item 8: "Remote execution" without toolchain containers. Built.

**Proof:** `grep -n "Remote execution is not built" docs/design.md` finds nothing;
`grep -n "toolchain container" docs/design.md` finds nothing in §11.2 or §13; every key of spec §3
appears in §4's table.

**Done when:** the greps hold and the section numbers are unchanged
(`grep -n "^## \|^### " docs/design.md` lists the same headings as before). Commit
`docs(repo): remote execution in the design document`.

### Task 2 — Architecture, building blocks, README, CLAUDE.md

**Files:** the rest.

**Change:** `architecture.md`: status (`:3`) says remote execution is built; §3.4 gains the two
gates and the wide dispatcher; §3.8 gains `execute`, `Executed`, the `execution` module and the
retry rule; §6's async record (`:598`) says the network fan-out is served by threads blocking on
the one runtime. `architecture-building-blocks.md`: status (`:3`), the `remote` module row
(`:154`) and the `RemoteCache` port row (`:541`). `README.md` and `CLAUDE.md`'s Status paragraph:
milestone 8 is built, as milestone 7's sentence reads.

**Proof:** `grep -n "Remote execution (item 8) is designed for and implemented by nothing here" docs/architecture.md`
finds nothing; `grep -n "network fan-out of remote execution, which this record said would reopen the question, is not built" docs/architecture.md` finds nothing.

**Done when:** the greps hold. Commit `docs(repo): remote execution built`.

## Risks

- §11.2's paragraph on hermetic toolchains (§9.6) still refers to "remote workers receive the
  toolchain as they receive any input"; it stays true and is left as it is.

## Proof

All greps above hold; `cargo make dod` still green (doctests unaffected).
