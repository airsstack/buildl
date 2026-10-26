//! The port through which a watching loop learns that files changed.
//!
//! Its own file because noticing a change means asking the operating system for file events,
//! which this crate cannot do, and settling a burst of them into one batch is the other half of
//! that work. What a batch means for any target stays with the loop.
//!
//! Responsibilities: [`Changes`].
//!
//! Non-responsibilities: implementing it. The real implementation belongs in an adapter crate.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::ChangeBatch;

/// Waits for files in the workspace to change.
pub trait Changes {
    /// Blocks until a burst of file events has settled, then returns the paths it named.
    ///
    /// Each call returns the changes seen since the last, so a path appears again only if it
    /// changed again.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter) when changes can no longer be watched.
    /// The caller stops watching on an error.
    fn next(&mut self) -> Result<ChangeBatch>;
}
