//! Layout: where every output of every target lives, and that no two of them collide.
//!
//! Its own file because an output's place is decided once, before anything runs, from the
//! workspace's output directory and the target's label, and a build whose targets would write
//! over one another is refused here rather than found out halfway through.
//!
//! Responsibilities: [`Layout`].
//!
//! Non-responsibilities: the rule that turns a label and a name into a path, which is
//! [`OutputPath::of`]; and putting anything at a path, which is a port's.

use std::collections::BTreeMap;

use crate::error::{DeclarationSite, Error, Result};
use crate::types::{Directory, Node, NodeId, OutputPath, TargetGraph};

/// The path of every output of every target of one graph.
#[derive(Debug)]
pub(crate) struct Layout {
    /// One list per node, in id order: the node's outputs, in the order they were written.
    paths: Vec<Vec<OutputPath>>,
}

impl Layout {
    /// Places every output of `graph` under `out`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutputConflict`] when two outputs would be written to one place: the
    /// same path, or a path another output needs as a directory. The outputs are taken in path
    /// order, then label order, and the first that collides with one taken before it is the one
    /// reported, with the shorter of the two paths and the two targets, lower label first. A
    /// target that declares both outputs is named twice.
    pub(crate) fn of(graph: &TargetGraph, out: &Directory) -> Result<Self> {
        let paths: Vec<Vec<OutputPath>> = graph
            .nodes()
            .iter()
            .map(|node| {
                node.outputs
                    .iter()
                    .map(|name| OutputPath::of(out, &node.label, name))
                    .collect()
            })
            .collect();

        let mut claims: Vec<(&OutputPath, &Node)> = graph
            .nodes()
            .iter()
            .zip(&paths)
            .flat_map(|(node, paths)| paths.iter().map(move |path| (path, node)))
            .collect();
        claims.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.label.cmp(&b.1.label)));

        let mut taken: BTreeMap<&str, (&OutputPath, &Node)> = BTreeMap::new();
        for (path, node) in claims {
            if let Some((shorter, holder)) = collision(&taken, path.as_str()) {
                let (first, second) = if holder.label <= node.label {
                    (holder, node)
                } else {
                    (node, holder)
                };
                return Err(Error::OutputConflict {
                    path: shorter.clone(),
                    first: Box::new(DeclarationSite::from(first)),
                    second: Box::new(DeclarationSite::from(second)),
                });
            }
            taken.insert(path.as_str(), (path, node));
        }
        Ok(Self { paths })
    }

    /// The paths of the outputs of the node `id` names, in the order they were written; empty
    /// for an id the graph did not issue.
    pub(crate) fn outputs(&self, id: NodeId) -> &[OutputPath] {
        self.paths.get(id.index()).map_or(&[], Vec::as_slice)
    }

    /// The paths of every output of every node `id` depends on: the dependencies in the order
    /// they were written, and each one's outputs in the order they were written.
    pub(crate) fn deps(&self, graph: &TargetGraph, id: NodeId) -> Vec<OutputPath> {
        graph
            .deps(id)
            .unwrap_or_default()
            .iter()
            .flat_map(|dep| self.outputs(*dep))
            .cloned()
            .collect()
    }
}

/// The path already taken that `path` collides with, and the target that took it: `path`
/// itself, or a directory above it.
///
/// A path sorts after every directory above it, so a collision is always with a path taken
/// earlier; and since the paths taken so far collide with one another nowhere, there is at most
/// one.
fn collision<'a>(
    taken: &BTreeMap<&str, (&'a OutputPath, &'a Node)>,
    path: &str,
) -> Option<(&'a OutputPath, &'a Node)> {
    let above = path.match_indices('/').filter_map(|(at, _)| path.get(..at));
    core::iter::once(path)
        .chain(above)
        .find_map(|candidate| taken.get(candidate).copied())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Layout;
    use crate::error::Error;
    use crate::execute::fixtures::{dir, graph, node, outputs};
    use crate::types::{Directory, NodeId, OutputPath, TargetGraph};

    fn texts(paths: &[OutputPath]) -> Vec<&str> {
        paths.iter().map(OutputPath::as_str).collect()
    }

    /// What `graph` is refused for under `out`: the path, and the two labels named, in order.
    fn conflict(graph: &TargetGraph) -> (String, String, String) {
        match Layout::of(graph, &dir("out")).unwrap_err() {
            Error::OutputConflict {
                path,
                first,
                second,
            } => (
                path.to_string(),
                first.label.to_string(),
                second.label.to_string(),
            ),
            other => unreachable!("expected OutputConflict, got {other:?}"),
        }
    }

    fn named(path: &str, first: &str, second: &str) -> (String, String, String) {
        (path.to_owned(), first.to_owned(), second.to_owned())
    }

    #[test]
    fn every_output_lives_under_the_output_directory_and_its_labels_directory() {
        let graph = graph(
            vec![
                node("//:app", |node| node.outputs = outputs(&["app", "app.map"])),
                node("//lib:text", |_| {}),
            ],
            &[&[], &[]],
        );
        let layout = Layout::of(&graph, &dir("out")).unwrap();
        assert_eq!(
            texts(layout.outputs(NodeId::new(0))),
            ["out/app", "out/app.map"]
        );
        assert_eq!(texts(layout.outputs(NodeId::new(1))), ["out/lib/text"]);
        assert_eq!(layout.outputs(NodeId::new(2)), []);

        let at_root = Layout::of(&graph, &Directory::root()).unwrap();
        assert_eq!(texts(at_root.outputs(NodeId::new(1))), ["lib/text"]);
    }

    #[test]
    fn dependency_outputs_come_in_edge_order_then_in_written_order() {
        let graph = graph(
            vec![
                node("//:a", |node| node.outputs = outputs(&["z", "a"])),
                node("//:app", |_| {}),
                node("//:c", |_| {}),
            ],
            &[&[], &[2, 0], &[]],
        );
        let layout = Layout::of(&graph, &dir("out")).unwrap();
        assert_eq!(
            texts(&layout.deps(&graph, NodeId::new(1))),
            ["out/c", "out/z", "out/a"]
        );
        assert_eq!(layout.deps(&graph, NodeId::new(0)), []);
        assert_eq!(layout.deps(&graph, NodeId::new(3)), []);
    }

    #[test]
    fn two_targets_may_not_claim_one_path() {
        let graph = graph(
            vec![
                node("//:app", |node| node.outputs = outputs(&["bin/tool"])),
                node("//bin:tool", |_| {}),
            ],
            &[&[], &[]],
        );
        assert_eq!(
            conflict(&graph),
            named("out/bin/tool", "//:app", "//bin:tool")
        );
    }

    #[test]
    fn an_output_may_not_lie_under_another() {
        let graph = graph(
            vec![node("//:lib", |_| {}), node("//lib:text", |_| {})],
            &[&[], &[]],
        );
        assert_eq!(conflict(&graph), named("out/lib", "//:lib", "//lib:text"));
        assert_eq!(
            Layout::of(&graph, &dir("out")).unwrap_err().to_string(),
            "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
        );
    }

    #[test]
    fn the_lower_label_is_named_first_whichever_path_it_claims() {
        let graph = graph(
            vec![
                node("//:a", |node| node.outputs = outputs(&["sub/x"])),
                node("//:z", |node| node.outputs = outputs(&["sub"])),
            ],
            &[&[], &[]],
        );
        assert_eq!(conflict(&graph), named("out/sub", "//:a", "//:z"));
    }

    #[test]
    fn a_name_that_only_starts_like_another_is_no_conflict() {
        let graph = graph(
            vec![
                node("//:lib", |node| node.outputs = outputs(&["lib", "lib.o"])),
                node("//:lib2", |_| {}),
                node("//lib3:text", |_| {}),
            ],
            &[&[], &[], &[]],
        );
        assert!(Layout::of(&graph, &dir("out")).is_ok());
    }

    #[test]
    fn a_conflict_is_found_past_a_path_that_sorts_between_the_two() {
        let graph = graph(
            vec![
                node("//:lib", |node| node.outputs = outputs(&["lib", "lib.o"])),
                node("//lib:text", |_| {}),
            ],
            &[&[], &[]],
        );
        assert_eq!(conflict(&graph), named("out/lib", "//:lib", "//lib:text"));
    }

    #[test]
    fn one_target_claiming_a_path_twice_names_itself_twice() {
        let same = graph(
            vec![node("//:app", |node| {
                node.outputs = outputs(&["app", "app"]);
            })],
            &[&[]],
        );
        assert_eq!(conflict(&same), named("out/app", "//:app", "//:app"));

        let under = graph(
            vec![node("//:app", |node| {
                node.outputs = outputs(&["gen/x", "gen"]);
            })],
            &[&[]],
        );
        assert_eq!(conflict(&under), named("out/gen", "//:app", "//:app"));
    }

    #[test]
    fn the_lowest_conflicting_path_is_the_one_reported() {
        let graph = graph(
            vec![
                node("//:a", |node| node.outputs = outputs(&["m"])),
                node("//:b", |node| node.outputs = outputs(&["m", "k"])),
                node("//:c", |node| node.outputs = outputs(&["k"])),
            ],
            &[&[], &[], &[]],
        );
        assert_eq!(conflict(&graph), named("out/k", "//:b", "//:c"));
    }
}
