//! The one way a value in this crate becomes JSON bytes, and the one way such bytes become a
//! value again.
//!
//! Its own module because every byte of JSON this crate emits must come from one place: the
//! graph file, the cache file, the event log and the bytes hashed into an action key all go
//! through it, so the key of a value and the file of a value can never disagree. Reading lives
//! beside writing for the same reason: what is read back is held to exactly what is written.
//!
//! Responsibilities: [`canonical`] — sorted-key, float-free serialization, and reading it back.
//!
//! Non-responsibilities: writing or reading anything anywhere. This module turns a value into
//! bytes and bytes into a value; a storage adapter decides where the bytes live.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod canonical;
