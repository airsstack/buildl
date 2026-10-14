//! Resolve, the second phase: from the declarations of every build file to the target graph.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: [`resolve`]; [`narrow`](fn@narrow), which cuts a graph down to the targets
//! one run was asked for; and [`reach`](fn@reach), the targets a query reaches.
//!
//! Non-responsibilities: evaluating build files and validating one declaration on its own,
//! which the phase before this one does; and anything about whether a target needs to run.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod assembly;
mod cycle;
mod index;
pub mod narrow;
pub mod reach;
mod references;
mod suggest;

#[cfg(test)]
mod fixtures;

pub use assembly::resolve;
pub use narrow::narrow;
pub use reach::reach;
