//! What one watching cycle ended with.
//!
//! Its own file because a cycle's result joins a build's result with the layout read to start
//! it, and the two are produced by different steps.
//!
//! Responsibilities: [`Watched`] and [`Cycle`].
//!
//! Non-responsibilities: running the build or opening the workspace.
//!
//! This file holds only type definitions, so it carries no logic to unit-test, per the
//! unit-test mandate's exemption for logic-free files.

use crate::error::Result;
use crate::types::{ActionOutcome, Built, WatchLayout};

/// One build and the graph it ran on.
#[derive(Debug)]
pub struct Watched {
    /// What the build ran on, or `None` when it ended before its graph existed: a manifest,
    /// load, resolve or discovery failure.
    pub built: Option<Built>,
    /// One outcome per target, or the error that ended the build.
    pub ended: Result<Vec<ActionOutcome>>,
}

/// What one cycle ended with.
#[derive(Debug)]
pub struct Cycle {
    /// What the cycle read of the workspace, or `None` when the workspace could not be opened.
    pub layout: Option<WatchLayout>,
    /// The build the cycle ran.
    pub watched: Watched,
}
