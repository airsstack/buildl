---
status: done
created: 2026-10-15
---

# Milestone: Core pipeline

## Goal

Load, Resolve, Plan, Execute and Record run end to end. The `buildl` binary runs `check`,
`graph`, `plan` and `build` on a real workspace, with a local cache and cas, and actions
run at isolation tier 1 (design §13 item 1). It needs only shipped airsl.

## Non-goals

The input sandbox (milestone 03), grant negotiation (milestone 04), and every command of
design §7 other than these four.
