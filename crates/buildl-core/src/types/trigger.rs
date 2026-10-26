//! What started a watching cycle.
//!
//! Its own file because it is the value a cycle is told on entry, apart from the reference it
//! is compared against and the result it returns.
//!
//! Responsibilities: [`Trigger`].
//!
//! Non-responsibilities: collecting the changes.
//!
//! This file holds only a type definition, so it carries no logic to unit-test, per the
//! unit-test mandate's exemption for logic-free files.

use crate::types::ChangeBatch;

/// What a cycle was started by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// The watch began; no change has been seen.
    Start,
    /// A burst of file events named these paths.
    Changed(ChangeBatch),
}
