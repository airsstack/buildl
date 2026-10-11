//! What a workspace's manifest tells the pipeline.
//!
//! Its own file because these are the facts about a workspace that no build file states: where
//! a build puts what it produces, where buildl keeps its own files, and what the workspace
//! allows.
//!
//! Responsibilities: [`Workspace`].
//!
//! Non-responsibilities: reading a manifest, which is a port's.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use crate::types::{Ceiling, Directory};

/// A workspace as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The directory named outputs are linked into.
    pub out: Directory,
    /// The directory buildl keeps its own files in: its caches, its content store, its log.
    pub state: Directory,
    /// What the workspace allows a build to do.
    pub ceiling: Ceiling,
}
