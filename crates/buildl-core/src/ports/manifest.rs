//! The port through which Plan learns what a workspace's manifest says.
//!
//! Its own file because the manifest is a file with a format, and this crate reads no file and
//! knows no format: what it needs from one arrives as a value.
//!
//! Responsibilities: [`Manifest`].
//!
//! Non-responsibilities: implementing it. Finding, reading and parsing a manifest belong to an
//! adapter crate.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::Workspace;

/// Reports what a workspace's manifest says.
pub trait Manifest {
    /// The workspace's directories and its ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Manifest`](crate::PortName::Manifest), when the manifest cannot be read or is
    /// not valid.
    fn workspace(&self) -> Result<Workspace>;
}
