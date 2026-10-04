---
status: done
created: 2026-09-18
---

# Intent: Make the workspace's architectural rules fail the gate

## Problem

The workspace chain delivered four crates whose dependency edges match the architecture, but it
delivered them as *facts*, not as *rules*. Its review recorded three gaps, all still open
(`docs/roadmap.md` §5, rows 1, 2 and 9):

- **The crate edges are unguarded.** `architecture-building-blocks.md` §7 rule 1 says `buildl-core`
  depends on no buildl crate, no airsl and no I/O API; rule 2 says airsl is reachable only from
  `buildl-lua`. The compiler enforces the half of this that is "a crate cannot import what its
  manifest does not list" — but nothing enforces the other half. Deleting `buildl`'s dependency
  edges today still passes `cargo make dod`. The gate stays green while the architecture is gone.
- **`buildl-core`'s purity is a review convention.** The rule that `buildl-core` names no
  filesystem, process, thread, environment or clock API is the thing that makes
  `architecture.md` §5 rule 4 true by construction — the crate boundary quarantines the wall clock
  instead of grep doing it. But the crate boundary only stops *other crates'* APIs. A `std::fs`
  call inside `buildl-core` compiles fine, and the first reviewer who misses one silently retires
  the guarantee.
- **The declared MSRV is never built.** The workspace sets `rust-version = 1.94` to match airsl's
  minimum. CI runs on stable only, with no job on 1.94, so the declared floor is untested. It can
  drift above 1.94 without anything going red, and the first person on an older toolchain finds out
  instead of CI.

The common shape: three architectural guarantees the documents state and the code currently
honours, with no mechanism that would notice if it stopped. That is worth fixing *now*, before
`buildl-core` has any code in it — a guard written against an empty crate starts green and stays
honest, whereas one retrofitted after four slices of code starts red and gets negotiated down.

What "the guard" should be is not obvious in every case. The ban list for `buildl-core`'s
forbidden APIs has to be enumerated, and a future slice may have a legitimate need for something on
it — what happens then is a policy question with more than one defensible answer, which is why this
chain takes a design pass rather than the spec skip.

## Affected systems

- `deny.toml` — the dependency-ban configuration
- `Makefile.toml` — the `cargo make dod` gate's task list
- `.github/workflows/ci.yml` — the CI matrix
- Workspace `Cargo.toml` — the lint tables, if the guards are expressed there
- Any new lint or tooling configuration file the design settles on

## Desired outcome

The three rules are machine-checked, and each has a falsifiable demonstration that the check works:

- Removing a crate's dependency edge, or adding airsl to a crate other than `buildl-lua`, turns the
  gate red rather than leaving it green.
- Naming a filesystem, process, thread, environment or clock API inside `buildl-core` turns the
  gate red, and the failure names the offending call rather than failing generically.
- A job builds the workspace on the declared `rust-version`, so raising the real floor above 1.94
  fails CI instead of passing silently.

Alongside those:

- Each guard runs in `cargo make dod`, in CI, or in both, and the design says which and why — a
  guard that only runs in CI is not available to the person about to commit.
- The escape hatch for a future legitimate use of a banned API is decided and written down, so the
  first slice that needs one follows a documented path instead of deleting the rule.
- Every ban carries its reason inline, matching the workspace convention that each dependency is
  commented with why it is there.
- `cargo make dod` and `cargo deny check` pass.

## Constraints

- The guards constrain `buildl-core` only. `buildl`, `buildl-lua` and `buildl-cli` are adapters and
  the composition root; filesystem, process, thread, environment and clock APIs are their job, and
  no guard may make that harder.
- No guard may require tooling the CI runners do not already have, or that a contributor cannot run
  locally.
- Existing workspace policy is a floor, never weakened to make a guard fit: `unsafe_code =
  "forbid"`, `unwrap_used` and `panic` denied, pedantic and nursery clippy at warn, no wildcard
  version requirements.
- Existing `deny.toml` entries and their documented reasons survive; this chain adds to that file
  rather than rewriting it.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The `buildl-core` types, error model, canonical serializer and clock port that these guards
  protect. They are the sibling chain, `2026-09-18-core-foundation`. This chain is ordered
  independently of it and needs none of its code to be testable.
- The documentation-consistency follow-ups (`docs/roadmap.md` §5, rows 6, 7 and 8): inaccurate
  tooling comments, the §7-versus-§4 contradiction about whether an adapter may depend on another
  adapter, and the `run` versus `build` command-name disagreement. Prose fixes, not gate changes.
- Row 10, whether the runners' rustup is new enough for `rustup toolchain install` with no
  argument. It is answered by observing the first CI run on a pushed branch, not by building
  anything.
- Enforcing the adapter-isolation rule *inside* `buildl` (that its adapter modules never import each
  other). `architecture-building-blocks.md` §7 records this as review-enforced until an adapter
  earns its own crate.
- Publishing readiness of any kind.
