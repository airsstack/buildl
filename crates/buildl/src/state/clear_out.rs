//! Emptying the output directory.
//!
//! Its own file because `clean` is the one command that removes the whole output directory, and it
//! must say whether there was anything to remove, which `clear_path` does not.
//!
//! Responsibilities: [`Cleaned`] and [`clear_out`].
//!
//! Non-responsibilities: holding the output directory to the ceiling, and taking the state
//! directory; the caller does both first.

use std::io;
use std::path::Path;

use buildl_core::Directory;

use crate::state::clear_found;

/// Whether the output directory held anything when it was cleaned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleaned {
    /// Something stood at the output directory's path, and it was removed.
    Removed,
    /// Nothing stood there.
    Absent,
}

/// Removes the output directory `out` of the workspace at `root`, with everything below it.
///
/// A symbolic link at the output directory itself is removed and what it points at is left
/// alone, and files are unlinked, never written to, as [`clear_path`](crate::state::clear_path)
/// does. Parent directories of `out` are resolved as the build resolves them when it places
/// outputs, so a link among them is followed.
///
/// # Errors
///
/// Returns the I/O error of looking at the path or of removing what is there.
pub(crate) fn clear_out(root: &Path, out: &Directory) -> io::Result<Cleaned> {
    Ok(if clear_found(&root.join(out.as_str()))? {
        Cleaned::Removed
    } else {
        Cleaned::Absent
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;

    use buildl_core::Directory;

    use super::{Cleaned, clear_out};

    #[test]
    fn a_directory_of_files_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("out/sub")).unwrap();
        fs::write(root.join("out/a"), b"a").unwrap();
        fs::write(root.join("out/sub/b"), b"b").unwrap();
        fs::create_dir(root.join("keep")).unwrap();
        fs::write(root.join("keep/c"), b"c").unwrap();

        let cleaned = clear_out(root, &Directory::parse("out").unwrap()).unwrap();

        assert_eq!(cleaned, Cleaned::Removed);
        assert!(!root.join("out").exists());
        assert_eq!(fs::read(root.join("keep/c")).unwrap(), b"c");
    }

    #[test]
    fn a_missing_directory_is_absent() {
        let dir = tempfile::tempdir().unwrap();

        let cleaned = clear_out(dir.path(), &Directory::parse("out").unwrap()).unwrap();

        assert_eq!(cleaned, Cleaned::Absent);
    }

    #[test]
    fn a_symlink_is_removed_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("file"), b"kept").unwrap();
        symlink(&outside, root.join("out")).unwrap();

        let cleaned = clear_out(&root, &Directory::parse("out").unwrap()).unwrap();

        assert_eq!(cleaned, Cleaned::Removed);
        assert!(fs::symlink_metadata(root.join("out")).is_err());
        assert_eq!(fs::read(outside.join("file")).unwrap(), b"kept");
    }
}
