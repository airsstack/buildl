//! The approvals a user has given a workspace, kept in a file outside it.
//!
//! Its own file because what the interactive approver remembers must survive the build, must not
//! be something a workspace can write for itself, and is read and written by no other approver.
//! The file lies in the user's data directory, named for the workspace root as the scratch
//! directory is, so one workspace has one name in both places.
//!
//! Responsibilities: [`Approvals`], the file's path, reading it and writing it.
//!
//! Non-responsibilities: asking the user, and deciding what is new; and where the data directory
//! is, which [`DataDir`](super::DataDir) answers.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{Diagnostic, Error, PortName, Result, WantedSet};

use crate::state::{Access, name_of, write_whole};

/// The approver's failure doing `what` to `file`.
fn failed(what: &str, file: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::Approver,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", file.display())),
    }
}

/// The remembered approvals of one workspace.
#[derive(Debug, Clone)]
pub struct Approvals {
    dir: PathBuf,
    file: PathBuf,
}

impl Approvals {
    /// The approvals of the workspace at `root`, kept below `data`. Touches nothing on disk.
    ///
    /// The file is `<data>/buildl/approved/<name>.json`, where `<name>` is the name `root`'s
    /// bytes give, the one the workspace's scratch directory has.
    #[must_use]
    pub fn new(data: &Path, root: &Path) -> Self {
        let name = name_of(root.as_os_str().as_encoded_bytes());
        let dir = data.join("buildl").join("approved");
        Self {
            file: dir.join(format!("{name}.json")),
            dir,
        }
    }

    /// The file.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// What has been approved. A file that is not there holds nothing.
    ///
    /// # Errors
    ///
    /// Returns `Error::CorruptState`, naming the file, when its bytes are not what
    /// [`Approvals::write`] writes, and an approver error, naming the file, when it is there and
    /// cannot be read.
    pub fn read(&self) -> Result<WantedSet> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(WantedSet::default());
            }
            Err(error) => return Err(failed("reading", &self.file, &error)),
        };
        canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
            file: self.file.clone(),
            source: Box::new(source),
        })
    }

    /// Makes `approved` the whole of what has been approved.
    ///
    /// # Errors
    ///
    /// Returns the error of [`canonical::to_vec`] when the set cannot become canonical JSON, and an
    /// approver error, naming the file, when the directory cannot be made or the file cannot be
    /// written.
    pub fn write(&self, approved: &WantedSet) -> Result<()> {
        let bytes = canonical::to_vec(approved)?;
        // Staged beside the file so the rename onto it stays on one filesystem.
        write_whole(&self.dir, &self.file, &bytes, Access::Rewritable)
            .map_err(|error| failed("writing", &self.file, &error))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use buildl_core::json::canonical;
    use buildl_core::{Directory, EnvName, Error, Label, PortName, Program, WantedSet};

    use super::Approvals;
    use crate::state::name_of;

    /// A set holding one of each kind of want.
    fn wanted() -> WantedSet {
        WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            env: BTreeSet::from([EnvName::parse("PATH").unwrap()]),
            write: BTreeSet::from([Directory::parse("out").unwrap()]),
            network: BTreeSet::from([Label::parse("//:fetch").unwrap()]),
        }
    }

    fn nothing() -> WantedSet {
        WantedSet::default()
    }

    /// Approvals for a root below a fresh data directory.
    fn approvals() -> (tempfile::TempDir, Approvals) {
        let data = tempfile::tempdir().unwrap();
        let approvals = Approvals::new(data.path(), Path::new("/work/one"));
        (data, approvals)
    }

    #[test]
    fn the_file_is_named_for_the_workspace_root() {
        let data = Path::new("/data");
        let root = Path::new("/work/one");

        let approvals = Approvals::new(data, root);

        let name = name_of(root.as_os_str().as_encoded_bytes());
        assert_eq!(
            approvals.file(),
            data.join("buildl")
                .join("approved")
                .join(format!("{name}.json"))
        );
    }

    #[test]
    fn two_roots_have_two_files() {
        let data = Path::new("/data");
        let one = Approvals::new(data, Path::new("/work/one"));
        let two = Approvals::new(data, Path::new("/work/two"));
        assert_ne!(one.file(), two.file());
        assert_eq!(
            one.file(),
            Approvals::new(data, Path::new("/work/one")).file()
        );
    }

    #[test]
    fn a_missing_file_reads_as_nothing_approved() {
        let (_data, approvals) = approvals();
        assert_eq!(approvals.read().unwrap(), nothing());
    }

    #[test]
    fn what_is_written_reads_back() {
        let (_data, approvals) = approvals();

        approvals.write(&wanted()).unwrap();
        assert_eq!(approvals.read().unwrap(), wanted());

        approvals.write(&nothing()).unwrap();
        assert_eq!(approvals.read().unwrap(), nothing());
    }

    #[test]
    fn the_file_is_canonical_json() {
        let (_data, approvals) = approvals();

        approvals.write(&wanted()).unwrap();

        assert_eq!(
            fs::read(approvals.file()).unwrap(),
            canonical::to_vec(&wanted()).unwrap()
        );
    }

    #[test]
    fn a_file_buildl_did_not_write_is_corrupt_state_naming_it() {
        let (_data, approvals) = approvals();
        fs::create_dir_all(approvals.file().parent().unwrap()).unwrap();
        fs::write(approvals.file(), b"{ \"programs\": [] }\n").unwrap();

        let error = approvals.read().unwrap_err();

        assert!(
            matches!(&error, Error::CorruptState { file, .. } if file == approvals.file()),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(approvals.file().to_str().unwrap()),
            "{error}"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_approver_error_naming_it() {
        let (_data, approvals) = approvals();
        // A directory stands where the file should be: it is there, and it cannot be read.
        fs::create_dir_all(approvals.file()).unwrap();

        let error = approvals.read().unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Approver,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(approvals.file().to_str().unwrap()),
            "{error}"
        );
    }

    #[test]
    fn an_unwritable_directory_is_an_approver_error() {
        let data = tempfile::tempdir().unwrap();
        // A file stands where the directory `buildl` should be.
        fs::write(data.path().join("buildl"), b"not a directory").unwrap();
        let approvals = Approvals::new(data.path(), Path::new("/work/one"));

        let error = approvals.write(&wanted()).unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Approver,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(approvals.file().to_str().unwrap()),
            "{error}"
        );
    }
}
