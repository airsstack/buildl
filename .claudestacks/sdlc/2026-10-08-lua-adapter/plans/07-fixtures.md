---
status: approved
created: 2026-10-08
depends-on: [06]
---

# Fixtures Implementation Plan

**Goal:** Real build files, evaluated through airsl, prove the adapter from the outside, ending in `buildl_core::load` over a real workspace.

**Architecture:** One integration-test target, `crates/buildl-lua/tests/flows/`, built only from the public APIs of `buildl_lua` and `buildl_core`, as the composition root will use them. Multi-line and multi-file cases are checked-in workspaces under `tests/fixtures/<case>/`. One-line failure and surface cases write their `build.lua` to a temporary workspace. The library these tests exercise already exists, so each task's tests pass when first run. They pin behaviour rather than drive it, and each task's red step is the test target failing to compile until its module is declared.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, airsl 0.1.4 (with its `mlua` re-export), `buildl-core`, `cargo-make`, `cargo-deny`.

**Content authority:** spec §11 (fixtures, determinism, sources, end to end), §13 (DoD).

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

| Item | From |
|---|---|
| `LuaSource::new(root, limits)`, `DeclarationLimits::default()` | plan `06` |
| `buildl_core::{load, EntryName, Declaration, Declared, Target, Action, Command, Argument, Label, SourcePath, OutputName, Provenance, Directory, …}` | `buildl-core`'s public API |
| `load` returns declarations in its sorted merge order (directory, then name); the end-to-end test compares the exact `Vec`, root declarations first | `crates/buildl-core/src/load/traversal.rs` |

Fixture paths are canonicalised (`fs::canonicalize`), because `LuaSource` requires a canonical root.
Empty source files in fixtures (`*.c`, `*.h`) exist only to be matched by `buildl.sources`. Git keeps
empty files, so commit them as they are. The staging-limit case relies on
`size_of::<StagedDeclaration>()` being large enough that 200 000 aliases exceed 16 MiB. It was 200 000
× (record size + text) > 16 777 216 on the proving run, and the test asserts the exact refusal
message.

## File structure

```text
crates/buildl-lua/tests/flows/main.rs               — [create] the test target's root                  (Tasks 1–4)
crates/buildl-lua/tests/flows/common.rs             — [create] workspace helpers                       (Task 1)
crates/buildl-lua/tests/flows/declarations.rs       — [create] staged values, flattening, determinism  (Task 1)
crates/buildl-lua/tests/fixtures/every_field/build.lua      — [create]                                 (Task 1)
crates/buildl-lua/tests/fixtures/flattened/build.lua        — [create]                                 (Task 1)
crates/buildl-lua/tests/fixtures/flattened/src/{a.c,b.c,a.h} — [create] empty                          (Task 1)
crates/buildl-lua/tests/flows/failures.rs           — [create] every failure kind                      (Task 2)
crates/buildl-lua/tests/flows/sandbox.rs            — [create] the curated surface; symlink escape     (Task 3)
crates/buildl-lua/tests/flows/load.rs               — [create] load over a real workspace              (Task 4)
crates/buildl-lua/tests/fixtures/workspace/build.lua, lib/build.lua — [create]                         (Task 4)
crates/buildl-lua/tests/fixtures/workspace/main.c, lib/src/text.c   — [create] empty                   (Task 4)
```

### Task 1 — Staged values from real files

**Files:**
- Create `crates/buildl-lua/tests/flows/main.rs`, `common.rs`, `declarations.rs`
- Create `crates/buildl-lua/tests/fixtures/every_field/build.lua`
- Create `crates/buildl-lua/tests/fixtures/flattened/build.lua`, and empty `src/a.c`, `src/b.c`, `src/a.h`

**Steps:**

1. Create `tests/fixtures/every_field/build.lua`:

   ```lua
   local b = buildl

   b.subdir("lib")

   b.rule("cc", {
     run  = { "cc", "-c", "$in", "-o", "$out" },
     desc = "compile $in",
   })

   b.target("main.o", { rule = "cc", inputs = { "src/main.c" } })

   b.target("app", {
     deps    = { "main.o", "//lib:text" },
     run     = { "cc", "$deps", "-o", "$out" },
     outputs = { "app" },
     env     = { "HOME" },
     network = true,
     always  = true,
   })

   b.test("app_test", { deps = { "app" }, run = { "$out/app", "--self-test" } })

   b.alias("default", "app")

   b.option("mode", { default = "debug" })
   ```

2. Create `tests/fixtures/flattened/build.lua`, plus the empty files `src/a.c`, `src/b.c` and `src/a.h`
   beside it (`touch`):

   ```lua
   buildl.target("app", {
     run    = { "cc", "$in", "-o", "$out" },
     inputs = { buildl.sources("src/*.c"), "go.mod" },
   })
   ```

3. Create `tests/flows/main.rs` declaring only the first two modules:

   ```rust
   //! Real build files evaluated through airsl by `LuaSource`, from the outside.
   //!
   //! Everything here uses only the public APIs of `buildl_lua` and `buildl_core`, as the composition
   //! root will.

   #![expect(
       clippy::unwrap_used,
       clippy::panic,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   mod common;
   mod declarations;
   ```

4. Run the target and confirm it fails because the modules do not exist yet:

   ```bash
   cargo test -p buildl-lua --test flows
   ```

   ```text
   error[E0583]: file not found for module `common`
   error[E0583]: file not found for module `declarations`
   ```

5. Create `tests/flows/common.rs` with the three helpers this task's tests use. Task 2 adds the two
   that build a temporary workspace; adding them now would fail clippy on dead code:

   ```rust
   //! Workspaces to evaluate: checked-in fixtures, or a temporary one holding a single build file.

   use std::fs;
   use std::path::{Path, PathBuf};

   use buildl_core::{BuildFile, DeclarationSource, Directory, Evaluated, Provenance, Result};
   use buildl_lua::{DeclarationLimits, LuaSource};

   /// The canonical path of the checked-in fixture workspace `name`.
   pub(crate) fn fixture(name: &str) -> PathBuf {
       fs::canonicalize(
           Path::new(env!("CARGO_MANIFEST_DIR"))
               .join("tests/fixtures")
               .join(name),
       )
       .unwrap()
   }

   /// The workspace root's build file.
   pub(crate) fn root_file() -> BuildFile {
       BuildFile::new(Provenance::new(
           PathBuf::from("build.lua"),
           Directory::root(),
       ))
   }

   /// Evaluates the root build file of the workspace at `root` under the default limits.
   pub(crate) fn evaluate(root: &Path) -> Result<Evaluated> {
       LuaSource::new(root, DeclarationLimits::default()).evaluate(&root_file())
   }
   ```

6. Create `tests/flows/declarations.rs`:

   ```rust
   //! A build file's calls come back as exactly the staged values they wrote.

   use buildl_core::{
       DeclarationOrder, Evaluated, Freshness, NetworkAccess, StagedAlias, StagedDeclaration,
       StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole,
       Written,
   };

   use crate::common::{evaluate, fixture};

   fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
       texts.iter().map(|text| Written::new(*text)).collect()
   }

   const fn declared(order: u32, item: StagedItem) -> StagedDeclaration {
       StagedDeclaration {
           order: DeclarationOrder::new(order),
           item,
       }
   }

   #[test]
   fn every_primitive_and_field_is_staged_as_written() {
       let expected = StagedFile {
           declarations: vec![
               declared(
                   1,
                   StagedItem::Rule(StagedRule {
                       name: Written::new("cc"),
                       run: strings(&["cc", "-c", "$in", "-o", "$out"]),
                       description: Some(Written::new("compile $in")),
                   }),
               ),
               declared(
                   2,
                   StagedItem::Target(StagedTarget {
                       name: Written::new("main.o"),
                       role: TargetRole::Build,
                       rule: Some(Written::new("cc")),
                       run: None,
                       inputs: strings(&["src/main.c"]),
                       deps: Vec::new(),
                       outputs: None,
                       env: Vec::new(),
                       network: NetworkAccess::Sealed,
                       freshness: Freshness::Cached,
                   }),
               ),
               declared(
                   3,
                   StagedItem::Target(StagedTarget {
                       name: Written::new("app"),
                       role: TargetRole::Build,
                       rule: None,
                       run: Some(strings(&["cc", "$deps", "-o", "$out"])),
                       inputs: Vec::new(),
                       deps: strings(&["main.o", "//lib:text"]),
                       outputs: Some(strings(&["app"])),
                       env: strings(&["HOME"]),
                       network: NetworkAccess::Declared,
                       freshness: Freshness::Always,
                   }),
               ),
               declared(
                   4,
                   StagedItem::Target(StagedTarget {
                       name: Written::new("app_test"),
                       role: TargetRole::Test,
                       rule: None,
                       run: Some(strings(&["$out/app", "--self-test"])),
                       inputs: Vec::new(),
                       deps: strings(&["app"]),
                       outputs: None,
                       env: Vec::new(),
                       network: NetworkAccess::Sealed,
                       freshness: Freshness::Cached,
                   }),
               ),
               declared(
                   5,
                   StagedItem::Alias(StagedAlias {
                       name: Written::new("default"),
                       target: Written::new("app"),
                   }),
               ),
               declared(
                   6,
                   StagedItem::Setting(StagedSetting {
                       name: Written::new("mode"),
                       default: Written::new("debug"),
                   }),
               ),
           ],
           subdirs: vec![StagedSubdir {
               path: Written::new("lib"),
               order: DeclarationOrder::new(0),
           }],
       };
       assert_eq!(
           evaluate(&fixture("every_field")).unwrap(),
           Evaluated::Staged(expected)
       );
   }

   #[test]
   fn a_nested_sources_list_is_spliced_into_inputs() {
       let Evaluated::Staged(staged) = evaluate(&fixture("flattened")).unwrap() else {
           panic!("the fixture has a build file");
       };
       let StagedItem::Target(target) = &staged.declarations[0].item else {
           panic!("the fixture declares a target");
       };
       assert_eq!(target.inputs, strings(&["src/a.c", "src/b.c", "go.mod"]));
   }

   #[test]
   fn evaluating_a_file_twice_stages_the_same_values() {
       let root = fixture("every_field");
       assert_eq!(evaluate(&root).unwrap(), evaluate(&root).unwrap());
   }

   #[test]
   fn a_missing_build_file_is_absent() {
       let dir = tempfile::tempdir().unwrap();
       assert_eq!(evaluate(dir.path()).unwrap(), Evaluated::Absent);
   }
   ```

7. Run the target:

   ```bash
   cargo test -p buildl-lua --test flows
   ```

   ```text
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; …
   ```

8. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
9. Commit: `test(buildl-lua): pin staged values from real build files`.

### Task 2 — Every failure kind

**Files:**
- Create `crates/buildl-lua/tests/flows/failures.rs`
- Modify `crates/buildl-lua/tests/flows/main.rs`
- Modify `crates/buildl-lua/tests/flows/common.rs`

**Steps:**

1. Add `mod failures;` to `tests/flows/main.rs` after `mod declarations;`, then run
   `cargo test -p buildl-lua --test flows` and confirm
   `error[E0583]: file not found for module `failures``.
2. Replace the whole of `tests/flows/common.rs` with this version, which adds `workspace` and
   `failure_of` and their imports:

   ```rust
   //! Workspaces to evaluate: checked-in fixtures, or a temporary one holding a single build file.

   use std::fs;
   use std::path::{Path, PathBuf};

   use buildl_core::{
       BuildFile, DeclarationSource, Diagnostic, Directory, Error, Evaluated, EvaluationFailure,
       Provenance, Result,
   };
   use buildl_lua::{DeclarationLimits, LuaSource};

   /// The canonical path of the checked-in fixture workspace `name`.
   pub(crate) fn fixture(name: &str) -> PathBuf {
       fs::canonicalize(
           Path::new(env!("CARGO_MANIFEST_DIR"))
               .join("tests/fixtures")
               .join(name),
       )
       .unwrap()
   }

   /// A temporary workspace whose root build file holds `source`.
   pub(crate) fn workspace(source: &str) -> (tempfile::TempDir, PathBuf) {
       let dir = tempfile::tempdir().unwrap();
       let root = fs::canonicalize(dir.path()).unwrap();
       fs::write(root.join("build.lua"), source).unwrap();
       (dir, root)
   }

   /// The workspace root's build file.
   pub(crate) fn root_file() -> BuildFile {
       BuildFile::new(Provenance::new(
           PathBuf::from("build.lua"),
           Directory::root(),
       ))
   }

   /// Evaluates the root build file of the workspace at `root` under the default limits.
   pub(crate) fn evaluate(root: &Path) -> Result<Evaluated> {
       LuaSource::new(root, DeclarationLimits::default()).evaluate(&root_file())
   }

   /// The failure and diagnostic of evaluating a root build file holding `source`.
   pub(crate) fn failure_of(source: &str) -> (EvaluationFailure, Diagnostic) {
       let (_dir, root) = workspace(source);
       match evaluate(&root) {
           Err(Error::Evaluation {
               failure,
               diagnostic,
               ..
           }) => (failure, diagnostic),
           other => panic!("expected an evaluation failure, got {other:?}"),
       }
   }
   ```

3. Create `tests/flows/failures.rs`:

   ```rust
   //! Every way a build file can fail maps onto one `EvaluationFailure`, with a usable diagnostic.

   use buildl_core::{EvaluationFailure, EvaluationLimit, Written};

   use crate::common::failure_of;

   #[test]
   fn a_file_that_does_not_parse_is_a_syntax_failure() {
       let (failure, diagnostic) = failure_of("buildl.target(");
       assert_eq!(failure, EvaluationFailure::Syntax);
       assert!(diagnostic.as_str().contains("build.lua:1:"), "{diagnostic}");
   }

   #[test]
   fn a_raised_error_is_a_runtime_failure() {
       let (failure, diagnostic) = failure_of("\nerror('boom')");
       assert_eq!(failure, EvaluationFailure::Runtime);
       assert!(
           diagnostic.as_str().contains("build.lua:2: boom"),
           "{diagnostic}"
       );
   }

   #[test]
   fn reading_through_airsl_s_glob_is_refused() {
       let (failure, diagnostic) = failure_of("airsstack.glob.walk('/', '*')");
       assert_eq!(failure, EvaluationFailure::Refused);
       assert!(
           diagnostic.as_str().contains("glob.walk denied"),
           "{diagnostic}"
       );
   }

   #[test]
   fn an_endless_loop_reaches_the_instruction_ceiling() {
       let (failure, _) = failure_of("while true do end");
       assert_eq!(
           failure,
           EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Instructions,
           }
       );
   }

   #[test]
   fn a_huge_allocation_reaches_the_memory_ceiling() {
       let (failure, _) = failure_of("local s = string.rep('x', 64 * 1024 * 1024)");
       assert_eq!(
           failure,
           EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Memory,
           }
       );
   }

   #[test]
   fn staging_past_the_memory_ceiling_reaches_the_staging_limit() {
       let (failure, diagnostic) = failure_of("for i = 1, 200000 do buildl.alias('a' .. i, 'b') end");
       assert_eq!(
           failure,
           EvaluationFailure::LimitReached {
               limit: EvaluationLimit::Staging,
           }
       );
       assert!(
           diagnostic
               .as_str()
               .ends_with("buildl.alias: staging budget of 16777216 bytes exceeded"),
           "{diagnostic}"
       );
   }

   #[test]
   fn an_unknown_option_field_is_named() {
       let (failure, diagnostic) = failure_of("buildl.target('app', { run = { 'cc' }, srcs = {} })");
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
   fn a_wrongly_typed_field_is_named() {
       let (failure, diagnostic) = failure_of("\n\nbuildl.rule('cc', { run = 'cc -c' })");
       assert_eq!(
           failure,
           EvaluationFailure::WrongFieldType {
               field: Written::new("run"),
           }
       );
       assert_eq!(
           diagnostic.as_str(),
           "line 3: buildl.rule 'cc': field 'run' must be a list of strings, found string"
       );
   }

   #[test]
   fn a_refusal_caught_with_pcall_still_fails_the_file() {
       let (failure, diagnostic) = failure_of(
           "local ok = pcall(buildl.option, 'mode', { default = 1 })\n\
            buildl.alias('default', 'app')",
       );
       assert_eq!(
           failure,
           EvaluationFailure::WrongFieldType {
               field: Written::new("default"),
           }
       );
       assert_eq!(
           diagnostic.as_str(),
           "line 1: buildl.option 'mode': field 'default' must be a string, found integer"
       );
   }
   ```

4. Run the target:

   ```bash
   cargo test -p buildl-lua --test flows
   ```

   ```text
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; …
   ```

5. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
6. Commit: `test(buildl-lua): pin how each evaluation failure is reported`.

### Task 3 — The sandbox from inside a build file

**Files:**
- Create `crates/buildl-lua/tests/flows/sandbox.rs`
- Modify `crates/buildl-lua/tests/flows/main.rs`

**Steps:**

1. Add `mod sandbox;` as the last module of `tests/flows/main.rs`, then run
   `cargo test -p buildl-lua --test flows` and confirm
   `error[E0583]: file not found for module `sandbox``.
2. Create `tests/flows/sandbox.rs`:

   ```rust
   //! What a build file can reach: the curated surface, and nothing outside the workspace.

   use std::fs;
   use std::os::unix::fs::symlink;
   use std::path::PathBuf;

   use buildl_core::{
       BuildFile, DeclarationSource, Directory, Error, Evaluated, EvaluationFailure, Provenance,
   };
   use buildl_lua::{DeclarationLimits, LuaSource};

   use crate::common::{evaluate, workspace};

   /// Each `assert` names what leaked; the file stages one alias only if every check passed.
   const SURFACE: &str = r#"
   assert(os == nil and io == nil and coroutine == nil and debug == nil, "standard library leak")
   assert(require == nil and load == nil and dofile == nil and loadfile == nil, "loader leak")
   assert(print == nil, "stdout leak")
   assert(math.random == nil and math.randomseed == nil, "entropy leak")
   assert(type(math.floor) == "function" and type(string.format) == "function", "pure library missing")
   assert(airsstack.fs == nil and airsstack.proc == nil and airsstack.env == nil
          and airsstack.time == nil and airsstack.stdio == nil, "host module leak")
   assert(type(airsstack.json) == "table" and type(airsstack.path) == "table"
          and type(airsstack.regex) == "table" and type(airsstack.hash) == "table"
          and type(airsstack.glob) == "table", "curated module missing")
   assert(airsstack.path.absolute == nil, "ambient path leak")
   assert(buildl == airsstack.buildl and buildl.json == nil, "buildl binding")
   buildl.alias("ok", "ok")
   "#;

   #[test]
   fn a_build_file_sees_the_curated_surface_only() {
       let (_dir, root) = workspace(SURFACE);
       let Evaluated::Staged(staged) = evaluate(&root).unwrap() else {
           panic!("the build file exists");
       };
       assert_eq!(staged.declarations.len(), 1);
   }

   #[test]
   fn sources_refuses_a_declaring_directory_that_resolves_outside_the_workspace() {
       let (_dir, root) = workspace("buildl.subdir('lib')");
       let elsewhere = tempfile::tempdir().unwrap();
       fs::write(elsewhere.path().join("build.lua"), "buildl.sources('*.c')").unwrap();
       symlink(elsewhere.path(), root.join("lib")).unwrap();
       let lib = BuildFile::new(Provenance::new(
           PathBuf::from("lib/build.lua"),
           Directory::parse("lib").unwrap(),
       ));
       match LuaSource::new(&root, DeclarationLimits::default()).evaluate(&lib) {
           Err(Error::Evaluation { failure, .. }) => assert_eq!(failure, EvaluationFailure::Refused),
           other => panic!("expected a refusal, got {other:?}"),
       }
   }
   ```

3. Run the target:

   ```bash
   cargo test -p buildl-lua --test flows
   ```

   ```text
   test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; …
   ```

4. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
5. Commit: `test(buildl-lua): pin the curated surface a build file sees`.

### Task 4 — `load` over a real workspace

**Files:**
- Create `crates/buildl-lua/tests/flows/load.rs`
- Create `crates/buildl-lua/tests/fixtures/workspace/build.lua`, `lib/build.lua`, and empty `main.c`, `lib/src/text.c`
- Modify `crates/buildl-lua/tests/flows/main.rs`

**Steps:**

1. Create `tests/fixtures/workspace/build.lua`:

   ```lua
   buildl.subdir("lib")

   buildl.target("app", {
     deps   = { "//lib:text" },
     run    = { "cc", "$in", "$deps", "-o", "$out" },
     inputs = buildl.sources("*.c"),
   })
   ```

2. Create `tests/fixtures/workspace/lib/build.lua`, plus the empty files `main.c` (beside the root
   `build.lua`) and `lib/src/text.c`:

   ```lua
   buildl.target("text", {
     run    = { "cc", "-c", "$in", "-o", "$out" },
     inputs = buildl.sources("src/*.c"),
   })
   ```

3. Add `mod load;` to `tests/flows/main.rs` between `mod failures;` and `mod sandbox;`, then run
   `cargo test -p buildl-lua --test flows` and confirm
   `error[E0583]: file not found for module `load``. `main.rs` now reads:

   ```rust
   //! Real build files evaluated through airsl by `LuaSource`, from the outside.
   //!
   //! Everything here uses only the public APIs of `buildl_lua` and `buildl_core`, as the composition
   //! root will.

   #![expect(
       clippy::unwrap_used,
       clippy::panic,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   mod common;
   mod declarations;
   mod failures;
   mod load;
   mod sandbox;
   ```

4. Create `tests/flows/load.rs`:

   ```rust
   //! `buildl_core::load` over a real workspace, through this adapter.

   use std::path::PathBuf;

   use buildl_core::{
       Action, Argument, Command, Declaration, Declared, Directory, EntryName, Freshness, Label,
       NetworkAccess, OutputName, Provenance, SourcePath, Target, TargetRole, load,
   };
   use buildl_lua::{DeclarationLimits, LuaSource};

   use crate::common::fixture;

   fn command(arguments: &[&str]) -> Command {
       Command::new(
           arguments
               .iter()
               .map(|argument| Argument::parse(*argument).unwrap())
               .collect(),
       )
       .unwrap()
   }

   fn target(
       file: &str,
       directory: &str,
       label: &str,
       run: &[&str],
       inputs: &[&str],
       deps: &[&str],
       output: &str,
   ) -> Declaration {
       let directory = if directory.is_empty() {
           Directory::root()
       } else {
           Directory::parse(directory).unwrap()
       };
       Declaration::new(
           Provenance::new(PathBuf::from(file), directory),
           Declared::Target(Target {
               label: Label::parse(label).unwrap(),
               role: TargetRole::Build,
               action: Action::Run(command(run)),
               inputs: inputs
                   .iter()
                   .map(|input| SourcePath::parse(*input).unwrap())
                   .collect(),
               deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
               outputs: vec![OutputName::parse(output).unwrap()],
               env: Vec::new(),
               network: NetworkAccess::Sealed,
               freshness: Freshness::Cached,
           }),
       )
   }

   #[test]
   fn load_turns_a_real_workspace_into_declarations() {
       let source = LuaSource::new(fixture("workspace"), DeclarationLimits::default());
       let declarations = load(&source, &EntryName::parse("build.lua").unwrap()).unwrap();
       let expected = vec![
           target(
               "build.lua",
               "",
               "//:app",
               &["cc", "$in", "$deps", "-o", "$out"],
               &["main.c"],
               &["//lib:text"],
               "app",
           ),
           target(
               "lib/build.lua",
               "lib",
               "//lib:text",
               &["cc", "-c", "$in", "-o", "$out"],
               &["lib/src/text.c"],
               &[],
               "text",
           ),
       ];
       assert_eq!(declarations, expected);
   }
   ```

5. Run the target:

   ```bash
   cargo test -p buildl-lua --test flows
   ```

   ```text
   test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; …
   ```

6. Run the whole gate:

   ```bash
   cargo make dod
   ```

   It exits `0`. Every step runs warnings-as-errors: fmt, clippy (with `guard-core-purity` and `guard-crate-edges`), rustdoc, tests and doctests.
7. Commit: `test(buildl-lua): load a real workspace through LuaSource`.

---

## Verification summary (plan-level)

- `cargo test -p buildl-lua --test flows` reports 16 passed.
- `cargo make dod` exits `0`; `git diff main -- crates/buildl-core` is empty.
