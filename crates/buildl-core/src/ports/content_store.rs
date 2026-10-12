//! The port through which a build stores what actions produce, by content.
//!
//! Its own file because a store keeps bytes somewhere and puts files where a workspace expects
//! them, and this crate does neither: it hands over bytes, asks whether a digest is held, and
//! names the path an output belongs at.
//!
//! Responsibilities: [`ContentStore`].
//!
//! Non-responsibilities: implementing it, and deciding what is stored or placed. The first
//! belongs to an adapter crate; the second to the phase that runs actions.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{Digest, OutputPath};

/// Holds content by its digest, and makes held content a file of the workspace.
///
/// `Send + Sync` because several actions store what they produced at once, each from the thread
/// that ran it.
pub trait ContentStore: Send + Sync {
    /// Stores `bytes` and returns their digest. Storing bytes already held is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the bytes cannot be stored.
    fn put(&self, bytes: &[u8]) -> Result<Digest>;

    /// Whether content with that digest is held.
    fn contains(&self, digest: &Digest) -> bool;

    /// Makes the content with that digest the file at `at`, replacing what is there.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the content is not held or
    /// the file cannot be made.
    fn place(&self, digest: &Digest, at: &OutputPath) -> Result<()>;
}
