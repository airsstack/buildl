---
status: approved
created: 2026-10-15
---

# Milestone: Local input sandbox (tier 2)

## Goal

Actions run in the local input sandbox, isolation tier 2, the correctness discipline
everything remote depends on (design §12.2). `verify` exists (design §12.3). This is
design §13 item 3.

## Non-goals

The `os-sandbox` and `container` tiers, which are later implementations of the same
`ExecStrategy` seam.
