//! A directory an action left as an output: its files, by path.
//!
//! Its own file because a tree is one value made of many, and what it holds decides its digest:
//! the same files must give the same tree whatever order a filesystem lists them in, and a list
//! of files that could not exist together as a directory is not a tree at all.
//!
//! Responsibilities: [`TreeEntry`], and [`Tree`] with its [`Tree::new`] constructor, which puts
//! the files in one order and refuses a list that is not a tree.
//!
//! Non-responsibilities: walking a directory and storing its files, which are the ports'; and
//! what is not a regular file. A tree holds files only: it has no entry for a directory, so a
//! directory exists in it exactly when a file lies below it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};

use crate::error::{Error, Result, TreeFault};
use crate::types::{Digest, FileMode, StoredFile, TreePath};

/// One file of a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEntry {
    /// Where the file is, below the directory.
    pub path: TreePath,
    /// The digest of the file's bytes.
    pub digest: Digest,
    /// Whether the file may be run.
    pub mode: FileMode,
}

impl TreeEntry {
    /// The file itself, without where it is.
    #[must_use]
    pub const fn file(&self) -> StoredFile {
        StoredFile {
            digest: self.digest,
            mode: self.mode,
        }
    }
}

/// The files of one directory, in path order.
///
/// Built only by [`Tree::new`], so every tree holds at least one file, no path twice, and no
/// file at a path another file needs as a directory. Two trees holding the same files are
/// equal, whatever order the files were given in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Vec<TreeEntry>")]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

impl Tree {
    /// The tree holding `entries`, which may come in any order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidTree`] with the first of: [`TreeFault::Empty`] when there is no
    /// entry; [`TreeFault::Repeated`] with the lowest path listed twice; and
    /// [`TreeFault::Clash`] with a file another entry lies below — the one above the lowest
    /// such entry.
    pub fn new(mut entries: Vec<TreeEntry>) -> Result<Self> {
        if entries.is_empty() {
            return Err(Error::InvalidTree {
                fault: TreeFault::Empty,
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let repeated = entries
            .iter()
            .zip(entries.iter().skip(1))
            .find_map(|(entry, next)| (entry.path == next.path).then_some(&entry.path));
        if let Some(path) = repeated {
            return Err(Error::InvalidTree {
                fault: TreeFault::Repeated(path.clone()),
            });
        }
        let files: BTreeMap<&str, &TreePath> = entries
            .iter()
            .map(|entry| (entry.path.as_str(), &entry.path))
            .collect();
        let clash = entries
            .iter()
            .find_map(|entry| file_above(&files, entry.path.as_str()));
        if let Some(path) = clash {
            return Err(Error::InvalidTree {
                fault: TreeFault::Clash(path.clone()),
            });
        }
        Ok(Self { entries })
    }

    /// The files, in path order.
    #[must_use]
    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }
}

/// The file of `files` at a directory above `path`, if there is one.
fn file_above<'a>(files: &BTreeMap<&str, &'a TreePath>, path: &str) -> Option<&'a TreePath> {
    path.match_indices('/')
        .filter_map(|(at, _)| path.get(..at))
        .find_map(|above| files.get(above).copied())
}

/// Serializes as the list of its files, in path order.
impl Serialize for Tree {
    fn serialize<S: Serializer>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error> {
        self.entries.serialize(serializer)
    }
}

impl TryFrom<Vec<TreeEntry>> for Tree {
    type Error = Error;
    fn try_from(entries: Vec<TreeEntry>) -> Result<Self> {
        Self::new(entries)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{Tree, TreeEntry};
    use crate::error::{Error, TreeFault};
    use crate::json::canonical;
    use crate::types::{Digest, FileMode, StoredFile, TreePath};

    fn path(raw: &str) -> TreePath {
        TreePath::parse(raw).unwrap()
    }

    /// A plain file at `raw` whose bytes are its own path.
    fn entry(raw: &str) -> TreeEntry {
        TreeEntry {
            path: path(raw),
            digest: Digest::of(raw.as_bytes()),
            mode: FileMode::Plain,
        }
    }

    fn tree(paths: &[&str]) -> Tree {
        Tree::new(paths.iter().map(|raw| entry(raw)).collect()).unwrap()
    }

    fn fault(entries: Vec<TreeEntry>) -> TreeFault {
        match Tree::new(entries).unwrap_err() {
            Error::InvalidTree { fault } => fault,
            other => unreachable!("expected InvalidTree, got {other:?}"),
        }
    }

    #[test]
    fn the_files_are_kept_in_path_order_whatever_order_they_came_in() {
        let one = tree(&["b", "a/b", "a-x", "assets/app.js"]);
        let other = tree(&["assets/app.js", "a-x", "b", "a/b"]);
        assert_eq!(one, other);
        let paths: Vec<&str> = one
            .entries()
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert_eq!(paths, ["a-x", "a/b", "assets/app.js", "b"]);
        assert_eq!(
            canonical::to_vec(&one).unwrap(),
            canonical::to_vec(&other).unwrap()
        );
    }

    #[test]
    fn an_entry_gives_its_file_without_its_path() {
        let entry = TreeEntry {
            mode: FileMode::Executable,
            ..entry("bin/go")
        };
        assert_eq!(
            entry.file(),
            StoredFile {
                digest: Digest::of(b"bin/go"),
                mode: FileMode::Executable,
            }
        );
    }

    #[test]
    fn a_list_of_no_file_is_not_a_tree() {
        assert_eq!(fault(Vec::new()), TreeFault::Empty);
    }

    #[test]
    fn a_path_listed_twice_is_refused_and_the_lowest_is_named() {
        let entries = vec![entry("z"), entry("m"), entry("z"), entry("m"), entry("a")];
        assert_eq!(fault(entries), TreeFault::Repeated(path("m")));

        let other_content = TreeEntry {
            digest: Digest::of(b"other"),
            ..entry("a")
        };
        assert_eq!(
            fault(vec![entry("a"), other_content]),
            TreeFault::Repeated(path("a")),
            "the same path with another content"
        );
    }

    #[test]
    fn a_file_where_another_needs_a_directory_is_refused() {
        assert_eq!(
            fault(vec![entry("a/b"), entry("a")]),
            TreeFault::Clash(path("a"))
        );
        assert_eq!(
            fault(vec![entry("a"), entry("a-x"), entry("a/b/c")]),
            TreeFault::Clash(path("a")),
            "found past a path that sorts between the two"
        );
        assert_eq!(
            fault(vec![entry("lib/x/y"), entry("lib/x")]),
            TreeFault::Clash(path("lib/x")),
            "at any depth"
        );
    }

    #[test]
    fn a_name_that_only_starts_like_another_is_no_clash() {
        assert_eq!(tree(&["lib", "lib.o", "lib2/x"]).entries().len(), 3);
    }

    #[test]
    fn serializes_as_the_list_of_its_files_in_path_order() {
        let digest_a = Digest::of(b"a");
        let digest_b = Digest::of(b"bin/go");
        let run = TreeEntry {
            mode: FileMode::Executable,
            ..entry("bin/go")
        };
        let tree = Tree::new(vec![run, entry("a")]).unwrap();
        assert_eq!(
            canonical::to_string(&tree).unwrap(),
            format!(
                concat!(
                    r#"[{{"digest":"{a}","mode":"plain","path":"a"}},"#,
                    r#"{{"digest":"{b}","mode":"executable","path":"bin/go"}}]"#
                ),
                a = digest_a,
                b = digest_b
            )
        );
    }

    #[test]
    fn round_trips_through_json_and_reads_back_through_the_constructor() {
        let tree = tree(&["assets/app.js", "index.html"]);
        let json = canonical::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<Tree>(&json).unwrap(), tree);

        let digest = Digest::of(b"a");
        let entry =
            |path: &str| format!(r#"{{"digest":"{digest}","mode":"plain","path":"{path}"}}"#);
        let listed = |paths: &[&str]| {
            let entries: Vec<String> = paths.iter().map(|path| entry(path)).collect();
            format!("[{}]", entries.join(","))
        };
        assert_eq!(
            serde_json::from_str::<Tree>(&listed(&["b", "a"])).unwrap(),
            Tree::new(vec![
                TreeEntry {
                    path: path("a"),
                    digest,
                    mode: FileMode::Plain
                },
                TreeEntry {
                    path: path("b"),
                    digest,
                    mode: FileMode::Plain
                },
            ])
            .unwrap(),
            "a list in another order is put in path order"
        );
        assert!(serde_json::from_str::<Tree>("[]").is_err());
        assert!(serde_json::from_str::<Tree>(&listed(&["a", "a"])).is_err());
        assert!(serde_json::from_str::<Tree>(&listed(&["a", "a/b"])).is_err());
        assert!(serde_json::from_str::<Tree>(&listed(&["../a"])).is_err());
    }
}
