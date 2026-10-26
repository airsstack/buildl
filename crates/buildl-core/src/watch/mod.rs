//! What a watching run decides from the changes it sees.
//!
//! Its own directory because it is neither a phase nor a type: it holds the decisions a watcher
//! makes between builds, which belong to no value in [`types`](crate::types) and to no phase.
//!
//! Responsibilities: [`react`], which says what a batch of changed paths starts.
//!
//! Non-responsibilities: noticing the changes, which a [`Changes`](crate::Changes) adapter does;
//! and running anything a reaction names.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod react;

pub use react::react;
