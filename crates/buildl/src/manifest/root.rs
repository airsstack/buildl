//! Finding the workspace a command was started in.
//!
//! Its own file because the root is found before anything is read: a directory is a workspace
//! root because it holds `buildl.toml`, whatever that file says. Every adapter that touches the
//! workspace is handed the path found here, so this is the one place it is made canonical.
//!
//! Responsibilities: [`WorkspaceRoot`], and the name of the manifest file.
//!
//! Non-responsibilities: reading the manifest. A `buildl.toml` that cannot be read, or says
//! something this version refuses, fails where it is read.

use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{Diagnostic, Error, PortName, Result};

/// The name of the file that marks a workspace root and holds its manifest.
pub(crate) const MANIFEST_FILE: &str = "buildl.toml";

/// The canonical, absolute directory that holds a workspace's `buildl.toml`, and where the
/// search for it began.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use std::path::Path;
///
/// use buildl::manifest::WorkspaceRoot;
///
/// let workspace = tempfile::tempdir()?;
/// fs::write(workspace.path().join("buildl.toml"), "")?;
/// fs::create_dir_all(workspace.path().join("lib/text"))?;
///
/// let root = WorkspaceRoot::find(&workspace.path().join("lib/text"))?;
/// assert_eq!(root.as_path(), fs::canonicalize(workspace.path())?);
/// assert_eq!(root.within(), Path::new("lib/text"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRoot {
    root: PathBuf,
    within: PathBuf,
}

impl WorkspaceRoot {
    /// The nearest directory, `start` itself or one above it, that holds `buildl.toml`.
    ///
    /// `start` is made canonical first, so a root reached through a symbolic link is the real
    /// directory. A directory holds the manifest when anything at all is named `buildl.toml` in
    /// it: the nearest one wins, and one that turns out to be unreadable fails when it is read
    /// instead of being passed over for a workspace further up.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], when `start` cannot be made
    /// canonical, and when neither it nor any directory above it holds `buildl.toml`.
    pub fn find(start: &Path) -> Result<Self> {
        let canonical = fs::canonicalize(start)
            .map_err(|error| failed(format!("{}: {error}", start.display())))?;
        canonical
            .ancestors()
            .find(|directory| fs::symlink_metadata(directory.join(MANIFEST_FILE)).is_ok())
            .map(|root| Self {
                root: root.to_path_buf(),
                within: canonical
                    .strip_prefix(root)
                    .map_or_else(|_| PathBuf::new(), Path::to_path_buf),
            })
            .ok_or_else(|| {
                failed(format!(
                    "no {MANIFEST_FILE} in {} or any directory above it",
                    canonical.display()
                ))
            })
    }

    /// The root: canonical and absolute.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.root
    }

    /// Where the search began, relative to the root; empty when it began at the root.
    #[must_use]
    pub fn within(&self) -> &Path {
        &self.within
    }
}

/// A failure to find a workspace, as the manifest adapter's error.
fn failed(message: String) -> Error {
    Error::Adapter {
        port: PortName::Manifest,
        diagnostic: Diagnostic::new(message),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use super::WorkspaceRoot;

    /// A temp directory holding `ws/a/b/c`, with a manifest in `ws`; and its canonical path.
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let top = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(top.join("ws/a/b/c")).unwrap();
        fs::write(top.join("ws/buildl.toml"), "").unwrap();
        (dir, top)
    }

    #[test]
    fn finds_the_nearest_manifest_above_the_start() {
        let (_dir, top) = tree();
        let outer = WorkspaceRoot::find(&top.join("ws/a/b/c")).unwrap();
        assert_eq!(outer.as_path(), top.join("ws"));

        fs::write(top.join("ws/a/buildl.toml"), "").unwrap();
        let nested = WorkspaceRoot::find(&top.join("ws/a/b/c")).unwrap();
        assert_eq!(nested.as_path(), top.join("ws/a"));
    }

    #[test]
    fn a_manifest_of_any_kind_marks_the_root() {
        let (_dir, top) = tree();
        fs::create_dir(top.join("ws/a/buildl.toml")).unwrap();
        symlink(top.join("nowhere"), top.join("ws/a/b/buildl.toml")).unwrap();
        assert_eq!(
            WorkspaceRoot::find(&top.join("ws/a")).unwrap().as_path(),
            top.join("ws/a")
        );
        assert_eq!(
            WorkspaceRoot::find(&top.join("ws/a/b/c"))
                .unwrap()
                .as_path(),
            top.join("ws/a/b")
        );
    }

    #[test]
    fn a_start_reached_through_a_link_gives_the_real_root() {
        let (_dir, top) = tree();
        fs::write(top.join("ws/a/buildl.toml"), "").unwrap();
        symlink(top.join("ws/a/b"), top.join("link")).unwrap();
        let root = WorkspaceRoot::find(&top.join("link")).unwrap();
        assert_eq!(root.as_path(), top.join("ws/a"));
        assert_eq!(root.within(), Path::new("b"));
    }

    #[test]
    fn no_manifest_up_to_the_filesystem_root_is_an_error() {
        let error = WorkspaceRoot::find(Path::new("/")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "manifest: no buildl.toml in / or any directory above it"
        );
    }

    #[test]
    fn a_start_that_does_not_exist_is_an_error_naming_it() {
        let (_dir, top) = tree();
        let start = top.join("ws/missing");
        let error = WorkspaceRoot::find(&start).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "manifest: {}: No such file or directory (os error 2)",
                start.display()
            )
        );
    }

    #[test]
    fn within_is_the_start_relative_to_the_root_and_empty_at_the_root() {
        let (_dir, top) = tree();
        let below = WorkspaceRoot::find(&top.join("ws/a/b")).unwrap();
        assert_eq!(below.within(), Path::new("a/b"));
        let at_root = WorkspaceRoot::find(&top.join("ws")).unwrap();
        assert_eq!(at_root.within(), Path::new(""));
    }
}
