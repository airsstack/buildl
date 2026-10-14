//! The rule that leads from a program's name to the file it runs.
//!
//! Its own module because two adapters need the rule and neither may name the other: the one
//! that fingerprints a tool and the one that runs it. Both are handed one [`SearchPath`] value,
//! so the file that is run is the file that was fingerprinted. This module implements no port,
//! and names no other module of this crate.
//!
//! Responsibilities: [`SearchPath`].
//!
//! Non-responsibilities: reading or running the file a name leads to, and deciding which
//! programs a workspace allows.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use buildl_core::Program;

/// The directories a bare program name is looked for in, in order.
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
///
/// use buildl::search_path::SearchPath;
/// use buildl_core::Program;
///
/// let search = SearchPath::new([PathBuf::from("/no/such/directory")]);
/// assert_eq!(search.locate(&Program::parse("cc")?, Path::new("/work")), None);
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchPath(Vec<PathBuf>);

impl SearchPath {
    /// The `PATH` of this process, read once.
    ///
    /// A `PATH` that is unset or empty gives a search path that finds no bare name.
    #[must_use]
    pub fn from_process() -> Self {
        env::var_os("PATH").map_or_else(Self::default, |path| Self::new(env::split_paths(&path)))
    }

    /// These directories, in order. One that is not absolute is left out.
    #[must_use]
    pub fn new(directories: impl IntoIterator<Item = PathBuf>) -> Self {
        Self(
            directories
                .into_iter()
                .filter(|directory| directory.is_absolute())
                .collect(),
        )
    }

    /// The file `program` names, for a command run in `base`.
    ///
    /// A name that holds a `/` is a path and is not searched for: an absolute one is itself, and
    /// a relative one is joined to `base`. A bare name is looked for in each directory in order.
    /// What is found is a regular file with an execute bit, links followed; a directory of that
    /// name, a file with no execute bit and a link to nothing are passed over.
    ///
    /// The path returned is the one that was tried, not its canonical form, so a program that
    /// behaves by the name it was called with still sees that name.
    #[must_use]
    pub fn locate(&self, program: &Program, base: &Path) -> Option<PathBuf> {
        let name = program.as_str();
        if name.contains('/') {
            // `join` returns an absolute name unchanged.
            return Some(base.join(name)).filter(|path| runnable(path));
        }
        self.0
            .iter()
            .map(|directory| directory.join(name))
            .find(|path| runnable(path))
    }
}

/// Whether `path` leads, through any links, to a regular file someone may execute.
///
/// The test reads the mode bits. A file this process may not run is still found, and fails when
/// it is started.
fn runnable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs::{self, Permissions};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use buildl_core::Program;

    use crate::search_path::SearchPath;

    /// A file at `path` with permission bits `mode`.
    fn file(path: &Path, mode: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
    }

    fn program(name: &str) -> Program {
        Program::parse(name).unwrap()
    }

    /// A canonical temporary directory.
    fn root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn the_first_executable_file_in_order_is_found() {
        let (_dir, root) = root();
        let (first, second) = (root.join("first"), root.join("second"));
        file(&second.join("only-second"), 0o755);
        file(&first.join("both"), 0o755);
        file(&second.join("both"), 0o755);
        let search = SearchPath::new([root.join("no-such-dir"), first.clone(), second.clone()]);

        assert_eq!(
            search.locate(&program("both"), &root),
            Some(first.join("both"))
        );
        assert_eq!(
            search.locate(&program("only-second"), &root),
            Some(second.join("only-second"))
        );
        assert_eq!(search.locate(&program("neither"), &root), None);
        assert_eq!(SearchPath::default().locate(&program("both"), &root), None);
    }

    #[test]
    fn a_directory_a_file_without_the_bit_and_a_dangling_link_are_passed_over() {
        let (_dir, root) = root();
        let (first, second) = (root.join("first"), root.join("second"));
        fs::create_dir_all(first.join("adir")).unwrap();
        file(&first.join("noexec"), 0o644);
        symlink(first.join("nowhere"), first.join("dangling")).unwrap();
        file(&second.join("real-tool"), 0o755);
        symlink(second.join("real-tool"), first.join("linked")).unwrap();
        for name in ["adir", "noexec", "dangling"] {
            file(&second.join(name), 0o755);
        }
        let search = SearchPath::new([first.clone(), second.clone()]);

        for name in ["adir", "noexec", "dangling"] {
            assert_eq!(
                search.locate(&program(name), &root),
                Some(second.join(name)),
                "{name}"
            );
        }
        // A link to an executable file is taken, and returned as the link.
        assert_eq!(
            search.locate(&program("linked"), &root),
            Some(first.join("linked"))
        );
        // With nothing behind them, the three are not found at all.
        let alone = SearchPath::new([first]);
        for name in ["adir", "noexec", "dangling"] {
            assert_eq!(alone.locate(&program(name), &root), None, "{name}");
        }
    }

    #[test]
    fn a_name_with_a_slash_is_not_searched() {
        let (_dir, root) = root();
        let bin = root.join("bin");
        file(&bin.join("gen.sh"), 0o755);
        file(&root.join("scripts").join("gen.sh"), 0o755);
        file(&root.join("scripts").join("plain.txt"), 0o644);
        let search = SearchPath::new([bin.clone()]);

        // Relative: joined to the base, whatever the search path holds.
        assert_eq!(
            search.locate(&program("scripts/gen.sh"), &root),
            Some(root.join("scripts").join("gen.sh"))
        );
        assert_eq!(search.locate(&program("./gen.sh"), &root), None);
        assert_eq!(search.locate(&program("scripts/plain.txt"), &root), None);
        assert_eq!(search.locate(&program("scripts/absent"), &root), None);

        // Absolute: itself, whatever the base is.
        let absolute = bin.join("gen.sh");
        assert_eq!(
            search.locate(&program(absolute.to_str().unwrap()), Path::new("/")),
            Some(absolute)
        );
        let missing = root.join("scripts").join("absent");
        assert_eq!(
            search.locate(&program(missing.to_str().unwrap()), &root),
            None
        );
    }

    #[test]
    fn a_relative_directory_is_left_out() {
        let (_dir, root) = root();
        file(&root.join("bin").join("tool"), 0o755);

        let search = SearchPath::new([
            PathBuf::new(),
            PathBuf::from("bin"),
            PathBuf::from("./bin"),
            root.join("bin"),
        ]);

        assert_eq!(search, SearchPath::new([root.join("bin")]));
        assert_eq!(
            SearchPath::new([PathBuf::new(), PathBuf::from("bin")]),
            SearchPath::default()
        );
        assert_eq!(
            search.locate(&program("tool"), &root),
            Some(root.join("bin").join("tool"))
        );
    }

    #[test]
    fn the_search_path_of_this_process_finds_a_system_tool() {
        let found = SearchPath::from_process()
            .locate(&program("sh"), Path::new("/"))
            .unwrap();

        assert!(found.is_absolute());
        assert_eq!(found.file_name().unwrap(), "sh");
    }
}
