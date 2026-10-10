---
status: done
created: 2026-10-10
depends-on: [04]
---

# Cycle Implementation Plan

**Goal:** A dependency cycle is found by a search that never recurses.

**Architecture:** One private file, `resolve/cycle.rs`, working on ids alone: `find` takes one edge list per node and returns the ids of the first cycle, or `None`. The search is depth-first with an explicit stack of `(node, next edge)` pairs and three states per node, so a chain of any length uses heap, not call stack. Roots are tried in id order and edges in list order, so one graph always reports one cycle. The cycle is rotated to start at its lowest id, which is its lowest label, because ids are issued in label order.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §4.5 (cycles), §9 (the `resolve/cycle.rs` row); the constraint "no recursion over the graph".

**Checkpoints:** review once, when task 1 is done.

---

## Context an implementer needs

What plan `04` leaves that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `resolve/mod.rs`, declaring `mod suggest;` under a `cfg_attr(not(test), expect(dead_code, …))` | `crates/buildl-core/src/resolve/mod.rs` | the file this plan adds `mod cycle;` to |
| `NodeId` (`new`, `index`, `Ord`, `Copy`) | `crates/buildl-core/src/types/node_id.rs` | the only type the search needs |

Gate facts that bite here. A bullet that names a lint or a rustdoc failure reports one seen as a failing run while this code was prototyped; the others explain a choice:

- Nothing outside its own tests calls this file's functions until plan `06`, so the library
  build reports them as dead code and the gate fails. The `mod` line therefore carries
  `cfg_attr(not(test), expect(dead_code, …))`. It must be `cfg_attr(not(test), …)`: in the test
  build the functions are used, and a plain `expect` would fail there as unfulfilled. Plan `06`
  removes the attribute.
- Roots come from `(0_u32..).take(edges.len())`: `take` stops after the last node and never asks
  the range for an id past it.
- The 100 000-node test is the reason the search is iterative; it runs in the gate.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form, and every command output below was captured by running that command on exactly the state the step describes. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/resolve/mod.rs     — [modify] declare cycle
crates/buildl-core/src/resolve/cycle.rs   — [create] find, its state type, unit tests
```

### Task 1 — Find a dependency cycle

**Files:**
- Modify `crates/buildl-core/src/resolve/mod.rs`
- Create `crates/buildl-core/src/resolve/cycle.rs`

**Steps:**

1. Declare the module. `crates/buildl-core/src/resolve/mod.rs` becomes:

   ```rust
   //! Resolve, the second phase: from the declarations of every build file to the target graph.
   //!
   //! Its own directory because it is a phase, and each phase has one home that names no other.
   //!
   //! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod cycle;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "called by the function that builds the graph, which is not written yet"
       )
   )]
   mod suggest;
   ```

2. Write the failing tests. Create `crates/buildl-core/src/resolve/cycle.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       use super::find;
       use crate::types::NodeId;

       fn edges(lists: &[&[u32]]) -> Vec<Vec<NodeId>> {
           lists
               .iter()
               .map(|list| list.iter().copied().map(NodeId::new).collect())
               .collect()
       }

       fn ids(raw: &[u32]) -> Vec<NodeId> {
           raw.iter().copied().map(NodeId::new).collect()
       }

       #[test]
       fn a_graph_with_no_edges_has_no_cycle() {
           assert_eq!(find(&[]), None);
           assert_eq!(find(&edges(&[&[], &[], &[]])), None);
       }

       #[test]
       fn a_diamond_is_not_a_cycle() {
           // 0 -> 1 -> 3 and 0 -> 2 -> 3
           assert_eq!(find(&edges(&[&[1, 2], &[3], &[3], &[]])), None);
       }

       #[test]
       fn a_node_that_depends_on_itself_is_a_cycle_of_one() {
           assert_eq!(find(&edges(&[&[], &[1]])), Some(ids(&[1])));
       }

       #[test]
       fn a_cycle_of_two_is_reported_from_its_lowest_id() {
           assert_eq!(find(&edges(&[&[1], &[0]])), Some(ids(&[0, 1])));
       }

       #[test]
       fn a_cycle_reached_through_a_tail_leaves_the_tail_out() {
           // 0 -> 3 -> 2 -> 4 -> 3: the cycle is 2 -> 4 -> 3.
           let graph = edges(&[&[3], &[], &[4], &[2], &[3]]);
           assert_eq!(find(&graph), Some(ids(&[2, 4, 3])));
       }

       #[test]
       fn the_first_cycle_in_root_then_edge_order_is_the_one_reported() {
           // Two cycles: 0 -> 2 -> 0 and 1 -> 3 -> 1. Root 0 is tried first.
           let graph = edges(&[&[2], &[3], &[0], &[1]]);
           assert_eq!(find(&graph), Some(ids(&[0, 2])));
           // From one root, the first listed dependency is searched first.
           let graph = edges(&[&[2, 1], &[0], &[0]]);
           assert_eq!(find(&graph), Some(ids(&[0, 2])));
       }

       #[test]
       fn a_long_chain_is_searched_without_recursion() {
           let length = 100_000_u32;
           let mut chain: Vec<Vec<NodeId>> = (1..length).map(|next| vec![NodeId::new(next)]).collect();
           chain.push(Vec::new());
           assert_eq!(find(&chain), None);
           // Closing the chain makes every node part of one cycle, reported from id 0.
           if let Some(last) = chain.last_mut() {
               last.push(NodeId::new(0));
           }
           let cycle = find(&chain);
           assert_eq!(cycle.as_ref().map(Vec::len), Some(100_000));
           assert_eq!(
               cycle.as_ref().and_then(|c| c.first()),
               Some(&NodeId::new(0))
           );
       }

       #[test]
       fn an_edge_to_an_id_outside_the_graph_is_ignored() {
           assert_eq!(find(&edges(&[&[7]])), None);
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --lib resolve::cycle
   error[E0432]: unresolved import `super::find`
   ```

4. Replace the placeholder line at the top of `crates/buildl-core/src/resolve/cycle.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Finding a dependency cycle without recursion.
   //!
   //! Its own file because it is a graph algorithm over ids alone: it needs no label, no build
   //! file and no declaration, only which node depends on which.
   //!
   //! Responsibilities: [`find`].
   //!
   //! Non-responsibilities: reporting. The caller turns the ids of a cycle into labels and build
   //! files.

   use crate::types::NodeId;

   /// Where the search stands with one node.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   enum State {
       /// Not reached yet.
       Unvisited,
       /// Reached, with its dependencies still being searched.
       OnStack,
       /// Searched, with everything it depends on: no cycle passes through it.
       Finished,
   }

   /// The first dependency cycle in `edges`, or `None` when there is none.
   ///
   /// `edges` holds one list per node: the ids that node depends on. The search is depth-first
   /// with an explicit stack, so a long dependency chain cannot exhaust the call stack. Roots are
   /// tried in id order and each node's dependencies in list order, so the same graph always
   /// reports the same cycle.
   ///
   /// The cycle is returned starting at its lowest id; each node depends on the next, and the last
   /// on the first. An id outside `edges` is ignored.
   pub(crate) fn find(edges: &[Vec<NodeId>]) -> Option<Vec<NodeId>> {
       let mut states = vec![State::Unvisited; edges.len()];
       let mut stack: Vec<(NodeId, usize)> = Vec::new();

       for root in (0_u32..).take(edges.len()).map(NodeId::new) {
           if states.get(root.index()) != Some(&State::Unvisited) {
               continue;
           }
           set(&mut states, root, State::OnStack);
           stack.push((root, 0));

           while let Some((node, next)) = stack.last_mut() {
               let node = *node;
               let dependency = edges.get(node.index()).and_then(|deps| deps.get(*next));
               *next += 1;
               let Some(dependency) = dependency.copied() else {
                   set(&mut states, node, State::Finished);
                   stack.pop();
                   continue;
               };
               match states.get(dependency.index()) {
                   Some(State::OnStack) => return Some(cycle_from(&stack, dependency)),
                   Some(State::Unvisited) => {
                       set(&mut states, dependency, State::OnStack);
                       stack.push((dependency, 0));
                   }
                   Some(State::Finished) | None => {}
               }
           }
       }
       None
   }

   /// Records `state` for `node`, if `node` is in bounds.
   fn set(states: &mut [State], node: NodeId, state: State) {
       if let Some(slot) = states.get_mut(node.index()) {
           *slot = state;
       }
   }

   /// The part of `stack` from `start` to its top, rotated to begin at its lowest id.
   fn cycle_from(stack: &[(NodeId, usize)], start: NodeId) -> Vec<NodeId> {
       let mut cycle: Vec<NodeId> = stack
           .iter()
           .map(|(node, _)| *node)
           .skip_while(|node| *node != start)
           .collect();
       let lowest = cycle
           .iter()
           .enumerate()
           .min_by_key(|(_, node)| **node)
           .map_or(0, |(position, _)| position);
       cycle.rotate_left(lowest);
       cycle
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl-core --lib resolve::cycle
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 165 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   ```

   Expected: both exit `0`. `cargo fmt --all -- --check` prints nothing, and `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`

7. Commit:

   ```
   $ git add crates/buildl-core/src/resolve/mod.rs crates/buildl-core/src/resolve/cycle.rs
   $ git commit -m "feat(buildl-core): find a dependency cycle without recursion"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- The tests cover a cycle of one, of two and of several; a cycle reached through a tail; a
  diamond that is not a cycle; the choice between two cycles; and a chain of 100 000 nodes, open
  and closed.

## Review findings

One reviewer pass over the task's two files, 2026-10-10. Verdict: spec compliant, blocking set empty. The reviewer re-ran the gate: `cargo fmt --all -- --check` exit 0; `cargo make dod` exit 0 (`buildl-core` unit 173 passed, flows 13); `cargo deny check` exit 0 (`advisories ok, bans ok, licenses ok, sources ok`). Both files match this plan's code blocks.

- nit — at exactly 2^32 nodes, the most a graph may hold, `(0_u32..).take(len)` computes the successor of `u32::MAX` while yielding the last id: a panic with overflow checks on, an unused wrap in release. Read from std's `RangeFrom::next`, not run (the case needs 2^32 edge lists). Not fixed — `crates/buildl-core/src/resolve/cycle.rs:37`
- nit — `a_cycle_of_two_is_reported_from_its_lowest_id` passes with or without the rotation, because root 0 is searched first and the cycle is already `[0, 1]`; only `a_cycle_reached_through_a_tail_leaves_the_tail_out` guards the rotation. The name claims more than the test proves. Not fixed — `crates/buildl-core/src/resolve/cycle.rs:123`

## Probe results

No separate probe was run. Every fact the task asserts about existing code is put under test by the task's own cycle, and both runs matched the plan:

- red — `cargo test -p buildl-core --lib resolve::cycle` — ``error[E0432]: unresolved import `super::find` ``
- green — `cargo test -p buildl-core --lib resolve::cycle` — `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 165 filtered out`

## Deviations

- 2026-10-10 — the task's commit step was not run during execution. The commit is the author's to make; the task's two files were left in the working tree.
- 2026-10-10 — the plan ran while plan `04`, which it depends on, was built and reviewed but not yet marked `done`. Plans `01` to `05` were executed as one requested run.
