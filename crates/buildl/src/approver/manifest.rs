//! The approver of a workspace that leaves approval to its manifest.
//!
//! Its own file because it is the one approver that asks nothing and keeps nothing: the
//! manifest's ceiling already bounds what a build may want, so every build inside it is
//! approved.
//!
//! Responsibilities: [`ManifestApprover`].

use buildl_core::{Approval, Approver, Result, WantedSet};

/// Approves every build.
///
/// A build that wants more than the ceiling allows is refused by the plan before any approver is
/// asked, so granting everything it is asked about leaves the ceiling as the only limit.
#[derive(Debug, Clone, Copy, Default)]
pub struct ManifestApprover;

impl Approver for ManifestApprover {
    fn approve(&mut self, _wanted: &WantedSet) -> Result<Approval> {
        Ok(Approval::Granted)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use buildl_core::{Approval, Approver, Directory, EnvName, Label, Program, WantedSet};

    use super::ManifestApprover;

    #[test]
    fn the_manifest_approver_grants_everything() {
        let mut approver = ManifestApprover;
        let empty = WantedSet {
            programs: BTreeSet::new(),
            env: BTreeSet::new(),
            write: BTreeSet::new(),
            network: BTreeSet::new(),
        };
        let full = WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            env: BTreeSet::from([EnvName::parse("PATH").unwrap()]),
            write: BTreeSet::from([Directory::parse("out").unwrap()]),
            network: BTreeSet::from([Label::parse("//:fetch").unwrap()]),
        };

        assert_eq!(approver.approve(&empty).unwrap(), Approval::Granted);
        assert_eq!(approver.approve(&full).unwrap(), Approval::Granted);
    }
}
