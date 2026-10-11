---
status: approved
created: 2026-10-11
---

# Small Values Implementation Plan

**Goal:** The small values an action key, a port and a ceiling are built from exist as validated types.

**Architecture:** Five types in four new files under `types/`, and one method on an existing type. `Program`, `EnvValue` and `Platform` are text newtypes with the shape every name in the crate already has: a private field, a fallible `parse` returning `Error::InvalidName`, `as_str`, `Display`, `FromStr`, and serde through `String`. `FileStat` and `StatEntry` are plain `Copy` structs with public fields. `Directory::contains` is a method because it decides something about two values of one kind, which `types` may hold; comparing a file path with a directory needs two kinds, so that stays out of this plan.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §3.1 (small values), §3.5 (`Directory::contains`), §9 (the rows for `types/program.rs`, `env_value.rs`, `platform.rs` and `types/directory.rs`).

**Checkpoints:** review once, when all six tasks are done.

---

## Context an implementer needs

What exists that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Error::InvalidName { kind, value, reason }`, `NameKind` | `crates/buildl-core/src/error.rs` | what every `parse` here returns on a bad value |
| `grammar::text` (`pub(super)`): rejects a `NUL` byte with `"must not contain a NUL byte"` | `crates/buildl-core/src/types/grammar.rs` | the free-text grammar `Program` and `EnvValue` share with `Argument` |
| `EnvName` | `crates/buildl-core/src/types/env_name.rs` | the model the three new names copy: derives, impls and test layout |
| `Argument` (`as_str`; may be empty; may hold `$`) | `crates/buildl-core/src/types/argument.rs` | what `Program::of` reads |
| `Timestamp` (`Copy`), `Digest` (`Copy`) | `crates/buildl-core/src/types/` | the fields of `FileStat` and `StatEntry` |
| `Directory` (`is_root`, `as_str`, a private `String`) | `crates/buildl-core/src/types/directory.rs` | the type that gains `contains` |

Gate facts that bite here:

- `crates/buildl-core/src/types/mod.rs` and `crates/buildl-core/src/lib.rs` keep their `mod`
  lines and `pub use` lists sorted. `cargo fmt --all` sorts and rewraps a `pub use a::{…}` list
  and a run of `pub mod` or `pub use` lines, so each task adds its line or name anywhere in the
  right list and lets `cargo fmt --all` place it. rustfmt refuses a whole crate while it declares
  a module that has no file (`failed to resolve mod`), so a task runs `cargo fmt --all` only once
  the file it declares exists. Both were checked by replaying tasks 1 and 2 on a clean copy with
  each addition placed last in its list.
- A doc bullet in `types/mod.rs` that links a type that does not exist yet fails `cargo make dod`
  at its rustdoc step. Each task therefore adds only the link for the type it creates.
- `FileStat` and `StatEntry` have no behaviour, so task 5 has no failing-test step: the gate's
  rustdoc and build steps are what check it.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/error.rs             — [modify] three NameKind variants
crates/buildl-core/src/types/program.rs     — [create] Program, its constructors, unit tests
crates/buildl-core/src/types/env_value.rs   — [create] EnvValue, unit tests
crates/buildl-core/src/types/platform.rs    — [create] Platform, unit tests
crates/buildl-core/src/types/file_stat.rs   — [create] FileStat, StatEntry
crates/buildl-core/src/types/directory.rs   — [modify] Directory::contains, unit tests
crates/buildl-core/src/types/mod.rs         — [modify] declare and re-export the four new modules
crates/buildl-core/src/lib.rs               — [modify] re-export the five new types
```

### Task 1 — Name the three new kinds of name

**Files:**
- Modify `crates/buildl-core/src/error.rs`

**Steps:**

1. Write the failing assertions. In `crates/buildl-core/src/error.rs`, in the test `name_kind_renders_a_human_phrase`, add three lines after the `NameKind::FieldName` assertion, so the end of the test reads:

   ```rust
           assert_eq!(NameKind::FieldName.to_string(), "field name");
           assert_eq!(NameKind::Program.to_string(), "program");
           assert_eq!(NameKind::EnvValue.to_string(), "environment variable value");
           assert_eq!(NameKind::Platform.to_string(), "platform");
       }
   ```

2. Run the test and confirm it fails to build:

   ```
   $ cargo test -p buildl-core --lib error::tests::name_kind
   error[E0599]: no variant, associated function, or constant named `Program` found for enum `NameKind` in the current scope
   error[E0599]: no variant, associated function, or constant named `EnvValue` found for enum `NameKind` in the current scope
   error[E0599]: no variant, associated function, or constant named `Platform` found for enum `NameKind` in the current scope
   ```

3. Add the variants. In the same file, extend `enum NameKind` after its last variant, so the end of the enum reads:

   ```rust
       /// A field name in a declaration's option table.
       FieldName,
       /// The name of a program a command runs.
       Program,
       /// An environment variable's value.
       EnvValue,
       /// A host platform: an operating system and an architecture.
       Platform,
   }
   ```

4. Extend `impl fmt::Display for NameKind`, so the end of its `match` reads:

   ```rust
               Self::FieldName => "field name",
               Self::Program => "program",
               Self::EnvValue => "environment variable value",
               Self::Platform => "platform",
           })
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl-core --lib error::tests::name_kind
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 214 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/error.rs
   $ git commit -m "feat(buildl-core): name the program, env value and platform kinds"
   ```

### Task 2 — Add `Program`

**Files:**
- Create `crates/buildl-core/src/types/program.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod program;` to the `pub mod` lines and `pub use program::Program;` to the `pub use` lines, and add one bullet to the module doc, directly above the `Diagnostic` bullet:

   ```rust
   //! - [`Program`] — the name of a program a command runs.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `Program` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/program.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Program;
       use crate::types::Argument;

       #[test]
       fn program_accepts_the_documented_forms() {
           for raw in ["cc", "/usr/bin/cc", "scripts/gen.sh", "my tool", "c++"] {
               assert!(Program::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn program_rejects_each_documented_form_with_its_reason() {
           let cases = [
               ("", "must not be empty"),
               ("a\0b", "must not contain a NUL byte"),
               ("$out/app", "must not hold '$'"),
               ("bin/$opt:tool", "must not hold '$'"),
           ];
           for (raw, reason) in cases {
               let err = Program::parse(raw).unwrap_err();
               assert_eq!(
                   err.to_string(),
                   format!("invalid program: {raw:?} — {reason}")
               );
           }
       }

       #[test]
       fn of_names_a_program_only_for_an_argument_that_is_one() {
           let cases = [
               ("", None),
               ("bin/$opt:tool", None),
               ("cc", Some("cc")),
               ("/usr/bin/cc", Some("/usr/bin/cc")),
               ("$out/app", None),
               ("scripts/gen.sh", Some("scripts/gen.sh")),
           ];
           for (raw, expected) in cases {
               let named = Program::of(&Argument::parse(raw).unwrap());
               assert_eq!(named.as_ref().map(Program::as_str), expected, "{raw:?}");
           }
       }

       #[test]
       fn program_round_trips_through_json_as_a_string() {
           let value = Program::parse("/usr/bin/cc").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(json, r#""/usr/bin/cc""#);
           assert_eq!(serde_json::from_str::<Program>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "/usr/bin/cc");
           assert!(serde_json::from_str::<Program>(r#""$in""#).is_err());
       }

       #[test]
       fn program_from_str_routes_through_parse() {
           assert_eq!("cc".parse::<Program>().unwrap().as_str(), "cc");
           assert!("".parse::<Program>().is_err());
       }

       #[test]
       fn programs_order_by_their_text() {
           assert!(Program::parse("cc").unwrap() < Program::parse("ld").unwrap());
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::program
   error[E0432]: unresolved import `super::Program`
   error[E0432]: unresolved import `program::Program`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/program.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ````rust
   //! The name of a program a command runs, validated at construction.
   //!
   //! Its own file because a program is not any argument: it is the one a workspace must allow
   //! before anything runs, and the one whose identity is part of an action key. Naming it with its
   //! own type keeps an argument that names nothing out of both.
   //!
   //! Responsibilities: [`Program`], its constructors, and its rendering.
   //!
   //! Non-responsibilities: finding the program. Whether it exists, and what it is, is answered
   //! through a port.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::Argument;
   use crate::types::grammar;

   /// The grammar of a program name, returning the reason a candidate is rejected.
   fn program_grammar(raw: &str) -> core::result::Result<(), &'static str> {
       if raw.is_empty() {
           return Err("must not be empty");
       }
       grammar::text(raw)?;
       if raw.contains('$') {
           return Err("must not hold '$'");
       }
       Ok(())
   }

   /// The name of a program to run, such as `cc`, `/usr/bin/cc` or `scripts/gen.sh`.
   ///
   /// Any non-empty UTF-8 text without a `NUL` byte and without `$`. A `$` starts a placeholder,
   /// and text holding one names nothing until the placeholder is expanded.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Program(String);

   impl Program {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is empty, holds a `NUL` byte, or holds `$`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match program_grammar(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::Program,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The program `argument` names; `None` when it is empty or holds `$`.
       ///
       /// # Examples
       ///
       /// ```
       /// use buildl_core::{Argument, Program};
       ///
       /// let named = Program::of(&Argument::parse("cc")?);
       /// assert_eq!(named.as_ref().map(Program::as_str), Some("cc"));
       /// assert_eq!(Program::of(&Argument::parse("$out/app")?), None);
       /// # Ok::<(), buildl_core::Error>(())
       /// ```
       #[must_use]
       pub fn of(argument: &Argument) -> Option<Self> {
           Self::parse(argument.as_str()).ok()
       }

       /// The name as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for Program {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for Program {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for Program {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Program> for String {
       fn from(value: Program) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for Program {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ````

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::program
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 215 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

9. Commit:

   ```
   $ git add crates/buildl-core/src/types/program.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the Program name type"
   ```

### Task 3 — Add `EnvValue`

**Files:**
- Create `crates/buildl-core/src/types/env_value.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod env_value;` and `pub use env_value::EnvValue;`, and extend the bullet task 2 added. Replace:

   ```rust
   //! - [`Program`] — the name of a program a command runs.
   ```

   With:

   ```rust
   //! - [`Program`] — the name of a program a command runs; [`EnvValue`] — an environment
   //!   variable's value.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `EnvValue` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/env_value.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::EnvValue;

       #[test]
       fn env_value_accepts_any_text_without_a_nul_byte() {
           for raw in ["", "/usr/bin:/bin", "a b", "ünïcödé", "x=y"] {
               assert!(EnvValue::parse(raw).is_ok(), "{raw:?} should parse");
           }
       }

       #[test]
       fn env_value_rejects_a_nul_byte() {
           let err = EnvValue::parse("a\0b").unwrap_err();
           assert_eq!(
               err.to_string(),
               r#"invalid environment variable value: "a\0b" — must not contain a NUL byte"#
           );
       }

       #[test]
       fn env_value_round_trips_through_json_as_a_string() {
           let value = EnvValue::parse("/usr/bin:/bin").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(json, r#""/usr/bin:/bin""#);
           assert_eq!(serde_json::from_str::<EnvValue>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "/usr/bin:/bin");
           assert_eq!(value.as_str(), "/usr/bin:/bin");
           assert!(serde_json::from_str::<EnvValue>(r#""a\u0000b""#).is_err());
       }

       #[test]
       fn env_value_from_str_routes_through_parse() {
           assert_eq!("".parse::<EnvValue>().unwrap().as_str(), "");
           assert!("a\0b".parse::<EnvValue>().is_err());
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::env_value
   error[E0432]: unresolved import `super::EnvValue`
   error[E0432]: unresolved import `env_value::EnvValue`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/env_value.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! An environment variable's value, validated at construction.
   //!
   //! Its own file because a value is not a name: a name has a grammar a shell imposes, a value is
   //! free text, and the two reach an action key as the two halves of one entry.
   //!
   //! Responsibilities: [`EnvValue`], its [`EnvValue::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: the environment. Nothing here reads a variable; a value arrives through
   //! a port.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};
   use crate::types::grammar;

   /// The value of an environment variable, such as `/usr/bin:/bin`.
   ///
   /// Any UTF-8 text without a `NUL` byte, the empty string included: a variable set to nothing is
   /// not an unset variable, and an environment block cannot carry `NUL`.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct EnvValue(String);

   impl EnvValue {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` holds a `NUL` byte.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match grammar::text(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::EnvValue,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The value as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for EnvValue {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for EnvValue {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for EnvValue {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<EnvValue> for String {
       fn from(value: EnvValue) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for EnvValue {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::env_value
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 221 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

9. Commit:

   ```
   $ git add crates/buildl-core/src/types/env_value.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the EnvValue type"
   ```

### Task 4 — Add `Platform`

**Files:**
- Create `crates/buildl-core/src/types/platform.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod platform;` and `pub use platform::Platform;`, and extend the same bullet. Replace:

   ```rust
   //! - [`Program`] — the name of a program a command runs; [`EnvValue`] — an environment
   //!   variable's value.
   ```

   With:

   ```rust
   //! - [`Program`] — the name of a program a command runs; [`EnvValue`] — an environment
   //!   variable's value; [`Platform`] — the operating system and architecture actions run on.
   ```

2. Re-export it. In `crates/buildl-core/src/lib.rs`, add `Platform` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/platform.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Platform;

       #[test]
       fn platform_accepts_the_documented_forms() {
           for raw in ["linux/x86_64", "macos/aarch64", "a/b", "free-bsd.14/arm_v7"] {
               assert!(Platform::parse(raw).is_ok(), "{raw:?} should parse");
           }
           let longest = format!("{}/{}", "a".repeat(64), "b".repeat(64));
           assert!(Platform::parse(longest).is_ok());
       }

       #[test]
       fn platform_rejects_each_documented_form_with_its_reason() {
           let joined = "must be an operating system and an architecture joined by '/'";
           let empty = "the operating system and the architecture must not be empty";
           let characters = "the operating system and the architecture may hold only ASCII letters, \
                             digits, '_', '-' and '.'";
           let cases = [
               ("", joined),
               ("linux", joined),
               ("/x86_64", empty),
               ("linux/", empty),
               ("linux/x86/64", characters),
               ("lin ux/x86_64", characters),
               ("linux/x86:64", characters),
           ];
           for (raw, reason) in cases {
               let err = Platform::parse(raw).unwrap_err();
               assert_eq!(
                   err.to_string(),
                   format!("invalid platform: {raw:?} — {reason}")
               );
           }
       }

       #[test]
       fn platform_rejects_a_part_longer_than_64_bytes() {
           let long_os = format!("{}/x86_64", "a".repeat(65));
           let long_arch = format!("linux/{}", "b".repeat(65));
           for raw in [long_os, long_arch] {
               let err = Platform::parse(raw).unwrap_err();
               assert!(
                   err.to_string().ends_with("must be at most 64 bytes each"),
                   "{err}"
               );
           }
       }

       #[test]
       fn platform_round_trips_through_json_as_a_string() {
           let value = Platform::parse("linux/x86_64").unwrap();
           let json = serde_json::to_string(&value).unwrap();
           assert_eq!(json, r#""linux/x86_64""#);
           assert_eq!(serde_json::from_str::<Platform>(&json).unwrap(), value);
           assert_eq!(value.to_string(), "linux/x86_64");
           assert!(serde_json::from_str::<Platform>(r#""linux""#).is_err());
       }

       #[test]
       fn platform_from_str_routes_through_parse() {
           let parsed = "macos/aarch64".parse::<Platform>().unwrap();
           assert_eq!(parsed.as_str(), "macos/aarch64");
           assert!("macos".parse::<Platform>().is_err());
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::platform
   error[E0432]: unresolved import `super::Platform`
   error[E0432]: unresolved import `platform::Platform`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/platform.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The platform actions run on, validated at construction.
   //!
   //! Its own file because an operating system and an architecture are one fact about a machine:
   //! the same command on another platform is another action, so the pair is part of an action key.
   //!
   //! Responsibilities: [`Platform`], its [`Platform::parse`] constructor, and its rendering.
   //!
   //! Non-responsibilities: detecting the platform. This crate never asks what machine it is on;
   //! the value arrives through a port.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};

   use crate::error::{Error, NameKind, Result};

   /// Longest accepted operating system or architecture, in bytes.
   const PART_MAX_LEN: usize = 64;

   /// Whether `byte` may appear in an operating system or an architecture.
   const fn is_part_byte(byte: u8) -> bool {
       byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
   }

   /// The grammar of one half of a platform, returning the reason a candidate is rejected.
   fn part_grammar(part: &str) -> core::result::Result<(), &'static str> {
       if part.is_empty() {
           return Err("the operating system and the architecture must not be empty");
       }
       if part.len() > PART_MAX_LEN {
           return Err("the operating system and the architecture must be at most 64 bytes each");
       }
       if !part.bytes().all(is_part_byte) {
           return Err(
               "the operating system and the architecture may hold only ASCII letters, digits, \
                '_', '-' and '.'",
           );
       }
       Ok(())
   }

   /// The grammar of a platform, returning the reason a candidate is rejected.
   fn platform_grammar(raw: &str) -> core::result::Result<(), &'static str> {
       let Some((os, arch)) = raw.split_once('/') else {
           return Err("must be an operating system and an architecture joined by '/'");
       };
       part_grammar(os)?;
       part_grammar(arch)
   }

   /// An operating system and an architecture, such as `linux/x86_64` or `macos/aarch64`.
   ///
   /// Two parts joined by one `/`, each 1 to 64 bytes of ASCII letters, digits, `_`, `-` and `.`.
   /// The spelling of each part is whatever the host reports: two platforms are the same exactly
   /// when their text is.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Platform(String);

   impl Platform {
       /// Validates `raw` and wraps it.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is not two parts joined by one `/`, or when a
       /// part is empty, exceeds 64 bytes, or holds a character outside ASCII alphanumerics, `_`,
       /// `-` and `.`.
       pub fn parse(raw: impl Into<String>) -> Result<Self> {
           let raw = raw.into();
           match platform_grammar(&raw) {
               Ok(()) => Ok(Self(raw)),
               Err(reason) => Err(Error::InvalidName {
                   kind: NameKind::Platform,
                   value: raw,
                   reason,
               }),
           }
       }

       /// The platform as a string slice.
       #[must_use]
       pub fn as_str(&self) -> &str {
           &self.0
       }
   }

   impl fmt::Display for Platform {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.write_str(&self.0)
       }
   }

   impl AsRef<str> for Platform {
       fn as_ref(&self) -> &str {
           &self.0
       }
   }

   impl FromStr for Platform {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::parse(raw)
       }
   }

   impl From<Platform> for String {
       fn from(value: Platform) -> Self {
           value.0
       }
   }

   impl TryFrom<String> for Platform {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::parse(raw)
       }
   }
   ```

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::platform
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 225 filtered out
   ```

8. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

9. Commit:

   ```
   $ git add crates/buildl-core/src/types/platform.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the Platform type"
   ```

### Task 5 — Add `FileStat` and `StatEntry`

**Files:**
- Create `crates/buildl-core/src/types/file_stat.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/types/file_stat.rs`. The file holds two type definitions and no logic, so it has no test module and this task has no failing-test step:

   ```rust
   //! A file's metadata, and a digest recorded beside the metadata it was taken under.
   //!
   //! Its own file because the pair answers one question without reading a file: whether it changed
   //! since it was last hashed. Hashing is what makes a warm build slow, so a digest is kept with
   //! the metadata that vouches for it.
   //!
   //! Responsibilities: [`FileStat`] and [`StatEntry`].
   //!
   //! Non-responsibilities: reading metadata, hashing, and deciding whether a recorded digest may
   //! be reused. The first two arrive through ports; the third belongs to the phase that plans.
   //!
   //! This file holds only type definitions, so it carries no logic to unit-test.

   use crate::types::{Digest, Timestamp};

   /// What decides whether a file changed: when it was last modified, and how long it is.
   ///
   /// Two values are compared for equality, never for order: a modification time earlier than a
   /// recorded one is as much a change as a later one.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct FileStat {
       /// When the file was last modified.
       pub modified: Timestamp,
       /// The file's length in bytes.
       pub size: u64,
   }

   /// A file's content digest, and the metadata the file had when the digest was taken.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct StatEntry {
       /// The metadata the digest was taken under.
       pub stat: FileStat,
       /// The digest of the file's content.
       pub digest: Digest,
   }
   ```

2. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod file_stat;` and `pub use file_stat::{FileStat, StatEntry};`, and add one bullet to the module doc, directly above the `Diagnostic` bullet:

   ```rust
   //! - [`FileStat`] and [`StatEntry`] — a file's metadata, and a digest recorded beside it.
   ```

3. Re-export both. In `crates/buildl-core/src/lib.rs`, add `FileStat` and `StatEntry` to the `pub use types::{…}` list, then run `cargo fmt --all`. The list now reads:

   ```rust
   pub use types::{
       Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
       Description, Diagnostic, Digest, Directory, EntryName, EnvName, EnvValue, Evaluated, FieldName,
       FileStat, Freshness, Label, NetworkAccess, Node, NodeId, OutputName, Platform, Program,
       Provenance, Rule, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
       StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
       StagedTarget, StatEntry, Target, TargetGraph, TargetName, TargetRole, Timestamp, Written,
   };
   ```

4. Confirm the module index re-exports the five types, on four lines:

   ```
   $ grep -c -e 'pub use program::Program;' -e 'pub use env_value::EnvValue;' -e 'pub use platform::Platform;' -e 'pub use file_stat::{FileStat, StatEntry};' crates/buildl-core/src/types/mod.rs
   4
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/src/types/file_stat.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add FileStat and StatEntry"
   ```

### Task 6 — Say whether one directory lies inside another

**Files:**
- Modify `crates/buildl-core/src/types/directory.rs`

**Steps:**

1. Write the failing tests. In the test module of `crates/buildl-core/src/types/directory.rs`, insert four tests directly above `fn round_trips_through_json_as_a_string`:

   ```rust
       #[test]
       fn contains_itself_and_everything_below_it() {
           let lib = Directory::parse("lib").unwrap();
           assert!(lib.contains(&lib));
           assert!(lib.contains(&Directory::parse("lib/text").unwrap()));
           assert!(lib.contains(&Directory::parse("lib/text/deep").unwrap()));
       }

       #[test]
       fn does_not_contain_what_is_above_or_beside_it() {
           let text = Directory::parse("lib/text").unwrap();
           let lib = Directory::parse("lib").unwrap();
           assert!(!text.contains(&lib));
           assert!(!text.contains(&Directory::parse("lib/other").unwrap()));
           assert!(!lib.contains(&Directory::root()));
       }

       #[test]
       fn a_shared_prefix_is_not_containment() {
           let lib = Directory::parse("lib").unwrap();
           assert!(!lib.contains(&Directory::parse("lib2").unwrap()));
           assert!(!lib.contains(&Directory::parse("lib2/text").unwrap()));
           assert!(!lib.contains(&Directory::parse("li").unwrap()));
       }

       #[test]
       fn the_root_contains_every_directory() {
           let root = Directory::root();
           assert!(root.contains(&root));
           assert!(root.contains(&Directory::parse("lib").unwrap()));
           assert!(root.contains(&Directory::parse("lib/text").unwrap()));
       }
   ```

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::directory
   error[E0599]: no method named `contains` found for struct `Directory` in the current scope
   ```

3. Add the method to `impl Directory`, directly below `is_root`:

   ```rust
       /// Whether `other` is this directory or lies below it.
       ///
       /// The workspace root contains every directory. A shared prefix is not containment: `lib`
       /// does not contain `lib2`.
       #[must_use]
       pub fn contains(&self, other: &Self) -> bool {
           self.is_root()
               || self == other
               || other
                   .0
                   .strip_prefix(self.0.as_str())
                   .is_some_and(|rest| rest.starts_with('/'))
       }
   ```

4. Update the module doc of the same file. Replace:

   ```rust
   //! Responsibilities: [`Directory`], its [`Directory::parse`] constructor, and its rendering.
   ```

   With:

   ```rust
   //! Responsibilities: [`Directory`], its [`Directory::parse`] constructor, its rendering, and
   //! whether one directory lies inside another.
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::directory
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 225 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/types/directory.rs
   $ git commit -m "feat(buildl-core): say whether a directory contains another"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- `Program`, `EnvValue` and `Platform` each reject every form their grammar excludes with a named
  reason, and round-trip through JSON as their text.
- `Program::of` answers for the six first arguments a command may have: empty, a placeholder
  inside a path, a bare name, an absolute path, a leading placeholder, a relative path.
- `Directory::contains` holds for itself, for anything below it and for the root, and fails for
  an ancestor, a sibling and a directory that only shares a prefix.
- `crates/buildl-core/Cargo.toml` is unchanged.
