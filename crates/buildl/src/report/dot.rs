//! A target graph as Graphviz DOT.
//!
//! Its own file because it is a second form of the graph, for a drawing tool instead of a
//! program: the canonical JSON of a graph is the graph's own serialized form, and this is a
//! rendering of it, with an order of its own to keep fixed.
//!
//! Responsibilities: [`graph_dot`].
//!
//! Non-responsibilities: building or narrowing a graph, and writing the text anywhere.

use buildl_core::TargetGraph;

/// `graph` as a Graphviz `digraph`, one statement a line, ending with a newline.
///
/// Every target comes first, in label order; then every dependency as an edge from the
/// dependent to what it depends on, by dependent in label order and by dependency in the order
/// it was written; then every alias in label order, as a node without a border and a dashed
/// edge to the target it names. The same graph always gives the same text.
///
/// Every name is quoted and none is escaped: a label holds only ASCII letters, digits, `.`,
/// `-`, `_`, `/` and `:`.
#[must_use]
pub fn graph_dot(graph: &TargetGraph) -> String {
    let mut lines = vec!["digraph buildl {\n".to_owned()];
    lines.extend(
        graph
            .nodes()
            .iter()
            .map(|node| format!("  \"{}\";\n", node.label)),
    );
    for node in graph.nodes() {
        let deps = graph
            .id_of(&node.label)
            .and_then(|id| graph.deps(id))
            .unwrap_or_default();
        lines.extend(
            deps.iter()
                .filter_map(|id| graph.node(*id))
                .map(|dep| format!("  \"{}\" -> \"{}\";\n", node.label, dep.label)),
        );
    }
    for (alias, id) in graph.aliases() {
        if let Some(target) = graph.node(*id) {
            lines.push(format!("  \"{alias}\" [shape=plaintext];\n"));
            lines.push(format!(
                "  \"{alias}\" -> \"{}\" [style=dashed];\n",
                target.label
            ));
        }
    }
    lines.push("}\n".to_owned());
    lines.concat()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use buildl_core::{
        Action, Alias, Argument, Command, Declaration, Declared, Freshness, Label, NetworkAccess,
        Origin, Provenance, Target, TargetRole, resolve,
    };

    use super::graph_dot;

    /// Where the declaration of `label` is: the build file of its directory.
    fn declared(label: &Label, item: Declared) -> Declaration {
        let directory = label.directory().clone();
        let file = if directory.as_str().is_empty() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(directory.as_str()).join("build.lua")
        };
        Declaration::new(Provenance::new(file, directory), Origin::BuildFile, item)
    }

    /// The target `raw`, running `cc`, depending on `deps` in that order.
    fn target(raw: &str, deps: &[&str]) -> Declaration {
        let label = Label::parse(raw).unwrap();
        declared(
            &label,
            Declared::Target(Target {
                label: label.clone(),
                role: TargetRole::Build,
                action: Action::Run(Command::new(vec![Argument::parse("cc").unwrap()]).unwrap()),
                inputs: Vec::new(),
                deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
                outputs: Vec::new(),
                env: Vec::new(),
                network: NetworkAccess::Sealed,
                freshness: Freshness::Cached,
            }),
        )
    }

    /// The alias `raw` for the target `of`.
    fn alias(raw: &str, of: &str) -> Declaration {
        let label = Label::parse(raw).unwrap();
        declared(
            &label,
            Declared::Alias(Alias {
                label: label.clone(),
                target: Label::parse(of).unwrap(),
            }),
        )
    }

    #[test]
    fn nodes_edges_and_aliases_come_in_a_fixed_order() {
        // Declared in an order that is neither label order nor the order of the text.
        let graph = resolve(vec![
            alias("//:default", "//:app"),
            target("//lib:text", &[]),
            target("//:main.o", &[]),
            target("//:docs", &[]),
            target("//:app", &["//:main.o", "//lib:text"]),
        ])
        .unwrap();
        assert_eq!(
            graph_dot(&graph),
            concat!(
                "digraph buildl {\n",
                "  \"//:app\";\n",
                "  \"//:docs\";\n",
                "  \"//:main.o\";\n",
                "  \"//lib:text\";\n",
                "  \"//:app\" -> \"//:main.o\";\n",
                "  \"//:app\" -> \"//lib:text\";\n",
                "  \"//:default\" [shape=plaintext];\n",
                "  \"//:default\" -> \"//:app\" [style=dashed];\n",
                "}\n",
            )
        );
    }

    #[test]
    fn a_dependency_keeps_the_order_it_was_written_in() {
        let graph = resolve(vec![
            target("//:a", &[]),
            target("//:b", &[]),
            target("//:z", &["//:b", "//:a"]),
            alias("//:all", "//:z"),
            alias("//:also", "//:a"),
        ])
        .unwrap();
        assert_eq!(
            graph_dot(&graph),
            concat!(
                "digraph buildl {\n",
                "  \"//:a\";\n",
                "  \"//:b\";\n",
                "  \"//:z\";\n",
                "  \"//:z\" -> \"//:b\";\n",
                "  \"//:z\" -> \"//:a\";\n",
                "  \"//:all\" [shape=plaintext];\n",
                "  \"//:all\" -> \"//:z\" [style=dashed];\n",
                "  \"//:also\" [shape=plaintext];\n",
                "  \"//:also\" -> \"//:a\" [style=dashed];\n",
                "}\n",
            )
        );
    }

    #[test]
    fn a_graph_with_no_target_is_an_empty_digraph() {
        let graph = resolve(Vec::new()).unwrap();
        assert_eq!(graph_dot(&graph), "digraph buildl {\n}\n");
    }
}
