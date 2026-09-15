# REVIEW.md — review policy

<!-- Provisioned by claudestacks-sdlc. Versioned: edit deliberately, log changes
     in the Tuning log. Reviewer-agent consumption of this file is a separate
     future chain; until then this policy is documentation you can point any
     reviewer at, including pasting it into a review prompt by hand. -->

## Passes, in order

1. **Bugs and logic errors** — correctness of the diff on its own terms.
2. **Security** — injection, secrets in the diff, unsafe input handling,
   privilege and network boundaries.
3. **Compliance** — the diff against the chain's spec and plan: scope drift,
   silent carry-over, missing or unauthorized requirements.

## Severity

- **Important** — must be addressed before commit.
- **Nit** — batch or ignore; never blocks.

## What closes a review

Every review names its Important set — a count, or `none`. A round closes when that
set is empty: each Important either fixed or explicitly declined in front of the
author. It does not close when the report is empty, because the report is not
expected to come back empty — each round of fixes writes new code, and new code gives
the next round new nits to find. A non-empty report under `none` Important is a pass,
not unfinished work, and never on its own justifies another round.

This narrows nothing about what gets reported: a review still writes down everything
it finds at both severities. The stopping condition governs what blocks a commit, not
what appears in the report.

## Exclusions

- Generated paths.
- Anything CI already enforces deterministically.

## Tuning log

<!-- Dated entries when this policy changes. Newest first. -->
