//! Values built by hand, shared by this phase's unit tests.
//!
//! Its own file because several test modules need the same builders. Compiled for tests only,
//! so it carries no test module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::execute::perform::Kept;
use crate::types::{
    ActionKey, Argument, CacheRow, Command, Digest, Directory, FileMode, Freshness, KeyComponents,
    Label, NetworkAccess, Node, NodeId, Origin, OutputManifest, OutputName, OutputPath,
    OutputSource, OwnComponents, Platform, Program, Provenance, SettingName, SettingValue,
    SourcePath, StoredFile, TargetGraph, TargetRole, Tree, TreeEntry, TreePath,
};

/// A directory from known-valid text.
pub(crate) fn dir(raw: &str) -> Directory {
    Directory::parse(raw).unwrap()
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

/// The own key components of a target running `cc` on `linux/x86_64` and declaring the outputs
/// `names`, with nothing else.
pub(crate) fn own(names: &[&str]) -> OwnComponents {
    OwnComponents {
        command: command(&["cc"]),
        inputs: Vec::new(),
        outputs: outputs(names),
        env: BTreeMap::new(),
        tool: None,
        settings: BTreeMap::new(),
        network: NetworkAccess::Sealed,
        output_source: OutputSource::Command,
        platform: Platform::parse("linux/x86_64").unwrap(),
    }
}

/// The key of a target with the components [`own`] gives `names` and no dependency.
pub(crate) fn key(names: &[&str]) -> ActionKey {
    ActionKey::new(KeyComponents {
        own: own(names),
        deps: Vec::new(),
    })
    .unwrap()
}

/// The manifest of an output that is one plain file holding `content`.
pub(crate) fn file(content: &str) -> OutputManifest {
    OutputManifest::File(StoredFile {
        digest: Digest::of(content.as_bytes()),
        mode: FileMode::Plain,
    })
}

/// The manifest of an output that is a directory of plain files, each a `(path, content)`.
pub(crate) fn tree(files: &[(&str, &str)]) -> OutputManifest {
    let entries = files
        .iter()
        .map(|(path, content)| TreeEntry {
            path: TreePath::parse(*path).unwrap(),
            digest: Digest::of(content.as_bytes()),
            mode: FileMode::Plain,
        })
        .collect();
    OutputManifest::Tree(Tree::new(entries).unwrap())
}

/// The digest of an output whose manifest is `manifest`: the digest of the manifest's bytes.
pub(crate) fn digest_of(manifest: &OutputManifest) -> Digest {
    Digest::of(&manifest.to_bytes().unwrap())
}

/// The output whose manifest is `manifest`, as a worker hands it back once that is stored.
pub(crate) fn kept(manifest: OutputManifest) -> Kept {
    Kept {
        digest: digest_of(&manifest),
        manifest,
    }
}

/// The row of a build under [`key`] in which each output was one plain file holding its own
/// name, and the action printed `out` and `err`.
pub(crate) fn row(names: &[&str]) -> CacheRow {
    CacheRow::new(
        key(names),
        names.iter().map(|name| digest_of(&file(name))).collect(),
        Digest::of(b"out"),
        Digest::of(b"err"),
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
        output_source: OutputSource::Command,
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
