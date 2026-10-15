//! What a reader of the build log hands on for one line of it.
//!
//! Its own file because a log read back is a sequence of these, and a line that is not an event
//! must be told apart from one that is without ending the read: the reader keeps going, and the
//! caller decides what a damaged line means.
//!
//! Responsibilities: [`LogLine`].
//!
//! Non-responsibilities: deciding whether a line is an event, which
//! [`canonical::from_slice`](crate::json::canonical::from_slice) does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use std::num::NonZeroUsize;

use crate::types::LoggedRun;

/// One line of the build log, read back.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "a log is read once into a list of lines and almost every line is an event, so boxing each run would add an allocation per line to save space on the rare damaged one"
)]
pub enum LogLine {
    /// A line that is an event.
    Event(LoggedRun),
    /// A whole line that is not an event, by its 1-based number in the file.
    Damaged {
        /// The line's number in the file, counting from 1.
        line: NonZeroUsize,
    },
}
