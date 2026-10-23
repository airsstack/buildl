//! The approver of a workspace that lets no build run.
//!
//! Its own file because it is the one approver that needs nothing to decide: the answer is the
//! workspace's own setting.
//!
//! Responsibilities: [`DenyApprover`].

use buildl_core::{Approval, Approver, Attribution, DenialReason, Result, WantedSet};

/// Why every build is refused.
const REASON: &str = "the workspace's approver is deny";

/// Refuses every build.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyApprover;

impl Approver for DenyApprover {
    fn approve(&mut self, wanted: &WantedSet, _attribution: &Attribution) -> Result<Approval> {
        Ok(Approval::Denied {
            unapproved: wanted.clone(),
            reason: DenialReason::new(REASON),
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use buildl_core::{Approval, Approver, Attribution, Program, WantedSet};

    use super::DenyApprover;

    #[test]
    fn deny_refuses_everything_naming_it() {
        let mut approver = DenyApprover;
        let wanted = WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            ..WantedSet::default()
        };

        for set in [wanted, WantedSet::default()] {
            let approval = approver.approve(&set, &Attribution::default()).unwrap();

            assert!(
                matches!(
                    &approval,
                    Approval::Denied { unapproved, reason }
                        if *unapproved == set && reason.to_string().contains("deny")
                ),
                "{approval:?}"
            );
        }
    }
}
