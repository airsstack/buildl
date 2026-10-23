---
status: approved
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
docs/architecture-building-blocks.md — [modify] adapter table (`plugins`), port change (`DeclarationSource`, `Approver`), types table, per-command wiring
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
