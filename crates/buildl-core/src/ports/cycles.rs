//! The port through which a watching loop runs one cycle.
//!
//! Its own file because a cycle opens the workspace again and builds it, which needs every other
//! adapter at once. The loop decides when a cycle runs and what to say about it; this port does
//! the work.
//!
//! Responsibilities: [`Cycles`].
//!
//! Non-responsibilities: implementing it. The real implementation belongs in an adapter crate.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{Cycle, TargetGraph};

/// Runs the work of one watching cycle.
pub trait Cycles {
    /// Opens the workspace again and runs one build on it.
    ///
    /// The build's own failure is part of the returned [`Cycle`], not a way for this call to
    /// fail: a failed build is a cycle that ended, and the loop goes on.
    fn build(&mut self) -> Cycle;

    /// Opens the workspace again and loads its build files, without building anything.
    ///
    /// # Errors
    ///
    /// Returns the error that stopped the build files from loading.
    fn graph(&mut self) -> Result<TargetGraph>;
}
