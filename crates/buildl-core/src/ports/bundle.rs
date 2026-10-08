//! The bundle that names one implementation of every port a pipeline uses.
//!
//! Its own file because it is a different kind of trait from the ports themselves: it has no
//! methods, only associated types, and it exists so that a pipeline takes one type parameter
//! instead of one per port.
//!
//! Responsibilities: [`Ports`].
//!
//! Non-responsibilities: choosing the implementations. A composition root does that, by
//! implementing this trait.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::ports::DeclarationSource;

/// One implementation of each port, chosen together.
///
/// # Examples
///
/// ```
/// use buildl_core::{BuildFile, DeclarationSource, Evaluated, Ports, Result};
///
/// /// A source with no build files at all.
/// struct Empty;
///
/// impl DeclarationSource for Empty {
///     fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
///         Ok(Evaluated::Absent)
///     }
/// }
///
/// /// A bundle choosing that source.
/// struct EmptyPorts;
///
/// impl Ports for EmptyPorts {
///     type Source = Empty;
/// }
/// ```
pub trait Ports {
    /// How build files are evaluated.
    type Source: DeclarationSource;
}
