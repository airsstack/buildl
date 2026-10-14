//! [`LocalStatCache`], the stat cache of a local workspace.
//!
//! Its own file because it holds the rule that makes a recorded digest safe to reuse. A file
//! changed twice inside one tick of the filesystem's clock keeps its modification time, and its
//! whole metadata when its length is the same too, so an entry taken from a file modified that
//! recently could vouch for bytes that are gone. An entry is therefore kept only for a file last
//! modified well before the run began.
//!
//! Responsibilities: [`LocalStatCache`] and its [`StatCache`] implementation.
//!
//! Non-responsibilities: deciding whether a recorded digest may be reused, which Plan does
//! by comparing metadata; and reading a file's metadata or hashing it, which the digester does.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{
    Diagnostic, Digest, Error, FileStat, PortName, Result, SourcePath, StatCache, StatEntry,
    Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::state::{Access, StateLayout, write_whole};

/// How long before the run began a file must have been last modified for its entry to be
/// kept, in nanoseconds: two seconds, the coarsest tick a filesystem's clock is known to have.
const WINDOW_NANOS: u64 = 2_000_000_000;

/// Holds the digest of each file already hashed, with the metadata it was hashed under, across
/// runs, in `statcache.json`.
///
/// An entry is kept only when the file was last modified more than two seconds before the run
/// began. An edit the metadata cannot show leaves modification time and length as they were, so
/// it falls in the same tick of the filesystem's clock as the recorded time; no file modified
/// that long ago can still be inside its tick.
#[derive(Debug)]
pub struct LocalStatCache {
    path: PathBuf,
    staging: PathBuf,
    entries: BTreeMap<SourcePath, StatEntry>,
    trusted_before: Timestamp,
    changed: bool,
}

/// An entry as it is written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEntry {
    digest: Digest,
    modified: Timestamp,
    size: u64,
}

impl From<StatEntry> for StoredEntry {
    fn from(entry: StatEntry) -> Self {
        Self {
            digest: entry.digest,
            modified: entry.stat.modified,
            size: entry.stat.size,
        }
    }
}

impl From<StoredEntry> for StatEntry {
    fn from(stored: StoredEntry) -> Self {
        Self {
            stat: FileStat {
                modified: stored.modified,
                size: stored.size,
            },
            digest: stored.digest,
        }
    }
}

/// The cache's failure doing `what` to `path`.
fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::StatCache,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
    }
}

impl LocalStatCache {
    /// Reads `statcache.json`. `began` is when the run began, by the clock of the filesystem
    /// that stamps the files: the time the lock on the state directory was taken.
    ///
    /// A workspace that has no such file has no entries. A file that does not hold what the
    /// canonical serializer writes for a set of entries is discarded: the cache starts empty,
    /// and the next [`commit`](StatCache::commit) replaces the file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::StatCache`], when the file is there and
    /// cannot be read.
    pub fn open(layout: &StateLayout, began: Timestamp) -> Result<Self> {
        let path = layout.stat_cache();
        let (entries, changed) = match fs::read(&path) {
            Ok(bytes) => canonical::from_slice::<BTreeMap<SourcePath, StoredEntry>>(&bytes)
                .map_or_else(
                    |_| (BTreeMap::new(), true),
                    |stored| {
                        let entries = stored
                            .into_iter()
                            .map(|(path, entry)| (path, StatEntry::from(entry)))
                            .collect();
                        (entries, false)
                    },
                ),
            Err(error) if error.kind() == ErrorKind::NotFound => (BTreeMap::new(), false),
            Err(error) => return Err(failed("reading", &path, &error)),
        };
        Ok(Self {
            path,
            staging: layout.dir().to_path_buf(),
            entries,
            trusted_before: Timestamp::from_unix_nanos(
                began.as_unix_nanos().saturating_sub(WINDOW_NANOS),
            ),
            changed,
        })
    }
}

impl StatCache for LocalStatCache {
    fn entry(&self, path: &SourcePath) -> Option<StatEntry> {
        self.entries.get(path).copied()
    }

    /// Replaces the entry for `path` when the file was last modified more than two seconds
    /// before the run began. Any other entry is not kept, and the one held for `path` is
    /// removed: the file has changed since that one was taken.
    fn record(&mut self, path: SourcePath, entry: StatEntry) {
        if entry.stat.modified < self.trusted_before {
            self.changed |= self.entries.insert(path, entry) != Some(entry);
        } else {
            self.changed |= self.entries.remove(&path).is_some();
        }
    }

    /// Writes every entry to `statcache.json`, whole. Does nothing when no entry changed since
    /// the file was read or last written.
    fn commit(&mut self) -> Result<()> {
        if !self.changed {
            return Ok(());
        }
        let stored: BTreeMap<&SourcePath, StoredEntry> = self
            .entries
            .iter()
            .map(|(path, entry)| (path, StoredEntry::from(*entry)))
            .collect();
        let bytes = canonical::to_vec(&stored)?;
        write_whole(&self.staging, &self.path, &bytes, Access::Rewritable)
            .map_err(|error| failed("writing", &self.path, &error))?;
        self.changed = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, File};
    use std::path::Path;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use buildl_core::{
        Digest, Directory, Error, FileStat, PortName, SourcePath, StatCache, StatEntry, Timestamp,
    };

    use super::LocalStatCache;
    use crate::state::{StateLayout, StateLock};

    /// A new workspace whose state directory this test holds: its root, and the lock, whose
    /// stamp is when the run began.
    fn workspace() -> (tempfile::TempDir, StateLock) {
        let dir = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
        let lock = StateLock::acquire(layout).unwrap();
        (dir, lock)
    }

    /// The workspace path `raw`.
    fn source(raw: &str) -> SourcePath {
        SourcePath::parse(raw).unwrap()
    }

    /// The instant `nanos` after the epoch, as the filesystem's clock counts.
    fn instant(nanos: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_nanos(nanos)
    }

    /// Writes `bytes` to the file `name` under `root`, sets its modification time to
    /// `modified`, and returns the entry Plan records for it: the metadata the file then
    /// has, and the digest of its bytes.
    fn hashed(root: &Path, name: &str, bytes: &[u8], modified: SystemTime) -> StatEntry {
        let path = root.join(name);
        fs::write(&path, bytes).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let since = metadata.modified().unwrap().duration_since(UNIX_EPOCH);
        StatEntry {
            stat: FileStat {
                modified: Timestamp::from_unix_nanos(
                    u64::try_from(since.unwrap().as_nanos()).unwrap(),
                ),
                size: metadata.len(),
            },
            digest: Digest::of(bytes),
        }
    }

    /// An entry for a file of five bytes last modified at `nanos` after the epoch.
    fn entry_at(nanos: u64) -> StatEntry {
        StatEntry {
            stat: FileStat {
                modified: Timestamp::from_unix_nanos(nanos),
                size: 5,
            },
            digest: Digest::of(b"hello"),
        }
    }

    #[test]
    fn the_entry_of_an_old_file_comes_back_after_commit_and_open() {
        let (dir, lock) = workspace();
        let began = lock.acquired();
        let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
        assert_eq!(cache.entry(&source("a.c")), None);

        // A file whose time is fixed, so the bytes written for it are too.
        let fixed = hashed(
            dir.path(),
            "a.c",
            b"hello",
            instant(1_600_000_000_000_000_000),
        );
        cache.record(source("a.c"), fixed);
        assert_eq!(cache.entry(&source("a.c")), Some(fixed));
        cache.commit().unwrap();
        assert_eq!(
            fs::read_to_string(lock.layout().stat_cache()).unwrap(),
            concat!(
                r#"{"a.c":{"digest":"#,
                r#""2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","#,
                r#""modified":1600000000000000000,"size":5}}"#
            )
        );

        // A file last modified ten seconds before the run began.
        let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
        let old = hashed(dir.path(), "b.c", b"other", ten_seconds_old);
        cache.record(source("b.c"), old);
        cache.commit().unwrap();
        drop(cache);

        let reopened = LocalStatCache::open(lock.layout(), began).unwrap();
        assert_eq!(reopened.entry(&source("a.c")), Some(fixed));
        assert_eq!(reopened.entry(&source("b.c")), Some(old));
        assert_eq!(reopened.entry(&source("c.c")), None);
    }

    #[test]
    fn an_entry_inside_the_window_is_not_kept_and_removes_the_older_one() {
        let (dir, lock) = workspace();
        let began = lock.acquired();
        let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
        let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
        let one_second_old = instant(began.as_unix_nanos()) - Duration::from_secs(1);
        let old = hashed(dir.path(), "a.c", b"hello", ten_seconds_old);
        cache.record(source("a.c"), old);
        cache.commit().unwrap();

        // The file is edited a second before the run and hashed again.
        let recent = hashed(dir.path(), "a.c", b"HELLO", one_second_old);
        cache.record(source("a.c"), recent);

        assert_eq!(cache.entry(&source("a.c")), None);
        cache.commit().unwrap();
        let reopened = LocalStatCache::open(lock.layout(), began).unwrap();
        assert_eq!(reopened.entry(&source("a.c")), None);

        // The window is the two seconds before the run began, its first instant included; an
        // entry from after the run began is not kept either.
        let edge = began.as_unix_nanos() - 2_000_000_000;
        for (nanos, kept) in [
            (edge - 1, true),
            (edge, false),
            (began.as_unix_nanos(), false),
            (began.as_unix_nanos() + 5_000_000_000, false),
        ] {
            cache.record(source("edge.c"), entry_at(nanos));
            assert_eq!(cache.entry(&source("edge.c")).is_some(), kept, "{nanos}");
        }
    }

    #[test]
    fn a_commit_with_nothing_changed_writes_nothing() {
        let (dir, lock) = workspace();
        let began = lock.acquired();
        let file = lock.layout().stat_cache();

        // Nothing recorded, and an entry that is not kept for a path that had none.
        let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
        cache.commit().unwrap();
        cache.record(source("new.c"), entry_at(began.as_unix_nanos()));
        cache.commit().unwrap();
        assert!(!file.exists());

        // The entry already held, recorded again.
        let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
        let old = hashed(dir.path(), "a.c", b"hello", ten_seconds_old);
        cache.record(source("a.c"), old);
        cache.commit().unwrap();
        let long_ago = instant(1_600_000_000_000_000_000);
        File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        let before = fs::read(&file).unwrap();

        cache.commit().unwrap();
        let mut reopened = LocalStatCache::open(lock.layout(), began).unwrap();
        reopened.record(source("a.c"), old);
        reopened.commit().unwrap();

        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), long_ago);
    }

    #[test]
    fn a_file_that_is_not_canonical_is_discarded_and_replaced() {
        // Bytes that are not JSON, JSON written another way than the canonical one, and an
        // entry with a key nothing declares.
        for damaged in [
            "not json",
            r#"{ "a.c":{"digest":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","modified":7,"size":5}}"#,
            r#"{"a.c":{"digest":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","inode":1,"modified":7,"size":5}}"#,
        ] {
            let (_dir, lock) = workspace();
            let file = lock.layout().stat_cache();
            fs::write(&file, damaged).unwrap();

            let mut cache = LocalStatCache::open(lock.layout(), lock.acquired()).unwrap();

            assert_eq!(cache.entry(&source("a.c")), None, "{damaged}");
            cache.commit().unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), "{}", "{damaged}");
        }
    }

    #[test]
    fn a_stat_cache_that_cannot_be_read_is_an_adapter_error() {
        let (_dir, lock) = workspace();
        let file = lock.layout().stat_cache();
        fs::create_dir(&file).unwrap();

        let error = LocalStatCache::open(lock.layout(), lock.acquired()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::StatCache);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("reading {}: ", file.display())),
            "{diagnostic}"
        );
    }
}
