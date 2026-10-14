//! The lock one process holds while it uses a state directory.
//!
//! Its own file because two builds in one workspace write the same files, and everything else
//! that keeps a file in the state directory is written as if it were alone there. Holding a
//! [`StateLock`] is what makes that true.
//!
//! Responsibilities: [`StateLock`], which takes the lock, stamps when it was taken, and sweeps
//! what an interrupted run left half-written; and [`LockError`].
//!
//! Non-responsibilities: waiting for a lock. A directory in use is refused at once, and the
//! caller decides what to tell the person who asked.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::UNIX_EPOCH;

use buildl_core::Timestamp;

use crate::state::write::STAGED_PREFIX;
use crate::state::{StateLayout, clear_path};

/// Proof that this process alone uses a state directory. Dropping it frees the directory.
///
/// The lock is one the system holds for the open file, so a process that dies frees it too: no
/// stale lock is ever left behind, and the lock file itself is never removed.
#[derive(Debug)]
pub struct StateLock {
    file: File,
    layout: StateLayout,
    acquired: Timestamp,
}

impl StateLock {
    /// Takes the lock of the state directory at `layout`, making the directory when it is
    /// missing.
    ///
    /// Once the lock is held, whatever an interrupted run left half-written is removed: every
    /// entry of the state directory whose name starts with `.tmp-`, and every entry of the blob
    /// staging directory.
    ///
    /// # Errors
    ///
    /// Returns [`LockError::Busy`] when another process holds the lock, and [`LockError::Io`]
    /// when the system refuses an operation on the directory or on something in it.
    pub fn acquire(layout: StateLayout) -> Result<Self, LockError> {
        fs::create_dir_all(layout.dir()).map_err(|source| failed(layout.dir(), source))?;
        let path = layout.lock();
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| failed(&path, source))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(LockError::Busy { path }),
            Err(TryLockError::Error(source)) => return Err(failed(&path, source)),
        }
        let acquired = stamp(&mut file).map_err(|source| failed(&path, source))?;
        sweep(layout.dir(), is_staged)?;
        sweep(&layout.blob_staging(), |_| true)?;
        Ok(Self {
            file,
            layout,
            acquired,
        })
    }

    /// The layout of the directory this lock holds.
    #[must_use]
    pub const fn layout(&self) -> &StateLayout {
        &self.layout
    }

    /// The filesystem's own time when the lock was taken.
    ///
    /// It is the modification time of the lock file, so it can be compared with the modification
    /// time of any other file on the same filesystem without asking a clock.
    #[must_use]
    pub const fn acquired(&self) -> Timestamp {
        self.acquired
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // Closing the file frees the lock as well. Asking for it says so, and frees the directory
        // even when a copy of the descriptor is still open somewhere.
        let _ = self.file.unlock();
    }
}

/// Why a state directory's lock was not taken.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LockError {
    /// Another process holds the lock.
    #[error("{} is in use by another buildl", .path.display())]
    Busy {
        /// The lock file.
        path: PathBuf,
    },
    /// The system refused an operation on the directory or on something in it.
    #[error("{}: {source}", .path.display())]
    Io {
        /// What the operation was on.
        path: PathBuf,
        /// What the system answered.
        #[source]
        source: io::Error,
    },
}

/// The error of an operation on `path`.
fn failed(path: &Path, source: io::Error) -> LockError {
    LockError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Makes the lock file name this process, and returns the modification time that write gave it.
fn stamp(file: &mut File) -> io::Result<Timestamp> {
    file.set_len(0)?;
    file.write_all(format!("{}\n", process::id()).as_bytes())?;
    let since = file
        .metadata()?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Ok(Timestamp::from_unix_nanos(
        u64::try_from(since.as_nanos()).unwrap_or(u64::MAX),
    ))
}

/// Whether `name` is that of a file that was still being written.
fn is_staged(name: &OsStr) -> bool {
    name.as_encoded_bytes()
        .starts_with(STAGED_PREFIX.as_bytes())
}

/// Removes every entry of `dir` whose name `swept` accepts. A `dir` that does not exist holds
/// nothing to remove.
fn sweep(dir: &Path, swept: impl Fn(&OsStr) -> bool) -> Result<(), LockError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(failed(dir, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| failed(dir, source))?;
        if swept(&entry.file_name()) {
            let path = entry.path();
            clear_path(&path).map_err(|source| failed(&path, source))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::time::UNIX_EPOCH;

    use buildl_core::{Directory, Timestamp};

    use super::{LockError, StateLock};
    use crate::state::StateLayout;

    /// A temporary workspace root, and the layout of its state directory.
    fn workspace() -> (tempfile::TempDir, StateLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
        (dir, layout)
    }

    #[test]
    fn a_second_lock_is_refused_and_a_dropped_one_is_free() {
        let (_dir, layout) = workspace();

        let held = StateLock::acquire(layout.clone()).unwrap();
        assert_eq!(held.layout(), &layout);
        let refused = StateLock::acquire(layout.clone()).unwrap_err();
        assert!(
            matches!(&refused, LockError::Busy { path } if *path == layout.lock()),
            "{refused:?}"
        );

        drop(held);
        StateLock::acquire(layout).unwrap();
    }

    #[test]
    fn taking_the_lock_makes_the_directory_and_names_the_holder() {
        let (_dir, layout) = workspace();
        assert!(!layout.dir().exists());

        let held = StateLock::acquire(layout.clone()).unwrap();
        assert_eq!(
            fs::read_to_string(layout.lock()).unwrap(),
            format!("{}\n", process::id())
        );

        // The file is kept and rewritten, never grown, by the holder after it.
        drop(held);
        fs::write(layout.lock(), b"4294967295 and more\n").unwrap();
        let _held = StateLock::acquire(layout.clone()).unwrap();
        assert_eq!(
            fs::read_to_string(layout.lock()).unwrap(),
            format!("{}\n", process::id())
        );
    }

    #[test]
    fn the_lock_stamp_is_not_before_a_file_written_earlier() {
        let (dir, layout) = workspace();
        let earlier = dir.path().join("earlier");
        fs::write(&earlier, b"bytes").unwrap();
        let written = fs::metadata(&earlier)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let written = Timestamp::from_unix_nanos(u64::try_from(written.as_nanos()).unwrap());

        let held = StateLock::acquire(layout).unwrap();

        assert!(held.acquired() >= written);
    }

    #[test]
    fn a_directory_that_cannot_be_made_is_an_io_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file"), b"bytes").unwrap();
        // The state directory would lie below a file.
        let layout = StateLayout::new(
            &dir.path().join("file"),
            &Directory::parse("state").unwrap(),
        );

        let refused = StateLock::acquire(layout.clone()).unwrap_err();
        assert!(
            matches!(&refused, LockError::Io { path, .. } if path == layout.dir()),
            "{refused:?}"
        );
    }

    #[test]
    fn each_error_names_its_path() {
        let busy = LockError::Busy {
            path: PathBuf::from("/work/.buildl/lock"),
        };
        assert_eq!(
            busy.to_string(),
            "/work/.buildl/lock is in use by another buildl"
        );

        let io = LockError::Io {
            path: PathBuf::from("/work/.buildl"),
            source: io::Error::other("refused"),
        };
        assert_eq!(io.to_string(), "/work/.buildl: refused");
    }

    #[test]
    fn taking_the_lock_sweeps_staging() {
        let (_dir, layout) = workspace();
        let shard = layout.blobs().join("ab");
        fs::create_dir_all(layout.blob_staging()).unwrap();
        fs::create_dir_all(&shard).unwrap();
        fs::write(layout.dir().join(".tmp-a1B2c3"), b"half").unwrap();
        fs::create_dir(layout.dir().join(".tmp-dir")).unwrap();
        fs::write(layout.blob_staging().join(".tmp-x9Y8z7"), b"half").unwrap();
        fs::write(layout.blob_staging().join("other"), b"half").unwrap();
        fs::write(layout.cache(), b"{}").unwrap();
        fs::write(layout.dir().join(".tmpfile"), b"kept").unwrap();
        fs::write(shard.join("cdef"), b"blob").unwrap();

        let _held = StateLock::acquire(layout.clone()).unwrap();

        // Gone: what was being written, in the directory itself and in the blob staging.
        assert!(!layout.dir().join(".tmp-a1B2c3").exists());
        assert!(!layout.dir().join(".tmp-dir").exists());
        assert_eq!(fs::read_dir(layout.blob_staging()).unwrap().count(), 0);
        // Kept: every other file, a name that only resembles the prefix, and the blobs.
        assert_eq!(fs::read(layout.cache()).unwrap(), b"{}");
        assert_eq!(fs::read(layout.dir().join(".tmpfile")).unwrap(), b"kept");
        assert_eq!(fs::read(shard.join("cdef")).unwrap(), b"blob");
    }
}
