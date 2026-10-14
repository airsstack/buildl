//! The port through which the pipeline learns what a workspace's manifest says.
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
    /// The workspace's directories, its ceiling, and how it wants its actions run.
    ///
    /// The phases read the directories and the ceiling. The execution settings,
    /// [`Workspace::execution`], are read by whoever assembles the adapters that run a build:
    /// its worker count for the dispatcher, its timeout and its strategy for what runs one
    /// action.
    ///
    /// An implementation builds each execution setting from the manifest's own text through
    /// that setting's type — [`Workers::parse`](crate::Workers::parse) or
    /// [`Workers::of`](crate::Workers::of), [`Timeout::parse`](crate::Timeout::parse) and
    /// [`Isolation::parse`](crate::Isolation::parse) — and gives a setting the manifest leaves
    /// out its type's default. Text a type rejects makes the manifest not valid.
    ///
    /// Every call on one instance returns the same value: Plan and Execute each ask, and the
    /// directories a build writes under are the ones its ceiling was checked for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Manifest`](crate::PortName::Manifest), when the manifest cannot be read or is
    /// not valid.
    fn workspace(&self) -> Result<Workspace>;
}
