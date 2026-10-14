//! What an execution strategy reports for one action it ran.
//!
//! Its own file because it is the value that comes back across the execution port: how the
//! command ended, and what it printed. It is raw because nothing in it is judged yet; whether a
//! build succeeded is decided from it, not by the strategy.
//!
//! Responsibilities: [`RawOutcome`] and [`Exit`].
//!
//! Non-responsibilities: classifying the outcome, and what the command left at its output
//! paths, which a strategy reports separately. A command that could not be started at all is
//! not an outcome: the strategy returns an error for it.
//!
//! This file holds only type definitions, so it carries no logic to unit-test.

/// How a command that was started came to an end.
///
/// Non-exhaustive: a strategy that can end a command another way gains a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exit {
    /// It exited on its own, with this status. Zero is success.
    Exited(i32),
    /// A signal ended it before it exited, with this number. It has no status.
    Signaled(i32),
    /// It ran longer than it was allowed to and was stopped.
    TimedOut,
}

/// What running one command produced, apart from what it left at its output paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutcome {
    /// How the command ended.
    pub exit: Exit,
    /// Everything it wrote to standard output.
    pub stdout: Vec<u8>,
    /// Everything it wrote to standard error.
    pub stderr: Vec<u8>,
}
