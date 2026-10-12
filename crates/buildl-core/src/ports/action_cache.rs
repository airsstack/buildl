//! The port through which the pipeline reads and writes what a build recorded for a target.
//!
//! Its own file because the rows outlive a run, so they are stored somewhere, and this crate
//! stores nothing. A plan reads the row a target's last build left; a build writes a new one
//! once the target has been built again.
//!
//! Responsibilities: [`ActionCache`].
//!
//! Non-responsibilities: implementing it, and deciding when a row is written. The first belongs
//! to an adapter crate; a row is written only for a target whose outputs are already stored.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{CacheRow, Label};

/// Holds the row the last successful build of each target recorded.
pub trait ActionCache {
    /// The row for the target `label` names, if it has one: the one a previous build left, or
    /// the one [`record`](ActionCache::record) last stored.
    fn row(&self, label: &Label) -> Option<&CacheRow>;

    /// Stores `row` durably as the row of the target `label` names.
    ///
    /// One call writes one row, and the row is kept once the call has returned `Ok`: a build
    /// that dies afterwards does not lose it. After `Ok`, [`row`](ActionCache::row) returns it.
    /// After `Err`, [`row`](ActionCache::row) returns what it returned before the call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ActionCache`](crate::PortName::ActionCache), when the row cannot be stored.
    fn record(&mut self, label: Label, row: CacheRow) -> Result<()>;
}
