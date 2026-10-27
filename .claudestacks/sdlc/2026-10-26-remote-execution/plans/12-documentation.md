---
status: done
created: 2026-10-26
depends-on: [01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11]
built-on: 8b1ed74430a0
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

## Review findings

One review over plans 11 and 12 (`21816dd..fa82cb2`); `cargo make dod` green. Plan 12's findings, all closed:
- docs (blocking) — `README.md` said `--no-remote` beside any switch flag is a usage error; `--no-remote --no-remote-execute` is accepted (`RemoteSwitches::resolve`) — fixed: it names the four flags refused
- docs (blocking) — design §11.2's retries paragraph left out the `UNKNOWN`/`INTERNAL` break, which is resumed with one retry and no reset (`execution.rs`, `Attempt::Broke`) — fixed, with the retried-`done` case
- docs — `README.md`'s doctor sentence said `execute` shows as the variable reads; it shows the manifest value and the variable — fixed
- docs — design's status headline still read "the core pipeline is implemented; the rest is design" — fixed: every item of §13 is built, some in part
- docs — architecture's testing paragraph made the CI job, not the test file, skip — fixed
- docs — `CLAUDE.md` left out that an answer from the server's cache ends `fetched` — fixed
- kept — the Risks line placing the toolchain sentence in §11.2: the sentence is in §9.6 (`docs/design.md:650`) and stays as it is

Proof: `grep -n "Remote execution is not built" docs/design.md` → nothing; `grep -n "toolchain container" docs/design.md` → nothing; `grep -n "Remote execution (item 8) is designed for and implemented by nothing here" docs/architecture.md` → nothing; `grep -n "network fan-out of remote execution, which this record said would reopen the question, is not built" docs/architecture.md` → nothing; headings of design, architecture, building blocks and README unchanged against `HEAD` before the plan; `crates/buildl/proto/README.md` already named `google/rpc/error_details.proto`. Gates: `cargo make dod` → Build Done.

## Probe results

- every doc claim the review checked against the code held after the fixes: the switch precedence, the usage errors, the delay schedule, the gates, `Wiring::dispatcher`/`ThreadPool::wide`, the `[remote]` key ranges, the bare refusal and its absence from `check`/`plan`/`doctor`, and the doctor layout (`remote_lines`)

## Deviations

- design §13 item 8 says "Toolchains are not shipped in containers" instead of naming toolchain containers, so the plan's grep holds and the non-goal is still stated
- the status line's "What else §13 lists is described here and still to come" was removed, since item 8 was the last item
- architecture §4's `OutcomeClass` list, the exit-0 row, the `RemoteSwitch`/`RemoteFlags` rows, §3.5's `Made` list, the summary-line sentence and the testing paragraph were brought up to date as well
- `cargo make dod` was not re-run for the two document-only commits (no source changed); it ran green on the fix commit
