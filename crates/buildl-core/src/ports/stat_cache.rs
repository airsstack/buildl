//! The port through which Plan keeps the digests of files it has already hashed.
//!
//! Its own file because the record outlives a run, so it is stored somewhere, and storing is
//! not this crate's to do. What is recorded, and when a record may be trusted, stay with the
//! caller.
//!
//! Responsibilities: [`StatCache`].
//!
//! Non-responsibilities: implementing it, and the rule for reusing an entry. The first belongs
//! to an adapter crate; the second is the caller's.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{SourcePath, StatEntry};

/// Holds, for each file hashed before, its digest and the metadata it was hashed under.
pub trait StatCache {
    /// What was recorded for `path`, if anything.
    fn entry(&self, path: &SourcePath) -> Option<StatEntry>;

    /// Replaces the entry for `path`, in memory.
    fn record(&mut self, path: SourcePath, entry: StatEntry);

    /// Persists every entry.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::StatCache`](crate::PortName::StatCache), when the entries cannot be stored.
    fn commit(&mut self) -> Result<()>;
}
