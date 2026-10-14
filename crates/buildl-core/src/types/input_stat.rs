//! What stands at the path of a declared input.
//!
//! Its own file because an input is a regular file, and whether a path holds one, nothing, or
//! something else decides which error a plan ends in: nothing there and something else there
//! are both mistakes of the build file that declared the input, and are told apart.
//!
//! Responsibilities: [`InputStat`], [`InputKind`] and the words [`InputKind`] is shown with.
//!
//! Non-responsibilities: looking at the path, which arrives through the
//! [`Digester`](crate::ports::Digester) port, and raising the error, which belongs to the phase
//! that plans.

use std::fmt;

use crate::types::FileStat;

/// What a digester found at the path of one input, following no symbolic link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputStat {
    /// Nothing is there.
    Missing,
    /// A regular file, with its metadata.
    File(FileStat),
    /// Something that is not a regular file.
    Other(InputKind),
}

/// What stands at a path that holds something other than a regular file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// A symbolic link, whatever it leads to.
    Symlink,
    /// A directory.
    Directory,
    /// Anything else: a pipe, a socket or a device.
    Special,
}

impl fmt::Display for InputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Symlink => "a symbolic link",
            Self::Directory => "a directory",
            Self::Special => "a pipe, a socket or a device",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::InputKind;

    #[test]
    fn each_kind_is_shown_as_what_a_person_would_call_it() {
        assert_eq!(InputKind::Symlink.to_string(), "a symbolic link");
        assert_eq!(InputKind::Directory.to_string(), "a directory");
        assert_eq!(
            InputKind::Special.to_string(),
            "a pipe, a socket or a device"
        );
    }
}
