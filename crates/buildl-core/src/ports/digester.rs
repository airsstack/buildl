//! The port through which Plan reads a file's metadata and content digest.
//!
//! Its own file because both mean touching the filesystem, which this crate never does. The two
//! are separate calls because they cost differently: metadata is cheap and asked of every input,
//! a digest reads the whole file and is asked only of the inputs whose metadata changed.
//!
//! Responsibilities: [`Digester`].
//!
//! Non-responsibilities: implementing it, and deciding which files need a digest. The first
//! belongs to an adapter crate; the second is the caller's.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{Digest, InputStat, SourcePath};

/// Reads the metadata and the content digests of workspace files.
pub trait Digester {
    /// What stands at `path`, following no symbolic link: nothing, a regular file with its
    /// metadata, or something that is not a regular file, with what it is.
    ///
    /// Only a regular file is an input. The other two answers are not failures of this port:
    /// the caller turns each into the error naming the target that declared the input.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Digester`](crate::PortName::Digester), when the metadata cannot be read.
    fn stat(&self, path: &SourcePath) -> Result<InputStat>;

    /// The content digest of each file in `paths`, in the order given.
    ///
    /// Takes every path at once so that an implementation may hash them in parallel.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Digester`](crate::PortName::Digester), when a file cannot be read.
    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>>;
}
