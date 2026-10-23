//! Grants: the build's authority — which programs a build made, everything a build wants, and
//! the approver's answer.
//!
//! Its own directory because it is shared, not a phase: Plan and Execute each need to know
//! whether a program is one the build made, and may not name each other.
//!
//! Responsibilities: the "built" rule, in `built`; what a graph's targets want, in `wanted`; the
//! wants a ceiling does not allow, in `outside`; the wants of a plugin's targets that a plugin
//! does not allow, in `plugins`; and putting those wants to an approver, in `negotiate`.
//!
//! Non-responsibilities: expanding a command, which is Placement's; and what a ceiling allows,
//! which is the manifest's to say.
//!
//! This file holds only module declarations and a re-export, so it carries no logic to unit-test.

pub(crate) mod built;
mod negotiate;
pub(crate) mod outside;
pub(crate) mod plugins;
pub(crate) mod wanted;

pub use negotiate::negotiate;

#[cfg(test)]
pub(crate) mod fixtures;
