//! Verify: re-running a workspace's cached targets and comparing what they produce.
//!
//! Its own directory because verify serves one command and is not a phase: like
//! [`history`](mod@crate::history), it decides things no value in [`types`](crate::types) owns,
//! and it may name the internals of a phase where a phase module may not.
//!
//! Responsibilities: [`verify`], which puts the cached targets back in place, re-runs a
//! [`Sample`](crate::Sample) of them and checks each against what was cached; the choice of
//! which targets a sample re-runs; and the walk that names where two manifests of one output
//! first differ.
//!
//! Non-responsibilities: running an action, storing content or reading the cache, which are each
//! a port's; and recording anything, which a verify never does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod assembly;
mod difference;
mod sample;

pub use assembly::verify;
pub(crate) use difference::first_difference;
pub(crate) use sample::{Chosen, chosen};
