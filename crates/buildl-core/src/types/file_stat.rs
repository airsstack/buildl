//! A file's metadata, and a digest recorded beside the metadata it was taken under.
//!
//! Its own file because the pair answers one question without reading a file: whether it changed
//! since it was last hashed. Hashing is what makes a warm build slow, so a digest is kept with
//! the metadata that vouches for it.
//!
//! Responsibilities: [`FileStat`] and [`StatEntry`].
//!
//! Non-responsibilities: reading metadata, hashing, and deciding whether a recorded digest may
//! be reused. The first two arrive through ports; the third belongs to the phase that plans.
//!
//! This file holds only type definitions, so it carries no logic to unit-test.

use crate::types::{Digest, Timestamp};

/// What decides whether a file changed: when it was last modified, and how long it is.
///
/// Two values are compared for equality, never for order: a modification time earlier than a
/// recorded one is as much a change as a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// When the file was last modified.
    pub modified: Timestamp,
    /// The file's length in bytes.
    pub size: u64,
}

/// A file's content digest, and the metadata the file had when the digest was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatEntry {
    /// The metadata the digest was taken under.
    pub stat: FileStat,
    /// The digest of the file's content.
    pub digest: Digest,
}
