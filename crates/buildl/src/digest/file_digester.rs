//! [`FileDigester`], the [`Digester`] that reads the files of a workspace.
//!
//! Its own file because it is the one place a declared input is looked at and read, and the one
//! place in the workspace that hashes on several threads at once. The threads never show: the
//! digests come back in the order the paths were given, and so does the first failure.
//!
//! Responsibilities: [`FileDigester`] and its [`Digester`] implementation.
//!
//! Non-responsibilities: remembering a digest between builds, and deciding which files need
//! one. Both are the caller's; this adapter hashes what it is asked to.

use std::fs::{self, File, FileType};
use std::io::{self, ErrorKind, Read};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use buildl_core::{
    Diagnostic, Digest, Digester, Error, FileStat, Hasher, InputKind, InputStat, PortName, Result,
    SourcePath, Timestamp,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

/// How many bytes of a file are read at a time. No file is held whole.
const BUFFER_BYTES: usize = 64 * 1024;

/// Reads the metadata and the content digests of the files under one workspace root.
///
/// A declared input is a regular file. What stands at a path is looked at without following a
/// symbolic link, so a link is reported as a link whatever it leads to.
///
/// # Examples
///
/// ```
/// use std::fs;
///
/// use buildl::digest::FileDigester;
/// use buildl_core::{Digest, Digester, InputStat, SourcePath};
///
/// let workspace = tempfile::tempdir()?;
/// let root = fs::canonicalize(workspace.path())?;
/// fs::write(root.join("main.c"), "int main(void) { return 0; }\n")?;
///
/// let digester = FileDigester::new(&root);
/// let main = SourcePath::parse("main.c")?;
/// assert!(matches!(digester.stat(&main)?, InputStat::File(stat) if stat.size == 29));
/// assert_eq!(
///     digester.digest(&[main])?,
///     [Digest::of(b"int main(void) { return 0; }\n")]
/// );
/// assert_eq!(
///     digester.stat(&SourcePath::parse("missing.c")?)?,
///     InputStat::Missing
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigester {
    root: PathBuf,
}

impl FileDigester {
    /// A digester for the workspace at `root`.
    ///
    /// `root` must be the workspace root's canonical, absolute path: a file that resolves to a
    /// path outside it is refused.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The digest of the regular file at `path`, read through `buffer`.
    ///
    /// The path is looked at again before it is opened, so nothing but a regular file is ever
    /// opened: a pipe is refused, not read.
    fn hash(&self, path: &SourcePath, buffer: &mut [u8]) -> Result<Digest> {
        let full = self.root.join(path.as_str());
        let metadata = fs::symlink_metadata(&full).map_err(|error| failed(path, &error))?;
        match kind(metadata.file_type()) {
            None => {}
            Some(InputKind::Special) => return Err(failed(path, &"is not a regular file")),
            Some(found) => {
                return Err(failed(
                    path,
                    &format_args!("is {found}; an input must be a regular file"),
                ));
            }
        }
        let resolved = fs::canonicalize(&full).map_err(|error| failed(path, &error))?;
        if !resolved.starts_with(&self.root) {
            return Err(failed(path, &"resolves outside the workspace"));
        }
        let mut file = File::open(&resolved).map_err(|error| failed(path, &error))?;
        let mut hasher = Hasher::new();
        loop {
            match file.read(buffer) {
                Ok(0) => return Ok(hasher.finish()),
                Ok(filled) => hasher.update(buffer.get(..filled).unwrap_or_default()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(failed(path, &error)),
            }
        }
    }
}

impl Digester for FileDigester {
    /// What stands at `path`, from one look that follows no symbolic link.
    ///
    /// Nothing there, and a path that runs through a file, are both
    /// [`InputStat::Missing`]. The modification time is the filesystem's own, to the
    /// nanosecond, never rounded.
    fn stat(&self, path: &SourcePath) -> Result<InputStat> {
        let metadata = match fs::symlink_metadata(self.root.join(path.as_str())) {
            Ok(metadata) => metadata,
            Err(error) if absent(&error) => return Ok(InputStat::Missing),
            Err(error) => return Err(failed(path, &error)),
        };
        if let Some(found) = kind(metadata.file_type()) {
            return Ok(InputStat::Other(found));
        }
        let modified = metadata
            .modified()
            .map_err(|error| failed(path, &error))
            .and_then(|modified| unix_nanos(modified).map_err(|reason| failed(path, &reason)))?;
        Ok(InputStat::File(FileStat {
            modified: Timestamp::from_unix_nanos(modified),
            size: metadata.len(),
        }))
    }

    /// The digest of every file in `paths`, hashed on several threads and returned in the order
    /// given. When more than one path fails, the error is the one for the first of them in that
    /// order, whichever thread met its failure first.
    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
        let hashed: Vec<Result<Digest>> = paths
            .par_iter()
            .map_init(
                || vec![0_u8; BUFFER_BYTES],
                |buffer, path| self.hash(path, buffer),
            )
            .collect();
        hashed.into_iter().collect()
    }
}

/// Whether `error` says that nothing stands at a path: it does not exist, or it runs through
/// something that is not a directory.
fn absent(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// What a file type is when it is not a regular file; `None` for a regular file.
fn kind(file_type: FileType) -> Option<InputKind> {
    if file_type.is_file() {
        None
    } else if file_type.is_symlink() {
        Some(InputKind::Symlink)
    } else if file_type.is_dir() {
        Some(InputKind::Directory)
    } else {
        Some(InputKind::Special)
    }
}

/// A modification time as whole nanoseconds since the Unix epoch, or why it has no such count.
fn unix_nanos(modified: SystemTime) -> core::result::Result<u64, &'static str> {
    let since_epoch = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "its modification time is before 1970")?;
    u64::try_from(since_epoch.as_nanos())
        .map_err(|_| "its modification time is after the year 2554")
}

/// A failure to look at or read `path`, as the digester's error.
fn failed(path: &SourcePath, reason: &dyn core::fmt::Display) -> Error {
    Error::Adapter {
        port: PortName::Digester,
        diagnostic: Diagnostic::new(format!("{path}: {reason}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, UNIX_EPOCH};

    use buildl_core::{Digest, Digester, Error, InputKind, InputStat, PortName, SourcePath};

    use super::{BUFFER_BYTES, FileDigester, unix_nanos};

    /// A temp directory, its canonical path, and a digester over it.
    fn workspace() -> (tempfile::TempDir, PathBuf, FileDigester) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let digester = FileDigester::new(&root);
        (dir, root, digester)
    }

    fn path(text: &str) -> SourcePath {
        SourcePath::parse(text).unwrap()
    }

    /// Makes a named pipe with the system's `mkfifo`.
    fn pipe(at: &Path) {
        let made = Command::new("mkfifo").arg(at).status().unwrap();
        assert!(made.success(), "mkfifo {}", at.display());
    }

    /// A file, a link to it, a dangling link, a directory and a pipe, all under `root`.
    fn one_of_each(root: &Path) {
        fs::write(root.join("file.txt"), "content").unwrap();
        symlink(root.join("file.txt"), root.join("link")).unwrap();
        symlink(root.join("nowhere"), root.join("dangling")).unwrap();
        fs::create_dir(root.join("dir")).unwrap();
        pipe(&root.join("pipe"));
    }

    fn refusal(digester: &FileDigester, text: &str) -> String {
        let error = digester.digest(&[path(text)]).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::Digester,
                    ..
                }
            ),
            "{error:?}"
        );
        error.to_string()
    }

    #[test]
    fn stat_reports_size_and_modification_time() {
        let (_dir, root, digester) = workspace();
        fs::write(root.join("main.c"), "int main;\n").unwrap();
        let modified = fs::metadata(root.join("main.c"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap();

        let stat = match digester.stat(&path("main.c")).unwrap() {
            InputStat::File(stat) => stat,
            other => panic!("expected a regular file, got {other:?}"),
        };
        assert_eq!(stat.size, 10);
        assert_eq!(
            u128::from(stat.modified.as_unix_nanos()),
            modified.as_nanos()
        );
    }

    #[test]
    fn stat_of_nothing_is_missing() {
        let (_dir, root, digester) = workspace();
        fs::write(root.join("file.txt"), "content").unwrap();
        fs::create_dir(root.join("dir")).unwrap();
        for nothing in [
            "missing.c",
            "dir/missing.c",
            "no-dir/missing.c",
            "file.txt/below",
        ] {
            assert_eq!(
                digester.stat(&path(nothing)).unwrap(),
                InputStat::Missing,
                "{nothing}"
            );
        }
    }

    #[test]
    fn stat_reports_a_link_a_directory_and_a_pipe_as_what_they_are() {
        let (_dir, root, digester) = workspace();
        one_of_each(&root);
        let cases = [
            ("link", InputKind::Symlink),
            ("dangling", InputKind::Symlink),
            ("dir", InputKind::Directory),
            ("pipe", InputKind::Special),
        ];
        for (at, kind) in cases {
            assert_eq!(
                digester.stat(&path(at)).unwrap(),
                InputStat::Other(kind),
                "{at}"
            );
        }
        assert!(matches!(
            digester.stat(&path("file.txt")).unwrap(),
            InputStat::File(stat) if stat.size == 7
        ));
    }

    #[test]
    fn stat_of_a_path_that_cannot_be_looked_at_is_an_adapter_error() {
        let (_dir, _root, digester) = workspace();
        let too_long = "n".repeat(300);
        let error = digester.stat(&path(&too_long)).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Digester,
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .starts_with(&format!("digester: {too_long}: ")),
            "{error}"
        );
    }

    #[test]
    fn a_modification_time_is_counted_in_nanoseconds_since_1970() {
        assert_eq!(unix_nanos(UNIX_EPOCH), Ok(0));
        assert_eq!(
            unix_nanos(UNIX_EPOCH + Duration::new(1_791_881_164, 927_344_286)),
            Ok(1_791_881_164_927_344_286)
        );
        assert_eq!(
            unix_nanos(UNIX_EPOCH - Duration::from_nanos(1)),
            Err("its modification time is before 1970")
        );
        assert_eq!(
            unix_nanos(UNIX_EPOCH + Duration::from_secs(20_000_000_000)),
            Err("its modification time is after the year 2554")
        );
    }

    #[test]
    fn digest_refuses_a_path_that_is_not_a_regular_file() {
        let (_dir, root, digester) = workspace();
        one_of_each(&root);
        let cases = [
            (
                "link",
                "digester: link: is a symbolic link; an input must be a regular file",
            ),
            (
                "dangling",
                "digester: dangling: is a symbolic link; an input must be a regular file",
            ),
            (
                "dir",
                "digester: dir: is a directory; an input must be a regular file",
            ),
            ("pipe", "digester: pipe: is not a regular file"),
        ];
        for (at, expected) in cases {
            assert_eq!(refusal(&digester, at), expected, "{at}");
        }
        assert_eq!(
            digester.digest(&[path("file.txt")]).unwrap(),
            [Digest::of(b"content")]
        );
    }

    #[test]
    fn digests_come_back_in_the_order_asked() {
        let (_dir, root, digester) = workspace();
        let contents: Vec<Vec<u8>> = (0_usize..500)
            .map(|index| format!("file {index}\n").repeat(1 + index % 7).into_bytes())
            .collect();
        let paths: Vec<SourcePath> = (0..contents.len())
            .map(|index| path(&format!("f{index:03}.txt")))
            .collect();
        for (at, content) in paths.iter().zip(&contents) {
            fs::write(root.join(at.as_str()), content).unwrap();
        }
        let expected: Vec<Digest> = contents.iter().map(|content| Digest::of(content)).collect();

        for run in 0..20 {
            assert_eq!(digester.digest(&paths).unwrap(), expected, "run {run}");
        }
        assert_eq!(digester.digest(&[]).unwrap(), []);
    }

    #[test]
    fn the_first_failing_path_in_order_is_the_error() {
        let (_dir, root, digester) = workspace();
        fs::create_dir(root.join("f350.txt")).unwrap();
        symlink(root.join("f000.txt"), root.join("f400.txt")).unwrap();
        let paths: Vec<SourcePath> = (0..500)
            .map(|index| path(&format!("f{index:03}.txt")))
            .collect();
        for (index, at) in paths.iter().enumerate() {
            if ![300, 350, 400, 450].contains(&index) {
                fs::write(root.join(at.as_str()), format!("file {index}\n")).unwrap();
            }
        }

        for run in 0..50 {
            assert_eq!(
                digester.digest(&paths).unwrap_err().to_string(),
                "digester: f300.txt: No such file or directory (os error 2)",
                "run {run}"
            );
        }
    }

    #[test]
    fn a_file_larger_than_the_buffer_hashes_like_its_bytes() {
        let (_dir, root, digester) = workspace();
        let sizes = [
            0,
            1,
            BUFFER_BYTES - 1,
            BUFFER_BYTES,
            BUFFER_BYTES + 1,
            3 * BUFFER_BYTES + 17,
        ];
        for size in sizes {
            let content: Vec<u8> = (0..size)
                .map(|index| u8::try_from(index % 251).unwrap())
                .collect();
            fs::write(root.join("blob.bin"), &content).unwrap();
            assert_eq!(
                digester.digest(&[path("blob.bin")]).unwrap(),
                [Digest::of(&content)],
                "{size} bytes"
            );
        }
    }

    #[test]
    fn a_file_that_resolves_outside_the_workspace_is_refused() {
        let (_dir, root, digester) = workspace();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "elsewhere").unwrap();
        symlink(outside.path(), root.join("vendor")).unwrap();

        assert!(matches!(
            digester.stat(&path("vendor/secret.txt")).unwrap(),
            InputStat::File(stat) if stat.size == 9
        ));
        assert_eq!(
            refusal(&digester, "vendor/secret.txt"),
            "digester: vendor/secret.txt: resolves outside the workspace"
        );

        fs::create_dir(root.join("real")).unwrap();
        fs::write(root.join("real/inside.txt"), "inside").unwrap();
        symlink(root.join("real"), root.join("alias")).unwrap();
        assert_eq!(
            digester.digest(&[path("alias/inside.txt")]).unwrap(),
            [Digest::of(b"inside")]
        );
    }
}
