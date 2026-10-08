//! The pipeline: the phases run in sequence, each command a prefix of that sequence.
//!
//! Its own directory because it is the only module that names more than one phase: the phases
//! communicate through values, and the pipeline is what passes each one's output to the next.
//!
//! Responsibilities: [`Pipeline`].
//!
//! Non-responsibilities: any phase's logic, and any port's implementation.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod driver;

pub use driver::Pipeline;
