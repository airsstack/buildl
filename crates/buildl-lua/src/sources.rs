//! The host-side walk behind `buildl.sources(pattern)`, the declaration phase's only filesystem
//! read.
//!
//! Its own file because it is the one place a build file's evaluation touches the filesystem. The
//! Lua engine holds no filesystem grant at all, so containment is enforced here and nowhere else:
//! the walk starts at the declaring directory, refuses a directory that resolves outside the
//! workspace, never follows a symlink, never enters a directory it was told to skip, and returns
//! regular files only.
//!
//! Responsibilities: [`Sources`], one declaring directory's walk.
//!
//! Non-responsibilities: turning the paths into inputs. They are returned to Lua as text relative
//! to the declaring directory, and joined with it after evaluation.

use core::cell::RefCell;
use core::num::NonZeroU64;
use core::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use buildl_core::{Directory, EntryKind, Error, EvaluationFailure, EvaluationLimit, IgnoreRules};
use globset::GlobBuilder;
use walkdir::{DirEntry, WalkDir};

use crate::refusal::Refusal;

/// The walk for one declaring directory.
///
/// Clones share one entry count, so every `buildl.sources` call of one build file draws on the same
/// ceiling.
///
/// The walk never lists, enters or counts anything inside a skipped directory. A skipped
/// directory is a workspace-relative path, matched against the whole path from the workspace
/// root: a directory of the same name deeper in the tree is not skipped. A declaring directory
/// that is itself a skipped directory, or lies inside one, yields no files.
///
/// The walk also never lists, enters or counts a path `ignore` reports ignored, and yields no
/// files when its declaring directory is ignored.
#[derive(Debug, Clone)]
pub(crate) struct Sources<I> {
    root: PathBuf,
    directory: PathBuf,
    skipped: Arc<[PathBuf]>,
    ignore: I,
    ceiling: NonZeroU64,
    visited: Arc<AtomicU64>,
}

impl<I: IgnoreRules> Sources<I> {
    /// The walk for `directory`, under the canonical workspace `root`, that visits at most
    /// `ceiling` directory entries over its lifetime, skips every directory in `skipped` and
    /// leaves out every path `ignore` reports ignored.
    pub(crate) fn new(
        root: &Path,
        directory: &Directory,
        ceiling: NonZeroU64,
        skipped: &[Directory],
        ignore: I,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            directory: root.join(directory.as_str()),
            skipped: skipped
                .iter()
                .map(|skipped| root.join(skipped.as_str()))
                .collect(),
            ignore,
            ceiling,
            visited: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Every regular file under the declaring directory whose relative path matches `pattern`,
    /// `/`-separated and sorted by bytes.
    ///
    /// The matcher uses the same flags as airsl's `glob` module: `*` stops at `/`, and `**/`
    /// matches zero or more directories. Every entry the walk visits, of any kind, counts against
    /// the ceiling; visiting more than it refuses with the walk limit. A skipped directory and an
    /// ignored entry are neither visited nor counted, so an ignored folder is never entered. The
    /// declaring directory is asked about first, unless it is the workspace root, and yields
    /// nothing when it is ignored.
    ///
    /// An error from the ignore rules stops the walk and is returned as a runtime refusal with
    /// the error's text.
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
        if self.skipped.iter().any(|skipped| base.starts_with(skipped)) {
            return Ok(found);
        }
        if self
            .is_ignored(&base, EntryKind::Folder)
            .map_err(|error| rule_refusal(&error))?
        {
            return Ok(found);
        }
        // The predicate can only answer keep or drop, so the first rule error waits here until
        // the loop reads it.
        let rule_error: RefCell<Option<Error>> = RefCell::new(None);
        for entry in WalkDir::new(&base)
            .follow_root_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || self.keeps(entry, &rule_error))
        {
            if let Some(error) = rule_error.take() {
                return Err(rule_refusal(&error));
            }
            let entry = entry
                .map_err(|error| Refusal::new(EvaluationFailure::Runtime, error.to_string()))?;
            if self.visited.fetch_add(1, Ordering::Relaxed) >= self.ceiling.get() {
                return Err(Refusal::new(
                    EvaluationFailure::LimitReached {
                        limit: EvaluationLimit::Walk,
                    },
                    format!("walk budget of {} entries exceeded", self.ceiling),
                ));
            }
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
        if let Some(error) = rule_error.take() {
            return Err(rule_refusal(&error));
        }
        found.sort();
        Ok(found)
    }

    /// Whether the walk goes on to `entry`: it is not a skipped directory and the ignore rules do
    /// not report it ignored. An error from the rules is kept in `rule_error`, unless one is
    /// already there, and drops the entry.
    fn keeps(&self, entry: &DirEntry, rule_error: &RefCell<Option<Error>>) -> bool {
        if self
            .skipped
            .iter()
            .any(|skipped| entry.path() == skipped.as_path())
        {
            return false;
        }
        let kind = if entry.file_type().is_dir() {
            EntryKind::Folder
        } else {
            EntryKind::File
        };
        match self.is_ignored(entry.path(), kind) {
            Ok(ignored) => !ignored,
            Err(error) => {
                rule_error.borrow_mut().get_or_insert(error);
                false
            }
        }
    }

    /// Whether the ignore rules report the absolute `path` ignored. The workspace root and a path
    /// whose workspace-relative form is not UTF-8 are not asked about and are not ignored.
    fn is_ignored(&self, path: &Path, kind: EntryKind) -> Result<bool, Error> {
        let Some(relative) = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(|relative| relative.to_str())
        else {
            return Ok(false);
        };
        if relative.is_empty() {
            return Ok(false);
        }
        Ok(self.ignore.ignored(relative, kind)?.is_some())
    }
}

/// An ignore-rules error as the refusal a `buildl.sources` call fails with.
fn rule_refusal(error: &Error) -> Refusal {
    Refusal::new(EvaluationFailure::Runtime, error.to_string())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroU64;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use buildl_core::{
        Directory, EntryKind, Error, EvaluationFailure, EvaluationLimit, IgnoreRules, Ignoring,
        NothingIgnored,
    };

    use super::Sources;
    use crate::limits::DeclarationLimits;

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

    fn sources(root: &Path, directory: &str) -> Sources<NothingIgnored> {
        sources_capped(root, directory, DeclarationLimits::default().walk_entries())
    }

    fn sources_capped(
        root: &Path,
        directory: &str,
        walk_entries: NonZeroU64,
    ) -> Sources<NothingIgnored> {
        let directory = if directory.is_empty() {
            Directory::root()
        } else {
            Directory::parse(directory).unwrap()
        };
        Sources::new(root, &directory, walk_entries, &[], NothingIgnored)
    }

    fn skipping(root: &Path, directory: &str, skipped: &[&str]) -> Sources<NothingIgnored> {
        let directory = if directory.is_empty() {
            Directory::root()
        } else {
            Directory::parse(directory).unwrap()
        };
        let skipped: Vec<Directory> = skipped
            .iter()
            .map(|path| Directory::parse(*path).unwrap())
            .collect();
        Sources::new(
            root,
            &directory,
            DeclarationLimits::default().walk_entries(),
            &skipped,
            NothingIgnored,
        )
    }

    fn entries(count: u64) -> NonZeroU64 {
        NonZeroU64::new(count).unwrap()
    }

    fn walk_limit() -> EvaluationFailure {
        EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Walk,
        }
    }

    /// Rules ignoring the workspace-relative paths in `ignored`, failing the question about the
    /// path in `fails_on`, and remembering every question asked.
    #[derive(Debug, Clone, Default)]
    struct Rules {
        ignored: Vec<&'static str>,
        fails_on: Option<&'static str>,
        asked: Arc<Mutex<Vec<(String, EntryKind)>>>,
    }

    impl Rules {
        fn ignoring(ignored: &[&'static str]) -> Self {
            Self {
                ignored: ignored.to_vec(),
                ..Self::default()
            }
        }

        fn failing_on(path: &'static str) -> Self {
            Self {
                fails_on: Some(path),
                ..Self::default()
            }
        }

        fn asked(&self) -> Vec<(String, EntryKind)> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl IgnoreRules for Rules {
        fn ignored(&self, path: &str, kind: EntryKind) -> buildl_core::Result<Option<Ignoring>> {
            self.asked.lock().unwrap().push((path.to_owned(), kind));
            if self.fails_on == Some(path) {
                return Err(Error::IgnoreFile {
                    file: "lib/.buildlignore".to_owned(),
                    line: Some(1),
                    reason: "unusable".to_owned(),
                });
            }
            Ok(self
                .ignored
                .contains(&path)
                .then(|| Ignoring::new(".buildlignore", path)))
        }
    }

    fn with_rules(
        root: &Path,
        directory: &str,
        skipped: &[&str],
        rules: Rules,
        walk_entries: NonZeroU64,
    ) -> Sources<Rules> {
        let directory = if directory.is_empty() {
            Directory::root()
        } else {
            Directory::parse(directory).unwrap()
        };
        let skipped: Vec<Directory> = skipped
            .iter()
            .map(|path| Directory::parse(*path).unwrap())
            .collect();
        Sources::new(root, &directory, walk_entries, &skipped, rules)
    }

    fn unbounded() -> NonZeroU64 {
        DeclarationLimits::default().walk_entries()
    }

    #[test]
    fn a_walk_that_visits_exactly_the_ceiling_passes() {
        // The root directory and three files are four entries.
        let (_dir, root) = workspace(&["a.c", "b.c", "c.c"]);
        let found = sources_capped(&root, "", entries(4)).find("*.c").unwrap();
        assert_eq!(found, ["a.c", "b.c", "c.c"]);
    }

    #[test]
    fn a_walk_that_visits_more_than_the_ceiling_is_refused() {
        let (_dir, root) = workspace(&["a.c", "b.c", "c.c"]);
        let refusal = sources_capped(&root, "", entries(3))
            .find("*.c")
            .unwrap_err();
        assert_eq!(refusal.failure(), &walk_limit());
        assert_eq!(
            refusal.diagnostic("sources", None).as_str(),
            "buildl.sources: walk budget of 3 entries exceeded"
        );
    }

    #[test]
    fn entries_are_counted_across_calls() {
        let (_dir, root) = workspace(&["a.c", "b.c", "c.c"]);
        let walk = sources_capped(&root, "", entries(7));
        assert!(walk.find("*.c").is_ok());
        let refusal = walk.find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &walk_limit());
    }

    #[test]
    fn calls_that_together_stay_at_the_ceiling_pass() {
        let (_dir, root) = workspace(&["a.c", "b.c", "c.c"]);
        let walk = sources_capped(&root, "", entries(8));
        assert!(walk.find("*.c").is_ok());
        assert!(walk.find("*.c").is_ok());
        assert!(walk.find("*.c").is_err());
    }

    #[test]
    fn a_clone_shares_the_count() {
        let (_dir, root) = workspace(&["a.c", "b.c", "c.c"]);
        let walk = sources_capped(&root, "", entries(7));
        let copy = walk.clone();
        assert!(walk.find("*.c").is_ok());
        assert_eq!(copy.find("*.c").unwrap_err().failure(), &walk_limit());
    }

    #[test]
    fn directories_and_symlinks_count_as_entries() {
        let (_dir, root) = workspace(&["d/a.c"]);
        symlink(root.join("d"), root.join("link")).unwrap();
        // root, d, d/a.c and link.
        assert!(sources_capped(&root, "", entries(4)).find("**/*.c").is_ok());
        assert_eq!(
            sources_capped(&root, "", entries(3))
                .find("**/*.c")
                .unwrap_err()
                .failure(),
            &walk_limit()
        );
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
        assert_eq!(
            sources(&root, "lib").find("../**/*.c").unwrap(),
            Vec::<String>::new()
        );
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

    #[test]
    fn skipped_directories_are_not_listed_from_the_workspace_root() {
        let (_dir, root) = workspace(&["a.c", "out/o.c", ".buildl/s.c", "lib/b.c"]);
        assert_eq!(
            skipping(&root, "", &["out", ".buildl"])
                .find("**/*.c")
                .unwrap(),
            ["a.c", "lib/b.c"]
        );
    }

    #[test]
    fn a_skipped_directory_is_whatever_was_configured() {
        let (_dir, root) = workspace(&["a.c", "out/o.c", "target/t.c", "var/state/s.c"]);
        assert_eq!(
            skipping(&root, "", &["target", "var/state"])
                .find("**/*.c")
                .unwrap(),
            ["a.c", "out/o.c"]
        );
    }

    #[test]
    fn a_skipped_directory_is_matched_by_its_workspace_relative_path_only() {
        let (_dir, root) = workspace(&["out/o.c", "lib/out/keep.c", "lib/out/deep/keep.c"]);
        assert_eq!(
            skipping(&root, "", &["out"]).find("**/*.c").unwrap(),
            ["lib/out/deep/keep.c", "lib/out/keep.c"]
        );
    }

    #[test]
    fn a_skipped_directory_below_the_declaring_directory_is_pruned() {
        let (_dir, root) = workspace(&["lib/a.c", "lib/gen/g.c", "app/b.c"]);
        assert_eq!(
            skipping(&root, "lib", &["lib/gen"]).find("**/*.c").unwrap(),
            ["a.c"]
        );
    }

    #[test]
    fn a_shared_prefix_is_not_a_skipped_directory() {
        let (_dir, root) = workspace(&["out/o.c", "out2/keep.c", "outer.c"]);
        assert_eq!(
            skipping(&root, "", &["out"]).find("**/*.c").unwrap(),
            ["out2/keep.c", "outer.c"]
        );
    }

    #[test]
    fn a_skipped_directory_is_not_entered_so_it_costs_no_walk_entries() {
        let (_dir, root) = workspace(&["a.c", "out/1.c", "out/2.c", "out/sub/3.c"]);
        let skipped = [Directory::parse("out").unwrap()];
        // The root and a.c are two entries; none of out is visited.
        let walk = Sources::new(
            &root,
            &Directory::root(),
            entries(2),
            &skipped,
            NothingIgnored,
        );
        assert_eq!(walk.find("**/*.c").unwrap(), ["a.c"]);
    }

    #[test]
    fn a_declaring_directory_inside_a_skipped_directory_lists_nothing() {
        let (_dir, root) = workspace(&["out/pkg/a.c", "out/b.c"]);
        let skipped = [Directory::parse("out").unwrap()];
        let walk = Sources::new(
            &root,
            &Directory::parse("out/pkg").unwrap(),
            entries(1),
            &skipped,
            NothingIgnored,
        );
        assert_eq!(walk.find("*.c").unwrap(), Vec::<String>::new());
        let walk = Sources::new(
            &root,
            &Directory::parse("out").unwrap(),
            entries(1),
            &skipped,
            NothingIgnored,
        );
        assert_eq!(walk.find("**/*.c").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_missing_declaring_directory_is_still_refused_when_directories_are_skipped() {
        let (_dir, root) = workspace(&[]);
        let refusal = skipping(&root, "gone", &["out"]).find("*.c").unwrap_err();
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

    #[test]
    fn an_ignored_folder_is_not_entered() {
        let mut files: Vec<String> = (0..50).map(|index| format!("notes/{index}.md")).collect();
        files.push("a.c".to_owned());
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_dir, root) = workspace(&files);
        // The root and a.c are two entries; none of notes is visited.
        let walk = with_rules(&root, "", &[], Rules::ignoring(&["notes"]), entries(2));
        assert_eq!(walk.find("**/*").unwrap(), ["a.c"]);
    }

    #[test]
    fn an_ignored_file_is_not_listed() {
        let (_dir, root) = workspace(&["a.log", "b.log"]);
        let walk = with_rules(&root, "", &[], Rules::ignoring(&["a.log"]), unbounded());
        assert_eq!(walk.find("*.log").unwrap(), ["b.log"]);
    }

    #[test]
    fn a_buildlignore_is_listed_unless_ignored() {
        let (_dir, root) = workspace(&[".buildlignore", "a.c"]);
        assert_eq!(
            sources(&root, "").find("**/*").unwrap(),
            [".buildlignore", "a.c"]
        );
        let walk = with_rules(
            &root,
            "",
            &[],
            Rules::ignoring(&[".buildlignore"]),
            unbounded(),
        );
        assert_eq!(walk.find("**/*").unwrap(), ["a.c"]);
    }

    #[test]
    fn entries_are_asked_with_their_kind_and_full_path() {
        let (_dir, root) = workspace(&["lib/docs/d.md", "lib/l.rs"]);
        let rules = Rules::default();
        let walk = with_rules(&root, "lib", &[], rules.clone(), unbounded());
        assert_eq!(walk.find("**/*").unwrap(), ["docs/d.md", "l.rs"]);
        assert_eq!(
            rules.asked(),
            [
                ("lib".to_owned(), EntryKind::Folder),
                ("lib/docs".to_owned(), EntryKind::Folder),
                ("lib/docs/d.md".to_owned(), EntryKind::File),
                ("lib/l.rs".to_owned(), EntryKind::File),
            ]
        );
    }

    #[test]
    fn the_skip_list_wins_over_the_rules() {
        let (_dir, root) = workspace(&["a.c", "out/o.c", ".buildl/s.c"]);
        for rules in [
            Rules::default(),
            Rules::ignoring(&["a.c", "out/o.c", ".buildl/s.c"]),
        ] {
            let walk = with_rules(&root, "", &["out", ".buildl"], rules.clone(), unbounded());
            let listed = walk.find("**/*.c").unwrap();
            assert!(
                listed.iter().all(|path| path == "a.c"),
                "{listed:?} for {rules:?}"
            );
            assert!(
                rules
                    .asked()
                    .iter()
                    .all(|(path, _)| !path.starts_with("out") && !path.starts_with(".buildl")),
                "{:?}",
                rules.asked()
            );
        }
    }

    #[test]
    fn a_walk_from_an_ignored_folder_lists_nothing() {
        let (_dir, root) = workspace(&["notes/a.md", "notes/sub/b.md"]);
        let walk = with_rules(&root, "notes", &[], Rules::ignoring(&["notes"]), entries(1));
        assert_eq!(walk.find("**/*").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_rule_error_on_the_last_entry_is_returned_with_its_text() {
        let (_dir, root) = workspace(&["lib/a.c", "lib/b.c"]);
        let walk = with_rules(&root, "lib", &[], Rules::failing_on("lib/b.c"), unbounded());
        let refusal = walk.find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
        assert_eq!(
            refusal.diagnostic("sources", None).as_str(),
            "buildl.sources: lib/.buildlignore:1: unusable"
        );
    }

    #[test]
    fn a_rule_error_stops_the_walk_before_later_entries() {
        let (_dir, root) = workspace(&["lib/a.c", "lib/b.c", "lib/c.c"]);
        let rules = Rules::failing_on("lib/a.c");
        let walk = with_rules(&root, "lib", &[], rules.clone(), unbounded());
        let refusal = walk.find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
        assert_eq!(
            refusal.diagnostic("sources", None).as_str(),
            "buildl.sources: lib/.buildlignore:1: unusable"
        );
        assert!(
            rules.asked().iter().all(|(path, _)| path != "lib/c.c"),
            "{:?}",
            rules.asked()
        );
    }

    #[test]
    fn a_rule_error_on_the_declaring_directory_is_returned_with_its_text() {
        let (_dir, root) = workspace(&["lib/a.c"]);
        let walk = with_rules(&root, "lib", &[], Rules::failing_on("lib"), unbounded());
        let refusal = walk.find("*.c").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Runtime);
        assert_eq!(
            refusal.diagnostic("sources", None).as_str(),
            "buildl.sources: lib/.buildlignore:1: unusable"
        );
    }

    #[test]
    fn the_root_is_never_asked() {
        let (_dir, root) = workspace(&["a.c", "lib/b.c"]);
        let rules = Rules::default();
        let walk = with_rules(&root, "", &[], rules.clone(), unbounded());
        assert_eq!(walk.find("**/*.c").unwrap(), ["a.c", "lib/b.c"]);
        let asked = rules.asked();
        assert!(
            asked.contains(&("a.c".to_owned(), EntryKind::File)),
            "{asked:?}"
        );
        assert!(asked.iter().all(|(path, _)| !path.is_empty()), "{asked:?}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_non_utf8_entry_is_not_asked() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let (_dir, root) = workspace(&["lib/a.c"]);
        let folder = root.join("lib").join(OsStr::from_bytes(b"bad\xff"));
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("x.h"), "").unwrap();
        let rules = Rules::default();
        let walk = with_rules(&root, "lib", &[], rules.clone(), unbounded());
        assert_eq!(walk.find("*.c").unwrap(), ["a.c"]);
        assert!(
            rules
                .asked()
                .iter()
                .all(|(path, _)| path == "lib" || path == "lib/a.c"),
            "{:?}",
            rules.asked()
        );
    }
}
