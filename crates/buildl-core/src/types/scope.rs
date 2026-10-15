//! Which targets of a workspace a command works on.
//!
//! Its own file because it is a choice a caller makes about one run that is not in the
//! workspace: the same build files are loaded either way, and only how much of their graph is
//! planned and built differs.
//!
//! Responsibilities: [`Scope`].
//!
//! Non-responsibilities: finding what the named targets need, which the phase that builds the
//! graph does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use std::collections::BTreeSet;

use crate::types::Label;

/// The part of a workspace's graph one run works on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scope {
    /// Every target the run's purpose means: every target but tests for a build, every test for a
    /// test run.
    #[default]
    Workspace,
    /// The targets these labels name, each directly or as an alias, and everything they depend
    /// on. No label is no target.
    Targets(BTreeSet<Label>),
}
