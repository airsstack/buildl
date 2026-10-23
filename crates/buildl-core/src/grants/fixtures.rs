//! Values built by hand, shared by the unit tests of this module and of Plan.
//!
//! Its own file because several test modules need the same builders. Compiled for tests only,
//! so it carries no test module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::types::{
    Argument, Command, Directory, EnvName, Freshness, Label, NetworkAccess, Node, NodeId, Origin,
    OutputName, OutputPath, Program, Provenance, SettingName, SettingValue, SourcePath,
    TargetGraph, TargetRole,
};

/// A directory from known-valid text.
pub(crate) fn dir(raw: &str) -> Directory {
    Directory::parse(raw).unwrap()
}

/// An environment variable name from known-valid text.
pub(crate) fn env_name(raw: &str) -> EnvName {
    EnvName::parse(raw).unwrap()
}

/// A label from known-valid text.
pub(crate) fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

/// Output names from known-valid text, in the order given.
pub(crate) fn outputs(names: &[&str]) -> Vec<OutputName> {
    names
        .iter()
        .map(|name| OutputName::parse(*name).unwrap())
        .collect()
}

/// A workspace file path from known-valid text.
pub(crate) fn path(raw: &str) -> SourcePath {
    SourcePath::parse(raw).unwrap()
}

/// A program name from known-valid text.
pub(crate) fn program(raw: &str) -> Program {
    Program::parse(raw).unwrap()
}

/// Where the output `name` of the target `raw` lives in a workspace whose outputs go under
/// `out`.
pub(crate) fn output_path(raw: &str, name: &str) -> OutputPath {
    OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
}

/// Build settings from known-valid `(name, value)` pairs.
pub(crate) fn settings(pairs: &[(&str, &str)]) -> BTreeMap<SettingName, SettingValue> {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                SettingName::parse(*name).unwrap(),
                SettingValue::parse(*value).unwrap(),
            )
        })
        .collect()
}

/// A command from known-valid arguments.
pub(crate) fn command(arguments: &[&str]) -> Command {
    Command::new(
        arguments
            .iter()
            .map(|argument| Argument::parse(*argument).unwrap())
            .collect(),
    )
    .unwrap()
}

/// A build target running `cc`, with one output named after it and nothing else, declared by
/// the build file of its label's directory, after `change` is applied.
pub(crate) fn node(raw: &str, change: impl FnOnce(&mut Node)) -> Node {
    let label = label(raw);
    let directory = label.directory().clone();
    let file = if directory.is_root() {
        PathBuf::from("build.lua")
    } else {
        PathBuf::from(format!("{directory}/build.lua"))
    };
    let mut node = Node {
        provenance: Provenance::new(file, directory),
        origin: Origin::BuildFile,
        narrowing: Vec::new(),
        role: TargetRole::Build,
        run: command(&["cc"]),
        description: None,
        inputs: Vec::new(),
        outputs: outputs(&[label.name().as_str()]),
        env: Vec::new(),
        network: NetworkAccess::Sealed,
        freshness: Freshness::Cached,
        label,
    };
    change(&mut node);
    node
}

/// A graph of `nodes`, which must be in label order. `edges` holds one list per node, in the
/// same order: the positions of the nodes it depends on.
pub(crate) fn graph(nodes: Vec<Node>, edges: &[&[u32]]) -> TargetGraph {
    let edges = edges
        .iter()
        .map(|deps| deps.iter().copied().map(NodeId::new).collect())
        .collect();
    TargetGraph::new(nodes, edges, BTreeMap::new(), BTreeMap::new())
}
