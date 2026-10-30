//! `buildl.lock`: what each git plugin was fetched as, kept beside `buildl.toml`.
//!
//! Its own file because the lock is a file of the workspace, committed with it, in a format of
//! its own: canonical JSON, one entry per git plugin, in name order. A path plugin has no entry.
//!
//! Responsibilities: [`Lock`], [`LockEntry`], and reading and writing the file.
//!
//! Non-responsibilities: deciding whether an entry matches what `buildl.toml` names, and
//! digesting a stored plugin, which the callers do with [`tree_digest`](super::tree_digest).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{
    CommitId, Diagnostic, Digest, Directory, Error, GitUrl, PluginName, PortName, Result, Rev,
};
use serde::{Deserialize, Serialize};

use crate::state::{Access, write_whole};

/// The name of the lock, in the workspace root.
const FILE: &str = "buildl.lock";

/// What one git plugin was fetched as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockEntry {
    /// The full commit `rev` resolved to.
    pub commit: CommitId,
    /// The folder of the repository that holds the plugin; the root, written `""`, when the
    /// plugin is the whole repository.
    pub dir: Directory,
    /// The repository, as a URL.
    pub git: GitUrl,
    /// The revision `buildl.toml` named, as it wrote it.
    pub rev: Rev,
    /// The digest of the plugin's files: see [`tree_digest`](super::tree_digest).
    pub sha256: Digest,
}

/// The entries of `buildl.lock`, by plugin name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Lock {
    entries: BTreeMap<PluginName, LockEntry>,
}

/// The failure to do `what` to the lock at `file`, which came through `port`.
fn failed(port: PortName, what: &str, file: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", file.display())),
    }
}

impl Lock {
    /// The lock of the workspace at `root`. A workspace with no `buildl.lock` has an empty one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CorruptState`], naming the file, when its bytes are not what
    /// [`Lock::write`] writes, and an adapter error for [`PortName::Manifest`], naming the
    /// file, when it is there and cannot be read: it is a file of the workspace, as the
    /// manifest is.
    pub fn read(root: &Path) -> Result<Self> {
        let file = Self::file(root);
        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(failed(PortName::Manifest, "reading", &file, &error)),
        };
        canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
            file,
            source: Box::new(source),
        })
    }

    /// Makes this the whole of the lock of the workspace at `root`, staging the new file in
    /// `staging` first, which must be on the same filesystem as `root`.
    ///
    /// # Errors
    ///
    /// Returns the error of [`canonical::to_vec`] when the lock cannot become canonical JSON,
    /// and an adapter error for [`PortName::PluginStore`], naming the file, when the file
    /// cannot be written.
    pub fn write(&self, root: &Path, staging: &Path) -> Result<()> {
        let file = Self::file(root);
        let bytes = canonical::to_vec(self)?;
        write_whole(staging, &file, &bytes, Access::Rewritable)
            .map_err(|error| failed(PortName::PluginStore, "writing", &file, &error))
    }

    /// The entry for `plugin`, if the lock has one.
    #[must_use]
    pub fn get(&self, plugin: &PluginName) -> Option<&LockEntry> {
        self.entries.get(plugin)
    }

    /// Makes `entry` the entry for `plugin`, replacing any it had.
    pub fn insert(&mut self, plugin: PluginName, entry: LockEntry) {
        self.entries.insert(plugin, entry);
    }

    /// The path of the lock of the workspace at `root`.
    fn file(root: &Path) -> PathBuf {
        root.join(FILE)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;

    use buildl_core::json::canonical;
    use buildl_core::{CommitId, Digest, Directory, Error, GitUrl, PluginName, PortName, Rev};

    use super::{Lock, LockEntry};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn entry(rev: &str) -> LockEntry {
        LockEntry {
            commit: CommitId::parse(COMMIT).unwrap(),
            dir: Directory::root(),
            git: GitUrl::github("acme/buildl-go").unwrap(),
            rev: Rev::parse(rev).unwrap(),
            sha256: Digest::of(b"tree"),
        }
    }

    fn name(raw: &str) -> PluginName {
        PluginName::parse(raw).unwrap()
    }

    #[test]
    fn a_lock_round_trips_canonically() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join(".buildl");
        let mut lock = Lock::default();
        lock.insert(name("zig"), entry(COMMIT));
        lock.insert(name("go"), entry("v1.2.0"));

        lock.write(dir.path(), &staging).unwrap();

        let bytes = fs::read(dir.path().join("buildl.lock")).unwrap();
        assert_eq!(bytes, canonical::to_vec(&lock).unwrap());
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            text.find("\"go\"").unwrap() < text.find("\"zig\"").unwrap(),
            "{text}"
        );
        assert!(text.contains(r#""rev":"v1.2.0""#), "{text}");
        assert!(text.contains(&format!(r#""rev":"{COMMIT}""#)), "{text}");

        let read = Lock::read(dir.path()).unwrap();
        assert_eq!(read, lock);
        assert_eq!(read.get(&name("go")), Some(&entry("v1.2.0")));
        assert_eq!(read.get(&name("none")), None);
    }

    #[test]
    fn a_root_row_writes_an_empty_dir() {
        let mut lock = Lock::default();
        lock.insert(name("go"), entry("v1.2.0"));

        let text = String::from_utf8(canonical::to_vec(&lock).unwrap()).unwrap();

        assert!(text.contains(r#""dir":"""#), "{text}");
    }

    #[test]
    fn a_row_with_dir_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join(".buildl");
        let row = LockEntry {
            dir: Directory::parse("plugins/c").unwrap(),
            ..entry("v1.2.0")
        };
        let mut lock = Lock::default();
        lock.insert(name("c"), row.clone());

        lock.write(dir.path(), &staging).unwrap();

        let text = fs::read_to_string(dir.path().join("buildl.lock")).unwrap();
        assert!(text.contains(r#""dir":"plugins/c""#), "{text}");
        let read = Lock::read(dir.path()).unwrap();
        assert_eq!(read.get(&name("c")), Some(&row));
    }

    #[test]
    fn a_row_without_dir_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("buildl.lock");
        let mut lock = Lock::default();
        lock.insert(name("go"), entry("v1.2.0"));
        let good = String::from_utf8(canonical::to_vec(&lock).unwrap()).unwrap();
        let without = good.replace(r#""dir":"","#, "");
        assert_ne!(without, good);
        fs::write(&file, without.as_bytes()).unwrap();

        let error = Lock::read(dir.path()).unwrap_err();

        assert!(matches!(error, Error::CorruptState { .. }), "{error:?}");
    }

    #[test]
    fn a_missing_lock_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Lock::read(dir.path()).unwrap();
        assert_eq!(lock, Lock::default());
        assert!(lock.get(&name("go")).is_none());
    }

    #[test]
    fn a_damaged_lock_is_corrupt_state() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("buildl.lock");
        let mut lock = Lock::default();
        lock.insert(name("go"), entry("v1.2.0"));
        let good = String::from_utf8(canonical::to_vec(&lock).unwrap()).unwrap();

        for damaged in [
            "{".to_owned(),
            format!(" {good}"),
            good.replace(COMMIT, "abc"),
            good.replace("\"git\"", "\"url\""),
        ] {
            fs::write(&file, damaged.as_bytes()).unwrap();
            let error = Lock::read(dir.path()).unwrap_err();
            let Error::CorruptState { file: named, .. } = error else {
                unreachable!("a damaged lock is corrupt state, got {error:?}");
            };
            assert_eq!(named, file, "{damaged}");
        }
    }

    #[test]
    fn a_lock_that_cannot_be_read_is_an_adapter_failure() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("buildl.lock")).unwrap();
        let error = Lock::read(dir.path()).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::Manifest,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_lock_that_cannot_be_written_is_a_plugin_store_failure() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("buildl.lock")).unwrap();
        let mut lock = Lock::default();
        lock.insert(name("go"), entry("v1.2.0"));
        let error = lock
            .write(dir.path(), &dir.path().join(".buildl"))
            .unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::PluginStore,
                    ..
                }
            ),
            "{error:?}"
        );
    }
}
