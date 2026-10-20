---
status: done
created: 2026-10-19
depends-on: [02]
built-on: 137a1ada1042
---

# CLI clean and gc Implementation Plan

**Goal:** Users run `buildl clean` and `buildl gc` from the binary, documented as built.

**Architecture:** Two argument-less subcommands dispatched in `run.rs` exactly as `check` is
(`run.rs:174-178`): call the facade, write one line to stderr, exit 0. Errors go through the
existing `Exit::of_error` (`exit.rs:76-90`), which already maps `Refused` → 4 and every port but
`Manifest` → 3, so `exit.rs` does not change. The documents then describe both as built.

**Tech Stack:** Rust (`buildl-cli`, clap derive), Markdown docs, `cargo make dod`, `cargo deny check`.

**Content authority:** spec §2, §3.8, §4 row CLI binary, §5.

---

## Files that change

```
crates/buildl-cli/src/cli.rs                 — [modify] `Command::Clean`, `Command::Gc`
crates/buildl-cli/src/run.rs                 — [modify] dispatch both; `clean` and `gc` fns
crates/buildl-cli/src/main.rs                — [modify] module-doc command table gains `clean`, `gc`
crates/buildl-cli/tests/cli/output.rs        — [modify] stderr lines, empty stdout
crates/buildl-cli/tests/cli/exit_codes.rs    — [modify] exit 0, 3 under a held lock, 4 for a refused out
crates/buildl-cli/tests/cli/arguments.rs     — [modify] extra arguments refused
docs/design.md                               — [modify] §7 row, §8.5 gc ordering
docs/architecture.md                         — [modify] :3 Status, §3 phase table, §3.5 gc paragraph
docs/architecture-building-blocks.md         — [modify] container diagram ports, port tables, :425 paragraph
README.md                                    — [modify] :7 status, :125 command row, :129 and :157 command notes
CLAUDE.md                                    — [modify] Status names clean and gc
```

## Order of work

### Task 1 — Add the `clean` and `gc` subcommands

**Files:**
- Modify `crates/buildl-cli/src/cli.rs` — after `Log(LogArgs)` (`cli.rs:68`): `Clean` with doc
  "Remove the output directory", `Gc` with doc "Drop rows of targets no longer declared and
  remove what nothing needs"
- Modify `crates/buildl-cli/src/run.rs` — two `Command::` arms (`run.rs:102-108`); `fn clean` and
  `fn gc`, shaped as `fn check` (`run.rs:174`): bind `let workspace = Workspace::open(start)?;`,
  then `workspace.clean()?` and write `clean_line(&cleaned, workspace.out())` to stderr (`Workspace::out` is added by plan 02 Task 2);
  `…gc()?` then `gc_line(&collected)`
- Test `crates/buildl-cli/tests/cli/output.rs`, `crates/buildl-cli/tests/cli/exit_codes.rs`,
  `crates/buildl-cli/tests/cli/arguments.rs`

**Change:** `buildl clean` and `buildl gc` exist, take no arguments, print nothing on stdout and
one summary line on stderr, exit 0.

**Proof:**
- `output.rs`:
  - `clean_says_it_removed_out_and_prints_nothing_on_stdout` — after a `build`: stderr is
    `clean: removed out\n`, stdout empty.
  - `clean_without_out_says_it_holds_nothing` — fresh fixture: stderr `clean: out holds nothing\n`.
  - `gc_reports_rows_blobs_and_bytes` — an inline two-target workspace shaped as `logged`
    (`hello` copies `hello.txt`, `quiet` copies hello's output; `logged` lives under
    `crates/buildl/tests/fixtures` and is out of reach of `buildl-cli`'s tests): after a `build`,
    remove `quiet` from `build.lua`,
    `gc`: stderr matches `^gc: dropped 1 row, removed \d+ blobs? \(\d+ bytes?\)\n$`, stdout empty.
- `exit_codes.rs`:
  - `clean_and_gc_exit_0`.
  - `clean_and_gc_exit_3_while_the_lock_is_held` — hold `.buildl/lock` with `try_lock` as the
    existing busy-lock test does (`exit_codes.rs:630-644`).
  - `clean_exits_4_when_the_ceiling_leaves_out_the_output_directory`.
- `crates/buildl-cli/tests/cli/arguments.rs`: `clean_and_gc_take_no_arguments` — `buildl clean x`
  and `buildl gc x` each exit 2 (clap usage error).

**Done when:** `cargo test -p buildl-cli` passes, `cargo run -p buildl-cli -- --help` lists `clean`
and `gc`, and `cargo make dod` is green. Commit `feat(buildl-cli): add clean and gc`.

### Task 2 — Describe `clean` and `gc` as built

**Files:**
- Modify `docs/design.md` — §7 table row (`design.md:212`, `buildl clean` / `gc`): describe as
  built (clean: removes `out/`, next build restores from the cas; gc: drops rows of undeclared
  targets, removes blobs no kept row or newest logged run names); §8.5 (`:285`, paragraph `:303`):
  one sentence that `gc` forgets rows before it removes any blob, so an interrupted `gc` leaves
  extra blobs, never a row naming a missing one
- Modify `docs/architecture.md` — `:3` Status names `clean` and `gc` among the built commands; §3
  phase table (`:184`, rows like `:194`): a `gc` row — `pipeline::gc` marks from kept rows and
  `history::logged_outputs`, ports `ActionCache`, `ContentStore`, `Sweep`, `RunHistory`; §3.5
  `:291`: replace "`gc` is designed and not built …" with the built design — the `Sweep` port kept
  out of `Ports`, `ActionCache::labels`, the four steps, rows before blobs, a missing manifest
  marking only itself, staging leftovers cleared by the lock
- Modify `docs/architecture-building-blocks.md` — the ports list in the container diagram (`:110`,
  a Mermaid node: edit the label text, keep it Mermaid) gains `Sweep`; the state table (`:144`
  area) notes `cas` is listed and pruned through `Sweep`; the port table (`:442` area) gains a
  `Sweep` row (what it gives, adapter `LocalCas` over `.buildl/cas`, crate `buildl`, since 2) and
  the `ActionCache` row names `labels`; the fakes table (`:504` area) gains `Sweep`; the "When ports are wired"
  row (`:601`) adds that `gc` wires its own six ports under the lock — the five of spec §3 plus the
`Dispatcher` that load now takes — and `clean` none (spec §5's
  "command list": the document has no list of commands, `grep -n "clean"
  docs/architecture-building-blocks.md` finds nothing, so this row and `:425` carry it); `:425` gains
  "`gc` is a free function too, `pipeline::gc`, over a `DeclarationSource`, `ActionCache`,
  `ContentStore`, `Sweep` and `RunHistory`"; `:429` "today `RunHistory`" becomes "today
  `RunHistory` and `Sweep`"
- Modify `README.md` — `:7` status lists `clean` and `gc`; `:125` row → `buildl clean` / `gc` |
  `out/`; rows + cas | remove outputs; drop rows of undeclared targets and blobs nothing needs |
  yes; `:129` adds that `clean` and `gc` take no arguments and take the lock; `:157` adds `clean`
  and `gc` to the commands that work without the strategy line (neither runs an action:
  `Workspace::clean`/`gc` call no `Wiring::runs`)
- Modify `CLAUDE.md` — Status (`:7`): "… and `log` shows what a target printed when it last ran"
  gains "; `clean` removes the output directory and `gc` prunes the store"

**Change:** documents only. Mermaid stays Mermaid (CLAUDE.md, "Chat ASCII, document Mermaid");
section numbers and cross-references are unchanged.

**Proof:**
- `grep -n "designed and not built" docs/architecture.md` no longer matches the `gc` paragraph.
- `grep -n "not yet" README.md` no longer matches the `clean` / `gc` row.
- `grep -c "Sweep" docs/architecture.md docs/architecture-building-blocks.md` is non-zero for both.
- Every `§` reference touched still resolves to a heading
  (`grep -n "^#\+ " docs/*.md` against each `§N.N` written).

**Done when:** the greps above hold, `cargo make dod` is green (doc links), and `cargo deny check`
is green. Commit `docs(repo): describe clean and gc as built`.

---

## Risks

- The `gc` CLI test edits its workspace's `build.lua`; `quiet` is a leaf, so `hello` still resolves.
- Docs line numbers are from commit `bc6bea6`; plans 01–02 do not touch docs, so they hold, but
  `execute` re-greps before editing.

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- End-to-end by hand on a copy of fixture `logged`: `buildl build; buildl clean; buildl build` →
  the second build's summary counts 2 cached and nothing built; `buildl gc` twice → second line `gc: dropped 0 rows, removed 0 blobs (0 bytes)`.

## Review findings

One reviewer pass over `07974cb` and `3b5c844`: DoD and `cargo deny check` green; the plan's
end-to-end Proof run by hand on a copy of `logged` passed (second build `2 cached`, nothing built;
second `gc` → `gc: dropped 0 rows, removed 0 blobs (0 bytes)`). No blocking finding.

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | 🟡 | building-blocks fakes row: `Sweep` fake "records what it removes; can be told to fail" — false | fixed: describes `FakeCas` as it is |
| 2 | 🟡 | README: `clean`/`gc` work without the strategy line — untested | fixed: `clean_and_gc_exit_0_without_a_strategy_line`; fails (exit 5) with a `runs()?` added to `Workspace::clean` |
| 3 | 🟡 | the inline gc workspace and the six ports were not written into this plan | fixed: Task 1 Proof and Task 2 amended |
| 4 | 🔵 | `main.rs` doc table showed literal `clean: removed out` | fixed: names the manifest's `out` directory |
| 5 | 🔵 | architecture §3 `gc` row listed 4 ports, §3.5 lists 6 | fixed: six |
| 6 | 🔵 | design §7 `gc` "Stops after" was "—" | fixed: Resolve for `gc` |
| 7 | 🔵 | `main.rs` not in Files | fixed: added to Files |

## Probe results

- The `gc` test's premise that it can use fixture `logged` failed: `logged` lives under
  `crates/buildl/tests/fixtures`, out of reach of `buildl-cli`'s tests. The test builds an inline
  workspace shaped the same way; plan text amended.
- `gc` wires six ports, not five: the parallel-load chain's plan 01 gave load a `Dispatcher`.
  Plan text amended. Neither is a claim of the spec, so no `disproves` marker.

## Deviations

- Briefs pointed each coder at its task's text in this plan rather than through a task-briefer
  ledger; tasks ran beside the parallel-load chain's plan 02 (disjoint files).
- `crates/buildl-cli/src/main.rs` module-doc table gained `clean` and `gc` (not in the plan's file
  list; added so the table is not stale).
