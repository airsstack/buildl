//! What an action printed, as bytes, for whoever shows it.
//!
//! Its own file because it is the value that crosses the reporting port beside an outcome: an
//! outcome holds the digests of what was printed, and a reporter is handed the bytes themselves.
//!
//! Responsibilities: [`Captured`].
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// What one action wrote to its two outputs, still to be shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Captured<'a> {
    /// What it wrote to standard output.
    pub stdout: &'a [u8],
    /// What it wrote to standard error.
    pub stderr: &'a [u8],
}
