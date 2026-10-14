//! The action cache on disk: the row each target's last successful build recorded.
//!
//! Its own module because it is the one place that decides how rows are kept across runs: a
//! canonical snapshot of every row, `cache.json`, and beside it a journal, `cache.journal`,
//! that takes one line for each row as it is recorded. A row is kept from the moment it is
//! recorded, and the snapshot is written once for a whole build.
//!
//! Responsibilities: [`LocalCache`], the [`ActionCache`](buildl_core::ActionCache) of a local
//! workspace.
//!
//! Non-responsibilities: deciding when a row is written, which the build does; and where the
//! state directory keeps the two files, which [`StateLayout`](crate::state::StateLayout) says.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod journal;
mod local_cache;

pub use local_cache::LocalCache;
