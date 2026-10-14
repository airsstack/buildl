//! Which of its two outputs a command wrote to.
//!
//! Its own file because the two are kept apart everywhere: captured apart, stored apart, and
//! shown apart. A name for each keeps a call that hands over bytes from saying which by position.
//!
//! Responsibilities: [`Channel`].
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// One of the two outputs of a running command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}
