---
status: done
created: 2026-10-04
depends-on: [03]
---

# Clock Port Implementation Plan

**Goal:** The wall clock is reachable from `buildl-core` only through a port.

**Architecture:** One trait, `Clock`, in `crates/buildl-core/src/ports/clock.rs`, returning the
`Timestamp` plan `03` built. The module has no implementation and will never gain one — that absence
is the whole content of the rule. The directory it creates, `ports/`, is where every later port
trait lands, so this plan also fixes that part of the crate's module topology.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `cargo-make`.

---

## Context an implementer needs

Plan `03` is `done`: `crates/buildl-core/src/types/timestamp.rs` provides `Timestamp` with
`from_unix_nanos`, `as_unix_nanos`, `UNIX_EPOCH` and `duration_since`, and
`crates/buildl-core/src/types/mod.rs` re-exports it.

`architecture.md:216` is the rule this plan implements: `now()` is read in exactly two places — log
event timestamps and durations — through a `Clock` port whose only real adapter lives in the
`buildl` crate. The crate boundary does not enforce it, because `std::time` is in scope for every
crate; what enforces it is `crates/buildl-core/clippy.toml:133-135`, which bans `Instant`,
`SystemTime` and `SystemTimeError` by name, asserted by `cargo make guard-core-purity`.

Two structural rules bear on this file, both from `claudestacks-guideline-rust:rust-guidelines`:

- `references/mod-rs-export-only` — `ports/mod.rs` holds module docs, `mod` and `pub use`, nothing
  else.
- `references/unit-test-mandate` exemption #3 — a file whose only item is a trait declaration with
  no executable logic *may* ship no inline tests, and must cite the exemption in its own `//!`
  block **when it ships under it**.

**This plan departs from the spec on that second point, deliberately.** Spec §8 says
`ports/clock.rs` ships no tests under exemption #3 and cites the exemption in its `//!` block. Task
1 instead ships two tests and no citation. The reason: the tests exercise a *fake implementation* of
the trait, not the trait, so the file does not ship under the exemption and the guideline's citation
requirement does not apply. What those tests buy is the demonstration that the port is satisfiable
without `SystemTime` — the property the whole rule exists for — and the pattern the Load slice's
`FakePorts` will follow. Recorded here so the departure is visible at the approval gate rather than
discovered during execution; the alternative is amending spec §8.

This plan writes no `Ports` bundle. `architecture-building-blocks.md` §5.3 places `Pipeline<P: Ports>`
and the associated-type bundle in the Load slice; this chain defines one port trait, not the
bundle that will carry it.

Run `cargo fmt` before `cargo make dod`. `fmt-check` is the gate's first step
(`Makefile.toml:46-50`), and the Rust blocks below are given in rustfmt's canonical form — but
retyping or re-wrapping one can drift, and a drifted block turns the gate red before a single test
runs.

All work happens in the worktree, on a branch, never on `main`. Commit follows Conventional
Commits with scope `buildl-core`.

### File map

```
crates/buildl-core/src/ports/mod.rs   — [create] export-only index for every port trait
crates/buildl-core/src/ports/clock.rs — [create] the Clock trait
crates/buildl-core/src/lib.rs         — [modify] declare and re-export ports
```

---

## Task 1 — Declare the `Clock` port

A trait declaration has no behaviour to drive with a failing unit test, so the red step is a
compile-level one: a fake implementation written against the trait before the trait exists. That
fake is also the proof the port is implementable against a value rather than a clock, which is the
property the rest of the pipeline depends on.

**Files:**
- Create `crates/buildl-core/src/ports/clock.rs`
- Create `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/ports/clock.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       use super::Clock;
       use crate::types::Timestamp;

       /// A clock that returns a fixed instant — the shape every test in this crate will use in
       /// place of the host clock.
       struct FixedClock(Timestamp);

       impl Clock for FixedClock {
           fn now(&self) -> Timestamp {
               self.0
           }
       }

       #[test]
       fn a_port_implementation_needs_no_host_clock() {
           let clock = FixedClock(Timestamp::from_unix_nanos(1_758_412_800_123_456_789));
           assert_eq!(clock.now().as_unix_nanos(), 1_758_412_800_123_456_789);
       }

       #[test]
       fn two_reads_of_a_fixed_clock_span_no_time() {
           let clock = FixedClock(Timestamp::from_unix_nanos(42));
           let first = clock.now();
           let second = clock.now();
           assert_eq!(second.duration_since(first).map(|d| d.as_nanos()), Some(0));
       }
   }
   ```

   This test module is the reason `clock.rs` carries tests at all despite exemption #3: the
   exemption permits a bare trait file to ship none, and the two above test the *fake*, not the
   trait. Keep them — they are what demonstrates the port is satisfiable without `SystemTime`, and
   they are the pattern the Load slice's `FakePorts` follows.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Clock`
    --> crates/buildl-core/src/ports/clock.rs:5:9
   ```

3. Replace the placeholder doc comment and add the trait above the test module:

   ```rust
   //! The port through which this crate reads the host wall clock.
   //!
   //! Its own file because the rule it implements is an absence: `now()` is read in exactly two
   //! places — event timestamps and durations — and never by this crate directly. `Instant`,
   //! `SystemTime` and `SystemTimeError` are banned here by `crates/buildl-core/clippy.toml`, so
   //! the only way a real instant enters is a caller passing an implementation of this trait.
   //!
   //! Responsibilities: [`Clock`].
   //!
   //! Non-responsibilities: implementing it. The real adapter lives in the composition root; a
   //! fake one lives in this crate's tests.

   use crate::types::Timestamp;

   /// Reads the host wall clock.
   ///
   /// The only implementation in this crate is a test fake. A real one reads the system clock,
   /// which is why it belongs to an adapter rather than here.
   pub trait Clock {
       /// The current instant.
       fn now(&self) -> Timestamp;
   }
   ```

4. Create `crates/buildl-core/src/ports/mod.rs`:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability it needs — evaluating a build file, hashing a file, running an action, reading the
   //! clock — arrives as an implementation of a trait declared here, supplied by a caller.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //!
   //! Non-responsibilities: implementations. A type implementing one of these traits does I/O by
   //! definition, so it cannot live in this crate.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod clock;

   pub use clock::Clock;
   ```

   and add to `crates/buildl-core/src/lib.rs`:

   ```rust
   pub mod ports;

   pub use ports::Clock;
   ```

5. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 2
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

   With all five earlier plans landed the suite is **45** tests; what this task asserts is `0 failed`
   and the two new names below it.

6. Confirm the purity guard still passes — it is the mechanism that makes this port meaningful
   rather than decorative:

   ```
   $ cargo make guard-core-purity
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): read the wall clock only through a port`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
```

All three exit 0.

At the end of this plan `buildl-core` exports `Clock`, holds a `ports/` directory for every later
port trait, and contains no implementation of `Clock` outside its own tests. Nothing in the crate
names `SystemTime` or `Instant`, which `cargo make guard-core-purity` asserts rather than leaving to
review.

---

## Review findings

- drift (blocking) — spec §2 rule 4 and §5 assert `ports/` "never gains an implementation" and that `Clock` is a bare trait with no implementation in `buildl-core`; both are false in letter, since `ports/clock.rs:33` holds `impl Clock for FixedClock` under `#[cfg(test)]`. The invariant those sentences protect — no I/O in `buildl-core` — is intact and mechanically guarded. Repaired on the source side by narrowing `lib.rs` rule 4; the spec sentences themselves are still owed an amendment — `.claudestacks/sdlc/2026-09-18-core-foundation/spec.md` §2 rule 4, §5.
- drift — spec §8 states that `ports/clock.rs` ships no tests under `unit-test-mandate` exemption #3 and cites that exemption in its `//!` block. The shipped file does neither, and the reviewer judged the file right and the spec wrong on both halves: exemption #3 is permissive ("may ship with zero inline tests"), the citation duty fires only for a file actually shipping under an exemption, and a literal "exemption #3" citation would itself violate `doc-comment-discipline`'s ban on rule-numbered shortcodes in source. This plan recorded the departure at its approval gate (lines 40-47) and named the spec amendment as the alternative; the amendment was not made — spec §8.
- risk — `pub use ports::Clock;` is unguarded: nothing in the crate or its tests names `buildl_core::Clock`, so deleting the root re-export — a semver break for downstreams — passes the whole gate green. A doctest or integration test on the root path would catch it. Pre-existing crate-wide pattern (the `error` and `types` re-exports are equally unguarded), not introduced here; left as shipped — `crates/buildl-core/src/lib.rs:57`.
- nit — `now()` "is read in exactly two places" stated a present-tense count that is untrue today, there being zero call sites. Fixed in the fix round to the ceiling form, "is read only for event timestamps and durations, and never by this crate directly". Verified by `cargo make dod` → `[cargo-make] INFO - Build Done in 8.82 seconds.`, exit 0 — `crates/buildl-core/src/ports/clock.rs:3`.
- nit — the ports index named three capabilities — "evaluating a build file, hashing a file, running an action" — whose traits do not exist in the directory, so a reader greping for a hasher port finds nothing. Fixed in the fix round to describe only what is there. Verified by the same `cargo make dod` run — `crates/buildl-core/src/ports/mod.rs:3`.
- nit — `two_reads_of_a_fixed_clock_span_no_time` re-asserts what `types/timestamp.rs::tests::measures_the_span_between_two_instants` already pins; what is unique to it is that `FixedClock` is constant, which is a property of the test fake rather than of the port. Harmless; left as shipped — `crates/buildl-core/src/ports/clock.rs:46`.
- nit — `pub use clock::Clock;` carries no `#[doc(inline)]`, so the trait's docs do not render on the `ports` module page (`M-DOC-INLINE`). Consistent with the `types/mod.rs` precedent, so left alone as a crate-wide decision rather than a one-file change — `crates/buildl-core/src/ports/mod.rs:18`.
- question — `Clock` carries no `Send + Sync` bound, although the executor is plain threads (`architecture.md` §6) and the clock feeds log event timestamps from worker threads. Spec §5 specifies a bare trait, so shipping it bare is spec-correct; the bound is presumed deferred to the `Ports` bundle in I3 (`architecture-building-blocks.md` §5.3). Recorded so the intent is not rediscovered in I3 — `crates/buildl-core/src/ports/clock.rs:19`.

## Probe results

- Claim: plan 03 landed `Timestamp` with `from_unix_nanos`, `as_unix_nanos`, `UNIX_EPOCH` and `duration_since`, re-exported from the crate root. Command: `grep -n 'pub const fn\|pub fn\|UNIX_EPOCH' crates/buildl-core/src/types/timestamp.rs` and a read of `lib.rs`. Output: `31: pub const UNIX_EPOCH: Self = Self(0);`, `35: pub const fn from_unix_nanos(nanos: u64) -> Self {`, `41: pub const fn as_unix_nanos(self) -> u64 {`, `50: pub fn duration_since(self, earlier: Self) -> Option<Duration> {`, and `pub use types::{Digest, Directory, Label, NodeId, Provenance, TargetName, Timestamp};`. Holds.
- Claim: `crates/buildl-core/src/ports/` does not yet exist. Command: `ls crates/buildl-core/src/`. Output: `error.rs  json  lib.rs  types`. Holds — the task creates it.
- **Claim came out AGAINST the plan.** Step 5 states "With all five earlier plans landed the suite is **45** tests". Command: `cargo test -p buildl-core`. Output: `test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`. The real baseline was 52, so this task landed on 54, not 47. See Deviations.
- Claim: no implementation of `Clock` exists outside this file's own tests. Command (run by the reviewer): `grep -rn "Clock for" --include="*.rs" --exclude-dir=target .`. Output: exactly one hit, `crates/buildl-core/src/ports/clock.rs:33`, inside the `#[cfg(test)] mod tests` opened at line 25. A wider `grep -rn "Clock"` over `crates/` hits only three files, so no other workspace crate names the trait. Holds.
- The task's own red step was its probe, so no second one was written. Command: `cargo test -p buildl-core` after step 1. Output: `error[E0432]: unresolved import `super::Clock`` with the span `--> crates/buildl-core/src/ports/clock.rs:5:9` and the note `no `Clock` in `ports::clock``. Matched the plan's predicted failure exactly.

## Deviations

- 2026-10-04 — The plan's step 5 predicted a 45-test suite; the real pre-task baseline was 52 and the task landed on 54. The coder was briefed up front to assert `0 failed` plus the two new test names rather than any total, and to report the real number rather than bend a test toward the plan's figure. No test was merged, dropped or invented. This is the third stale count in this chain, so it was pre-empted rather than left to surface in the fix round.
- 2026-10-04 — Spec §8's prescription — no tests, plus an exemption-#3 citation in the `//!` block — was not followed, because the plan's own step 1 prescribes two tests against a `FixedClock` fake. The plan recorded this departure at its approval gate. The reviewer independently judged the shipped file right and spec §8 wrong on both halves. The spec amendment the plan named as the alternative has not been made.
- 2026-10-04 — `lib.rs` rule 4, whose text plan 06 task 1 prescribed verbatim, asserted that `ports` "never gains an implementation". That is contradicted by this plan's own test fake, so the fix round narrowed it to "never gains a production implementation … the only implementations here are test fakes". This is a deviation from plan 06's prescribed block, made because the prescribed text was false.
