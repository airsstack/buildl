//! What a verify found across a workspace's cached targets.
//!
//! Its own file because a verification is what a verify hands back: one check per target it
//! re-ran, and a count of the targets it left alone and why.
//!
//! Responsibilities: [`Verification`], its constructor and its accessors.
//!
//! Non-responsibilities: making the checks, which the `verify` module does; and rendering a
//! verification for a person, which belongs to whoever shows it.

use crate::types::{Check, Label};

/// What a verify found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    checked: Vec<(Label, Check)>,
    not_cached: usize,
    not_sampled: usize,
}

impl Verification {
    /// Joins the checks with the counts of the targets that were not checked.
    ///
    /// `checked` is kept in label order, whatever order it is given in.
    #[must_use]
    pub fn new(mut checked: Vec<(Label, Check)>, not_cached: usize, not_sampled: usize) -> Self {
        checked.sort_by(|(left, _), (right, _)| left.cmp(right));
        Self {
            checked,
            not_cached,
            not_sampled,
        }
    }

    /// The check of each target that was looked at, in label order.
    #[must_use]
    pub fn checked(&self) -> &[(Label, Check)] {
        &self.checked
    }

    /// How many targets had nothing cached to check against.
    #[must_use]
    pub const fn not_cached(&self) -> usize {
        self.not_cached
    }

    /// How many cached targets the sample left out.
    #[must_use]
    pub const fn not_sampled(&self) -> usize {
        self.not_sampled
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    #[test]
    fn verification_lists_checks_in_label_order() {
        let verification = Verification::new(
            vec![
                (label("//z:last"), Check::Verified),
                (
                    label("//m:mid"),
                    Check::Skipped {
                        because: label("//z:last"),
                    },
                ),
                (label("//a:first"), Check::Verified),
            ],
            2,
            3,
        );
        let labels: Vec<&Label> = verification.checked().iter().map(|(l, _)| l).collect();
        assert_eq!(
            labels,
            vec![&label("//a:first"), &label("//m:mid"), &label("//z:last")]
        );
        assert_eq!(verification.not_cached(), 2);
        assert_eq!(verification.not_sampled(), 3);
    }
}
