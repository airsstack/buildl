---
status: done
created: 2026-10-10
---

# Setting References Implementation Plan

**Goal:** An `Argument` reports the build-setting names its `$opt:` references hold.

**Architecture:** `Argument::setting_references` scans the argument's own text, so it lives on `Argument` (crate rule 2: a type may decide things about its own kind). The byte class of a setting name already exists inside `grammar::key` as a closure; task 1 names it `is_key_byte` so the scanner and the grammar share one definition. The scanner splits on the literal `$opt:` and takes the longest run of key bytes after it; an empty run is not a reference. It returns names as written, as `&str`, because a run longer than 256 bytes is a reference that no `SettingName` can hold.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §3.4 (`Argument::setting_references`), §9 (the `types/argument.rs` row); decision D6.

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What already exists and is used here:

| Item | Where | Used as |
|---|---|---|
| `Argument(String)`, `Argument::as_str` | `crates/buildl-core/src/types/argument.rs` | the type the method is added to |
| `grammar::key` | `crates/buildl-core/src/types/grammar.rs:68` | the grammar whose byte class task 1 names |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- A test iterating a list of byte literals such as `[b'a', b'Z']` fails clippy's
  `byte_char_slices`; the tests below iterate `*b"aZ09-_"` instead.
- `setting_references` is public, so it needs `#[must_use]` and a doc example; the example is a
  doctest and runs in the gate.
- `split_at(end)` is safe at `end` because every byte before it is ASCII.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/grammar.rs    — [modify] is_key_byte, used by key (task 1)
crates/buildl-core/src/types/argument.rs   — [modify] Argument::setting_references and its tests (task 2)
```

### Task 1 — Name the byte class of a key

**Files:**
- Modify `crates/buildl-core/src/types/grammar.rs`

**Steps:**

1. Write the failing test. In the test module of `crates/buildl-core/src/types/grammar.rs`, import the function that does not exist yet. Replace:

   ```rust
       use super::{key, name, path, text};
   ```

   with:

   ```rust
       use super::{is_key_byte, key, name, path, text};
   ```

2. Add the test in the same module. Insert:

   ```rust
       #[test]
       fn key_bytes_are_ascii_letters_digits_dash_and_underscore() {
           for byte in *b"aZ09-_" {
               assert!(is_key_byte(byte), "{:?} is a key byte", char::from(byte));
           }
           for byte in *b".:/ $" {
               assert!(
                   !is_key_byte(byte),
                   "{:?} is not a key byte",
                   char::from(byte)
               );
           }
           assert!(!is_key_byte(0xC3), "a non-ASCII byte is not a key byte");
       }
   ```

   immediately before:

   ```rust
       #[test]
       fn text_rejects_only_a_nul_byte() {
   ```

3. Run it and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib types::grammar
   error[E0432]: unresolved import `super::is_key_byte`
   ```

4. Add the function. Insert:

   ```rust
   /// Whether `byte` may appear in a key.
   pub(super) const fn is_key_byte(byte: u8) -> bool {
       byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
   }
   ```

   immediately before:

   ```rust
   /// A non-empty workspace-relative path:
   ```

5. Use it in `key`, so the class has one definition. Replace:

   ```rust
       if !raw
           .bytes()
           .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
       {
           return Err("may hold only ASCII letters, digits, '-' and '_'");
       }
   ```

   with:

   ```rust
       if !raw.bytes().all(is_key_byte) {
           return Err("may hold only ASCII letters, digits, '-' and '_'");
       }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::grammar
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 139 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

8. Commit:

   ```
   $ git add crates/buildl-core/src/types/grammar.rs
   $ git commit -m "refactor(buildl-core): name the byte class of a key"
   ```

### Task 2 — Find the build settings an argument refers to

**Files:**
- Modify `crates/buildl-core/src/types/argument.rs`

**Steps:**

1. Write the failing tests, in the test module of `crates/buildl-core/src/types/argument.rs`. Insert:

   ```rust
       fn references(raw: &str) -> Vec<String> {
           Argument::parse(raw)
               .unwrap()
               .setting_references()
               .into_iter()
               .map(str::to_owned)
               .collect()
       }

       #[test]
       fn setting_references_are_found_in_order() {
           assert_eq!(references("$opt:test_filter"), ["test_filter"]);
           assert_eq!(references("--run=$opt:a.$opt:b-c"), ["a", "b-c"]);
           assert_eq!(references("$opt:A9_x/tail"), ["A9_x"]);
           assert_eq!(references("$opt:a$opt:b"), ["a", "b"]);
       }

       #[test]
       fn text_that_names_no_setting_is_not_a_reference() {
           for raw in ["$opt:", "$opt:.x", "$option", "opt:x", "$in", "", "$opt"] {
               assert!(references(raw).is_empty(), "{raw:?} holds no reference");
           }
       }

       #[test]
       fn a_marker_with_no_name_does_not_hide_a_later_reference() {
           assert_eq!(references("$opt:.$opt:x"), ["x"]);
           assert_eq!(references("$opt:$opt:x"), ["x"]);
       }

       #[test]
       fn a_reference_longer_than_a_setting_name_is_returned_whole() {
           let name = "a".repeat(257);
           assert_eq!(references(&format!("$opt:{name}")), [name]);
       }
   ```

   immediately before:

   ```rust
       #[test]
       fn command_keeps_its_arguments_in_order() {
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::argument
   error[E0599]: no method named `setting_references` found for struct `Argument` in the current scope
   ```

3. Update the module doc. Replace:

   ```rust
   //! Responsibilities: [`Argument`] and [`Command`], their constructors, and their rendering.
   //!
   //! Non-responsibilities: placeholder expansion. `$in`, `$out`, `$deps` and `$opt:name` stay text
   //! here; the executor expands them when the action runs.
   ```

   with:

   ```rust
   //! Responsibilities: [`Argument`] and [`Command`], their constructors, their rendering, and
   //! finding the build settings an argument refers to.
   //!
   //! Non-responsibilities: placeholder expansion. `$in`, `$out`, `$deps` and `$opt:name` stay text
   //! here; the executor expands them when the action runs. Whether a setting an argument refers to
   //! is declared belongs to the phase that builds the graph.
   ```

4. Add the marker constant. Insert:

   ```rust
   /// The text that starts a reference to a build setting.
   const SETTING_MARKER: &str = "$opt:";
   ```

   immediately before:

   ```rust
   /// One element of a command's argument vector, such as `cc` or `$in`.
   ```

5. Add the method to `impl Argument`. Insert:

   ````rust

       /// The build-setting names this argument refers to, in the order they appear.
       ///
       /// A reference is `$opt:` followed by a name: the longest run of ASCII letters, digits, `-`
       /// and `_`. An `$opt:` with no such character after it is not a reference and stays text.
       /// A name is returned as written, so it may be longer than a setting name can be.
       ///
       /// # Examples
       ///
       /// ```
       /// use buildl_core::Argument;
       ///
       /// let argument = Argument::parse("--run=$opt:test_filter")?;
       /// assert_eq!(argument.setting_references(), ["test_filter"]);
       /// # Ok::<(), buildl_core::Error>(())
       /// ```
       #[must_use]
       pub fn setting_references(&self) -> Vec<&str> {
           let mut references = Vec::new();
           let mut rest = self.0.as_str();
           while let Some((_, after)) = rest.split_once(SETTING_MARKER) {
               let end = after
                   .bytes()
                   .position(|byte| !grammar::is_key_byte(byte))
                   .unwrap_or(after.len());
               let (name, tail) = after.split_at(end);
               if !name.is_empty() {
                   references.push(name);
               }
               rest = tail;
           }
           references
       }
   ````

   immediately after:

   ```rust
       pub fn as_str(&self) -> &str {
           &self.0
       }
   ```

6. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::argument
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 139 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

8. Commit:

   ```
   $ git add crates/buildl-core/src/types/argument.rs
   $ git commit -m "feat(buildl-core): find the build settings an argument refers to"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- The four new `types::argument` tests cover every row of spec §3.4's table and a run of 257
  bytes.
- `grammar::key` still rejects what it rejected before: its existing test is unchanged and passes.

## Review findings

One reviewer pass over both tasks' files, 2026-10-10. Verdict: spec compliant, blocking set empty. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0; `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). Every code block of both tasks is in the tree as written.

- nit — `split_at(end)` panics off a character boundary and is safe only because `grammar::is_key_byte` accepts ASCII bytes alone; that reason is stated nowhere in the source, and no test puts a multi-byte character straight after a name. Correct today. Not fixed — `crates/buildl-core/src/types/argument.rs:80`
- nit — the module doc still says the module holds "the four shared grammars" and that "each function only says why a candidate is rejected"; `is_key_byte` is now handed to a sibling and returns `bool`. This plan prescribed no doc edit there. Not fixed — `crates/buildl-core/src/types/grammar.rs:8`

## Probe results

No separate probe was run. Every fact the tasks assert about existing code is put under test by their own cycles, and all four runs matched the plan:

- task 1 red — `cargo test -p buildl-core --lib types::grammar` — ``error[E0432]: unresolved import `super::is_key_byte` ``
- task 1 green — `cargo test -p buildl-core --lib types::grammar` — `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 139 filtered out`
- task 2 red — `cargo test -p buildl-core --lib types::argument` — ``error[E0599]: no method named `setting_references` found for struct `Argument` in the current scope``
- task 2 green — `cargo test -p buildl-core --lib types::argument` — `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 139 filtered out`

## Deviations

- 2026-10-10 — neither task's commit step was run during execution. The commit is the author's to make; each task's file was left in the working tree.
