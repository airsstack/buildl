//! The scratch directory of a workspace, kept outside it.
//!
//! Its own file because the state lock empties the directory and the sandbox makes each action's
//! directory in it, and no adapter may name another. The directory lies outside the workspace
//! because a tool that searches upward for its project file would otherwise find the workspace
//! from inside the sandbox. Each action's directory has a name that depends only on the action's
//! label, so it is the same on every run: a compile with debug info records the directory it ran
//! in, and a name that changed would change the output.
//!
//! Responsibilities: [`Scratch`], which names the directory and each action's directory in it,
//! claims it safely and removes it; and [`Owner`], the user a directory belongs to.
//!
//! Non-responsibilities: what an action does in its directory, and when the directory is removed.

use std::fs::{self, DirBuilder, Metadata};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use buildl_core::{Digest, Label};

use crate::state::clear_path;

/// The user a directory belongs to.
///
/// Made only from a file's metadata, so a user id never travels as a bare number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner(u32);

impl Owner {
    /// The user the file described by `metadata` belongs to.
    #[must_use]
    pub fn of(metadata: &Metadata) -> Self {
        Self(metadata.uid())
    }
}

/// How many hexadecimal digits of a digest name a directory.
const NAME_DIGITS: usize = 32;

/// The mode the scratch directory has: the owner alone may enter it.
const PRIVATE: u32 = 0o700;

/// The name `bytes` give a directory: the first digits of their digest.
fn name_of(bytes: &[u8]) -> String {
    Digest::of(bytes)
        .to_string()
        .chars()
        .take(NAME_DIGITS)
        .collect()
}

/// A refusal of the scratch directory for `reason`. It names no path, so a log that holds it
/// does not say where the directory is.
fn refused(reason: &str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, reason.to_owned())
}

/// Where a workspace's actions run: a directory of its own outside the workspace.
///
/// Making a [`Scratch`] touches nothing on disk.
#[derive(Debug, Clone)]
pub struct Scratch {
    dir: PathBuf,
    root: PathBuf,
}

impl Scratch {
    /// The scratch directory of the workspace at `root`, below `temp`.
    #[must_use]
    pub fn new(temp: &Path, root: &Path) -> Self {
        Self {
            dir: temp.join(format!(
                "buildl-{}",
                name_of(root.as_os_str().as_encoded_bytes())
            )),
            root: root.to_path_buf(),
        }
    }

    /// The scratch directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The directory the action of `label` runs in.
    #[must_use]
    pub fn action(&self, label: &Label) -> PathBuf {
        self.dir.join(name_of(label.to_string().as_bytes()))
    }

    /// Makes the scratch directory, readable by `owner` alone, or accepts one that is already so.
    ///
    /// # Errors
    ///
    /// Returns the system's error when the directory cannot be made, and a refusal that names
    /// the reason and no path when what is there is a link, is not a directory, belongs to
    /// another user than `owner`, is not mode `0700`, or lies inside the workspace.
    pub fn claim(&self, owner: Owner) -> io::Result<()> {
        match DirBuilder::new().mode(PRIVATE).create(&self.dir) {
            Ok(()) => {}
            // Another worker made it first, or an earlier run left it: look at what is there.
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let found = fs::symlink_metadata(&self.dir)?;
        if found.file_type().is_symlink() {
            return Err(refused("the scratch directory is a symbolic link"));
        }
        if !found.is_dir() {
            return Err(refused("the scratch directory is not a directory"));
        }
        if Owner::of(&found) != owner {
            return Err(refused("the scratch directory belongs to another user"));
        }
        let mode = found.mode() & 0o777;
        if mode != PRIVATE {
            return Err(refused(&format!(
                "the scratch directory has mode {mode:04o}, not {PRIVATE:04o}"
            )));
        }
        if fs::canonicalize(&self.dir)?.starts_with(fs::canonicalize(&self.root)?) {
            return Err(refused("the scratch directory lies inside the workspace"));
        }
        Ok(())
    }

    /// Removes the scratch directory and everything in it. Nothing being there is not an error.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of removing what is there.
    pub fn remove(&self) -> io::Result<()> {
        clear_path(&self.dir)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, Permissions};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::thread;

    use buildl_core::Label;

    use super::{Owner, Scratch};

    /// The owner of the temporary base, which is the user running the test.
    fn own(base: &tempfile::TempDir) -> Owner {
        Owner::of(&fs::metadata(base.path()).unwrap())
    }

    /// A scratch below a fresh temporary base, for a workspace root that lies elsewhere.
    fn scratch() -> (tempfile::TempDir, Scratch) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("workspace");
        fs::create_dir(&root).unwrap();
        let scratch = Scratch::new(base.path(), &root);
        (base, scratch)
    }

    fn is_hex(text: &str, length: usize) -> bool {
        text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let base = tempfile::tempdir().unwrap();
        let first = Scratch::new(base.path(), "/work/one".as_ref());
        let again = Scratch::new(base.path(), "/work/one".as_ref());
        let other = Scratch::new(base.path(), "/work/two".as_ref());
        assert_eq!(first.dir(), again.dir());
        assert_ne!(first.dir(), other.dir());

        let name = first.dir().strip_prefix(base.path()).unwrap();
        let name = name.to_str().unwrap().strip_prefix("buildl-").unwrap();
        assert!(is_hex(name, 32), "{name}");

        let one = Label::parse("//app:one").unwrap();
        let two = Label::parse("//app:two").unwrap();
        assert_eq!(first.action(&one), again.action(&one));
        assert_ne!(first.action(&one), first.action(&two));
        let id = first.action(&one);
        assert_eq!(id.parent().unwrap(), first.dir());
        assert!(is_hex(id.file_name().unwrap().to_str().unwrap(), 32));
    }

    #[test]
    fn claim_makes_the_directory_0700() {
        let (base, scratch) = scratch();

        scratch.claim(own(&base)).unwrap();

        let mode = fs::metadata(scratch.dir()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        // A directory that is already right is accepted.
        scratch.claim(own(&base)).unwrap();
    }

    #[test]
    fn claim_refuses_a_link() {
        let (base, scratch) = scratch();
        let target = base.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, Permissions::from_mode(0o700)).unwrap();
        symlink(&target, scratch.dir()).unwrap();

        let refused = scratch.claim(own(&base)).unwrap_err().to_string();

        assert!(refused.contains("symbolic link"), "{refused}");
        assert!(
            !refused.contains(scratch.dir().to_str().unwrap()),
            "{refused}"
        );
    }

    #[test]
    fn claim_refuses_a_wrong_mode() {
        let (base, scratch) = scratch();
        fs::create_dir(scratch.dir()).unwrap();
        fs::set_permissions(scratch.dir(), Permissions::from_mode(0o755)).unwrap();

        let refused = scratch.claim(own(&base)).unwrap_err().to_string();

        assert!(refused.contains("0755"), "{refused}");
        assert!(
            !refused.contains(scratch.dir().to_str().unwrap()),
            "{refused}"
        );
    }

    #[test]
    fn claim_refuses_a_file() {
        let (base, scratch) = scratch();
        fs::write(scratch.dir(), b"bytes").unwrap();

        let refused = scratch.claim(own(&base)).unwrap_err().to_string();

        assert!(refused.contains("not a directory"), "{refused}");
    }

    #[test]
    fn claim_refuses_another_owner() {
        let (base, scratch) = scratch();
        let root_owner = Owner::of(&fs::metadata("/").unwrap());
        if root_owner == own(&base) {
            // Running as root: no file is another user's.
            return;
        }
        fs::create_dir(scratch.dir()).unwrap();
        fs::set_permissions(scratch.dir(), Permissions::from_mode(0o700)).unwrap();

        let refused = scratch.claim(root_owner).unwrap_err().to_string();

        assert!(refused.contains("another user"), "{refused}");
    }

    #[test]
    fn claim_refuses_a_scratch_inside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let temp = root.path().join("tmp");
        fs::create_dir(&temp).unwrap();
        let scratch = Scratch::new(&temp, root.path());

        let refused = scratch
            .claim(Owner::of(&fs::metadata(root.path()).unwrap()))
            .unwrap_err()
            .to_string();

        assert!(refused.contains("inside the workspace"), "{refused}");
    }

    #[test]
    fn claim_from_many_threads_at_once_succeeds() {
        let (base, scratch) = scratch();
        let owner = own(&base);

        let mut results = Vec::new();
        thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| scratch.claim(owner)))
                .collect();
            for handle in handles {
                results.push(handle.join().unwrap());
            }
        });

        assert!(results.iter().all(Result::is_ok), "{results:?}");
    }

    #[test]
    fn remove_removes_a_tree_with_read_only_files() {
        let (base, scratch) = scratch();
        // Nothing there yet.
        scratch.remove().unwrap();
        scratch.claim(own(&base)).unwrap();
        let inner = scratch.dir().join("inner");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("file"), b"bytes").unwrap();
        fs::set_permissions(inner.join("file"), Permissions::from_mode(0o444)).unwrap();

        scratch.remove().unwrap();

        assert!(fs::symlink_metadata(scratch.dir()).is_err());
        scratch.remove().unwrap();
    }
}
