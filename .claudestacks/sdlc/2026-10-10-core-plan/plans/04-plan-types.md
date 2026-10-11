---
status: done
created: 2026-10-11
depends-on: [02, 03]
---

# Plan Types Implementation Plan

**Goal:** A plan of one verdict per target serializes keyed by label.

**Architecture:** One new file, `types/plan.rs`. `Verdict` has three cases: `Clean`, with the key that matched; `Dirty`, with a `DirtyReason` and a `PlannedKey`; and `Conditional`, for a target that runs only if a dependency it waits on produces something new. A `PlannedKey` is `Finished` when every dependency is clean and `Unfinished` otherwise: an unfinished key holds the target's own components and the ids of the dependencies it awaits. `DirtyReason::Changed` lists `Difference`s, which name what differs and carry no old or new value. `Plan` keeps one label and one verdict per node, in id order, with the wanted set; its fields are private and its constructor is crate-private. It implements `Serialize` by hand through three private view structs, so the output is keyed by label and holds no `NodeId`. `Difference` and `DirtyReason` derive `Serialize`: serde's default enum form is exactly the required shape.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D1, D8, D9), §3.4 (verdicts and the plan), §9 (the `types/plan.rs` row).

**Checkpoints:** review once, when the task is done.

---

## Context an implementer needs

What plans `02` and `03` leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `ActionKey` (`Serialize`, `Clone`, `Eq`), `OwnComponents`, `KeyComponents` | `crates/buildl-core/src/types/action_key.rs` | what a verdict holds |
| `WantedSet` (`Serialize`) | `crates/buildl-core/src/types/authority.rs` | what a plan reports beside its verdicts |
| `NodeId` (`index()`), `Label` | `crates/buildl-core/src/types/` | how a verdict is addressed, and how it is written out |
| `json::canonical::to_string` | `crates/buildl-core/src/json/canonical.rs` | the serializer the tests pin the shape through |

Gate facts that bite here. Each reports something seen as a failing run while this code was
prototyped:

- Nothing outside this file's tests calls `Plan::new` until plan `10`, so the library build
  reports it as dead code and the gate fails. The function therefore carries
  `cfg_attr(not(test), expect(dead_code, …))`. It must be `cfg_attr(not(test), …)`: in the test
  build the function is used, and a plain `expect` would fail there as unfulfilled. Plan `10`
  removes the attribute.
- `Plan`'s doc comment cannot link the function that makes a plan, which does not exist yet: an
  unresolved link fails the gate's rustdoc step. Plan `10` adds the sentence.
- No variant is boxed. The spec's probe PE measured `Verdict` at 288 bytes and found no clippy
  size lint over these shapes.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/types/plan.rs   — [create] Difference, DirtyReason, Unfinished, PlannedKey, Verdict, Plan, unit tests
crates/buildl-core/src/types/mod.rs    — [modify] declare and re-export the module
crates/buildl-core/src/lib.rs          — [modify] re-export the six types
```

### Task 1 — Add the plan and its verdicts

**Files:**
- Create `crates/buildl-core/src/types/plan.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Declare the module. In `crates/buildl-core/src/types/mod.rs`, add `pub mod plan;` to the `pub mod` lines and `pub use plan::{Difference, DirtyReason, Plan, PlannedKey, Unfinished, Verdict};` to the `pub use` lines, and add to the module doc, directly below the `Workspace` bullet:

   ```rust
   //! - [`Plan`], [`Verdict`] and their parts — what a build would do with each target, and why.
   ```

2. Re-export them. In `crates/buildl-core/src/lib.rs`, add `Difference`, `DirtyReason`, `Plan`, `PlannedKey`, `Unfinished`, `Verdict` to the `pub use types::{…}` list.

3. Write the failing tests. Create `crates/buildl-core/src/types/plan.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::{BTreeMap, BTreeSet};

       use super::{Difference, DirtyReason, Plan, PlannedKey, Unfinished, Verdict};
       use crate::json::canonical;
       use crate::types::{
           ActionKey, Argument, Command, Directory, EnvName, KeyComponents, Label, NetworkAccess,
           NodeId, OutputName, OwnComponents, Platform, Program, SettingName, SourcePath, WantedSet,
       };

       fn label(raw: &str) -> Label {
           Label::parse(raw).unwrap()
       }

       /// The known components of a target producing one output, `output`.
       fn own(output: &str) -> OwnComponents {
           OwnComponents {
               command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
               inputs: Vec::new(),
               outputs: vec![OutputName::parse(output).unwrap()],
               env: BTreeMap::new(),
               tool: None,
               settings: BTreeMap::new(),
               network: NetworkAccess::Sealed,
               platform: Platform::parse("linux/x86_64").unwrap(),
           }
       }

       fn key(output: &str) -> ActionKey {
           ActionKey::new(KeyComponents {
               own: own(output),
               deps: Vec::new(),
           })
           .unwrap()
       }

       fn wanted() -> WantedSet {
           WantedSet {
               programs: BTreeSet::from([Program::parse("cc").unwrap()]),
               env: BTreeSet::from([EnvName::parse("HOME").unwrap()]),
               write: BTreeSet::from([
                   Directory::parse("out").unwrap(),
                   Directory::parse(".buildl").unwrap(),
               ]),
               network: BTreeSet::from([label("//:e")]),
           }
       }

       /// Five targets, `//:a` to `//:e`: clean, dirty for each of the three reasons — the last
       /// with an unfinished key — and conditional.
       fn plan() -> Plan {
           let labels = ["//:a", "//:b", "//:c", "//:d", "//:e"].map(label).to_vec();
           let verdicts = vec![
               Verdict::Clean { key: key("a") },
               Verdict::Dirty {
                   reason: DirtyReason::Always,
                   key: PlannedKey::Finished(key("b")),
               },
               Verdict::Dirty {
                   reason: DirtyReason::NeverBuilt,
                   key: PlannedKey::Finished(key("c")),
               },
               Verdict::Dirty {
                   reason: DirtyReason::Changed(vec![
                       Difference::Command,
                       Difference::Input(SourcePath::parse("src/main.c").unwrap()),
                   ]),
                   key: PlannedKey::Unfinished(Unfinished {
                       own: own("d"),
                       awaiting: vec![NodeId::new(2), NodeId::new(1)],
                   }),
               },
               Verdict::Conditional(Unfinished {
                   own: own("e"),
                   awaiting: vec![NodeId::new(3)],
               }),
           ];
           Plan::new(labels, verdicts, wanted())
       }

       #[test]
       fn a_plan_answers_by_id_and_by_position() {
           let plan = plan();
           assert_eq!(plan.len(), 5);
           assert!(!plan.is_empty());
           assert_eq!(plan.label(NodeId::new(3)), Some(&label("//:d")));
           assert_eq!(
               plan.verdict(NodeId::new(0)),
               Some(&Verdict::Clean { key: key("a") })
           );
           assert_eq!(plan.verdicts().len(), 5);
           assert_eq!(plan.wanted(), &wanted());
       }

       #[test]
       fn an_id_the_plans_graph_did_not_issue_answers_none() {
           let plan = plan();
           assert!(plan.label(NodeId::new(5)).is_none());
           assert!(plan.verdict(NodeId::new(5)).is_none());
       }

       #[test]
       fn an_empty_plan_holds_nothing() {
           let empty = WantedSet {
               programs: BTreeSet::new(),
               env: BTreeSet::new(),
               write: BTreeSet::new(),
               network: BTreeSet::new(),
           };
           let plan = Plan::new(Vec::new(), Vec::new(), empty);
           assert!(plan.is_empty());
           assert_eq!(plan.len(), 0);
           assert_eq!(
               canonical::to_string(&plan).unwrap(),
               r#"{"targets":{},"wanted":{"env":[],"network":[],"programs":[],"write":[]}}"#
           );
       }

       #[test]
       fn every_difference_serializes_as_its_name_or_as_one_key() {
           let cases = [
               (Difference::Command, r#""Command""#),
               (
                   Difference::Input(SourcePath::parse("src/main.c").unwrap()),
                   r#"{"Input":"src/main.c"}"#,
               ),
               (Difference::InputOrder, r#""InputOrder""#),
               (Difference::Outputs, r#""Outputs""#),
               (
                   Difference::Env(EnvName::parse("HOME").unwrap()),
                   r#"{"Env":"HOME"}"#,
               ),
               (Difference::Tool, r#""Tool""#),
               (
                   Difference::Setting(SettingName::parse("test_filter").unwrap()),
                   r#"{"Setting":"test_filter"}"#,
               ),
               (Difference::Network, r#""Network""#),
               (Difference::Platform, r#""Platform""#),
               (
                   Difference::Dep(label("//lib:text")),
                   r#"{"Dep":"//lib:text"}"#,
               ),
               (Difference::DepOrder, r#""DepOrder""#),
           ];
           for (difference, json) in cases {
               assert_eq!(canonical::to_string(&difference).unwrap(), json);
           }
       }

       #[test]
       fn every_reason_serializes_as_its_name_or_as_its_differences() {
           let changed = DirtyReason::Changed(vec![Difference::Tool, Difference::DepOrder]);
           assert_eq!(
               canonical::to_string(&DirtyReason::Always).unwrap(),
               r#""Always""#
           );
           assert_eq!(
               canonical::to_string(&DirtyReason::NeverBuilt).unwrap(),
               r#""NeverBuilt""#
           );
           assert_eq!(
               canonical::to_string(&changed).unwrap(),
               r#"{"Changed":["Tool","DepOrder"]}"#
           );
       }

       #[test]
       fn a_plan_serializes_keyed_by_label_with_awaited_targets_as_labels() {
           let json = canonical::to_string(&plan()).unwrap();
           let key_of = |output: &str| canonical::to_string(&key(output)).unwrap();
           let own_of = |output: &str| canonical::to_string(&own(output)).unwrap();
           let expected = [
               r#"{"targets":{"#,
               r#""//:a":{"key":"#,
               &key_of("a"),
               r#","reason":null,"unfinished":null,"verdict":"Clean"},"#,
               r#""//:b":{"key":"#,
               &key_of("b"),
               r#","reason":"Always","unfinished":null,"verdict":"Dirty"},"#,
               r#""//:c":{"key":"#,
               &key_of("c"),
               r#","reason":"NeverBuilt","unfinished":null,"verdict":"Dirty"},"#,
               r#""//:d":{"key":null,"reason":{"Changed":["Command",{"Input":"src/main.c"}]},"#,
               r#""unfinished":{"awaiting":["//:c","//:b"],"own":"#,
               &own_of("d"),
               r#"},"verdict":"Dirty"},"#,
               r#""//:e":{"key":null,"reason":null,"unfinished":{"awaiting":["//:d"],"own":"#,
               &own_of("e"),
               r#"},"verdict":"Conditional"}},"#,
               r#""wanted":{"env":["HOME"],"network":["//:e"],"programs":["cc"],"#,
               r#""write":[".buildl","out"]}}"#,
           ]
           .concat();
           assert_eq!(json, expected);
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib types::plan
   error[E0432]: unresolved imports `super::Difference`, `super::DirtyReason`, `super::Plan`, `super::PlannedKey`, `super::Unfinished`, `super::Verdict`
   error[E0432]: unresolved imports `plan::Difference`, `plan::DirtyReason`, `plan::Plan`, `plan::PlannedKey`, `plan::Unfinished`, `plan::Verdict`
   ```

5. Replace the placeholder line at the top of `crates/buildl-core/src/types/plan.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The plan: what a build of a workspace would do, target by target, and why.
   //!
   //! Its own file because a plan is a different value from the graph it is made from: the graph
   //! says what each target is, a plan says whether each one needs to run, and it is the value the
   //! `plan` command prints and the executor later walks.
   //!
   //! Responsibilities: [`Plan`], the [`Verdict`] it holds for each target, the parts of a verdict
   //! — [`DirtyReason`], [`Difference`], [`PlannedKey`], [`Unfinished`] — and a plan's serialized
   //! form.
   //!
   //! Non-responsibilities: reaching a verdict. Comparing a key with the one a previous build
   //! recorded belongs to the phase that plans; this file stores what it decided.

   use std::collections::BTreeMap;

   use serde::{Serialize, Serializer};

   use crate::types::{
       ActionKey, EnvName, Label, NodeId, OwnComponents, SettingName, SourcePath, WantedSet,
   };

   /// One way a target's key differs from the key its last build recorded.
   ///
   /// A difference names what changed, never the old or the new value: both are in the two keys.
   /// Non-exhaustive: a key that gains a component gains a difference.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   #[non_exhaustive]
   pub enum Difference {
       /// The command.
       Command,
       /// An input that was added, removed, or whose content changed.
       Input(SourcePath),
       /// The same inputs, written in another order.
       InputOrder,
       /// The declared outputs.
       Outputs,
       /// An environment variable that was added, removed, or whose value changed.
       Env(EnvName),
       /// The program the command names, or its fingerprint.
       Tool,
       /// A build setting that was added, removed, or whose value changed.
       Setting(SettingName),
       /// Whether the action may reach the network.
       Network,
       /// The platform.
       Platform,
       /// A dependency that was added, removed, or that produced something else.
       Dep(Label),
       /// The same dependencies, written in another order.
       DepOrder,
   }

   /// Why a target will run.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub enum DirtyReason {
       /// The target is declared to run on every build.
       Always,
       /// No previous build recorded a result for it.
       NeverBuilt,
       /// Its key differs from the recorded one in these ways, in a fixed order.
       Changed(Vec<Difference>),
   }

   /// A key that cannot be finished before the build: some dependency has yet to produce its
   /// outputs.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct Unfinished {
       /// The parts of the key that are already known.
       pub own: OwnComponents,
       /// The dependencies whose outputs are not known yet, in the order they were written.
       pub awaiting: Vec<NodeId>,
   }

   /// The key of a target that will run: whole, or waiting on its dependencies.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum PlannedKey {
       /// Every dependency is clean, so the key is complete.
       Finished(ActionKey),
       /// Some dependency is not clean, so the key waits for what it produces.
       Unfinished(Unfinished),
   }

   /// What a build would do with one target.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Verdict {
       /// Nothing: its key equals the one its last build recorded.
       Clean {
           /// The key.
           key: ActionKey,
       },
       /// Run it.
       Dirty {
           /// Why.
           reason: DirtyReason,
           /// Its key, as far as it is known before the build.
           key: PlannedKey,
       },
       /// Run it only if a dependency it waits on produces something other than what the last
       /// build recorded; nothing else about it changed.
       Conditional(Unfinished),
   }

   /// What a build of a workspace would do: one verdict per target, and what the build wants.
   ///
   /// Its verdicts are addressed by the [`NodeId`]s of the graph it was made from.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct Plan {
       labels: Vec<Label>,
       verdicts: Vec<Verdict>,
       wanted: WantedSet,
   }

   impl Plan {
       /// Assembles a plan from one label and one verdict per node, both in id order.
       #[cfg_attr(
           not(test),
           expect(
               dead_code,
               reason = "called by the phase that plans, which is not written yet"
           )
       )]
       pub(crate) const fn new(labels: Vec<Label>, verdicts: Vec<Verdict>, wanted: WantedSet) -> Self {
           Self {
               labels,
               verdicts,
               wanted,
           }
       }

       /// The number of targets.
       #[must_use]
       pub const fn len(&self) -> usize {
           self.verdicts.len()
       }

       /// Whether the plan holds no target.
       #[must_use]
       pub const fn is_empty(&self) -> bool {
           self.verdicts.is_empty()
       }

       /// The label of the target `id` names; `None` when the plan's graph did not issue `id`.
       #[must_use]
       pub fn label(&self, id: NodeId) -> Option<&Label> {
           self.labels.get(id.index())
       }

       /// The verdict for the target `id` names; `None` when the plan's graph did not issue `id`.
       #[must_use]
       pub fn verdict(&self, id: NodeId) -> Option<&Verdict> {
           self.verdicts.get(id.index())
       }

       /// Every verdict, in id order.
       #[must_use]
       pub fn verdicts(&self) -> &[Verdict] {
           &self.verdicts
       }

       /// What the build wants.
       #[must_use]
       pub const fn wanted(&self) -> &WantedSet {
           &self.wanted
       }
   }

   /// An unfinished key as it is written to JSON: the dependencies it waits on as labels.
   #[derive(Serialize)]
   struct UnfinishedView<'a> {
       awaiting: Vec<&'a Label>,
       own: &'a OwnComponents,
   }

   /// One target's verdict as it is written to JSON.
   #[derive(Serialize)]
   struct TargetView<'a> {
       key: Option<&'a ActionKey>,
       reason: Option<&'a DirtyReason>,
       unfinished: Option<UnfinishedView<'a>>,
       verdict: &'static str,
   }

   /// A plan as it is written to JSON: every target under its label, no id anywhere.
   #[derive(Serialize)]
   struct PlanView<'a> {
       targets: BTreeMap<&'a Label, TargetView<'a>>,
       wanted: &'a WantedSet,
   }

   impl Plan {
       /// `unfinished` with the ids it waits on written as labels.
       fn unfinished_view<'a>(&'a self, unfinished: &'a Unfinished) -> UnfinishedView<'a> {
           UnfinishedView {
               awaiting: unfinished
                   .awaiting
                   .iter()
                   .filter_map(|id| self.label(*id))
                   .collect(),
               own: &unfinished.own,
           }
       }

       /// `verdict` as it is written to JSON.
       fn target_view<'a>(&'a self, verdict: &'a Verdict) -> TargetView<'a> {
           match verdict {
               Verdict::Clean { key } => TargetView {
                   key: Some(key),
                   reason: None,
                   unfinished: None,
                   verdict: "Clean",
               },
               Verdict::Dirty { reason, key } => {
                   let (key, unfinished) = match key {
                       PlannedKey::Finished(key) => (Some(key), None),
                       PlannedKey::Unfinished(unfinished) => {
                           (None, Some(self.unfinished_view(unfinished)))
                       }
                   };
                   TargetView {
                       key,
                       reason: Some(reason),
                       unfinished,
                       verdict: "Dirty",
                   }
               }
               Verdict::Conditional(unfinished) => TargetView {
                   key: None,
                   reason: None,
                   unfinished: Some(self.unfinished_view(unfinished)),
                   verdict: "Conditional",
               },
           }
       }
   }

   /// Serializes keyed by label, so that a plan can be read without the graph it was made from.
   ///
   /// Ids are positions in the graph's arena and mean nothing outside it; labels do. There is no
   /// `Deserialize`: a plan is made again on every run, never read back.
   impl Serialize for Plan {
       fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
           let targets = self
               .labels
               .iter()
               .zip(&self.verdicts)
               .map(|(label, verdict)| (label, self.target_view(verdict)))
               .collect();
           PlanView {
               targets,
               wanted: &self.wanted,
           }
           .serialize(serializer)
       }
   }
   ```

6. Run `cargo fmt --all`. It puts the lines and names added to the module index and to `lib.rs` in sorted order, which it could not do before the new file existed: rustfmt refuses a crate that declares a module with no file.

7. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib types::plan
   test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 243 filtered out
   ```

8. After `cargo fmt --all`, the list in `crates/buildl-core/src/lib.rs` reads:

   ```rust
   pub use types::{
       Action, ActionKey, Alias, Argument, BuildFile, CacheRow, Ceiling, Command, Declaration,
       DeclarationOrder, Declared, DepProducts, Description, Diagnostic, Difference, Digest,
       Directory, DirtyReason, EntryName, EnvName, EnvValue, Evaluated, FieldName, FileStat,
       Freshness, InputDigest, KeyComponents, Label, NetworkAccess, Node, NodeId, OutputName,
       OwnComponents, Plan, PlannedKey, Platform, Produced, Program, Provenance, Rule, Setting,
       SettingName, SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
       StagedRule, StagedSetting, StagedSubdir, StagedTarget, StatEntry, Target, TargetGraph,
       TargetName, TargetRole, Timestamp, ToolIdentity, Unfinished, Verdict, Want, WantedSet,
       Workspace, Written,
   };
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
   $ git add crates/buildl-core/src/types/plan.rs crates/buildl-core/src/types/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the plan and its verdicts"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- A plan holding a clean target, a dirty one for each of the three reasons and a conditional one
  serializes to exact, pinned JSON: `targets` keyed by label, each with `key`, `reason`,
  `unfinished` and `verdict`, exactly one of `key` and `unfinished` non-null, awaited targets
  written as labels; and `wanted` with `env`, `network`, `programs` and `write`.
- Each of the eleven differences serializes as its name, or as an object of one key for the four
  that name something.
- `Plan` answers `label` and `verdict` by id, and `None` for an id past its end.
- `Plan` implements `Serialize` and not `Deserialize`.

## Review findings

One reviewer pass over the plan's whole diff, 2026-10-11. Verdict: compliant with the plan, blocking set empty. The reviewer's own gate run: `cargo make dod` exit `0` (`[cargo-make] INFO - Build Done in 5.06 seconds.`), `cargo deny check` exit `0` (`advisories ok, bans ok, licenses ok, sources ok`).

- strong-types (risk, not blocking) — `Plan::new` takes `labels` and `verdicts` unchecked, so "one label and one verdict per node" rests on the caller; on a mismatch `len()` counts verdicts, `label()` indexes labels and serialization truncates silently. Unreachable today: the constructor is `pub(crate)` and only tests call it. Becomes live when plan `10` adds the first caller — `types/plan.rs:122`
- correctness (risk, not blocking) — `unfinished_view` drops an awaited id that names no node of the plan, so a written plan would under-report what a target waits on; no test covers that arm. Unreachable today for the same reason — `types/plan.rs:197`
- doc accuracy (nit) — the module doc names "the `plan` command" and "the executor", neither of which exists in the workspace yet; follows the crate's precedent — `types/plan.rs:5`
- doc discipline (nit) — the `expect` reason "which is not written yet" narrates future work; it is true and expires by itself once a caller lands — `types/plan.rs:119`
- reversion-guard (nit) — nothing fails if a `Deserialize` is added to `Plan`, which the plan's summary rules out — `types/plan.rs:240`

No finding was fixed: none is in the blocking set.

## Probe results

No throwaway test was written. Every claim the task makes about existing code was settled by its own red-green cycle, which printed the failure and the pass the plan quotes.

- The task leaves the tree the plan was written from — `diff -r crates <prototype snapshot after plan 04>/crates` — no output, exit `0`. Not against the plan.

## Deviations

- 2026-10-11 — the commit step was not run. The work was left in the working tree, uncommitted, because committing is the author's act in this run. Nothing else departed from the plan.
