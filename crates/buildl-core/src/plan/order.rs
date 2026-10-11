//! The order a graph's targets are visited in: every target after all it depends on.
//!
//! Its own file because it is a graph algorithm that knows nothing about keys: a verdict needs
//! the verdicts of a target's dependencies first, and a target's id says nothing about where it
//! sits among them.
//!
//! Responsibilities: [`topological`].
//!
//! Non-responsibilities: detecting a cycle. A graph has none by the time it exists.

use std::collections::VecDeque;

use crate::types::{NodeId, TargetGraph};

/// Every node of `graph`, each after all the nodes it depends on.
///
/// Computed with a queue and one counter per node, never by recursion, so a long dependency
/// chain cannot exhaust the call stack. Among nodes that are ready at the same time the lower id
/// comes first, so the same graph always yields the same order.
pub(crate) fn topological(graph: &TargetGraph) -> Vec<NodeId> {
    let ids = || (0_u32..).take(graph.len()).map(NodeId::new);
    let mut waiting: Vec<usize> = ids()
        .map(|id| graph.deps(id).map_or(0, <[NodeId]>::len))
        .collect();
    let mut ready: VecDeque<NodeId> = ids()
        .filter(|id| waiting.get(id.index()) == Some(&0))
        .collect();

    let mut order = Vec::with_capacity(graph.len());
    while let Some(id) = ready.pop_front() {
        order.push(id);
        for dependent in graph.dependents(id).unwrap_or_default() {
            if let Some(count) = waiting.get_mut(dependent.index()) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ready.push_back(*dependent);
                }
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::topological;
    use crate::plan::fixtures::{graph, node};
    use crate::types::NodeId;

    fn ids(raw: &[u32]) -> Vec<NodeId> {
        raw.iter().copied().map(NodeId::new).collect()
    }

    #[test]
    fn an_empty_graph_has_an_empty_order() {
        assert!(topological(&graph(Vec::new(), &[])).is_empty());
    }

    #[test]
    fn targets_that_depend_on_nothing_come_in_id_order() {
        let graph = graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
            ],
            &[&[], &[], &[]],
        );
        assert_eq!(topological(&graph), ids(&[0, 1, 2]));
    }

    #[test]
    fn a_target_comes_after_a_dependency_with_a_higher_id() {
        // `//:a` (0) depends on `//:z` (1): ids are in label order, not dependency order.
        let graph = graph(
            vec![node("//:a", |_| {}), node("//:z", |_| {})],
            &[&[1], &[]],
        );
        assert_eq!(topological(&graph), ids(&[1, 0]));
    }

    #[test]
    fn every_node_of_a_diamond_comes_after_all_it_depends_on() {
        // 0 -> 1 -> 3 and 0 -> 2 -> 3: `//:d` is built first and `//:a` last.
        let graph = graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
                node("//:d", |_| {}),
            ],
            &[&[1, 2], &[3], &[3], &[]],
        );
        assert_eq!(topological(&graph), ids(&[3, 1, 2, 0]));
    }

    #[test]
    fn a_long_chain_is_ordered_without_recursion() {
        let length = 100_000_u32;
        let nodes = (0..length)
            .map(|index| node(&format!("//:n{index:06}"), |_| {}))
            .collect();
        // Each node depends on the next, so the last node comes first.
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
        let order = topological(&graph(nodes, &edges));
        assert_eq!(order.len(), 100_000);
        assert_eq!(order.first(), Some(&NodeId::new(length - 1)));
        assert_eq!(order.last(), Some(&NodeId::new(0)));
        assert!(order.windows(2).all(|pair| pair[0] > pair[1]));
    }
}
