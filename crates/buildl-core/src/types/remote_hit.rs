//! What a remote cache answered when asked for an action's result, exactly as it answered.
//!
//! Its own file because the answer comes from another machine and is trusted for nothing: paths
//! are the server's text, not validated names, and no digest has been checked. Keeping the raw
//! answer apart from the validated values the pipeline works with means a check cannot be
//! skipped by passing the wrong one.
//!
//! Responsibilities: [`RemoteOutputFile`], [`RemoteOutputDirectory`], [`RemoteHit`] and
//! [`RemoteTreeFile`].
//!
//! Non-responsibilities: deciding whether an answer is usable, and fetching what it names, which
//! the phase that executes and the remote adapter do.

use crate::types::{Blob, FileMode, TreePath};

/// One output file a result names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOutputFile {
    /// The path as the server wrote it.
    pub path: String,
    /// The file's content.
    pub blob: Blob,
    /// Whether the file may be run.
    pub mode: FileMode,
}

/// One output directory a result names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOutputDirectory {
    /// The path as the server wrote it.
    pub path: String,
    /// The blob that lists the directory's files.
    pub tree: Blob,
}

/// A result as the server answered, unchecked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHit {
    /// The command's exit code.
    pub exit_code: i32,
    /// The output files.
    pub files: Vec<RemoteOutputFile>,
    /// The output directories.
    pub directories: Vec<RemoteOutputDirectory>,
    /// The paths of the outputs the server reported as symbolic links, which a build cannot
    /// place; empty when the result names none.
    pub symlinks: Vec<String>,
    /// What the command printed on its standard output, when the server kept it.
    pub stdout: Option<Blob>,
    /// What the command printed on its standard error, when the server kept it.
    pub stderr: Option<Blob>,
}

/// One regular file of an output directory, as the blob that lists the directory names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTreeFile {
    /// Where the file is, below the directory.
    pub path: TreePath,
    /// The file's content.
    pub blob: Blob,
    /// Whether the file may be run.
    pub mode: FileMode,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{RemoteHit, RemoteOutputDirectory, RemoteOutputFile, RemoteTreeFile};
    use crate::types::{Blob, FileMode, TreePath};

    #[test]
    fn a_hit_keeps_the_servers_path_text_as_it_was_written() {
        let hit = RemoteHit {
            exit_code: 0,
            files: vec![RemoteOutputFile {
                path: "../out/./x".to_owned(),
                blob: Blob::of(b"x"),
                mode: FileMode::Plain,
            }],
            directories: vec![RemoteOutputDirectory {
                path: "out//dir/".to_owned(),
                tree: Blob::of(b"tree"),
            }],
            symlinks: Vec::new(),
            stdout: None,
            stderr: Some(Blob::of(b"warning")),
        };
        assert_eq!(hit.files[0].path, "../out/./x");
        assert_eq!(hit.directories[0].path, "out//dir/");
        assert_eq!(hit.clone(), hit);
    }

    #[test]
    fn a_tree_file_names_its_path_below_the_directory() {
        let file = RemoteTreeFile {
            path: TreePath::parse("a/b.txt").unwrap(),
            blob: Blob::of(b"b"),
            mode: FileMode::Executable,
        };
        assert_eq!(file.path.as_str(), "a/b.txt");
    }
}
