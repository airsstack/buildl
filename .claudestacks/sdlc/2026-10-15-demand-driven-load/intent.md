---
status: draft
created: 2026-10-15
milestone: buildl/02
---

# Intent: I15 demand-driven load

## Problem

A command that names targets loads only the directories those targets and their dependencies need (design §11.1). Only commands that name targets: `query`, `graph`, `check` and a command run without targets load the whole workspace, because `rdeps` needs every reverse edge, `graph.json` is the whole graph, and a build file is still evaluated only because an evaluated one named its directory (design §14).

## Affected systems

Not explored yet — parked by milestone buildl/02.

## Desired outcome

Not explored yet — parked by milestone buildl/02.

## Constraints

Not explored yet — parked by milestone buildl/02.

## Non-goals

Not explored yet — parked by milestone buildl/02.
