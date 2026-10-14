//! Narrowing: from a workspace's whole graph to the part one run was asked for.
//!
//! Its own file because it is the one step of this phase that reads a value of the invocation:
//! which targets were asked for. It answers with a graph like any other, so nothing after it
//! knows whether it was handed a whole workspace or a part of one.
//!
//! Responsibilities: [`narrow`].
//!
//! Non-responsibilities: reading build files and checking references, which happen for the
//! whole workspace before a graph exists; and deciding whether a target needs to run.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::error::Result;
use crate::resolve::reach::{closure, unknown_target};
use crate::types::{NodeId, Scope, TargetGraph};

/// The graph of what `scope` asks for: `graph` itself for [`Scope::Workspace`], and otherwise
/// the targets its labels name, each directly or as an alias, with everything they depend on.
///
/// The narrowed graph is a graph like any other: its targets are numbered in label order, each
/// keeps its dependencies in the order they were written, an alias is kept when the target it
/// names is, and every build setting is kept, since a setting belongs to the workspace and not
/// to a target. A target outside it is not part of the run at all.
///
/// # Errors
///
/// Returns [`Error::UnknownTarget`](crate::Error::UnknownTarget) for the first label of `scope`,
/// in label order, that names neither a target nor an alias of `graph`.
pub fn narrow<'g>(graph: &'g TargetGraph, scope: &Scope) -> Result<Cow<'g, TargetGraph>> {
    let Scope::Targets(labels) = scope else {
        return Ok(Cow::Borrowed(graph));
    };
    let starts = labels
        .iter()
        .map(|label| {
            graph
                .id_of(label)
                .ok_or_else(|| unknown_target(graph, label))
        })
        .collect::<Result<Vec<NodeId>>>()?;
    let wanted = closure(graph, starts, TargetGraph::deps);

    let renumbered: BTreeMap<NodeId, NodeId> = (0_u32..)
        .map(NodeId::new)
        .zip(&wanted)
        .filter(|(_, wanted)| **wanted)
        .map(|(id, _)| id)
        .zip((0_u32..).map(NodeId::new))
        .collect();
    let nodes = renumbered
        .keys()
        .filter_map(|id| graph.node(*id))
        .cloned()
        .collect();
    let edges = renumbered
        .keys()
        .map(|id| {
            graph
                .deps(*id)
                .unwrap_or_default()
                .iter()
                .filter_map(|dep| renumbered.get(dep))
                .copied()
                .collect()
        })
        .collect();
    let aliases = graph
        .aliases()
        .iter()
        .filter_map(|(alias, id)| Some((alias.clone(), *renumbered.get(id)?)))
        .collect();
    Ok(Cow::Owned(TargetGraph::new(
        nodes,
        edges,
        aliases,
        graph.settings().clone(),
    )))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::narrow;
    use crate::error::Error;
    use crate::resolve::assembly::resolve;
    use crate::resolve::fixtures::{alias, label, setting, target};
    use crate::types::{NodeId, Scope, TargetGraph};

    /// `//:a` depends on `//:b`, `//:b` on `//:c`; `//:z` stands alone; `//:all` names `//:a`.
    fn graph() -> TargetGraph {
        resolve(vec![
            target("//:a", &["//:b"]),
            target("//:b", &["//:c"]),
            target("//:c", &[]),
            target("//:z", &[]),
            alias("//:all", "//:a"),
            setting("mode"),
        ])
        .unwrap()
    }

    fn only(labels: &[&str]) -> Scope {
        Scope::Targets(labels.iter().map(|raw| label(raw)).collect())
    }

    fn labels(graph: &TargetGraph) -> Vec<String> {
        graph
            .nodes()
            .iter()
            .map(|node| node.label.to_string())
            .collect()
    }

    #[test]
    fn the_whole_workspace_is_the_graph_itself() {
        let graph = graph();
        let narrowed = narrow(&graph, &Scope::Workspace).unwrap();
        assert_eq!(*narrowed, graph);
    }

    #[test]
    fn a_target_brings_everything_it_depends_on_and_nothing_else() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:b"])).unwrap();
        assert_eq!(labels(&narrowed), ["//:b", "//:c"]);
        assert_eq!(narrowed.deps(NodeId::new(0)), Some(&[NodeId::new(1)][..]));
        assert!(narrowed.aliases().is_empty());
    }

    #[test]
    fn an_alias_names_its_target_and_is_kept_with_it() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:all"])).unwrap();
        assert_eq!(labels(&narrowed), ["//:a", "//:b", "//:c"]);
        assert_eq!(narrowed.id_of(&label("//:all")), Some(NodeId::new(0)));
    }

    #[test]
    fn asking_for_every_target_gives_back_an_equal_graph() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:a", "//:z"])).unwrap();
        assert_eq!(*narrowed, graph);
    }

    #[test]
    fn no_label_is_no_target() {
        let graph = graph();
        let narrowed = narrow(&graph, &Scope::Targets(BTreeSet::new())).unwrap();
        assert!(narrowed.is_empty());
    }

    #[test]
    fn a_label_naming_nothing_is_refused_with_the_nearest_one() {
        let graph = graph();
        let error = narrow(&graph, &only(&["//:al"])).unwrap_err();
        match error {
            Error::UnknownTarget {
                label: asked,
                suggestion,
            } => {
                assert_eq!(asked, label("//:al"));
                assert_eq!(suggestion, Some(label("//:a")));
            }
            other => unreachable!("expected UnknownTarget, got {other:?}"),
        }
    }
}
