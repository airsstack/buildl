//! The build log on disk: how each target of each build ended, one line per target.
//!
//! Its own module because the log is kept apart from the rest of a workspace's state: it is an
//! artifact a person reads, it grows across builds, and nothing a build decides is read from it.
//!
//! Responsibilities: [`LocalLog`], the [`EventLog`](buildl_core::EventLog) of a local workspace.
//!
//! Non-responsibilities: building the events, which the build does; and where the state
//! directory keeps the log, which [`StateLayout`](crate::state::StateLayout) says.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod local_log;

pub use local_log::LocalLog;
