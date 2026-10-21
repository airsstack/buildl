//! How many of a workspace's cached targets a verify re-runs.
//!
//! Its own file because it is a choice a caller makes about one run, as a
//! [`Rerun`](crate::types::Rerun) is, that is neither in the workspace nor part of any target's
//! key.
//!
//! Responsibilities: [`Sample`].
//!
//! Non-responsibilities: choosing which targets a sample holds, which the `verify` module
//! does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use core::num::NonZeroUsize;

/// How many candidates a verify re-runs.
///
/// An invocation value: it is never a component of an action key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sample {
    /// Every candidate is re-run.
    All,
    /// Only this many candidates are re-run, the ones with the lowest key digests.
    Lowest(NonZeroUsize),
}
