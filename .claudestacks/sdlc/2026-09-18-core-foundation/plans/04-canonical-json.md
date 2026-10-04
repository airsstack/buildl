---
status: approved
created: 2026-10-04
depends-on: [01]
---

# Canonical JSON Implementation Plan

**Goal:** Every value leaves `buildl-core` as canonical, float-free JSON bytes.

**Architecture:** One module, `crates/buildl-core/src/json/canonical.rs`, exposing `to_vec` and
`to_string`. Sorting is not hand-written: routing a value through `serde_json::to_value` produces
byte-order key sorting at every level, because with `preserve_order` off a `serde_json::Map` is a
`BTreeMap`. Floats are refused in two places with different jobs — a tree walk at the boundary, and
a compile-time ban on `f32`/`f64` in this crate, which is the only place a non-finite float still
exists to be seen.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` 1.0, `serde_json` 1.0, `cargo-make`,
clippy `disallowed_types`.

---

## Context an implementer needs

Plan `01` is `done`: `crates/buildl-core/src/error.rs` holds `Error` (`#[non_exhaustive]`, with one
`InvalidName` variant), `NameKind`, and
`pub type Result<T> = core::result::Result<T, Error>`. `crates/buildl-core/Cargo.toml` already
depends on `serde`, `serde_json`, `sha2` and `thiserror`, and `crates/expected-edges.txt` already
records those edges — **this plan changes neither file**, which is what lets it run concurrently
with plan `02`.

This plan adds two variants to the `Error` enum from plan `01`. That is the only file it shares with
another plan in flight, and `02` does not touch it.

### Why the ban is half the mechanism

`architecture.md:214` requires that all JSON leave through one serializer with "sorted keys, fixed
float handling", without saying what the float handling is. The decision is that no float reaches
JSON at all — the bytes this module produces are hashed into an action key, and a value that
serializes differently from what it means makes the key and the file disagree.

A tree walk cannot deliver that alone. Probed with `serde_json` 1.0.151:

```
to_value(1.5)   = Number(1.5)  is_f64=true
to_value(3.0)   = Number(3.0)  is_f64=true   is_i64=false
to_value(NAN)   = Null         is_f64=false  is_null=true
to_string(&f64::NAN)      = null
to_string(&f64::INFINITY) = null
```

By the time a `Value` exists, `NaN` and `±Infinity` are indistinguishable from a real `null`. The
walk catches every finite float, whole-valued ones included; it can never catch the three whose
silent conversion is the hazard. So the guarantee is made at compile time, where the float still
exists, and the walk stays as the second line for a float arriving through a dependency's
`Serialize` implementation.

### The one form the float test may take

Clippy's `disallowed_types` does **not** fire on an un-annotated float literal. Probed with the two
entries this plan adds, run with `-D clippy::disallowed_types` over a file holding
`canonical(&1.5)`, `canonical(&3.0)`, `canonical(&vec![1.5, 2.5])` and `let d: f64 = 1.5`, exactly
one error appeared:

```
error: use of a disallowed type `f64`
  --> src/main.rs:16:12
   |
16 |     let d: f64 = 1.5;
   |            ^^^
```

Only the annotated binding. So task 4's test must write `to_vec(&1.5)` and never `1.5f64`,
`let x: f64 = 1.5`, or a fixture struct with a float field — each of those turns the gate red with
no remedy available inside the crate, because `guard-core-purity` rejects every in-source spelling
of an allow attribute and `cargo make clippy` covers `--all-targets`.

### Gate facts

- `clippy::unwrap_used` is **denied** (`Cargo.toml:71`), so test modules open with
  `#![expect(clippy::unwrap_used, reason = "…")]`.
- `clippy::doc_markdown` fires on a bare crate name in a doc comment: write `` `serde_json` ``.
- `guard-core-purity` (`Makefile.toml:146-216`) asserts every ban in
  `crates/buildl-core/clippy.toml` resolves. Task 1's two new entries must therefore name types that
  exist, which `f32` and `f64` do — clippy resolves primitives by path.

Run `cargo fmt` before every `cargo make dod` in this plan. `fmt-check` is the gate's first step
(`Makefile.toml:46-50`), and every Rust block below is given in rustfmt's canonical form — but
retyping or re-wrapping one can drift, and a drifted block turns the gate red before a single test
runs.

All work happens in the worktree, on a branch, never on `main`. Commits follow Conventional
Commits; scope is `buildl-core` except task 1, which is `repo` because it edits a workspace guard
config. One commit per task.

### File map

```
crates/buildl-core/clippy.toml           — [modify] ban f32 and f64 (task 1)
docs/architecture-building-blocks.md     — [modify] record the float family under §1.2 (task 1)
crates/buildl-core/src/error.rs          — [modify] add the two JSON variants (task 2)
crates/buildl-core/src/json/mod.rs       — [create] export-only index (task 2)
crates/buildl-core/src/json/canonical.rs — [create] to_vec, to_string (task 2)
                                           [modify] the float walk (task 3)
                                           [modify] the float tests (task 4)
crates/buildl-core/src/lib.rs            — [modify] declare and re-export json (task 2)
```

---

## Task 1 — Ban `f32` and `f64` in this crate

The red step is clippy accepting a float before the ban exists. This task edits a file the
`2026-09-18-workspace-guardrails` chain owns; the spec records that departure and its reason in its
§12.1.

**Files:**
- Modify `crates/buildl-core/clippy.toml`
- Modify `docs/architecture-building-blocks.md`

**Steps:**

1. Confirm clippy currently accepts a float in this crate. Create a throwaway
   `crates/buildl-core/src/probe.rs` — a separate file rather than a function in `lib.rs`, which
   stays export-only even for a probe:

   ```rust
   //! Probe. Deleted in step 4.

   /// Probe.
   #[must_use]
   pub const fn probe() -> f64 {
       1.5
   }
   ```

   and add `pub mod probe;` to `crates/buildl-core/src/lib.rs`, then:

   ```
   $ cargo make guard-core-purity
   [cargo-make] INFO - Build Done in …
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings
       Finished `dev` profile …
   ```

   Both pass, which is the hole this task closes.

2. Append to the `disallowed-types` array in `crates/buildl-core/clippy.toml`, after the
   `std::os::unix::fs::DirBuilderExt` entry that currently ends it:

   ```toml
       { path = "f32", reason = "no float reaches canonical JSON; a non-finite float serializes as `null`, which would make an action key disagree with the value it was computed from (architecture.md §5 rule 2)" },
       { path = "f64", reason = "no float reaches canonical JSON; a non-finite float serializes as `null`, which would make an action key disagree with the value it was computed from (architecture.md §5 rule 2)" },
   ```

3. Confirm the ban now fires on the probe:

   ```
   $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings
   error: use of a disallowed type `f64`
     --> crates/buildl-core/src/probe.rs:5:25
      |
    5 |   pub const fn probe() -> f64 {
      |                           ^^^
   ```

4. Delete `crates/buildl-core/src/probe.rs` and its `pub mod probe;` line, then confirm the gate is
   green again and that `git status --short` shows no leftover probe:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Record the family in `docs/architecture-building-blocks.md` §1.2. `clippy.toml:21` points at
   §1.2 for "Derivation and per-family counts", and §1.2 (`abb:27-36`) is a four-row dependency
   table that has never held either — a banned primitive is not a dependency, so it cannot become a
   fifth row. Add a short paragraph beneath that table naming the families the config enumerates:

   ```markdown
   The table is the dependency rule. `crates/buildl-core/clippy.toml` is its enumeration, and bans
   nine families by name: the filesystem, the environment, processes, threads, the standard streams,
   the network, the platform filesystem extensions, the clock — and floating-point numbers, which
   are not an I/O surface at all. A float is banned because a non-finite one serializes as JSON
   `null`, so a value could reach an action key differing from the value it was computed from
   (`architecture.md` §5 rule 2). `cargo make guard-core-purity` asserts every one of them.
   ```

6. Confirm the documentation gate and the guards:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `build(repo): ban floating-point numbers in buildl-core`.

---

## Task 2 — The serializer, sorting only

Floats are not handled yet; this task delivers sorted bytes and the two error variants the next
task needs.

**Files:**
- Modify `crates/buildl-core/src/error.rs`
- Create `crates/buildl-core/src/json/mod.rs`
- Create `crates/buildl-core/src/json/canonical.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/json/canonical.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::{to_string, to_vec};
       use std::collections::BTreeMap;

       #[test]
       fn sorts_object_keys() {
           let mut map = BTreeMap::new();
           map.insert("zebra", 1_u32);
           map.insert("alpha", 2_u32);
           map.insert("middle", 3_u32);
           assert_eq!(to_string(&map).unwrap(), r#"{"alpha":2,"middle":3,"zebra":1}"#);
       }

       #[test]
       fn sorts_at_every_level() {
           let mut inner = BTreeMap::new();
           inner.insert("zebra", 1_u32);
           inner.insert("alpha", 2_u32);
           let mut outer = BTreeMap::new();
           outer.insert("outer_z", inner);
           assert_eq!(
               to_string(&outer).unwrap(),
               r#"{"outer_z":{"alpha":2,"zebra":1}}"#
           );
       }

       #[test]
       fn is_independent_of_insertion_order() {
           let mut first = serde_json::Map::new();
           first.insert("b".to_owned(), 1.into());
           first.insert("a".to_owned(), 2.into());
           let mut second = serde_json::Map::new();
           second.insert("a".to_owned(), 2.into());
           second.insert("b".to_owned(), 1.into());
           assert_eq!(
               to_vec(&serde_json::Value::Object(first)).unwrap(),
               to_vec(&serde_json::Value::Object(second)).unwrap()
           );
       }

       #[test]
       fn preserves_integers_at_the_extremes() {
           assert_eq!(to_string(&u64::MAX).unwrap(), "18446744073709551615");
           assert_eq!(to_string(&i64::MIN).unwrap(), "-9223372036854775808");
       }

       #[test]
       fn emits_no_whitespace() {
           assert_eq!(to_string(&vec![1, 2, 3]).unwrap(), "[1,2,3]");
       }
   }
   ```

2. Create `crates/buildl-core/src/json/mod.rs`:

   ```rust
   //! The one way a value in this crate becomes JSON bytes.
   //!
   //! Its own module because every byte of JSON this crate emits must come from one place: the
   //! graph file, the cache file, the event log and the bytes hashed into an action key all go
   //! through it, so the key of a value and the file of a value can never disagree.
   //!
   //! Responsibilities: [`canonical`] — sorted-key, float-free serialization.
   //!
   //! Non-responsibilities: writing anything anywhere. This module returns bytes; a storage adapter
   //! decides where they land.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod canonical;
   ```

   and add to `crates/buildl-core/src/lib.rs`:

   ```rust
   pub mod json;
   ```

   No `pub use` here: the call reads `json::canonical::to_vec(&value)`, which says at the call site
   that the bytes are the canonical form.

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved imports `super::to_string`, `super::to_vec`
    --> crates/buildl-core/src/json/canonical.rs:10:17
   ```

4. Add the two variants to `crates/buildl-core/src/error.rs`'s `Error` enum, after `InvalidName`:

   ```rust
       /// A value could not be serialized as JSON at all.
       #[error("value could not be serialized as canonical JSON")]
       CanonicalJson {
           /// The underlying `serde_json` failure.
           #[source]
           source: serde_json::Error,
       },
       /// A floating-point number reached the canonical serializer.
       #[error("floating-point value at {path} cannot appear in canonical JSON")]
       FloatRejected {
           /// Where in the value the float was found, as a JSONPath-style location.
           path: String,
       },
   ```

   `FloatRejected` is declared now and raised in task 3.

5. Replace the placeholder doc comment in `crates/buildl-core/src/json/canonical.rs` and add the
   implementation above the test module:

   ```rust
   //! Sorted-key, float-free JSON serialization.
   //!
   //! Its own file because the determinism rule it implements belongs in one place. Sorting is not
   //! implemented here: routing a value through `serde_json::to_value` yields byte-order key
   //! sorting at every level, because with `preserve_order` off a `serde_json::Map` is a
   //! `BTreeMap`. What this file adds is the refusal of anything that would make the bytes
   //! disagree with the value.
   //!
   //! Responsibilities: [`to_vec`] and [`to_string`].
   //!
   //! Non-responsibilities: deciding what to serialize, and where the bytes go.

   use serde::Serialize;

   use crate::error::{Error, Result};

   /// Serializes `value` as canonical JSON bytes: object keys sorted at every level.
   ///
   /// # Errors
   ///
   /// Returns [`Error::CanonicalJson`] when the value cannot become JSON at all.
   pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
       let tree = serde_json::to_value(value).map_err(|source| Error::CanonicalJson { source })?;
       serde_json::to_vec(&tree).map_err(|source| Error::CanonicalJson { source })
   }

   /// Serializes `value` as a canonical JSON string.
   ///
   /// # Errors
   ///
   /// Returns [`Error::CanonicalJson`] when the value cannot become JSON at all.
   pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
       let tree = serde_json::to_value(value).map_err(|source| Error::CanonicalJson { source })?;
       serde_json::to_string(&tree).map_err(|source| Error::CanonicalJson { source })
   }
   ```

6. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 5
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

7. Commit `feat(buildl-core): serialize every value through one sorted-key serializer`.

---

## Task 3 — Refuse floats at the boundary

**Files:**
- Modify `crates/buildl-core/src/json/canonical.rs`

**Steps:**

1. Add the failing tests to the existing test module. Extend its imports to
   `use super::{to_string, to_vec}; use crate::error::Error;`:

   ```rust
   #[test]
   fn rejects_a_finite_float_and_names_where_it_was() {
       let err = to_vec(&1.5).unwrap_err();
       assert!(matches!(err, Error::FloatRejected { ref path } if path == "$"));
   }

   #[test]
   fn rejects_a_whole_valued_float_too() {
       assert!(to_vec(&3.0).is_err());
   }

   #[test]
   fn names_a_float_inside_an_object_and_an_array() {
       let nested: serde_json::Value = serde_json::json!({ "timings": { "elapsed": 1.5 } });
       let err = to_vec(&nested).unwrap_err();
       assert!(matches!(err, Error::FloatRejected { ref path } if path == "$.timings.elapsed"));

       let in_array: serde_json::Value = serde_json::json!([1, 2.5]);
       let err = to_vec(&in_array).unwrap_err();
       assert!(matches!(err, Error::FloatRejected { ref path } if path == "$[1]"));
   }

   #[test]
   fn leaves_every_other_value_alone() {
       assert!(to_vec(&vec![1, 2, 3]).is_ok());
       assert!(to_string(&"text").is_ok());
       assert!(to_vec(&serde_json::Value::Null).is_ok());
       assert!(to_vec(&true).is_ok());
   }
   ```

   Every float literal here is un-annotated, which is the only form the `f32`/`f64` ban from task 1
   permits in this crate. Writing `1.5f64` or `let x: f64 = 1.5` fails the gate.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   ---- json::canonical::tests::rejects_a_finite_float_and_names_where_it_was stdout ----
   thread 'json::canonical::tests::rejects_a_finite_float_and_names_where_it_was' panicked at
   crates/buildl-core/src/json/canonical.rs:…:
   called `Result::unwrap_err()` on an `Ok` value: [49, 46, 53]
   ```

3. Add the walk to `crates/buildl-core/src/json/canonical.rs` and call it from both entry points.
   Insert after the `use` block:

   ```rust
   /// Refuses any floating-point number in `tree`, naming its location.
   fn reject_floats(tree: &serde_json::Value, path: &str) -> Result<()> {
       match tree {
           serde_json::Value::Number(number) if number.is_f64() => Err(Error::FloatRejected {
               path: path.to_owned(),
           }),
           serde_json::Value::Array(items) => {
               for (index, item) in items.iter().enumerate() {
                   reject_floats(item, &format!("{path}[{index}]"))?;
               }
               Ok(())
           }
           serde_json::Value::Object(entries) => {
               for (key, entry) in entries {
                   reject_floats(entry, &format!("{path}.{key}"))?;
               }
               Ok(())
           }
           _ => Ok(()),
       }
   }
   ```

   and add `reject_floats(&tree, "$")?;` between the two statements of each of `to_vec` and
   `to_string`. Extend both doc comments' `# Errors` sections:

   ```rust
   /// Returns [`Error::CanonicalJson`] when the value cannot become JSON at all, and
   /// [`Error::FloatRejected`] when it holds a floating-point number.
   ```

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 4
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): refuse a floating-point value at the JSON boundary`.

---

## Task 4 — Record what the walk cannot catch

A documentation-only task, and the one that keeps the module honest about its own limits.

**Files:**
- Modify `crates/buildl-core/src/json/canonical.rs`

**Steps:**

1. Add a `# What this module does and does not guarantee` section to the module doc comment, below
   the existing `Non-responsibilities` paragraph:

   ```rust
   //! # What this module does and does not guarantee
   //!
   //! No float can reach here from a type in this crate: `f32` and `f64` are banned by
   //! `crates/buildl-core/clippy.toml`, asserted by `cargo make guard-core-purity`. That ban, not
   //! the walk below, is the real guarantee.
   //!
   //! The walk is the second line, for a float arriving through a dependency's `Serialize`
   //! implementation. It catches every finite float, whole-valued ones included. It cannot catch a
   //! non-finite one: `serde_json` converts `NaN` and `±Infinity` to `Value::Null` before any walk
   //! sees them, so such a value is indistinguishable from a genuine `null` at this point.
   //!
   //! Two further residues, neither currently reachable:
   //!
   //! - A non-string map key is stringified silently — a `BTreeMap<u32, _>` renders as `{"7":…}`.
   //!   The rule that every domain newtype serializes as its `Display` string is what keeps the
   //!   maps in this crate honest; nothing mechanically prevents a future type keying on a number.
   //! - Float *formatting*, were a float ever permitted, would be `serde_json`'s own and therefore
   //!   pinned to a dependency version rather than to a specification.
   ```

2. Confirm the documentation gate, which is the only thing this task can break:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

   `RUSTDOCFLAGS="-D warnings"` is what makes the `doc` step a gate (`Makefile.toml:75-85`), so a
   broken intra-doc link here fails the build.

3. Commit `docs(buildl-core): record what the JSON float check cannot catch`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
```

All three exit 0. `cargo make guard-core-purity` runs inside `cargo make dod` and is what proves
the two new `clippy.toml` entries resolve rather than sitting dead.

At the end of this plan `buildl-core` exports `json::canonical::{to_vec, to_string}`, `Error`
carries the two JSON variants, and no type in the crate can hold a float.
