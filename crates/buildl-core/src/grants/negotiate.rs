//! Putting a build's wants to an approver.
//!
//! Its own file because it is the one step that turns an approver's answer into the build's:
//! the approver decides, and this reads the decision as run or refuse.
//!
//! Responsibilities: [`negotiate`].
//!
//! Non-responsibilities: deciding, which an approver does; holding a want to the ceiling, which
//! the plan does before this is reached; and choosing when to ask, which the pipeline does.

use crate::error::{Error, Result};
use crate::ports::Approver;
use crate::types::{Approval, Attribution, WantedSet};

/// Puts `wanted` to `approver` with the `attribution` that explains it, and goes on only when it
/// is granted.
///
/// # Errors
///
/// Returns [`Error::NotApproved`] naming what was not approved and why when the approver
/// denies, and the approver's own error, unchanged, when it fails: a failure is never read as
/// approval.
pub fn negotiate<A: Approver>(
    wanted: &WantedSet,
    attribution: &Attribution,
    approver: &mut A,
) -> Result<()> {
    match approver.approve(wanted, attribution)? {
        Approval::Granted => Ok(()),
        Approval::Denied { unapproved, reason } => Err(Error::NotApproved {
            unapproved: Box::new(unapproved),
            reason,
        }),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::negotiate;
    use crate::error::{Error, PortName, Result};
    use crate::grants::fixtures::program;
    use crate::ports::Approver;
    use crate::types::{
        Approval, Attribution, DenialReason, Diagnostic, Label, PluginChain, PluginName, PluginRef,
        PluginVersion, Want, WantedSet,
    };

    /// Answers with a fixed result and keeps what it was asked about.
    struct Scripted {
        answer: Option<Result<Approval>>,
        asked: Vec<WantedSet>,
        attributions: Vec<Attribution>,
    }

    impl Scripted {
        fn answering(answer: Result<Approval>) -> Self {
            Self {
                answer: Some(answer),
                asked: Vec::new(),
                attributions: Vec::new(),
            }
        }
    }

    impl Approver for Scripted {
        fn approve(&mut self, wanted: &WantedSet, attribution: &Attribution) -> Result<Approval> {
            self.asked.push(wanted.clone());
            self.attributions.push(attribution.clone());
            self.answer.take().unwrap()
        }
    }

    fn wanting_cc() -> WantedSet {
        WantedSet {
            programs: BTreeSet::from([program("cc")]),
            env: BTreeSet::new(),
            write: BTreeSet::new(),
            network: BTreeSet::new(),
        }
    }

    #[test]
    fn a_granted_set_goes_on() {
        let wanted = wanting_cc();
        let mut approver = Scripted::answering(Ok(Approval::Granted));

        negotiate(&wanted, &Attribution::default(), &mut approver).unwrap();

        assert_eq!(approver.asked, vec![wanted], "asked once, about the set");
    }

    #[test]
    fn the_attribution_reaches_the_approver() {
        let wanted = wanting_cc();
        let chain = PluginChain::new(vec![PluginRef {
            name: PluginName::parse("cc").unwrap(),
            version: PluginVersion::parse("1.0.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }])
        .unwrap();
        let mut attribution = Attribution::default();
        attribution.record(
            Want::Program(program("cc")),
            &chain,
            &Label::parse("//:lib").unwrap(),
        );
        let mut approver = Scripted::answering(Ok(Approval::Granted));

        negotiate(&wanted, &attribution, &mut approver).unwrap();

        assert_eq!(approver.attributions, vec![attribution]);
    }

    #[test]
    fn a_denial_is_not_approved_naming_what_and_why() {
        let wanted = wanting_cc();
        let mut approver = Scripted::answering(Ok(Approval::Denied {
            unapproved: wanted.clone(),
            reason: DenialReason::new("not today"),
        }));

        let err = negotiate(&wanted, &Attribution::default(), &mut approver).unwrap_err();

        let Error::NotApproved { unapproved, reason } = err else {
            unreachable!("expected NotApproved, got {err:?}");
        };
        assert_eq!(*unapproved, wanted);
        assert_eq!(reason.to_string(), "not today");
    }

    #[test]
    fn a_partial_denial_names_only_what_the_approver_left_unapproved() {
        let wanted = WantedSet {
            programs: BTreeSet::from([program("cc"), program("ld")]),
            ..wanting_cc()
        };
        let unapproved = WantedSet {
            programs: BTreeSet::from([program("ld")]),
            ..wanting_cc()
        };
        let mut approver = Scripted::answering(Ok(Approval::Denied {
            unapproved: unapproved.clone(),
            reason: DenialReason::new("ld is not approved"),
        }));

        let err = negotiate(&wanted, &Attribution::default(), &mut approver).unwrap_err();

        let Error::NotApproved {
            unapproved: named, ..
        } = err
        else {
            unreachable!("expected NotApproved, got {err:?}");
        };
        assert_eq!(
            *named, unapproved,
            "the approver's subset, not the asked set"
        );
        assert_ne!(*named, wanted);
    }

    #[test]
    fn an_approver_error_is_returned_not_read_as_approval() {
        let failure = Error::Adapter {
            port: PortName::Approver,
            diagnostic: Diagnostic::new("the approvals file is unreadable"),
        };
        let mut approver = Scripted::answering(Err(failure));

        let err = negotiate(&wanting_cc(), &Attribution::default(), &mut approver).unwrap_err();

        let Error::Adapter { port, diagnostic } = err else {
            unreachable!("expected the approver's own error, got {err:?}");
        };
        assert_eq!(port, PortName::Approver);
        assert_eq!(diagnostic.to_string(), "the approvals file is unreadable");
    }
}
