---
status: done
created: 2026-10-11
depends-on: [06, 09]
---

# Pipeline Plan Implementation Plan

**Goal:** The pipeline stops at `plan`.

**Architecture:** `plan/assembly.rs` holds `plan`, a free function generic over the `Ports` bundle, re-exported from the crate root as `load` and `resolve` are. It only sequences: the manifest, the overrides, the ceiling check, then — and only then — the tools, the input digests, the host, each target's own components, and the verdicts over a topological order. The three steps that ask a port live beside the decisions they wrap, as thin functions in `plan/tools.rs`, `plan/inputs.rs` and `plan/components.rs`; they are written here, with the flows that are their tests, because nothing inside the crate implements a port. `Pipeline::plan` runs Load once, Resolve, then Plan, and takes `&mut self` because recording into the stat cache does. The flows drive it through the fakes and simulate a previous build through the public API: targets are taken in dependency order and each row's key is finished with `ActionKey::new`, which also works for a target that is never clean. One flow pins the serialized plan of a fixture workspace in a golden file.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §5.1 (signature, placement, order), §5.4 (tools, inputs, host), §6 (the pipeline), §8 (the module edges), §9 (the flows, the golden file, `plan/assembly.rs`'s row).

**Checkpoints:** review twice: once when task 1 is done, and once when tasks 2 and 3 are done.

---

## Context an implementer needs

What plans `04` to `09` leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `effective` | `crates/buildl-core/src/plan/settings.rs` | step 2 |
| `sites`, `Sites { programs, env, inputs }`, `check` | `crates/buildl-core/src/plan/authority.rs` | step 3, and the lists steps 4 to 6 ask for |
| `programs`, `program_of` | `crates/buildl-core/src/plan/tools.rs` | the file that gains the call to `ToolResolver` |
| `Read`, `split`, `Split`, `hashed` | `crates/buildl-core/src/plan/inputs.rs` | the file that gains the calls to `Digester` and `StatCache` |
| `Gathered`, `own` | `crates/buildl-core/src/plan/components.rs` | step 7; the file that gains the call to `Host` |
| `topological` | `crates/buildl-core/src/plan/order.rs` | the order step 8 walks |
| `verdict::all(graph, own, order, row)` | `crates/buildl-core/src/plan/verdict.rs` | step 8 |
| `Plan::new` (`pub(crate)`, under a `cfg_attr(not(test), expect(dead_code, …))`) | `crates/buildl-core/src/types/plan.rs` | step 9 |
| `PortSet<P>` with public fields; `Pipeline::ports`, `ports_mut`, `graph` | `crates/buildl-core/src/ports/port_set.rs`, `crates/buildl-core/src/pipeline/driver.rs` | what `plan` borrows, and what the flows change between two plans |
| `Error::MissingTool`, `Error::MissingInput`, `DeclarationSite::from(&Node)` | `crates/buildl-core/src/error.rs` | the two failures the port calls raise |
| `ports`, `open_workspace`, `program`, `dir`, the six fakes with their counters | `crates/buildl-core/tests/flows/common.rs` | what the flows are built from |
| `fixture()`, the two-directory workspace of the graph flow (private) | `crates/buildl-core/tests/flows/graph.rs` | the workspace task 3 pins the plan of |
| `mod plan;` (private); every `mod` of `plan/mod.rs` under a `cfg_attr(not(test), expect(dead_code, …))` | `crates/buildl-core/src/lib.rs`, `crates/buildl-core/src/plan/mod.rs` | what becomes public, and the attributes this plan removes |
| 18 lines, with `plan -> error` followed by `plan -> types` | `crates/expected-module-edges.txt` | the golden list that gains two edges |

Gate facts that bite here. A bullet that names a lint reports one seen as a failing run while this
code was prototyped:

- Once `plan` calls every file of the phase, an `expect(dead_code)` left on a `mod` line is
  unfulfilled and fails the gate, and so does the one on `Plan::new`. Task 1 removes them all in
  the same commit that adds the caller.
- `pipeline/driver.rs` gains `crate::plan`, and three files of `plan/` gain `crate::ports`, so
  `crates/expected-module-edges.txt` gains `pipeline -> plan` and `plan -> ports` in task 1. The file must hold no edge
  between `plan` and `load` or `resolve`.
- clippy's `missing_const_for_fn` rejects the flow helpers `own_of` and `changed` unless they
  are `const fn`.
- The default fakes describe a workspace whose ceiling allows `cc` to run, any workspace file to
  be read, and `out` and `.buildl` to be written; a flow that needs another ceiling changes it
  through `ports_mut` before planning.
- The golden file is one line with a trailing newline; the flow compares against
  `GOLDEN.trim_end()`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/plan/assembly.rs         — [create] plan: the nine steps in order
crates/buildl-core/src/plan/tools.rs            — [modify] fingerprints: ask the tool resolver
crates/buildl-core/src/plan/inputs.rs           — [modify] digests: read, reuse, hash, record, commit
crates/buildl-core/src/plan/components.rs       — [modify] environment: ask the host
crates/buildl-core/src/plan/mod.rs              — [modify] declare assembly, export plan, drop the dead-code expectations
crates/buildl-core/src/lib.rs                   — [modify] make the module public, re-export plan
crates/buildl-core/src/types/plan.rs            — [modify] drop the dead-code expectation; link the function
crates/buildl-core/src/pipeline/driver.rs       — [modify] Pipeline::plan
crates/buildl-core/tests/flows/plan.rs          — [create] the flows
crates/buildl-core/tests/flows/golden/plan.json — [create] the pinned serialized plan
crates/buildl-core/tests/flows/graph.rs         — [modify] share the fixture workspace
crates/buildl-core/tests/flows/main.rs          — [modify] declare the flow module; its doc names every port
crates/expected-module-edges.txt                — [modify] two edges
```

### Task 1 — Plan a workspace through the ports

**Files:**
- Create `crates/buildl-core/src/plan/assembly.rs`
- Create `crates/buildl-core/tests/flows/plan.rs`
- Modify `crates/buildl-core/src/plan/tools.rs`
- Modify `crates/buildl-core/src/plan/inputs.rs`
- Modify `crates/buildl-core/src/plan/components.rs`
- Modify `crates/buildl-core/src/plan/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/types/plan.rs`
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/main.rs`
- Modify `crates/expected-module-edges.txt`

**Steps:**

1. Declare the flow module, and say in the file's doc that the flows now drive every port. `crates/buildl-core/tests/flows/main.rs` becomes:

   ```rust
   //! Pipeline flows through the ports, driven by fakes built from the public API alone.
   //!
   //! One test binary, so the fakes are shared by every flow without being compiled once per file.
   //! That the fakes compile at all is itself a check: an adapter outside this crate can build staged
   //! targets, settings and subdirs and implement every port the same way.

   pub mod common;

   mod check;
   mod graph;
   mod load;
   mod plan;
   ```

2. Write the failing flows. Create `crates/buildl-core/tests/flows/plan.rs`:

   ```rust
   //! `Pipeline::plan`, driven through every port it reads by the fakes of [`crate::common`].

   #![expect(
       clippy::unwrap_used,
       reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
   )]

   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, FileStat,
       KeyComponents, Label, NodeId, OwnComponents, Pipeline, Plan, PlannedKey, SettingName,
       SettingValue, SourcePath, TargetGraph, Timestamp, Verdict, Written,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, file, ports, program, target_with, workspace,
   };

   /// The values given to settings for one run.
   type Overrides = BTreeMap<SettingName, SettingValue>;

   fn entry() -> EntryName {
       EntryName::parse("build.lua").unwrap()
   }

   fn label(raw: &str) -> Label {
       Label::parse(raw).unwrap()
   }

   fn path(raw: &str) -> SourcePath {
       SourcePath::parse(raw).unwrap()
   }

   /// Writes or replaces a workspace file holding `text`, last modified at `modified`.
   fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str, text: &str, modified: u64) {
       let content = FakeContent {
           bytes: text.as_bytes().to_vec(),
           stat: FileStat {
               modified: Timestamp::from_unix_nanos(modified),
               size: u64::try_from(text.len()).unwrap(),
           },
       };
       pipeline
           .ports_mut()
           .digester
           .files
           .insert(path(raw), content);
   }

   /// A plan of the workspace with no setting overridden.
   fn planned(pipeline: &mut Pipeline<FakePorts>) -> Plan {
       pipeline.plan(&entry(), &Overrides::new()).unwrap()
   }

   /// What a verdict says, without its key: the reason, and the labels of the targets awaited.
   #[derive(Debug, PartialEq, Eq)]
   enum Said {
       Clean,
       Dirty(DirtyReason, Vec<Label>),
       Conditional(Vec<Label>),
   }

   /// What `plan` says about the target `raw` names.
   fn said(plan: &Plan, raw: &str) -> Said {
       let wanted = label(raw);
       let id = (0_u32..)
           .take(plan.len())
           .map(NodeId::new)
           .find(|id| plan.label(*id) == Some(&wanted))
           .unwrap();
       let labels = |awaiting: &[NodeId]| -> Vec<Label> {
           awaiting
               .iter()
               .map(|id| plan.label(*id).unwrap().clone())
               .collect()
       };
       match plan.verdict(id).unwrap() {
           Verdict::Clean { .. } => Said::Clean,
           Verdict::Dirty { reason, key } => {
               let awaiting = match key {
                   PlannedKey::Finished(_) => Vec::new(),
                   PlannedKey::Unfinished(unfinished) => labels(&unfinished.awaiting),
               };
               Said::Dirty(reason.clone(), awaiting)
           }
           Verdict::Conditional(unfinished) => Said::Conditional(labels(&unfinished.awaiting)),
       }
   }

   /// The components a verdict holds for its target, whatever the verdict.
   const fn own_of(verdict: &Verdict) -> &OwnComponents {
       match verdict {
           Verdict::Clean { key }
           | Verdict::Dirty {
               key: PlannedKey::Finished(key),
               ..
           } => &key.components().own,
           Verdict::Dirty {
               key: PlannedKey::Unfinished(unfinished),
               ..
           }
           | Verdict::Conditional(unfinished) => &unfinished.own,
       }
   }

   /// Records one target as built, as an executor would: under the key `plan` holds for it, finished
   /// with what its dependencies' rows say they produced, with every output's content derived from
   /// `version`.
   fn record(
       pipeline: &mut Pipeline<FakePorts>,
       graph: &TargetGraph,
       plan: &Plan,
       id: NodeId,
       version: &str,
   ) {
       let label = plan.label(id).unwrap().clone();
       let rows = &pipeline.ports().cache.rows;
       let deps = graph
           .deps(id)
           .unwrap()
           .iter()
           .map(|dep| {
               let dep_label = plan.label(*dep).unwrap();
               DepProducts {
                   label: dep_label.clone(),
                   outputs: rows.get(dep_label).unwrap().produced(),
               }
           })
           .collect();
       let own = own_of(plan.verdict(id).unwrap()).clone();
       let outputs = own
           .outputs
           .iter()
           .map(|name| Digest::of(format!("{label} {name} {version}").as_bytes()))
           .collect();
       let key = ActionKey::new(KeyComponents { own, deps }).unwrap();
       let row = CacheRow::new(key, outputs).unwrap();
       pipeline.ports_mut().cache.rows.insert(label, row);
   }

   /// Records a build of every target, each after the targets it depends on.
   ///
   /// The helper takes the targets in dependency order rather than planning until all are clean,
   /// so it also works for a workspace holding a target that is never clean.
   fn build(pipeline: &mut Pipeline<FakePorts>, overrides: &Overrides, version: &str) {
       let graph = pipeline.graph(&entry()).unwrap();
       let plan = pipeline.plan(&entry(), overrides).unwrap();
       let mut built = BTreeSet::new();
       while built.len() < graph.len() {
           let ready: Vec<NodeId> = (0_u32..)
               .take(graph.len())
               .map(NodeId::new)
               .filter(|id| !built.contains(id))
               .filter(|id| {
                   graph
                       .deps(*id)
                       .unwrap()
                       .iter()
                       .all(|dep| built.contains(dep))
               })
               .collect();
           assert!(!ready.is_empty(), "a graph has no cycle");
           for id in ready {
               record(pipeline, &graph, &plan, id, version);
               built.insert(id);
           }
       }
   }

   /// Records the target `raw` names as rebuilt under the key the current plan holds for it.
   fn rebuild(pipeline: &mut Pipeline<FakePorts>, raw: &str, version: &str) {
       let graph = pipeline.graph(&entry()).unwrap();
       let plan = planned(pipeline);
       let id = graph.id_of(&label(raw)).unwrap();
       record(pipeline, &graph, &plan, id, version);
   }

   /// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone. Each reads one file and
   /// runs `cc`, which the fake resolver can find.
   fn chain() -> Pipeline<FakePorts> {
       let reading = |name: &str, input: &str, deps: &[&str]| {
           target_with(name, |target| {
               target.inputs = vec![Written::new(input)];
               target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
           })
       };
       let source = FakeSource::new(workspace(vec![(
           "",
           file(
               vec![
                   reading("gen", "gen.c", &[]),
                   reading("lib", "lib.c", &["gen"]),
                   reading("app", "main.c", &["lib"]),
                   reading("docs", "README", &[]),
               ],
               &[],
           ),
       )]));
       let mut pipeline = Pipeline::new(ports(source));
       for name in ["gen.c", "lib.c", "main.c", "README"] {
           put(&mut pipeline, name, name, 100);
       }
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       pipeline
   }

   const fn changed(differences: Vec<Difference>) -> DirtyReason {
       DirtyReason::Changed(differences)
   }

   #[test]
   fn a_workspace_never_built_is_dirty_throughout_and_awaits_its_dependencies() {
       let mut pipeline = chain();
       let plan = planned(&mut pipeline);
       let never = |awaiting: &[&str]| {
           Said::Dirty(
               DirtyReason::NeverBuilt,
               awaiting.iter().map(|raw| label(raw)).collect(),
           )
       };
       assert_eq!(plan.len(), 4);
       assert_eq!(said(&plan, "//:gen"), never(&[]));
       assert_eq!(said(&plan, "//:docs"), never(&[]));
       assert_eq!(said(&plan, "//:lib"), never(&["//:gen"]));
       assert_eq!(said(&plan, "//:app"), never(&["//:lib"]));
       assert_eq!(
           plan.wanted().programs,
           BTreeSet::from([program("cc")]),
           "the wanted set lists the one program every command names"
       );
   }

   #[test]
   fn after_a_build_every_target_is_clean_and_nothing_is_hashed_or_written() {
       let mut pipeline = chain();
       build(&mut pipeline, &Overrides::new(), "v1");
       let digests = pipeline.ports().digester.digest_calls();
       let commits = pipeline.ports().stat_cache.commits();
       assert_eq!(
           (digests, commits),
           (1, 1),
           "the first plan hashed every input"
       );

       let plan = planned(&mut pipeline);

       for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
           assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
       }
       assert_eq!(pipeline.ports().digester.digest_calls(), digests);
       assert_eq!(pipeline.ports().stat_cache.commits(), commits);
   }

   #[test]
   fn a_changed_input_dirties_its_target_and_makes_everything_downstream_conditional() {
       let mut pipeline = chain();
       build(&mut pipeline, &Overrides::new(), "v1");
       put(&mut pipeline, "gen.c", "gen.c, edited", 200);

       let plan = planned(&mut pipeline);

       assert_eq!(
           said(&plan, "//:gen"),
           Said::Dirty(changed(vec![Difference::Input(path("gen.c"))]), Vec::new())
       );
       assert_eq!(
           said(&plan, "//:lib"),
           Said::Conditional(vec![label("//:gen")])
       );
       assert_eq!(
           said(&plan, "//:app"),
           Said::Conditional(vec![label("//:lib")])
       );
       assert_eq!(said(&plan, "//:docs"), Said::Clean);
   }

   #[test]
   fn a_target_rebuilt_to_the_same_output_leaves_its_dependents_clean() {
       let mut pipeline = chain();
       build(&mut pipeline, &Overrides::new(), "v1");
       put(&mut pipeline, "gen.c", "gen.c, edited", 200);
       let old_key = pipeline.ports().cache.rows[&label("//:gen")].key().digest();

       rebuild(&mut pipeline, "//:gen", "v1");

       let new_key = pipeline.ports().cache.rows[&label("//:gen")].key().digest();
       assert_ne!(new_key, old_key, "the edit changed the key of `//:gen`");
       let plan = planned(&mut pipeline);
       for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
           assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
       }
   }

   #[test]
   fn a_target_rebuilt_to_another_output_dirties_the_target_that_depends_on_it() {
       let mut pipeline = chain();
       build(&mut pipeline, &Overrides::new(), "v1");
       put(&mut pipeline, "gen.c", "gen.c, edited", 200);

       rebuild(&mut pipeline, "//:gen", "v2");

       let plan = planned(&mut pipeline);
       assert_eq!(said(&plan, "//:gen"), Said::Clean);
       assert_eq!(
           said(&plan, "//:lib"),
           Said::Dirty(changed(vec![Difference::Dep(label("//:gen"))]), Vec::new())
       );
       assert_eq!(
           said(&plan, "//:app"),
           Said::Conditional(vec![label("//:lib")])
       );
       assert_eq!(said(&plan, "//:docs"), Said::Clean);
   }

   #[test]
   fn a_file_touched_without_changing_is_hashed_once_and_its_target_stays_clean() {
       let mut pipeline = chain();
       build(&mut pipeline, &Overrides::new(), "v1");
       put(&mut pipeline, "gen.c", "gen.c", 200);

       let plan = planned(&mut pipeline);

       for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
           assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
       }
       assert_eq!(pipeline.ports().digester.digest_calls(), 2);
       assert_eq!(
           pipeline.ports().digester.digested(),
           [
               path("README"),
               path("gen.c"),
               path("lib.c"),
               path("main.c"),
               path("gen.c"),
           ],
           "every input on the first plan, in path order, then the touched file alone"
       );
       assert_eq!(pipeline.ports().stat_cache.commits(), 2);

       planned(&mut pipeline);
       assert_eq!(
           pipeline.ports().digester.digest_calls(),
           2,
           "the new metadata was recorded"
       );
   }
   ```

3. Run them and confirm they fail to build, because `Pipeline` has no `plan`:

   ```
   $ cargo test -p buildl-core --test flows plan
   error[E0599]: no method named `plan` found for mutable reference `&mut Pipeline<FakePorts>` in the current scope
   ```

4. Ask the tool resolver. In `crates/buildl-core/src/plan/tools.rs`, update the module doc and the imports. Replace:

   ```rust
   //! Responsibilities: [`program_of`] and [`programs`].
   //!
   //! Non-responsibilities: whether a program may run, which is decided against the ceiling.

   use std::collections::BTreeMap;

   use crate::types::{Command, Node, Program, TargetGraph};
   ```

   With:

   ```rust
   //! Responsibilities: [`program_of`], [`programs`] and [`fingerprints`].
   //!
   //! Non-responsibilities: whether a program may run, which is decided against the ceiling, and
   //! finding one, which is the [`ToolResolver`] port's.

   use std::collections::BTreeMap;

   use crate::error::{DeclarationSite, Error, Result};
   use crate::ports::ToolResolver;
   use crate::types::{Command, Digest, Node, Program, TargetGraph};
   ```

5. In the same file, directly above `#[cfg(test)]`, insert:

   ```rust
   /// The fingerprint of every program in `programs`, asked for in program order.
   ///
   /// # Errors
   ///
   /// Returns [`Error::MissingTool`] for the first program `tools` cannot find, naming the
   /// lowest-labelled target whose command names it, and whatever `tools` itself returns.
   pub(crate) fn fingerprints<T: ToolResolver>(
       programs: &BTreeMap<Program, &Node>,
       tools: &T,
   ) -> Result<BTreeMap<Program, Digest>> {
       let mut found = BTreeMap::new();
       for (program, node) in programs {
           let Some(fingerprint) = tools.fingerprint(program)? else {
               return Err(Error::MissingTool {
                   site: Box::new(DeclarationSite::from(*node)),
                   program: program.clone(),
               });
           };
           found.insert(program.clone(), fingerprint);
       }
       Ok(found)
   }
   ```

6. Read, reuse, hash and record. In `crates/buildl-core/src/plan/inputs.rs`, update the module doc and the imports. Replace:

   ```rust
   //! Responsibilities: [`Read`], [`reusable`], [`split`], [`Split`] and [`hashed`].
   //!
   //! Non-responsibilities: reading metadata, hashing, and storing what was hashed. All three
   //! arrive through ports; the decisions here are over the values those ports returned.

   use crate::error::{Error, PortName, Result};
   use crate::types::{Diagnostic, Digest, FileStat, SourcePath, StatEntry};
   ```

   With:

   ```rust
   //! Responsibilities: [`digests`], and the decisions it is made of: [`Read`], [`reusable`],
   //! [`split`], [`Split`] and [`hashed`].
   //!
   //! Non-responsibilities: reading metadata, hashing, and storing what was hashed. All three
   //! arrive through ports; the decisions here are over the values those ports returned.

   use std::collections::BTreeMap;

   use crate::error::{DeclarationSite, Error, PortName, Result};
   use crate::ports::{Digester, StatCache};
   use crate::types::{Diagnostic, Digest, FileStat, Node, SourcePath, StatEntry};
   ```

7. In the same file, directly above the doc comment `/// One input once its metadata has been read.`, insert:

   ```rust
   /// The content digest of every input in `inputs`.
   ///
   /// Each input's metadata is read first, in path order. An input whose metadata equals what was
   /// recorded for it keeps the recorded digest; the others are hashed in one call, in path order,
   /// and recorded. The stat cache is committed once when anything was recorded and not at all
   /// otherwise, so a plan that hashes nothing writes nothing.
   ///
   /// # Errors
   ///
   /// Returns [`Error::MissingInput`] for the first input that does not exist, naming the
   /// lowest-labelled target declaring it; [`Error::Adapter`] when the digester's reply does not
   /// hold one digest per file; and whatever either port returns.
   pub(crate) fn digests<'g, D: Digester, S: StatCache>(
       inputs: &BTreeMap<&'g SourcePath, &'g Node>,
       digester: &D,
       stat_cache: &mut S,
   ) -> Result<BTreeMap<&'g SourcePath, Digest>> {
       let mut read = Vec::with_capacity(inputs.len());
       for (path, node) in inputs {
           let Some(stat) = digester.stat(path)? else {
               return Err(Error::MissingInput {
                   site: Box::new(DeclarationSite::from(*node)),
                   path: (*path).clone(),
               });
           };
           read.push(Read {
               path,
               stat,
               recorded: stat_cache.entry(path),
           });
       }

       let Split { reused, pending } = split(&read);
       let mut found: BTreeMap<&SourcePath, Digest> = reused.into_iter().collect();
       if pending.is_empty() {
           return Ok(found);
       }
       let paths: Vec<SourcePath> = pending.iter().map(|(path, _)| (*path).clone()).collect();
       let reply = digester.digest(&paths)?;
       for (path, entry) in hashed(&pending, &reply)? {
           stat_cache.record(path.clone(), entry);
           found.insert(path, entry.digest);
       }
       stat_cache.commit()?;
       Ok(found)
   }
   ```

8. Ask the host. In `crates/buildl-core/src/plan/components.rs`, update the module doc and the imports. Replace:

   ```rust
   //! Responsibilities: [`Gathered`] and [`own`].
   //!
   //! Non-responsibilities: fetching the values, and a target's dependencies, which join its key
   //! only once it is known what they produced.

   use std::collections::BTreeMap;

   use crate::plan::settings::referenced;
   use crate::plan::tools::program_of;
   use crate::types::{
       Digest, EnvName, EnvValue, InputDigest, Node, OwnComponents, Platform, Program, SettingName,
       SettingValue, SourcePath, ToolIdentity,
   };
   ```

   With:

   ```rust
   //! Responsibilities: [`environment`], [`Gathered`] and [`own`].
   //!
   //! Non-responsibilities: fetching the other values, and a target's dependencies, which join its
   //! key only once it is known what they produced.

   use std::collections::BTreeMap;

   use crate::error::Result;
   use crate::plan::settings::referenced;
   use crate::plan::tools::program_of;
   use crate::ports::Host;
   use crate::types::{
       Digest, EnvName, EnvValue, InputDigest, Node, OwnComponents, Platform, Program, SettingName,
       SettingValue, SourcePath, ToolIdentity,
   };
   ```

9. In the same file, directly above the doc comment `/// Everything gathered for a graph before any target's components are built.`, insert:

   ```rust
   /// The value of every variable in `names`, asked for in the order given.
   ///
   /// # Errors
   ///
   /// Returns whatever `host` returns for the first variable it cannot report.
   pub(crate) fn environment<'g, H: Host>(
       names: impl IntoIterator<Item = &'g EnvName>,
       host: &H,
   ) -> Result<BTreeMap<&'g EnvName, Option<EnvValue>>> {
       names
           .into_iter()
           .map(|name| Ok((name, host.env(name)?)))
           .collect()
   }
   ```

10. Sequence the steps. Create `crates/buildl-core/src/plan/assembly.rs`:

   ```rust
   //! The assembly: from a target graph, and what the ports report, to one plan.
   //!
   //! Its own file because it is where this phase's steps meet and where their order is fixed:
   //! read the manifest, check the overrides, check every want against the ceiling, and only then
   //! ask the machine anything — find the tools, digest the inputs, read the environment — before
   //! keying every target and comparing each key with the one its last build recorded.
   //!
   //! Responsibilities: [`plan`].
   //!
   //! Non-responsibilities: what each step decides. Each has its own sibling file.
   //!
   //! This file holds no test module: it only passes values between its siblings and the ports,
   //! so it is exercised by the crate's integration tests, against in-memory ports.

   use std::collections::BTreeMap;

   use crate::error::Result;
   use crate::plan::authority::{check, sites};
   use crate::plan::components::{Gathered, environment, own};
   use crate::plan::inputs::digests;
   use crate::plan::order::topological;
   use crate::plan::settings::effective;
   use crate::plan::tools::fingerprints;
   use crate::plan::verdict;
   use crate::ports::{ActionCache, Host, Manifest, PortSet, Ports};
   use crate::types::{Plan, SettingName, SettingValue, TargetGraph};

   /// Plans a build of `graph`: one verdict per target, and what the build wants.
   ///
   /// `overrides` gives values to build settings for this run. Every target is given an action key
   /// from its command, the digests of its inputs, its environment, the tool it runs, the settings
   /// it names, the platform, and what its dependencies produced; the key is compared with the one
   /// the target's last build recorded. A target is clean when the two are equal, dirty when they
   /// differ, and conditional when all that could still differ is the output of a dependency that
   /// is about to run.
   ///
   /// Nothing runs. The one thing a plan writes is the stat cache, and only when it had to hash a
   /// file.
   ///
   /// # Errors
   ///
   /// Returns the first failure met, in this order:
   ///
   /// 1. whatever the manifest port returns;
   /// 2. [`Error::UnknownOverride`](crate::Error::UnknownOverride) for an override naming no
   ///    declared setting;
   /// 3. [`Error::Refused`](crate::Error::Refused) for a want outside the workspace's ceiling;
   ///    nothing is asked of any other port before this check has passed;
   /// 4. [`Error::MissingTool`](crate::Error::MissingTool) for a program that cannot be found;
   /// 5. [`Error::MissingInput`](crate::Error::MissingInput) for an input that does not exist;
   /// 6. whatever the host port returns for an environment variable.
   ///
   /// A port's own failure is returned from the step that met it.
   pub fn plan<P: Ports>(
       graph: &TargetGraph,
       overrides: &BTreeMap<SettingName, SettingValue>,
       ports: &mut PortSet<P>,
   ) -> Result<Plan> {
       let workspace = ports.manifest.workspace()?;
       let settings = effective(graph.settings(), overrides)?;
       let sites = sites(graph);
       let wanted = check(graph, &sites, &workspace)?;

       let fingerprints = fingerprints(&sites.programs, &ports.tools)?;
       let digests = digests(&sites.inputs, &ports.digester, &mut ports.stat_cache)?;
       let platform = ports.host.platform();
       let env = environment(sites.env.keys().copied(), &ports.host)?;
       let gathered = Gathered {
           settings,
           fingerprints,
           digests,
           env,
           platform,
       };

       let components = graph
           .nodes()
           .iter()
           .map(|node| own(node, &gathered))
           .collect();
       let order = topological(graph);
       let verdicts = verdict::all(graph, components, &order, |label| ports.cache.row(label))?;
       let labels = graph
           .nodes()
           .iter()
           .map(|node| node.label.clone())
           .collect();
       Ok(Plan::new(labels, verdicts, wanted))
   }
   ```

11. Declare and export it, and drop every dead-code expectation. `crates/buildl-core/src/plan/mod.rs` becomes:

   ```rust
   //! Plan, the third phase: from the target graph to what a build of it would do.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: [`plan`].
   //!
   //! Non-responsibilities: building the graph, which the phase before this one does; and running
   //! anything, or recording what a run produced.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   pub mod assembly;
   mod authority;
   mod components;
   mod difference;
   mod inputs;
   mod order;
   mod settings;
   mod tools;
   mod verdict;

   #[cfg(test)]
   mod fixtures;

   pub use assembly::plan;
   ```

12. Make the phase public. In `crates/buildl-core/src/lib.rs`, replace:

   ```rust
   pub mod pipeline;
   mod plan;
   ```

   With:

   ```rust
   pub mod pipeline;
   pub mod plan;
   ```

13. And re-export the function, in the same file. Replace:

   ```rust
   pub use pipeline::Pipeline;
   ```

   With:

   ```rust
   pub use pipeline::Pipeline;
   pub use plan::plan;
   ```

14. `Plan::new` has a caller now. In `crates/buildl-core/src/types/plan.rs`, replace:

   ```rust
       /// Assembles a plan from one label and one verdict per node, both in id order.
       #[cfg_attr(
           not(test),
           expect(
               dead_code,
               reason = "called by the phase that plans, which is not written yet"
           )
       )]
       pub(crate)
   ```

   With:

   ```rust
       /// Assembles a plan from one label and one verdict per node, both in id order.
       pub(crate)
   ```

15. In the same file, say where a plan comes from. Replace:

   ```rust
   /// Its verdicts are addressed by the [`NodeId`]s of the graph it was made from.
   ```

   With:

   ```rust
   /// A plan is obtained from [`plan`](fn@crate::plan). Its verdicts are addressed by the
   /// [`NodeId`]s of the graph it was made from.
   ```

16. Add the command. In `crates/buildl-core/src/pipeline/driver.rs`, update the module doc and the imports. Replace:

   ```rust
   //! Its own file because the driver owns the sequence of phases, and the sequence grows by one
   //! method per command as later phases arrive. Today it runs Load and Resolve.
   //!
   //! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`] and [`Pipeline::graph`].
   //!
   //! Non-responsibilities: the phases' own logic, which each phase's module holds.

   use core::cmp::min_by;
   use core::fmt;

   use crate::error::{Error, Result};
   use crate::load::{declaration_order, load};
   use crate::ports::{PortSet, Ports};
   use crate::resolve::resolve;
   use crate::types::{Declaration, EntryName, TargetGraph};
   ```

   With:

   ```rust
   //! Its own file because the driver owns the sequence of phases, and the sequence grows by one
   //! method per command as later phases arrive. Today it runs Load, Resolve and Plan.
   //!
   //! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`] and
   //! [`Pipeline::plan`].
   //!
   //! Non-responsibilities: the phases' own logic, which each phase's module holds.

   use core::cmp::min_by;
   use core::fmt;
   use std::collections::BTreeMap;

   use crate::error::{Error, Result};
   use crate::load::{declaration_order, load};
   use crate::plan::plan;
   use crate::ports::{PortSet, Ports};
   use crate::resolve::resolve;
   use crate::types::{Declaration, EntryName, Plan, SettingName, SettingValue, TargetGraph};
   ```

17. Add the method to `impl<P: Ports> Pipeline<P>`, directly below `graph`. Insert:

   ```rust
       /// Loads the workspace once, resolves it, and plans a build of the graph.
       ///
       /// `overrides` gives values to build settings for this run. Takes `&mut self` because a plan
       /// records the digest of every file it had to hash.
       ///
       /// # Errors
       ///
       /// Returns whatever the load returns, then whatever [`resolve`] returns, and otherwise
       /// whatever [`plan`](fn@plan) returns: an override naming no setting, a want outside the
       /// workspace's ceiling, a program or an input that cannot be found, or a port's own failure.
       pub fn plan(
           &mut self,
           entry: &EntryName,
           overrides: &BTreeMap<SettingName, SettingValue>,
       ) -> Result<Plan> {
           let graph = resolve(load(&self.ports.source, entry)?)?;
           plan(&graph, overrides, &mut self.ports)
       }
   ```

18. Record the two new module edges. In `crates/expected-module-edges.txt`, first the pipeline's. Replace:

   ```
   pipeline -> load
   pipeline -> ports
   ```

   With:

   ```
   pipeline -> load
   pipeline -> plan
   pipeline -> ports
   ```

19. Then the phase's. Replace:

   ```
   plan -> error
   plan -> types
   ```

   With:

   ```
   plan -> error
   plan -> ports
   plan -> types
   ```

   The file now reads:

   ```
   error -> types
   json -> error
   load -> error
   load -> ports
   load -> types
   pipeline -> error
   pipeline -> load
   pipeline -> plan
   pipeline -> ports
   pipeline -> resolve
   pipeline -> types
   plan -> error
   plan -> ports
   plan -> types
   ports -> error
   ports -> types
   resolve -> error
   resolve -> types
   types -> error
   types -> json
   ```

20. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows plan
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out
   ```

21. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

22. Commit:

   ```
   $ git add crates/buildl-core/src/plan/assembly.rs crates/buildl-core/src/plan/tools.rs crates/buildl-core/src/plan/inputs.rs crates/buildl-core/src/plan/components.rs crates/buildl-core/src/plan/mod.rs crates/buildl-core/src/lib.rs crates/buildl-core/src/types/plan.rs crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/plan.rs crates/buildl-core/tests/flows/main.rs crates/expected-module-edges.txt
   $ git commit -m "feat(buildl-core): plan a workspace through the ports"
   ```

### Task 2 — Pin what reaches a key, what is refused, and in what order

**Files:**
- Modify `crates/buildl-core/tests/flows/plan.rs`

**Steps:**

1. Widen the imports of `crates/buildl-core/tests/flows/plan.rs`. Replace:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, FileStat,
       KeyComponents, Label, NodeId, OwnComponents, Pipeline, Plan, PlannedKey, SettingName,
       SettingValue, SourcePath, TargetGraph, Timestamp, Verdict, Written,
   };

   use crate::common::{
       FakeContent, FakePorts, FakeSource, file, ports, program, target_with, workspace,
   };
   ```

   With:

   ```rust
   use std::collections::{BTreeMap, BTreeSet};
   use std::path::Path;

   use buildl_core::{
       ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, EnvName,
       EnvValue, Error, EvaluationFailure, FileStat, Freshness, KeyComponents, Label, NodeId,
       OwnComponents, Pipeline, Plan, PlannedKey, Platform, PortName, SettingName, SettingValue,
       SourcePath, StagedItem, TargetGraph, Timestamp, Verdict, Want, Written,
   };

   use crate::common::{
       FakeContent, FakeFile, FakePorts, FakeSource, dir, file, ports, program, setting, target,
       target_with, workspace,
   };
   ```

2. Add five helpers and twelve flows at the end of the file:

   ```rust
   fn overrides(pairs: &[(&str, &str)]) -> Overrides {
       pairs
           .iter()
           .map(|(name, value)| {
               (
                   SettingName::parse(*name).unwrap(),
                   SettingValue::parse(*value).unwrap(),
               )
           })
           .collect()
   }

   fn env_name(raw: &str) -> EnvName {
       EnvName::parse(raw).unwrap()
   }

   /// A `b.target(name, { run = run })` call.
   fn running(name: &str, run: &[&str]) -> StagedItem {
       target_with(name, |target| {
           target.run = Some(run.iter().map(|argument| Written::new(*argument)).collect());
       })
   }

   /// A pipeline over one root build file making `items`, with `cc` findable.
   fn root_only(items: Vec<StagedItem>) -> Pipeline<FakePorts> {
       let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
       let mut pipeline = Pipeline::new(ports(source));
       pipeline
           .ports_mut()
           .tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       pipeline
   }

   /// The label and build file an error names as its site.
   fn site_of(error: &Error) -> Option<(String, &Path)> {
       let site = match error {
           Error::Refused { site, .. } => site.as_deref(),
           Error::MissingTool { site, .. } | Error::MissingInput { site, .. } => Some(&**site),
           _ => None,
       };
       site.map(|site| (site.label.to_string(), site.provenance.file()))
   }

   #[test]
   fn an_override_dirties_exactly_the_targets_whose_commands_name_the_setting() {
       let mut pipeline = root_only(vec![
           setting("mode"),
           running("a", &["cc", "-m=$opt:mode"]),
           running("b", &["cc", "$opt:mode"]),
           running("c", &["cc"]),
       ]);
       build(&mut pipeline, &Overrides::new(), "v1");

       let plan = pipeline
           .plan(&entry(), &overrides(&[("mode", "release")]))
           .unwrap();

       let mode = SettingName::parse("mode").unwrap();
       for raw in ["//:a", "//:b"] {
           assert_eq!(
               said(&plan, raw),
               Said::Dirty(changed(vec![Difference::Setting(mode.clone())]), Vec::new()),
               "{raw}"
           );
       }
       assert_eq!(said(&plan, "//:c"), Said::Clean);
       assert_eq!(said(&planned(&mut pipeline), "//:a"), Said::Clean);
   }

   #[test]
   fn an_environment_value_a_tool_and_the_platform_each_reach_the_key() {
       let mut pipeline = root_only(vec![target_with("app", |target| {
           target.env = vec![Written::new("PATH")];
       })]);
       let path_name = env_name("PATH");
       let value = |text: &str| EnvValue::parse(text).unwrap();
       {
           let ports = pipeline.ports_mut();
           ports
               .manifest
               .workspace
               .ceiling
               .env
               .insert(path_name.clone());
           ports
               .host
               .variables
               .insert(path_name.clone(), value("/bin"));
       }
       build(&mut pipeline, &Overrides::new(), "v1");
       let dirty = |difference: Difference| Said::Dirty(changed(vec![difference]), Vec::new());

       let host = &mut pipeline.ports_mut().host;
       host.variables.insert(path_name.clone(), value("/usr/bin"));
       assert_eq!(
           said(&planned(&mut pipeline), "//:app"),
           dirty(Difference::Env(path_name.clone()))
       );
       pipeline.ports_mut().host.variables.remove(&path_name);
       assert_eq!(
           said(&planned(&mut pipeline), "//:app"),
           dirty(Difference::Env(path_name.clone())),
           "an unset variable is not the value it had"
       );
       let host = &mut pipeline.ports_mut().host;
       host.variables.insert(path_name, value("/bin"));
       assert_eq!(said(&planned(&mut pipeline), "//:app"), Said::Clean);

       let tools = &mut pipeline.ports_mut().tools;
       tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 15.1"));
       assert_eq!(
           said(&planned(&mut pipeline), "//:app"),
           dirty(Difference::Tool)
       );
       let tools = &mut pipeline.ports_mut().tools;
       tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));

       pipeline.ports_mut().host.platform = Platform::parse("macos/aarch64").unwrap();
       assert_eq!(
           said(&planned(&mut pipeline), "//:app"),
           dirty(Difference::Platform)
       );
   }

   #[test]
   fn a_target_that_always_runs_is_dirty_on_every_plan_and_its_dependent_conditional() {
       let mut pipeline = root_only(vec![
           target_with("gen", |target| target.freshness = Freshness::Always),
           target("lib", &["gen"]),
       ]);
       build(&mut pipeline, &Overrides::new(), "v1");

       for _ in 0..2 {
           let plan = planned(&mut pipeline);
           assert_eq!(
               said(&plan, "//:gen"),
               Said::Dirty(DirtyReason::Always, Vec::new())
           );
           assert_eq!(
               said(&plan, "//:lib"),
               Said::Conditional(vec![label("//:gen")])
           );
       }
   }

   #[test]
   fn a_program_outside_the_ceiling_is_refused_before_anything_is_read() {
       let mut pipeline = chain();
       pipeline.ports_mut().manifest.workspace.ceiling.run.clear();

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
           error.to_string(),
           "build.lua: //:app: program cc is outside the ceiling (proc.run allows: nothing)"
       );
       assert_eq!(
           site_of(&error),
           Some(("//:app".to_owned(), Path::new("build.lua")))
       );
       match error {
           Error::Refused { want, ceiling, .. } => {
               assert_eq!(want, Want::Program(program("cc")));
               assert_eq!(*ceiling, pipeline.ports().manifest.workspace.ceiling);
           }
           other => unreachable!("expected Refused, got {other:?}"),
       }
       let ports = pipeline.ports();
       assert_eq!(ports.digester.stat_calls(), 0);
       assert_eq!(ports.digester.digest_calls(), 0);
       assert!(ports.tools.lookups().is_empty());
       assert_eq!(ports.stat_cache.commits(), 0);
   }

   #[test]
   fn a_variable_an_input_and_a_directory_outside_the_ceiling_are_each_refused() {
       let mut reads_secret = root_only(vec![target_with("app", |target| {
           target.env = vec![Written::new("AWS_SECRET")];
       })]);
       let error = reads_secret.plan(&entry(), &Overrides::new()).unwrap_err();
       assert_eq!(
           error.to_string(),
           "build.lua: //:app: environment variable AWS_SECRET is outside the ceiling \
            (env.read allows: nothing)"
       );

       let mut reads_outside = chain();
       let ceiling = &mut reads_outside.ports_mut().manifest.workspace.ceiling;
       ceiling.read = BTreeSet::from([dir("src")]);
       let error = reads_outside.plan(&entry(), &Overrides::new()).unwrap_err();
       assert_eq!(
           error.to_string(),
           "build.lua: //:docs: input README is outside the ceiling (fs.read allows: src)"
       );

       let mut writes_outside = chain();
       let ceiling = &mut writes_outside.ports_mut().manifest.workspace.ceiling;
       ceiling.write = BTreeSet::from([dir(".buildl")]);
       let error = writes_outside
           .plan(&entry(), &Overrides::new())
           .unwrap_err();
       assert_eq!(
           error.to_string(),
           "directory out is outside the ceiling (fs.write allows: .buildl)"
       );
       assert_eq!(site_of(&error), None);
   }

   #[test]
   fn a_program_that_cannot_be_found_names_the_lowest_labelled_target_running_it() {
       let mut pipeline = chain();
       pipeline.ports_mut().tools.fingerprints.clear();

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
           error.to_string(),
           "build.lua: //:app: program cc was not found"
       );
       assert_eq!(
           site_of(&error),
           Some(("//:app".to_owned(), Path::new("build.lua")))
       );
       assert!(
           matches!(error, Error::MissingTool { program: missing, .. } if missing == program("cc"))
       );
   }

   #[test]
   fn an_input_that_does_not_exist_names_the_target_declaring_it() {
       let mut pipeline = chain();
       pipeline.ports_mut().digester.files.remove(&path("lib.c"));

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(
           error.to_string(),
           "build.lua: //:lib: input lib.c does not exist"
       );
       assert_eq!(
           site_of(&error),
           Some(("//:lib".to_owned(), Path::new("build.lua")))
       );
       assert!(matches!(error, Error::MissingInput { path: missing, .. } if missing == path("lib.c")));
       assert_eq!(pipeline.ports().digester.digest_calls(), 0);
   }

   #[test]
   fn a_program_named_by_several_targets_is_looked_up_once() {
       let mut pipeline = chain();
       planned(&mut pipeline);
       assert_eq!(pipeline.ports().tools.lookups(), [program("cc")]);
   }

   #[test]
   fn of_two_variables_the_host_cannot_report_the_lower_name_is_the_error() {
       let mut pipeline = root_only(vec![target_with("app", |target| {
           target.env = vec![Written::new("ZED"), Written::new("ALPHA")];
       })]);
       let names = BTreeSet::from([env_name("ALPHA"), env_name("ZED")]);
       {
           let ports = pipeline.ports_mut();
           ports.manifest.workspace.ceiling.env = names.clone();
           ports.host.unreportable = names;
       }

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();

       assert_eq!(error.to_string(), "host: ALPHA does not hold valid text");
       assert!(matches!(error, Error::Adapter { port, .. } if port == PortName::Host));
   }

   #[test]
   fn several_faults_are_reported_one_at_a_time_in_a_fixed_order() {
       let source = FakeSource::new(workspace(vec![(
           "",
           file(
               vec![
                   running("image", &["docker", "build"]),
                   target_with("app", |target| {
                       target.inputs = vec![Written::new("missing.c")];
                   }),
               ],
               &[],
           ),
       )]));
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       let unknown = overrides(&[("bogus", "1")]);
       let error = pipeline.plan(&entry(), &unknown).unwrap_err();
       assert!(matches!(error, Error::UnknownOverride { .. }), "{error}");

       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("docker"))),
           "{error}"
       );

       let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
       ceiling.run.insert(program("docker"));
       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::MissingTool { program: missing, .. } if missing == &program("cc")),
           "{error}"
       );

       for name in ["cc", "docker"] {
           let tools = &mut pipeline.ports_mut().tools;
           tools
               .fingerprints
               .insert(program(name), Digest::of(name.as_bytes()));
       }
       let error = pipeline.plan(&entry(), &Overrides::new()).unwrap_err();
       assert!(
           matches!(&error, Error::MissingInput { path: missing, .. } if missing == &path("missing.c")),
           "{error}"
       );

       put(&mut pipeline, "missing.c", "found", 100);
       assert_eq!(planned(&mut pipeline).len(), 2);
   }

   #[test]
   fn an_override_naming_no_setting_is_refused() {
       let mut pipeline = root_only(vec![setting("mode"), running("a", &["cc", "$opt:mode"])]);

       let error = pipeline
           .plan(&entry(), &overrides(&[("mod", "release")]))
           .unwrap_err();

       assert_eq!(
           error.to_string(),
           "setting mod is overridden but not declared"
       );
       assert!(matches!(error, Error::UnknownOverride { name } if name.as_str() == "mod"));
   }

   #[test]
   fn a_build_file_that_fails_to_evaluate_is_the_load_error_unchanged() {
       let source = FakeSource::new(workspace(vec![
           ("", file(vec![target("app", &[])], &["lib"])),
           ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
       ]));
       let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

       match pipeline.plan(&entry(), &Overrides::new()).unwrap_err() {
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

3. Run the flows. They pass at once, because task 1 built the behaviour they pin:

   ```
   $ cargo test -p buildl-core --test flows plan
   test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out
   ```

4. Confirm such a flow can fail. In `crates/buildl-core/src/plan/assembly.rs`, move the line `let settings = effective(graph.settings(), overrides)?;` to directly below `let wanted = check(graph, &sites, &workspace)?;`, so that the ceiling is checked before the overrides, and run the flow that pins the order:

   ```
   $ cargo test -p buildl-core --test flows plan::several_faults
   test plan::several_faults_are_reported_one_at_a_time_in_a_fixed_order ... FAILED
   build.lua: //:image: program docker is outside the ceiling (proc.run allows: cc)
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 38 filtered out; finished in 0.00s
   ```

   The refusal arrives where the unknown override should. Move the line back.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/plan.rs
   $ git commit -m "test(buildl-core): pin what a plan keys on, refuses and reports first"
   ```

### Task 3 — Pin the serialized plan of a fixture workspace

**Files:**
- Create `crates/buildl-core/tests/flows/golden/plan.json`
- Modify `crates/buildl-core/tests/flows/plan.rs`
- Modify `crates/buildl-core/tests/flows/graph.rs`

**Steps:**

1. Share the graph flow's workspace. In `crates/buildl-core/tests/flows/graph.rs`, replace:

   ```rust
   fn fixture() -> FakeSource {
   ```

   With:

   ```rust
   pub(crate) fn fixture() -> FakeSource {
   ```

2. Write the failing flow. In `crates/buildl-core/tests/flows/plan.rs`, import the serializer. Replace:

   ```rust
       SourcePath, StagedItem, TargetGraph, Timestamp, Verdict, Want, Written,
   };
   ```

   With:

   ```rust
       SourcePath, StagedItem, TargetGraph, Timestamp, Verdict, Want, Written, json,
   };
   ```

3. Add the golden constant directly above the doc comment `/// The values given to settings for one run.` Insert:

   ```rust
   /// The serialized plan of [`crate::graph::fixture`] after a build and one edit, pinned.
   const GOLDEN: &str = include_str!("golden/plan.json");
   ```

4. Add the flow at the end of the file:

   ```rust
   #[test]
   fn a_built_workspace_with_one_edit_plans_to_the_pinned_json() {
       let mut pipeline = Pipeline::<FakePorts>::new(ports(crate::graph::fixture()));
       put(&mut pipeline, "main.c", "main.c", 100);
       put(&mut pipeline, "lib/text.c", "lib/text.c", 100);
       let tools = &mut pipeline.ports_mut().tools;
       tools
           .fingerprints
           .insert(program("cc"), Digest::of(b"cc 14.2"));
       build(&mut pipeline, &Overrides::new(), "v1");
       put(&mut pipeline, "main.c", "main.c, edited", 200);

       let plan = planned(&mut pipeline);

       assert_eq!(
           said(&plan, "//:main.o"),
           Said::Dirty(changed(vec![Difference::Input(path("main.c"))]), Vec::new())
       );
       assert_eq!(
           said(&plan, "//:app"),
           Said::Conditional(vec![label("//:main.o")])
       );
       assert_eq!(
           said(&plan, "//:app_test"),
           Said::Conditional(vec![label("//:app")])
       );
       assert_eq!(said(&plan, "//lib:text"), Said::Clean);
       let json = json::canonical::to_string(&plan).unwrap();
       assert_eq!(json, GOLDEN.trim_end());
   }
   ```

5. Create `crates/buildl-core/tests/flows/golden/plan.json` holding an empty object, so the flow builds and fails:

   ```json
   {}
   ```

6. Run the flow and confirm it fails, the plan on the left and the empty object on the right:

   ```
   $ cargo test -p buildl-core --test flows plan::a_built_workspace
   test plan::a_built_workspace_with_one_edit_plans_to_the_pinned_json ... FAILED
   assertion `left == right` failed
    right: "{}"
   ```

7. Replace the content of `crates/buildl-core/tests/flows/golden/plan.json` with the serialized plan, on one line:

   ```json
   {"targets":{"//:app":{"key":null,"reason":null,"unfinished":{"awaiting":["//:main.o"],"own":{"command":["cc","$deps","-o","$out"],"env":{},"inputs":[],"network":"Sealed","outputs":["app"],"platform":"linux/x86_64","settings":{},"tool":{"fingerprint":"12d03249b5f2b1c97fb30b2f2e8faf27b7ecd84fadeb0a7fe107a52ad35a63aa","program":"cc"}}},"verdict":"Conditional"},"//:app_test":{"key":null,"reason":null,"unfinished":{"awaiting":["//:app"],"own":{"command":["$out/app","--filter=$opt:test_filter"],"env":{},"inputs":[],"network":"Sealed","outputs":["app_test"],"platform":"linux/x86_64","settings":{"test_filter":""},"tool":null}},"verdict":"Conditional"},"//:main.o":{"key":{"components":{"deps":[],"own":{"command":["cc","-c","$in","-o","$out"],"env":{},"inputs":[{"digest":"576a27338998ee787ae6ec7f55964756d93fff3af52e82e9055a3331abe7a86b","path":"main.c"}],"network":"Sealed","outputs":["main.o"],"platform":"linux/x86_64","settings":{},"tool":{"fingerprint":"12d03249b5f2b1c97fb30b2f2e8faf27b7ecd84fadeb0a7fe107a52ad35a63aa","program":"cc"}}},"digest":"c2d61443575d1e6e66ab478e123cc51f7c7fb14e360d779b45880c1983eda99e"},"reason":{"Changed":[{"Input":"main.c"}]},"unfinished":null,"verdict":"Dirty"},"//lib:text":{"key":{"components":{"deps":[],"own":{"command":["cc"],"env":{},"inputs":[{"digest":"3af219d436f20c866267e88c10515c86aa80160c0193b9381e1ca145611136aa","path":"lib/text.c"}],"network":"Sealed","outputs":["text"],"platform":"linux/x86_64","settings":{},"tool":{"fingerprint":"12d03249b5f2b1c97fb30b2f2e8faf27b7ecd84fadeb0a7fe107a52ad35a63aa","program":"cc"}}},"digest":"904d61e88efb81acaab476b70ef0efbdd7f5c2c5d7764f825d4c1d242c2c2f06"},"reason":null,"unfinished":null,"verdict":"Clean"}},"wanted":{"env":[],"network":[],"programs":["cc"],"write":[".buildl","out"]}}
   ```

8. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows plan
   test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

10. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/golden/plan.json crates/buildl-core/tests/flows/plan.rs crates/buildl-core/tests/flows/graph.rs
   $ git commit -m "test(buildl-core): pin the serialized plan of a fixture workspace"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`; `crates/buildl-core/Cargo.toml` is unchanged.
- Every row of spec §9's flow table has a passing flow: a workspace never built; a build after
  which nothing is hashed or written; a changed input; early cutoff, both ways; a file touched
  without changing; an override; an environment value, a tool and the platform; a target that
  always runs; a refused program with no port asked; a refused variable, input and directory; a
  missing tool and a missing input; one lookup per program; the lower of two unreportable
  variables; several faults in their fixed order; an unknown override; the golden plan; a Load
  error passed through unchanged.
- `crates/expected-module-edges.txt` holds 20 lines: the 16 it had before this chain, and
  `pipeline -> plan`, `plan -> error`, `plan -> ports`, `plan -> types`. It holds no edge between
  `plan` and `load` or `resolve`.
- No `expect(dead_code)` remains under `crates/buildl-core/src/plan/` or on `Plan::new`.

## Review findings

Two reviewer passes, 2026-10-11, one at each checkpoint the plan marks: task 1 alone, then tasks 2 and 3. Both verdicts: compliant with the plan and the spec, blocking set empty. The reviewer's own gate runs: after task 1, `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 5.29 seconds.`) and `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`); after task 3, `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 5.63 seconds.`) and `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`).

Task 1:

- reversion guard (risk) — nothing pins that `Pipeline::plan` runs Load once: with a second `load` added, every flow still passes. One `assert_eq!(pipeline.ports().source.evaluated(), [dir("")])` in a flow would pin it (spec §6) — `pipeline/driver.rs`, `Pipeline::plan`
- reversion guard (risk) — nothing pins that `Host` is not asked before the ceiling check: with `platform` and `environment` moved above `check`, all plan flows pass. A later edit could read a variable the workspace forbids before refusing the plan, and no test would notice. Putting `AWS_SECRET` in `host.unreportable` in the refused-variable flow would pin it (spec §5.1) — `plan/assembly.rs`, `plan`
- unit-test-mandate (risk) — `plan/assembly.rs` has no test module, on a "covered by the flows" note, and the three functions that call a port (`fingerprints`, `digests`, `environment`) have no colocated test. Spec §8 and §9 and the crate's fifth module rule put their tests in the flows, which is where task 2 pins them; the guideline lists both shapes as rejected, and the choice between the two is the author's — `plan/assembly.rs`, `plan/tools.rs`, `plan/inputs.rs`, `plan/components.rs`
- doc accuracy (nit) — the `# Errors` sections of `plan` and `Pipeline::plan` list neither `Error::CanonicalJson` nor the `Error::Adapter` that Plan raises itself for a digester reply of the wrong length — `plan/assembly.rs`, `pipeline/driver.rs`
- reversion guard (nit) — manifest-before-overrides is not pinned: swapping the two lines passes every flow, because the fake manifest cannot fail — `plan/assembly.rs`, `plan`
- doc accuracy (nit) — "implement every port the same way": the crate exports eight port traits and the fakes implement seven; `Clock` has none — `tests/flows/main.rs:5`
- spec drift (nit) — spec §8's row for `plan/components.rs` says steps 6 and 7; half of step 6, `ports.host.platform()`, is called from `plan/assembly.rs`, as task 1 step 10 prescribes

Tasks 2 and 3:

- reversion guard (nit) — the always-runs flow plans twice over identical port state, so its second round re-asserts the first — `tests/flows/plan.rs`, `a_target_that_always_runs_is_dirty_on_every_plan_and_its_dependent_conditional`
- reversion guard (nit) — "unchanged" is asserted for `provenance` and `failure`; `diagnostic` is skipped by `..` — `tests/flows/plan.rs`, `a_build_file_that_fails_to_evaluate_is_the_load_error_unchanged`
- spec accuracy (note) — spec §3.2 and §3.4 say the golden file pins shapes it does not hold: a non-empty `deps`, a non-null `env` value, a dirty verdict with an unfinished key, an `Always` or `NeverBuilt` reason. Unit tests pin them; the spec sentence overstates the file

No finding was fixed: none is in the blocking set. The two reversion guards of task 1 were left open, because tasks 2 and 3 were delivered as the plan gives them.

## Probe results

- **Nothing is asked before the ceiling check passes.** The reviewer's probe of a refused plan on a scratch copy: `stat=0 digest=0 lookups=[] commits=0 entries=0`. Not against the plan.
- **The stat cache spares the digester.** Three plans in a row: `digest_calls=1 digested=4 commits=1` throughout; a missing input after an edit leaves both counts unchanged. Not against the plan.
- **`Pipeline::plan` loads once.** `evaluated after one plan: [Directory("")]`. Not against the plan; the behaviour holds and is not pinned by a flow, as the first finding says.
- **Each flow of tasks 2 and 3 fails when the behaviour it names is broken.** Seven mutations on a scratch copy, seven caught; baseline `19 passed; 0 failed`. Among them the plan's own task 2 step 4: with the overrides checked after the ceiling, `several_faults` fails with `build.lua: //:image: program docker is outside the ceiling (proc.run allows: cc)`.
- **The golden file is what the fixture serializes to.** Re-derived independently with Python's `hashlib`: the tool fingerprint is `sha256("cc 14.2")`, `main.c`'s digest is that of its edited content, both finished keys' digests equal the SHA-256 of their canonical components, and the file is canonical. Not against the plan.
- **The plan-level verification summary holds.** `crates/expected-module-edges.txt` holds 20 lines with no edge between `plan` and `load` or `resolve`; `grep -rn dead_code crates/buildl-core/src/` prints nothing; `crates/buildl-core/Cargo.toml`, the root `Cargo.toml` and `Cargo.lock` are unchanged; spec §9's 17 flow rows are covered by 19 flows.
- **The three tasks leave the tree the plan was written from.** `diff -rq <prototype snapshot after plan 10>/crates crates` names only `error.rs`, `ports/port_set.rs` and `types/action_key.rs`, the three files plans `02`, `05` and `06` record as departing from their text. Not against the plan.
- The red steps were run before the green ones: task 1, `cargo test -p buildl-core --test flows plan` → exit `101`, `E0599` no method named `plan`, then `6 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out`; task 2, `18 passed; 0 failed`, and during its temporary change `0 passed; 1 failed`, with `plan/assembly.rs` afterwards byte-identical to the copy taken before (`cmp` exit `0`); task 3, `cargo test -p buildl-core --test flows plan::a_built_workspace` → exit `101`, `right: "{}"`, then `19 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out`.

## Deviations

- 2026-10-11 — the commit steps were not run. The work was left in the working tree, uncommitted, because committing is the author's act in this run. Nothing else departed from the plan.
- 2026-10-11 — after the pull request's first CI run, one test assertion departs from the plan's text: `assert!(ports.tools.lookups().is_empty())` in `tests/flows/plan.rs` became `assert_eq!(ports.tools.lookups(), [])`. Rust 1.99, which the pull request's CI installed while the plan was written and run on 1.98, adds clippy's `assert_is_empty`, and the gate failed on it there. The assertions check the same thing and now show the value when they fail. `cargo make dod` on 1.99 after the change: exit `0`, `[cargo-make] INFO - Build Done in 29.17 seconds.`
