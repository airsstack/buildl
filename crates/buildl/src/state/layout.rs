//! Where each file of a workspace's state directory is.
//!
//! Its own file because the names inside the state directory are a format: every module that
//! keeps a file there asks this type for the path, so the name of a file is written once.
//!
//! Responsibilities: [`StateLayout`].
//!
//! Non-responsibilities: making, reading or writing anything. A layout is a set of paths, and
//! building one touches nothing on disk.

use std::path::{Path, PathBuf};

use buildl_core::Directory;

/// Where each file of a workspace's state directory is.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use buildl::state::StateLayout;
/// use buildl_core::Directory;
///
/// let layout = StateLayout::new(Path::new("/work"), &Directory::parse(".buildl")?);
/// assert_eq!(layout.dir(), Path::new("/work/.buildl"));
/// assert_eq!(layout.cache(), Path::new("/work/.buildl/cache.json"));
/// assert_eq!(layout.blob_staging(), Path::new("/work/.buildl/cas/tmp"));
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateLayout {
    dir: PathBuf,
}

impl StateLayout {
    /// The layout of the state directory `state` of the workspace at `root`.
    #[must_use]
    pub fn new(root: &Path, state: &Directory) -> Self {
        Self {
            dir: root.join(state.as_str()),
        }
    }

    /// The state directory itself.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `lock`: the file a process locks while it uses the directory.
    #[must_use]
    pub fn lock(&self) -> PathBuf {
        self.dir.join("lock")
    }

    /// `cache.json`: the row of every target built so far.
    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.dir.join("cache.json")
    }

    /// `cache.journal`: the rows recorded since `cache.json` was last written.
    #[must_use]
    pub fn journal(&self) -> PathBuf {
        self.dir.join("cache.journal")
    }

    /// `statcache.json`: the digests of files that have not changed since they were hashed.
    #[must_use]
    pub fn stat_cache(&self) -> PathBuf {
        self.dir.join("statcache.json")
    }

    /// `log.jsonl`: one line for every action that ended.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.dir.join("log.jsonl")
    }

    /// `graph.json`: the graph of the last build.
    #[must_use]
    pub fn graph(&self) -> PathBuf {
        self.dir.join("graph.json")
    }

    /// `cas`: the directory content is held in, by digest.
    #[must_use]
    pub fn blobs(&self) -> PathBuf {
        self.dir.join("cas")
    }

    /// `cas/tmp`: where a blob is written before it gets its name.
    #[must_use]
    pub fn blob_staging(&self) -> PathBuf {
        self.blobs().join("tmp")
    }

    /// `plugins`: the directory fetched plugins are stored in, one directory per plugin and
    /// commit.
    #[must_use]
    pub fn plugins(&self) -> PathBuf {
        self.dir.join("plugins")
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::Path;

    use buildl_core::Directory;

    use super::StateLayout;

    fn layout() -> StateLayout {
        StateLayout::new(Path::new("/work"), &Directory::parse(".buildl").unwrap())
    }

    #[test]
    fn the_state_directory_is_joined_to_the_workspace_root() {
        assert_eq!(layout().dir(), Path::new("/work/.buildl"));
        let nested = StateLayout::new(Path::new("/work"), &Directory::parse("var/state").unwrap());
        assert_eq!(nested.dir(), Path::new("/work/var/state"));
    }

    #[test]
    fn every_file_has_its_name_directly_in_the_state_directory() {
        let layout = layout();
        assert_eq!(layout.lock(), Path::new("/work/.buildl/lock"));
        assert_eq!(layout.cache(), Path::new("/work/.buildl/cache.json"));
        assert_eq!(layout.journal(), Path::new("/work/.buildl/cache.journal"));
        assert_eq!(
            layout.stat_cache(),
            Path::new("/work/.buildl/statcache.json")
        );
        assert_eq!(layout.log(), Path::new("/work/.buildl/log.jsonl"));
        assert_eq!(layout.graph(), Path::new("/work/.buildl/graph.json"));
    }

    #[test]
    fn blobs_are_staged_inside_the_directory_that_holds_them() {
        let layout = layout();
        assert_eq!(layout.blobs(), Path::new("/work/.buildl/cas"));
        assert_eq!(layout.blob_staging(), Path::new("/work/.buildl/cas/tmp"));
    }

    #[test]
    fn fetched_plugins_are_kept_in_a_directory_of_their_own() {
        assert_eq!(layout().plugins(), Path::new("/work/.buildl/plugins"));
    }
}
