//! The one way a value in this crate becomes JSON bytes.
//!
//! Its own module because every byte of JSON this crate emits must come from one place: the
//! graph file, the cache file, the event log and the bytes hashed into an action key all go
//! through it, so the key of a value and the file of a value can never disagree.
//!
//! Responsibilities: [`canonical`] — sorted-key, float-free serialization.
//!
//! Non-responsibilities: writing anything anywhere. This module returns bytes; a storage adapter
//! decides where they land.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod canonical;
