---
status: done
created: 2026-10-11
depends-on: [01, 06]
---

# Schedule Implementation Plan

**Goal:** The state of every target of a build is kept in one schedule that answers what each finish changes.

**Architecture:** One logic file, `execute/schedule.rs`, over plain values. `Schedule` keeps one state per target and one count per target of the dependencies that have not succeeded yet. `start` hands out every target that waits on nothing. `complete` is told how a started target finished and answers with `Effects`: the targets that may start now, and the targets that never will. What a failure does to the rest is the build's policy, `OnFailure`: under `Stop` the build stops at its first failure, remembers it, and every target still waiting is skipped; under `KeepGoing` only the targets that depend on the failure, directly or through others, are skipped, found with an explicit stack and never by recursion. `abandon` turns a target that was started, and that nobody began building before the build stopped, into a skipped one. The schedule asks no port and its caller is its only writer, so the counts are plain integers. Nothing calls it yet.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D13), §5.5 (the schedule), §8 (the module layout), §9 (the row for `execute/schedule.rs`).

**Checkpoints:** review once, when both tasks are done.

---

## Context an implementer needs

What plans `01` and `06` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `OnFailure { Stop, KeepGoing }` (`Copy`) | `crates/buildl-core/src/types/on_failure.rs:15` | the policy a schedule is built with |
| `TargetGraph::len()` | `crates/buildl-core/src/types/target_graph.rs:105` | how many states and counts a schedule holds |
| `TargetGraph::deps(id) -> Option<&[NodeId]>`: the nodes `id` depends on; "the length of the list is the node's dependency count"; no list repeats an id | `crates/buildl-core/src/types/target_graph.rs:132` | each target's starting count |
| `TargetGraph::dependents(id) -> Option<&[NodeId]>`: the nodes that depend on `id`, in id order | `crates/buildl-core/src/types/target_graph.rs:138` | who is released by a success, who is skipped after a failure |
| `NodeId` (`Copy`, `Ord`; `NodeId::new(u32)`, `index() -> usize`) | `crates/buildl-core/src/types/node_id.rs` | how targets are named everywhere here |
| `execute/mod.rs` declaring `admission`, `expand` and `layout`, each under a `cfg_attr(not(test), expect(dead_code, …))`, then `#[cfg(test)] mod fixtures;` | `crates/buildl-core/src/execute/mod.rs` | the file task 1 adds one `mod` line to |
| `graph(Vec<Node>, &[&[u32]]) -> TargetGraph` and `node(label, change) -> Node` | `crates/buildl-core/src/execute/fixtures.rs` | the builders this plan's tests use; none is added |
| `execute -> error` and `execute -> types` | `crates/expected-module-edges.txt` | the edges `execute/` already has; this plan adds none |

Gate facts that bite here:

- Nothing outside the file's own tests calls the schedule until the function that runs a build is
  written, so the library build reports it as dead code and the gate fails. The `mod` line in
  `execute/mod.rs` therefore carries `cfg_attr(not(test), expect(dead_code, …))`. It must be
  `cfg_attr(not(test), …)`: in the test build the schedule is used, and a plain `expect` would
  fail there as unfulfilled. The plan that writes that function removes the attribute.
- `execute/schedule.rs` imports only `crate::types`, an edge `execute/` already has, so
  `crates/expected-module-edges.txt` does not change.
- clippy's `assert_is_empty` rejects `assert!(list.is_empty())` on a slice or a `Vec`, so the
  tests compare with `[]`.
- The test module unwraps nothing, so it carries no `expect(clippy::unwrap_used)`.
- The test over a chain of 100 000 targets builds its graph with the same fixtures as the others.
  The whole test module ran in 0.19 seconds while this code was prototyped.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. A failing run is quoted by the error lines that name the cause, not in full. The gate was run after every task and exited `0`. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/execute/schedule.rs   — [create] Schedule, Finish, Effects, unit tests
crates/buildl-core/src/execute/mod.rs        — [modify] declare the module
```

### Task 1 — Start what is ready, and decide what a failure does to the rest

**Files:**
- Create `crates/buildl-core/src/execute/schedule.rs`
- Modify `crates/buildl-core/src/execute/mod.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/execute/mod.rs` becomes:

   ```rust
   //! Execute, the fourth phase: from a plan to a finished build.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //! Recording what an action produced is the last step of running it, so it lives here too.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod admission;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod expand;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod layout;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that runs a build, which is not written yet"
       )
   )]
   mod schedule;

   #[cfg(test)]
   mod fixtures;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/execute/schedule.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use super::{Effects, Finish, Schedule};
       use crate::execute::fixtures::{graph, node};
       use crate::types::{NodeId, OnFailure, TargetGraph};

       fn ids(raw: &[u32]) -> Vec<NodeId> {
           raw.iter().copied().map(NodeId::new).collect()
       }

       fn effects(started: &[u32], skipped: &[u32]) -> Effects {
           Effects {
               started: ids(started),
               skipped: ids(skipped),
           }
       }

       /// `//:a` depends on `//:b` and `//:c`, and both of those on `//:d`.
       fn diamond() -> TargetGraph {
           graph(
               vec![
                   node("//:a", |_| {}),
                   node("//:b", |_| {}),
                   node("//:c", |_| {}),
                   node("//:d", |_| {}),
               ],
               &[&[1, 2], &[3], &[3], &[]],
           )
       }

       /// `//:c` depends on `//:b`, which depends on `//:a`; apart from them, `//:y` depends on
       /// `//:x`.
       fn two_chains() -> TargetGraph {
           graph(
               vec![
                   node("//:a", |_| {}),
                   node("//:b", |_| {}),
                   node("//:c", |_| {}),
                   node("//:x", |_| {}),
                   node("//:y", |_| {}),
               ],
               &[&[], &[0], &[1], &[], &[3]],
           )
       }

       #[test]
       fn an_empty_graph_starts_nothing() {
           let graph = graph(Vec::new(), &[]);
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), []);
           assert_eq!(schedule.stopped(), None);
       }

       #[test]
       fn a_target_starts_once_everything_it_depends_on_has_succeeded() {
           let graph = diamond();
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), ids(&[3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
               effects(&[1, 2], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(2), Finish::Succeeded),
               effects(&[], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(1), Finish::Succeeded),
               effects(&[0], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
               effects(&[], &[])
           );
           assert_eq!(schedule.stopped(), None);
       }

       #[test]
       fn nothing_starts_twice() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           assert_eq!(schedule.start(), []);
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
               effects(&[1], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
               effects(&[], &[]),
               "a target that already finished"
           );
           assert_eq!(schedule.start(), []);
       }

       #[test]
       fn a_target_that_was_not_started_cannot_finish() {
           let graph = diamond();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Failed),
               effects(&[], &[]),
               "before anything started"
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(9), Finish::Succeeded),
               effects(&[], &[]),
               "an id the graph did not issue"
           );
           assert_eq!(schedule.start(), ids(&[3]));
       }

       #[test]
       fn a_failure_stops_the_build_when_told_to_stop() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           assert_eq!(schedule.stopped(), None);
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Failed),
               effects(&[], &[1, 2, 4]),
               "every target still waiting is skipped, downstream of the failure or not"
           );
           assert_eq!(schedule.stopped(), Some(NodeId::new(0)));
       }

       #[test]
       fn a_success_after_the_build_stopped_starts_nothing() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Failed),
               effects(&[], &[1, 2, 4])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
               effects(&[], &[])
           );
           assert_eq!(schedule.start(), []);
       }

       #[test]
       fn a_stopped_build_remembers_its_first_failure() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Failed),
               effects(&[], &[1, 2, 4])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Failed),
               effects(&[], &[])
           );
           assert_eq!(schedule.stopped(), Some(NodeId::new(3)));
       }

       #[test]
       fn a_failure_skips_only_what_depends_on_it_when_told_to_keep_going() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Failed),
               effects(&[], &[1, 2]),
               "the target that depends on it, and the one that depends on that"
           );
           assert_eq!(schedule.stopped(), None);
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
               effects(&[4], &[]),
               "the rest of the build goes on"
           );
       }

       #[test]
       fn a_second_failure_skips_only_what_was_still_waiting() {
           let graph = diamond();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
               effects(&[1, 2], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(1), Finish::Failed),
               effects(&[], &[0])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(2), Finish::Failed),
               effects(&[], &[]),
               "the one target downstream was skipped by the first failure"
           );
           assert_eq!(schedule.stopped(), None);
       }

       #[test]
       fn a_skipped_target_does_not_start_when_its_other_dependency_succeeds() {
           let graph = diamond();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[3]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
               effects(&[1, 2], &[])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(1), Finish::Failed),
               effects(&[], &[0])
           );
           assert_eq!(
               schedule.complete(&graph, NodeId::new(2), Finish::Succeeded),
               effects(&[], &[])
           );
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::schedule
   error[E0432]: unresolved imports `super::Effects`, `super::Finish`, `super::Schedule`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/execute/schedule.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The schedule: which targets may start, and which never will.
   //!
   //! Its own file because it is the one place that knows how far a build has come: a target
   //! starts when everything it depends on has succeeded, and a failure decides what happens to
   //! the targets that have not started, by the policy the build was given.
   //!
   //! Responsibilities: [`Schedule`], [`Finish`] and [`Effects`].
   //!
   //! Non-responsibilities: running anything, and deciding whether a target succeeded. The
   //! schedule is told how each target finished and answers with what that changes; it asks no
   //! port. Its caller is its only writer, so the counts it keeps are plain integers.

   use crate::types::{NodeId, OnFailure, TargetGraph};

   /// How far one target has come.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   enum State {
       /// Waiting on a dependency.
       Pending,
       /// Handed to its caller to be built.
       Started,
       /// Built, or found already built.
       Succeeded,
       /// It could not be built.
       Failed,
       /// It will not be built: something it needed failed, or the build stopped first.
       Skipped,
   }

   /// How a started target finished.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) enum Finish {
       /// Its outputs are in place.
       Succeeded,
       /// It could not be built.
       Failed,
   }

   /// What one target finishing changed for the others.
   #[derive(Debug, Default, PartialEq, Eq)]
   pub(crate) struct Effects {
       /// The targets that may start now, in id order.
       pub(crate) started: Vec<NodeId>,
       /// The targets that will never start, in id order.
       pub(crate) skipped: Vec<NodeId>,
   }

   /// The state of every target of one build.
   ///
   /// A target is pending until everything it depends on has succeeded, then started, and from
   /// there it succeeds or fails. A pending target is skipped when something it depends on fails,
   /// or, under [`OnFailure::Stop`], when anything fails.
   #[derive(Debug)]
   pub(crate) struct Schedule {
       /// One state per node, in id order.
       states: Vec<State>,
       /// Per node, in id order: how many of its dependencies have not succeeded yet.
       waiting: Vec<usize>,
       /// What a failure does to the rest of the build.
       on_failure: OnFailure,
       /// The failure that stopped the build, once one has.
       stopped: Option<NodeId>,
   }

   impl Schedule {
       /// A schedule in which every target of `graph` is pending.
       pub(crate) fn new(graph: &TargetGraph, on_failure: OnFailure) -> Self {
           let waiting: Vec<usize> = (0_u32..)
               .take(graph.len())
               .map(|id| graph.deps(NodeId::new(id)).map_or(0, <[NodeId]>::len))
               .collect();
           Self {
               states: vec![State::Pending; waiting.len()],
               waiting,
               on_failure,
               stopped: None,
           }
       }

       /// Starts every pending target that waits on nothing, and returns them in id order.
       pub(crate) fn start(&mut self) -> Vec<NodeId> {
           let mut started = Vec::new();
           let ids = (0_u32..).map(NodeId::new);
           for ((state, count), id) in self.states.iter_mut().zip(&self.waiting).zip(ids) {
               if *state == State::Pending && *count == 0 {
                   *state = State::Started;
                   started.push(id);
               }
           }
           started
       }

       /// Records that the started target `node` finished, and returns what that changes.
       ///
       /// After a success, every target that waited only on `node` starts, unless the build has
       /// stopped. After a failure, under [`OnFailure::Stop`] the build stops and every pending
       /// target is skipped; under [`OnFailure::KeepGoing`] every pending target that depends on
       /// `node`, directly or through others, is skipped.
       ///
       /// A `node` that is not started changes nothing.
       pub(crate) fn complete(
           &mut self,
           graph: &TargetGraph,
           node: NodeId,
           finish: Finish,
       ) -> Effects {
           match self.states.get_mut(node.index()) {
               Some(state) if *state == State::Started => {
                   *state = match finish {
                       Finish::Succeeded => State::Succeeded,
                       Finish::Failed => State::Failed,
                   };
               }
               _ => return Effects::default(),
           }
           match (finish, self.on_failure) {
               (Finish::Succeeded, _) if self.stopped.is_some() => Effects::default(),
               (Finish::Succeeded, _) => Effects {
                   started: self.release(graph, node),
                   skipped: Vec::new(),
               },
               (Finish::Failed, OnFailure::Stop) => Effects {
                   started: Vec::new(),
                   skipped: self.stop(node),
               },
               (Finish::Failed, OnFailure::KeepGoing) => Effects {
                   started: Vec::new(),
                   skipped: self.skip_from(graph, node),
               },
           }
       }

       /// The failure that stopped the build, if it has stopped.
       ///
       /// A build stops only under [`OnFailure::Stop`], at its first failure.
       pub(crate) const fn stopped(&self) -> Option<NodeId> {
           self.stopped
       }

       /// Counts `node` as succeeded for each target that depends on it, and starts those left
       /// waiting on nothing. Returns them in id order.
       fn release(&mut self, graph: &TargetGraph, node: NodeId) -> Vec<NodeId> {
           let mut started = Vec::new();
           for dependent in graph.dependents(node).unwrap_or_default() {
               let index = dependent.index();
               let (Some(count), Some(state)) =
                   (self.waiting.get_mut(index), self.states.get_mut(index))
               else {
                   continue;
               };
               *count = count.saturating_sub(1);
               if *count == 0 && *state == State::Pending {
                   *state = State::Started;
                   started.push(*dependent);
               }
           }
           started.sort_unstable();
           started
       }

       /// Stops the build at `failed`, unless it has stopped already, and skips every pending
       /// target. Returns them in id order.
       fn stop(&mut self, failed: NodeId) -> Vec<NodeId> {
           if self.stopped.is_none() {
               self.stopped = Some(failed);
           }
           let mut skipped = Vec::new();
           let ids = (0_u32..).map(NodeId::new);
           for (state, id) in self.states.iter_mut().zip(ids) {
               if *state == State::Pending {
                   *state = State::Skipped;
                   skipped.push(id);
               }
           }
           skipped
       }

       /// Skips every pending target that depends on `failed`, directly or through others, and
       /// returns them in id order.
       ///
       /// Walks with an explicit stack, never by recursion, so a long dependency chain cannot
       /// exhaust the call stack. Only a target skipped here is walked through: one skipped
       /// earlier had everything behind it skipped then.
       fn skip_from(&mut self, graph: &TargetGraph, failed: NodeId) -> Vec<NodeId> {
           let mut skipped = Vec::new();
           let mut stack = vec![failed];
           while let Some(node) = stack.pop() {
               for dependent in graph.dependents(node).unwrap_or_default() {
                   if let Some(state) = self.states.get_mut(dependent.index())
                       && *state == State::Pending
                   {
                       *state = State::Skipped;
                       skipped.push(*dependent);
                       stack.push(*dependent);
                   }
               }
           }
           skipped.sort_unstable();
           skipped
       }
   }
   ```

   Only a started target can finish: `complete` for any other target, or for an id the graph did not issue, changes nothing and returns empty effects. That is what keeps a count from being lowered twice. A build stops only under `Stop`, and `stop` keeps the first failure, so a target that was already running when the build stopped and fails afterwards does not replace it. After a stop no target is pending any more, which is why the same sweep finds nothing the second time.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::schedule
   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 379 filtered out; finished in 0.00s
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree this plan leaves; no task of it changes a manifest.

7. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): schedule what starts and what a failure skips"
   ```

### Task 2 — Abandon a started target

**Files:**
- Modify `crates/buildl-core/src/execute/schedule.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl-core/src/execute/schedule.rs`, add these four tests at the end of the test module, after `a_skipped_target_does_not_start_when_its_other_dependency_succeeds` and before the module's closing brace:

   ```rust
       #[test]
       fn an_abandoned_target_is_skipped_and_releases_nothing() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           schedule.abandon(NodeId::new(0));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
               effects(&[], &[]),
               "an abandoned target is no longer started, so it cannot finish"
           );
           assert_eq!(schedule.start(), []);
       }

       #[test]
       fn only_a_started_target_can_be_abandoned() {
           let graph = two_chains();
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), ids(&[0, 3]));
           schedule.abandon(NodeId::new(1));
           schedule.abandon(NodeId::new(9));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
               effects(&[1], &[]),
               "the pending target was left pending"
           );
       }

       #[test]
       fn started_targets_are_abandoned_once_the_build_has_stopped() {
           let graph = graph(
               vec![
                   node("//:a", |_| {}),
                   node("//:b", |_| {}),
                   node("//:c", |_| {}),
               ],
               &[&[], &[], &[]],
           );
           let mut schedule = Schedule::new(&graph, OnFailure::Stop);
           assert_eq!(schedule.start(), ids(&[0, 1, 2]));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(0), Finish::Failed),
               effects(&[], &[]),
               "nothing was pending, so the schedule itself skips nothing"
           );
           schedule.abandon(NodeId::new(1));
           schedule.abandon(NodeId::new(2));
           assert_eq!(
               schedule.complete(&graph, NodeId::new(1), Finish::Failed),
               effects(&[], &[])
           );
           assert_eq!(schedule.stopped(), Some(NodeId::new(0)));
       }

       #[test]
       fn a_long_chain_is_skipped_without_recursion() {
           let length = 100_000_u32;
           let nodes = (0..length)
               .map(|index| node(&format!("//:n{index:06}"), |_| {}))
               .collect();
           // Each node depends on the next, so only the last one can start.
           let lists: Vec<Vec<u32>> = (0..length)
               .map(|index| {
                   if index + 1 < length {
                       vec![index + 1]
                   } else {
                       Vec::new()
                   }
               })
               .collect();
           let edges: Vec<&[u32]> = lists.iter().map(Vec::as_slice).collect();
           let graph = graph(nodes, &edges);
           let last = NodeId::new(length - 1);
           let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
           assert_eq!(schedule.start(), [last]);

           let effects = schedule.complete(&graph, last, Finish::Failed);
           assert_eq!(effects.started, []);
           assert_eq!(effects.skipped.len(), 99_999);
           assert_eq!(effects.skipped.first(), Some(&NodeId::new(0)));
           assert_eq!(effects.skipped.last(), Some(&NodeId::new(length - 2)));
           assert!(effects.skipped.windows(2).all(|pair| pair[0] < pair[1]));
       }
   ```

   The last of the four needs nothing new: it pins what task 1 already does, that the targets behind a failure are found with a stack and not by recursion. It is added here so that the whole row of tests for this file exists once the plan is done.

2. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib execute::schedule
   error[E0599]: no method named `abandon` found for struct `Schedule` in the current scope
   ```

3. Add the method. In `crates/buildl-core/src/execute/schedule.rs`, inside `impl Schedule`, directly above the doc comment of `stopped` (`/// The failure that stopped the build, if it has stopped.`), insert, followed by a blank line:

   ```rust
       /// Gives up on the started target `node`: it is skipped, and nothing that waits on it will
       /// start.
       ///
       /// For a target that was started and that no one began building before the build stopped.
       /// A `node` that is not started changes nothing.
       pub(crate) fn abandon(&mut self, node: NodeId) {
           if let Some(state) = self.states.get_mut(node.index())
               && *state == State::Started
           {
               *state = State::Skipped;
           }
       }
   ```

4. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib execute::schedule
   test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 379 filtered out; finished in 0.19s
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`.

6. Commit:

   ```
   $ git add crates/buildl-core/src/execute
   $ git commit -m "feat(buildl-core): abandon a started target"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- In a diamond, the one target that waits on nothing starts first, the two that wait on it start
  when it succeeds, and the last starts only when both of those have; nothing starts twice.
- Under `Stop`, the first failure stops the build: `stopped()` names it from then on, every
  target still pending is skipped whether or not it depends on the failure, a later failure does
  not replace the first, and a later success starts nothing.
- Under `KeepGoing`, a failure skips exactly the pending targets that depend on it, directly or
  through others; the rest of the build goes on; a second failure reports only what was still
  pending; `stopped()` stays `None`.
- A target that is not started cannot finish and cannot be abandoned; an abandoned target cannot
  finish and releases nothing.
- A chain of 100 000 targets is skipped from its far end without recursion, in id order.
- `crates/expected-module-edges.txt` is unchanged.

## Review findings

Reviewed once, 2026-10-12, when both tasks were done. The reviewer re-ran the gate:
`cargo make dod` exit 0 (`[cargo-make] INFO - Build Done in 4.93 seconds.`), `cargo deny check`
exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). Blocking: none. Fit to the plan:
`execute/schedule.rs` is byte-identical to the plan's final code blocks, and the `mod schedule;`
line with its attribute matches.

- risk — `Schedule::abandon` requires "the build has stopped" only in its doc: abandoning while
  the build has not stopped would leave that target's pending dependents pending for good —
  `execute/schedule.rs`. Left as it is: it matches spec §5.5, and spec §5.9 abandons only after
  a stop; the caller is written in plan 09.
- nit — the `Schedule` type doc says a started target "succeeds or fails" and leaves out
  started → skipped through `abandon` — `execute/schedule.rs`. Left as it is: the plan's own
  text.
- nit — the `skip_from` doc sentence "one skipped earlier had everything behind it skipped
  then" does not hold for a target skipped by `abandon`; the walk is still correct —
  `execute/schedule.rs`. Left as it is: the plan's own text.
- nit — the `stopped.is_some()` arm of `complete` cannot be observed (after a stop nothing is
  pending), so no test would catch its removal — `execute/schedule.rs`. Left as it is: the code
  is right.
- nit — `started.sort_unstable()` in `release` repeats the id order `TargetGraph::dependents`
  already gives; no test guards it — `execute/schedule.rs`. Left as it is: harmless.
- nit — no test asserts `stopped()` before a failure under `KeepGoing` (after one is covered;
  both are covered under `Stop`) — spec §9 row for `execute/schedule.rs`. Left as it is.

## Probe results

No separate probe was run. The facts each task asserts about existing code are ones its own
compile and test runs depend on:

- Task 1 — red: `error[E0432]: unresolved imports `super::Effects`, `super::Finish`,
  `super::Schedule``. Green: `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 379
  filtered out; finished in 0.00s` — as the plan quotes.
- Task 2 — red: `error[E0599]: no method named `abandon` found for struct `Schedule` in the
  current scope`, five times (one per call site), "could not compile `buildl-core` (lib test)
  due to 5 previous errors". Green: `test result: ok. 14 passed; 0 failed; 0 ignored; 0
  measured; 379 filtered out; finished in 0.20s` — as the plan quotes.
- After each task: `cargo fmt --all -- --check` printed nothing, `cargo make dod` ended
  `[cargo-make] INFO - Build Done in … seconds.`, `cargo deny check` ended `advisories ok, bans
  ok, licenses ok, sources ok`.

## Deviations

- 2026-10-12 — both tasks' commit steps were skipped. The work is in the tree, uncommitted;
  committing is left to the author.
- 2026-10-12 — this plan started while plan 06 was `executing` (built and reviewed), not
  `done`: the author asked for plans 06 to 10 in one run.
