//! The check that every plugin a workspace names is there, and is what it was locked as.
//!
//! Its own file because the check is one rule with one table of outcomes: a path plugin must be
//! a directory that exists; a git plugin must have a lock entry for the source `buildl.toml`
//! names, a stored directory for that entry's commit, and files that digest to the entry's
//! `sha256`. It reads the lock and the store and never the network.
//!
//! Responsibilities: [`check`] and [`Checked`].
//!
//! Non-responsibilities: fetching a plugin, and what is done with a plugin's directory once it
//! is checked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use buildl_core::{Error, PluginName, PluginSource, Result};

use super::lock::{Lock, LockEntry};
use super::tree_digest::tree_digest;
use crate::state::StateLayout;

/// The directory of each plugin that passed [`check`], by plugin name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checked {
    directories: BTreeMap<PluginName, PathBuf>,
}

impl Checked {
    /// The absolute directory of `plugin`, if the workspace names it.
    #[must_use]
    pub fn get(&self, plugin: &PluginName) -> Option<&Path> {
        self.directories.get(plugin).map(PathBuf::as_path)
    }

    /// Every plugin with its absolute directory, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&PluginName, &Path)> {
        self.directories
            .iter()
            .map(|(plugin, directory)| (plugin, directory.as_path()))
    }
}

/// Finds every plugin in `sources` ready to use, and returns where each one is.
///
/// `root` is the workspace root, which a path plugin's directory is relative to. A git plugin is
/// kept in `state`, in a directory named for the plugin and the commit `lock` pins it to.
///
/// # Errors
///
/// Returns [`Error::PluginNotFetched`] for the first plugin, in name order, that is a path
/// plugin whose directory is not there, or a git plugin with no entry in `lock` for the source
/// `sources` names or with no directory in the store. Returns [`Error::PluginChanged`] for the
/// first git plugin whose stored files are not the ones its entry pins: they digest to something
/// else, there are none, or one is not a regular file. Returns the error of
/// [`tree_digest`] when the store cannot be read.
pub fn check(
    root: &Path,
    state: &StateLayout,
    sources: &BTreeMap<PluginName, PluginSource>,
    lock: &Lock,
) -> Result<Checked> {
    let mut directories = BTreeMap::new();
    for (plugin, source) in sources {
        let directory = match source {
            PluginSource::Path(directory) => {
                let at = root.join(directory.as_str());
                if !at.is_dir() {
                    return Err(not_fetched(
                        plugin,
                        format!("its directory {} is not there", at.display()),
                    ));
                }
                at
            }
            PluginSource::Git { url, rev, dir } => {
                let entry = lock
                    .get(plugin)
                    .filter(|entry| {
                        entry.git == *url && entry.rev == *rev && entry.dir == *dir
                    })
                    .ok_or_else(|| {
                        let folder = if dir.is_root() {
                            String::new()
                        } else {
                            format!(" in the folder {}", dir.as_str())
                        };
                        not_fetched(
                            plugin,
                            format!(
                                "buildl.lock does not pin {url} at {rev}{folder}; run `buildl plugins fetch`"
                            ),
                        )
                    })?;
                stored(plugin, state, entry)?
            }
        };
        directories.insert(plugin.clone(), directory);
    }
    Ok(Checked { directories })
}

/// The store directory of `plugin` as `entry` pins it, once its files are seen to be the ones
/// the entry digests.
fn stored(plugin: &PluginName, state: &StateLayout, entry: &LockEntry) -> Result<PathBuf> {
    let directory = state
        .plugins()
        .join(plugin.to_string())
        .join(entry.commit.as_str());
    if !directory.is_dir() {
        return Err(not_fetched(
            plugin,
            format!(
                "{} is not there; run `buildl plugins fetch`",
                directory.display()
            ),
        ));
    }
    match tree_digest(plugin, &directory) {
        Ok(digest) if digest == entry.sha256 => Ok(directory),
        Ok(digest) => Err(changed(
            plugin,
            format!(
                "its files digest to {digest} and buildl.lock pins {}",
                entry.sha256
            ),
        )),
        Err(Error::PluginTree { path, .. }) => Err(changed(
            plugin,
            format!(
                "{} is a link, a special file or has a name that is not Unicode, and a plugin holds none of those",
                path.display()
            ),
        )),
        Err(Error::InvalidTree { .. }) => Err(changed(
            plugin,
            format!("{} holds no files", directory.display()),
        )),
        Err(error) => Err(error),
    }
}

/// `plugin` is not there to use, for `reason`.
fn not_fetched(plugin: &PluginName, reason: String) -> Error {
    Error::PluginNotFetched {
        plugin: plugin.clone(),
        reason,
    }
}

/// `plugin`'s files are not the ones that were locked, for `reason`.
fn changed(plugin: &PluginName, reason: String) -> Error {
    Error::PluginChanged {
        plugin: plugin.clone(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use buildl_core::{CommitId, Digest, Directory, Error, GitUrl, PluginName, PluginSource, Rev};

    use super::check;
    use crate::plugins::{Lock, LockEntry, tree_digest};
    use crate::state::StateLayout;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const URL: &str = "https://example.com/acme/go.git";

    fn go() -> PluginName {
        PluginName::parse("go").unwrap()
    }

    fn state(root: &Path) -> StateLayout {
        StateLayout::new(root, &Directory::parse(".buildl").unwrap())
    }

    fn git(rev: &str) -> BTreeMap<PluginName, PluginSource> {
        BTreeMap::from([(
            go(),
            PluginSource::Git {
                url: GitUrl::parse(URL).unwrap(),
                rev: Rev::parse(rev).unwrap(),
                dir: Directory::root(),
            },
        )])
    }

    /// A workspace at `root` whose store holds `go` as `COMMIT`, and the lock that pins it.
    fn stored(root: &Path) -> Lock {
        let directory = state(root).plugins().join("go").join(COMMIT);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("a.lua"), b"a\n").unwrap();
        let mut lock = Lock::default();
        lock.insert(
            go(),
            LockEntry {
                commit: CommitId::parse(COMMIT).unwrap(),
                dir: Directory::root(),
                git: GitUrl::parse(URL).unwrap(),
                rev: Rev::parse("v1.0.0").unwrap(),
                sha256: tree_digest(&go(), &directory).unwrap(),
            },
        );
        lock
    }

    #[test]
    fn no_plugins_is_nothing_checked() {
        let dir = tempfile::tempdir().unwrap();
        let checked = check(
            dir.path(),
            &state(dir.path()),
            &BTreeMap::new(),
            &Lock::default(),
        )
        .unwrap();
        assert_eq!(checked.iter().count(), 0);
    }

    #[test]
    fn a_path_plugin_is_its_directory_below_the_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tools/go")).unwrap();
        let sources = BTreeMap::from([(
            go(),
            PluginSource::Path(Directory::parse("tools/go").unwrap()),
        )]);

        let checked = check(dir.path(), &state(dir.path()), &sources, &Lock::default()).unwrap();

        assert_eq!(
            checked.get(&go()),
            Some(dir.path().join("tools/go").as_path())
        );
    }

    #[test]
    fn a_path_plugin_that_is_not_a_directory_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tools"), b"a file").unwrap();
        let sources =
            BTreeMap::from([(go(), PluginSource::Path(Directory::parse("tools").unwrap()))]);

        let error = check(dir.path(), &state(dir.path()), &sources, &Lock::default()).unwrap_err();

        assert!(matches!(error, Error::PluginNotFetched { .. }), "{error:?}");
    }

    #[test]
    fn a_pinned_and_stored_git_plugin_is_its_store_directory() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());

        let checked = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap();

        let expected = dir.path().join(".buildl/plugins/go").join(COMMIT);
        assert_eq!(checked.get(&go()), Some(expected.as_path()));
        assert_eq!(checked.get(&PluginName::parse("zig").unwrap()), None);
    }

    #[test]
    fn a_git_plugin_the_lock_does_not_pin_as_named_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());
        let other = BTreeMap::from([(
            go(),
            PluginSource::Git {
                url: GitUrl::parse("https://example.com/other/go.git").unwrap(),
                rev: Rev::parse("v1.0.0").unwrap(),
                dir: Directory::root(),
            },
        )]);

        for (sources, lock) in [
            (git("v1.1.0"), &lock),
            (other, &lock),
            (git("v1.0.0"), &Lock::default()),
        ] {
            let error = check(dir.path(), &state(dir.path()), &sources, lock).unwrap_err();
            let Error::PluginNotFetched { reason, .. } = &error else {
                unreachable!("an unpinned plugin is not fetched, got {error:?}");
            };
            assert!(reason.contains("buildl plugins fetch"), "{reason}");
        }
    }

    #[test]
    fn a_row_whose_dir_differs_is_not_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = stored(dir.path());
        let entry = lock.get(&go()).unwrap().clone();
        lock.insert(
            go(),
            LockEntry {
                dir: Directory::parse("plugins/a").unwrap(),
                ..entry
            },
        );
        let sources = BTreeMap::from([(
            go(),
            PluginSource::Git {
                url: GitUrl::parse(URL).unwrap(),
                rev: Rev::parse("v1.0.0").unwrap(),
                dir: Directory::parse("plugins/b").unwrap(),
            },
        )]);

        let error = check(dir.path(), &state(dir.path()), &sources, &lock).unwrap_err();

        let Error::PluginNotFetched { reason, .. } = &error else {
            unreachable!("an unpinned plugin is not fetched, got {error:?}");
        };
        assert!(reason.contains("does not pin"), "{reason}");
        assert!(reason.contains("folder plugins/b"), "{reason}");
        assert!(reason.contains("buildl plugins fetch"), "{reason}");
    }

    #[test]
    fn a_pinned_plugin_with_no_store_directory_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());
        fs::remove_dir_all(dir.path().join(".buildl/plugins")).unwrap();

        let error = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap_err();

        assert!(matches!(error, Error::PluginNotFetched { .. }), "{error:?}");
    }

    #[test]
    fn files_that_digest_differently_are_changed() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());
        let file = dir
            .path()
            .join(".buildl/plugins/go")
            .join(COMMIT)
            .join("a.lua");
        fs::write(file, b"edited\n").unwrap();

        let error = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap_err();

        assert!(matches!(error, Error::PluginChanged { .. }), "{error:?}");
    }

    #[test]
    fn a_link_in_the_store_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());
        let store = dir.path().join(".buildl/plugins/go").join(COMMIT);
        symlink("a.lua", store.join("link")).unwrap();

        let error = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap_err();

        assert!(matches!(error, Error::PluginChanged { .. }), "{error:?}");
    }

    #[test]
    fn a_store_directory_with_no_files_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        let lock = stored(dir.path());
        let store = dir.path().join(".buildl/plugins/go").join(COMMIT);
        fs::remove_file(store.join("a.lua")).unwrap();

        let error = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap_err();

        assert!(matches!(error, Error::PluginChanged { .. }), "{error:?}");
    }

    #[test]
    fn a_digest_the_lock_got_wrong_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = stored(dir.path());
        let entry = lock.get(&go()).unwrap().clone();
        lock.insert(
            go(),
            LockEntry {
                sha256: Digest::of(b"something else"),
                ..entry
            },
        );

        let error = check(dir.path(), &state(dir.path()), &git("v1.0.0"), &lock).unwrap_err();

        assert!(matches!(error, Error::PluginChanged { .. }), "{error:?}");
    }
}
