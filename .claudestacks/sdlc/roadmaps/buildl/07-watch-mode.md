---
status: done
created: 2026-10-15
---

# Milestone: Watch mode

## Goal

`buildl watch` re-plans on file change, in its full-reload form. This is design §13
item 7.

## Non-goals

A persistent engine dispatching through `ext.on`, unless profiling demands it.
