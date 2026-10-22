//! What an approver answers: the build may run, or what it may not do and why.
//!
//! Its own file because the answer is the value two sides share: the port an approver
//! implements returns it, and the pure step that acts on it reads it.
//!
//! Responsibilities: [`Approval`] and [`DenialReason`].
//!
//! Non-responsibilities: deciding. An approver decides; turning a denial into an error is the
//! step that negotiates a build's wants with it.

use core::fmt;

use crate::types::WantedSet;

/// An approver's answer about what one build wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    /// The build may run.
    Granted,
    /// The build may not run.
    Denied {
        /// What was not approved.
        unapproved: WantedSet,
        /// Why, in words for the user.
        reason: DenialReason,
    },
}

/// Why a build was not approved, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenialReason(String);

impl DenialReason {
    /// A reason made of `text`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl fmt::Display for DenialReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::DenialReason;

    #[test]
    fn a_denial_reason_shows_its_text() {
        assert_eq!(
            DenialReason::new("not approved").to_string(),
            "not approved"
        );
        assert_eq!(
            DenialReason::new(String::from("no terminal")).to_string(),
            "no terminal"
        );
    }
}
