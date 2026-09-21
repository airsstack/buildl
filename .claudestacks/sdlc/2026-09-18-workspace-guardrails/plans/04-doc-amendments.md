---
status: approved
created: 2026-09-18
---

# Documentation Amendments Implementation Plan

**Goal:** Replace the three documentation sentences that credit an enforcement mechanism which
does not exist with the mechanisms this chain builds.

**Architecture:** Prose only. Three sentences across two design documents assert that the crate
boundary or the compiler enforces rules it does not enforce; each is rewritten to name the actual
guard and to say what the compiler does and does not cover. `docs/roadmap.md` then records the
chain as complete: §3's ladder row, a new §4 row, and the deletion of the three §5 follow-ups the
chain closes. No code, no configuration.

**Tech Stack:** Markdown.

---

## Context an implementer needs

Three claims in `docs/` are false today, and all three are about the rules this chain enforces:

| Location | Claim | Reality |
|---|---|---|
| `docs/architecture-building-blocks.md:268-271` (§7's "Enforced by" table) | rules 1, 4 and the cross-crate half of 2 are "enforced by the compiler" | The compiler stops a crate *importing* what its `[dependencies]` omits. It does not stop anyone *adding* to `[dependencies]`, nor notice one being removed. |
| `docs/architecture.md:216` (§5 rule 4) | "`buildl-core` cannot name the system clock at all, so the crate boundary enforces the rule rather than grep" | `std::time::SystemTime::now()` compiles inside `buildl-core`. The crate boundary constrains *other crates'* APIs; `std` is in scope for every crate. |
| `docs/architecture-building-blocks.md:259` | "enforced by the crate boundary: `buildl-core` has no way to read time except through the port" | The same claim, in the other document. |

These three are in scope because they describe the mechanisms this chain builds. The rest of the
documentation inaccuracies recorded in `docs/roadmap.md` §5 (rows 6, 7 and 8) belong to a separate
docs sweep and are **not** touched here.

**Line numbers shift as you edit.** Locate each passage by its text, not by line number:

```
$ grep -n 'Enforced by\|crate boundary' docs/architecture-building-blocks.md docs/architecture.md
```

Every section cross-reference in these documents ("design §8.5", "§12.4 ledger", "architecture.md
§5.4") is load-bearing — none of these edits renumbers a section, and every reference in the new
text points at a section that exists.

This plan is independent of plans 01–03 and can run in any order relative to them, but it reads
best committed last, once the mechanisms it describes are in the tree.

All work happens in the git worktree, on a branch, never on `main`. Commits follow Conventional
Commits: `type(scope): summary`, scope `repo`. One commit per task.

### File map

```
docs/architecture-building-blocks.md  — [modify] §6 Clock port sentence, §7 "Enforced by" table (task 1)
docs/architecture.md                  — [modify] §5 rule 4 (task 2)
docs/roadmap.md                       — [modify] §3 I-guard status, new §4 row, delete §5 rows 1, 2, 9 (task 3)
```

---

## Task 1 — Name the actual enforcers in `architecture-building-blocks.md`

**Files:**
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. Find the §6 ports-table follow-on sentence (`docs/architecture-building-blocks.md:259`). It
   currently reads, as one line:

   ```markdown
   The `Clock` port is `architecture.md` §5.4's quarantine of the wall clock, enforced by the crate boundary: `buildl-core` has no way to read time except through the port.
   ```

   Replace it with:

   ```markdown
   The `Clock` port is `architecture.md` §5.4's quarantine of the wall clock. The crate boundary does not enforce it — `std::time` is in scope for every crate — so `crates/buildl-core/clippy.toml` bans `Instant`, `SystemTime` and `SystemTimeError` by name, and `cargo make guard-core-purity` fails the gate when one is used, when a ban stops resolving, or when a source file suppresses the lint.
   ```

2. Find §7's "Enforced by" table (`docs/architecture-building-blocks.md:268-271`). It currently
   reads:

   ```markdown
   |Rule|Enforced by|
   |---|---|
   |1, 4, and the cross-crate half of 2|the compiler — a crate cannot import what its `[dependencies]` does not list|
   |adapter isolation inside `buildl`|review, until an adapter earns its own crate (§11)|
   ```

   Replace it with:

   ```markdown
   |Rule|Enforced by|
   |---|---|
   |the dependency half of 1, rule 4, and the cross-crate half of 2|`deny.toml`'s `[bans].deny` wrapper lists, which name the only direct parents a crate may have, plus `cargo make guard-crate-edges`, which diffs every member's direct dependencies against `crates/expected-edges.txt`. The compiler covers only imports: it stops a crate using what its `[dependencies]` omits, but not a dependency being added, and not one being removed.|
   |the "no I/O API" half of 1|`crates/buildl-core/clippy.toml`, an enumerated ban on the filesystem, process, thread, environment, standard-stream and clock APIs, asserted by `cargo make guard-core-purity`|
   |adapter isolation inside `buildl`|review, until an adapter earns its own crate (§11)|
   ```

3. Leave `docs/architecture-building-blocks.md:339` alone. That line — "A crate boundary also
   turns the dependency rules into compiler errors" — is a parenthetical inside a §10 decision
   record about *crate naming*, not a statement of the enforcement mechanism, and it is true of
   imports, which is what a crate boundary does govern. The spec (§8) lists it "for the plan to
   judge, not mandated"; this is the judgement, and it is recorded here so nobody re-opens it.

4. Confirm exactly the intended lines changed:

   ```
   $ git diff --stat docs/architecture-building-blocks.md
    docs/architecture-building-blocks.md | 5 +++--
   ```

   and that the false claims are gone, with a control proving the search works:

   ```
   $ grep -n 'enforced by the crate boundary' docs/architecture-building-blocks.md
   $ grep -c 'crate boundary' docs/architecture-building-blocks.md
   1
   ```

   The first search prints nothing; the second finds the surviving line 339, which is the control
   showing the pattern still matches something.

5. Commit `docs(repo): name the actual enforcers of the dependency and purity rules`.

---

## Task 2 — Correct the clock quarantine in `architecture.md`

**Files:**
- Modify `docs/architecture.md`

**Steps:**

1. Find §5 rule 4 (`docs/architecture.md:216`). It currently reads, as one line:

   ```markdown
   4. **Wall clock is quarantined.** `now()` is read in exactly two places — log event timestamps and durations — via the `Clock` port, whose only real adapter lives in `buildl`; `buildl-core` cannot name the system clock at all, so the crate boundary enforces the rule rather than grep.
   ```

   Replace it with:

   ```markdown
   4. **Wall clock is quarantined.** `now()` is read in exactly two places — log event timestamps and durations — via the `Clock` port, whose only real adapter lives in `buildl`. The crate boundary does not enforce this: `std::time` is in scope for every crate. `crates/buildl-core/clippy.toml` bans `Instant`, `SystemTime` and `SystemTimeError` under `disallowed-types`, and `cargo make guard-core-purity` runs that ban inside `cargo make dod` — so it is lint configuration, not grep and not the compiler, that keeps the rule true.
   ```

2. Confirm the change is one line and the false claim is gone:

   ```
   $ git diff --stat docs/architecture.md
    docs/architecture.md | 2 +-
   $ grep -n 'crate boundary enforces' docs/architecture.md
   ```

   The grep prints nothing.

3. Commit `docs(repo): correct the clock quarantine's enforcement mechanism`.

---

## Task 3 — Close the I-guard follow-ups in the roadmap

**Files:**
- Modify `docs/roadmap.md`

**Steps:**

1. In §3's intent-ladder table, change the **I-guard** row's Status cell from:

   ```markdown
   |**intent approved**, 2026-09-18; chain `2026-09-18-workspace-guardrails`|
   ```

   to:

   ```markdown
   |**done**, 2026-09-18 (§4)|
   ```

   matching the shape the **I1 workspace** row already uses.

2. In §4 "Completed intents", add a row beneath the existing `I1 workspace` row:

   The Commits column is a range. Get its two ends before writing the row — the first is plan
   01 task 1's commit, the last is the commit immediately before this one (this row cannot cite
   the commit that creates it):

   ```
   $ git log --oneline --reverse | grep -n 'ban the forbidden crate edges'
   $ git rev-parse --short HEAD
   ```

   The first command gives the opening SHA, the second the closing one. Write the row with those
   two values substituted, matching the `I1 workspace` row's format exactly:

   ```markdown
   |I-guard|`.claudestacks/sdlc/2026-09-18-workspace-guardrails/`|`01-crate-edge-guards` (4 tasks), `02-core-purity-bans` (3 tasks), `03-msrv-ci-job` (2 tasks), `04-doc-amendments` (3 tasks)|`0000000` … `0000000`|
   ```

   If plans 01–03 have not landed yet when this task runs, record that in `## Deviations` and use
   this task's own branch point as the opening SHA.

3. In §5 "Carried follow-ups", delete rows **1**, **2** and **9** — the crate-edge guard, the
   `buildl-core` purity ban, and the unbuilt `rust-version`. This chain closes all three.

   **Do not renumber the remaining rows.** The `#` column is an identifier other documents cite
   by number — this chain's own spec refers to "§5 rows 6, 7 and 8", and the
   `2026-09-18-core-foundation` intent refers to follow-ups #1, #2 and #3. Renumbering would
   silently redirect every one of those references. The surviving `#` values are therefore
   3, 4, 5, 6, 7, 8, 10.

4. Confirm the result. Note that `^|<digit>|` also matches §2's milestone table, so assert on the
   follow-up rows by their Origin column instead:

   ```
   $ grep -c '|I1 review|' docs/roadmap.md
   6
   $ grep -n '|I-guard|$' docs/roadmap.md
   ```

   Before the edit `grep -c '|I1 review|'` returns `9`; rows 1, 2 and 9 are three of them, so `6`
   remains. The second grep finds follow-up rows whose Target column is `I-guard` and must print
   nothing. Control — the same anchored pattern still finds a surviving row:

   ```
   $ grep -n '^|10|' docs/roadmap.md
   ```

   prints the row-10 line, proving the searches match real content.

5. Confirm nothing else in the repository still points at the closed rows:

   ```
   $ grep -rn 'roadmap.*§5 row' docs/ README.md
   ```

   Any hit naming row 1, 2 or 9 must be updated in the same commit.

6. Commit `docs(repo): close the crate-edge, purity and MSRV follow-ups`.

---

## Review findings

_Filled in during execution._

## Probe results

_Filled in during execution._

## Deviations

_Filled in during execution._
