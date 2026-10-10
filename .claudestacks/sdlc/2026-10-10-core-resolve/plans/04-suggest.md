---
status: approved
created: 2026-10-10
---

# Suggest Implementation Plan

**Goal:** The nearest declared name to a misspelled one is found by edit distance.

**Architecture:** A new phase module `resolve/` starts here with one private file, `suggest.rs`. `nearest` is generic over anything that renders and orders, so the same function serves labels and setting names; the caller chooses the candidates. Distance is Levenshtein over bytes, two rows at a time, because every name compared is ASCII by its grammar. A candidate is offered when its distance is at most a third of the wanted name's length and never less than one edit; ties go to the lowest candidate by its own `Ord`, so the answer does not depend on the order candidates arrive in.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §4.3 (the Suggestions paragraph), §8 (module layout), §9 (the `resolve/suggest.rs` row); decision D11.

**Checkpoints:** review once, when task 1 is done.

---

## Context an implementer needs

Nothing from another plan of this chain is needed. `crates/buildl-core/src/resolve/` does not
exist yet; this plan creates it.

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- Nothing outside its own tests calls this file's functions until plan `06`, so the library
  build reports them as dead code and the gate fails. The `mod` line therefore carries
  `cfg_attr(not(test), expect(dead_code, …))`. It must be `cfg_attr(not(test), …)`: in the test
  build the functions are used, and a plain `expect` would fail there as unfulfilled. Plan `06`
  removes the attribute.
- `resolve/mod.rs` is export-only: docs and `mod` lines, nothing else.
- `pub mod resolve;` goes between `pub mod ports;` and `pub mod types;` in `lib.rs`. The crate
  doc's module table gains its `resolve` row in plan `09`; the gate does not need it.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/resolve/mod.rs       — [create] the phase module, declaring suggest
crates/buildl-core/src/resolve/suggest.rs   — [create] nearest, distance, unit tests
crates/buildl-core/src/lib.rs               — [modify] pub mod resolve;
```

### Task 1 — Add the `resolve` module with `nearest`

**Files:**
- Create `crates/buildl-core/src/resolve/mod.rs`
- Create `crates/buildl-core/src/resolve/suggest.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/resolve/mod.rs`:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod suggest;
   ```

2. Declare the module in `crates/buildl-core/src/lib.rs`. Replace:

   ```rust
   pub mod ports;
   pub mod types;
   ```

   with:

   ```rust
   pub mod ports;
   pub mod resolve;
   pub mod types;
   ```

3. Write the failing tests. Create `crates/buildl-core/src/resolve/suggest.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use super::{distance, nearest};

       #[test]
       fn distance_counts_single_byte_edits() {
           assert_eq!(distance("", ""), 0);
           assert_eq!(distance("abc", "abc"), 0);
           assert_eq!(distance("", "abc"), 3);
           assert_eq!(distance("abc", ""), 3);
           assert_eq!(distance("//:mian.o", "//:main.o"), 2);
           assert_eq!(distance("kitten", "sitting"), 3);
           assert_eq!(distance("test_fliter", "test_filter"), 2);
       }

       #[test]
       fn the_nearest_candidate_within_the_limit_is_returned() {
           let candidates = ["//:main.o", "//:util.o", "//lib:text"].map(str::to_owned);
           assert_eq!(
               nearest("//:mian.o", &candidates),
               Some(&"//:main.o".to_owned())
           );
       }

       #[test]
       fn a_candidate_at_the_limit_is_offered_and_one_past_it_is_not() {
           // Nine bytes allow three edits.
           let at_limit = ["abcdefxyz".to_owned()];
           assert_eq!(nearest("abcdefghi", &at_limit), Some(&at_limit[0]));
           let past_limit = ["abcdewxyz".to_owned()];
           assert_eq!(nearest("abcdefghi", &past_limit), None);
       }

       #[test]
       fn a_short_name_still_allows_one_edit() {
           let candidates = ["b".to_owned()];
           assert_eq!(nearest("a", &candidates), Some(&candidates[0]));
           let far = ["bc".to_owned()];
           assert_eq!(nearest("a", &far), None);
       }

       #[test]
       fn equally_near_candidates_resolve_to_the_lowest_whatever_their_order() {
           let forward = ["//:ab".to_owned(), "//:ac".to_owned()];
           let backward = ["//:ac".to_owned(), "//:ab".to_owned()];
           assert_eq!(nearest("//:aa", &forward), Some(&"//:ab".to_owned()));
           assert_eq!(nearest("//:aa", &backward), Some(&"//:ab".to_owned()));
       }

       #[test]
       fn no_candidates_offer_nothing() {
           let none: [String; 0] = [];
           assert_eq!(nearest("//:app", &none), None);
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib resolve::suggest
   error[E0432]: unresolved imports `super::distance`, `super::nearest`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/resolve/suggest.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The nearest declared name to one that names nothing.
   //!
   //! Its own file because it is the one part of this phase that knows nothing about build files:
   //! it compares rendered names, and the caller decides which names are candidates.
   //!
   //! Responsibilities: [`nearest`], and the edit distance it ranks by.
   //!
   //! Non-responsibilities: choosing the candidates. A dependency may be a target or an alias and a
   //! rule reference only a rule; the caller filters before asking.

   use core::fmt;

   /// The candidate nearest to `wanted`, if one is near enough to be a likely misspelling.
   ///
   /// Distance is the edit distance between the rendered texts. A candidate is near enough when
   /// its distance is at most a third of `wanted`'s length in bytes, and never less than one edit.
   /// Among equally near candidates the lowest by `T`'s own order is returned, so the answer does
   /// not depend on the order `candidates` arrive in.
   pub(crate) fn nearest<'a, T, I>(wanted: &str, candidates: I) -> Option<&'a T>
   where
       T: fmt::Display + Ord + 'a,
       I: IntoIterator<Item = &'a T>,
   {
       let limit = (wanted.len() / 3).max(1);
       candidates
           .into_iter()
           .map(|candidate| (distance(wanted, &candidate.to_string()), candidate))
           .filter(|(distance, _)| *distance <= limit)
           .min()
           .map(|(_, candidate)| candidate)
   }

   /// The Levenshtein distance between `a` and `b`, counted in bytes.
   ///
   /// Bytes rather than characters because every name compared here is ASCII by its grammar.
   fn distance(a: &str, b: &str) -> usize {
       let b = b.as_bytes();
       let mut previous: Vec<usize> = (0..=b.len()).collect();
       for (row, x) in a.bytes().enumerate() {
           let mut current = Vec::with_capacity(b.len() + 1);
           current.push(row + 1);
           for (column, y) in b.iter().enumerate() {
               let substitute = previous[column] + usize::from(x != *y);
               let delete = previous[column + 1] + 1;
               let insert = current[column] + 1;
               current.push(substitute.min(delete).min(insert));
           }
           previous = current;
       }
       previous[b.len()]
   }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::suggest
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 159 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

8. Commit:

   ```
   $ git add crates/buildl-core/src/resolve/mod.rs crates/buildl-core/src/resolve/suggest.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): suggest the nearest declared name"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- The tests pin the distance of known pairs, the limit at its edge, the one-edit floor for a short
  name, ties resolved the same way in either candidate order, and the empty candidate set.
