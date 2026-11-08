//! The port through which Plan learns about the machine actions run on.
//!
//! Its own file because the platform, the environment and the machine's own folders are ambient
//! facts, and this crate reads none of them: an action's key depends on the first two, and the
//! third tells what a key must not hold, so all arrive through one port that names where they
//! come from.
//!
//! Responsibilities: [`Host`].
//!
//! Non-responsibilities: implementing it, and deciding which variables may be read. The first
//! belongs to an adapter crate; the second is decided before this port is asked anything.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{EnvName, EnvValue, MachineFolders, Platform};

/// Reports the platform actions run on, the value of an environment variable, and the folders
/// of this machine.
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

    /// This machine's home, workspace and sandbox folders, each in the forms it is known by.
    ///
    /// A host that cannot tell reports [`MachineFolders::default`], no folders.
    fn machine_folders(&self) -> MachineFolders;
}
