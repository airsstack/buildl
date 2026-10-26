//! Watching a workspace's files through the operating system's file events.
//!
//! Its own module because asking the operating system for file events is the one place this
//! crate names the `notify` library, and settling a burst of events into one batch is the other
//! half of that work. What a batch means for any target stays with the loop that reads it.
//!
//! Responsibilities: [`NotifyChanges`], the implementation of `buildl_core::Changes`.
//!
//! Non-responsibilities: deciding which paths matter. Only a path outside the workspace, the
//! workspace root itself, and a path that is not a valid changed path are dropped here.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod notify_changes;

pub use notify_changes::NotifyChanges;
