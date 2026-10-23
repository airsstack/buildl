//! A report of a workspace's authority: what it allows, what its build wants, and who asked.
//!
//! Its own file because it is the value `doctor` hands back: a build would refuse at the first
//! want outside the ceiling or outside a plugin, while a checkup lists them all and refuses
//! nothing. It holds the manifest's workspace, the wants, those the ceiling does not allow, the
//! plugins' targets behind the wants, and the wants a plugin does not allow.
//!
//! Responsibilities: [`Checkup`].
//!
//! Non-responsibilities: computing the wants, which Grants does; and showing them, which the
//! command line does.

use crate::types::{Attribution, PluginViolation, WantedSet, Workspace};

/// What a workspace allows and wants, as `doctor` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkup {
    workspace: Workspace,
    wanted: WantedSet,
    outside: WantedSet,
    attribution: Attribution,
    plugin_violations: Vec<PluginViolation>,
}

impl Checkup {
    /// A checkup of `workspace`, whose build wants `wanted`, of which `outside` is what the
    /// ceiling does not allow. `attribution` says which plugins' targets are behind the wants,
    /// and `plugin_violations` lists every want a plugin does not allow.
    #[must_use]
    pub const fn new(
        workspace: Workspace,
        wanted: WantedSet,
        outside: WantedSet,
        attribution: Attribution,
        plugin_violations: Vec<PluginViolation>,
    ) -> Self {
        Self {
            workspace,
            wanted,
            outside,
            attribution,
            plugin_violations,
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

    /// Which plugins' targets are behind the workspace's wants.
    #[must_use]
    pub const fn attribution(&self) -> &Attribution {
        &self.attribution
    }

    /// Every want of a plugin's target that a plugin does not allow, in label order and then
    /// chain order.
    #[must_use]
    pub fn plugin_violations(&self) -> &[PluginViolation] {
        &self.plugin_violations
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
        ApproverMode, Attribution, Ceiling, Directory, EnvName, Execution, Label, PluginChain,
        PluginName, PluginRef, PluginVersion, Program, Want, WantedSet, Workspace,
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
        let go = PluginChain::new(vec![PluginRef {
            name: PluginName::parse("go").unwrap(),
            version: PluginVersion::parse("1.2.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }])
        .unwrap();
        let mut attribution = Attribution::default();
        attribution.record(
            Want::Program(Program::parse("cc").unwrap()),
            &go,
            &Label::parse("//:app").unwrap(),
        );
        let checkup = Checkup::new(
            workspace.clone(),
            wanted.clone(),
            outside.clone(),
            attribution.clone(),
            Vec::new(),
        );
        assert_eq!(checkup.attribution(), &attribution);
        assert_eq!(checkup.plugin_violations(), []);
        assert_eq!(checkup.workspace(), &workspace);
        assert_eq!(checkup.wanted(), &wanted);
        assert_eq!(checkup.outside(), &outside);
    }
}
