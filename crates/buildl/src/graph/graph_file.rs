//! [`GraphFile`], the record of the graph a local workspace's last build resolved.
//!
//! Its own file because the record has one rule and it is about bytes: `graph.json` holds what
//! the canonical serializer writes for the whole graph and nothing else, written whole. The
//! same build files therefore leave the same file, whichever targets a build was asked for, and
//! a reader finds the old record or the new one, never part of either.
//!
//! Responsibilities: [`GraphFile`] and its [`GraphRecord`] implementation.
//!
//! Non-responsibilities: resolving the graph, and deciding when it is recorded; both are the
//! pipeline's.

use std::path::PathBuf;

use buildl_core::json::canonical;
use buildl_core::{Diagnostic, Error, GraphRecord, PortName, Result, TargetGraph};

use crate::state::{Access, StateLayout, write_whole};

/// Keeps the graph the last build resolved, in `graph.json`.
#[derive(Debug)]
pub struct GraphFile {
    path: PathBuf,
    staging: PathBuf,
}

impl GraphFile {
    /// A record where `layout` keeps it. Touches nothing on disk.
    #[must_use]
    pub fn new(layout: &StateLayout) -> Self {
        Self {
            path: layout.graph(),
            staging: layout.dir().to_path_buf(),
        }
    }
}

impl GraphRecord for GraphFile {
    fn record(&mut self, graph: &TargetGraph) -> Result<()> {
        let bytes = canonical::to_vec(graph)?;
        write_whole(&self.staging, &self.path, &bytes, Access::Rewritable).map_err(|error| {
            Error::Adapter {
                port: PortName::GraphRecord,
                diagnostic: Diagnostic::new(format!("writing {}: {error}", self.path.display())),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::PathBuf;

    use buildl_core::{
        Action, Alias, Argument, Command, Declaration, Declared, Directory, Error, Freshness,
        GraphRecord, Label, NetworkAccess, OutputName, PortName, Provenance, Target, TargetGraph,
        TargetRole, resolve,
    };

    use super::GraphFile;
    use crate::state::StateLayout;

    /// The layout of a new workspace whose state directory exists and is empty.
    fn workspace() -> (tempfile::TempDir, StateLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
        fs::create_dir_all(layout.dir()).unwrap();
        (dir, layout)
    }

    /// The label `//:<name>`.
    fn label(name: &str) -> Label {
        Label::parse(&format!("//:{name}")).unwrap()
    }

    /// A graph of two targets declared at the workspace root, `app` depending on `lib`, each
    /// running `cc` and producing one output of its own name, with the alias `default` for
    /// `app`.
    fn graph() -> TargetGraph {
        let file = Provenance::new(PathBuf::from("build.lua"), Directory::root());
        let target = |name: &str, deps: &[&str]| {
            Declaration::new(
                file.clone(),
                Declared::Target(Target {
                    label: label(name),
                    role: TargetRole::Build,
                    action: Action::Run(
                        Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                    ),
                    inputs: Vec::new(),
                    deps: deps.iter().map(|dep| label(dep)).collect(),
                    outputs: vec![OutputName::parse(name).unwrap()],
                    env: Vec::new(),
                    network: NetworkAccess::Sealed,
                    freshness: Freshness::Cached,
                }),
            )
        };
        let alias = Declaration::new(
            file.clone(),
            Declared::Alias(Alias {
                label: label("default"),
                target: label("app"),
            }),
        );
        resolve(vec![target("app", &["lib"]), target("lib", &[]), alias]).unwrap()
    }

    /// The names of the entries of the state directory, in order.
    fn entries(layout: &StateLayout) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(layout.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_record_is_the_canonical_bytes_and_nothing_else() {
        let (_dir, layout) = workspace();
        let mut record = GraphFile::new(&layout);
        assert_eq!(entries(&layout), [] as [&str; 0]);

        record.record(&graph()).unwrap();

        assert_eq!(
            fs::read_to_string(layout.graph()).unwrap(),
            concat!(
                r#"{"aliases":{"//:default":"//:app"},"settings":{},"targets":{"#,
                r#""//:app":{"deps":["//:lib"],"description":null,"env":[],"#,
                r#""freshness":"Cached","inputs":[],"network":"Sealed","outputs":["app"],"#,
                r#""provenance":{"directory":"","file":"build.lua"},"role":"Build","#,
                r#""run":["cc"]},"#,
                r#""//:lib":{"deps":[],"description":null,"env":[],"#,
                r#""freshness":"Cached","inputs":[],"network":"Sealed","outputs":["lib"],"#,
                r#""provenance":{"directory":"","file":"build.lua"},"role":"Build","#,
                r#""run":["cc"]}}}"#
            )
        );
        assert_eq!(entries(&layout), ["graph.json"]);
    }

    #[test]
    fn a_second_record_replaces_the_first_and_leaves_no_temp_file() {
        let (_dir, layout) = workspace();
        let mut record = GraphFile::new(&layout);
        record.record(&graph()).unwrap();

        record.record(&resolve(Vec::new()).unwrap()).unwrap();

        assert_eq!(
            fs::read_to_string(layout.graph()).unwrap(),
            r#"{"aliases":{},"settings":{},"targets":{}}"#
        );
        assert_eq!(entries(&layout), ["graph.json"]);
    }

    #[test]
    fn a_graph_that_cannot_be_written_is_an_adapter_error() {
        let (_dir, layout) = workspace();
        fs::create_dir(layout.graph()).unwrap();
        let mut record = GraphFile::new(&layout);

        let error = record.record(&graph()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::GraphRecord);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("writing {}: ", layout.graph().display())),
            "{diagnostic}"
        );
        assert_eq!(entries(&layout), ["graph.json"]);
    }
}
