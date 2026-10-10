---
status: done
created: 2026-10-10
depends-on: [06]
---

# Pipeline Graph Implementation Plan

**Goal:** `Pipeline::graph` runs the pipeline up to the `graph` handoff.

**Architecture:** `Pipeline` gains its second command: `graph` calls `load` once and hands the result to `resolve`. It does not repeat `check`'s double load. `pipeline/` stays the only module that names two phases. The flows live in the one integration-test binary, `tests/flows/`, driven through `FakeSource` with fakes built from the public API alone; this plan adds the builders the new flows need. The serialized graph of one fixture workspace is pinned in a golden file, so a change to the graph's JSON shape shows up as a diff of that file.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §6 (the pipeline), §9 (the flows table); decisions D7, D8.

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What plan `06` leaves that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `resolve(declarations: Vec<Declaration>) -> Result<TargetGraph>`, re-exported from the crate root | `crates/buildl-core/src/resolve/assembly.rs` | the phase `graph` runs after Load |
| `TargetGraph`, serializing keyed by label | `crates/buildl-core/src/types/target_graph.rs` | `graph`'s result, and the golden file's content |
| `Pipeline<P: Ports>` with `new` and `check`; `load` | `crates/buildl-core/src/pipeline/driver.rs` | the type that gains `graph` |
| `FakeSource`, `FakePorts`, `FakeFile`, `file`, `target`, `setting`, `workspace` | `crates/buildl-core/tests/flows/common.rs` | the fakes every flow uses |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- Each builder in `tests/flows/common.rs` arrives with the flow that first calls it: `target_with`
  and `rule` in task 1, `alias` in task 2.
- `sites.iter().map(Provenance::file)` rather than a closure: clippy's `redundant_closure`.
- The golden file is read with `include_str!` and compared after `trim_end()`, so the file may
  end with a newline. It is one line.
- Through Load, a label can be declared twice only inside one file, because a label always sits
  in its declaring directory and a directory has one build file. The duplicate that spans two
  files is a setting.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/pipeline/driver.rs         — [modify] Pipeline::graph (task 1)
crates/buildl-core/tests/flows/common.rs         — [modify] target_with, rule (task 1); alias (task 2)
crates/buildl-core/tests/flows/main.rs           — [modify] mod graph; (task 1)
crates/buildl-core/tests/flows/graph.rs          — [create] the six failure flows (task 1); the fixture and two golden flows (task 2)
crates/buildl-core/tests/flows/golden/graph.json  — [create] the pinned serialized graph (task 2)
```

### Task 1 — Run the pipeline up to the graph handoff

**Files:**
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/tests/flows/main.rs`
- Create `crates/buildl-core/tests/flows/graph.rs`

**Steps:**

1. Add the builders the flows need. In `crates/buildl-core/tests/flows/common.rs`, widen the import. Replace:

   ```rust
   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedDeclaration, StagedFile,
       StagedItem, StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
   };
   ```

   with:

   ```rust
   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedDeclaration, StagedFile,
       StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
   };
   ```

2. In the same file, rebuild `target` on a general `target_with` and add `rule`. Replace:

   ```rust
   /// A `b.target(name, { run = { "cc" }, deps = deps })` call.
   #[must_use]
   pub fn target(name: &str, deps: &[&str]) -> StagedItem {
       StagedItem::Target(StagedTarget {
           name: Written::new(name),
           role: TargetRole::Build,
           rule: None,
           run: Some(vec![Written::new("cc")]),
           inputs: Vec::new(),
           deps: deps.iter().map(|dep| Written::new(*dep)).collect(),
           outputs: None,
           env: Vec::new(),
           network: NetworkAccess::Sealed,
           freshness: Freshness::Cached,
       })
   }
   ```

   with:

   ```rust
   /// A `b.target(name, { run = { "cc" }, deps = deps })` call.
   #[must_use]
   pub fn target(name: &str, deps: &[&str]) -> StagedItem {
       target_with(name, |target| {
           target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
       })
   }

   /// A `b.target(name, { run = { "cc" } })` call, after `change` is applied to it.
   #[must_use]
   pub fn target_with(name: &str, change: impl FnOnce(&mut StagedTarget)) -> StagedItem {
       let mut target = StagedTarget {
           name: Written::new(name),
           role: TargetRole::Build,
           rule: None,
           run: Some(vec![Written::new("cc")]),
           inputs: Vec::new(),
           deps: Vec::new(),
           outputs: None,
           env: Vec::new(),
           network: NetworkAccess::Sealed,
           freshness: Freshness::Cached,
       };
       change(&mut target);
       StagedItem::Target(target)
   }

   /// A `b.rule(name, { run = run, desc = description })` call.
   #[must_use]
   pub fn rule(name: &str, run: &[&str], description: Option<&str>) -> StagedItem {
       StagedItem::Rule(StagedRule {
           name: Written::new(name),
           run: run.iter().map(|argument| Written::new(*argument)).collect(),
           description: description.map(Written::new),
       })
   }
   ```

3. Declare the new flow module in `crates/buildl-core/tests/flows/main.rs`. Replace:

   ```rust
   mod check;
   mod load;
   ```

   with:

   ```rust
   mod check;
   mod graph;
   mod load;
   ```

4. Write the failing flows. Create `crates/buildl-core/tests/flows/graph.rs`:

   ```rust
   //! `Pipeline::graph`, driven through the build-file port by [`FakeSource`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::path::Path;

   use buildl_core::{
       DeclarationField, DeclaredKind, EntryName, Error, EvaluationFailure, Pipeline, Provenance,
       TargetGraph,
   };

   use crate::common::{FakeFile, FakePorts, FakeSource, file, rule, setting, target, workspace};

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   fn graph(source: FakeSource) -> buildl_core::Result<TargetGraph> {
       Pipeline::<FakePorts>::new(source).graph(&entry())
   }

   #[test]
   fn a_label_declared_twice_in_one_file_names_that_file_twice() {
       let source = FakeSource::new(workspace(vec![(
           "",
           file(vec![target("a", &[]), target("a", &[])], &[]),
       )]));
       match graph(source).unwrap_err() {
           Error::DuplicateLabel { label, sites } => {
               assert_eq!(label.to_string(), "//:a");
               let files: Vec<&Path> = sites.iter().map(Provenance::file).collect();
               assert_eq!(files, [Path::new("build.lua"), Path::new("build.lua")]);
           }
           other => unreachable!("expected DuplicateLabel, got {other:?}"),
       }
   }

   #[test]
   fn a_setting_declared_in_two_files_names_both() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![setting("mode")], &["lib"])),
           ("lib", file(vec![setting("mode")], &[])),
       ]));
       match graph(source).unwrap_err() {
           Error::DuplicateSetting { name, sites } => {
               assert_eq!(name.as_str(), "mode");
               let files: Vec<&Path> = sites.iter().map(Provenance::file).collect();
               assert_eq!(files, [Path::new("build.lua"), Path::new("lib/build.lua")]);
           }
           other => unreachable!("expected DuplicateSetting, got {other:?}"),
       }
   }

   #[test]
   fn a_misspelled_dependency_names_its_file_and_the_nearest_label() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("main.o", &[])], &["app"])),
           ("app", file(vec![target("bin", &["//:mian.o"])], &[])),
       ]));
       let error = graph(source).unwrap_err();
       assert_eq!(
           error.to_string(),
           "app/build.lua: //app:bin: dep //:mian.o is not declared (did you mean //:main.o?)"
       );
       match error {
           Error::UnknownReference {
               site,
               field,
               reference,
               suggestion,
           } => {
               assert_eq!(site.provenance.file(), Path::new("app/build.lua"));
               assert_eq!(site.label.to_string(), "//app:bin");
               assert_eq!(field, DeclarationField::Dep);
               assert_eq!(reference.to_string(), "//:mian.o");
               assert_eq!(suggestion.unwrap().to_string(), "//:main.o");
           }
           other => unreachable!("expected UnknownReference, got {other:?}"),
       }
   }

   #[test]
   fn a_dependency_naming_a_rule_names_both_files() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("app", &["//tools:cc"])], &["tools"])),
           ("tools", file(vec![rule("cc", &["cc"], None)], &[])),
       ]));
       let error = graph(source).unwrap_err();
       assert_eq!(
           error.to_string(),
           "build.lua: //:app: dep //tools:cc names a rule, declared in tools/build.lua"
       );
       match error {
           Error::WrongReferenceKind {
               site,
               found,
               declared_in,
               ..
           } => {
               assert_eq!(site.provenance.file(), Path::new("build.lua"));
               assert_eq!(found, DeclaredKind::Rule);
               assert_eq!(declared_in.file(), Path::new("tools/build.lua"));
           }
           other => unreachable!("expected WrongReferenceKind, got {other:?}"),
       }
   }

   #[test]
   fn a_cycle_across_two_directories_names_every_target_and_its_file() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("b", &["//lib:c"])], &["lib"])),
           (
               "lib",
               file(vec![target("c", &["a"]), target("a", &["//:b"])], &[]),
           ),
       ]));
       let error = graph(source).unwrap_err();
       assert_eq!(
           error.to_string(),
           "dependency cycle: //:b (build.lua) -> //lib:c (lib/build.lua) -> \
            //lib:a (lib/build.lua) -> //:b"
       );
       match error {
           Error::DependencyCycle { path } => {
               let steps: Vec<(String, &Path)> = path
                   .iter()
                   .map(|site| (site.label.to_string(), site.provenance.file()))
                   .collect();
               assert_eq!(
                   steps,
                   [
                       ("//:b".to_owned(), Path::new("build.lua")),
                       ("//lib:c".to_owned(), Path::new("lib/build.lua")),
                       ("//lib:a".to_owned(), Path::new("lib/build.lua")),
                   ]
               );
           }
           other => unreachable!("expected DependencyCycle, got {other:?}"),
       }
   }

   #[test]
   fn a_build_file_that_fails_to_evaluate_is_the_load_error_unchanged() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("app", &[])], &["lib"])),
           ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
       ]));
       match graph(source).unwrap_err() {
           Error::Evaluation {
               provenance,
               failure,
               ..
           } => {
               assert_eq!(provenance.file(), Path::new("lib/build.lua"));
               assert_eq!(failure, EvaluationFailure::Syntax);
           }
           other => unreachable!("expected Evaluation, got {other:?}"),
       }
   }
   ```

5. Run them and confirm they fail to build, because `Pipeline` has no `graph`:

   ```
   $ cargo test -p buildl-core --test flows graph
   error[E0599]: no method named `graph` found for struct `Pipeline<P>` in the current scope
   ```

6. Implement the command. In `crates/buildl-core/src/pipeline/driver.rs`, update the module doc. Replace:

   ```rust
   //! Its own file because the driver owns the sequence of phases, and the sequence grows by one
   //! method per command as later phases arrive. Today it runs Load.
   //!
   //! Responsibilities: [`Pipeline`] and [`Pipeline::check`].
   ```

   with:

   ```rust
   //! Its own file because the driver owns the sequence of phases, and the sequence grows by one
   //! method per command as later phases arrive. Today it runs Load and Resolve.
   //!
   //! Responsibilities: [`Pipeline`], [`Pipeline::check`] and [`Pipeline::graph`].
   ```

7. Widen the imports. Replace:

   ```rust
   use crate::ports::Ports;
   use crate::types::{Declaration, EntryName};
   ```

   with:

   ```rust
   use crate::ports::Ports;
   use crate::resolve::resolve;
   use crate::types::{Declaration, EntryName, TargetGraph};
   ```

8. Add the method to `impl<P: Ports> Pipeline<P>`, after `check`. Insert:

   ```rust

       /// Loads the workspace once and resolves its declarations into the target graph.
       ///
       /// The double evaluation that catches a nondeterministic build file is [`Pipeline::check`]'s;
       /// this command loads once.
       ///
       /// # Errors
       ///
       /// Returns whatever the load returns, and otherwise whatever [`resolve`] returns: a name
       /// declared more than once, a reference that names nothing or the wrong kind of thing, or a
       /// dependency cycle.
       pub fn graph(&self, entry: &EntryName) -> Result<TargetGraph> {
           resolve(load(&self.source, entry)?)
       }
   ```

   immediately after:

   ```rust
           first_difference(&first, &second).map_or(Ok(first), Err)
       }
   ```

9. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows graph
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out
   ```

10. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

11. Commit:

   ```
   $ git add crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/common.rs crates/buildl-core/tests/flows/main.rs crates/buildl-core/tests/flows/graph.rs
   $ git commit -m "feat(buildl-core): run the pipeline up to the graph handoff"
   ```

### Task 2 — Pin the serialized graph of a fixture workspace

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/tests/flows/graph.rs`
- Create `crates/buildl-core/tests/flows/golden/graph.json`

**Steps:**

1. In `crates/buildl-core/tests/flows/common.rs`, widen the import. Replace:

   ```rust
   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedDeclaration, StagedFile,
       StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
   };
   ```

   with:

   ```rust
   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedAlias, StagedDeclaration,
       StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole,
       Written,
   };
   ```

2. Add the `alias` builder. Insert:

   ```rust
   /// A `b.alias(name, target)` call.
   #[must_use]
   pub fn alias(name: &str, target: &str) -> StagedItem {
       StagedItem::Alias(StagedAlias {
           name: Written::new(name),
           target: Written::new(target),
       })
   }
   ```

   immediately before:

   ```rust
   /// A `b.option(name, { default = "" })` call.
   ```

3. Write the failing flows. In `crates/buildl-core/tests/flows/graph.rs`, widen the first import. Replace:

   ```rust
   use buildl_core::{
       DeclarationField, DeclaredKind, EntryName, Error, EvaluationFailure, Pipeline, Provenance,
       TargetGraph,
   };
   ```

   with:

   ```rust
   use buildl_core::{
       DeclarationField, DeclaredKind, EntryName, Error, EvaluationFailure, Pipeline, Provenance,
       StagedItem, TargetGraph, TargetRole, Written, json,
   };
   ```

4. Widen the second import. Replace:

   ```rust
   use crate::common::{FakeFile, FakePorts, FakeSource, file, rule, setting, target, workspace};
   ```

   with:

   ```rust
   use crate::common::{
       FakeFile, FakePorts, FakeSource, alias, file, rule, setting, target, target_with, workspace,
   };
   ```

5. Add the golden constant. Insert:

   ```rust
   /// The serialized graph of [`fixture`], pinned.
   const GOLDEN: &str = include_str!("golden/graph.json");
   ```

   immediately before:

   ```rust
   fn entry() -> EntryName {
   ```

6. Add the fixture workspace and the two flows that compare it with the golden file. Insert:

   ```rust
   /// The root file's declarations: a rule, a target using it, a target with dependencies in two
   /// directories, an alias, a test target naming a setting, and the setting.
   fn root_items() -> Vec<StagedItem> {
       vec![
           rule(
               "cc",
               &["cc", "-c", "$in", "-o", "$out"],
               Some("compile $in"),
           ),
           target_with("main.o", |target| {
               target.run = None;
               target.rule = Some(Written::new("cc"));
               target.inputs = vec![Written::new("main.c")];
           }),
           target_with("app", |target| {
               target.run = Some(["cc", "$deps", "-o", "$out"].map(Written::new).to_vec());
               target.deps = vec![Written::new("main.o"), Written::new("//lib:text")];
           }),
           alias("default", "app"),
           target_with("app_test", |target| {
               target.role = TargetRole::Test;
               target.run = Some(
                   ["$out/app", "--filter=$opt:test_filter"]
                       .map(Written::new)
                       .to_vec(),
               );
               target.deps = vec![Written::new("default")];
           }),
           setting("test_filter"),
       ]
   }

   fn lib_items() -> Vec<StagedItem> {
       vec![target_with("text", |target| {
           target.inputs = vec![Written::new("text.c")];
       })]
   }

   fn fixture() -> FakeSource {
       FakeSource::new(workspace(vec![
           ("", file(root_items(), &["lib"])),
           ("lib", file(lib_items(), &[])),
       ]))
   }

   #[test]
   fn a_workspace_resolves_to_the_pinned_graph() {
       let graph = graph(fixture()).unwrap();
       assert_eq!(graph.len(), 4);
       let json = json::canonical::to_string(&graph).unwrap();
       assert_eq!(json, GOLDEN.trim_end());
   }

   #[test]
   fn declaring_in_another_order_yields_the_same_bytes() {
       let mut reversed = root_items();
       reversed.reverse();
       let source = FakeSource::new(workspace(vec![
           ("", file(reversed, &["lib"])),
           ("lib", file(lib_items(), &[])),
       ]));
       let json = json::canonical::to_string(&graph(source).unwrap()).unwrap();
       assert_eq!(json, GOLDEN.trim_end());
   }
   ```

   immediately before:

   ```rust
   #[test]
   fn a_label_declared_twice_in_one_file_names_that_file_twice() {
   ```

7. Create `crates/buildl-core/tests/flows/golden/graph.json` holding an empty object, so the flows build and fail:

   ```json
   {}
   ```

8. Run the flows and confirm the two new ones fail:

   ```
   $ cargo test -p buildl-core --test flows graph
   test graph::a_workspace_resolves_to_the_pinned_graph ... FAILED
   test graph::declaring_in_another_order_yields_the_same_bytes ... FAILED
   test result: FAILED. 6 passed; 2 failed; 0 ignored; 0 measured; 13 filtered out
   ```

9. Replace the content of `crates/buildl-core/tests/flows/golden/graph.json` with the serialized graph, on one line:

   ```json
   {"aliases":{"//:default":"//:app"},"settings":{"test_filter":""},"targets":{"//:app":{"deps":["//:main.o","//lib:text"],"description":null,"env":[],"freshness":"Cached","inputs":[],"network":"Sealed","outputs":["app"],"provenance":{"directory":"","file":"build.lua"},"role":"Build","run":["cc","$deps","-o","$out"]},"//:app_test":{"deps":["//:app"],"description":null,"env":[],"freshness":"Cached","inputs":[],"network":"Sealed","outputs":["app_test"],"provenance":{"directory":"","file":"build.lua"},"role":"Test","run":["$out/app","--filter=$opt:test_filter"]},"//:main.o":{"deps":[],"description":"compile $in","env":[],"freshness":"Cached","inputs":["main.c"],"network":"Sealed","outputs":["main.o"],"provenance":{"directory":"","file":"build.lua"},"role":"Build","run":["cc","-c","$in","-o","$out"]},"//lib:text":{"deps":[],"description":null,"env":[],"freshness":"Cached","inputs":["lib/text.c"],"network":"Sealed","outputs":["text"],"provenance":{"directory":"lib","file":"lib/build.lua"},"role":"Build","run":["cc"]}}}
   ```

10. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows graph
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out
   ```

11. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

12. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs crates/buildl-core/tests/flows/graph.rs crates/buildl-core/tests/flows/golden/graph.json
   $ git commit -m "test(buildl-core): pin the serialized graph of a fixture workspace"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- Every row of spec §9's flows table has a passing flow in `tests/flows/graph.rs`.
- The golden file holds no number: every reference in it is a label.
- `Pipeline::check` is unchanged, and its four flows still pass.

## Review findings

One reviewer pass over both tasks, 2026-10-10. Verdict: spec compliant, no drift, blocking set empty. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0 (`buildl-core` unit 215 passed, flows 21, doctests 4); `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). The golden file, the fixture and the six failure flows are `diff`-identical to this plan's blocks. No finding was fixed: each sits in text this plan prescribes.

- risk — the single load (spec D8, and the doc sentence "this command loads once") has no guard: `resolve(self.check(entry)?)` in place of the body passes all eight graph flows, because every one uses `FakeSource::new`. A flow over `FakeSource::with_second_run` asserting that `graph` returns `Ok` would catch it. Neither this plan nor the spec's §9 calls for that flow — `crates/buildl-core/src/pipeline/driver.rs:68`
- nit — the `# Errors` list reads as complete and leaves out `Error::TooManyTargets`, which `resolve`'s own doc lists — `crates/buildl-core/src/pipeline/driver.rs:65`
- nit — only the root file is permuted; `lib_items()` holds one declaration, so "each file's declarations in another order" is met trivially for `lib`. Load's sort absorbs the permutation before Resolve, so the flow pins order independence through Load; Resolve's own rests on the unit test in `resolve/assembly.rs` — `crates/buildl-core/tests/flows/graph.rs:84`
- nit — the test name says the Load error is "unchanged", but the match drops `diagnostic` with `..`; only `provenance` and `failure` are asserted — `crates/buildl-core/tests/flows/graph.rs:216`

## Probe results

No separate probe was run. Every fact a task asserts about existing code is put under test by the task's own cycle, and every run matched the plan:

- task 1 red — `cargo test -p buildl-core --test flows graph` — ``error[E0599]: no method named `graph` found for struct `Pipeline<P>` in the current scope``
- task 1 green — same command — `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out`
- task 2 red, with the placeholder golden `{}` — same command — `test result: FAILED. 6 passed; 2 failed; 0 ignored; 0 measured; 13 filtered out`. The serialized graph the two failures printed matched this plan's golden text before the golden was written.
- task 2 green — same command — `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out`

## Deviations

- 2026-10-10 — no task's commit step was run during execution. The commit is the author's to make; each task's files were left in the working tree.
- 2026-10-10 — the plan ran while plan `06`, which it depends on, was built and reviewed but not yet marked `done`. Plans `06` to `09` were executed as one requested run.
