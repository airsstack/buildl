//! What an execution strategy found at one output path of an action it ran.
//!
//! Its own file because it is the value that comes back across the execution port for an
//! action's outputs. It is a report, not a judgement: a strategy says what is there, including
//! what cannot be kept, and the phase that executes decides what that means for the build.
//!
//! Responsibilities: [`Found`], [`FoundEntry`] and [`FoundKind`].
//!
//! Non-responsibilities: walking a directory and storing a file, which a strategy does; and
//! refusing anything, which the phase that executes does.
//!
//! This file holds only type definitions, so it carries no logic to unit-test.

use crate::types::StoredFile;

/// What is at one output path once the action has run.
///
/// A strategy looks at the path itself and follows no symbolic link, at the path or below it.
/// Non-exhaustive: a strategy that can find another kind of thing gains a variant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Found {
    /// Nothing.
    Nothing,
    /// A regular file, whose bytes the strategy stored.
    File(StoredFile),
    /// A directory, with everything below it, in any order. An empty list is an empty
    /// directory.
    Tree(Vec<FoundEntry>),
    /// A symbolic link.
    Symlink,
    /// Something else: a device, a socket, a named pipe.
    Special,
}

/// One thing below a directory output.
///
/// A directory that holds something is not listed: the paths of what it holds say it is there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FoundEntry {
    /// Its path below the output path, every segment joined by `/` whatever the host's own
    /// separator is. For a [`FoundKind::NotText`] entry, the path with what is not text
    /// replaced, for display only.
    pub path: String,
    /// What it is.
    pub kind: FoundKind,
}

/// What one thing below a directory output is.
///
/// Non-exhaustive: a strategy that can find another kind of thing gains a variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum FoundKind {
    /// A regular file, whose bytes the strategy stored.
    File(StoredFile),
    /// A directory holding nothing.
    EmptyDirectory,
    /// A symbolic link.
    Symlink,
    /// Something else: a device, a socket, a named pipe.
    Special,
    /// Something whose name, or the name of a directory above it, is not valid Unicode.
    NotText,
}
