---
status: dropped
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

## Dropped

Dropped 2026-10-20. Demand-driven load breaks four settled rules: build settings are
workspace-global names with no directory to load (design §5, §10.2); `graph.json` is always the
whole graph, and output places are checked over the whole graph (architecture §6); and a build
file is evaluated only because a `subdir` named it (design §14). A named build could then fail
where the whole build passes. The gain is under 0.2 s on 10,000 build files now that Load is
parallel, and design §11.1 says to start eager.
