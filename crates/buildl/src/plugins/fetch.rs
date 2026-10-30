//! Fetching the git plugins a workspace names: pinning each in `buildl.lock` and keeping its
//! files in the plugin store.
//!
//! Its own file because fetching is one rule applied to each git plugin in name order: a plugin
//! whose lock entry matches what `buildl.toml` names and whose stored files are the ones the
//! entry digests needs nothing; any other is fetched into a staging directory of its own,
//! checked against the entry if there is one, and only then moved to its place in the store. The
//! lock is written once, after every plugin, and only when it differs from the one read; what
//! the new lock does not name is then removed from the store.
//!
//! Responsibilities: [`fetch`], [`Fetched`], [`FetchedPlugin`] and [`FetchStatus`].
//!
//! Non-responsibilities: holding the state lock, which the caller does while this writes, and
//! reading `buildl.toml`. A path plugin is not fetched and has no entry.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use buildl_core::{
    CommitId, Diagnostic, Error, GitUrl, PluginName, PluginSource, PortName, Result, Rev,
};

use super::git::Git;
use super::lock::{Lock, LockEntry};
use super::tree_digest::tree_digest;
use crate::search_path::SearchPath;
use crate::state::{StateLayout, clear_path};

/// The start of the name of the directory a plugin is fetched into before it is stored. It holds
/// `~`, which no plugin name does, so no plugin's directory can have this name.
const STAGING: &str = "fetching~";

/// What [`fetch`] did for one plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    /// The plugin's files were fetched and stored.
    Fetched,
    /// The plugin was already pinned and stored as `buildl.toml` names it.
    Unchanged,
}

/// One git plugin as [`fetch`] left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPlugin {
    /// The plugin.
    pub plugin: PluginName,
    /// The full commit the plugin is pinned to.
    pub commit: CommitId,
    /// Whether anything was fetched for it.
    pub status: FetchStatus,
}

/// Every git plugin [`fetch`] pinned, in name order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fetched {
    plugins: Vec<FetchedPlugin>,
}

impl Fetched {
    /// Every git plugin, in name order.
    pub fn iter(&self) -> impl Iterator<Item = &FetchedPlugin> {
        self.plugins.iter()
    }
}

/// Pins every git plugin in `sources` in the lock of the workspace at `root` and stores its
/// files in `state`, then drops from the lock and the store whatever `sources` no longer names.
///
/// `git` is the system `git` found on `search`, and is looked for only when a plugin has to be
/// fetched. A plugin is fetched when the lock has no entry for the source `sources` names, or
/// its stored files are missing or are not the ones the entry digests; in the second case the
/// files fetched have to be the ones the entry pins.
///
/// # Errors
///
/// Returns [`Error::PluginFetch`] when `git` is not on `search`, cannot be run, or refuses the
/// repository or the revision. Returns [`Error::PluginTree`] when a fetched plugin holds a
/// symbolic link, a special file or a name that is not Unicode, and [`Error::PluginNotFetched`]
/// when it holds no file. Returns [`Error::PluginChanged`] when a plugin the lock pins now fetches as
/// another commit, as when its tag has moved, or as other files. Returns an adapter error for
/// [`PortName::PluginStore`] when the store or the lock cannot be written, and the error of
/// [`Lock::read`] when the lock is damaged. The first failure ends the run: the lock is left as
/// it was, and the directory of the plugin that failed holds nothing of the failed fetch.
pub fn fetch(
    root: &Path,
    state: &StateLayout,
    sources: &BTreeMap<PluginName, PluginSource>,
    search: &SearchPath,
) -> Result<Fetched> {
    let old = Lock::read(root)?;
    let mut new = Lock::default();
    let mut plugins = Vec::new();
    for (plugin, source) in sources {
        let PluginSource::Git { url, rev, .. } = source else {
            continue;
        };
        let pinned = old
            .get(plugin)
            .filter(|entry| entry.git == *url && entry.rev == *rev);
        let (entry, status) = pin(plugin, url, rev, pinned, state, search)?;
        plugins.push(FetchedPlugin {
            plugin: plugin.clone(),
            commit: entry.commit.clone(),
            status,
        });
        new.insert(plugin.clone(), entry);
    }
    if new != old {
        new.write(root, state.dir())?;
    }
    prune(&state.plugins(), &plugins)?;
    Ok(Fetched { plugins })
}

/// The entry that pins `plugin` at `rev` of `url`, with its files in the store. `pinned` is the
/// lock's entry for that source, if it has one.
fn pin(
    plugin: &PluginName,
    url: &GitUrl,
    rev: &Rev,
    pinned: Option<&LockEntry>,
    state: &StateLayout,
    search: &SearchPath,
) -> Result<(LockEntry, FetchStatus)> {
    if let Some(entry) = pinned
        && stored(plugin, state, entry)?
    {
        return Ok((entry.clone(), FetchStatus::Unchanged));
    }
    let git = Git::locate(plugin, search)?;
    let staging = state.plugins().join(format!("{STAGING}{plugin}"));
    let fetched = bring_in(&git, plugin, url, rev, pinned, &staging, state);
    if fetched.is_err() {
        // A directory that stays is removed by the next fetch, so failing to remove it now adds
        // nothing to the error that is being returned.
        drop(clear_path(&staging));
    }
    fetched.map(|entry| (entry, FetchStatus::Fetched))
}

/// Fetches `rev` of `url` into `staging`, and moves the files to their place in the store once
/// they are what `pinned`, if there is one, pins.
fn bring_in(
    git: &Git,
    plugin: &PluginName,
    url: &GitUrl,
    rev: &Rev,
    pinned: Option<&LockEntry>,
    staging: &Path,
    state: &StateLayout,
) -> Result<LockEntry> {
    clear_path(staging).map_err(|error| unwritten("clearing", staging, &error))?;
    fs::create_dir_all(staging).map_err(|error| unwritten("creating", staging, &error))?;
    git.init(staging)?;
    git.fetch_rev(staging, url, rev)?;
    git.checkout_fetched(staging)?;
    let commit = git.head_commit(staging)?;
    let dot_git = staging.join(".git");
    clear_path(&dot_git).map_err(|error| unwritten("removing", &dot_git, &error))?;
    let sha256 = tree_digest(plugin, staging).map_err(|error| match error {
        Error::InvalidTree { .. } => Error::PluginNotFetched {
            plugin: plugin.clone(),
            reason: format!("{rev} fetched as {commit}, which holds no file; a plugin needs one"),
        },
        other => other,
    })?;
    if let Some(entry) = pinned {
        if entry.commit != commit {
            return Err(changed(
                plugin,
                format!(
                    "{rev} now fetches as {commit} and buildl.lock pins {}",
                    entry.commit
                ),
            ));
        }
        if entry.sha256 != sha256 {
            return Err(changed(
                plugin,
                format!(
                    "{commit} now fetches as files that digest to {sha256} and buildl.lock pins {}",
                    entry.sha256
                ),
            ));
        }
    }
    let directory = stored_at(plugin, state, &commit);
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent).map_err(|error| unwritten("creating", parent, &error))?;
    }
    // A directory already there is what an interrupted fetch or a damaged store left.
    clear_path(&directory).map_err(|error| unwritten("clearing", &directory, &error))?;
    fs::rename(staging, &directory).map_err(|error| unwritten("storing", &directory, &error))?;
    Ok(LockEntry {
        commit,
        git: url.clone(),
        rev: rev.clone(),
        sha256,
    })
}

/// Whether the store holds the files `entry` pins for `plugin`.
///
/// Files that are missing, or are not a plugin's, are not an error: they are fetched again.
fn stored(plugin: &PluginName, state: &StateLayout, entry: &LockEntry) -> Result<bool> {
    let directory = stored_at(plugin, state, &entry.commit);
    if !directory.is_dir() {
        return Ok(false);
    }
    match tree_digest(plugin, &directory) {
        Ok(digest) => Ok(digest == entry.sha256),
        Err(Error::PluginTree { .. } | Error::InvalidTree { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Where the store keeps `plugin` as `commit`.
fn stored_at(plugin: &PluginName, state: &StateLayout, commit: &CommitId) -> PathBuf {
    state.plugins().join(plugin.as_str()).join(commit.as_str())
}

/// Removes from `plugins`, the store, everything that is not the stored directory of one of
/// `kept`: a plugin no longer named, an older commit, and a staging directory left behind.
fn prune(plugins: &Path, kept: &[FetchedPlugin]) -> Result<()> {
    let keep: BTreeMap<&str, &str> = kept
        .iter()
        .map(|one| (one.plugin.as_str(), one.commit.as_str()))
        .collect();
    for path in children(plugins)? {
        let commit = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| keep.get(name));
        let Some(commit) = commit else {
            clear_path(&path).map_err(|error| unwritten("removing", &path, &error))?;
            continue;
        };
        for old in children(&path)? {
            let is_wanted = old.file_name().and_then(|name| name.to_str()) == Some(*commit);
            if !is_wanted {
                clear_path(&old).map_err(|error| unwritten("removing", &old, &error))?;
            }
        }
    }
    Ok(())
}

/// The path of everything directly in `dir`. A directory that is not there holds nothing.
fn children(dir: &Path) -> Result<Vec<PathBuf>> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(unwritten("reading", dir, &error)),
    };
    listing
        .map(|child| child.map(|child| child.path()))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| unwritten("reading", dir, &error))
}

/// `plugin`'s files are not the ones that were locked, for `reason`.
fn changed(plugin: &PluginName, reason: String) -> Error {
    Error::PluginChanged {
        plugin: plugin.clone(),
        reason,
    }
}

/// The failure to do `what` to `path` in the plugin store.
fn unwritten(what: &str, path: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::PluginStore,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::Path;

    use buildl_core::{CommitId, Digest, Directory, GitUrl, PluginName, Rev};

    use super::{FetchStatus, FetchedPlugin, prune, stored};
    use crate::plugins::{LockEntry, tree_digest};
    use crate::state::StateLayout;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const OTHER: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn go() -> PluginName {
        PluginName::parse("go").unwrap()
    }

    fn state(root: &Path) -> StateLayout {
        StateLayout::new(root, &Directory::parse(".buildl").unwrap())
    }

    fn kept(commit: &str) -> FetchedPlugin {
        FetchedPlugin {
            plugin: go(),
            commit: CommitId::parse(commit).unwrap(),
            status: FetchStatus::Fetched,
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn prune_keeps_only_the_stored_directory_of_each_kept_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        for path in [
            format!("go/{COMMIT}"),
            format!("go/{OTHER}"),
            format!("old/{COMMIT}"),
            "fetching~go".to_owned(),
        ] {
            fs::create_dir_all(plugins.join(path)).unwrap();
        }
        fs::write(plugins.join("stray"), b"x").unwrap();
        fs::write(plugins.join("go/stray"), b"x").unwrap();

        prune(&plugins, &[kept(COMMIT)]).unwrap();

        assert_eq!(names(&plugins), ["go"]);
        assert_eq!(names(&plugins.join("go")), [COMMIT]);
    }

    #[test]
    fn prune_of_a_store_that_is_not_there_is_nothing() {
        let dir = tempfile::tempdir().unwrap();
        prune(&dir.path().join("plugins"), &[]).unwrap();
    }

    #[test]
    fn stored_files_are_the_ones_the_entry_digests_or_they_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path());
        let directory = state.plugins().join("go").join(COMMIT);
        let entry = |sha256| LockEntry {
            commit: CommitId::parse(COMMIT).unwrap(),
            git: GitUrl::parse("https://example.com/acme/go.git").unwrap(),
            rev: Rev::parse("v1").unwrap(),
            sha256,
        };

        assert!(!stored(&go(), &state, &entry(Digest::of(b"x"))).unwrap());

        fs::create_dir_all(&directory).unwrap();
        assert!(
            !stored(&go(), &state, &entry(Digest::of(b"x"))).unwrap(),
            "a directory with no file is not a plugin"
        );

        fs::write(directory.join("a.lua"), b"a\n").unwrap();
        let digest = tree_digest(&go(), &directory).unwrap();
        assert!(stored(&go(), &state, &entry(digest)).unwrap());
        assert!(!stored(&go(), &state, &entry(Digest::of(b"x"))).unwrap());

        std::os::unix::fs::symlink("/etc/hosts", directory.join("link")).unwrap();
        assert!(!stored(&go(), &state, &entry(Digest::of(b"x"))).unwrap());
    }
}
