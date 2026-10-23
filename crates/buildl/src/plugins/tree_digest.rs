//! The one digest of a plugin's files.
//!
//! Its own file because the lock records this digest when a plugin is fetched and the check
//! recomputes it on every command, and the two must agree on what is digested: every regular
//! file of the directory, by path, with its bytes and whether it may be run, as an
//! [`OutputManifest`] tree, so a plugin is digested as an output directory is.
//!
//! Responsibilities: [`tree_digest`].
//!
//! Non-responsibilities: where the directory is, and what a differing digest means.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{
    Diagnostic, Digest, Error, FileMode, OutputManifest, PluginName, PortName, Result, Tree,
    TreeEntry, TreePath,
};

/// The directory at the top of a checkout that is git's own, and not a file of the plugin.
const GIT_DIR: &str = ".git";

/// The digest of the manifest of the regular files below `dir`.
///
/// Files are digested by bytes, with the one fact about their permissions that is kept:
/// whether any execute bit is set. The `.git` entry at the top level is skipped; one further
/// down is digested like any other. A directory holds no entry of its own, so one with no file
/// below it leaves no trace.
///
/// # Errors
///
/// Returns [`Error::PluginTree`] for the first path, walking each directory in name order, that
/// is a symbolic link, a special file, or has a name that is not Unicode;
/// [`Error::InvalidTree`] when there is no file at all; and an adapter error for
/// [`PortName::PluginStore`] when `dir`, or something in it, cannot be read.
pub fn tree_digest(plugin: &PluginName, dir: &Path) -> Result<Digest> {
    let mut entries = Vec::new();
    collect(plugin, dir, dir, &mut entries)?;
    let manifest = OutputManifest::Tree(Tree::new(entries)?);
    Ok(Digest::of(&canonical::to_vec(&manifest)?))
}

/// Adds an entry to `entries` for each regular file below `at`, in name order.
fn collect(plugin: &PluginName, top: &Path, at: &Path, entries: &mut Vec<TreeEntry>) -> Result<()> {
    let mut children: Vec<PathBuf> = fs::read_dir(at)
        .and_then(|listing| {
            listing
                .map(|child| child.map(|child| child.path()))
                .collect()
        })
        .map_err(|error| unread(at, &error))?;
    children.sort();
    for path in children {
        if at == top && path.file_name().is_some_and(|name| name == GIT_DIR) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| unread(&path, &error))?;
        if metadata.is_dir() {
            collect(plugin, top, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(entry(plugin, top, &path, &metadata)?);
        } else {
            return Err(refused(plugin, &path));
        }
    }
    Ok(())
}

/// The entry of the regular file at `path`, below `top`.
fn entry(
    plugin: &PluginName,
    top: &Path,
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<TreeEntry> {
    let tree_path = path
        .strip_prefix(top)
        .ok()
        .and_then(Path::to_str)
        .and_then(|relative| TreePath::parse(relative).ok())
        .ok_or_else(|| refused(plugin, path))?;
    let bytes = fs::read(path).map_err(|error| unread(path, &error))?;
    Ok(TreeEntry {
        path: tree_path,
        digest: Digest::of(&bytes),
        mode: FileMode::of_unix_bits(metadata.permissions().mode()),
    })
}

/// The refusal of `path`, which a plugin may not hold.
fn refused(plugin: &PluginName, path: &Path) -> Error {
    Error::PluginTree {
        plugin: plugin.clone(),
        path: path.to_path_buf(),
    }
}

/// The failure to read `path`.
fn unread(path: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::PluginStore,
        diagnostic: Diagnostic::new(format!("reading {}: {error}", path.display())),
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
    use std::path::Path;

    use buildl_core::{Error, PluginName};

    use super::tree_digest;

    fn plugin() -> PluginName {
        PluginName::parse("go").unwrap()
    }

    /// A directory holding `lib/a.lua`, `b.lua` and a `.git` directory.
    fn checkout() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("lib")).unwrap();
        fs::write(dir.path().join("lib/a.lua"), b"a\n").unwrap();
        fs::write(dir.path().join("b.lua"), b"b\n").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        dir
    }

    fn digest_of(dir: &Path) -> buildl_core::Digest {
        tree_digest(&plugin(), dir).unwrap()
    }

    #[test]
    fn same_files_same_digest() {
        let (first, second) = (checkout(), checkout());
        assert_eq!(digest_of(first.path()), digest_of(second.path()));
    }

    #[test]
    fn an_edited_file_changes_the_digest() {
        let dir = checkout();
        let before = digest_of(dir.path());
        fs::write(dir.path().join("lib/a.lua"), b"changed\n").unwrap();
        assert_ne!(digest_of(dir.path()), before);
    }

    #[test]
    fn the_executable_bit_changes_the_digest() {
        let dir = checkout();
        let before = digest_of(dir.path());
        fs::set_permissions(dir.path().join("b.lua"), Permissions::from_mode(0o755)).unwrap();
        assert_ne!(digest_of(dir.path()), before);
    }

    #[test]
    fn dot_git_is_ignored() {
        let dir = checkout();
        let before = digest_of(dir.path());
        fs::write(dir.path().join(".git/HEAD"), b"other\n").unwrap();
        fs::write(dir.path().join(".git/extra"), b"x").unwrap();
        symlink("nowhere", dir.path().join(".git/link")).unwrap();
        assert_eq!(digest_of(dir.path()), before);
    }

    #[test]
    fn a_nested_dot_git_is_digested() {
        let dir = checkout();
        let before = digest_of(dir.path());
        fs::create_dir(dir.path().join("lib/.git")).unwrap();
        fs::write(dir.path().join("lib/.git/HEAD"), b"x").unwrap();
        assert_ne!(digest_of(dir.path()), before);
    }

    #[test]
    fn a_symlink_is_refused_as_plugin_tree() {
        let dir = checkout();
        symlink("b.lua", dir.path().join("lib/link")).unwrap();
        let error = tree_digest(&plugin(), dir.path()).unwrap_err();
        let Error::PluginTree {
            plugin: named,
            path,
        } = error
        else {
            unreachable!("a link is refused as a plugin tree, got {error:?}");
        };
        assert_eq!(named, plugin());
        assert_eq!(path, dir.path().join("lib/link"));
    }

    /// Linux only: other filesystems, APFS among them, refuse to create a name that is not
    /// Unicode.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_unicode_is_refused_as_plugin_tree() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dir = checkout();
        let odd = dir
            .path()
            .join("lib")
            .join(OsStr::from_bytes(b"caf\xe9.lua"));
        fs::write(&odd, b"x").unwrap();
        let error = tree_digest(&plugin(), dir.path()).unwrap_err();
        let Error::PluginTree { path, .. } = error else {
            unreachable!("a name that is not Unicode is refused as a plugin tree, got {error:?}");
        };
        assert_eq!(path, odd);
    }

    #[test]
    fn a_directory_without_files_is_not_a_tree() {
        let dir = tempfile::tempdir().unwrap();
        let error = tree_digest(&plugin(), dir.path()).unwrap_err();
        assert!(matches!(error, Error::InvalidTree { .. }), "{error:?}");
    }

    #[test]
    fn a_missing_directory_is_a_store_failure() {
        let dir = tempfile::tempdir().unwrap();
        let error = tree_digest(&plugin(), &dir.path().join("absent")).unwrap_err();
        assert!(matches!(error, Error::Adapter { .. }), "{error:?}");
    }
}
