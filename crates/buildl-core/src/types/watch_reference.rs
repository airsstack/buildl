//! What a watching run knows about the workspace between cycles.
//!
//! Its own file because these are the values a cycle is compared against: the layout the last
//! open of the workspace read, and the graph and inputs the last build ran on. They are plain
//! data that the watcher holds and replaces; none of them decides anything.
//!
//! Responsibilities: [`WatchLayout`], [`Built`] and [`Reference`].
//!
//! Non-responsibilities: reading the layout or running the build, and deciding what a change
//! means against either.
//!
//! This file holds only type definitions, so it carries no logic to unit-test, per the
//! unit-test mandate's exemption for logic-free files.

use std::collections::BTreeSet;

use crate::types::{ChangedPath, Directory, SourcePath, TargetGraph};

/// What the last open of the workspace read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchLayout {
    /// The directory build outputs are placed in.
    pub out: Directory,
    /// The directory the build's own state is kept in.
    pub state: Directory,
    /// The files whose content decides the workspace's settings: `buildl.toml`, `buildl.lock`
    /// and the `extension.toml` of each plugin kept in the workspace.
    pub config: BTreeSet<ChangedPath>,
}

/// What a build ran on: its target graph and the source files it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    /// The runnable targets and the dependencies between them.
    pub graph: TargetGraph,
    /// The declared inputs of the targets the build's scope selected and what they need, and
    /// of every discovery target and what it needs. An input of a target outside the scope is
    /// not here.
    pub inputs: BTreeSet<SourcePath>,
}

/// What a cycle is compared against: the layout, and the build that last ran if one did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// What the last open of the workspace read.
    pub layout: WatchLayout,
    /// What the last build ran on, or `None` when no build has run yet.
    pub built: Option<Built>,
}
