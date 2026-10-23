//! The plugins a workspace names: the lock that pins them and the digest of a stored tree.
//!
//! Its own module because the manifest adapter reads which plugins a workspace names and the
//! checks that follow read the lock and the store, and no adapter may name another. What both
//! sides share is the values in `buildl-core`; what the lock and the store hold is kept here.
//! This module implements no port, and names only [`state`](crate::state) of the other modules
//! of this crate.
//!
//! Responsibilities:
//! - [`Lock`] and [`LockEntry`] — reading and writing `buildl.lock`.
//! - [`tree_digest`] — the one digest of a directory of plugin files.
//! - [`check`] and [`Checked`] — finding every named plugin ready to use, from the lock and the
//!   store.
//!
//! Non-responsibilities: fetching a plugin, and deciding what a plugin may do, which are the
//! callers'.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod check;
mod lock;
mod tree_digest;

pub use check::{Checked, check};
pub use lock::{Lock, LockEntry};
pub use tree_digest::tree_digest;
