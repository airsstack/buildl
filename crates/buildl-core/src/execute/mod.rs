//! Execute, the fourth phase: from a plan to a finished build.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//! Recording what an action produced is the last step of running it, so it lives here too.
//!
//! Responsibilities: [`execute`]; `remote_action`, which describes one task for a remote cache;
//! `fetch`, which downloads and checks a result the remote holds, then keeps it in the store;
//! and `upload`, which describes a run for the remote cache and sends it.
//!
//! Non-responsibilities: planning the build, which the phase before this one does; and running
//! a command, storing content, keeping rows or writing the log, which are each a port's.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod admission;
pub mod assembly;
pub(crate) mod fetch;
pub(crate) mod perform;
pub(crate) mod record;
pub(crate) mod remote_action;
mod schedule;
pub(crate) mod settle;
pub(crate) mod upload;

#[cfg(test)]
pub(crate) mod fixtures;

pub use assembly::execute;
