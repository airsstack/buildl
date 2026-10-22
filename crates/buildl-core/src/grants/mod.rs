//! Grants: the build's authority — which programs a build made, everything a build wants, and
//! the approver's answer.
//!
//! Its own directory because it is shared, not a phase: Plan and Execute each need to know
//! whether a program is one the build made, and may not name each other.
//!
//! Responsibilities: the "built" rule, in `built`.
//!
//! Non-responsibilities: expanding a command, which is Placement's; and what a ceiling allows,
//! which is the manifest's to say.
//!
//! This file holds only module declarations, so it carries no logic to unit-test.

pub(crate) mod built;

#[cfg(test)]
mod fixtures;
