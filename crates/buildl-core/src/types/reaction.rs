//! What a watching run does about a batch of changes.
//!
//! Its own file because it is the answer a classification gives and a cycle acts on, a
//! different concept from the changes that prompt it.
//!
//! Responsibilities: [`Reaction`].
//!
//! Non-responsibilities: deciding which reaction a change calls for.
//!
//! This file holds only a type definition, so it carries no logic to unit-test, per the
//! unit-test mandate's exemption for logic-free files.

/// What a cycle does once it knows what changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Run the build again.
    Build,
    /// Load and resolve the build files again without running anything, to learn whether the
    /// graph changed.
    Check,
    /// Do nothing.
    Ignore,
}
