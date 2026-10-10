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
