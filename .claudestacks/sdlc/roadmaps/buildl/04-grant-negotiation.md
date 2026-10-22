---
status: approved
created: 2026-10-15
---

# Milestone: Grant negotiation UX

## Goal

Grants are negotiated: the wanted set is intersected with the ceiling, an approver
decides, and `doctor` prints the result (design §7). Per-target grant narrowing (design
§14) is settled. It follows airsl's model (intersect, never union; the approver only
narrows; fail closed) rather than consuming airsl's types. This is design §13 item 4.

## Non-goals

The airsl extension manifest parser and the `ext.on` dispatcher, which are not needed
yet.
