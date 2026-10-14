//! Writing a file whole: a reader finds the old bytes or the new ones, never part of either.
//!
//! Its own file because every file a build keeps is written this way, and the two steps that
//! make it safe, a new file beside the old one and then a rename onto it, are easy to get half
//! right when each module writes them out for itself.
//!
//! Responsibilities: [`write_whole`], and [`Access`], the mode a written file is given.
//!
//! Non-responsibilities: surviving the machine losing power. Nothing is synced: a rename covers
//! a process that dies, which is what a build promises, and a sync costs many times the write.

use std::fs::{self, Permissions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tempfile::{Builder, NamedTempFile};

/// The start of the name of a file that is still being written.
pub(super) const STAGED_PREFIX: &str = ".tmp-";

/// What may be done with a file once it is written.
///
/// Each is the mode asked for when the file is made. What the file gets is that mode less the
/// process's umask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read by anyone, written by its owner: `0o644`.
    Rewritable,
    /// Read by anyone, written by no one: `0o444`.
    ReadOnly,
    /// Read and run by anyone, written by no one: `0o555`.
    ReadOnlyRunnable,
}

impl Access {
    /// The mode a file with this access is made with.
    const fn mode(self) -> u32 {
        match self {
            Self::Rewritable => 0o644,
            Self::ReadOnly => 0o444,
            Self::ReadOnlyRunnable => 0o555,
        }
    }
}

/// Makes `bytes` the whole of `target`: written to a new file in `staging`, then renamed onto it.
///
/// A `staging` directory that does not exist is made, and so is a missing parent of `target`.
/// When the write fails, the new file is removed and `target` is left as it was.
///
/// # Errors
///
/// Returns the I/O error of the step that failed. `staging` and `target` must be on one
/// filesystem: a rename across two is refused by the system, and that refusal is what is
/// returned.
pub fn write_whole(staging: &Path, target: &Path, bytes: &[u8], access: Access) -> io::Result<()> {
    let mut file = staged(staging, access)?;
    file.write_all(bytes)?;
    let Err(refused) = file.persist(target) else {
        return Ok(());
    };
    let missing_parent = refused.error.kind() == ErrorKind::NotFound;
    match target.parent() {
        Some(parent) if missing_parent => {
            fs::create_dir_all(parent)?;
            refused.file.persist(target)?;
            Ok(())
        }
        _ => Err(refused.error),
    }
}

/// A new, empty file in `staging` with the mode of `access`, making `staging` when it is missing.
fn staged(staging: &Path, access: Access) -> io::Result<NamedTempFile> {
    let mut builder = Builder::new();
    builder
        .prefix(STAGED_PREFIX)
        .permissions(Permissions::from_mode(access.mode()));
    match builder.tempfile_in(staging) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(staging)?;
            builder.tempfile_in(staging)
        }
        made => made,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, OpenOptions};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::Path;

    use super::{Access, write_whole};

    /// The permission bits of the file at `path`.
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// The names in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn write_whole_replaces_the_target_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        let kept = dir.path().join("kept");
        fs::create_dir(&staging).unwrap();
        fs::create_dir(&kept).unwrap();
        let target = kept.join("blob");

        write_whole(&staging, &target, b"first", Access::ReadOnly).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"first");

        // The file being replaced is read-only: a rename needs no write access to it.
        write_whole(&staging, &target, b"second", Access::ReadOnly).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second");
        assert_eq!(names(&staging), Vec::<String>::new());
        assert_eq!(names(&kept), ["blob"]);
    }

    #[test]
    fn write_whole_gives_the_file_its_access() {
        let dir = tempfile::tempdir().unwrap();
        // What the umask lets through, learned from a file made asking for everything.
        let probe = dir.path().join("probe");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o777)
            .open(&probe)
            .unwrap();
        let allowed = mode(&probe);

        for (access, asked) in [
            (Access::Rewritable, 0o644),
            (Access::ReadOnly, 0o444),
            (Access::ReadOnlyRunnable, 0o555),
        ] {
            let target = dir.path().join(format!("{asked:o}"));
            write_whole(dir.path(), &target, b"bytes", access).unwrap();
            assert_eq!(mode(&target), asked & allowed, "asked for {asked:o}");
        }
    }

    #[test]
    fn write_whole_makes_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("a").join("staging");
        let target = dir.path().join("b").join("c").join("file");

        write_whole(&staging, &target, b"bytes", Access::Rewritable).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"bytes");
        assert_eq!(names(&staging), Vec::<String>::new());
    }

    #[test]
    fn a_write_that_fails_leaves_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");

        // A directory with something in it cannot be renamed over.
        let occupied = dir.path().join("occupied");
        fs::create_dir(&occupied).unwrap();
        fs::write(occupied.join("old"), b"old").unwrap();
        assert!(write_whole(&staging, &occupied, b"new", Access::Rewritable).is_err());
        assert_eq!(fs::read(occupied.join("old")).unwrap(), b"old");
        assert_eq!(names(&staging), Vec::<String>::new());

        // Nor can a file be staged below a path that is a file.
        let target = dir.path().join("file");
        fs::write(&target, b"old").unwrap();
        let below_a_file = target.join("staging");
        assert!(write_whole(&below_a_file, &target, b"new", Access::Rewritable).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert_eq!(names(dir.path()), ["file", "occupied", "staging"]);
    }
}
