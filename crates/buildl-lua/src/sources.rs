//! The host-side walk behind `buildl.sources(pattern)`, the declaration phase's only filesystem
//! read.
//!
//! Its own file because it is the one place a build file's evaluation touches the filesystem. The
//! Lua engine holds no filesystem grant at all, so containment is enforced here and nowhere else:
//! the walk starts at the declaring directory, refuses a directory that resolves outside the
//! workspace, never follows a symlink, and returns regular files only.
//!
//! Responsibilities: [`Sources`], one declaring directory's walk.
//!
//! Non-responsibilities: turning the paths into inputs. They are returned to Lua as text relative
//! to the declaring directory, and joined with it after evaluation.

use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{Directory, EvaluationFailure};
use globset::GlobBuilder;
use walkdir::WalkDir;

use crate::refusal::Refusal;

/// The walk for one declaring directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sources {
    root: PathBuf,
    directory: PathBuf,
}

impl Sources {
    /// The walk for `directory`, under the canonical workspace `root`.
    pub(crate) fn new(root: &Path, directory: &Directory) -> Self {
        Self {
            root: root.to_path_buf(),
            directory: root.join(directory.as_str()),
        }
    }

    /// Every regular file under the declaring directory whose relative path matches `pattern`,
    /// `/`-separated and sorted by bytes.
    ///
    /// The matcher uses the same flags as airsl's `glob` module: `*` stops at `/`, and `**/`
    /// matches zero or more directories.
    pub(crate) fn find(&self, pattern: &str) -> Result<Vec<String>, Refusal> {
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
            .map_err(|error| Refusal::new(EvaluationFailure::Runtime, error.to_string()))?
            .compile_matcher();
        let base = fs::canonicalize(&self.directory).map_err(|error| {
            Refusal::new(
                EvaluationFailure::Runtime,
                format!("{}: {error}", self.directory.display()),
            )
        })?;
        if !base.starts_with(&self.root) {
            return Err(Refusal::new(
                EvaluationFailure::Refused,
                format!(
                    "{} resolves outside the workspace",
                    self.directory.display()
                ),
            ));
        }

        let mut found = Vec::new();
        for entry in WalkDir::new(&base).follow_root_links(false) {
            let entry = entry
                .map_err(|error| Refusal::new(EvaluationFailure::Runtime, error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(&base) else {
                continue;
            };
            if !matcher.is_match(relative) {
                continue;
            }
            let text = relative.to_str().ok_or_else(|| {
                Refusal::new(
                    EvaluationFailure::Runtime,
                    format!("non-UTF-8 file name {}", relative.display()),
                )
            })?;
            found.push(text.to_owned());
        }
        found.sort();
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use buildl_core::{Directory, EvaluationFailure};

    use super::Sources;

    /// A canonical temporary workspace holding `files`, each created empty.
    fn workspace(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        (dir, root)
    }

    fn sources(root: &Path, directory: &str) -> Sources {
        let directory = if directory.is_empty() {
            Directory::root()
        } else {
            Directory::parse(directory).unwrap()
        };
        Sources::new(root, &directory)
    }

    #[test]
    fn returns_matching_regular_files_sorted_by_bytes() {
        let (_dir, root) = workspace(&["b.c", "a/z.c", "a.c", "sub/c.c", "a.h", ".hidden/h.c"]);
        assert_eq!(
            sources(&root, "").find("**/*.c").unwrap(),
            [".hidden/h.c", "a.c", "a/z.c", "b.c", "sub/c.c"]
        );
    }

    #[test]
    fn a_star_stops_at_a_directory_boundary() {
        let (_dir, root) = workspace(&["a.c", "sub/b.c"]);
        assert_eq!(sources(&root, "").find("*.c").unwrap(), ["a.c"]);
    }

    #[test]
    fn paths_are_relative_to_the_declaring_directory() {
        let (_dir, root) = workspace(&["lib/src/text.c", "app/main.c"]);
        assert_eq!(
            sources(&root, "lib").find("src/*.c").unwrap(),
            ["src/text.c"]
        );
    }

    #[test]
    fn a_pattern_cannot_reach_outside_the_declaring_directory() {
        let (_dir, root) = workspace(&["lib/a.c", "app/b.c"]);
        assert!(sources(&root, "lib").find("../**/*.c").unwrap().is_empty());
    }

    #[test]
    fn symlinks_below_the_directory_are_neither_returned_nor_entered() {
        let (_dir, root) = workspace(&["a.c", "outside/secret.c"]);
        symlink(root.join("outside"), root.join("link")).unwrap();
        symlink(root.join("a.c"), root.join("alias.c")).unwrap();
        let (_other, elsewhere) = workspace(&["far.c"]);
        symlink(&elsewhere, root.join("far")).unwrap();
        assert_eq!(
            sources(&root, "").find("**/*.c").unwrap(),
            ["a.c", "outside/secret.c"]
        );
    }

    #[test]
    fn a_declaring_directory_that_resolves_outside_the_workspace_is_refused() {
        let (_dir, root) = workspace(&[]);
        let (_other, elsewhere) = workspace(&["far.c"]);
        symlink(&elsewhere, root.join("lib")).unwrap();
        let refusal = sources(&root, "lib").find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Refused);
    }

    #[test]
    fn an_invalid_pattern_is_refused_with_globset_s_message() {
        let (_dir, root) = workspace(&[]);
        let refusal = sources(&root, "").find("a[").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
        assert_eq!(
            refusal.diagnostic("sources", None).as_str(),
            "buildl.sources: error parsing glob 'a[': unclosed character class; missing ']'"
        );
    }

    #[test]
    fn a_missing_declaring_directory_is_a_runtime_refusal() {
        let (_dir, root) = workspace(&[]);
        let refusal = sources(&root, "gone").find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_matching_non_utf8_file_name_is_refused() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let (_dir, root) = workspace(&[]);
        fs::write(root.join(OsStr::from_bytes(b"bad\xff.c")), "").unwrap();
        let refusal = sources(&root, "").find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
    }
}
