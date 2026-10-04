---
status: approved
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
