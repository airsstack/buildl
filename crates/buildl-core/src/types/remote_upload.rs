//! What a run produced, in the terms a remote cache stores a result.
//!
//! Its own file because an upload names each output by its path and the size of every file in it,
//! which the local manifests do not hold: the local content store knows a file by its digest
//! alone. The sizes are read once, where the bytes are, and carried here.
//!
//! Responsibilities: [`RemoteContent`], [`RemoteOutput`], [`RemoteUpload`] and [`Uploaded`].
//!
//! Non-responsibilities: reading the bytes, and sending them, which the content store and the
//! remote adapter do.

use crate::types::{Blob, FileMode, OutputPath, RemoteTreeFile, RootPath};

/// What one output is, with the size of each file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteContent {
    /// One regular file.
    File {
        /// The file's content.
        blob: Blob,
        /// Whether the file may be run.
        mode: FileMode,
    },
    /// A directory of regular files, in path order.
    Tree(Vec<RemoteTreeFile>),
}

/// One declared output of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOutput {
    /// Where the output lives in the workspace.
    pub path: OutputPath,
    /// What it is.
    pub content: RemoteContent,
}

/// What a run produced, as the result stored for its action will name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUpload {
    /// Each declared output.
    pub outputs: Vec<RemoteOutput>,
    /// What the command printed on its standard output.
    pub stdout: Blob,
    /// What the command printed on its standard error.
    pub stderr: Blob,
}

/// How an upload ended, when no call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uploaded {
    /// The result and everything it names are stored.
    Done,
    /// An input file no longer holds the bytes its digest was taken of, so nothing was stored.
    InputChanged(RootPath),
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{RemoteContent, RemoteOutput, RemoteUpload, Uploaded};
    use crate::types::{
        Blob, Directory, FileMode, Label, OutputName, OutputPath, RemoteTreeFile, RootPath,
        SourcePath, TreePath,
    };

    fn output(content: RemoteContent) -> RemoteOutput {
        RemoteOutput {
            path: OutputPath::of(
                &Directory::parse("out").unwrap(),
                &Label::parse("//:gen").unwrap(),
                &OutputName::parse("gen").unwrap(),
            ),
            content,
        }
    }

    #[test]
    fn a_tree_output_lists_each_file_with_its_size() {
        let upload = RemoteUpload {
            outputs: vec![output(RemoteContent::Tree(vec![RemoteTreeFile {
                path: TreePath::parse("a.txt").unwrap(),
                blob: Blob::of(b"abc"),
                mode: FileMode::Plain,
            }]))],
            stdout: Blob::of(b""),
            stderr: Blob::of(b""),
        };
        let RemoteContent::Tree(files) = &upload.outputs[0].content else {
            unreachable!("the output was built as a tree");
        };
        assert_eq!(files[0].blob.size, 3);
    }

    #[test]
    fn an_upload_that_found_a_changed_input_names_it() {
        let path = RootPath::from(&SourcePath::parse("src/main.c").unwrap());
        assert_ne!(Uploaded::Done, Uploaded::InputChanged(path.clone()));
        assert_eq!(
            Uploaded::InputChanged(path.clone()),
            Uploaded::InputChanged(path)
        );
    }
}
