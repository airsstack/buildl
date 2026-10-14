//! The port through which a build writes down the graph it resolved.
//!
//! Its own file because the graph a build worked from is kept where a later reader can find it,
//! and this crate keeps nothing: it hands over the graph and lets an adapter store it.
//!
//! Responsibilities: [`GraphRecord`].
//!
//! Non-responsibilities: implementing it, and deciding when a graph is recorded. The first
//! belongs to an adapter crate; the second to the pipeline.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::TargetGraph;

/// Keeps the graph the last build resolved.
pub trait GraphRecord {
    /// Replaces what is kept with `graph`.
    ///
    /// An implementation stores the bytes
    /// [`json::canonical::to_vec`](crate::json::canonical::to_vec) writes for `graph`, and no
    /// other byte, so the same workspace always leaves the same record. A reader finds the old
    /// record whole or the new one whole, never part of either.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::GraphRecord`](crate::PortName::GraphRecord), when the graph cannot be stored.
    fn record(&mut self, graph: &TargetGraph) -> Result<()>;
}
