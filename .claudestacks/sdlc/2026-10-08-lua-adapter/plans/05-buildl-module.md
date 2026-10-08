---
status: done
created: 2026-10-08
depends-on: [02, 03, 04]
---

# buildl Module Implementation Plan

**Goal:** The `buildl` table stages every declaration primitive under both of its names, with refusals that stick and name the build file's line.

**Architecture:** Two files. `primitives.rs` gives each primitive its argument shape: positional parameters are read left to right, then the option table is checked for unknown keys, then the accepted fields are read in their listed order. Once the declared name is known, a refusal names it. `module.rs` is `BuildlModule`, airsl's `HostModule` named `buildl`. It installs `target`, `test`, `rule`, `alias`, `option`, `subdir` and `sources` on the table airsl hands it, binds that same table to the global `buildl`, and removes `math.random`, `math.randomseed` and `print`. Every call goes through one guard. A file that already failed fails again with the same message. A new refusal is recorded in the staging buffer, then raised as a Lua error carrying the line of the nearest Lua frame, so `pcall` cannot un-fail the file.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §5.2 (install), §6 (the table, positional names, validation order), §7.3 (sticky refusals, line), D4, D5, D6; §1.1 P3, P7b, S1 (probe results).

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
| `Staging` (`stage`, `stage_subdir`, `record`, `recorded`, `finish`), `Exceeded` | plan `02` | the buffer |
| `Refusal`, the value readers | plan `03` | argument shapes |
| `Sources` (`new`, `find`) | plan `04` | `buildl.sources` |
| `airsl::{HostModule, InstallContext, ModuleName, Engine, ModuleSet, Policy, LanguageSurface, Script}` | airsl 0.1.4 | the module; tests build a minimal engine |
| `Lua::create_function`, `Lua::create_sequence_from`, `Lua::inspect_stack(level, f)`, `Debug::current_line()` | mlua 0.12 (`state.rs:1042`, `debug.rs:143`) | functions, results, call site |

Two facts settled by running them:

- Under `pcall`, stack level 1 is `pcall` itself, a C frame with no line. The call site is therefore
  the first level, walking upward from 1, whose frame has a current line.
- `airsstack` is not a global yet while modules install (airsl `builder.rs:196-202` sets it after the
  last module), so `install` can reach `math` and `print`, which are Lua globals, but not
  `airsstack`.

## File structure

```text
crates/buildl-lua/src/primitives.rs — [create] per-primitive argument shapes, with unit tests      (Task 1)
crates/buildl-lua/src/module.rs     — [create] BuildlModule and the refusal guard, with unit tests  (Task 2)
crates/buildl-lua/src/lib.rs        — [modify] declare both modules                                 (Tasks 1, 2)
```

### Task 1 — The primitives' argument shapes

**Files:**
- Create `crates/buildl-lua/src/primitives.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod primitives;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/primitives.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! What each `buildl` primitive reads from its arguments, and the staged record it produces.
   //!
   //! Its own file because the argument shape of each primitive is the build-file API's contract,
   //! and keeping all of them together keeps them consistent: positional parameters are read left
   //! to right, then an option table is checked for unknown keys, then its accepted fields are read
   //! in the order each primitive lists them. A refusal after the name is known names it.
   //!
   //! Responsibilities: [`target`], [`rule`], [`alias`], [`option`], [`subdir`], [`pattern`].
   //!
   //! Non-responsibilities: staging the records and installing the functions, which the `buildl`
   //! module does.

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
       use buildl_core::{
           EvaluationFailure, Freshness, NetworkAccess, StagedAlias, StagedRule, StagedSetting,
           StagedTarget, TargetRole, Written,
       };

       use super::{alias, option, pattern, rule, subdir, target};

       fn eval(lua: &Lua, source: &str) -> Value {
           lua.load(source).eval::<Value>().unwrap()
       }

       fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
           texts.iter().map(|text| Written::new(*text)).collect()
       }

       #[test]
       fn target_reads_every_field() {
           let lua = Lua::new();
           let staged = target(
               &eval(&lua, "return 'app'"),
               &eval(
                   &lua,
                   "return { rule = 'cc', run = { 'cc', '$in' }, inputs = { { 'a.c' }, 'b.c' }, \
                    deps = { '//lib:text' }, outputs = { 'app' }, env = { 'HOME' }, \
                    network = true, always = true }",
               ),
               TargetRole::Build,
           )
           .unwrap();
           assert_eq!(
               staged,
               StagedTarget {
                   name: Written::new("app"),
                   role: TargetRole::Build,
                   rule: Some(Written::new("cc")),
                   run: Some(strings(&["cc", "$in"])),
                   inputs: strings(&["a.c", "b.c"]),
                   deps: strings(&["//lib:text"]),
                   outputs: Some(strings(&["app"])),
                   env: strings(&["HOME"]),
                   network: NetworkAccess::Declared,
                   freshness: Freshness::Always,
               }
           );
       }

       #[test]
       fn target_defaults_every_absent_field() {
           let lua = Lua::new();
           let staged = target(
               &eval(&lua, "return 'app_test'"),
               &eval(&lua, "return { network = false, always = false }"),
               TargetRole::Test,
           )
           .unwrap();
           assert_eq!(
               staged,
               StagedTarget {
                   name: Written::new("app_test"),
                   role: TargetRole::Test,
                   rule: None,
                   run: None,
                   inputs: Vec::new(),
                   deps: Vec::new(),
                   outputs: None,
                   env: Vec::new(),
                   network: NetworkAccess::Sealed,
                   freshness: Freshness::Cached,
               }
           );
       }

       #[test]
       fn a_bad_name_is_refused_before_the_options_are_looked_at() {
           let refusal = target(&Value::Nil, &Value::Nil, TargetRole::Build).unwrap_err();
           assert_eq!(
               refusal.diagnostic("target", None).as_str(),
               "buildl.target: field 'name' must be a string, found nil"
           );
       }

       #[test]
       fn an_unknown_field_is_refused_before_a_wrongly_typed_one() {
           let lua = Lua::new();
           let refusal = target(
               &eval(&lua, "return 'app'"),
               &eval(&lua, "return { run = 1, srcs = {} }"),
               TargetRole::Build,
           )
           .unwrap_err();
           assert_eq!(
               refusal.failure(),
               &EvaluationFailure::UnknownField {
                   field: Written::new("srcs"),
               }
           );
           assert_eq!(
               refusal.diagnostic("target", Some(4)).as_str(),
               "line 4: buildl.target 'app': unknown field 'srcs'"
           );
       }

       #[test]
       fn accepted_fields_are_checked_in_their_listed_order() {
           let lua = Lua::new();
           let refusal = target(
               &eval(&lua, "return 'app'"),
               &eval(&lua, "return { always = 1, run = 1 }"),
               TargetRole::Build,
           )
           .unwrap_err();
           assert_eq!(
               refusal.failure(),
               &EvaluationFailure::WrongFieldType {
                   field: Written::new("run"),
               }
           );
       }

       #[test]
       fn rule_requires_run_and_reads_desc() {
           let lua = Lua::new();
           let staged = rule(
               &eval(&lua, "return 'cc'"),
               &eval(
                   &lua,
                   "return { run = { 'cc', '-c' }, desc = 'compile $in' }",
               ),
           )
           .unwrap();
           assert_eq!(
               staged,
               StagedRule {
                   name: Written::new("cc"),
                   run: strings(&["cc", "-c"]),
                   description: Some(Written::new("compile $in")),
               }
           );
           let refusal = rule(&eval(&lua, "return 'cc'"), &eval(&lua, "return {}")).unwrap_err();
           assert_eq!(
               refusal.diagnostic("rule", None).as_str(),
               "buildl.rule 'cc': field 'run' must be a list of strings, found nil"
           );
       }

       #[test]
       fn alias_reads_two_strings() {
           let lua = Lua::new();
           assert_eq!(
               alias(&eval(&lua, "return 'default'"), &eval(&lua, "return 'app'")).unwrap(),
               StagedAlias {
                   name: Written::new("default"),
                   target: Written::new("app"),
               }
           );
           let refusal = alias(&eval(&lua, "return 'default'"), &Value::Nil).unwrap_err();
           assert_eq!(
               refusal.diagnostic("alias", None).as_str(),
               "buildl.alias 'default': field 'target' must be a string, found nil"
           );
       }

       #[test]
       fn option_requires_a_string_default() {
           let lua = Lua::new();
           assert_eq!(
               option(
                   &eval(&lua, "return 'mode'"),
                   &eval(&lua, "return { default = 'debug' }")
               )
               .unwrap(),
               StagedSetting {
                   name: Written::new("mode"),
                   default: Written::new("debug"),
               }
           );
           let refusal = option(
               &eval(&lua, "return 'jobs'"),
               &eval(&lua, "return { default = 4 }"),
           )
           .unwrap_err();
           assert_eq!(
               refusal.diagnostic("option", None).as_str(),
               "buildl.option 'jobs': field 'default' must be a string, found integer"
           );
       }

       #[test]
       fn subdir_and_pattern_read_one_string() {
           let lua = Lua::new();
           assert_eq!(
               subdir(&eval(&lua, "return 'lib'")).unwrap(),
               Written::new("lib")
           );
           assert_eq!(pattern(&eval(&lua, "return '*.c'")).unwrap(), "*.c");
           assert_eq!(
               pattern(&Value::Nil)
                   .unwrap_err()
                   .diagnostic("sources", None)
                   .as_str(),
               "buildl.sources: field 'pattern' must be a string, found nil"
           );
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib primitives::tests
   ```

   ```text
   error[E0432]: unresolved imports `super::alias`, `super::option`, `super::pattern`, `super::rule`, `super::subdir`, `super::target`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use airsl::mlua::Value;
   use buildl_core::{
       Directory, Freshness, NetworkAccess, SettingName, StagedAlias, StagedRule, StagedSetting,
       StagedTarget, TargetName, TargetRole, Written,
   };

   use crate::refusal::Refusal;
   use crate::values::{closed, field, flag, list, optional_list, optional_text, string, table, text};

   /// The fields `buildl.target` and `buildl.test` accept, in the order they are read.
   const TARGET_FIELDS: [&str; 8] = [
       "rule", "run", "inputs", "deps", "outputs", "env", "network", "always",
   ];
   /// The fields `buildl.rule` accepts, in the order they are read.
   const RULE_FIELDS: [&str; 2] = ["run", "desc"];
   /// The fields `buildl.option` accepts.
   const OPTION_FIELDS: [&str; 1] = ["default"];

   /// `buildl.target(name, options)` and `buildl.test(name, options)`, told apart by `role`.
   pub(crate) fn target(
       name: &Value,
       options: &Value,
       role: TargetRole,
   ) -> Result<StagedTarget, Refusal> {
       let name: Written<TargetName> = text(name, "name")?;
       let subject = name.as_written().to_owned();
       target_fields(name, options, role).map_err(|refusal| refusal.about(&subject))
   }

   fn target_fields(
       name: Written<TargetName>,
       options: &Value,
       role: TargetRole,
   ) -> Result<StagedTarget, Refusal> {
       let options = table(options, "options")?;
       closed(&options, &TARGET_FIELDS)?;
       Ok(StagedTarget {
           name,
           role,
           rule: optional_text(&field(&options, "rule")?, "rule")?,
           run: optional_list(&field(&options, "run")?, "run")?,
           inputs: optional_list(&field(&options, "inputs")?, "inputs")?.unwrap_or_default(),
           deps: optional_list(&field(&options, "deps")?, "deps")?.unwrap_or_default(),
           outputs: optional_list(&field(&options, "outputs")?, "outputs")?,
           env: optional_list(&field(&options, "env")?, "env")?.unwrap_or_default(),
           network: match flag(&field(&options, "network")?, "network")? {
               Some(true) => NetworkAccess::Declared,
               Some(false) | None => NetworkAccess::Sealed,
           },
           freshness: match flag(&field(&options, "always")?, "always")? {
               Some(true) => Freshness::Always,
               Some(false) | None => Freshness::Cached,
           },
       })
   }

   /// `buildl.rule(name, options)`.
   pub(crate) fn rule(name: &Value, options: &Value) -> Result<StagedRule, Refusal> {
       let name: Written<TargetName> = text(name, "name")?;
       let subject = name.as_written().to_owned();
       rule_fields(name, options).map_err(|refusal| refusal.about(&subject))
   }

   fn rule_fields(name: Written<TargetName>, options: &Value) -> Result<StagedRule, Refusal> {
       let options = table(options, "options")?;
       closed(&options, &RULE_FIELDS)?;
       Ok(StagedRule {
           name,
           run: list(&field(&options, "run")?, "run")?,
           description: optional_text(&field(&options, "desc")?, "desc")?,
       })
   }

   /// `buildl.alias(name, target)`.
   pub(crate) fn alias(name: &Value, target: &Value) -> Result<StagedAlias, Refusal> {
       let name: Written<TargetName> = text(name, "name")?;
       let target = text(target, "target").map_err(|refusal| refusal.about(name.as_written()))?;
       Ok(StagedAlias { name, target })
   }

   /// `buildl.option(name, options)`.
   pub(crate) fn option(name: &Value, options: &Value) -> Result<StagedSetting, Refusal> {
       let name: Written<SettingName> = text(name, "name")?;
       let subject = name.as_written().to_owned();
       option_fields(name, options).map_err(|refusal| refusal.about(&subject))
   }

   fn option_fields(name: Written<SettingName>, options: &Value) -> Result<StagedSetting, Refusal> {
       let options = table(options, "options")?;
       closed(&options, &OPTION_FIELDS)?;
       Ok(StagedSetting {
           name,
           default: text(&field(&options, "default")?, "default")?,
       })
   }

   /// `buildl.subdir(path)`.
   pub(crate) fn subdir(path: &Value) -> Result<Written<Directory>, Refusal> {
       text(path, "path")
   }

   /// The pattern argument of `buildl.sources(pattern)`.
   pub(crate) fn pattern(pattern: &Value) -> Result<String, Refusal> {
       string(pattern, "pattern")
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib primitives::tests
   ```

   ```text
   test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): read each buildl primitive's arguments into a staged record`.

### Task 2 — The `buildl` host module

**Files:**
- Create `crates/buildl-lua/src/module.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod module;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/module.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! The `buildl` module table: the declaration API a build file calls.
   //!
   //! Its own file because it is the one airsl host module buildl writes. airsl installs it as
   //! `airsstack.buildl`, and its installation binds the same table to the global `buildl`. It also
   //! withholds the three globals that would let a build file's output vary between runs or reach the
   //! host's stdout: `math.random`, `math.randomseed` and `print`.
   //!
   //! Responsibilities: [`BuildlModule`]; turning a primitive call into a staged record or a sticky
   //! refusal that names the build file's line.
   //!
   //! Non-responsibilities: the argument shapes, which `primitives` owns, and the walk behind
   //! `buildl.sources`, which `sources` owns.

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
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::sync::{Arc, Mutex};

       use airsl::{Engine, LanguageSurface, ModuleSet, Policy, Script};
       use buildl_core::{
           DeclarationOrder, Diagnostic, Directory, EvaluationFailure, EvaluationLimit, Freshness,
           NetworkAccess, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
           StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
       };

       use super::BuildlModule;
       use crate::sources::Sources;
       use crate::staging::Staging;

       /// Evaluates `source` with only the `buildl` module installed, returning what `eval_to` gave
       /// and what was staged.
       fn run(
           source: &str,
           budget: u64,
       ) -> (
           airsl::Result<String>,
           Result<StagedFile, (EvaluationFailure, Diagnostic)>,
       ) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           fs::write(root.join("a.c"), "").unwrap();
           fs::write(root.join("b.c"), "").unwrap();
           let staging = Arc::new(Mutex::new(Staging::new(budget)));
           // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
           let mut modules = ModuleSet::new();
           modules
               .insert(Box::new(
                   BuildlModule::new(
                       Arc::clone(&staging),
                       Sources::new(&root, &Directory::root()),
                   )
                   .unwrap(),
               ))
               .unwrap();
           let engine = Engine::builder()
               .policy(Policy::confined().with_language(LanguageSurface::Minimal))
               .stdlib(modules)
               .build()
               .unwrap();
           let result = engine.eval_to::<String>(&Script::from_source(source, "build.lua").unwrap());
           drop(engine);
           let staged = std::mem::replace(&mut *staging.lock().unwrap(), Staging::new(0)).finish();
           (result, staged)
       }

       fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
           texts.iter().map(|text| Written::new(*text)).collect()
       }

       #[test]
       fn binds_one_table_under_two_names_without_inheritance() {
           let (result, _) = run(
               "return tostring(buildl == airsstack.buildl) .. ' ' .. tostring(buildl.json)",
               u64::MAX,
           );
           assert_eq!(result.unwrap(), "true nil");
       }

       #[test]
       fn withholds_random_and_print() {
           let (result, _) = run(
               "return type(math.random) .. type(math.randomseed) .. type(print) .. type(math.floor)",
               u64::MAX,
           );
           assert_eq!(result.unwrap(), "nilnilnilfunction");
       }

       #[test]
       fn stages_every_primitive_in_call_order() {
           let (result, staged) = run(
               "buildl.subdir('lib')\n\
                buildl.rule('cc', { run = { 'cc' } })\n\
                buildl.target('app', { rule = 'cc' })\n\
                buildl.test('app_test', { run = { 'app' } })\n\
                buildl.alias('default', 'app')\n\
                buildl.option('mode', { default = 'debug' })\n\
                return 'ok'",
               u64::MAX,
           );
           assert_eq!(result.unwrap(), "ok");
           let target = |name: &str, role, rule: Option<&str>, run: Option<&[&str]>| StagedTarget {
               name: Written::new(name),
               role,
               rule: rule.map(Written::new),
               run: run.map(strings),
               inputs: Vec::new(),
               deps: Vec::new(),
               outputs: None,
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           };
           let declared = |order, item| StagedDeclaration {
               order: DeclarationOrder::new(order),
               item,
           };
           assert_eq!(
               staged.unwrap(),
               StagedFile {
                   declarations: vec![
                       declared(
                           1,
                           StagedItem::Rule(StagedRule {
                               name: Written::new("cc"),
                               run: strings(&["cc"]),
                               description: None,
                           })
                       ),
                       declared(
                           2,
                           StagedItem::Target(target("app", TargetRole::Build, Some("cc"), None))
                       ),
                       declared(
                           3,
                           StagedItem::Target(target(
                               "app_test",
                               TargetRole::Test,
                               None,
                               Some(&["app"])
                           ))
                       ),
                       declared(
                           4,
                           StagedItem::Alias(StagedAlias {
                               name: Written::new("default"),
                               target: Written::new("app"),
                           })
                       ),
                       declared(
                           5,
                           StagedItem::Setting(StagedSetting {
                               name: Written::new("mode"),
                               default: Written::new("debug"),
                           })
                       ),
                   ],
                   subdirs: vec![StagedSubdir {
                       path: Written::new("lib"),
                       order: DeclarationOrder::new(0),
                   }],
               }
           );
       }

       #[test]
       fn a_refusal_names_the_line_and_fails_the_file() {
           let (result, staged) = run("\n\nbuildl.target('app', { run = 1 })", u64::MAX);
           let message = result.unwrap_err().to_string();
           assert!(
               message.contains(
                   "line 3: buildl.target 'app': field 'run' must be a list of strings, found integer"
               ),
               "{message}"
           );
           assert_eq!(
               staged.unwrap_err(),
               (
                   EvaluationFailure::WrongFieldType {
                       field: Written::new("run"),
                   },
                   Diagnostic::new(
                       "line 3: buildl.target 'app': field 'run' must be a list of strings, found integer"
                   ),
               )
           );
       }

       #[test]
       fn a_swallowed_refusal_still_fails_the_file_and_stops_later_calls() {
           let (result, staged) = run(
               "local ok = pcall(buildl.target, 'app', { srcs = {} })\n\
                local again = pcall(buildl.alias, 'default', 'app')\n\
                return tostring(ok) .. ' ' .. tostring(again)",
               u64::MAX,
           );
           assert_eq!(result.unwrap(), "false false");
           let (failure, diagnostic) = staged.unwrap_err();
           assert_eq!(
               failure,
               EvaluationFailure::UnknownField {
                   field: Written::new("srcs"),
               }
           );
           assert_eq!(
               diagnostic.as_str(),
               "line 1: buildl.target 'app': unknown field 'srcs'"
           );
       }

       #[test]
       fn a_call_past_the_budget_is_a_staging_limit() {
           let (result, staged) = run("buildl.subdir('lib')", 1);
           assert!(result.is_err());
           let (failure, diagnostic) = staged.unwrap_err();
           assert_eq!(
               failure,
               EvaluationFailure::LimitReached {
                   limit: EvaluationLimit::Staging,
               }
           );
           assert_eq!(
               diagnostic.as_str(),
               "line 1: buildl.subdir: staging budget of 1 bytes exceeded"
           );
       }

       #[test]
       fn sources_returns_a_sorted_lua_list() {
           let (result, staged) = run("return table.concat(buildl.sources('*.c'), ',')", u64::MAX);
           assert_eq!(result.unwrap(), "a.c,b.c");
           assert_eq!(
               staged.unwrap(),
               StagedFile {
                   declarations: Vec::new(),
                   subdirs: Vec::new(),
               }
           );
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib module::tests
   ```

   ```text
   error[E0432]: unresolved import `super::BuildlModule`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use std::sync::{Arc, Mutex, PoisonError};

   use airsl::mlua::{self, Lua, Table, Value};
   use airsl::{HostModule, InstallContext, ModuleName};
   use buildl_core::{EvaluationFailure, EvaluationLimit, StagedItem, TargetRole};

   use crate::primitives;
   use crate::refusal::Refusal;
   use crate::sources::Sources;
   use crate::staging::{Exceeded, Staging};

   /// The `buildl` host module for one build file's evaluation.
   #[derive(Debug)]
   pub(crate) struct BuildlModule {
       name: ModuleName,
       staging: Arc<Mutex<Staging>>,
       sources: Sources,
   }

   impl BuildlModule {
       /// A module staging into `staging`, whose `buildl.sources` walks `sources`.
       ///
       /// # Errors
       ///
       /// Never in practice: the only failure is `buildl` not being a valid module name.
       pub(crate) fn new(staging: Arc<Mutex<Staging>>, sources: Sources) -> airsl::Result<Self> {
           Ok(Self {
               name: ModuleName::new("buildl")?,
               staging,
               sources,
           })
       }
   }

   impl HostModule for BuildlModule {
       fn name(&self) -> &ModuleName {
           &self.name
       }

       fn install(
           &self,
           lua: &Lua,
           table: &Table,
           _context: &InstallContext<'_>,
       ) -> airsl::Result<()> {
           let fail = |error| airsl::Error::lua("buildl", error);

           for (primitive, role) in [("target", TargetRole::Build), ("test", TargetRole::Test)] {
               let staging = Arc::clone(&self.staging);
               let function = lua
                   .create_function(move |lua, (name, options): (Value, Value)| {
                       declare(lua, &staging, primitive, || {
                           primitives::target(&name, &options, role).map(StagedItem::Target)
                       })
                   })
                   .map_err(fail)?;
               table.set(primitive, function).map_err(fail)?;
           }

           let staging = Arc::clone(&self.staging);
           let rule = lua
               .create_function(move |lua, (name, options): (Value, Value)| {
                   declare(lua, &staging, "rule", || {
                       primitives::rule(&name, &options).map(StagedItem::Rule)
                   })
               })
               .map_err(fail)?;
           table.set("rule", rule).map_err(fail)?;

           let staging = Arc::clone(&self.staging);
           let alias = lua
               .create_function(move |lua, (name, target): (Value, Value)| {
                   declare(lua, &staging, "alias", || {
                       primitives::alias(&name, &target).map(StagedItem::Alias)
                   })
               })
               .map_err(fail)?;
           table.set("alias", alias).map_err(fail)?;

           let staging = Arc::clone(&self.staging);
           let option = lua
               .create_function(move |lua, (name, options): (Value, Value)| {
                   declare(lua, &staging, "option", || {
                       primitives::option(&name, &options).map(StagedItem::Setting)
                   })
               })
               .map_err(fail)?;
           table.set("option", option).map_err(fail)?;

           let staging = Arc::clone(&self.staging);
           let subdir = lua
               .create_function(move |lua, path: Value| {
                   guarded(lua, &staging, "subdir", |staging| {
                       let path = primitives::subdir(&path)?;
                       staging.stage_subdir(path).map_err(exceeded)
                   })
               })
               .map_err(fail)?;
           table.set("subdir", subdir).map_err(fail)?;

           let staging = Arc::clone(&self.staging);
           let walk = self.sources.clone();
           let sources = lua
               .create_function(move |lua, pattern: Value| {
                   let found = guarded(lua, &staging, "sources", |_| {
                       walk.find(&primitives::pattern(&pattern)?)
                   })?;
                   lua.create_sequence_from(found)
               })
               .map_err(fail)?;
           table.set("sources", sources).map_err(fail)?;

           let globals = lua.globals();
           globals.set("buildl", table.clone()).map_err(fail)?;
           let math: Table = globals.get("math").map_err(fail)?;
           math.set("random", Value::Nil).map_err(fail)?;
           math.set("randomseed", Value::Nil).map_err(fail)?;
           globals.set("print", Value::Nil).map_err(fail)?;
           Ok(())
       }
   }

   /// Reads a declaration with `read` and stages it.
   fn declare(
       lua: &Lua,
       staging: &Mutex<Staging>,
       primitive: &'static str,
       read: impl FnOnce() -> Result<StagedItem, Refusal>,
   ) -> mlua::Result<()> {
       guarded(lua, staging, primitive, |staging| {
           staging.stage(read()?).map_err(exceeded)
       })
   }

   /// Runs one primitive call against the staging buffer.
   ///
   /// A file that already failed fails every later call with the same message. A new refusal is
   /// recorded — so a `pcall` that swallows it does not un-fail the file — and raised as a Lua error
   /// naming the line that made the call.
   fn guarded<T>(
       lua: &Lua,
       staging: &Mutex<Staging>,
       primitive: &'static str,
       work: impl FnOnce(&mut Staging) -> Result<T, Refusal>,
   ) -> mlua::Result<T> {
       let line = call_site(lua);
       let mut staging = staging.lock().unwrap_or_else(PoisonError::into_inner);
       let recorded = staging.recorded().map(ToString::to_string);
       let outcome = recorded.map_or_else(
           || {
               work(&mut staging).map_err(|refusal| {
                   let diagnostic = refusal.diagnostic(primitive, line);
                   let message = diagnostic.to_string();
                   staging.record(refusal.failure().clone(), diagnostic);
                   message
               })
           },
           Err,
       );
       drop(staging);
       outcome.map_err(mlua::Error::RuntimeError)
   }

   /// The line of the nearest Lua frame on the stack: the build-file line that made the call, even
   /// when it went through `pcall`.
   fn call_site(lua: &Lua) -> Option<usize> {
       #[expect(
           clippy::redundant_closure_for_method_calls,
           reason = "`Debug::current_line` as a path is not general over `Debug`'s lifetime"
       )]
       (1..)
           .map_while(|level| lua.inspect_stack(level, |debug| debug.current_line()))
           .flatten()
           .next()
   }

   fn exceeded(exceeded: Exceeded) -> Refusal {
       Refusal::new(
           EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Staging,
           },
           exceeded.to_string(),
       )
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib module::tests
   ```

   ```text
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): install the buildl table under both names with sticky refusals`.

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --lib primitives::tests` reports 9 passed and `module::tests` 7 passed.
- `cargo make dod` exits `0`.

## Review findings

- unit-test-mandate / reversion guard (🟡) — no test proved that a `buildl.sources` refusal sticks (spec §6.2, §7.3). If `walk.find` moved outside `guarded`, all 7 module tests still passed — `crates/buildl-lua/src/module.rs:122`. **Fixed and verified.** Added `a_swallowed_sources_refusal_still_fails_the_file_and_stops_later_calls`. With `walk.find` moved outside `guarded`, the test failed (left `"false true"`, right `"false false"`). With the closure restored, `cargo test -p buildl-lua --lib module::tests` gave `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out`.
- correctness (🟡) — the staging mutex stayed locked while `work` called the Lua C API. A `__gc` finalizer that re-entered a buildl primitive on the same thread would self-deadlock, and no instruction or memory limit can interrupt that — `module.rs:168`. The reviewer's scratch probe reproduced the re-entry when the code read fresh keys, but not with the 8 real target field names. **Fixed.** `guarded` now uses `try_lock`. `WouldBlock` raises `buildl.<primitive>: buildl primitives cannot be called while another buildl call is in progress` and records no sticky refusal; a poisoned lock is still recovered through `into_inner`. A test, `a_call_while_the_staging_buffer_is_in_use_fails_instead_of_waiting`, holds the lock and asserts the call returns `false` with that message, and that nothing was recorded. That test passes (in the 9 above). It was written after the fix and was never seen failing; without the fix it would block on a same-thread re-lock.
- unit-test-mandate (🔵) — `assert!(result.is_err())` does not check the raised message. The staged assertion already covers the diagnostic — `module.rs:411`
- spec drift (🔵) — spec §5.2 says `BuildlModule` holds "the root and the declaring directory". It holds a `Sources` that wraps those two values. They are equivalent, but the spec sentence was never amended — spec §5.2

Blocking set: none. The reviewer re-ran `cargo make dod` and it exited 0. The reviewer also confirmed that the code matches the plan's blocks, that the `redundant_closure_for_method_calls` expectation's reason is correct, and that the readers never reach `__index`, `__len` or `__pairs` (mlua 0.12.1 `pairs` uses `lua_next`).

## Probe results

- Every asserted fact was structural or covered by the tasks' own red-green steps. Task 1 gave `error[E0432]: unresolved imports super::alias, …`, then `test result: ok. 9 passed`. Task 2 gave `error[E0432]: unresolved import super::BuildlModule`, then `test result: ok. 7 passed`.
- Batch gate: `cargo make dod` gave a buildl-lua lib total of `test result: ok. 57 passed; 0 failed`, and `Build Done in 2.73 seconds`. After the fix round, the coder's `cargo make dod` gave buildl-lua `59 passed` and `Build Done`.

## Deviations

- 2026-10-08 — `module.rs` goes beyond the plan text. `guarded` takes the lock with `try_lock` and raises a runtime error on re-entry instead of blocking. The `run` test helper is split into `evaluate` and `run`, and two tests were added, so the module total is 9 instead of 7. Both changes came from review findings.
- 2026-10-08 — One coder ran Tasks 1–2 in order, because Task 2 imports Task 1's `primitives`. The plan's per-task commits were not made; commits are left to the user.
