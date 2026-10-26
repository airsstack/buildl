//! A workspace-relative file of the input root a remote action is given.
//!
//! Its own file because the input root holds two kinds of file, declared inputs and the files of
//! a dependency's outputs, and both are named by a path from the workspace root. One type for
//! both keeps a path that is neither from being listed, and gives the sort a server requires of
//! them a single definition.
//!
//! Responsibilities: [`RootPath`], its two sources, and its rendering.
//!
//! Non-responsibilities: whether the file exists, and building the directory tree the paths
//! make, which are the remote adapter's.

use core::fmt;

use crate::types::{OutputPath, SourcePath};

/// The workspace-relative path of one file of the input root, such as `src/main.c` or
/// `out/gen/a/b.txt`.
///
/// Made only from a [`SourcePath`] or an [`OutputPath`], so every path held is one the layout
/// rules produced or lies below one. A tree output's file is made from the path
/// [`OutputPath::entry`] gives. Paths compare by their UTF-8 bytes, the order a server requires
/// of the entries of a directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootPath(String);

impl RootPath {
    /// The path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&SourcePath> for RootPath {
    fn from(path: &SourcePath) -> Self {
        Self(path.as_str().to_owned())
    }
}

impl From<&OutputPath> for RootPath {
    fn from(path: &OutputPath) -> Self {
        Self(path.as_str().to_owned())
    }
}

impl fmt::Display for RootPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::RootPath;
    use crate::types::{Directory, Label, OutputName, OutputPath, SourcePath, TreePath};

    fn output(label: &str, name: &str) -> OutputPath {
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &Label::parse(label).unwrap(),
            &OutputName::parse(name).unwrap(),
        )
    }

    #[test]
    fn a_root_path_of_a_tree_entry_is_its_output_path_entry() {
        let gen_dir = output("//:gen", "gen");
        let entry = gen_dir.entry(&TreePath::parse("a/b.txt").unwrap());
        assert_eq!(RootPath::from(&entry).as_str(), "out/gen/a/b.txt");
    }

    #[test]
    fn a_root_path_of_a_source_is_its_text() {
        let source = SourcePath::parse("src/main.c").unwrap();
        assert_eq!(RootPath::from(&source).as_str(), "src/main.c");
        assert_eq!(RootPath::from(&source).to_string(), "src/main.c");
    }

    #[test]
    fn root_paths_sort_by_bytes() {
        let mut paths: Vec<RootPath> = ["a0", "a", "a/b", "B"]
            .into_iter()
            .map(|text| RootPath::from(&SourcePath::parse(text).unwrap()))
            .collect();
        paths.sort();
        let sorted: Vec<&str> = paths.iter().map(RootPath::as_str).collect();
        assert_eq!(sorted, ["B", "a", "a/b", "a0"]);
    }
}
