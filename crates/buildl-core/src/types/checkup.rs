//! A report of a workspace's authority: what its manifest says, what its build wants, and which
//! of those wants its ceiling does not allow.
//!
//! Its own file because it is the value `doctor` hands back: a build would refuse at the first
//! want outside the ceiling, while a checkup lists them all and refuses nothing.
//!
//! Responsibilities: [`Checkup`].
//!
//! Non-responsibilities: computing the wants, which Grants does; and showing them, which the
//! command line does.

use crate::types::{WantedSet, Workspace};

/// What a workspace allows and wants, as `doctor` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkup {
    workspace: Workspace,
    wanted: WantedSet,
    outside: WantedSet,
}

impl Checkup {
    /// A checkup of `workspace`, whose build wants `wanted`, of which `outside` is what the
    /// ceiling does not allow.
    #[must_use]
    pub const fn new(workspace: Workspace, wanted: WantedSet, outside: WantedSet) -> Self {
        Self {
            workspace,
            wanted,
            outside,
        }
    }

    /// The workspace as its manifest describes it.
    #[must_use]
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Everything the workspace's build wants.
    #[must_use]
    pub const fn wanted(&self) -> &WantedSet {
        &self.wanted
    }

    /// The wants the workspace's ceiling does not allow.
    #[must_use]
    pub const fn outside(&self) -> &WantedSet {
        &self.outside
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::Checkup;
    use crate::types::{
        ApproverMode, Ceiling, Directory, EnvName, Execution, Program, WantedSet, Workspace,
    };

    #[test]
    fn a_checkup_hands_back_what_it_was_built_from() {
        let workspace = Workspace {
            out: Directory::parse("out").unwrap(),
            state: Directory::parse(".buildl").unwrap(),
            ceiling: Ceiling {
                run: BTreeSet::new(),
                env: BTreeSet::new(),
                read: BTreeSet::new(),
                write: BTreeSet::new(),
            },
            execution: Execution::default(),
            approver: ApproverMode::Deny,
            plugins: std::collections::BTreeMap::new(),
        };
        let wanted = WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            env: BTreeSet::from([EnvName::parse("PATH").unwrap()]),
            ..WantedSet::default()
        };
        let outside = WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            ..WantedSet::default()
        };
        let checkup = Checkup::new(workspace.clone(), wanted.clone(), outside.clone());
        assert_eq!(checkup.workspace(), &workspace);
        assert_eq!(checkup.wanted(), &wanted);
        assert_eq!(checkup.outside(), &outside);
    }
}
