//! What a run is for.
//!
//! Its own file because it is a choice a caller makes about one run that is not in the
//! workspace: the same build files are loaded either way, and only which targets a whole
//! workspace means differs.
//!
//! Responsibilities: [`Purpose`].
//!
//! Non-responsibilities: choosing the targets, which the phase that narrows the graph does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// What a run is for, which decides the targets a whole workspace means and which targets the
/// run may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// A build or a plan: a whole workspace means every target but tests, and a run may name any
    /// target.
    Build,
    /// A test run: a whole workspace means every test, and a run may name only tests.
    Test,
}
