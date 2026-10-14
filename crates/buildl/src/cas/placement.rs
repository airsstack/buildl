//! How a held file becomes a file of the workspace: by a hard link, or by a copy.
//!
//! Its own file because the choice is made once per store, by trying rather than by asking what
//! the filesystem or the user is. A link is safe only where a write through it is refused, and
//! the read-only mode of a held file refuses it only when permission bits are kept and bind the
//! user: not on FAT, and not for root.
//!
//! Responsibilities: [`Placement`] and the probe that chooses it.
//!
//! Non-responsibilities: linking or copying, which the store does.

use std::fs::OpenOptions;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::thread;

use crate::state::{Access, clear_path, write_whole};

/// How a held file becomes a file of the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// By a hard link: the file of the workspace and the held file are one file.
    Link,
    /// By a copy: the file of the workspace is a file of its own.
    Copy,
}

impl Placement {
    /// `Link` when a write to a read-only file in `staging` is refused, `Copy` when it is
    /// allowed.
    ///
    /// A read-only file is written in `staging`, opened for writing, and removed. Where the
    /// write is let through, a link would let a write to a file of the workspace change what the
    /// store holds, so nothing is linked there.
    pub(crate) fn probe(staging: &Path) -> io::Result<Self> {
        // The name is this thread's own, so two threads probing one store at once do not remove
        // each other's file.
        let witness = staging.join(format!("probe-{:?}", thread::current().id()));
        write_whole(staging, &witness, b"", Access::ReadOnly)?;
        let answer = match OpenOptions::new().write(true).open(&witness) {
            Ok(_) => Ok(Self::Copy),
            Err(error) if error.kind() == ErrorKind::PermissionDenied => Ok(Self::Link),
            Err(error) => Err(error),
        };
        clear_path(&witness)?;
        answer
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, OpenOptions};

    use super::Placement;
    use crate::state::{Access, write_whole};

    #[test]
    fn the_probe_answers_link_exactly_when_a_write_to_a_read_only_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // What this user and this filesystem do with a read-only file, seen directly.
        let witness = dir.path().join("witness");
        write_whole(dir.path(), &witness, b"", Access::ReadOnly).unwrap();
        let refused = OpenOptions::new().write(true).open(&witness).is_err();

        let placement = Placement::probe(&dir.path().join("cas/tmp")).unwrap();

        let expected = if refused {
            Placement::Link
        } else {
            Placement::Copy
        };
        assert_eq!(placement, expected);
    }

    #[test]
    fn the_probe_makes_its_directory_and_leaves_nothing_in_it() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("cas/tmp");

        Placement::probe(&staging).unwrap();
        Placement::probe(&staging).unwrap();

        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    }

    #[test]
    fn a_probe_that_cannot_write_its_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, b"").unwrap();

        assert!(Placement::probe(&file.join("tmp")).is_err());
    }
}
