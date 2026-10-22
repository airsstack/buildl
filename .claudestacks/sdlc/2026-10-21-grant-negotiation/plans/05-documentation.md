---
status: done
created: 2026-10-22
depends-on: [01, 02, 03, 04]
built-on: 5d10ef9b2f2e
---

# Documentation Implementation Plan

**Goal:** The design, both architecture documents, the roadmap, the READMEs and `CLAUDE.md` describe grant negotiation, built programs and `buildl doctor` as built.

**Architecture:** Text only. Mermaid stays Mermaid in `docs/` (CLAUDE.md); section numbers and
cross-references (`§12.2`, `§13`, `§14`) keep their numbers. New design content cites into the
documents' numbered reference lists (CLAUDE.md Conventions).

**Tech Stack:** Markdown; `cargo make dod` (doc build) and `cargo deny check` before the commit.

**Content authority:** spec §2 (no `Grants` value), §7, §10; intent "Documents" and "Wording"
rows.

---

## Files that change

```
docs/design.md                                       — [modify] §4, §6, §7, §12.2, §13 item 4, §14
docs/architecture.md                                 — [modify] grants step, NotApproved, decision record
docs/architecture-building-blocks.md                 — [modify] Status line, §5, §6, Ports listing
.claudestacks/sdlc/roadmaps/buildl/04-grant-negotiation.md — [modify] "follows airsl's model"
README.md                                            — [modify] approver, built programs, doctor
CLAUDE.md                                            — [modify] Status paragraph
crates/buildl-core/README.md, crates/buildl/README.md, crates/buildl-cli/README.md — [modify] new parts
```

## Order of work

### Task 1 — The design document

**Files:** `docs/design.md`.

**Change** (lines from a read at `8707b21`):
- §4 (`:90`): the `[approver]` table, `default = "manifest" | "interactive" | "deny"`, absent
  means `manifest`; `proc.run` names host programs, and a program a dependency built needs no
  entry. The sentence (`:120`) saying an `[approver]` table is an unknown key until grant
  negotiation ships goes. The design's `buildl.toml` example and the `DESIGN_EXAMPLE` test
  constant in `crates/buildl/src/manifest/toml_manifest.rs` (which holds the `[approver]` table
  since plan 03) stay the same text (amended 2026-10-22, execution of plan 03).
- §6 (`:176`): the negotiation as built — wanted set → ceiling → approver → run; the prompt
  text of spec §4; the approvals file `<data>/buildl/approved/<workspace id>.json` outside the
  workspace and why; no terminal refuses; `deny` refuses even a cached build. The diagram ends
  at the approver's answer (spec §2: no `Grants` value); keep it Mermaid.
- §7 (`:190`): the `doctor` row; one sentence that a command may run what a dependency built.
- §12.2 (`:665`): the tier-1 paragraph says a `$deps`-named (built) program runs without a
  ceiling entry, and its bytes enter the key as its dependency's output digest.
- §13 item 4 (`:749`): "follows airsl's model (intersect, never union; the approver only
  narrows; fail closed)" instead of "consumes the airsl extension host's ceiling + `Approver`";
  marked done as the other built items are.
- §14 (`:755`, question `:759`): per-target narrowing settled as declined, with spec §7's
  reason.

**Proof:** `grep -n "consumes the airsl" docs/design.md` finds nothing; `grep -n "approver"
docs/design.md` finds §4, §6, §7 and §13.

**Done when:** read through once for cross-references; `cargo make dod` green. Commit
`docs(repo): grant negotiation and doctor in the design`.

### Task 2 — Architecture documents and roadmap

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`,
`.claudestacks/sdlc/roadmaps/buildl/04-grant-negotiation.md`.

**Change:**
- `architecture.md`: Plan's section says plan places outputs and expands every command, and
  the built rule; a `negotiate` step between Plan and Execute in `build`/`test`/`verify`; the
  error section gains `NotApproved` (exit 4) and `PortName::Approver`; §6 gains a decision
  record "approvals are remembered outside the workspace" (a repository can ship its own
  `.buildl/`; direnv's `$XDG_DATA_HOME` precedent, chain `research-built-tools.md`).
- `architecture-building-blocks.md`: Status line `:3` and `:156` say the approver port and the
  grants logic are built and that there is no `Grants` value; `:110-111` boxes ("later:
  Approver", "grants") become built; `:177` Authority row drops `Grants`, adds `Approval`,
  `DenialReason`, `ApproverMode`, `Checkup`; `:365`, `:431` grants row built; `:459` `Approver`
  catalog row built; the `Ports` listing (`:350`) gains `type Approver`; `LocalPorts` (`:376`)
  gains `C`; the modules list gains `placement` and `grants` in the core, `approver` in
  `buildl`.
- Roadmap `04-grant-negotiation.md:10-13`: "It follows airsl's model (intersect, never union;
  the approver only narrows; fail closed) rather than consuming airsl's types." Its status is
  left to the status board.

**Proof:** `grep -n "Grants" docs/architecture-building-blocks.md` finds no line that lists it
as a type; `grep -n "consumes" .claudestacks/sdlc/roadmaps/buildl/04-grant-negotiation.md` finds
nothing.

**Done when:** `cargo make dod` green. Commit `docs(repo): grant negotiation in the
architecture documents`.

### Task 3 — READMEs and `CLAUDE.md`

**Files:** `README.md`, `CLAUDE.md`, `crates/buildl-core/README.md`, `crates/buildl/README.md`,
`crates/buildl-cli/README.md`.

**Change:**
- `README.md`: the `[ceiling]` example (`:70-74`) loses any `out/` path and gains an
  `[approver]` line; `:128` `buildl doctor` row as built; `:189` grant negotiation item done;
  one line on built programs.
- `CLAUDE.md` Status: Milestone 4 is done — the approver modes, built programs needing no
  ceiling entry, and `buildl doctor`.
- Crate READMEs: `buildl-core` (`:24-29`) names `grants`, the `Approver` port and
  `pipeline::doctor`; `buildl` (`:26`) names the `approver` module and `Workspace::doctor`;
  `buildl-cli` (`:24`) exit 4 covers "not approved" and lists `doctor`.

**Proof:** `grep -rn "out/greet\|out/gen" README.md crates/*/README.md docs/` finds no ceiling
entry naming an output path.

**Done when:** `cargo make dod` and `cargo deny check` green. Commit `docs(repo): approver and
doctor in the READMEs`.

---

## Risks

- Line numbers drift as plans 01–04 land; each edit is located by its heading and text, not by
  number alone.
- Cross-references between the docs are load-bearing (CLAUDE.md); renumbering nothing is the
  rule.

## Proof

- Every spec §10 item is edited; the intent's "Documents" row holds.
- `cargo make dod` and `cargo deny check` green.

## Review findings

Commits: `3dcc368` (task 1), `50d4e7b` (task 2), `a0f0f91` (task 3), `9a05b49` (fix round).
One review over the three tasks: spec compliant; 5 blocking, 7 non-blocking — all fixed.

- false claim — design §8.7 policy-refusal row named `$deps` programs as refused — fixed: a
  program a dependency built is never refused; the row adds an approver's no.
- false claim — design §12.2 tier 1 and §10.2 said every written-out or setting-named program
  is fingerprinted and held to `proc.run` — fixed: unless a dependency built it.
- false claim — architecture §6 "Program named through a build setting" said only the build-time
  check knows `$in`/`$out`/`$deps` programs — fixed: Plan expands every command.
- false claim — "Thirteen" adapter modules (architecture §6, building blocks §10) — fixed:
  fourteen, with `approver`.
- false claim — the approvals decision record said a relative `XDG_DATA_HOME` makes the
  approver fail — fixed: it falls back to `$HOME/.local/share`, failing only when neither is
  absolute; the direnv sentence now says where, not why.
- incomplete — `buildl-cli` exit 6 doc and README said "a file in `.buildl/`"; design exit 3
  left out the approvals file; architecture `CorruptState` said "workspace's state"; building
  blocks said Plan shares `outside` and listed three non-phase modules; the CLI README said
  "with none, every target" for every command and `--set` for `plan` and `build` only — fixed.

## Probe results

- Claim: the design's `buildl.toml` example and `DESIGN_EXAMPLE` can be the same text — held:
  both now carry `[approver] default = "interactive"`.
- Claim: the prompt in design §6 is the as-built text — held: `WantedSet`'s `Display` pads
  labels to the longest plus one (`types/authority.rs`), shown with all four lines.

## Deviations

- 2026-10-22 — task 1: also corrected design §8.7 (policy refusals at plan), §9.6 (what
  `doctor` reports), §10.2 (`$in`/`$out`/`$deps` held at plan) and the exit-code rows 3, 4
  and 6, which plans 01–04 had made false.
- 2026-10-22 — task 2: also updated building blocks §10 rows on the reporter (`LocalPorts<R, C>`)
  and port wiring (`doctor`).
- 2026-10-22 — task 3: also corrected `crates/buildl/README.md`'s stale tier-1 status and listed
  every command in `crates/buildl-cli/README.md`.
