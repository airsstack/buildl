//! The `IgnoreRules` port over a workspace's `.buildlignore` files.
//!
//! Its own file because it holds the one decision the port exists for: which folders and files
//! bear on a path, and in what order they are asked.
//!
//! Responsibilities: [`BuildlIgnore`], the cache of the parsed files it reads, and the root-down
//! decision for a path.
//!
//! Non-responsibilities: reading and parsing one file, which `FolderRules` does.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use buildl_core::{EntryKind, IgnoreRules, Ignoring, Result};

use super::folder_rules::{Decision, FolderRules};

/// A folder's parsed file: `None` when the folder has none.
type Cached = Option<Arc<FolderRules>>;

/// What every handle of one workspace shares.
struct Inner {
    root: PathBuf,
    folders: Mutex<HashMap<String, Cached>>,
}

/// The `.buildlignore` files of one workspace, asked about one path at a time.
///
/// A cheap-to-clone handle: clones share the files already read. Each folder's file is read the
/// first time a path needs it and a file that was used is not read again, so a change to one
/// shows only in a new instance. A file that is refused is not kept: it is read, and refused,
/// again on every question that needs it. Two instances over the same root are equal.
#[derive(Clone)]
pub struct BuildlIgnore {
    inner: Arc<Inner>,
}

impl BuildlIgnore {
    /// Rules over the workspace at `root`. Nothing is read until a path is asked about.
    ///
    /// `root` must be the canonical, absolute path of the workspace root: every file is read
    /// under it, so a relative root would follow the process's working directory.
    ///
    /// # Panics
    ///
    /// Panics in a debug build when `root` is not absolute.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        debug_assert!(root.is_absolute(), "the workspace root must be absolute");
        Self {
            inner: Arc::new(Inner {
                root,
                folders: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// The root the rules read from.
    fn root(&self) -> &Path {
        &self.inner.root
    }

    /// The parsed file of `folder` (workspace-relative, empty for the root), read on first use.
    fn rules_of(&self, folder: &str) -> Result<Cached> {
        let mut folders = self
            .inner
            .folders
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(cached) = folders.get(folder) {
            return Ok(cached.clone());
        }
        let read = FolderRules::read(self.root(), folder)?.map(Arc::new);
        folders.insert(folder.to_owned(), read.clone());
        drop(folders);
        Ok(read)
    }

    /// Whether `path` is a folder, resolving `Unknown` by looking at the disk: an existing folder
    /// is one, an existing file and a missing path are files.
    fn is_folder(&self, path: &str, kind: EntryKind) -> bool {
        match kind {
            EntryKind::Folder => true,
            EntryKind::File => false,
            EntryKind::Unknown => {
                fs::symlink_metadata(self.root().join(path)).is_ok_and(|meta| meta.is_dir())
            }
        }
    }

    /// Asks `folders`, each a folder's workspace-relative path paired with the target's path
    /// relative to that folder, listed from the root down, deepest first. The first file with a
    /// line that matches decides.
    fn decide_target(&self, folders: &[(&str, &str)], is_folder: bool) -> Result<Decision> {
        for (folder, relative) in folders.iter().rev() {
            let Some(rules) = self.rules_of(folder)? else {
                continue;
            };
            match rules.decide(relative, is_folder) {
                Decision::NoMatch => {}
                decided => return Ok(decided),
            }
        }
        Ok(Decision::NoMatch)
    }
}

impl IgnoreRules for BuildlIgnore {
    fn ignored(&self, path: &str, kind: EntryKind) -> Result<Option<Ignoring>> {
        // The folders on the way down end at each `/`; the path itself is the last target.
        let ends: Vec<usize> = path.match_indices('/').map(|(end, _)| end).collect();
        for (depth, end) in ends.iter().copied().chain([path.len()]).enumerate() {
            let on_the_way = depth < ends.len();
            let (target, _) = path.split_at(end);
            let is_folder = on_the_way || self.is_folder(path, kind);
            // The root's file bears on the target, and so does that of each folder above it.
            let mut folders = vec![("", target)];
            for folder_end in ends.iter().copied().take(depth) {
                let (folder, _) = path.split_at(folder_end);
                let (_, relative) = target.split_at(folder_end + 1);
                folders.push((folder, relative));
            }
            if let Decision::Ignored(ignoring) = self.decide_target(&folders, is_folder)? {
                return Ok(Some(ignoring));
            }
        }
        Ok(None)
    }
}

impl fmt::Debug for BuildlIgnore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuildlIgnore")
            .field("root", &self.inner.root)
            .finish()
    }
}

impl PartialEq for BuildlIgnore {
    fn eq(&self, other: &Self) -> bool {
        self.inner.root == other.inner.root
    }
}

impl Eq for BuildlIgnore {}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::Path;

    use buildl_core::{EntryKind, Error, IgnoreRules};
    use tempfile::TempDir;

    use super::BuildlIgnore;

    /// A workspace of the given files, `(path, bytes)`, and the rules over it.
    fn workspace(files: &[(&str, &[u8])]) -> (TempDir, BuildlIgnore) {
        let dir = tempfile::tempdir().unwrap();
        for (path, bytes) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, bytes).unwrap();
        }
        let rules = BuildlIgnore::new(dir.path());
        (dir, rules)
    }

    fn ignored(rules: &BuildlIgnore, path: &str, kind: EntryKind) -> bool {
        rules.ignored(path, kind).unwrap().is_some()
    }

    fn file_ignored(rules: &BuildlIgnore, path: &str) -> bool {
        ignored(rules, path, EntryKind::File)
    }

    #[test]
    fn a_missing_file_has_no_rules() {
        let (_dir, rules) = workspace(&[("src/a.c", b"")]);
        assert!(!file_ignored(&rules, "src/a.c"));
        assert!(!ignored(&rules, "src", EntryKind::Folder));
    }

    #[test]
    fn an_empty_or_comment_only_file_ignores_nothing() {
        let (_dir, rules) = workspace(&[
            (".buildlignore", b""),
            ("lib/.buildlignore", b"\n# a comment\n"),
            ("lib/l.rs", b""),
        ]);
        assert!(!file_ignored(&rules, "lib/l.rs"));
        assert!(!file_ignored(&rules, "a.txt"));
    }

    #[test]
    fn a_bom_does_not_hide_the_first_pattern() {
        let (_dir, rules) = workspace(&[(".buildlignore", "\u{feff}notes/\n".as_bytes())]);
        assert!(ignored(&rules, "notes", EntryKind::Folder));
    }

    #[test]
    fn braces_are_literal() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"{a\n")]);
        assert!(file_ignored(&rules, "{a"));
        assert!(!file_ignored(&rules, "a"));

        let (_dir, rules) = workspace(&[(".buildlignore", b"{a,b}\n")]);
        assert!(file_ignored(&rules, "{a,b}"));
        assert!(!file_ignored(&rules, "a"));

        let (_dir, rules) = workspace(&[(".buildlignore", b"\\{a\n")]);
        assert!(file_ignored(&rules, "{a"));
        assert!(!file_ignored(&rules, "\\{a"));

        let (_dir, rules) = workspace(&[(".buildlignore", b"\\\\{a\n")]);
        assert!(file_ignored(&rules, "\\{a"));
        assert!(!file_ignored(&rules, "{a"));
    }

    #[test]
    fn the_pattern_is_reported_as_written() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"{a\n")]);
        let ignoring = rules.ignored("{a", EntryKind::File).unwrap().unwrap();
        assert_eq!(ignoring.pattern(), "{a");
        assert_eq!(ignoring.file(), ".buildlignore");
    }

    #[test]
    fn a_bad_pattern_is_refused_with_its_line() {
        let (_dir, rules) = workspace(&[("lib/.buildlignore", b"[z-a]\n")]);
        let err = rules.ignored("lib/l.rs", EntryKind::File).unwrap_err();
        assert!(matches!(
            err,
            Error::IgnoreFile { line: Some(1), ref file, .. } if file == "lib/.buildlignore"
        ));
        assert_eq!(
            err.to_string(),
            "lib/.buildlignore:1: error parsing glob '[z-a]': invalid range; 'z' > 'a'"
        );
    }

    #[test]
    fn non_utf8_text_is_refused_with_its_line() {
        let (_dir, rules) = workspace(&[("lib/.buildlignore", b"ok\n\xff\xfe\n")]);
        let err = rules.ignored("lib/l.rs", EntryKind::File).unwrap_err();
        assert_eq!(err.to_string(), "lib/.buildlignore:2: not UTF-8");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_refused() {
        let (dir, rules) = workspace(&[("real", b"x\n"), ("lib/l.rs", b"")]);
        std::os::unix::fs::symlink(
            dir.path().join("real"),
            dir.path().join("lib/.buildlignore"),
        )
        .unwrap();
        let err = rules.ignored("lib/l.rs", EntryKind::File).unwrap_err();
        assert!(matches!(err, Error::IgnoreFile { line: None, .. }));
        assert_eq!(err.to_string(), "lib/.buildlignore: a symlink, not a file");
    }

    #[test]
    fn an_unreadable_file_is_refused() {
        let (dir, rules) = workspace(&[("lib/l.rs", b"")]);
        fs::create_dir(dir.path().join("lib/.buildlignore")).unwrap();
        let err = rules.ignored("lib/l.rs", EntryKind::File).unwrap_err();
        assert!(matches!(
            err,
            Error::IgnoreFile { line: None, ref file, .. } if file == "lib/.buildlignore"
        ));
    }

    #[test]
    fn case_matters() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"Notes/\n"), ("notes/a.md", b"")]);
        assert!(!ignored(&rules, "notes", EntryKind::Folder));
        assert!(!file_ignored(&rules, "notes/a.md"));
        assert!(ignored(&rules, "Notes", EntryKind::Folder));
    }

    #[test]
    fn trailing_slash_matches_folders_only() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"docs/\n")]);
        assert!(ignored(&rules, "docs", EntryKind::Folder));
        assert!(!file_ignored(&rules, "docs"));
    }

    #[test]
    fn a_root_pattern_ignores_at_any_depth() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"docs/\n")]);
        assert!(file_ignored(&rules, "lib/docs/d.md"));
        assert!(!file_ignored(&rules, "lib/docs.txt"));
    }

    #[test]
    fn a_leading_slash_anchors_to_the_file() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"/a.log\n")]);
        assert!(file_ignored(&rules, "a.log"));
        assert!(!file_ignored(&rules, "lib/a.log"));
    }

    #[test]
    fn a_nested_file_applies_below_its_folder() {
        let (_dir, rules) = workspace(&[("src/.buildlignore", b"gen/*\n!gen/keep.rs\n")]);
        assert!(file_ignored(&rules, "src/gen/x.rs"));
        assert!(!file_ignored(&rules, "src/gen/keep.rs"));
        assert!(!file_ignored(&rules, "gen/x.rs"));
    }

    #[test]
    fn a_deeper_file_wins() {
        let (_dir, rules) = workspace(&[
            (".buildlignore", b"*.log\n"),
            ("logs/.buildlignore", b"!keep.log\n"),
        ]);
        assert!(!file_ignored(&rules, "logs/keep.log"));
        assert!(file_ignored(&rules, "logs/other.log"));
        assert!(file_ignored(&rules, "a.log"));
    }

    #[test]
    fn no_reinclude_under_an_ignored_folder() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"notes/\n!notes/keep/\n")]);
        let ignoring = rules
            .ignored("notes/keep/b.md", EntryKind::File)
            .unwrap()
            .unwrap();
        assert_eq!(ignoring.pattern(), "notes/");
        assert!(ignored(&rules, "notes/keep", EntryKind::Folder));
    }

    #[test]
    fn a_self_ignoring_file_still_applies() {
        let (_dir, rules) = workspace(&[("sub/.buildlignore", b"x.txt\n.buildlignore\n")]);
        assert!(file_ignored(&rules, "sub/.buildlignore"));
        assert!(file_ignored(&rules, "sub/x.txt"));
        assert!(!file_ignored(&rules, "sub/y.txt"));
    }

    #[test]
    fn other_ignore_files_are_never_read() {
        let (_dir, rules) = workspace(&[
            (".gitignore", b"src/\n"),
            (".ignore", b"src/\n"),
            ("src/a.c", b""),
        ]);
        assert!(!file_ignored(&rules, "src/a.c"));
    }

    #[test]
    fn a_file_above_the_root_is_never_read() {
        let (dir, _) = workspace(&[(".buildlignore", b"*\n"), ("ws/a.c", b"")]);
        let rules = BuildlIgnore::new(dir.path().join("ws"));
        assert!(!file_ignored(&rules, "a.c"));
        assert!(!ignored(&rules, "ws", EntryKind::Folder));
    }

    #[test]
    fn unknown_on_an_existing_folder_is_a_folder() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"notes/\n"), ("notes/a.md", b"")]);
        assert!(ignored(&rules, "notes", EntryKind::Unknown));
    }

    #[test]
    fn unknown_on_a_missing_path_is_a_file() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"notes/\n")]);
        assert!(!ignored(&rules, "notes", EntryKind::Unknown));

        let (_dir, rules) = workspace(&[(".buildlignore", b"/a.log\n")]);
        assert!(ignored(&rules, "a.log", EntryKind::Unknown));
    }

    #[test]
    fn a_missing_file_under_an_ignored_folder_is_ignored() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"notes/\n")]);
        assert!(ignored(&rules, "notes/a.md", EntryKind::Unknown));
    }

    #[test]
    fn a_bad_file_on_the_way_is_an_error() {
        let (_dir, rules) = workspace(&[("lib/.buildlignore", b"[z-a]\n"), ("lib/deep/l.rs", b"")]);
        assert!(matches!(
            rules.ignored("lib/deep/l.rs", EntryKind::File),
            Err(Error::IgnoreFile { .. })
        ));
    }

    #[test]
    fn a_path_under_a_regular_file_is_not_ignored() {
        let (_dir, rules) = workspace(&[(".buildlignore", b"c.txt\n"), ("a.txt", b"")]);
        assert!(!file_ignored(&rules, "a.txt/b"));
    }

    #[test]
    fn each_file_is_read_once() {
        let (dir, rules) = workspace(&[(".buildlignore", b"a.log\n")]);
        assert!(file_ignored(&rules, "a.log"));
        fs::remove_file(dir.path().join(".buildlignore")).unwrap();
        assert!(file_ignored(&rules, "a.log"));
        assert!(!file_ignored(&BuildlIgnore::new(dir.path()), "a.log"));
    }

    #[test]
    fn instances_over_one_root_are_equal() {
        let one = BuildlIgnore::new("/ws");
        assert_eq!(one, BuildlIgnore::new(Path::new("/ws")));
        assert_ne!(one, BuildlIgnore::new("/other"));
        assert!(format!("{one:?}").contains("/ws"));
    }
}
