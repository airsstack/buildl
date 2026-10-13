//! The port through which a build stores what actions produce, by content.
//!
//! Its own file because a store keeps bytes somewhere and puts files where a workspace expects
//! them, and this crate does neither: it hands over bytes, asks whether a digest is held, reads
//! held bytes back, and names the path a file belongs at.
//!
//! Responsibilities: [`ContentStore`].
//!
//! Non-responsibilities: implementing it, and deciding what is stored or placed. The first
//! belongs to an adapter crate; the second to the phase that runs actions.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{Digest, FileMode, OutputPath};

/// Holds content by its digest, and makes held content a file of the workspace.
///
/// What is held is bytes, identified by their digest and by nothing else: whether a file may be
/// run is not part of what is stored, and is said again each time the content is placed.
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

    /// The content held under that digest; `None` when it is not held.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the content is held and
    /// cannot be read.
    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>>;

    /// Removes whatever is at `at`: a file, or a directory with everything below it. Nothing
    /// being there is not an error.
    ///
    /// A build clears an output's path before it places the output there, and once more when
    /// it could not place all of it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ContentStore`](crate::PortName::ContentStore), when what is there cannot be
    /// removed.
    fn clear(&self, at: &OutputPath) -> Result<()>;

    /// Makes the content with that digest the file at `at`, one that may be run or not as `mode`
    /// says, making every directory above it that is missing and replacing a file already
    /// there.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ContentStore`](crate::PortName::ContentStore), when the content is not held or
    /// the file cannot be made.
    fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()>;
}
