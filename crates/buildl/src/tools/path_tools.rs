//! [`PathTools`], the [`ToolResolver`] that finds a program and hashes the file it names.
//!
//! Its own file because what identifies a tool is a choice, and it is made here: the bytes of
//! the file its name leads to, links followed. No path, size or time is part of it, so a
//! fingerprint holds no clock value, and nothing is run to take it.
//!
//! Responsibilities: [`PathTools`] and its [`ToolResolver`] implementation.
//!
//! Non-responsibilities: the lookup itself, which is the search path's, shared with whatever
//! runs the program so that the file run is the file fingerprinted.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use buildl_core::{Diagnostic, Digest, Error, Hasher, PortName, Program, Result, ToolResolver};

use crate::search_path::SearchPath;

/// How many bytes of a program are read at a time. No program is held whole.
const BUFFER_BYTES: usize = 64 * 1024;

/// Finds the programs commands name, and fingerprints each by its content.
///
/// A fingerprint changes exactly when the bytes of the file the name leads to change. A
/// launcher that picks the real tool when it runs is fingerprinted as the launcher: a change
/// behind it, like a change to a library a tool loads, is not seen.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use std::os::unix::fs::PermissionsExt;
///
/// use buildl::search_path::SearchPath;
/// use buildl::tools::PathTools;
/// use buildl_core::{Digest, Program, ToolResolver};
///
/// let workspace = tempfile::tempdir()?;
/// let bin = fs::canonicalize(workspace.path())?.join("bin");
/// fs::create_dir(&bin)?;
/// fs::write(bin.join("gen"), "#!/bin/sh\necho generated\n")?;
/// fs::set_permissions(bin.join("gen"), fs::Permissions::from_mode(0o755))?;
///
/// let tools = PathTools::new(workspace.path(), SearchPath::new([bin]));
/// assert_eq!(
///     tools.fingerprint(&Program::parse("gen")?)?,
///     Some(Digest::of(b"#!/bin/sh\necho generated\n"))
/// );
/// assert_eq!(tools.fingerprint(&Program::parse("no-such-tool")?)?, None);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTools {
    root: PathBuf,
    search: SearchPath,
}

impl PathTools {
    /// A resolver for the workspace at `root`, looking bare names up in `search`.
    ///
    /// `root` is the directory a program named by a relative path is looked for in.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, search: SearchPath) -> Self {
        Self {
            root: root.into(),
            search,
        }
    }
}

impl ToolResolver for PathTools {
    /// The SHA-256 of the file `program` leads to; `Ok(None)` when the lookup finds no file.
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
        self.search
            .locate(program, &self.root)
            .map(|located| content(&located).map_err(|error| failed(&located, &error)))
            .transpose()
    }
}

/// The digest of the file at `located`, links followed, read in pieces.
fn content(located: &Path) -> std::io::Result<Digest> {
    let mut file = File::open(located)?;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut hasher = Hasher::new();
    loop {
        match file.read(&mut buffer) {
            Ok(0) => return Ok(hasher.finish()),
            Ok(filled) => hasher.update(buffer.get(..filled).unwrap_or_default()),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// A located program that could not be read, as the tool resolver's error.
fn failed(located: &Path, error: &std::io::Error) -> Error {
    Error::Adapter {
        port: PortName::ToolResolver,
        diagnostic: Diagnostic::new(format!("{}: {error}", located.display())),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, File};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use buildl_core::{Digest, Error, PortName, Program, ToolResolver};

    use super::{BUFFER_BYTES, PathTools};
    use crate::search_path::SearchPath;

    /// A temp directory and its canonical path.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        (dir, root)
    }

    /// Writes `bytes` at `at` with `mode`, making the directory above it.
    fn file(at: &Path, bytes: &[u8], mode: u32) {
        fs::create_dir_all(at.parent().unwrap()).unwrap();
        fs::write(at, bytes).unwrap();
        fs::set_permissions(at, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn program(name: &str) -> Program {
        Program::parse(name).unwrap()
    }

    #[test]
    fn the_fingerprint_is_the_digest_of_the_file() {
        let (_dir, root) = workspace();
        let large: Vec<u8> = (0..3 * BUFFER_BYTES + 17)
            .map(|index| u8::try_from(index % 251).unwrap())
            .collect();
        file(&root.join("bin/small"), b"#!/bin/sh\n", 0o755);
        file(&root.join("bin/large"), &large, 0o755);
        file(
            &root.join("scripts/gen.sh"),
            b"#!/bin/sh\necho gen\n",
            0o755,
        );
        let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));

        assert_eq!(
            tools.fingerprint(&program("small")).unwrap(),
            Some(Digest::of(b"#!/bin/sh\n"))
        );
        assert_eq!(
            tools.fingerprint(&program("large")).unwrap(),
            Some(Digest::of(&large))
        );
        assert_eq!(
            tools.fingerprint(&program("scripts/gen.sh")).unwrap(),
            Some(Digest::of(b"#!/bin/sh\necho gen\n"))
        );
        let absolute = root.join("bin/small");
        assert_eq!(
            tools
                .fingerprint(&program(absolute.to_str().unwrap()))
                .unwrap(),
            Some(Digest::of(b"#!/bin/sh\n"))
        );
    }

    #[test]
    fn the_fingerprint_holds_no_path_and_no_time() {
        let (_dir, root) = workspace();
        file(&root.join("first/tool"), b"same bytes", 0o755);
        file(&root.join("second/other-name"), b"same bytes", 0o700);
        let first = PathTools::new(&root, SearchPath::new([root.join("first")]));
        let second = PathTools::new(&root, SearchPath::new([root.join("second")]));
        assert_eq!(
            first.fingerprint(&program("tool")).unwrap(),
            second.fingerprint(&program("other-name")).unwrap()
        );
    }

    #[test]
    fn a_linked_tool_has_its_targets_fingerprint() {
        let (_dir, root) = workspace();
        file(&root.join("real/real-tool"), b"version 1", 0o755);
        fs::create_dir(root.join("bin")).unwrap();
        symlink(root.join("real/real-tool"), root.join("bin/linked")).unwrap();
        let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));

        assert_eq!(
            tools.fingerprint(&program("linked")).unwrap(),
            Some(Digest::of(b"version 1"))
        );

        file(&root.join("real/real-tool"), b"version 2", 0o755);
        assert_eq!(
            tools.fingerprint(&program("linked")).unwrap(),
            Some(Digest::of(b"version 2"))
        );
    }

    #[test]
    fn a_program_that_is_not_found_is_none() {
        let (_dir, root) = workspace();
        file(&root.join("bin/not-executable"), b"data", 0o644);
        fs::create_dir(root.join("bin/a-directory")).unwrap();
        let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));
        for name in [
            "no-such-tool",
            "not-executable",
            "a-directory",
            "scripts/missing.sh",
            "/no/such/directory/tool",
        ] {
            assert_eq!(tools.fingerprint(&program(name)).unwrap(), None, "{name}");
        }

        let nowhere = PathTools::new(&root, SearchPath::new([]));
        assert_eq!(nowhere.fingerprint(&program("sh")).unwrap(), None);
    }

    #[test]
    fn a_tool_that_cannot_be_read_is_an_adapter_error() {
        let (_dir, root) = workspace();
        let tool = root.join("bin/sealed");
        file(&tool, b"unreadable", 0o111);
        if File::open(&tool).is_ok() {
            // Whoever runs this test may read any file, so no file here can be unreadable.
            return;
        }
        let tools = PathTools::new(&root, SearchPath::new([root.join("bin")]));
        let error = tools.fingerprint(&program("sealed")).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::ToolResolver,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            format!(
                "tool resolver: {}: Permission denied (os error 13)",
                tool.display()
            )
        );
    }
}
