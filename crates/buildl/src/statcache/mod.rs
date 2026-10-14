//! The stat cache on disk: the digest of each file already hashed, beside the metadata it was
//! hashed under.
//!
//! Its own module because the record is an optimisation and nothing more: it holds nothing a
//! hash cannot give again, no artifact depends on it, and a file that cannot be trusted is
//! thrown away instead of stopping a run.
//!
//! Responsibilities: [`LocalStatCache`], the [`StatCache`](buildl_core::StatCache) of a local
//! workspace.
//!
//! Non-responsibilities: deciding when an entry may be reused, which Plan does; and where
//! the state directory keeps the record, which [`StateLayout`](crate::state::StateLayout) says.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod local_stat_cache;

pub use local_stat_cache::LocalStatCache;
