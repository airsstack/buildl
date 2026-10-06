---
status: approved
created: 2026-10-06
depends-on: [03]
---

# Ports Implementation Plan

**Goal:** Load's one outside capability, evaluating a build file, is a public trait in `ports/` that an adapter crate can implement.

**Architecture:** Two trait files in `crates/buildl-core/src/ports/`, one trait each. `DeclarationSource`
(`declaration_source.rs`) is the port: one method, `evaluate(&self, &BuildFile) -> Result<Evaluated>`,
with absence reported as `Evaluated::Absent` rather than as an error. `Ports` (`bundle.rs`) is the
bundle: a trait of associated types only, holding `type Source: DeclarationSource`, so a pipeline
takes one type parameter rather than one per port. Both live in `ports/` so every trait of the crate
sits in one module and no logic module declares one. Neither file has logic, so neither carries a
`#[cfg(test)]` module; each trait's rustdoc carries a doctest that implements it from outside the
crate (`use buildl_core::…`), and that doctest is the task's red/green test — it fails until the
trait is reachable from the crate root, and passing it proves the public API alone suffices to
implement the port.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, rustdoc doctests, `cargo-make`.

**Content authority:** spec §3.4 (the port's values), §5.1 (every trait lives in `ports/`), §7
(module layout: `ports/ + declaration_source.rs bundle.rs`), §8 (the two trait files are pure trait
definitions, exempt from colocated unit tests).

---

## Context an implementer needs

Plan `03` is `done`, and with it `01` and `02`. What this plan uses from them, all already
re-exported from `crates/buildl-core/src/lib.rs`:

| Item | From | Used by |
|---|---|---|
| `BuildFile` (`new`, `directory`, `provenance`, `file`) | plan 03, `types/build_file.rs` | the `evaluate` signature; both doctests |
| `Evaluated` (`Staged(StagedFile)`, `Absent`; derives `Debug, Clone, PartialEq, Eq`) | plan 03 | the `evaluate` return; `assert_eq!` in the doctest |
| `StagedFile { declarations, subdirs }`, `StagedSubdir { path, order }` (pub fields) | plan 03 | the `DeclarationSource` doctest |
| `DeclarationOrder::new(u32)` | plan 03 | the `DeclarationSource` doctest |
| `Written::new(impl Into<String>)` | plan 02 | the `DeclarationSource` doctest |
| `Directory::parse`, `Directory::is_root`, `Provenance::new(PathBuf, Directory)` | foundation chain | the `DeclarationSource` doctest |
| `Error`, `Result` | foundation chain | the signature; the doctest's `?` trailer |

`crates/buildl-core/src/ports/mod.rs` today declares only `pub mod clock;` and `pub use clock::Clock;`,
and `lib.rs` re-exports it with the single line `pub use ports::Clock;`. This plan grows both.

**Independence from plan `04`.** Plan `04` may run in parallel with this one; nothing here depends on
its error variants. The one place the final code names a plan-04 item is the `# Errors` section of
`DeclarationSource::evaluate`, which in the finished crate is the intra-doc link
``[`Error::Evaluation`](crate::Error::Evaluation)``. Until plan `04` lands that link is broken, and
`cargo make dod`'s doc step (`RUSTDOCFLAGS=-D warnings`) fails on a broken intra-doc link. This plan
therefore writes the variant as a plain code span, `` `Error::Evaluation` ``, which names the same
contract and resolves nothing. Turning it into the link is doc prose and belongs to plan `08`, which
runs after both `04` and `05`. The two plans also both edit `lib.rs`, but on different lines — plan
`04` grows the `pub use error::{…}` line, this plan grows the `pub use ports::…` line — so run them in
either order, or concurrently only if you reconcile that one file by hand.

**Doc prose deferred to plan `08`.** `ports/mod.rs` gains one Responsibilities bullet per new trait
(the module's index, like its `pub use` list). Its sentence "Every trait of the crate lives here, so
no logic module declares one." and every `lib.rs` crate-doc change belong to plan `08`.

Gate facts that bear on these two files, each of which turns `cargo make dod` red:

- `missing_docs` (warn, `-D warnings`): the trait, its method and its associated type each carry a
  doc comment.
- `clippy::missing_errors_doc` (pedantic): `evaluate` returns `Result`, so its doc has a `# Errors`
  section.
- `rustdoc::broken_intra_doc_links` under `RUSTDOCFLAGS=-D warnings`: hence the code span above.
- A doctest compiles as an external crate: it imports through `buildl_core::…`, and a doctest using
  `?` ends with the hidden trailer `# Ok::<(), buildl_core::Error>(())`.
- No `#[allow]` anywhere; no new dependency; `crates/expected-edges.txt` unchanged.

Both tasks' red output and green gate below were produced by running them against a tree holding
plans `01`–`03` and not plan `04`.

Run `cargo fmt --all` before every `cargo make dod`. Every Rust block below is in rustfmt's
canonical form. Commits follow Conventional Commits with scope `buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/ports/declaration_source.rs — [create] DeclarationSource and its doctest (task 1)
crates/buildl-core/src/ports/bundle.rs             — [create] Ports and its doctest (task 2)
crates/buildl-core/src/ports/mod.rs                — [modify] declare and re-export each trait (tasks 1, 2)
crates/buildl-core/src/lib.rs                      — [modify] re-export each trait at the crate root (tasks 1, 2)
```

---

## Task 1 — Declare the `DeclarationSource` port

**Files:**
- Create `crates/buildl-core/src/ports/declaration_source.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/ports/declaration_source.rs`; the test is
   the doctest in the trait's `# Examples` section, which implements the port using nothing but
   `buildl_core`'s root exports:

   ```rust
   //! The port through which Load evaluates one build file.
   //!
   //! Its own file because evaluating a build file is the one thing Load cannot do itself: it means
   //! reading a file and running a language this crate does not know. Everything around that one call
   //! — which files to evaluate, in what order, and what their results mean — stays in Load.
   //!
   //! Responsibilities: [`DeclarationSource`].
   //!
   //! Non-responsibilities: implementing it. The real implementation lives in an adapter crate.

   use crate::error::Result;
   use crate::types::{BuildFile, Evaluated};

   /// Evaluates one build file and returns what it staged, exactly as written.
   ///
   /// An implementation reports a missing file as [`Evaluated::Absent`] rather than as an error:
   /// what a missing file means depends on who asked for it, which only the caller knows.
   ///
   /// # Examples
   ///
   /// An implementation needs nothing but this crate's public API:
   ///
   /// ```
   /// use std::path::PathBuf;
   ///
   /// use buildl_core::{
   ///     BuildFile, DeclarationOrder, DeclarationSource, Directory, Evaluated, Provenance, Result,
   ///     StagedFile, StagedSubdir, Written,
   /// };
   ///
   /// /// A source in which only the workspace root has a build file, requesting `lib`.
   /// struct RootOnly;
   ///
   /// impl DeclarationSource for RootOnly {
   ///     fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
   ///         if !file.directory().is_root() {
   ///             return Ok(Evaluated::Absent);
   ///         }
   ///         Ok(Evaluated::Staged(StagedFile {
   ///             declarations: Vec::new(),
   ///             subdirs: vec![StagedSubdir {
   ///                 path: Written::new("lib"),
   ///                 order: DeclarationOrder::new(0),
   ///             }],
   ///         }))
   ///     }
   /// }
   ///
   /// let lib = BuildFile::new(Provenance::new(
   ///     PathBuf::from("lib/build.lua"),
   ///     Directory::parse("lib")?,
   /// ));
   /// assert_eq!(RootOnly.evaluate(&lib)?, Evaluated::Absent);
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   pub trait DeclarationSource {
       /// Evaluates `file`.
       ///
       /// # Errors
       ///
       /// Returns `Error::Evaluation` when the file exists but cannot be evaluated.
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
   }
   ```

2. Declare and re-export the module. Replace `crates/buildl-core/src/ports/mod.rs` with:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //!
   //! Non-responsibilities: implementations. A type implementing one of these traits does I/O by
   //! definition, so it cannot live in this crate.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod clock;
   pub mod declaration_source;

   pub use clock::Clock;
   pub use declaration_source::DeclarationSource;
   ```

3. Run and confirm failure — the trait exists, but an outside crate cannot name it:

   ```
   $ cargo test -p buildl-core --doc
   running 1 test
   test crates/buildl-core/src/ports/declaration_source.rs - ports::declaration_source::DeclarationSource (line 23) ... FAILED
   error[E0432]: unresolved import `buildl_core::DeclarationSource`
     --> crates/buildl-core/src/ports/declaration_source.rs:28:34
      |
   28 |     BuildFile, DeclarationOrder, DeclarationSource, Directory, Evaluated, Provenance, Result,
      |                                  ^^^^^^^^^^^^^^^^^ no `DeclarationSource` in the root
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
   ```

4. Write the minimal code. In `crates/buildl-core/src/lib.rs`, replace the line

   ```rust
   pub use ports::Clock;
   ```

   with

   ```rust
   pub use ports::{Clock, DeclarationSource};
   ```

   Leave every other line of `lib.rs` as it is.

5. Run and confirm the doctest `ports::declaration_source::DeclarationSource (line 23)` passes:

   ```
   $ cargo test -p buildl-core --doc
   running 1 test
   test crates/buildl-core/src/ports/declaration_source.rs - ports::declaration_source::DeclarationSource (line 23) ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

   Expected: exit 0; the doc step raises no broken-link warning, because `Error::Evaluation` is a
   code span, not a link.

7. Commit `feat(buildl-core): declare the port that evaluates one build file`.

---

## Task 2 — Bundle the ports behind `Ports`

**Files:**
- Create `crates/buildl-core/src/ports/bundle.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing test. Create `crates/buildl-core/src/ports/bundle.rs`; the test is the doctest,
   which implements both `DeclarationSource` and `Ports` from outside the crate:

   ```rust
   //! The bundle that names one implementation of every port a pipeline uses.
   //!
   //! Its own file because it is a different kind of trait from the ports themselves: it has no
   //! methods, only associated types, and it exists so that a pipeline takes one type parameter
   //! instead of one per port.
   //!
   //! Responsibilities: [`Ports`].
   //!
   //! Non-responsibilities: choosing the implementations. A composition root does that, by
   //! implementing this trait.

   use crate::ports::DeclarationSource;

   /// One implementation of each port, chosen together.
   ///
   /// # Examples
   ///
   /// ```
   /// use buildl_core::{BuildFile, DeclarationSource, Evaluated, Ports, Result};
   ///
   /// /// A source with no build files at all.
   /// struct Empty;
   ///
   /// impl DeclarationSource for Empty {
   ///     fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
   ///         Ok(Evaluated::Absent)
   ///     }
   /// }
   ///
   /// /// A bundle choosing that source.
   /// struct EmptyPorts;
   ///
   /// impl Ports for EmptyPorts {
   ///     type Source = Empty;
   /// }
   /// ```
   pub trait Ports {
       /// How build files are evaluated.
       type Source: DeclarationSource;
   }
   ```

2. Declare and re-export the module. Replace `crates/buildl-core/src/ports/mod.rs` with:

   ```rust
   //! The ports: the traits through which this crate reaches everything outside itself.
   //!
   //! Its own directory because the crate's defining property is that it names no I/O API. Every
   //! capability this crate needs from the outside world arrives as an implementation of a trait
   //! declared here, supplied by a caller, rather than through a direct call to an I/O API.
   //!
   //! Responsibilities:
   //!
   //! - [`Clock`] — reading the host wall clock.
   //! - [`DeclarationSource`] — evaluating one build file.
   //! - [`Ports`] — the bundle naming one implementation of each port.
   //!
   //! Non-responsibilities: implementations. A type implementing one of these traits does I/O by
   //! definition, so it cannot live in this crate.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod bundle;
   pub mod clock;
   pub mod declaration_source;

   pub use bundle::Ports;
   pub use clock::Clock;
   pub use declaration_source::DeclarationSource;
   ```

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core --doc
   running 2 tests
   test crates/buildl-core/src/ports/bundle.rs - ports::bundle::Ports (line 18) ... FAILED
   error[E0432]: unresolved import `buildl_core::Ports`
     --> crates/buildl-core/src/ports/bundle.rs:20:60
      |
   20 | use buildl_core::{BuildFile, DeclarationSource, Evaluated, Ports, Result};
      |                                                            ^^^^^ no `Ports` in the root
   ```

4. Write the minimal code. In `crates/buildl-core/src/lib.rs`, replace the line

   ```rust
   pub use ports::{Clock, DeclarationSource};
   ```

   with

   ```rust
   pub use ports::{Clock, DeclarationSource, Ports};
   ```

   Leave every other line of `lib.rs` as it is.

5. Run and confirm both doctests pass, the new one being `ports::bundle::Ports (line 18)`:

   ```
   $ cargo test -p buildl-core --doc
   running 2 tests
   test crates/buildl-core/src/ports/bundle.rs - ports::bundle::Ports (line 18) ... ok
   test crates/buildl-core/src/ports/declaration_source.rs - ports::declaration_source::DeclarationSource (line 23) ... ok
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): bundle the ports a pipeline uses behind one trait`.

---

## Verification summary (plan-level)

```
$ cargo fmt --all
$ cargo make dod
$ cargo make deny
$ cargo make guard-crate-edges
$ cargo make guard-core-purity
$ cargo +1.94 check --workspace --all-targets --all-features
```

All exit 0. `cargo make deny` reports `advisories ok, bans ok, licenses ok, sources ok`, unchanged:
this plan adds no dependency and leaves `crates/expected-edges.txt` untouched.

At the end of this plan `buildl-core` exports `DeclarationSource` and `Ports` from its root;
`ports/mod.rs` declares `bundle`, `clock` and `declaration_source` in that order; `cargo test --doc`
runs two doctests, each implementing a trait from outside the crate. Two differences from the
finished crate remain, both owned by plan `08`: `evaluate`'s `# Errors` names `Error::Evaluation` as
a code span rather than as the intra-doc link, and `ports/mod.rs` lacks the sentence "Every trait of
the crate lives here, so no logic module declares one."
