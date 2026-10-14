//! Querying the graph, and the walk and refusal it shares with narrowing.
//!
//! Its own file because narrowing and querying need the same walk and the same refusal, and
//! neither should own the other's copy. Ranking names is `suggest`'s job; this file only chooses
//! the candidates.
//!
//! Responsibilities: [`reach`], the targets a query reaches; `closure`, the targets reachable from
//! a set of starts; and `unknown_target`, the error for a label that names nothing in a graph.
//!
//! Non-responsibilities: reading a query from text, which [`Query`] does; and building a graph
//! from the targets reached.

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::resolve::suggest::nearest;
use crate::types::{Direction, Label, NodeId, Query, TargetGraph};

/// The labels of the targets `query` reaches in `graph`: everything its target needs for
/// [`Direction::Deps`], and everything that needs its target for [`Direction::Rdeps`], through
/// any number of steps.
///
/// The query's own target is never in the answer, even when a cycle leads back to it, and an alias
/// is followed to the target it names but never answered, since an alias is not a node.
///
/// # Errors
///
/// Returns [`Error::UnknownTarget`] when the query's target is neither a target nor an alias of
/// `graph`.
pub fn reach(graph: &TargetGraph, query: &Query) -> Result<BTreeSet<Label>> {
    let target = graph
        .id_of(query.target())
        .ok_or_else(|| unknown_target(graph, query.target()))?;
    let step: Step = match query.direction() {
        Direction::Deps => TargetGraph::deps,
        Direction::Rdeps => TargetGraph::dependents,
    };
    let mut reached = closure(
        graph,
        step(graph, target).unwrap_or_default().to_vec(),
        step,
    );
    if let Some(own) = reached.get_mut(target.index()) {
        *own = false;
    }
    Ok(graph
        .nodes()
        .iter()
        .zip(&reached)
        .filter(|(_, reached)| **reached)
        .map(|(node, _)| node.label.clone())
        .collect())
}

/// One step of a walk from a node: the nodes next to it, or `None` for a node the graph does not
/// have. `TargetGraph::deps` and `TargetGraph::dependents` are the two steps.
type Step = fn(&TargetGraph, NodeId) -> Option<&[NodeId]>;

/// One flag per node of `graph`, indexed by [`NodeId::index`], set for every node reachable from
/// `starts` by `step`, the starts included.
///
/// Each node is visited once, so a node reached by several paths costs one visit.
pub(super) fn closure(graph: &TargetGraph, starts: Vec<NodeId>, step: Step) -> Vec<bool> {
    let mut reached = vec![false; graph.len()];
    let mut stack = starts;
    while let Some(id) = stack.pop() {
        if let Some(seen) = reached.get_mut(id.index())
            && !*seen
        {
            *seen = true;
            stack.extend(step(graph, id).unwrap_or_default());
        }
    }
    reached
}

/// The error for `label`, which names nothing in `graph`, with the nearest label that does.
pub(super) fn unknown_target(graph: &TargetGraph, label: &Label) -> Error {
    let declared = graph
        .nodes()
        .iter()
        .map(|node| &node.label)
        .chain(graph.aliases().keys());
    Error::UnknownTarget {
        label: label.clone(),
        suggestion: nearest(&label.to_string(), declared).cloned(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::{closure, reach};
    use crate::error::Error;
    use crate::resolve::assembly::resolve;
    use crate::resolve::fixtures::{alias, label, target};
    use crate::types::{Direction, NodeId, Query, TargetGraph};

    /// `//:top` depends on `//:left` and `//:right`, both on `//:base`; `//:solo` stands alone;
    /// `//:all` names `//:top`.
    fn diamond() -> TargetGraph {
        resolve(vec![
            target("//:top", &["//:left", "//:right"]),
            target("//:left", &["//:base"]),
            target("//:right", &["//:base"]),
            target("//:base", &[]),
            target("//:solo", &[]),
            alias("//:all", "//:top"),
        ])
        .unwrap()
    }

    fn id(graph: &TargetGraph, raw: &str) -> NodeId {
        graph.id_of(&label(raw)).unwrap()
    }

    /// The labels of the nodes `reached` flags, in node order.
    fn flagged(graph: &TargetGraph, reached: &[bool]) -> Vec<String> {
        graph
            .nodes()
            .iter()
            .zip(reached)
            .filter(|(_, reached)| **reached)
            .map(|(node, _)| node.label.to_string())
            .collect()
    }

    #[test]
    fn deps_reach_everything_below_once_with_the_start() {
        let graph = diamond();
        let reached = closure(&graph, vec![id(&graph, "//:top")], TargetGraph::deps);
        assert_eq!(reached.len(), graph.len());
        assert_eq!(
            flagged(&graph, &reached),
            ["//:base", "//:left", "//:right", "//:top"]
        );
    }

    #[test]
    fn rdeps_reach_everything_above_once_with_the_start() {
        let graph = diamond();
        let reached = closure(&graph, vec![id(&graph, "//:base")], TargetGraph::dependents);
        assert_eq!(
            flagged(&graph, &reached),
            ["//:base", "//:left", "//:right", "//:top"]
        );
    }

    #[test]
    fn no_start_reaches_nothing() {
        let graph = diamond();
        let reached = closure(&graph, Vec::new(), TargetGraph::deps);
        assert_eq!(flagged(&graph, &reached), Vec::<String>::new());
    }

    fn ask(graph: &TargetGraph, direction: Direction, raw: &str) -> BTreeSet<String> {
        reach(graph, &Query::new(direction, label(raw)))
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn set(labels: &[&str]) -> BTreeSet<String> {
        labels.iter().map(|label| (*label).to_owned()).collect()
    }

    #[test]
    fn deps_answer_everything_the_target_needs_without_the_target() {
        let graph = diamond();
        assert_eq!(
            ask(&graph, Direction::Deps, "//:top"),
            set(&["//:base", "//:left", "//:right"])
        );
        assert_eq!(ask(&graph, Direction::Deps, "//:left"), set(&["//:base"]));
    }

    #[test]
    fn rdeps_answer_everything_that_needs_the_target_without_the_target() {
        let graph = diamond();
        assert_eq!(
            ask(&graph, Direction::Rdeps, "//:base"),
            set(&["//:left", "//:right", "//:top"])
        );
        assert_eq!(ask(&graph, Direction::Rdeps, "//:left"), set(&["//:top"]));
    }

    #[test]
    fn a_leaf_answers_nothing() {
        let graph = diamond();
        assert_eq!(ask(&graph, Direction::Deps, "//:base"), set(&[]));
        assert_eq!(ask(&graph, Direction::Rdeps, "//:top"), set(&[]));
        assert_eq!(ask(&graph, Direction::Rdeps, "//:solo"), set(&[]));
    }

    #[test]
    fn an_alias_is_followed_and_never_answered() {
        let graph = diamond();
        assert_eq!(
            ask(&graph, Direction::Deps, "//:all"),
            ask(&graph, Direction::Deps, "//:top")
        );
        assert_eq!(ask(&graph, Direction::Rdeps, "//:all"), set(&[]));
        assert!(!ask(&graph, Direction::Rdeps, "//:base").contains("//:all"));
    }

    #[test]
    fn a_target_naming_nothing_is_refused_with_the_nearest_one() {
        let graph = diamond();
        let err = reach(&graph, &Query::new(Direction::Deps, label("//:tpo"))).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownTarget { label: ref asked, suggestion: Some(ref near) }
                    if *asked == label("//:tpo") && *near == label("//:top")
            ),
            "{err:?}"
        );
    }
}
