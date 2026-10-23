//! The declarations a command loaded, with the discovery targets whose result it lacked.
//!
//! Its own file because a command that reads discovery results hands two things back together:
//! what the build files declared, and which discovery targets had no result to declare from, so
//! the caller can say so.
//!
//! Responsibilities: [`Loaded`].
//!
//! Non-responsibilities: reading the results, which the `discovery` module does; and showing the
//! labels, which the caller does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use crate::types::{Declaration, Label};

/// The declarations of a workspace once its discovery results were read, and the discovery
/// targets that had none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The validated, sorted declarations: those of the second load when a discovery target is
    /// declared, otherwise those of the only load.
    pub declarations: Vec<Declaration>,
    /// The discovery targets with no result to read, in label order, each once.
    pub missing: Vec<Label>,
}
