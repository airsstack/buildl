//! The port through which Plan learns about the machine actions run on.
//!
//! Its own file because the platform and the environment are ambient facts, and this crate
//! reads neither: an action's key depends on both, so both arrive through one port that names
//! where they come from.
//!
//! Responsibilities: [`Host`].
//!
//! Non-responsibilities: implementing it, and deciding which variables may be read. The first
//! belongs to an adapter crate; the second is decided before this port is asked anything.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{EnvName, EnvValue, Platform};

/// Reports the platform actions run on, and the value of an environment variable.
pub trait Host {
    /// The operating system and architecture actions run on.
    fn platform(&self) -> Platform;

    /// The value of the variable `name`; `Ok(None)` when it is unset.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Host`](crate::PortName::Host), when the variable holds a value an
    /// [`EnvValue`] cannot.
    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>;
}
