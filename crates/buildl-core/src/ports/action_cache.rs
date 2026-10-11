//! The port through which Plan reads what the previous build recorded.
//!
//! Its own file because the rows outlive a run, so they are stored somewhere, and this crate
//! stores nothing. Reading one is all a plan needs.
//!
//! Responsibilities: [`ActionCache`].
//!
//! Non-responsibilities: implementing it, and writing rows. The first belongs to an adapter
//! crate; a row is written only after a target has been built.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::types::{CacheRow, Label};

/// Holds the row the last successful build of each target recorded.
pub trait ActionCache {
    /// The row recorded for the target `label` names, if it has one.
    fn row(&self, label: &Label) -> Option<&CacheRow>;
}
