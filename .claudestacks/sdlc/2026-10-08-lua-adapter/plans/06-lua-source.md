---
status: done
created: 2026-10-08
depends-on: [05]
---

# LuaSource Implementation Plan

**Goal:** `LuaSource` evaluates one build file on a fresh, curated airsl engine and returns what it staged or the one failure that stopped it.

**Architecture:** Five files. `curated.rs` wraps airsl's `path` module and drops `absolute`, the one function in it that reads the working directory. `classify.rs` names an airsl failure as an `EvaluationFailure`: instruction or memory ceiling, `Denied` → `Refused`, a syntax error → `Syntax`, anything else → `Runtime`. `limits.rs` is the public `DeclarationLimits`: 16 MiB and 10 000 000 instructions by default, with the memory ceiling doubling as the staging budget. `engine.rs` builds the engine: minimal surface, no grants, `json`, curated `path`, `regex`, `hash`, `glob`, then `buildl`. `source.rs` is the public `LuaSource`. It reads the file itself (`NotFound` → `Absent`, non-UTF-8 → `Syntax`), evaluates it with `Script::from_source` (so there is no `require`), drops the engine, and takes the buffer out under the lock. A recorded refusal is reported before the eval result, then an airsl failure is classified.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §3 (public API), §4 (one evaluation), §5.1 (engine), §8 (classification), D7, D9, D10, D13; §1.1 P5–P15, S2 (probe results).

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
| `BuildlModule::new(staging, sources)`, `Staging::{new, finish}`, `Sources::new` | plans `02`–`05` | the evaluation |
| `airsl::{Engine, GrantSet, LanguageSurface, ModuleSet, Policy, Script, InstructionLimit, MemoryLimit, ResourceLimits}`, `airsl::modules::{Json, Path, Regex, Hash, Glob}` | airsl 0.1.4 | the engine |
| `airsl::Error::{InstructionLimit, MemoryLimit, Lua, Denied}`, `mlua::Error::SyntaxError`, `mlua::Error::downcast_ref` | airsl / mlua | classification (P5, P7, P8: `downcast_ref` reaches a callback's external error) |
| `DeclarationSource`, `BuildFile` (`file`, `directory`, `provenance`), `Evaluated`, `Error::Evaluation { provenance, failure, diagnostic }`, `Diagnostic` | `buildl-core` | the port |

Task order matters for the gate. The first four tasks add modules that nothing public uses yet, so
each carries the same dead-code expectation as the earlier plans. `limits.rs` is public from Task 3,
but its `resource_limits` stays dead until Task 5: Task 4's `engine::build` calls it, but nothing
public calls `engine::build` yet. Its expectation names the method for that reason. Task 5 makes
everything live and deletes every expectation at once.

## File structure

```text
crates/buildl-lua/src/curated.rs  — [create] CuratedPath, with a unit test                     (Task 1)
crates/buildl-lua/src/classify.rs — [create] classify, with unit tests                         (Task 2)
crates/buildl-lua/src/limits.rs   — [create] DeclarationLimits, with unit tests and a doctest  (Task 3)
crates/buildl-lua/src/engine.rs   — [create] the engine builder, with unit tests               (Task 4)
crates/buildl-lua/src/source.rs   — [create] LuaSource, with unit tests and a doctest          (Task 5)
crates/buildl-lua/src/{staging,refusal,values,sources,primitives,module,curated,classify,engine,limits}.rs
                                  — [modify] delete the dead-code expectation                   (Task 5)
crates/buildl-lua/src/lib.rs      — [modify] declare the modules; re-export the two public types (Tasks 1–5)
```

### Task 1 — airsl's `path` without `absolute`

**Files:**
- Create `crates/buildl-lua/src/curated.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod curated;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/curated.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! airsl's `path` module without its one ambient read.
   //!
   //! Its own file because it changes an airsl module rather than adding one. `airsstack.path` is
   //! pure path arithmetic except `absolute`, which resolves against the process working directory:
   //! a build file using it would declare different text depending on where `buildl` was run from,
   //! and that difference cannot be seen by evaluating the file twice in one process. The
   //! `airsstack` table does not exist yet while modules install, so the function is removed from
   //! the module's own table, here, rather than from `airsstack.path` afterwards.
   //!
   //! Responsibilities: [`CuratedPath`].
   //!
   //! Non-responsibilities: everything else `path` does, which is airsl's.

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

       use airsl::{Engine, ModuleSet, Policy, Script};

       use super::CuratedPath;

       #[test]
       fn keeps_path_arithmetic_and_drops_absolute() {
           // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
           let mut modules = ModuleSet::new();
           modules.insert(Box::new(CuratedPath::default())).unwrap();
           let engine = Engine::builder()
               .policy(Policy::pure())
               .stdlib(modules)
               .build()
               .unwrap();
           let result = engine
               .eval_to::<String>(
                   &Script::from_source(
                       "return type(airsstack.path.absolute) .. ' ' .. airsstack.path.stem('src/a.c')",
                       "build.lua",
                   )
                   .unwrap(),
               )
               .unwrap();
           assert_eq!(result, "nil a");
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib curated::tests
   ```

   ```text
   error[E0432]: unresolved import `super::CuratedPath`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use airsl::mlua::{Lua, Table, Value};
   use airsl::modules::Path;
   use airsl::{HostModule, InstallContext, ModuleName};

   /// `airsstack.path`, minus `absolute`.
   #[derive(Debug, Default)]
   pub(crate) struct CuratedPath(Path);

   impl HostModule for CuratedPath {
       fn name(&self) -> &ModuleName {
           self.0.name()
       }

       fn install(&self, lua: &Lua, table: &Table, context: &InstallContext<'_>) -> airsl::Result<()> {
           self.0.install(lua, table, context)?;
           table
               .set("absolute", Value::Nil)
               .map_err(|error| airsl::Error::lua("path", error))
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib curated::tests
   ```

   ```text
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): curate airsl's path module without absolute`.

### Task 2 — Naming an airsl failure

**Files:**
- Create `crates/buildl-lua/src/classify.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod classify;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/classify.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! Naming the kind of an airsl evaluation failure.
   //!
   //! Its own file because the mapping is the one place airsl's error vocabulary meets buildl's, and
   //! an airsl upgrade that reshapes its errors should have to change only this. A refusal raised by
   //! a `buildl` primitive never reaches here: it was recorded when it was raised, and that record
   //! is reported instead.
   //!
   //! Responsibilities: [`classify`].
   //!
   //! Non-responsibilities: the diagnostic's text, which is airsl's own rendering of the error.

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

       use airsl::modules::Glob;
       use airsl::{
           Engine, InstructionLimit, LanguageSurface, MemoryLimit, ModuleSet, Policy, ResourceLimits,
           Script,
       };
       use buildl_core::{EvaluationFailure, EvaluationLimit};

       use super::classify;

       fn failure_of(source: &str) -> EvaluationFailure {
           // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
           let mut modules = ModuleSet::new();
           modules.insert(Box::new(Glob::new())).unwrap();
           let engine = Engine::builder()
               .policy(
                   Policy::confined()
                       .with_language(LanguageSurface::Minimal)
                       .with_limits(ResourceLimits::new(
                           Some(MemoryLimit::mebibytes(16)),
                           Some(InstructionLimit::count(1_000_000)),
                       )),
               )
               .stdlib(modules)
               .build()
               .unwrap();
           let error = engine
               .eval(&Script::from_source(source, "build.lua").unwrap())
               .unwrap_err();
           classify(&error)
       }

       #[test]
       fn any_other_airsl_failure_is_a_runtime_failure() {
           // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
           let mut modules = ModuleSet::new();
           modules.insert(Box::new(Glob::new())).unwrap();
           let duplicate = modules.insert(Box::new(Glob::new())).unwrap_err();
           assert!(
               matches!(duplicate, airsl::Error::DuplicateModule { .. }),
               "{duplicate}"
           );
           assert_eq!(classify(&duplicate), EvaluationFailure::Runtime);
       }

       #[test]
       fn a_chunk_that_does_not_compile_is_a_syntax_failure() {
           assert_eq!(failure_of("return ("), EvaluationFailure::Syntax);
       }

       #[test]
       fn a_raised_error_is_a_runtime_failure() {
           assert_eq!(failure_of("error('boom')"), EvaluationFailure::Runtime);
           assert_eq!(failure_of("local x = nil; x()"), EvaluationFailure::Runtime);
       }

       #[test]
       fn a_denied_operation_is_a_refusal() {
           assert_eq!(
               failure_of("airsstack.glob.walk('/', '*')"),
               EvaluationFailure::Refused
           );
       }

       #[test]
       fn an_endless_loop_reaches_the_instruction_ceiling() {
           assert_eq!(
               failure_of("while true do end"),
               EvaluationFailure::LimitReached {
                   limit: EvaluationLimit::Instructions,
               }
           );
       }

       #[test]
       fn an_unbounded_allocation_reaches_the_memory_ceiling() {
           assert_eq!(
               failure_of("local s = string.rep('x', 64 * 1024 * 1024)"),
               EvaluationFailure::LimitReached {
                   limit: EvaluationLimit::Memory,
               }
           );
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib classify::tests
   ```

   ```text
   error[E0432]: unresolved import `super::classify`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use airsl::mlua;
   use buildl_core::{EvaluationFailure, EvaluationLimit};

   /// The kind of failure `error` is.
   pub(crate) fn classify(error: &airsl::Error) -> EvaluationFailure {
       match error {
           airsl::Error::InstructionLimit { .. } => EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Instructions,
           },
           airsl::Error::MemoryLimit { .. } => EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Memory,
           },
           airsl::Error::Lua { source, .. } => {
               if matches!(
                   source.downcast_ref::<airsl::Error>(),
                   Some(airsl::Error::Denied { .. })
               ) {
                   EvaluationFailure::Refused
               } else if matches!(**source, mlua::Error::SyntaxError { .. }) {
                   EvaluationFailure::Syntax
               } else {
                   EvaluationFailure::Runtime
               }
           }
           _ => EvaluationFailure::Runtime,
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib classify::tests
   ```

   ```text
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): classify airsl failures as evaluation failures`.

### Task 3 — `DeclarationLimits`

**Files:**
- Create `crates/buildl-lua/src/limits.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod limits;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration. Also add a blank line after the `mod` block and then `pub use limits::DeclarationLimits;`, because the doctest names the type through the crate root.
2. Create `crates/buildl-lua/src/limits.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! The ceilings a build file's evaluation runs under.
   //!
   //! Its own file because the limits are the adapter's only configuration, and the composition root
   //! fills them from the workspace manifest's `[declaration]` table. They are this crate's own type
   //! so that no caller has to name an airsl type to configure the adapter.
   //!
   //! Responsibilities: [`DeclarationLimits`] and its conversion into airsl's resource limits.
   //!
   //! Non-responsibilities: parsing the manifest, which happens before an adapter is built.

   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "`resource_limits` has no caller until `LuaSource` configures an engine"
       )
   )]

   #[cfg(test)]
   mod tests {
       use core::num::NonZeroU64;

       use airsl::{InstructionLimit, MemoryLimit};

       use super::DeclarationLimits;

       #[test]
       fn defaults_to_sixteen_mebibytes_and_ten_million_instructions() {
           let limits = DeclarationLimits::default();
           assert_eq!(limits.memory_bytes().get(), 16_777_216);
           assert_eq!(limits.instructions().get(), 10_000_000);
       }

       #[test]
       fn converts_into_both_airsl_ceilings() {
           let limits = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN.saturating_add(1));
           let resource = limits.resource_limits();
           assert_eq!(resource.memory(), Some(MemoryLimit::bytes(1)));
           assert_eq!(resource.instructions(), Some(InstructionLimit::count(2)));
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib limits::tests
   ```

   ```text
   error[E0432]: unresolved import `super::DeclarationLimits`
   error[E0432]: unresolved import `limits::DeclarationLimits`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use core::num::NonZeroU64;

   use airsl::{InstructionLimit, MemoryLimit, ResourceLimits};

   /// 16 MiB, the declaration memory ceiling when the manifest sets none.
   const DEFAULT_MEMORY_BYTES: NonZeroU64 = NonZeroU64::MIN.saturating_add(16 * 1024 * 1024 - 1);
   /// The declaration instruction ceiling when the manifest sets none.
   const DEFAULT_INSTRUCTIONS: NonZeroU64 = NonZeroU64::MIN.saturating_add(10_000_000 - 1);

   /// The memory and instruction ceilings for evaluating one build file.
   ///
   /// The memory ceiling bounds the Lua heap, and it is also the byte budget for the declarations the
   /// file stages outside that heap.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::num::NonZeroU64;
   ///
   /// use buildl_lua::DeclarationLimits;
   ///
   /// let defaults = DeclarationLimits::default();
   /// assert_eq!(defaults.memory_bytes().get(), 16 * 1024 * 1024);
   /// assert_eq!(defaults.instructions().get(), 10_000_000);
   ///
   /// let tight = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN);
   /// assert_eq!(tight.memory_bytes().get(), 1);
   /// ```
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub struct DeclarationLimits {
       memory_bytes: NonZeroU64,
       instructions: NonZeroU64,
   }

   impl DeclarationLimits {
       /// Limits of `memory_bytes` bytes and `instructions` Lua VM instructions per build file.
       #[must_use]
       pub const fn new(memory_bytes: NonZeroU64, instructions: NonZeroU64) -> Self {
           Self {
               memory_bytes,
               instructions,
           }
       }

       /// The memory ceiling, and staging budget, in bytes.
       #[must_use]
       pub const fn memory_bytes(&self) -> NonZeroU64 {
           self.memory_bytes
       }

       /// The instruction ceiling.
       #[must_use]
       pub const fn instructions(&self) -> NonZeroU64 {
           self.instructions
       }

       /// The same ceilings in airsl's terms. A memory ceiling beyond the address space is clamped to
       /// it.
       pub(crate) fn resource_limits(&self) -> ResourceLimits {
           let memory = usize::try_from(self.memory_bytes.get()).unwrap_or(usize::MAX);
           ResourceLimits::new(
               Some(MemoryLimit::bytes(memory)),
               Some(InstructionLimit::count(self.instructions.get())),
           )
       }
   }

   impl Default for DeclarationLimits {
       /// 16 MiB and 10 000 000 instructions.
       fn default() -> Self {
           Self::new(DEFAULT_MEMORY_BYTES, DEFAULT_INSTRUCTIONS)
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib limits::tests
   ```

   ```text
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): add DeclarationLimits`.

### Task 4 — The curated engine

**Files:**
- Create `crates/buildl-lua/src/engine.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod engine;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration.
2. Create `crates/buildl-lua/src/engine.rs` holding the module docs, the dead-code expectation and the tests only:

   ```rust
   //! The airsl engine one build file is evaluated on.
   //!
   //! Its own file because the engine's configuration is the declaration phase's sandbox, and it is
   //! stated once, here. The language surface is airsl's minimal one (no `os`, no `coroutine`, no
   //! `require`), the engine holds no grant of any kind, and the host modules are a curated set:
   //! `json`, `path` without `absolute`, `regex`, `hash` and `glob`, then `buildl`. With no grant,
   //! `glob.walk` is refused; `buildl.sources` is the only filesystem read.
   //!
   //! Responsibilities: [`build`].
   //!
   //! Non-responsibilities: what the `buildl` module installs, which `module` owns.

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

       use airsl::{Engine, Script};
       use buildl_core::Directory;

       use super::build;
       use crate::limits::DeclarationLimits;
       use crate::module::BuildlModule;
       use crate::sources::Sources;
       use crate::staging::Staging;

       fn engine() -> (tempfile::TempDir, Engine) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           let module = BuildlModule::new(
               Arc::new(Mutex::new(Staging::new(u64::MAX))),
               Sources::new(&root, &Directory::root()),
           )
           .unwrap();
           (dir, build(&DeclarationLimits::default(), module).unwrap())
       }

       fn eval(engine: &Engine, source: &str) -> airsl::Result<String> {
           engine.eval_to::<String>(&Script::from_source(source, "build.lua").unwrap())
       }

       #[test]
       fn installs_the_curated_modules_and_nothing_else() {
           let (_dir, engine) = engine();
           let names: Vec<&str> = engine
               .module_names()
               .iter()
               .map(|name| name.as_str())
               .collect();
           assert_eq!(names, ["json", "path", "regex", "hash", "glob", "buildl"]);
       }

       #[test]
       fn withholds_the_ambient_globals() {
           let (_dir, engine) = engine();
           let found = eval(
               &engine,
               "return table.concat({ type(os), type(io), type(coroutine), type(require), \
                type(load), type(dofile), type(debug), type(print), type(math.random), \
                type(airsstack.path.absolute) }, ',')",
           )
           .unwrap();
           assert_eq!(found, "nil,nil,nil,nil,nil,nil,nil,nil,nil,nil");
       }

       #[test]
       fn glob_walk_has_no_grant_to_read_with() {
           let (dir, engine) = engine();
           let root = fs::canonicalize(dir.path()).unwrap();
           let source = format!(
               "return table.concat(airsstack.glob.walk('{}', '*'), ',')",
               root.display()
           );
           let message = eval(&engine, &source).unwrap_err().to_string();
           assert!(message.contains("glob.walk denied"), "{message}");
       }
   }
   ```

   The `#![cfg_attr(not(test), expect(dead_code, …))]` attribute is deliberate. Nothing public reaches this module until `LuaSource` exists, so its items are dead code in the library build, and `-D warnings` would fail on them. `expect` rather than `allow` means the attribute turns into an error the moment the code becomes live. The task that makes it live removes the attribute.
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib engine::tests
   ```

   ```text
   error[E0432]: unresolved import `super::build`
   ```
4. Insert the implementation between the module header (after the `#![cfg_attr(…)]` attribute) and `#[cfg(test)]`, so the file reads: docs, attribute, this code, tests:

   ```rust
   use airsl::modules::{Glob, Hash, Json, Regex};
   use airsl::{Engine, GrantSet, LanguageSurface, ModuleSet, Policy};

   use crate::curated::CuratedPath;
   use crate::limits::DeclarationLimits;
   use crate::module::BuildlModule;

   /// A fresh engine under `limits`, with `buildl` installed last.
   ///
   /// # Errors
   ///
   /// Returns airsl's error when the engine cannot be configured or a module cannot be installed.
   pub(crate) fn build(limits: &DeclarationLimits, buildl: BuildlModule) -> airsl::Result<Engine> {
       // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
       let mut modules = ModuleSet::new();
       modules.insert(Box::new(Json::new()))?;
       modules.insert(Box::new(CuratedPath::default()))?;
       modules.insert(Box::new(Regex::new()))?;
       modules.insert(Box::new(Hash::new()))?;
       modules.insert(Box::new(Glob::new()))?;
       modules.insert(Box::new(buildl))?;
       let policy = Policy::confined()
           .with_language(LanguageSurface::Minimal)
           .with_grants(GrantSet::declared())
           .with_limits(limits.resource_limits());
       Engine::builder().policy(policy).stdlib(modules).build()
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib engine::tests
   ```

   ```text
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): build the curated, grantless declaration engine`.

### Task 5 — `LuaSource`, and every module made live

**Files:**
- Create `crates/buildl-lua/src/source.rs`
- Modify `crates/buildl-lua/src/lib.rs`

**Steps:**

1. Declare the module in `crates/buildl-lua/src/lib.rs`: add the line `mod source;` to the block of `mod` declarations after the crate docs, keeping that block sorted. If the block does not exist yet, add a blank line after the last `//!` line, then the declaration. Also add `pub use source::LuaSource;` below `pub use limits::DeclarationLimits;`.
2. Create `crates/buildl-lua/src/source.rs` holding the module docs and the tests only:

   ```rust
   //! [`LuaSource`], the [`DeclarationSource`] that evaluates build files with airsl.
   //!
   //! Its own file because it is the adapter's entry point: it turns one [`BuildFile`] into either
   //! the declarations the file staged or the one failure that stopped it. Every evaluation gets a
   //! fresh engine and a fresh staging buffer, so nothing one file does can reach the next.
   //!
   //! Responsibilities: [`LuaSource`] and its [`DeclarationSource`] implementation.
   //!
   //! Non-responsibilities: deciding which build files to evaluate, and what their declarations
   //! mean. Both are the caller's.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           clippy::panic,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{
           BuildFile, DeclarationSource, Directory, Error, Evaluated, EvaluationFailure, Provenance,
       };

       use super::LuaSource;
       use crate::limits::DeclarationLimits;

       fn root_file() -> BuildFile {
           BuildFile::new(Provenance::new(
               PathBuf::from("build.lua"),
               Directory::root(),
           ))
       }

       fn workspace(build_lua: &[u8]) -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           fs::write(root.join("build.lua"), build_lua).unwrap();
           (dir, root)
       }

       fn failure(root: &Path) -> EvaluationFailure {
           match LuaSource::new(root, DeclarationLimits::default()).evaluate(&root_file()) {
               Err(Error::Evaluation { failure, .. }) => failure,
               other => panic!("expected an evaluation failure, got {other:?}"),
           }
       }

       #[test]
       fn a_missing_file_is_absent() {
           let dir = tempfile::tempdir().unwrap();
           let source = LuaSource::new(dir.path(), DeclarationLimits::default());
           assert_eq!(source.evaluate(&root_file()).unwrap(), Evaluated::Absent);
       }

       #[test]
       fn a_file_that_is_not_utf8_is_a_syntax_failure() {
           let (_dir, root) = workspace(b"return '\xff'");
           assert_eq!(failure(&root), EvaluationFailure::Syntax);
       }

       #[test]
       fn a_recorded_refusal_is_reported_over_the_eval_result() {
           let (_dir, root) = workspace(b"pcall(buildl.target, 'app', 1)\nreturn 'ok'");
           assert_eq!(
               failure(&root),
               EvaluationFailure::WrongFieldType {
                   field: buildl_core::Written::new("options"),
               }
           );
       }

       #[test]
       fn an_airsl_failure_is_classified() {
           let (_dir, root) = workspace(b"error('boom')");
           assert_eq!(failure(&root), EvaluationFailure::Runtime);
       }

       #[test]
       fn evaluation_failures_carry_the_file_s_provenance() {
           let (_dir, root) = workspace(b"return (");
           let error = LuaSource::new(&root, DeclarationLimits::default())
               .evaluate(&root_file())
               .unwrap_err();
           assert!(
               error.to_string().starts_with("build.lua: syntax error: "),
               "{error}"
           );
       }

       #[test]
       fn a_staged_file_is_returned_with_its_declarations() {
           let (_dir, root) = workspace(b"buildl.subdir('lib')\nbuildl.alias('default', 'app')");
           let Evaluated::Staged(staged) = LuaSource::new(&root, DeclarationLimits::default())
               .evaluate(&root_file())
               .unwrap()
           else {
               panic!("the build file exists");
           };
           assert_eq!(staged.declarations.len(), 1);
           assert_eq!(staged.subdirs.len(), 1);
       }
   }
   ```
3. Run the tests and confirm they fail to compile:

   ```bash
   cargo test -p buildl-lua --lib source::tests
   ```

   ```text
   error[E0432]: unresolved import `super::LuaSource`
   error[E0432]: unresolved import `source::LuaSource`
   ```
4. Insert the implementation between the module header (after the last `//!` line) and `#[cfg(test)]`, so the file reads: docs, this code, tests:

   ```rust
   use std::fs;
   use std::io::ErrorKind;
   use std::mem;
   use std::path::PathBuf;
   use std::sync::{Arc, Mutex, PoisonError};

   use airsl::Script;
   use buildl_core::{
       BuildFile, DeclarationSource, Diagnostic, Error, Evaluated, EvaluationFailure, Result,
   };

   use crate::classify::classify;
   use crate::engine;
   use crate::limits::DeclarationLimits;
   use crate::module::BuildlModule;
   use crate::sources::Sources;
   use crate::staging::Staging;

   /// Evaluates build files on the airsl embedded runtime, one fresh engine per file.
   ///
   /// A build file runs on airsl's minimal Lua surface with no grants, under `limits`. It sees
   /// the `buildl` module table as both `buildl` and `airsstack.buildl`, and the curated modules
   /// `json`, `path`, `regex`, `hash` and `glob` under `airsstack`.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::fs;
   /// use std::path::PathBuf;
   ///
   /// use buildl_core::{BuildFile, DeclarationSource, Directory, Evaluated, Provenance};
   /// use buildl_lua::{DeclarationLimits, LuaSource};
   ///
   /// let workspace = tempfile::tempdir()?;
   /// let root = fs::canonicalize(workspace.path())?;
   /// fs::write(root.join("build.lua"), "buildl.alias('default', 'app')")?;
   ///
   /// let source = LuaSource::new(&root, DeclarationLimits::default());
   /// let file = BuildFile::new(Provenance::new(PathBuf::from("build.lua"), Directory::root()));
   /// assert!(matches!(
   ///     source.evaluate(&file)?,
   ///     Evaluated::Staged(staged) if staged.declarations.len() == 1
   /// ));
   /// # Ok::<(), Box<dyn std::error::Error>>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct LuaSource {
       root: PathBuf,
       limits: DeclarationLimits,
   }

   impl LuaSource {
       /// A source for the workspace at `root`, evaluating under `limits`.
       ///
       /// `root` must be the workspace root's canonical, absolute path. `buildl.sources` walks
       /// beneath it and refuses any directory that resolves outside it.
       #[must_use]
       pub fn new(root: impl Into<PathBuf>, limits: DeclarationLimits) -> Self {
           Self {
               root: root.into(),
               limits,
           }
       }
   }

   impl DeclarationSource for LuaSource {
       /// Evaluates `file`.
       ///
       /// A file that does not exist is [`Evaluated::Absent`].
       ///
       /// # Errors
       ///
       /// Returns [`Error::Evaluation`] when the file cannot be read, is not UTF-8, does not parse,
       /// raises, is refused an operation, reaches a ceiling, or calls a `buildl` primitive with
       /// arguments it does not accept. A refusal from a primitive is reported even when the build
       /// file caught it with `pcall` and carried on.
       fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
           let path = self.root.join(file.file());
           let bytes = match fs::read(&path) {
               Ok(bytes) => bytes,
               Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Evaluated::Absent),
               Err(error) => {
                   return Err(failed(
                       file,
                       EvaluationFailure::Runtime,
                       format!("{}: {error}", path.display()),
                   ));
               }
           };
           let Ok(text) = String::from_utf8(bytes) else {
               return Err(failed(file, EvaluationFailure::Syntax, "not UTF-8 text"));
           };

           let staging = Arc::new(Mutex::new(Staging::new(self.limits.memory_bytes().get())));
           let outcome = BuildlModule::new(
               Arc::clone(&staging),
               Sources::new(&self.root, file.directory()),
           )
           .and_then(|module| engine::build(&self.limits, module))
           .and_then(|engine| {
               let chunk = file.file().to_string_lossy().into_owned();
               engine.eval(&Script::from_source(text, chunk)?)
           });

           let staged = mem::replace(
               &mut *staging.lock().unwrap_or_else(PoisonError::into_inner),
               Staging::new(0),
           );
           let staged = staged
               .finish()
               .map_err(|(failure, diagnostic)| Error::Evaluation {
                   provenance: file.provenance().clone(),
                   failure,
                   diagnostic,
               })?;
           outcome.map_err(|error| failed(file, classify(&error), error.to_string()))?;
           Ok(Evaluated::Staged(staged))
       }
   }

   fn failed(file: &BuildFile, failure: EvaluationFailure, diagnostic: impl Into<String>) -> Error {
       Error::Evaluation {
           provenance: file.provenance().clone(),
           failure,
           diagnostic: Diagnostic::new(diagnostic),
       }
   }
   ```
5. Run the tests and confirm they pass:

   ```bash
   cargo test -p buildl-lua --lib source::tests
   ```

   ```text
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; …
   ```
6. Before running the gate, delete the dead-code expectation from every module that had one. With `LuaSource` public, all of them are now reachable, and an unfulfilled `expect` is itself an error. In each of `src/staging.rs`, `src/refusal.rs`, `src/values.rs`, `src/sources.rs`, `src/primitives.rs`, `src/module.rs`, `src/curated.rs`, `src/classify.rs`, `src/engine.rs`, delete this block (and the blank line before it):

   ```rust
   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until `LuaSource` evaluates a build file"
       )
   )]
   ```

   In `src/limits.rs`, delete this block (and the blank line before it):

   ```rust
   #![cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "`resource_limits` has no caller until `LuaSource` configures an engine"
       )
   )]
   ```

   Then run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `feat(buildl-lua): evaluate a build file with LuaSource`.

   After this task `crates/buildl-lua/src/lib.rs` reads:

   ```rust
   //! Lua build-file evaluation for buildl, on the airsl embedded runtime.
   //!
   //! This crate is the only part of buildl that depends on a Lua runtime, so
   //! the effect of an airsl upgrade stays within one crate.
   //!
   //! # Responsibilities
   //!
   //! - Evaluating one build file per call inside an airsl sandbox and returning
   //!   the declarations it staged.
   //! - Installing the `buildl` module table, reachable from Lua both as
   //!   `airsstack.buildl` and as the global `buildl`.
   //!
   //! # Non-responsibilities
   //!
   //! - Resolving, planning, scheduling, caching, or executing anything a build
   //!   file declares.
   //!
   //! This file holds only module declarations and re-exports, so it carries no
   //! logic to unit-test.

   mod classify;
   mod curated;
   mod engine;
   mod limits;
   mod module;
   mod primitives;
   mod refusal;
   mod source;
   mod sources;
   mod staging;
   mod values;

   pub use limits::DeclarationLimits;
   pub use source::LuaSource;
   ```

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --lib` reports 75 passed on macOS, 76 on Linux (the Linux-only non-UTF-8 `sources` test).
- `cargo test -p buildl-lua --doc` reports 2 passed (`DeclarationLimits`, `LuaSource`).
- `grep -rn dead_code crates/buildl-lua/src` prints nothing.
- `cargo make dod` and `cargo deny check` both pass.

## Review findings

- unit-test-mandate / reversion guard (🟡) — nothing checked that `DeclarationLimits` reaches the engine. If `.with_limits(limits.resource_limits())` were dropped, airsl's `Policy::confined()` defaults would apply silently (64 MiB and 100 000 000 instructions; airsl-0.1.4 `sandbox/policy.rs` `CONFINED_MEMORY`/`CONFINED_INSTRUCTIONS`) — `crates/buildl-lua/src/engine.rs:37`. **Fixed and verified.** Added `the_supplied_instruction_ceiling_bounds_evaluation`. A 100 000-iteration loop fails with `airsl::Error::InstructionLimit` under a 1 000-instruction ceiling, and returns `"5000050000"` under the default limits. With `.with_limits(...)` removed, `cargo test -p buildl-lua --lib engine::` gave `the_supplied_instruction_ceiling_bounds_evaluation ... FAILED` (`called Result::unwrap_err() on an Ok value: "5000050000"`; 3 passed, 1 failed). After the line was restored, `cargo test -p buildl-lua --lib` gave `test result: ok. 81 passed; 0 failed`.
- doc-comment-discipline (🔵) — the module doc said, in the present tense, that the composition root fills the limits from the manifest's `[declaration]` table, but no such code exists (spec D10) — `limits.rs:3`. **Fixed**: it now says the composition root "is meant to fill" them.
- unit-test-mandate (🔵) — no test covered a non-NotFound io error mapping to `Runtime` — `source.rs:93`. **Fixed**: added `a_build_file_that_is_a_directory_is_a_runtime_failure`.
- unit-test-mandate (🔵) — the test `a_recorded_refusal_is_reported_over_the_eval_result` only exercised a refusal caught by `pcall` — `source.rs:193`. **Fixed**: renamed it to `a_refusal_caught_by_pcall_is_still_reported`, and added `an_uncaught_refusal_is_reported_as_the_recorded_failure`, where an uncaught `buildl.target('app', 1)` gives `WrongFieldType { field: options }`.
- unit-test-mandate (🔵) — nothing guarded the `Send + Sync` property that spec §3 requires — `source.rs:58`. **Fixed**: `the_source_can_be_shared_across_threads` asserts it at compile time.
- spec drift (🔵) — the spec §9 layout row gives `classify.rs` the shape `airsl::Error → (EvaluationFailure, Diagnostic)`. The code returns only `EvaluationFailure`, and `source.rs` builds the diagnostic from `error.to_string()`. The behaviour matches §8, but the spec row was never amended — spec §9

Blocking set: none. The reviewer re-ran `cargo make dod` and it exited 0. The reviewer also confirmed that `curated`/`classify`/`limits`/`engine`/`source.rs` and `lib.rs` match the plan's blocks word for word, and that `grep -rn "dead_code\|cfg_attr" crates/buildl-lua/src` prints nothing.

## Probe results

- Task 1's claim that `airsstack.path` is pure path arithmetic except for `absolute` was checked against the airsl 0.1.4 source. `grep -nE 'create_function|"[a-z_]+"|current_dir|std::env|std::fs' ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/airsl-0.1.4/src/modules/path.rs` lists the nine installed functions: `join` :79, `dirname` :84, `basename` :89, `stem` :94, `ext` :99, `normalize` :104, `relative_to` :110, `is_absolute` :126 and `absolute` :133. The grep matches no `std::env` or `std::fs`. `is_absolute` is `std::path::Path::new(..).is_absolute()` (:120-124), which is platform grammar and reads no process state. The claim holds.
- Every other asserted fact was structural or covered by the tasks' own red-green steps. Each task gave `error[E0432]` first, then curated `1 passed`, classify `6 passed`, limits `2 passed`, engine `3 passed` and source `6 passed`.
- Batch gate: `cargo make dod` gave buildl-lua lib `77 passed`, doctests `2 passed`, and `Build Done in 3.80 seconds`. `grep -rn dead_code crates/buildl-lua/src` printed nothing, and the coder's `cargo deny check` reported `advisories ok, bans ok, licenses ok, sources ok`. After the fix round, the coder's `cargo make dod` gave buildl-lua lib `81 passed` and `Build Done`.

## Deviations

- 2026-10-08 — The lib totals are higher than the plan states: 77 instead of 75 before the fix round, because of plan 05's two added module tests, and 81 after it, because of four added tests (engine 4, source 9). `engine.rs` gained an `engine_with(&DeclarationLimits)` test helper. One test in `source.rs` was renamed. The `limits.rs` module doc wording changed. All of these came from review findings.
- 2026-10-08 — One coder ran Tasks 1–5 in order, because every task modifies `lib.rs`. The plan's per-task commits were not made; commits are left to the user.
