//! The read side of the build log: which of a target's logged runs a reader is shown.
//!
//! Its own directory because it is neither a phase nor a type: it decides between lines of a log
//! that has already been read, and a decision between lines belongs to no value in
//! [`types`](crate::types).
//!
//! Responsibilities: [`LastRun`], the newest run of a target that left output; and
//! `logged_outputs`, the blobs those runs name, which the log still reads.
//!
//! Non-responsibilities: reading the log file, which a [`RunHistory`](crate::RunHistory) adapter
//! does; and showing the run, which the caller of [`pipeline::log`](fn@crate::pipeline::log)
//! does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod last_run;
mod logged_outputs;

pub use last_run::LastRun;
pub(crate) use last_run::last_run;
#[expect(unused_imports, reason = "no caller yet")]
pub(crate) use logged_outputs::logged_outputs;
