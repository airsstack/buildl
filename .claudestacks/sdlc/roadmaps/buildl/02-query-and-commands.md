---
status: approved
created: 2026-10-15
---

# Milestone: `query` and the commands around a build

## Goal

`buildl query` answers `deps()` and `rdeps()`. The commands around a build exist: `test`,
`log`, `clean` and `gc` (design §7). Build files load in parallel, and a command that names
targets loads only the directories those targets need (design §11.1). This is design §13
item 2.

## Non-goals

Demand-driven loading for `query`, `graph`, `check` and a run without targets: they load
the whole workspace, because `rdeps` needs every reverse edge and `graph.json` is the whole
graph (design §14). The first publish to crates.io (milestone 09).
