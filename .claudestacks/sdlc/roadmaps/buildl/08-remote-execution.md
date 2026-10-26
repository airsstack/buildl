---
status: approved
created: 2026-10-15
---

# Milestone: Remote execution

## Goal

Actions can run on a REAPI server's workers, with buildl as a client only, for a
repository that has outgrown one machine. This is design §13 item 8, without its
toolchain containers.

## Non-goals

- Containers of any kind, toolchain containers included (decided 2026-10-26).
- Anything design §13 does not list.
