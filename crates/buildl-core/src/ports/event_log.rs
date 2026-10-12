//! The port through which a build writes down how each target ended.
//!
//! Its own file because a log is kept somewhere a later reader can find it, and this crate keeps
//! nothing: it hands over one event per target and lets an adapter decide where events go.
//!
//! Responsibilities: [`EventLog`].
//!
//! Non-responsibilities: implementing it, and building the events. The first belongs to an
//! adapter crate; the second to the phase that runs actions.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::OutcomeEvent;

/// Receives the record of every target a build handled, one event at a time.
pub trait EventLog {
    /// Appends one event.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::EventLog`](crate::PortName::EventLog), when the event cannot be written.
    fn append(&mut self, event: &OutcomeEvent) -> Result<()>;
}
