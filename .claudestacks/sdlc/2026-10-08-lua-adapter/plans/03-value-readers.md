---
status: done
created: 2026-10-08
depends-on: [01]
---

# Value Readers Implementation Plan

**Goal:** A primitive's Lua arguments are read into as-written text by strict shape rules, or refused with a structured reason.

**Architecture:** Two files. `refusal.rs` holds `Refusal`: an `EvaluationFailure` kind, a detail message, and optionally the name of the declaration being made. It renders as `line N: buildl.<primitive> '<name>': <detail>`, and the call site is supplied later by the module that knows it. `values.rs` holds the readers every primitive shares. A string field takes a Lua string holding UTF-8 and nothing Lua would coerce into one. A list field takes a sequence `1..n` of strings, splicing one level of nested sequences in order. A flag takes a boolean. A closed option table refuses its first unknown key in sorted order, so the refusal does not depend on `pairs` order. Fields are read with `raw_get`, past any metatable.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §6.1 (reading values, field names, key rendering, validation order), D8 (strict shapes, one-level flattening), §7.3 (diagnostic shape).

---

## Context an implementer needs

**Conventions every task follows.** The workspace lints are strict. `unwrap`, `expect` and `panic!`
are denied outside tests, pedantic and nursery clippy run at `-D warnings`, and `missing_docs` and
`unreachable_pub` are on. Crate-internal items are therefore `pub(crate)` inside private modules. A
test module opens with `#![expect(clippy::unwrap_used, reason = …)]`, adding `clippy::panic` where
it panics. `lib.rs` holds only module docs, `mod` declarations and `pub use` re-exports. Every file
opens with `//!` docs naming its responsibilities and non-responsibilities, and carries no internal
planning vocabulary. Every commit is Conventional Commits with scope `buildl-lua`, or `repo` for
workspace files. Every task ends green on `cargo make dod` before it commits.

**Where the code comes from.** Every code block below was compiled, linted and tested in a scratch
copy of the workspace at every task boundary of this plan set, on macOS with rustc 1.98 and clippy
1.98. The expected outputs are the outputs those runs printed.

| Item | From | Used for |
|---|---|---|
| `airsl::mlua::{Lua, Table, Value}` | airsl's `mlua` re-export (0.12) | the Lua values read; tests build values with `Lua::new()` |
| `Value::type_name()`, `String::to_str()`, `String::as_bytes()`, `Table::raw_len()`, `Table::pairs()`, `Table::raw_get()` | mlua 0.12 | shape checks |
| `EvaluationFailure::{WrongFieldType, UnknownField, Runtime}`, `Written<T>`, `Diagnostic` | `buildl-core` | refusals |

`EvaluationFailure` is `#[non_exhaustive]`, but its variants can still be constructed from outside
`buildl-core`. Tests compare it with `assert_eq!`, never with an exhaustive `match`.

Plan `01` must be done (lint allowance). Plan `02` need not be. If it is, `lib.rs` already holds
`mod staging;`, and the new declarations join that sorted block.

## File structure

```text
crates/buildl-lua/src/refusal.rs — [create] Refusal and its rendering, with unit tests  (Task 1)
crates/buildl-lua/src/values.rs  — [create] the value readers, with unit tests          (Task 2)
crates/buildl-lua/src/lib.rs     — [modify] declare both modules                         (Tasks 1, 2)
```

### Task 1 — Refusals and their diagnostics

**Files:**
- Create `crates/buildl-lua/src/refusal.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod refusal;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/refusal.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! Why a `buildl` primitive turned a call down.
   //!
   //! Its own file because three different readers produce a refusal — the value readers, the
   //! primitives and `buildl.sources` — and none of them knows where in the build file the call was
   //! made. The refusal carries what they do know; the module that installed the primitive adds the
   //! call site when it renders the diagnostic.
   //!
   //! Responsibilities: [`Refusal`] and its rendering into a [`Diagnostic`].
   //!
   //! Non-responsibilities: recording a refusal so it sticks, which the staging buffer does.

   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until `LuaSource` evaluates a build file"
       )
   )]

   #[cfg(test)]
   mod tests {
       use buildl_core::{EvaluationFailure, Written};

       use super::Refusal;

       fn wrong_run() -> Refusal {
           Refusal::new(
               EvaluationFailure::WrongFieldType {
                   field: Written::new("run"),
               },
               "field 'run' must be a list of strings, found number",
           )
       }

       #[test]
       fn renders_line_primitive_subject_and_detail() {
           let diagnostic = wrong_run().about("app").diagnostic("target", Some(12));
           assert_eq!(
               diagnostic.as_str(),
               "line 12: buildl.target 'app': field 'run' must be a list of strings, found number"
           );
       }

       #[test]
       fn omits_what_is_not_known() {
           let diagnostic = wrong_run().diagnostic("rule", None);
           assert_eq!(
               diagnostic.as_str(),
               "buildl.rule: field 'run' must be a list of strings, found number"
           );
       }

       #[test]
       fn keeps_the_failure_kind() {
           assert_eq!(
               wrong_run().about("app").failure(),
               &EvaluationFailure::WrongFieldType {
                   field: Written::new("run"),
               }
           );
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib refusal::tests
   ```

   ```text
   error[E0432]: unresolved import `super::Refusal`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use buildl_core::{Diagnostic, EvaluationFailure};

   /// A primitive call that was refused: the failure's kind, what was wrong, and — once known — the
   /// name the call declares.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) struct Refusal {
       failure: EvaluationFailure,
       detail: String,
       subject: Option<String>,
   }

   impl Refusal {
       /// A refusal of kind `failure`, explained by `detail`.
       pub(crate) fn new(failure: EvaluationFailure, detail: impl Into<String>) -> Self {
           Self {
               failure,
               detail: detail.into(),
               subject: None,
           }
       }

       /// The same refusal, attributed to the declaration named `subject`.
       #[must_use]
       pub(crate) fn about(mut self, subject: &str) -> Self {
           self.subject = Some(subject.to_owned());
           self
       }

       /// The failure's kind.
       pub(crate) const fn failure(&self) -> &EvaluationFailure {
           &self.failure
       }

       /// Renders the refusal for `buildl.<primitive>`, prefixed with the build file's `line` when it
       /// is known.
       pub(crate) fn diagnostic(&self, primitive: &str, line: Option<usize>) -> Diagnostic {
           let location = line.map_or_else(String::new, |line| format!("line {line}: "));
           let subject = self
               .subject
               .as_ref()
               .map_or_else(String::new, |subject| format!(" '{subject}'"));
           Diagnostic::new(format!(
               "{location}buildl.{primitive}{subject}: {}",
               self.detail
           ))
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib refusal::tests
   ```

   ```text
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): describe a refused primitive call`.

### Task 2 — The value readers

**Files:**
- Create `crates/buildl-lua/src/values.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod values;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/values.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! Reading a primitive's Lua arguments into text as the build file wrote it.
   //!
   //! Its own file because every primitive reads the same four shapes — a string, a list of strings,
   //! a boolean, a closed option table — and the shape rules must not differ between them. The rules
   //! are strict: a string field takes a Lua string and nothing that Lua would coerce into one, a list
   //! is a sequence `1..n` whose elements are strings or one level of nested sequences of strings
   //! (spliced in order), and a flag is a boolean.
   //!
   //! Responsibilities: the value readers, and the closed-table check that names an unknown field.
   //!
   //! Non-responsibilities: validating what the text says. A name's grammar, a label's resolution and
   //! a path's containment are decided after evaluation, not here.

   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until `LuaSource` evaluates a build file"
       )
   )]

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid Lua snippets; a panic is the intended failure signal"
       )]

       use airsl::mlua::{Lua, Value};
       use buildl_core::{EvaluationFailure, Written};

       use super::{closed, field, flag, list, optional_list, optional_text, string, table, text};
       use crate::refusal::Refusal;

       fn eval(lua: &Lua, source: &str) -> Value {
           lua.load(source).eval::<Value>().unwrap()
       }

       fn texts(written: &[Written<()>]) -> Vec<&str> {
           written.iter().map(Written::as_written).collect()
       }

       fn wrong_type(field: &str) -> EvaluationFailure {
           EvaluationFailure::WrongFieldType {
               field: Written::new(field),
           }
       }

       fn failure(refusal: &Refusal) -> &EvaluationFailure {
           refusal.failure()
       }

       #[test]
       fn text_reads_a_string_as_written() {
           let lua = Lua::new();
           let written: Written<()> = text(&eval(&lua, "return 'src/a.c'"), "name").unwrap();
           assert_eq!(written.as_written(), "src/a.c");
       }

       #[test]
       fn text_refuses_a_number_rather_than_coercing_it() {
           let lua = Lua::new();
           let refusal = text::<()>(&eval(&lua, "return 1"), "name").unwrap_err();
           assert_eq!(failure(&refusal), &wrong_type("name"));
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'name' must be a string, found integer"
           );
       }

       #[test]
       fn text_refuses_non_utf8_bytes() {
           let lua = Lua::new();
           let refusal = text::<()>(&eval(&lua, "return '\\255'"), "name").unwrap_err();
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'name' must be a string, found non-UTF-8 text"
           );
       }

       #[test]
       fn string_reads_plain_text() {
           let lua = Lua::new();
           assert_eq!(
               string(&eval(&lua, "return '**/*.c'"), "pattern").unwrap(),
               "**/*.c"
           );
       }

       #[test]
       fn text_refuses_nil() {
           let refusal = text::<()>(&Value::Nil, "default").unwrap_err();
           assert_eq!(failure(&refusal), &wrong_type("default"));
       }

       #[test]
       fn optional_text_reads_nil_as_none() {
           assert_eq!(optional_text::<()>(&Value::Nil, "desc").unwrap(), None);
       }

       #[test]
       fn list_reads_a_sequence_in_order() {
           let lua = Lua::new();
           let written: Vec<Written<()>> =
               list(&eval(&lua, "return { 'cc', '-c', '$in' }"), "run").unwrap();
           assert_eq!(texts(&written), ["cc", "-c", "$in"]);
       }

       #[test]
       fn list_splices_one_level_of_nested_lists_in_order() {
           let lua = Lua::new();
           let written: Vec<Written<()>> = list(
               &eval(&lua, "return { { 'a.c', 'b.c' }, 'go.mod', {}, { 'c.c' } }"),
               "inputs",
           )
           .unwrap();
           assert_eq!(texts(&written), ["a.c", "b.c", "go.mod", "c.c"]);
       }

       #[test]
       fn list_refuses_two_levels_of_nesting() {
           let lua = Lua::new();
           let refusal = list::<()>(&eval(&lua, "return { { { 'a.c' } } }"), "inputs").unwrap_err();
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'inputs' must be a list of strings, found table"
           );
       }

       #[test]
       fn list_refuses_a_table_with_holes_or_named_keys() {
           let lua = Lua::new();
           for source in [
               "return { [1] = 'a', [3] = 'c' }",
               "return { 'a', name = 'b' }",
               "return { [0] = 'a' }",
           ] {
               let refusal = list::<()>(&eval(&lua, source), "deps").unwrap_err();
               assert_eq!(
                   refusal.diagnostic("target", None).as_str(),
                   "buildl.target: field 'deps' must be a list of strings, found a table that is not a list",
                   "{source}"
               );
           }
       }

       #[test]
       fn list_refuses_a_non_string_element() {
           let lua = Lua::new();
           let refusal = list::<()>(&eval(&lua, "return { 'cc', 1 }"), "run").unwrap_err();
           assert_eq!(failure(&refusal), &wrong_type("run"));
       }

       #[test]
       fn list_refuses_a_bare_string() {
           let lua = Lua::new();
           let refusal = list::<()>(&eval(&lua, "return 'cc'"), "run").unwrap_err();
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'run' must be a list of strings, found string"
           );
       }

       #[test]
       fn optional_list_reads_nil_as_none_and_an_empty_table_as_empty() {
           let lua = Lua::new();
           assert_eq!(optional_list::<()>(&Value::Nil, "outputs").unwrap(), None);
           assert_eq!(
               optional_list::<()>(&eval(&lua, "return {}"), "outputs").unwrap(),
               Some(Vec::new())
           );
       }

       #[test]
       fn flag_reads_booleans_and_nil() {
           assert_eq!(flag(&Value::Boolean(true), "network").unwrap(), Some(true));
           assert_eq!(
               flag(&Value::Boolean(false), "network").unwrap(),
               Some(false)
           );
           assert_eq!(flag(&Value::Nil, "network").unwrap(), None);
       }

       #[test]
       fn flag_refuses_a_truthy_non_boolean() {
           let refusal = flag(&Value::Integer(1), "always").unwrap_err();
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'always' must be a boolean, found integer"
           );
       }

       #[test]
       fn table_refuses_anything_but_a_table() {
           let refusal = table(&Value::Nil, "options").unwrap_err();
           assert_eq!(failure(&refusal), &wrong_type("options"));
       }

       #[test]
       fn closed_accepts_known_keys_only() {
           let lua = Lua::new();
           let options = table(&eval(&lua, "return { run = {}, desc = 'x' }"), "options").unwrap();
           assert!(closed(&options, &["run", "desc"]).is_ok());
       }

       #[test]
       fn closed_refuses_the_first_unknown_key_in_sorted_order() {
           let lua = Lua::new();
           let options = table(
               &eval(&lua, "return { zeta = 1, run = {}, alpha = 2 }"),
               "options",
           )
           .unwrap();
           let refusal = closed(&options, &["run"]).unwrap_err();
           assert_eq!(
               failure(&refusal),
               &EvaluationFailure::UnknownField {
                   field: Written::new("alpha"),
               }
           );
           assert_eq!(
               refusal.diagnostic("rule", None).as_str(),
               "buildl.rule: unknown field 'alpha'"
           );
       }

       #[test]
       fn closed_renders_a_non_string_key_by_its_type() {
           let lua = Lua::new();
           let options = table(&eval(&lua, "return { 'positional' }"), "options").unwrap();
           let refusal = closed(&options, &["run"]).unwrap_err();
           assert_eq!(
               failure(&refusal),
               &EvaluationFailure::UnknownField {
                   field: Written::new("<integer>"),
               }
           );
       }

       #[test]
       fn closed_renders_a_non_utf8_key_lossily() {
           let lua = Lua::new();
           let options = table(&eval(&lua, "return { ['\\255'] = 1 }"), "options").unwrap();
           let refusal = closed(&options, &["run"]).unwrap_err();
           assert_eq!(
               failure(&refusal),
               &EvaluationFailure::UnknownField {
                   field: Written::new("\u{fffd}"),
               }
           );
       }

       #[test]
       fn field_reads_without_metamethods() {
           let lua = Lua::new();
           let options = table(
               &eval(
                   &lua,
                   "return setmetatable({}, { __index = function() return 'x' end })",
               ),
               "options",
           )
           .unwrap();
           assert!(matches!(field(&options, "run").unwrap(), Value::Nil));
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib values::tests
   ```

   ```text
   error[E0432]: unresolved imports `super::closed`, `super::field`, `super::flag`, `super::list`, `super::optional_list`, `super::optional_text`, `super::string`, `super::table`, `super::text`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use airsl::mlua::{self, Table, Value};
   use buildl_core::{EvaluationFailure, Written};

   use crate::refusal::Refusal;

   const A_STRING: &str = "a string";
   const A_LIST: &str = "a list of strings";
   const A_BOOLEAN: &str = "a boolean";
   const A_TABLE: &str = "a table";

   /// Reads a required string argument as plain text.
   pub(crate) fn string(value: &Value, field: &str) -> Result<String, Refusal> {
       utf8(value).ok_or_else(|| wrong(field, A_STRING, &describe(value)))
   }

   /// Reads a required string field.
   pub(crate) fn text<T>(value: &Value, field: &str) -> Result<Written<T>, Refusal> {
       string(value, field).map(Written::new)
   }

   /// Reads an optional string field; `nil` is `None`.
   pub(crate) fn optional_text<T>(value: &Value, field: &str) -> Result<Option<Written<T>>, Refusal> {
       match value {
           Value::Nil => Ok(None),
           other => text(other, field).map(Some),
       }
   }

   /// Reads a required list field, splicing one level of nested lists in order.
   pub(crate) fn list<T>(value: &Value, field: &str) -> Result<Vec<Written<T>>, Refusal> {
       let Value::Table(table) = value else {
           return Err(wrong(field, A_LIST, &describe(value)));
       };
       let elements =
           sequence(table)?.ok_or_else(|| wrong(field, A_LIST, "a table that is not a list"))?;
       let mut written = Vec::with_capacity(elements.len());
       for element in &elements {
           if let Value::Table(nested) = element {
               let nested = sequence(nested)?
                   .ok_or_else(|| wrong(field, A_LIST, "a nested table that is not a list"))?;
               for item in &nested {
                   written.push(Written::new(
                       utf8(item).ok_or_else(|| wrong(field, A_LIST, &describe(item)))?,
                   ));
               }
           } else {
               written.push(Written::new(
                   utf8(element).ok_or_else(|| wrong(field, A_LIST, &describe(element)))?,
               ));
           }
       }
       Ok(written)
   }

   /// Reads an optional list field; `nil` is `None`.
   pub(crate) fn optional_list<T>(
       value: &Value,
       field: &str,
   ) -> Result<Option<Vec<Written<T>>>, Refusal> {
       match value {
           Value::Nil => Ok(None),
           other => list(other, field).map(Some),
       }
   }

   /// Reads an optional boolean field; `nil` is `None`.
   pub(crate) fn flag(value: &Value, field: &str) -> Result<Option<bool>, Refusal> {
       match value {
           Value::Nil => Ok(None),
           Value::Boolean(set) => Ok(Some(*set)),
           other => Err(wrong(field, A_BOOLEAN, &describe(other))),
       }
   }

   /// Reads a required table argument.
   pub(crate) fn table(value: &Value, field: &str) -> Result<Table, Refusal> {
       match value {
           Value::Table(table) => Ok(table.clone()),
           other => Err(wrong(field, A_TABLE, &describe(other))),
       }
   }

   /// Refuses the first key of `table`, in sorted order, that `accepted` does not name.
   ///
   /// Sorting makes the refused key the same whatever order `pairs` visits the table in.
   pub(crate) fn closed(table: &Table, accepted: &[&str]) -> Result<(), Refusal> {
       let mut keys = Vec::new();
       for pair in table.pairs::<Value, Value>() {
           let (key, _) = pair.map_err(|error| runtime(&error))?;
           keys.push(render(&key));
       }
       keys.sort();
       keys.into_iter()
           .find(|key| !accepted.contains(&key.as_str()))
           .map_or(Ok(()), |key| {
               Err(Refusal::new(
                   EvaluationFailure::UnknownField {
                       field: Written::new(key.as_str()),
                   },
                   format!("unknown field '{key}'"),
               ))
           })
   }

   /// The value stored under `name`, read without metamethods.
   pub(crate) fn field(table: &Table, name: &str) -> Result<Value, Refusal> {
       table
           .raw_get::<Value>(name)
           .map_err(|error| runtime(&error))
   }

   /// The elements of `table` when its keys are exactly `1..n`, in order.
   fn sequence(table: &Table) -> Result<Option<Vec<Value>>, Refusal> {
       let len = table.raw_len();
       let mut count = 0_usize;
       for pair in table.pairs::<Value, Value>() {
           let (key, _) = pair.map_err(|error| runtime(&error))?;
           let in_range = match key {
               Value::Integer(index) => usize::try_from(index).is_ok_and(|i| (1..=len).contains(&i)),
               _ => false,
           };
           if !in_range {
               return Ok(None);
           }
           count += 1;
       }
       if count != len {
           return Ok(None);
       }
       (1..=len)
           .map(|index| table.raw_get::<Value>(index))
           .collect::<mlua::Result<Vec<_>>>()
           .map(Some)
           .map_err(|error| runtime(&error))
   }

   /// The value's text when it is a Lua string holding valid UTF-8.
   fn utf8(value: &Value) -> Option<String> {
       match value {
           Value::String(text) => text.to_str().ok().map(|text| text.to_owned()),
           _ => None,
       }
   }

   /// How a refusal names what it found.
   fn describe(value: &Value) -> String {
       match value {
           Value::String(text) if text.to_str().is_err() => String::from("non-UTF-8 text"),
           other => other.type_name().to_owned(),
       }
   }

   /// How an option table's key is named in a refusal.
   fn render(key: &Value) -> String {
       match key {
           Value::String(text) => String::from_utf8_lossy(&text.as_bytes()).into_owned(),
           other => format!("<{}>", other.type_name()),
       }
   }

   fn wrong(field: &str, wanted: &str, found: &str) -> Refusal {
       Refusal::new(
           EvaluationFailure::WrongFieldType {
               field: Written::new(field),
           },
           format!("field '{field}' must be {wanted}, found {found}"),
       )
   }

   fn runtime(error: &mlua::Error) -> Refusal {
       Refusal::new(EvaluationFailure::Runtime, error.to_string())
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib values::tests
   ```

   ```text
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): read primitive arguments by strict shape rules`.

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --lib refusal::tests` reports 3 passed and `values::tests` 21 passed.
- `cargo make dod` exits `0`.

## Review findings

- determinism / reversion guard (🟡) — `closed_refuses_the_first_unknown_key_in_sorted_order` caught a removed `keys.sort()` only by chance, because Lua 5.4 seeds its string hash per state — `crates/buildl-lua/src/values.rs:113`. **Fixed and verified.** The test now runs 64 fresh `Lua` states over `{ zeta, run, alpha, mu }`. With line 113 commented out, the test failed 3 runs out of 3: `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 32 filtered out`. After restoring the line, `cargo make dod` gave `test result: ok. 33 passed` and `Build Done`.
- doc-comment-discipline — the module doc names three refusal producers, but only the value readers exist yet — `refusal.rs:3`
- strong-types — `diagnostic(primitive: &str, …)` takes any string; a `Primitive` enum would stop a typo — `refusal.rs:55`
- escaping — the subject and unknown keys are rendered unescaped, so a newline in a name can forge a `line N:` diagnostic line. Risk is low, because the author of the build file is the one reading it — `refusal.rs:62`, `values.rs:121`
- modularity — the nested and flat list branches repeat the same push logic — `values.rs:58`
- unit-test-mandate — no test reaches the "nested table that is not a list" refusal — `values.rs:61`
- reversion guard — no test guards `raw_len`/`raw_get` in `sequence` or the `count != len` check — `values.rs:135`, `:148`
- spec note — the §6.1 example key `<number>` renders as `<integer>`, consistent with §7.3 — spec §6.1

Blocking set: none. The reviewer re-ran `cargo make dod` and it exited 0.

## Probe results

- The tasks' red steps proved their premises. `refusal::tests` gave `error[E0432]: unresolved import super::Refusal` and then `3 passed`. `values::tests` gave `error[E0432]: unresolved imports super::closed, …` and then `21 passed`. The lib total is 33.
- Determinism guard probe: with `keys.sort()` at `values.rs:113` commented out, `cargo test -p buildl-lua --lib closed_refuses_the_first_unknown_key_in_sorted_order` gave `FAILED. 0 passed; 1 failed` on 3 runs out of 3. The file was then restored.

## Deviations

- 2026-10-08 — `values.rs` test `closed_refuses_the_first_unknown_key_in_sorted_order` departs from the plan text. It loops over 64 fresh `Lua` states and adds a fourth key, `mu = 3`, so that reverting the sort fails reliably instead of on about one run in three. The test count is unchanged at 21.
- 2026-10-08 — One coder ran Tasks 1–2 in order, because Task 2 imports Task 1's `Refusal`. Commits are left to the user.
