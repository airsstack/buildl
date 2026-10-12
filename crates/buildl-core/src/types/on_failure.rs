//! What a build does with the rest of its targets once one has failed.
//!
//! Its own file because it is the one choice a caller makes about a build that is not in the
//! workspace: the same graph is built either way, and only what happens after a failure differs.
//! An enum rather than a flag, so a call site reads as the policy it chose.
//!
//! Responsibilities: [`OnFailure`].
//!
//! Non-responsibilities: applying the policy, which the phase that executes does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// What a build does once a target has failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnFailure {
    /// Begin nothing more: every target not yet begun is skipped, and an action already running
    /// finishes.
    Stop,
    /// Skip only the targets that depend on the failed one, directly or through others, and
    /// build the rest.
    KeepGoing,
}
