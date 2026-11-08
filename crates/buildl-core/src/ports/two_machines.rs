//! A fake [`Host`] standing for two machines that differ in the folders their variables name.
//!
//! Its own file because the shared stub of the port tests answers `None` for every variable, so
//! it cannot show that two machines give one key. Compiled for tests only, so it carries no test
//! module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;

use crate::error::Result;
use crate::ports::Host;
use crate::types::{EnvName, EnvValue, MachineFolders, Platform};

/// One machine's variables, on `linux/x86_64`.
#[derive(Debug, Clone)]
pub(crate) struct TwoMachines {
    values: BTreeMap<EnvName, EnvValue>,
}

impl TwoMachines {
    /// A machine that sets `HOME` and `PATH` as one user's does, and `RUSTFLAGS` as every
    /// machine of the pair does.
    pub(crate) fn first() -> Self {
        Self::of(&[
            ("HOME", "/home/alice"),
            ("PATH", "/usr/bin"),
            ("RUSTFLAGS", "-Cdebuginfo=0"),
        ])
    }

    /// A machine that sets `HOME` and `PATH` to other folders than [`first`](Self::first)'s, and
    /// `RUSTFLAGS` to the same value.
    pub(crate) fn second() -> Self {
        Self::of(&[
            ("HOME", "/Users/bob"),
            ("PATH", "/opt/bin:/usr/bin"),
            ("RUSTFLAGS", "-Cdebuginfo=0"),
        ])
    }

    /// This machine with `name` set to `value`.
    pub(crate) fn with(mut self, name: &str, value: &str) -> Self {
        self.values.insert(
            EnvName::parse(name).unwrap(),
            EnvValue::parse(value).unwrap(),
        );
        self
    }

    /// This machine with `name` unset.
    pub(crate) fn without(mut self, name: &str) -> Self {
        self.values.remove(&EnvName::parse(name).unwrap());
        self
    }

    fn of(pairs: &[(&str, &str)]) -> Self {
        let machine = Self {
            values: BTreeMap::new(),
        };
        pairs
            .iter()
            .fold(machine, |machine, (name, value)| machine.with(name, value))
    }
}

impl Host for TwoMachines {
    fn platform(&self) -> Platform {
        Platform::parse("linux/x86_64").unwrap()
    }

    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
        Ok(self.values.get(name).cloned())
    }

    fn machine_folders(&self) -> MachineFolders {
        MachineFolders::default()
    }
}
