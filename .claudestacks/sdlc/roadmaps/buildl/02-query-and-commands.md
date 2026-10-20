---
status: done
created: 2026-10-15
---

# Milestone: `query` and the commands around a build

## Goal

`buildl query` answers `deps()` and `rdeps()`. The commands around a build exist: `test`,
`log`, `clean` and `gc` (design §7). Build files load in parallel (design §11.1). This is
design §13 item 2.

## Non-goals

Demand-driven loading, for any command: every command that loads build files loads the whole
workspace (design §11.1). It was dropped on 2026-10-20 (chain `2026-10-15-demand-driven-load`).
The first publish to crates.io (milestone 09).
