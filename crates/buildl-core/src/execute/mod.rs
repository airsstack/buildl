//! Execute, the fourth phase: from a plan to a finished build.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//! Recording what an action produced is the last step of running it, so it lives here too.
//!
//! Responsibilities: [`execute`].
//!
//! Non-responsibilities: planning the build, which the phase before this one does; and running
//! a command, storing content, keeping rows or writing the log, which are each a port's.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod admission;
pub mod assembly;
mod expand;
pub(crate) mod layout;
pub(crate) mod perform;
pub(crate) mod record;
mod schedule;
pub(crate) mod settle;

#[cfg(test)]
pub(crate) mod fixtures;

pub use assembly::execute;
