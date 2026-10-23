//! The read side of the build log: which of a target's logged runs a reader is shown.
//!
//! Its own directory because it is neither a phase nor a type: it decides between lines of a log
//! that has already been read, and a decision between lines belongs to no value in
//! [`types`](crate::types).
//!
//! Responsibilities: [`LastRun`], the newest run of a target that left output; `last_success`,
//! the one output a target's newest successful run produced; and `logged_outputs`, the blobs
//! those runs name, which the log still reads.
//!
//! Non-responsibilities: reading the log file, which a [`RunHistory`](crate::RunHistory) adapter
//! does; and showing the run, which the caller of [`pipeline::log`](fn@crate::pipeline::log)
//! does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod last_run;
mod last_success;
mod logged_outputs;

pub use last_run::LastRun;
pub(crate) use last_run::last_run;
pub(crate) use last_success::last_success;
pub(crate) use logged_outputs::logged_outputs;
