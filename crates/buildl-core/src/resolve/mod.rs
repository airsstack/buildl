//! Resolve, the second phase: from the declarations of every build file to the target graph.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: [`resolve`]; and [`narrow`](fn@narrow), which cuts a graph down to the
//! targets one run was asked for.
//!
//! Non-responsibilities: evaluating build files and validating one declaration on its own,
//! which the phase before this one does; and anything about whether a target needs to run.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod assembly;
mod cycle;
mod index;
pub mod narrow;
mod references;
mod suggest;

#[cfg(test)]
mod fixtures;

pub use assembly::resolve;
pub use narrow::narrow;
