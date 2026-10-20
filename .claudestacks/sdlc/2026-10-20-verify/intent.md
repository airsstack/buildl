---
status: draft
created: 2026-10-20
milestone: buildl/03
---

# Intent: I17 buildl verify

## Problem

Nothing checks that an action produces the same bytes twice, so a nondeterministic tool silently poisons the cache. `buildl verify` must re-run cache-hit actions in the sandbox and compare output digests, naming the target and output that differ (design §12.3). Depends on the input sandbox.

## Affected systems

Not explored yet — parked by milestone buildl/03.

## Desired outcome

Not explored yet — parked by milestone buildl/03.

## Constraints

Not explored yet — parked by milestone buildl/03.

## Non-goals

Not explored yet — parked by milestone buildl/03.
