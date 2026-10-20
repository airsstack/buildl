//! Removing whatever stands at a path.
//!
//! Its own file because removing is asked for by path, and the right call depends on what is
//! found there: `remove_file` fails on a directory and `remove_dir_all` fails on a file, each
//! with a different error on each system. One function looks first, so no caller guesses.
//!
//! Responsibilities: [`clear_path`] and `clear_found`.
//!
//! Non-responsibilities: deciding what may be removed. A caller hands over a path it owns.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

/// Removes whatever is at `path`, following no symbolic link. Nothing being there is not an
/// error.
///
/// A directory is removed with everything below it. A symbolic link is removed and what it
/// points at is left alone, a link to a directory included. A file is unlinked and never opened,
/// so the bytes stay whole for any other name they have.
///
/// # Errors
///
/// Returns the I/O error of looking at `path` or of removing what is there.
pub(crate) fn clear_path(path: &Path) -> io::Result<()> {
    clear_found(path).map(drop)
}

/// As [`clear_path`], and says whether anything stood at `path`: `true` when something was
/// removed, `false` when nothing was there. The answer comes from the one look that decides what
/// to remove.
///
/// # Errors
///
/// Returns the I/O error of looking at `path` or of removing what is there.
pub(crate) fn clear_found(path: &Path) -> io::Result<bool> {
    let found = match fs::symlink_metadata(path) {
        Ok(found) => found,
        // A parent that is a file holds nothing either.
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    if found.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, Permissions};
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::Path;

    use super::{clear_found, clear_path};

    /// A file at `path` holding `bytes`, which nobody may write to.
    fn read_only(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, Permissions::from_mode(0o444)).unwrap();
    }

    #[test]
    fn clear_path_removes_nothing_a_file_a_read_only_tree_and_a_link_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Nothing there, and nothing below a path that is a file.
        clear_path(&root.join("absent")).unwrap();
        read_only(&root.join("file"), b"bytes");
        clear_path(&root.join("file").join("below")).unwrap();
        assert!(root.join("file").exists());

        // A file nobody may write to.
        clear_path(&root.join("file")).unwrap();
        assert!(!root.join("file").exists());

        // A directory of such files.
        let tree = root.join("tree");
        fs::create_dir_all(tree.join("inner")).unwrap();
        read_only(&tree.join("a"), b"a");
        read_only(&tree.join("inner").join("b"), b"b");
        clear_path(&tree).unwrap();
        assert!(!tree.exists());

        // A link to a directory: the link goes, what it points at stays.
        let kept = root.join("kept");
        fs::create_dir(&kept).unwrap();
        fs::write(kept.join("inside"), b"inside").unwrap();
        symlink(&kept, root.join("link")).unwrap();
        clear_path(&root.join("link")).unwrap();
        assert!(fs::symlink_metadata(root.join("link")).is_err());
        assert_eq!(fs::read(kept.join("inside")).unwrap(), b"inside");

        // A link to nothing.
        symlink(root.join("nowhere"), root.join("dangling")).unwrap();
        clear_path(&root.join("dangling")).unwrap();
        assert!(fs::symlink_metadata(root.join("dangling")).is_err());
    }

    #[test]
    fn clear_path_unlinks_and_never_writes_through_a_hard_link() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("blob");
        let placed = dir.path().join("placed");
        read_only(&blob, b"bytes");
        fs::hard_link(&blob, &placed).unwrap();
        assert_eq!(fs::metadata(&blob).unwrap().nlink(), 2);

        clear_path(&placed).unwrap();

        assert!(!placed.exists());
        assert_eq!(fs::read(&blob).unwrap(), b"bytes");
        let left = fs::metadata(&blob).unwrap();
        assert_eq!(left.nlink(), 1);
        assert_eq!(left.permissions().mode() & 0o777, 0o444);
    }

    #[test]
    fn clear_found_says_whether_anything_stood_there() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("file"), b"bytes").unwrap();

        assert!(clear_found(&root.join("file")).unwrap());
        assert!(!root.join("file").exists());
        assert!(!clear_found(&root.join("file")).unwrap());
        assert!(!clear_found(&root.join("absent").join("below")).unwrap());
    }
}
