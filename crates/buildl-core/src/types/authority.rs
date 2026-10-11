//! Authority as values: what a workspace allows, and what a build wants.
//!
//! Its own file because nothing in a build is ambient: what may run, which variables may be
//! read and which directories may be touched are written down once for a workspace, what a
//! build needs is derived from its targets, and the two are compared before anything runs.
//!
//! Responsibilities: [`Ceiling`], [`WantedSet`] and [`Want`].
//!
//! Non-responsibilities: the comparison. Deciding whether a want is inside a ceiling needs both
//! values and the graph, so it belongs to the phase that plans; nothing here grants or refuses.
//!
//! This file holds only type definitions, so it carries no logic to unit-test.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::types::{Directory, EnvName, Label, Program, SourcePath};

/// The most a workspace allows any build to do.
///
/// Four lists, each empty unless the workspace says otherwise: an empty ceiling allows nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ceiling {
    /// The programs a command may run.
    pub run: BTreeSet<Program>,
    /// The environment variables a target may read.
    pub env: BTreeSet<EnvName>,
    /// The directories a target's inputs may lie in.
    pub read: BTreeSet<Directory>,
    /// The directories a build may write into.
    pub write: BTreeSet<Directory>,
}

/// What one build wants, derived from its targets and its workspace.
///
/// The files a build reads are not copied here: they are each target's declared inputs, and stay
/// in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WantedSet {
    /// Every program a command names.
    pub programs: BTreeSet<Program>,
    /// Every environment variable a target reads.
    pub env: BTreeSet<EnvName>,
    /// Every directory the build writes into.
    pub write: BTreeSet<Directory>,
    /// Every target that declared it reaches the network.
    pub network: BTreeSet<Label>,
}

/// One thing a build wants that a ceiling is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Want {
    /// To run a program.
    Program(Program),
    /// To read an environment variable.
    Env(EnvName),
    /// To read a workspace file as an input.
    Read(SourcePath),
    /// To write into a directory.
    Write(Directory),
}
