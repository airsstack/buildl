//! The port through which a build asks whether it may do what it wants.
//!
//! Its own file because approving is a decision about the machine and the person using it, and
//! this crate neither reads a terminal nor a file: it says what a build wants, and an adapter
//! decides.
//!
//! Responsibilities: [`Approver`].
//!
//! Non-responsibilities: implementing it, and acting on its answer. The first belongs to an
//! adapter crate; the second to the step that negotiates a build's wants with it. What a
//! workspace's ceiling allows is not asked here either: the plan holds every want to the
//! ceiling first.
//!
//! An approver borrowed mutably is an approver too, so a caller that keeps its approver can
//! lend it to one build.

use crate::error::Result;
use crate::types::{Approval, Attribution, WantedSet};

/// Says whether a build that wants a set of things may run.
///
/// An approver can only narrow what a build may do. It is never asked about a want the
/// workspace's ceiling does not allow, because a build wanting one is refused before it is
/// asked; so a grant never widens the ceiling.
///
/// An approver that fails returns the error rather than an answer, and the build treats that as
/// no approval: nothing runs.
pub trait Approver {
    /// Whether a build that wants `wanted` may run.
    ///
    /// `attribution` says which plugins' targets are behind the wants it covers. It explains the
    /// set and never changes it: what an approver remembers or refuses is decided on `wanted`.
    ///
    /// # Errors
    ///
    /// Returns an error when the approver cannot decide, for example because what it keeps
    /// cannot be read or written. It is never read as approval.
    fn approve(&mut self, wanted: &WantedSet, attribution: &Attribution) -> Result<Approval>;
}

impl<A: Approver + ?Sized> Approver for &mut A {
    fn approve(&mut self, wanted: &WantedSet, attribution: &Attribution) -> Result<Approval> {
        (**self).approve(wanted, attribution)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::Approver;
    use crate::error::Result;
    use crate::types::{Approval, Attribution, WantedSet};

    /// Grants, and counts how many times it was asked.
    #[derive(Default)]
    struct Counting {
        asked: usize,
    }

    impl Approver for Counting {
        fn approve(&mut self, _wanted: &WantedSet, _attribution: &Attribution) -> Result<Approval> {
            self.asked += 1;
            Ok(Approval::Granted)
        }
    }

    /// Asks `approver` once, as a pipeline holding any approver does.
    fn ask<A: Approver>(mut approver: A) -> Approval {
        let wanted = WantedSet {
            programs: BTreeSet::new(),
            env: BTreeSet::new(),
            write: BTreeSet::new(),
            network: BTreeSet::new(),
        };
        approver.approve(&wanted, &Attribution::default()).unwrap()
    }

    #[test]
    fn a_borrowed_approver_answers_as_the_approver() {
        let mut counting = Counting::default();

        let answer = ask(&mut counting);

        assert_eq!(answer, Approval::Granted);
        assert_eq!(counting.asked, 1, "the lent approver is the one asked");
    }
}
