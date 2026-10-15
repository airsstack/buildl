//! Whether a run runs its tests again, whatever their keys say.
//!
//! Its own file because it is a choice a caller makes about one run, as a
//! [`Scope`](crate::types::Scope) is, that is neither in the workspace nor part of any target's
//! key: the same build files, rows and inputs give the same keys either way, and only what is done
//! with a test's matching row differs.
//!
//! Responsibilities: [`Rerun`].
//!
//! Non-responsibilities: deciding which targets a rerun reaches, which the phases that plan and
//! execute do.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// Whether a run runs its tests again.
///
/// An invocation value: it is never a component of an action key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rerun {
    /// A test whose last run still stands is served from its row, like any other target.
    Off,
    /// Every test runs again, whatever its key, and its new result is recorded.
    Tests,
}
