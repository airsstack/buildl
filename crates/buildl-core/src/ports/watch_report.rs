//! The port through which a watching loop tells a person what it is doing.
//!
//! Its own file because telling means writing to a terminal, which this crate does not do. The
//! loop says which of three things is true; an adapter decides how that looks.
//!
//! Responsibilities: [`WatchReport`].
//!
//! Non-responsibilities: implementing it, and showing the targets a build ran. The second is the
//! build's own reporter.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Error;
use crate::types::Trigger;

/// Is told what a watching loop is doing, one call at a time.
///
/// No call can fail: what a report cannot show is its own to deal with, and changes nothing the
/// loop does.
pub trait WatchReport {
    /// A cycle is starting, begun by `trigger`.
    fn building(&mut self, trigger: &Trigger);

    /// A cycle ended with `error`, and the loop goes on.
    fn failed(&mut self, error: &Error);

    /// The loop has nothing to do and is waiting for the next change.
    fn waiting(&mut self);
}
