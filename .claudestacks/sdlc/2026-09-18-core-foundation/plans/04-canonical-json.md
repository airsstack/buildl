---
status: done
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
   //!   What keeps the maps in this crate honest is that the four types able to appear as a map key
   //!   — [`Directory`](crate::Directory), [`TargetName`](crate::TargetName),
   //!   [`Label`](crate::Label) and [`Digest`](crate::Digest) — each serializes as its `Display`
   //!   string. It is not a crate-wide property: [`NodeId`](crate::NodeId) and
   //!   [`Timestamp`](crate::Timestamp) are transparent scalars that serialize as bare numbers, and
   //!   neither keys a map. Nothing mechanically prevents a future type keying on a number.
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

---

## Review findings

- doc-comment-discipline, accuracy (bug — blocking) — the module doc claimed "The rule that every domain newtype serializes as its `Display` string is what keeps the maps in this crate honest", which is false of the shipped crate: `NodeId` and `Timestamp` are `#[serde(transparent)]` over integers (`node_id.rs:19`, `timestamp.rs:26`) and serialize as bare numbers, which task 2's own `serializes_as_a_bare_number` test asserts. The claim mattered because it was the entire basis on which the non-string-map-key residue is called safe — `crates/buildl-core/src/json/canonical.rs:27`. Root cause is upstream: spec §3.4 (`spec.md:333-340`) states the rule correctly and narrowly — "the rule binds **the four types that can appear as a map key or inside a label** — `Directory`, `TargetName`, `Label` and `Digest` … It does not bind the other three, and deliberately so" — while spec §7.2 (`spec.md:616`) mis-restates its own rule as "every `types/` newtype serializes as its `Display` string". This task's prescribed doc text inherited §7.2's wording. Fixed: narrowed to the four map-key types, naming the two transparent-scalar exemptions, with intra-doc links so the claim is checkable from the rendered documentation. Verified: `cargo make dod` → `[cargo-make] INFO - Build Done in 5.73 seconds.` — which also proves the new intra-doc links resolve, since `RUSTDOCFLAGS="-D warnings"` fails the doc step on a broken one.
- cross-reference integrity (risk — blocking) — the §1.2 amendment did not achieve what the spec says it is for. `crates/buildl-core/clippy.toml:21` reads "Derivation and per-family counts: docs/architecture-building-blocks.md §1.2", and `spec.md:607-609` states that §10 records this amendment "that keeps that file's stated derivation true", but the paragraph as written named the families while supplying neither a derivation nor a count — leaving the pointer unsatisfied — `docs/architecture-building-blocks.md:38`. Fixed by adding the counts rather than by softening the `clippy.toml` pointer, since weakening a guard cross-reference is the worse trade. The counts were derived from the file, not computed from memory: `grep -c '{ path = '` → `130`, and `grep -o 'reason = "[^"]*"' | sort | uniq -c` collapsed into filesystem 41, environment 23, threads 20, processes 15, platform filesystem extensions 13, standard streams 10, clock 3, network 3, floats 2 — summing to exactly 130, so the partition is provably the whole config, which is itself the derivation the pointer promises. Verified by the same `cargo make dod` run above.
- accuracy (nit) — "`cargo make guard-core-purity` asserts every one of them" overstated the guard, which asserts that each present entry still resolves, that no source file suppresses the lints, and that the config is not switched off — `docs/architecture-building-blocks.md:43`. Fixed in the same edit, to spec §7.1's own accurate three-part phrasing (`spec.md:593-595`).
- unit-test-mandate (risk) — `Error::CanonicalJson` and `Error::FloatRejected`, both added by this plan, shipped with zero tests, while the file's own `invalid_name_states_what_was_found` sets the convention of asserting a variant's rendered `Display` text; rewording either `#[error]` string would have broken nothing — `crates/buildl-core/src/error.rs:44`. Fixed: `canonical_json_states_the_value_could_not_be_serialized` and `float_rejected_states_where_the_float_was_found`, the latter pinning the `{path}` interpolation as `"floating-point value at $.timings.elapsed cannot appear in canonical JSON"`. Verified: `cargo test -p buildl-core` → `test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`; `cargo make dod` exit 0.
- unit-test-mandate (risk) — the `Error::CanonicalJson` arm of both public functions was never exercised, so nothing caught a reversion of the `map_err` wiring — `crates/buildl-core/src/json/canonical.rs:82`. Fixed: `reports_a_value_that_cannot_become_json_at_all`, driving the arm with `u128::MAX`. The reachability was re-verified on the version actually resolved here rather than taken from the reviewer's run — see Probe results. Verified by the same run above.
- M-DONT-LEAK-TYPES (risk) — `Error::CanonicalJson` carries a `serde_json::Error` field, and enum variant fields are always public, so a downstream `match` names the external type and a `serde_json` major bump becomes a breaking change for this crate — `crates/buildl-core/src/error.rs:39`. **Not fixed; surfaced to the author.** Task 2 step 4 prescribes this shape verbatim and `abb` §1.2 admits `serde_json` as a core dependency, so it is authorized rather than drift — but it is a public-API decision, not an implementation detail to re-engineer inside a fix round.
- reversion guard (nit) — deleting both float entries from `clippy.toml` leaves the entire gate green: `[tasks.guard-core-purity]` asserts only that entries which are present still resolve, never that a family is listed — `crates/buildl-core/clippy.toml:149`. Accepted as-is: spec §12.2 scopes this plan to "no guard task, no golden file".
- diagnostic quality (nit) — the `$.{key}` path form is ambiguous when a key itself holds `.` or `[`: `{"a.b": 1.5}` reports `$.a.b`, indistinguishable from `{"a":{"b":1.5}}`; the JSONPath bracket form `$["a.b"]` removes it — `crates/buildl-core/src/json/canonical.rs:49`. Accepted as-is: diagnostic text only, and the plan prescribes this form. Worth revisiting alongside the other diagnostic-quality item carried from plan `02`.
- modularity (nit) — `to_vec` and `to_string` duplicate the three-line `to_value` / `reject_floats` / `map_err` preamble; a private `fn tree<T: Serialize + ?Sized>(value: &T) -> Result<serde_json::Value>` would leave one site the walk is called from, so a future third entry point could not forget it — `crates/buildl-core/src/json/canonical.rs:64`. Accepted as-is: the plan prescribes both bodies and the shipped code is byte-identical to the approved text.
- plan File map (drift, nit) — two index rows contradict their own task bodies: `canonical.rs … [modify] the float tests (task 4)` where task 4's body is doc-comment-only and the four float tests belong to task 3 step 1, and `lib.rs … declare and re-export json` where task 2 step 2 says "No `pub use` here". The task body was treated as authoritative in both cases, being the only place carrying steps, code and verification. Nothing is missing as a result: all four float tests are present and passing, and the shipped `lib.rs` declares `pub mod json;` with no re-export, so call sites read `json::canonical::to_vec(&value)`.
- spec §10 coverage — no finding. The float-related `architecture.md` §5 rule 2 and §6 amendments are owned by plan `06` task 3 (its lines 237-257), and plan `06` task 4 step 3 re-verifies this §1.2 paragraph. Correctly deferred rather than silently carried.

## Probe results

- **Claim: `serde_json` converts `NaN` and `±Infinity` to `Value::Null` before any walk sees them, so the float walk cannot catch a non-finite float.** This is the module's own documented limitation and task 4 ships it as permanent doc text, while task 4's only verification is `cargo make dod` — which cannot discriminate it either way. Probed with a throwaway crate against the resolved `serde_json` 1.0.151 (`Cargo.lock:625`; the manifest requests "1.0.150"), carrying a finite-float control. Real output:
  ```
  -- claim 1: non-finite floats --
  NaN                                  to_value -> Null   is_null=true  is_f64=false
  Infinity                             to_value -> Null   is_null=true  is_f64=false
  -Infinity                            to_value -> Null   is_null=true  is_f64=false
  1.5 (control, must stay a Number)    to_value -> Number(1.5)   is_null=false  is_f64=true

  -- claim 1b: same values straight to_string --
  NaN                                  to_string -> null
  Infinity                             to_string -> null
  1.5 (control)                        to_string -> 1.5
  ```
  **Came out for the plan.** The control is what makes it evidence: `1.5` survives as `Number(1.5)` with `is_f64() == true`, so the harness is not flattening everything to `Null`. The `Value::Number(number) if number.is_f64()` arm therefore genuinely cannot see a non-finite value, and the documented blind spot is real rather than defensive hedging. Probe deleted.
- **Claim: a non-string map key is stringified silently — a `BTreeMap<u32, _>` renders as `{"7":…}`.** Same probe run. Real output:
  ```
  -- claim 2: integer map key --
  BTreeMap<u32, &str> {7: "x"}         -> {"7":"x"}
  same, to_value                       -> Object {"7": String("x")}
  ```
  **Came out for the plan**, in both the string and the `Value` form.
- **Claim: clippy currently accepts a float in this crate, which is the hole task 1 closes.** This is a before/after claim, so both halves were run. Before the ban, with a throwaway `crates/buildl-core/src/probe.rs` holding `pub const fn probe() -> f64 { 1.5 }` and declared from `lib.rs`:
  ```
  $ cargo make guard-core-purity
  [cargo-make] INFO - Build Done in 0.59 seconds.
  $ cargo clippy -p buildl-core --all-targets --all-features -- -D warnings
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
  ```
  Both green with a `f64` in the crate. After appending the two `disallowed-types` entries, the same clippy invocation:
  ```
  error: use of a disallowed type `f64`
   --> crates/buildl-core/src/probe.rs:5:25
    |
  5 | pub const fn probe() -> f64 {
    |                         ^^^
    = note: no float reaches canonical JSON; a non-finite float serializes as `null`, which would make an action key disagree with the value it was computed from (architecture.md §5 rule 2)
    = note: `-D clippy::disallowed-types` implied by `-D warnings`
  error: could not compile `buildl-core` (lib) due to 1 previous error
  ```
  Matched the plan's predicted error, file, line and column exactly, with the ban's own reason echoed back as the note. Probe file and its `pub mod probe;` line deleted; `git status --short` confirmed no leftover.
- **Claim: `crates/buildl-core/clippy.toml` bans nine families by name, the §1.2 paragraph's enumeration being complete.** An exhaustive-list claim, so checked for completeness rather than only correctness — a list whose every entry is right while three are missing reads as verified and is not. All 16 distinct `reason =` strings were enumerated with `grep -o 'reason = "[^"]*"' | sort | uniq -c` and collapsed into families: filesystem (32 std `fs` entries + 9 `Path` methods), environment (18 + 2 compile-time + 2 PATH-format + 1 `join_paths` error type), threads (19 + 1 `thread::panicking`), processes (14 + 1 own-pid), platform filesystem extensions (13), standard streams (6 + 4 print macros), clock (3), network (3). Eight existing families, and floating-point makes nine. The paragraph's list matches, and nothing is missing from it. **Came out for the plan.**
- **Claim: `RUSTDOCFLAGS="-D warnings"` is what makes the `doc` step a gate, so task 4 can break the build.** Read directly at `Makefile.toml`: `[tasks.doc.env]` sets `RUSTDOCFLAGS = "-D warnings"`, with the file's own comment stating that plain `cargo doc` prints warnings and still exits 0. Confirmed.
- **Claim the plan makes about key sorting — that routing through `serde_json::to_value` yields byte-order sorting at every level, because with `preserve_order` off a `serde_json::Map` is a `BTreeMap`.** Deliberately NOT probed separately: task 2's own three tests (`sorts_object_keys`, `sorts_at_every_level`, `is_independent_of_insertion_order`) discriminate exactly this claim, and they pass. The plan's own red-green cycle is the probe; a second one would have re-verified an established result.

## Deviations

- **2026-10-04 — no commits.** Each task's final step names a commit; none was run. The commit gate belongs to the author, and no agent in this flow runs a commit. Messages held for the author: `build(repo): ban floating-point numbers in buildl-core` (task 1, scope `repo` because it edits a workspace guard config), `feat(buildl-core): serialize every value through one sorted-key serializer` (task 2), `feat(buildl-core): refuse a floating-point value at the JSON boundary` (task 3), `docs(buildl-core): record what the JSON float check cannot catch` (task 4).
- **2026-10-04 — tasks 1 and 4 ran inline rather than through a coder subagent.** Task 1 is a two-line config append plus a documentation paragraph, with no red-green code cycle of its own: its red step is the guard, which the orchestrator ran directly, including the before-state proving clippy accepted a float first. Task 4 is a single prescribed doc-comment block whose only verification is the gate. Delegating either would have cost a spawn and bought nothing.
- **2026-10-04 — this plan ran after plan `03` rather than concurrently with it, though the two are symbol-disjoint.** Neither references the other's types — `grep` for `Digest|NodeId|Provenance|Timestamp` and for `Label|TargetName|Directory` across this plan both returned nothing. They were still serialized because every task in both plans verifies with `cargo test -p buildl-core` over one crate: concurrent coders in a shared compilation unit observe each other's half-written files through their own test runs, so neither can trust a red or green signal. The two plans also both append to `crates/buildl-core/src/lib.rs`. Running `04` second meant its `f32`/`f64` ban was installed after plan `03`'s types were written; none of those types holds a float, so the order changed nothing in the result.
- **2026-10-04 — task 3's red step is a test failure, not a compile error, unlike every other red step in this chain.** The plan predicts `called `Result::unwrap_err()` on an `Ok` value: [49, 46, 53]` and that is exactly what appeared, those bytes being `"1.5"`. Three of the four new tests failed pre-implementation rather than one: `rejects_a_whole_valued_float_too` asserts only `is_err()` so it failed too, while `leaves_every_other_value_alone` passed before and after, correctly, since it holds no floats. Matches in kind; the count differs because the plan quotes only the first failure.
- **2026-10-04 — task 4 was executed from its step body, not its File map row.** The plan's File map lists `crates/buildl-core/src/json/canonical.rs — [modify] the float tests (task 4)`, but task 4's own steps specify only the module-doc addition and no test change. The body was treated as authoritative, since it is the construction text an implementer follows; the float tests had already landed in task 3. Flagged by the task briefer rather than discovered late.
- **2026-10-04 — three tests beyond the plan's count, from the review fix round.** The plan ends at 49 tests; the tree holds 52. Added: `canonical_json_states_the_value_could_not_be_serialized` and `float_rejected_states_where_the_float_was_found` in `error.rs`, and `reports_a_value_that_cannot_become_json_at_all` in `canonical.rs`. They add no behaviour — they pin the two new error messages and the `Error::CanonicalJson` arm, all three of which the plan left unguarded.
- **2026-10-04 — the two blocking findings were fixed inline by the orchestrator rather than routed to a coder.** Both were documentation prose the orchestrator had written itself while executing tasks 1 and 4 from the plan's prescribed text, so the author of the defect was the right party to correct it; the two test findings went to a fresh coder spawn in the same round. The reviewer was not re-spawned over the result, per the one-fix-round budget, and only the gate the fixes could break was re-run.
- **2026-10-04 — a test-only clippy lint surfaced during the fix round.** The first draft of the `CanonicalJson` test used a `match` to reach the `Err` arm; `cargo make dod`'s clippy step flagged `clippy::manual_let_else`, and it was rewritten as `let Err(source) = … else { unreachable!(…) }`. Test code only; no implementation touched. Recorded because it is evidence the `-D warnings` gate covers `--all-targets` as the plan's preamble claims.
- **2026-10-04 — not a deviation, a correction to this record.** An earlier draft of this section claimed the plan and spec cite a `serde_json` version other than the one in use. That was wrong: this plan's own preamble (line 42) cites `1.0.151`, which is exactly what `Cargo.lock:625` resolves, and the spec cites no version. Only `Cargo.toml` requests `"1.0.150"`, and resolving to `1.0.151` under that requirement is ordinary semver behaviour rather than a discrepancy. Every probe and test in this plan ran against `1.0.151`: `NaN`/`±Infinity` → `Value::Null`, integer map keys stringified, and `to_value(u128::MAX)` → `Err(Error("number out of range", line: 0, column: 0))` all hold there. The tested version is recorded here because a future bump is what would invalidate the module's documented limitation.
- **2026-10-04 — this plan's task 4 code block and the spec's §7.2 were amended after execution, on the author's authorization.** The block prescribed the doc sentence "The rule that every domain newtype serializes as its `Display` string", which the review found false of the shipped crate; it now carries the corrected text that actually shipped, naming the four map-key types and the three exemptions. `spec.md` §7.2's restatement was corrected at the same time, since that is where the over-broad wording originated — §3.4 had it right all along. The Review findings entry above quotes the original wording verbatim and is left unchanged, so the record of what was found still reads true.
