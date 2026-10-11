//! Plan, the third phase: from the target graph to what a build of it would do.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: [`plan`].
//!
//! Non-responsibilities: building the graph, which the phase before this one does; and running
//! anything, or recording what a run produced.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod assembly;
mod authority;
mod components;
mod difference;
mod inputs;
mod order;
mod settings;
mod tools;
mod verdict;

#[cfg(test)]
mod fixtures;

pub use assembly::plan;
