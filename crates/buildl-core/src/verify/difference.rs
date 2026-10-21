//! Where two manifests of one output first differ.
//!
//! Its own file because the walk is one pure decision over two values: it needs no port, so it
//! is tested here against hand-built manifests.
//!
//! Responsibilities: [`first_difference`].
//!
//! Non-responsibilities: choosing which outputs to compare, or in what order, which the flow that
//! calls this does.

use core::cmp::Ordering;

use crate::types::{
    EntryChange, FileChange, OutputDifference, OutputManifest, StoredFile, Tree, TreeEntry,
};

/// The first way the re-run's manifest of an output differs from the cached one; `None` when they
/// are equal.
///
/// A file against a file differs in its bytes, or failing that in its mode. A tree against a tree
/// is walked entry by entry in path order, and the earliest path that differs decides: a path
/// only one holds, else a path whose bytes differ, else a path whose mode differs. A file
/// against a tree, either way round, differs in kind.
#[must_use]
pub(crate) fn first_difference(
    cached: &OutputManifest,
    rerun: &OutputManifest,
) -> Option<OutputDifference> {
    match (cached, rerun) {
        (OutputManifest::File(cached), OutputManifest::File(rerun)) => {
            file_change(cached, rerun).map(OutputDifference::File)
        }
        (OutputManifest::Tree(cached), OutputManifest::Tree(rerun)) => {
            tree_difference(cached, rerun)
        }
        (OutputManifest::File(_), OutputManifest::Tree(_))
        | (OutputManifest::Tree(_), OutputManifest::File(_)) => Some(OutputDifference::Kind),
    }
}

fn file_change(cached: &StoredFile, rerun: &StoredFile) -> Option<FileChange> {
    if cached.digest != rerun.digest {
        Some(FileChange::Bytes)
    } else if cached.mode != rerun.mode {
        Some(FileChange::Mode)
    } else {
        None
    }
}

fn tree_difference(cached: &Tree, rerun: &Tree) -> Option<OutputDifference> {
    let mut cached_entries = cached.entries().iter().peekable();
    let mut rerun_entries = rerun.entries().iter().peekable();
    loop {
        let (path, change) = match (cached_entries.peek(), rerun_entries.peek()) {
            (None, None) => return None,
            (Some(entry), None) => (&entry.path, EntryChange::OnlyCached),
            (None, Some(entry)) => (&entry.path, EntryChange::OnlyRerun),
            (Some(cached_entry), Some(rerun_entry)) => {
                match cached_entry.path.cmp(&rerun_entry.path) {
                    Ordering::Less => (&cached_entry.path, EntryChange::OnlyCached),
                    Ordering::Greater => (&rerun_entry.path, EntryChange::OnlyRerun),
                    Ordering::Equal => {
                        if let Some(change) = entry_change(cached_entry, rerun_entry) {
                            (&cached_entry.path, change)
                        } else {
                            cached_entries.next();
                            rerun_entries.next();
                            continue;
                        }
                    }
                }
            }
        };
        return Some(OutputDifference::Entry {
            path: path.clone(),
            change,
        });
    }
}

fn entry_change(cached: &TreeEntry, rerun: &TreeEntry) -> Option<EntryChange> {
    if cached.digest != rerun.digest {
        Some(EntryChange::Bytes)
    } else if cached.mode != rerun.mode {
        Some(EntryChange::Mode)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;
    use crate::execute::fixtures::{file, tree};
    use crate::types::{Digest, FileMode, TreePath};

    fn path(raw: &str) -> TreePath {
        TreePath::parse(raw).unwrap()
    }

    fn entry(raw: &str, change: EntryChange) -> OutputDifference {
        OutputDifference::Entry {
            path: path(raw),
            change,
        }
    }

    fn executable(content: &str) -> OutputManifest {
        OutputManifest::File(StoredFile {
            digest: Digest::of(content.as_bytes()),
            mode: FileMode::Executable,
        })
    }

    /// A tree of one file per `(path, content, mode)`.
    fn moded(files: &[(&str, &str, FileMode)]) -> OutputManifest {
        let entries = files
            .iter()
            .map(|(raw, content, mode)| TreeEntry {
                path: path(raw),
                digest: Digest::of(content.as_bytes()),
                mode: *mode,
            })
            .collect();
        OutputManifest::Tree(Tree::new(entries).unwrap())
    }

    #[test]
    fn equal_manifests_have_no_difference() {
        assert_eq!(first_difference(&file("a"), &file("a")), None);
        let files = [("x/one", "1"), ("two", "2")];
        assert_eq!(first_difference(&tree(&files), &tree(&files)), None);
    }

    #[test]
    fn a_file_with_other_bytes_differs_in_bytes() {
        assert_eq!(
            first_difference(&file("a"), &file("b")),
            Some(OutputDifference::File(FileChange::Bytes))
        );
    }

    #[test]
    fn a_file_with_only_another_mode_differs_in_mode() {
        assert_eq!(
            first_difference(&file("a"), &executable("a")),
            Some(OutputDifference::File(FileChange::Mode))
        );
    }

    #[test]
    fn bytes_outrank_mode_for_a_file() {
        assert_eq!(
            first_difference(&file("a"), &executable("b")),
            Some(OutputDifference::File(FileChange::Bytes))
        );
    }

    #[test]
    fn a_tree_names_the_first_path_whose_bytes_differ() {
        let cached = tree(&[("a", "1"), ("b", "2"), ("c", "3")]);
        let rerun = tree(&[("a", "1"), ("b", "X"), ("c", "Y")]);
        assert_eq!(
            first_difference(&cached, &rerun),
            Some(entry("b", EntryChange::Bytes))
        );
        let cached = moded(&[("a", "1", FileMode::Plain), ("b", "2", FileMode::Plain)]);
        let rerun = moded(&[
            ("a", "1", FileMode::Plain),
            ("b", "2", FileMode::Executable),
        ]);
        assert_eq!(
            first_difference(&cached, &rerun),
            Some(entry("b", EntryChange::Mode))
        );
    }

    #[test]
    fn bytes_outrank_mode_for_a_tree_entry() {
        let cached = moded(&[("a", "1", FileMode::Plain)]);
        let rerun = moded(&[("a", "X", FileMode::Executable)]);
        assert_eq!(
            first_difference(&cached, &rerun),
            Some(entry("a", EntryChange::Bytes))
        );
    }

    #[test]
    fn a_tree_names_a_path_only_the_cache_holds() {
        let cached = tree(&[("a", "1"), ("b", "2")]);
        assert_eq!(
            first_difference(&cached, &tree(&[("a", "1")])),
            Some(entry("b", EntryChange::OnlyCached))
        );
        assert_eq!(
            first_difference(&cached, &tree(&[("b", "2")])),
            Some(entry("a", EntryChange::OnlyCached))
        );
    }

    #[test]
    fn a_tree_names_a_path_only_the_rerun_holds() {
        let rerun = tree(&[("a", "1"), ("b", "2")]);
        assert_eq!(
            first_difference(&tree(&[("a", "1")]), &rerun),
            Some(entry("b", EntryChange::OnlyRerun))
        );
        assert_eq!(
            first_difference(&tree(&[("b", "2")]), &rerun),
            Some(entry("a", EntryChange::OnlyRerun))
        );
    }

    #[test]
    fn the_earliest_path_wins_whatever_kind_of_change() {
        let bytes_first = first_difference(&tree(&[("a", "1"), ("b", "2")]), &tree(&[("a", "X")]));
        assert_eq!(bytes_first, Some(entry("a", EntryChange::Bytes)));
        let only_first = first_difference(
            &tree(&[("b", "2"), ("c", "3")]),
            &tree(&[("a", "0"), ("b", "X"), ("c", "3")]),
        );
        assert_eq!(only_first, Some(entry("a", EntryChange::OnlyRerun)));
        let mode_first = first_difference(
            &moded(&[("a", "1", FileMode::Plain), ("b", "2", FileMode::Plain)]),
            &moded(&[
                ("a", "1", FileMode::Executable),
                ("b", "X", FileMode::Plain),
            ]),
        );
        assert_eq!(mode_first, Some(entry("a", EntryChange::Mode)));
    }

    #[test]
    fn a_file_against_a_tree_is_a_change_of_kind() {
        let one_file = file("a");
        let a_tree = tree(&[("a", "a")]);
        assert_eq!(
            first_difference(&one_file, &a_tree),
            Some(OutputDifference::Kind)
        );
        assert_eq!(
            first_difference(&a_tree, &one_file),
            Some(OutputDifference::Kind)
        );
    }
}
