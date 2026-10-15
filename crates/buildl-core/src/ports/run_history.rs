//! The port through which a reader gets a build log back.
//!
//! Its own file because the two sides of a log never share a method: Execute appends through
//! [`EventLog`](crate::EventLog), a reader reads through this, and neither carries the other's
//! method.
//!
//! Responsibilities: [`RunHistory`].
//!
//! Non-responsibilities: implementing it, and choosing which line a reader wants. The first
//! belongs to an adapter crate; the second to the module that reads the log.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::LogLine;

/// Hands over the lines of a build log.
pub trait RunHistory {
    /// The lines of a build log, oldest first: each whole line an event or a damaged line.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::RunHistory`](crate::PortName::RunHistory), when the log cannot be read.
    fn lines(&self) -> Result<Vec<LogLine>>;
}
