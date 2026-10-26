//! What a watching run decides from the changes it sees.
//!
//! Its own directory because it is neither a phase nor a type: it holds the decisions a watcher
//! makes between builds, which belong to no value in [`types`](crate::types) and to no phase.
//!
//! Responsibilities: [`react`], which says what a batch of changed paths starts, [`watch`], the
//! loop that runs a build, waits for changes and answers each batch with [`react`]'s verdict, and
//! the input set of a build, which is the files whose change a watcher must see.
//!
//! Non-responsibilities: noticing the changes, which a [`Changes`](crate::Changes) adapter does;
//! and running a build or a check, which a [`Cycles`](crate::Cycles) adapter does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod inputs;
mod react;
mod run;

pub(crate) use inputs::watched_inputs;
pub use react::react;
pub use run::watch;
