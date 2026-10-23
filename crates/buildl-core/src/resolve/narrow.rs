//! Narrowing: from a workspace's whole graph to the part one run was asked for.
//!
//! Its own file because it is the one step of this phase that reads values of the invocation:
//! which targets were asked for, and what the run is for. It answers with a graph like any other,
//! so nothing after it knows whether it was handed a whole workspace or a part of one.
//!
//! Responsibilities: [`narrow`].
//!
//! Non-responsibilities: reading build files and checking references, which happen for the
//! whole workspace before a graph exists; and deciding whether a target needs to run.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::error::{DeclarationSite, Error, Result};
use crate::resolve::reach::{closure, unknown_target};
use crate::types::{Label, NodeId, Purpose, Scope, TargetGraph, TargetRole};

/// The graph of what `scope` asks for when a run is for `purpose`.
///
/// [`Scope::Workspace`] asks for every target the purpose means: every target but tests for
/// [`Purpose::Build`], and every test for [`Purpose::Test`]. [`Scope::Targets`] asks for the
/// targets its labels name, each directly or as an alias: any target for [`Purpose::Build`], and
/// only tests for [`Purpose::Test`]. Whatever is asked for is taken with everything it depends on,
/// so a test that a taken target depends on is taken too.
///
/// The narrowed graph is a graph like any other: its targets are numbered in label order, each
/// keeps its dependencies in the order they were written, an alias is kept when the target it
/// names is, and every build setting is kept, since a setting belongs to the workspace and not
/// to a target. A target outside it is not part of the run at all, and a workspace with no target
/// of the purpose narrows to a graph with no target.
///
/// # Errors
///
/// Returns, for the first label of `scope` in label order that fails:
/// [`Error::UnknownTarget`] when it names neither a target nor an alias of `graph`, and, for
/// [`Purpose::Test`] only, [`Error::NotATest`] when it names a target that is not a test.
pub fn narrow<'g>(
    graph: &'g TargetGraph,
    scope: &Scope,
    purpose: Purpose,
) -> Result<Cow<'g, TargetGraph>> {
    let starts = match scope {
        Scope::Workspace => (0_u32..)
            .map(NodeId::new)
            .zip(graph.nodes())
            .filter(|(_, node)| node.role == role_of(purpose))
            .map(|(id, _)| id)
            .collect(),
        Scope::Targets(labels) => labels
            .iter()
            .map(|label| named(graph, label, purpose))
            .collect::<Result<Vec<NodeId>>>()?,
    };
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

/// The role of the targets a whole workspace means for `purpose`.
const fn role_of(purpose: Purpose) -> TargetRole {
    match purpose {
        Purpose::Build => TargetRole::Build,
        Purpose::Test => TargetRole::Test,
    }
}

/// The target `label` names in `graph`, directly or as an alias, when a run for `purpose` may name
/// it: any target for a build, and only a test for a test run.
fn named(graph: &TargetGraph, label: &Label, purpose: Purpose) -> Result<NodeId> {
    let id = graph
        .id_of(label)
        .ok_or_else(|| unknown_target(graph, label))?;
    match (purpose, graph.node(id)) {
        (Purpose::Test, Some(node)) if node.role != TargetRole::Test => Err(Error::NotATest {
            asked: label.clone(),
            site: Box::new(DeclarationSite::from(node)),
        }),
        // `id_of` gave `id`, so the node is there; a build takes any target named.
        _ => Ok(id),
    }
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
    use crate::resolve::fixtures::{
        alias, file, label, made_by, plugin, setting, target, target_with,
    };
    use crate::types::{NodeId, Origin, Purpose, Scope, TargetGraph, TargetRole};

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

    /// A build `//:app` and a test `//:app_test` of it; a build `//:lib` that `//:app` and the test
    /// `//:lib_test` use; a test `//:tool` that the build `//:gen` uses; a build `//:solo`;
    /// `//:check` names `//:app_test` and `//:default` names `//:app`.
    fn tested() -> TargetGraph {
        let test = |raw: &str, deps: &[&str]| {
            target_with(raw, |target| {
                target.role = TargetRole::Test;
                target.deps = deps.iter().map(|dep| label(dep)).collect();
            })
        };
        resolve(vec![
            target("//:app", &["//:lib"]),
            test("//:app_test", &["//:app"]),
            target("//:lib", &[]),
            test("//:lib_test", &["//:lib"]),
            test("//:tool", &[]),
            target("//:gen", &["//:tool"]),
            target("//:solo", &[]),
            alias("//:check", "//:app_test"),
            alias("//:default", "//:app"),
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
        let narrowed = narrow(&graph, &Scope::Workspace, Purpose::Build).unwrap();
        assert_eq!(*narrowed, graph);
    }

    #[test]
    fn a_target_brings_everything_it_depends_on_and_nothing_else() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:b"]), Purpose::Build).unwrap();
        assert_eq!(labels(&narrowed), ["//:b", "//:c"]);
        assert_eq!(narrowed.deps(NodeId::new(0)), Some(&[NodeId::new(1)][..]));
        assert!(narrowed.aliases().is_empty());
    }

    #[test]
    fn an_alias_names_its_target_and_is_kept_with_it() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:all"]), Purpose::Build).unwrap();
        assert_eq!(labels(&narrowed), ["//:a", "//:b", "//:c"]);
        assert_eq!(narrowed.id_of(&label("//:all")), Some(NodeId::new(0)));
    }

    #[test]
    fn asking_for_every_target_gives_back_an_equal_graph() {
        let graph = graph();
        let narrowed = narrow(&graph, &only(&["//:a", "//:z"]), Purpose::Build).unwrap();
        assert_eq!(*narrowed, graph);
    }

    #[test]
    fn no_label_is_no_target() {
        let graph = graph();
        let narrowed = narrow(&graph, &Scope::Targets(BTreeSet::new()), Purpose::Build).unwrap();
        assert!(narrowed.is_empty());
    }

    #[test]
    fn a_label_naming_nothing_is_refused_with_the_nearest_one() {
        let graph = graph();
        let error = narrow(&graph, &only(&["//:al"]), Purpose::Build).unwrap_err();
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

    #[test]
    fn a_build_of_the_workspace_leaves_tests_out() {
        let graph = tested();
        let narrowed = narrow(&graph, &Scope::Workspace, Purpose::Build).unwrap();
        assert_eq!(
            labels(&narrowed),
            ["//:app", "//:gen", "//:lib", "//:solo", "//:tool"]
        );
    }

    #[test]
    fn a_build_of_the_workspace_keeps_a_test_a_target_depends_on() {
        let graph = tested();
        let narrowed = narrow(&graph, &Scope::Workspace, Purpose::Build).unwrap();
        assert!(narrowed.id_of(&label("//:tool")).is_some());
    }

    #[test]
    fn a_build_of_a_named_test_includes_it() {
        let graph = tested();
        let narrowed = narrow(&graph, &only(&["//:app_test"]), Purpose::Build).unwrap();
        assert_eq!(labels(&narrowed), ["//:app", "//:app_test", "//:lib"]);
    }

    #[test]
    fn a_test_run_of_the_workspace_takes_every_test_and_its_deps() {
        let graph = tested();
        let narrowed = narrow(&graph, &Scope::Workspace, Purpose::Test).unwrap();
        assert_eq!(
            labels(&narrowed),
            ["//:app", "//:app_test", "//:lib", "//:lib_test", "//:tool"]
        );
    }

    #[test]
    fn a_test_run_of_named_tests_takes_them_and_their_deps() {
        let graph = tested();
        let narrowed = narrow(&graph, &only(&["//:lib_test"]), Purpose::Test).unwrap();
        assert_eq!(labels(&narrowed), ["//:lib", "//:lib_test"]);
    }

    #[test]
    fn a_test_run_follows_an_alias_to_a_test() {
        let graph = tested();
        let narrowed = narrow(&graph, &only(&["//:check"]), Purpose::Test).unwrap();
        assert_eq!(labels(&narrowed), ["//:app", "//:app_test", "//:lib"]);
        assert_eq!(narrowed.id_of(&label("//:check")), Some(NodeId::new(1)));
    }

    #[test]
    fn a_test_run_naming_a_build_target_is_not_a_test() {
        let graph = tested();
        let error = narrow(&graph, &only(&["//:default"]), Purpose::Test).unwrap_err();
        match error {
            Error::NotATest { asked, site } => {
                assert_eq!(asked, label("//:default"));
                assert_eq!(site.label, label("//:app"));
                assert_eq!(site.provenance, file(""));
                assert_eq!(site.origin, Origin::BuildFile);
            }
            other => unreachable!("expected NotATest, got {other:?}"),
        }
    }

    #[test]
    fn a_target_that_is_not_a_test_names_the_plugin_that_declared_it() {
        let graph = resolve(vec![made_by(vec![plugin("go")], target("//:app", &[]))]).unwrap();
        let error = narrow(&graph, &only(&["//:app"]), Purpose::Test).unwrap_err();
        let text = error.to_string();
        assert!(text.ends_with("in build.lua, plugin go@1.0.0"), "{text}");
    }

    #[test]
    fn the_first_failing_label_in_label_order_decides_the_error() {
        let graph = tested();
        let unknown_first = narrow(&graph, &only(&["//:app", "//:aaa"]), Purpose::Test);
        assert!(matches!(unknown_first, Err(Error::UnknownTarget { .. })));
        let not_a_test_first = narrow(&graph, &only(&["//:zzz", "//:app"]), Purpose::Test);
        assert!(matches!(not_a_test_first, Err(Error::NotATest { .. })));
    }

    #[test]
    fn a_test_run_of_a_workspace_without_tests_is_empty() {
        let graph = graph();
        let narrowed = narrow(&graph, &Scope::Workspace, Purpose::Test).unwrap();
        assert_eq!(labels(&narrowed), Vec::<String>::new());
        assert_eq!(narrowed.settings(), graph.settings());
    }
}
