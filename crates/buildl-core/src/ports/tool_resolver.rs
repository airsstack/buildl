//! The port through which Plan identifies the programs commands name.
//!
//! Its own file because finding a program means searching the machine, and what identifies one
//! — its content, or the version it reports — is the finder's to choose. This crate needs only
//! the answer: a value that changes when the tool does.
//!
//! Responsibilities: [`ToolResolver`].
//!
//! Non-responsibilities: implementing it, and deciding which programs may run. The first
//! belongs to an adapter crate; the second is decided before this port is asked anything.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{Digest, Program};

/// Finds programs, and reports what identifies each one.
pub trait ToolResolver {
    /// The fingerprint of `program`; `Ok(None)` when it cannot be found.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ToolResolver`](crate::PortName::ToolResolver), when the search itself fails.
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>>;
}
