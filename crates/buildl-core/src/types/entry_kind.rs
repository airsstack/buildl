//! What a path is, as the one asking whether it is ignored knows it.
//!
//! Its own file because a pattern that ends in `/` matches folders only, so the answer to an
//! ignore question depends on a kind the question has to carry.
//!
//! Responsibilities: [`EntryKind`].
//!
//! Non-responsibilities: finding the kind out, which is the asker's and arrives through a port.
//!
//! This file declares a plain enum with no logic, so it carries no unit tests.

/// What a workspace path names, as its asker can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A file.
    File,
    /// A folder.
    Folder,
    /// A path the asker cannot classify.
    Unknown,
}
