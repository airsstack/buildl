//! What stands at one output path of an action that ran.
//!
//! Its own file because looking is not judging: an output path is reported as it is found — a
//! file, a directory with everything below it, a link, something else, or nothing — and what a
//! report means for the build is decided by the pipeline. No symbolic link is followed, at the
//! path or below it, so nothing outside the output is ever read.
//!
//! Responsibilities: `found` and `Unread`.
//!
//! Non-responsibilities: refusing anything. A link, a pipe and an empty directory are reported,
//! not rejected.

use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use buildl_core::{ContentStore, Error, FileMode, Found, FoundEntry, FoundKind, StoredFile};

/// Why what stands at an output path could not be reported.
#[derive(Debug)]
pub(crate) enum Unread {
    /// The path, or something below it, could not be looked at, listed or read.
    Io(io::Error),
    /// The store refused a file's bytes. The error is the store's own.
    Store(Error),
}

impl From<io::Error> for Unread {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// What is at `path`, following no symbolic link; every regular file is stored through `cas`.
pub(crate) fn found<C: ContentStore>(path: &Path, cas: &C) -> Result<Found, Unread> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Found::Nothing),
        Err(error) => return Err(Unread::Io(error)),
    };
    let kind = metadata.file_type();
    if kind.is_symlink() {
        return Ok(Found::Symlink);
    }
    if kind.is_file() {
        return Ok(Found::File(stored(path, &metadata, cas)?));
    }
    if kind.is_dir() {
        return Ok(Found::Tree(below(path, cas)?));
    }
    Ok(Found::Special)
}

/// One directory still to list.
struct Level {
    /// Where it is.
    directory: PathBuf,
    /// What the path of everything it holds starts with: nothing for the output itself, and
    /// otherwise the directory's own path below the output, then a `/`.
    prefix: String,
    /// Whether its own name, and the name of every directory between it and the output, is
    /// valid Unicode.
    text: bool,
}

/// Everything below the directory `top`, in the order the filesystem lists it.
///
/// A directory that holds something is not listed: the paths of what it holds say it is there.
/// A name that is not valid Unicode is reported as [`FoundKind::NotText`], with what is not
/// text replaced in its path; a directory with such a name is still listed, and everything
/// below it is reported the same way and not stored. The directories still to list are kept on
/// a stack of this function's own, so a deep tree cannot exhaust the thread's.
fn below<C: ContentStore>(top: &Path, cas: &C) -> Result<Vec<FoundEntry>, Unread> {
    let mut entries = Vec::new();
    let mut pending = vec![Level {
        directory: top.to_path_buf(),
        prefix: String::new(),
        text: true,
    }];
    while let Some(level) = pending.pop() {
        let mut holds_something = false;
        for entry in fs::read_dir(&level.directory)? {
            let entry = entry?;
            holds_something = true;
            let name = entry.file_name();
            let (path, text) = match name.to_str() {
                Some(name) if level.text => (format!("{}{name}", level.prefix), true),
                _ => (format!("{}{}", level.prefix, name.to_string_lossy()), false),
            };
            // The entry's own metadata: a link is not followed.
            let metadata = entry.metadata()?;
            let kind = metadata.file_type();
            if kind.is_dir() {
                pending.push(Level {
                    directory: entry.path(),
                    prefix: format!("{path}/"),
                    text,
                });
                continue;
            }
            let kind = if !text {
                FoundKind::NotText
            } else if kind.is_symlink() {
                FoundKind::Symlink
            } else if kind.is_file() {
                FoundKind::File(stored(&entry.path(), &metadata, cas)?)
            } else {
                FoundKind::Special
            };
            entries.push(FoundEntry { path, kind });
        }
        if !holds_something && !level.prefix.is_empty() {
            entries.push(FoundEntry {
                path: level.prefix.trim_end_matches('/').to_owned(),
                kind: if level.text {
                    FoundKind::EmptyDirectory
                } else {
                    FoundKind::NotText
                },
            });
        }
    }
    Ok(entries)
}

/// Stores the regular file at `path`, and says whether it may be run: it may when any of its
/// three execute bits is set.
fn stored<C: ContentStore>(
    path: &Path,
    metadata: &Metadata,
    cas: &C,
) -> Result<StoredFile, Unread> {
    let digest = cas.put(&fs::read(path)?).map_err(Unread::Store)?;
    let mode = if metadata.permissions().mode() & 0o111 == 0 {
        FileMode::Plain
    } else {
        FileMode::Executable
    };
    Ok(StoredFile { digest, mode })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, Permissions};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use std::process::Command;

    use buildl_core::{ContentStore, Digest, FileMode, Found, FoundEntry, FoundKind, StoredFile};

    use super::found;
    use crate::exec::memory_store::MemoryStore;

    /// Makes a named pipe at `path` with the system's `mkfifo`: something that is neither a
    /// file, a directory nor a link.
    fn pipe(path: &Path) {
        let made = Command::new("mkfifo").arg(path).status().unwrap();
        assert!(made.success());
    }

    fn entry(path: &str, kind: FoundKind) -> FoundEntry {
        FoundEntry {
            path: path.to_owned(),
            kind,
        }
    }

    fn file(bytes: &[u8], mode: FileMode) -> FoundKind {
        FoundKind::File(StoredFile {
            digest: Digest::of(bytes),
            mode,
        })
    }

    #[test]
    fn collect_reports_a_file_with_its_mode_and_stores_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        let plain = dir.path().join("plain");
        let tool = dir.path().join("tool");
        fs::write(&plain, "text").unwrap();
        fs::write(&tool, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&plain, Permissions::from_mode(0o644)).unwrap();
        // One execute bit is enough, and it need not be the owner's.
        fs::set_permissions(&tool, Permissions::from_mode(0o610)).unwrap();

        assert_eq!(
            found(&plain, &store).unwrap(),
            Found::File(StoredFile {
                digest: Digest::of(b"text"),
                mode: FileMode::Plain,
            })
        );
        assert_eq!(
            found(&tool, &store).unwrap(),
            Found::File(StoredFile {
                digest: Digest::of(b"#!/bin/sh\n"),
                mode: FileMode::Executable,
            })
        );
        assert_eq!(
            store.get(&Digest::of(b"text")).unwrap(),
            Some(b"text".to_vec())
        );
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn collect_reports_nothing_a_link_and_a_special_file_at_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        fs::write(dir.path().join("file"), "text").unwrap();
        symlink("file", dir.path().join("link")).unwrap();
        symlink("nowhere", dir.path().join("dangling")).unwrap();
        pipe(&dir.path().join("pipe"));
        fs::create_dir(dir.path().join("hollow")).unwrap();

        let at = |name: &str| found(&dir.path().join(name), &store).unwrap();

        assert_eq!(at("absent"), Found::Nothing);
        assert_eq!(at("link"), Found::Symlink);
        assert_eq!(at("dangling"), Found::Symlink);
        assert_eq!(at("pipe"), Found::Special);
        assert_eq!(at("hollow"), Found::Tree(Vec::new()));
        // Nothing was read through the link, and a pipe was never opened.
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn collect_reports_everything_below_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        let top = dir.path().join("tree");
        fs::create_dir_all(top.join("sub/deeper")).unwrap();
        fs::create_dir_all(top.join("empty")).unwrap();
        fs::write(top.join("a"), "a\n").unwrap();
        fs::write(top.join("sub/b"), "b\n").unwrap();
        fs::set_permissions(top.join("sub/b"), Permissions::from_mode(0o755)).unwrap();
        fs::write(top.join("sub/deeper/c"), "c\n").unwrap();
        symlink("a", top.join("link")).unwrap();
        symlink("sub", top.join("linked-directory")).unwrap();
        pipe(&top.join("pipe"));

        let Found::Tree(mut entries) = found(&top, &store).unwrap() else {
            panic!("a directory is reported as a tree");
        };
        entries.sort();

        // A directory that holds something is not listed, and a link to one is not entered.
        assert_eq!(
            entries,
            [
                entry("a", file(b"a\n", FileMode::Plain)),
                entry("empty", FoundKind::EmptyDirectory),
                entry("link", FoundKind::Symlink),
                entry("linked-directory", FoundKind::Symlink),
                entry("pipe", FoundKind::Special),
                entry("sub/b", file(b"b\n", FileMode::Executable)),
                entry("sub/deeper/c", file(b"c\n", FileMode::Plain)),
            ]
        );
        assert_eq!(store.len(), 3);
        assert_eq!(
            store.get(&Digest::of(b"c\n")).unwrap(),
            Some(b"c\n".to_vec())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn collect_reports_a_name_that_is_not_unicode() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        let top = dir.path().join("tree");
        let raw_file = OsString::from_vec(vec![b'f', 0xff]);
        let raw_directory = OsString::from_vec(vec![b'd', 0xff]);
        let raw_hollow = OsString::from_vec(vec![b'h', 0xff]);
        fs::create_dir_all(top.join(&raw_directory).join("deeper")).unwrap();
        fs::create_dir_all(top.join(&raw_hollow)).unwrap();
        fs::write(top.join(&raw_file), "x").unwrap();
        fs::write(top.join(&raw_directory).join("inside"), "y").unwrap();
        fs::write(top.join(&raw_directory).join("deeper/below"), "z").unwrap();
        fs::write(top.join("plain"), "p").unwrap();

        let Found::Tree(mut entries) = found(&top, &store).unwrap() else {
            panic!("a directory is reported as a tree");
        };
        entries.sort();

        // What is not text is shown with U+FFFD in its place. Below a directory whose name is
        // not text nothing is text, whatever its own name is, and nothing there is stored.
        assert_eq!(
            entries,
            [
                entry("d\u{fffd}/deeper/below", FoundKind::NotText),
                entry("d\u{fffd}/inside", FoundKind::NotText),
                entry("f\u{fffd}", FoundKind::NotText),
                entry("h\u{fffd}", FoundKind::NotText),
                entry("plain", file(b"p", FileMode::Plain)),
            ]
        );
        assert_eq!(store.len(), 1);
    }
}
