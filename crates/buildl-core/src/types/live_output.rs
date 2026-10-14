//! Whether a build shows what an action prints while the action is still running.
//!
//! Its own file because it is a choice a caller makes about one build that is not in the
//! workspace: the same actions run and the same bytes are stored either way, and only when they
//! are shown differs. An enum rather than a flag, so a call site reads as the choice it made.
//!
//! Responsibilities: [`LiveOutput`].
//!
//! Non-responsibilities: applying the choice, which the phase that executes does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// When a build shows what its actions print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiveOutput {
    /// Only once an action has ended, all of it together.
    #[default]
    Off,
    /// As it is printed, for one running action at a time; every other action as with
    /// [`LiveOutput::Off`].
    OneAction,
}
