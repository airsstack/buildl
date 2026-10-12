---
status: done
created: 2026-10-11
---

# Cache Row Implementation Plan

**Goal:** A cache row holds the digests of what its action printed.

**Architecture:** One existing type changes, in `types/cache_row.rs`. `CacheRow` gains two private fields, `stdout` and `stderr`, each the digest of what the action wrote to that stream, and `CacheRow::new` takes them as its third and fourth arguments. It still returns `None` when the outputs do not hold one digest per declared output; the two new digests have no rule to break. Two accessors return them — `stdout()` and `stderr()`, by value, because a `Digest` is `Copy` — and `digests()` yields every digest the row holds: the outputs in declared order, then standard output, then standard error. That list is what a later build needs to still have before it may treat the target as already built. A build that finds a target unchanged can then report what its action printed without running it. Every existing call of `CacheRow::new` is in test code, in three files; all nine are updated in this one task, because the crate does not build in between.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D6, D9), §3.7 (the cache row), §9 (the row for `types/cache_row.rs`).

**Checkpoints:** review once, when the task is done.

---

## Context an implementer needs

What exists that this plan changes or builds on:

| Item | Where | Used as |
|---|---|---|
| `CacheRow { key, outputs }`, `CacheRow::new(key, outputs) -> Option<Self>`, `key()`, `produced()` | `crates/buildl-core/src/types/cache_row.rs` | the type that gains two fields, two arguments and three methods |
| `Digest` (`Copy`; `Digest::of(&[u8])`) | `crates/buildl-core/src/types/digest.rs` | the two new fields |
| The `key` helper of the test module (a key declaring the given outputs) | `crates/buildl-core/src/types/cache_row.rs` | kept as it is; the new `row` helper is built on it |

The nine calls of `CacheRow::new` before this plan, found with `grep -rn 'CacheRow::new(' crates`:

| Call | In |
|---|---|
| `crates/buildl-core/src/types/cache_row.rs:95`, `:96`, `:97`, `:98`, `:99` | the test `a_row_needs_exactly_one_digest_per_declared_output` |
| `crates/buildl-core/src/types/cache_row.rs:104` | the test `a_row_keeps_the_key_it_was_built_under` |
| `crates/buildl-core/src/types/cache_row.rs:112` | the test `produced_pairs_each_output_name_with_its_digest_in_declared_order` |
| `crates/buildl-core/src/plan/verdict.rs:184` | the `row` helper of that file's test module |
| `crates/buildl-core/tests/flows/plan.rs:144` | the `record` helper of the plan flows |

No other crate of the workspace names `CacheRow::new`: the search above covers `crates/buildl-lua`, `crates/buildl` and `crates/buildl-cli` too.

Gate facts that bite here:

- Once `CacheRow::new` takes four arguments, the library's test build fails until
  `plan/verdict.rs` is updated, and the flows fail to build until `tests/flows/plan.rs` is. The
  task therefore changes all three files before it runs anything green.
- rustfmt breaks the four-argument signature of `new` over several lines. The block below is in
  that form.
- `digests` returns `impl Iterator`, which is already marked as a value that must be used, and
  the gate passed without a `#[must_use]` on the method.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after the task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/cache_row.rs   — [modify] two fields, the new constructor, stdout, stderr, digests, unit tests
crates/buildl-core/src/plan/verdict.rs      — [modify] the one CacheRow::new call in its tests
crates/buildl-core/tests/flows/plan.rs      — [modify] the one CacheRow::new call in its helper
```

### Task 1 — Keep what an action printed in its cache row

**Files:**
- Modify `crates/buildl-core/src/types/cache_row.rs`
- Modify `crates/buildl-core/src/plan/verdict.rs`
- Modify `crates/buildl-core/tests/flows/plan.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/types/cache_row.rs`, keep the imports and the `key` helper, and replace the three tests below `key` with a `row` helper and seven tests, so the end of the file reads:

   ```rust
       /// A row under a key declaring `outputs`, holding `digests` for them, for an action that
       /// printed `compiling` and warned `careful`.
       fn row(outputs: &[&str], digests: Vec<Digest>) -> Option<CacheRow> {
           CacheRow::new(
               key(outputs),
               digests,
               Digest::of(b"compiling"),
               Digest::of(b"careful"),
           )
       }

       #[test]
       fn a_row_needs_exactly_one_digest_per_declared_output() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           assert!(row(&["app", "app.map"], vec![app]).is_none());
           assert!(row(&["app", "app.map"], vec![app, map, app]).is_none());
           assert!(row(&["app", "app.map"], vec![app, map]).is_some());
           assert!(row(&[], Vec::new()).is_some());
           assert!(row(&[], vec![app]).is_none());
       }

       #[test]
       fn a_row_keeps_the_key_it_was_built_under() {
           let row = row(&["app"], vec![Digest::of(b"app")]).unwrap();
           assert_eq!(row.key(), &key(&["app"]));
       }

       #[test]
       fn produced_pairs_each_output_name_with_its_digest_in_declared_order() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           let row = row(&["z.out", "a.out"], vec![app, map]).unwrap();
           let produced: Vec<(String, Digest)> = row
               .produced()
               .into_iter()
               .map(|output| (output.name.to_string(), output.digest))
               .collect();
           assert_eq!(
               produced,
               [("z.out".to_owned(), app), ("a.out".to_owned(), map)]
           );
       }

       #[test]
       fn a_row_keeps_the_digests_of_what_the_action_printed() {
           let row = row(&["app"], vec![Digest::of(b"app")]).unwrap();
           assert_eq!(row.stdout(), Digest::of(b"compiling"));
           assert_eq!(row.stderr(), Digest::of(b"careful"));
       }

       #[test]
       fn digests_lists_the_outputs_in_declared_order_then_what_was_printed() {
           let app = Digest::of(b"app");
           let map = Digest::of(b"map");
           let row = row(&["z.out", "a.out"], vec![app, map]).unwrap();
           let digests: Vec<Digest> = row.digests().collect();
           assert_eq!(
               digests,
               [app, map, Digest::of(b"compiling"), Digest::of(b"careful")]
           );
       }

       #[test]
       fn a_row_with_no_output_still_holds_what_was_printed() {
           let row = row(&[], Vec::new()).unwrap();
           let digests: Vec<Digest> = row.digests().collect();
           assert_eq!(digests, [Digest::of(b"compiling"), Digest::of(b"careful")]);
       }

       #[test]
       fn two_rows_differing_only_in_what_was_printed_are_not_equal() {
           let app = Digest::of(b"app");
           let quiet = CacheRow::new(key(&["app"]), vec![app], Digest::of(b""), Digest::of(b""));
           assert_ne!(row(&["app"], vec![app]), quiet);
       }
   }
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::cache_row
   error[E0061]: this function takes 2 arguments but 4 arguments were supplied
   error[E0599]: no method named `stdout` found for struct `CacheRow` in the current scope
   error[E0599]: no method named `stderr` found for struct `CacheRow` in the current scope
   error[E0599]: no method named `digests` found for struct `CacheRow` in the current scope
   error[E0599]: no method named `digests` found for struct `CacheRow` in the current scope
   error[E0061]: this function takes 2 arguments but 4 arguments were supplied
   ```

3. Change the type. In the same file, replace everything above the test module with:

   ```rust
   //! A cache row: what the last successful build of one target recorded.
   //!
   //! Its own file because a row is what a later plan compares against: it holds the key the
   //! target was built under, to say whether anything changed, and the digests of what the build
   //! produced, which the targets depending on it key on. It also holds the digests of what the
   //! action printed, so a build that finds the target unchanged can report the same output
   //! without running it.
   //!
   //! Responsibilities: [`CacheRow`], its constructor, pairing its outputs with their names, and
   //! listing every digest it holds.
   //!
   //! Non-responsibilities: storing rows, and deciding what a row means for the next build.

   use crate::types::{ActionKey, Digest, Produced};

   /// What the last successful build of one target recorded: its key, what it produced, and what
   /// its action printed.
   ///
   /// A row holds one digest per output the key declares, in the same order. That is checked once,
   /// by [`CacheRow::new`], so every reader may pair names with digests without checking again.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct CacheRow {
       key: ActionKey,
       outputs: Vec<Digest>,
       stdout: Digest,
       stderr: Digest,
   }

   impl CacheRow {
       /// A row for a build under `key` that produced `outputs`, one digest per declared output in
       /// declared order, and whose action wrote the content `stdout` to standard output and
       /// `stderr` to standard error; `None` when `outputs` does not hold exactly one digest per
       /// output.
       #[must_use]
       pub fn new(
           key: ActionKey,
           outputs: Vec<Digest>,
           stdout: Digest,
           stderr: Digest,
       ) -> Option<Self> {
           (outputs.len() == key.components().own.outputs.len()).then_some(Self {
               key,
               outputs,
               stdout,
               stderr,
           })
       }

       /// The key the target was built under.
       #[must_use]
       pub const fn key(&self) -> &ActionKey {
           &self.key
       }

       /// The digest of what the action wrote to standard output.
       #[must_use]
       pub const fn stdout(&self) -> Digest {
           self.stdout
       }

       /// The digest of what the action wrote to standard error.
       #[must_use]
       pub const fn stderr(&self) -> Digest {
           self.stderr
       }

       /// Every digest the row holds: the outputs in declared order, then standard output, then
       /// standard error.
       ///
       /// These are the contents a build needs to still have before it may treat the target as
       /// already built.
       pub fn digests(&self) -> impl Iterator<Item = Digest> {
           self.outputs
               .iter()
               .copied()
               .chain([self.stdout, self.stderr])
       }

       /// Each declared output with the digest of what was produced under that name, in declared
       /// order.
       #[must_use]
       pub fn produced(&self) -> Vec<Produced> {
           self.key
               .components()
               .own
               .outputs
               .iter()
               .zip(&self.outputs)
               .map(|(name, digest)| Produced {
                   name: name.clone(),
                   digest: *digest,
               })
               .collect()
       }
   }
   ```

4. Update the call in `crates/buildl-core/src/plan/verdict.rs`, in the `row` helper of its test module:

   ```diff
   --- a/crates/buildl-core/src/plan/verdict.rs
   +++ b/crates/buildl-core/src/plan/verdict.rs
   @@ -179,9 +179,11 @@ mod tests {
            ActionKey::new(KeyComponents { own, deps }).unwrap()
        }

   -    /// The row of a build under `key` whose one output had the content `content`.
   +    /// The row of a build under `key` whose one output had the content `content`, and whose
   +    /// action printed nothing.
        fn row(key: ActionKey, content: &str) -> CacheRow {
   -        CacheRow::new(key, vec![Digest::of(content.as_bytes())]).unwrap()
   +        let nothing = Digest::of(b"");
   +        CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
        }

        fn clean<'a>(id: u32, label: &'a Label, name: &str, content: &str) -> Dependency<'a> {
   ```

5. Update the call in `crates/buildl-core/tests/flows/plan.rs`, in its `record` helper:

   ```diff
   --- a/crates/buildl-core/tests/flows/plan.rs
   +++ b/crates/buildl-core/tests/flows/plan.rs
   @@ -112,7 +112,7 @@ const fn own_of(verdict: &Verdict) -> &OwnComponents {

    /// Records one target as built, as an executor would: under the key `plan` holds for it, finished
    /// with what its dependencies' rows say they produced, with every output's content derived from
   -/// `version`.
   +/// `version`, for an action that printed nothing.
    fn record(
        pipeline: &mut Pipeline<FakePorts>,
        graph: &TargetGraph,
   @@ -141,7 +141,8 @@ fn record(
            .map(|name| Digest::of(format!("{label} {name} {version}").as_bytes()))
            .collect();
        let key = ActionKey::new(KeyComponents { own, deps }).unwrap();
   -    let row = CacheRow::new(key, outputs).unwrap();
   +    let nothing = Digest::of(b"");
   +    let row = CacheRow::new(key, outputs, nothing, nothing).unwrap();
        pipeline.ports_mut().cache.rows.insert(label, row);
    }
   ```

6. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::cache_row
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 337 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree with plans `01` to `04` landed; none of them changes a manifest. The gate's test step is what runs the tests of `plan/verdict.rs` and the plan flows against the changed constructor.

8. Commit:

   ```
   $ git add crates/buildl-core/src/types/cache_row.rs crates/buildl-core/src/plan/verdict.rs crates/buildl-core/tests/flows/plan.rs
   $ git commit -m "feat(buildl-core): keep what an action printed in its cache row"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `CacheRow::new(key, outputs, stdout, stderr)` returns `None` with too few or too many output
  digests and `Some` with exactly one per declared output, as before.
- `stdout()` and `stderr()` return the two digests the row was built with; `digests()` yields the
  outputs in declared order, then `stdout`, then `stderr`, and yields those two alone for a row
  with no output.
- Two rows that differ only in what was printed are not equal.
- No call of `CacheRow::new` with two arguments is left: `grep -rn 'CacheRow::new(' crates`
  finds the constructor called in `types/cache_row.rs` (the `row` helper and one test), in
  `plan/verdict.rs` and in `tests/flows/plan.rs`, each with four arguments.
- `crates/buildl-core/Cargo.toml` and `crates/expected-module-edges.txt` are unchanged.

## Review findings

Reviewed once, 2026-10-12. The reviewer re-ran the gate: `cargo make dod` exit 0
(`[cargo-make] INFO - Build Done in 5.22 seconds.`), `cargo deny check` exit 0
(`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan: the three
files match the plan's code blocks and diffs; no drift.

- nit — `CacheRow::new`'s doc says the action "wrote the content `stdout`", where the parameter
  is the digest of that content — `types/cache_row.rs`. Left as it is: the plan's own text.
- nit — `CacheRow::new` takes two `Digest` parameters side by side, so a swapped call compiles
  — `types/cache_row.rs`. Left as it is: spec §3.7 fixes this signature.
- nit — the test `two_rows_differing_only_in_what_was_printed_are_not_equal` compares two
  `Option<CacheRow>`, so it would also pass if one constructor returned `None` —
  `types/cache_row.rs`. Left as it is: both are `Some`, and the plan's own text.

## Probe results

No separate probe was run. The facts the task asserts about existing code (the nine calls of
`CacheRow::new`, the `row` and `record` helpers) are ones its own compile and test runs depend
on:

- Red: `cargo test -p buildl-core --lib types::cache_row` failed to build with `E0061`
  ("this function takes 2 arguments but 4 arguments were supplied", twice) and `E0599` for
  `stdout`, `stderr` and `digests`; "could not compile `buildl-core` (lib test) due to 6
  previous errors".
- Green: `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 337 filtered out` — as
  the plan quotes.

## Deviations

- 2026-10-12 — the task's commit step was skipped. The work is in the tree, uncommitted;
  committing is left to the author.
