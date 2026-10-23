---
status: done
created: 2026-10-22
depends-on: [01, 02, 03, 04, 05, 06, 07]
built-on: 0f868841fc48
---

# Documentation Implementation Plan

**Goal:** The design, architecture, README, CLAUDE.md, roadmap and crate docs describe plugins and discovery targets as built.

**Architecture:** Documentation only. Each document keeps its role (CLAUDE.md "Documents and
their roles"); section cross-references stay load-bearing; diagrams added to `docs/` are Mermaid.
Every statement is checked against the code the earlier plans merged, not against the spec.

**Tech Stack:** Markdown, Mermaid, rustdoc; `cargo make dod` (doc step `-D warnings`).

**Content authority:** spec §10, and the behaviour plans 01–07 delivered.

---

## Files that change

```
docs/design.md                       — [modify] §4, §5, §7, §8.7, §10, §13 item 5, §14
docs/architecture.md                 — [modify] status line; origin and plugin check in Plan; discovery passes; the plugins adapter
docs/architecture-building-blocks.md — [modify] adapter table (`plugins`), port change (`DeclarationSource`, `Approver`, the `Ports::Log` bound `EventLog + RunHistory`), types table, per-command wiring
README.md                            — [modify] plugins, `plugins fetch`, discovery
CLAUDE.md                            — [modify] Status paragraph: milestone 5 built
.claudestacks/sdlc/roadmaps/buildl/05-plugins.md — [modify] goal wording: manifest is `extension.toml`
crates/*/README.md and crate-level docs (lib.rs) — [modify] where they list commands or modules
```

## Order of work

### Task 1 — The design document

**Files:** `docs/design.md`.

**Change:** §4 adds the `[plugins.<name>]` table (keys, defaults, refusals) and `buildl.lock`;
§5 adds `buildl.use`, `buildl.discover`, `buildl.discovered`; §7 adds `buildl plugins fetch` to the
command table and states the per-command discovery flow; §8.7 adds the new failures to the exit
table (5: not fetched, bad manifest, discovery changed, tree with a link; 4: outside a plugin;
3: git; 6: plugin changed; 1: discovery output a directory); §10 replaces `plugin.toml`/`[plugin]`
with `extension.toml`/`[extension]`, describes sources, the environment, origin chains and the
per-plugin intersection; §13 item 5 says "Built."; §14 marks discovery targets and plugin API
versioning resolved, with the decisions.

**Proof:** `grep -n "plugin.toml\|\[plugin\]" docs/design.md` finds nothing;
`grep -n "Built\." docs/design.md` shows item 5; every `§` reference in the file still resolves to
a heading (`grep -o "§[0-9.]*"` checked against the outline).

**Done when:** proof holds, `cargo make dod` and `cargo deny check` green. Commit
`docs(repo): design for plugins and discovery targets`.

### Task 2 — Architecture documents

**Files:** `docs/architecture.md`, `docs/architecture-building-blocks.md`.

**Change:** describe, as built: `Origin` and `Node::plugins`, the plugin check before the ceiling,
`Attribution`, the `Approver` signature, the `Discovered` port argument and the `discovery`
module, the `plugins` adapter module (lock, tree digest, check, fetch through git) and that it
imports no other adapter, which commands now read the log and cas without the lock.

**Proof:** each new type and module name in the documents exists in the code
(`git grep -n "<name>" -- crates` per name); no `mermaid` block converted to ASCII.

**Done when:** proof holds, `cargo make dod` and `cargo deny check` green. Commit
`docs(repo): architecture for plugins and discovery`.

### Task 3 — README, CLAUDE.md, roadmap, crate docs

**Files:** `README.md`, `CLAUDE.md`, `.claudestacks/sdlc/roadmaps/buildl/05-plugins.md`, crate
READMEs and `lib.rs` docs that list commands or modules.

**Change:** README summarises plugins, sources, `plugins fetch`, discovery targets, consistent with
the design; CLAUDE.md's Status says milestone 5 is built; the milestone's goal says
`extension.toml`; crate docs list the new module and command.

**Proof:** `cargo make dod` green (rustdoc `-D warnings`); README and design agree on every
command name (`grep -n "plugins fetch" README.md docs/design.md`).

**Done when:** proof holds, `cargo make dod` and `cargo deny check` green. Commit
`docs(repo): plugins in README, CLAUDE.md and the roadmap`.

## Risks

- Renumbering in design §10 or §14 would break cross-references; add, don't renumber.
- Describing what the spec said instead of what the code does; each task's proof checks the code.

## Proof

`cargo make dod` green; no document mentions `plugin.toml`; every new name in the docs exists in
the code.

## Review findings

One review (the first spawn died on a lost API connection before writing anything; re-run); blocking set: the first two lines; all 13 fixed in one round (`d3b923a`), verified `cargo make dod` exit 0, `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`. The reviewer checked design §4, §5, §7, §8.7, §10, §13, §14, architecture §3.6, §3.7, §4, §6 and the building-blocks tables against the code: accurate; every `§` reference resolves; no `plugin.toml`/`[plugin]` left; no Mermaid block converted.

- doc accuracy (blocking) — `buildl-cli/src/main.rs` command table said `graph`/`query`/`doctor` write nothing to stderr; `doctor` stdout omitted attribution and plugin violations — corrected
- doc consistency (blocking) — `crates/buildl/README.md` said `plugins` names only `state` — now `state` and `search_path`
- doc accuracy — `main.rs` `doctor` evaluates twice with a discovery target
- doc accuracy — `buildl-core/README.md` `Pipeline::graph`/`plan`/`build` load twice with a discovery target
- doc accuracy — `buildl-cli/README.md` exit 5 for a link only at fetch; a stored plugin with a link exits 6
- consistency — README status list gains `plugins fetch`; fetch runs git also for a missing or damaged store; "every command that loads build files" checks plugins; the not-run note is `check`/`graph`/`query`/`doctor`, `plan` prints "is not up to date"
- doc consistency — `architecture.md` no longer gives `gc` as a reason for the `Ports::Log` bound
- doc accuracy — design: a path plugin may not be or lie inside the configured output or state directory; the read-only commands may remove a blob failing its digest; the refusal example starts with the workspace-relative build file path

## Probe results

None: a documentation plan. Each task's proof ran instead — `grep -n "plugin.toml\|\[plugin\]" docs/design.md README.md` finds nothing; `grep -c "plugins fetch"` → `docs/design.md:11`, `README.md:4`; design `§` references 29 distinct, all resolving; 28 new type/module names in the architecture docs each found in `crates/` by `git grep`; Mermaid blocks architecture.md 3 → 4, building-blocks 7 → 7.

## Deviations

- 2026-10-23 — the three tasks ran side by side (disjoint files) and were committed 3, 1, 2.
- 2026-10-23 — the docs name `Node::narrowing` (the built field), not `Node::plugins`.
- 2026-10-23 — design §3 layout, the status paragraph, §6 and §9.6 also changed, to stay consistent (`buildl.use` takes only a name; `b.fetch` and the toolchain idiom are not built).
- 2026-10-23 — `crates/buildl/src/lib.rs` `plugins` note corrected to name `search_path` with Task 2's commit.
