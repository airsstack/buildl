---
status: approved
created: 2026-10-15
---

# Milestone: Plugins

## Goal

Plugins exist: an `extension.toml` manifest, attribution, and `b.use()`, consuming the airsl
extension manifest format. Discovery targets (design §10) exist. This is design §13 item 5.

## Non-goals

Per-action mid-execution discovery, which extends the discovery target contract only if
it is ever required (design §14).
