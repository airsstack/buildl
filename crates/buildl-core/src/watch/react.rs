//! What one batch of changed paths starts.
//!
//! Responsibilities: [`kept`], which holds the paths of a batch that are changes, and [`react`],
//! which sorts a batch into a build, a check of the build files, or nothing.
//!
//! Non-responsibilities: what kind of event named a path. A path created, written, renamed or
//! removed is the same change here.

use crate::ports::IgnoreRules;
use crate::types::{
    ChangeBatch, ChangedPath, Directory, EntryKind, Reaction, Reference, WatchLayout,
};

/// Whether `path` is `dir` or lies below it.
///
/// The workspace root holds every path. A shared prefix is not containment: `out` does not hold
/// `outer/x`.
fn under(path: &ChangedPath, dir: &Directory) -> bool {
    dir.is_root() || under_text(path.as_str(), dir.as_str())
}

/// Whether `path` is the directory `dir` or starts with it followed by `/`.
fn under_text(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The paths of `batch` that are changes to the workspace, or `None` when none is.
///
/// Paths under the output directory, the state directory or `.git` are not changes: the build
/// itself writes the first two, and none of the three holds a source. Nor is a path that `rules`
/// reports ignored, asked as [`EntryKind::Unknown`] because a changed path may be gone. Without
/// `rules`, nothing is dropped for being ignored, and a path whose question fails is kept.
#[must_use]
pub(super) fn kept<I: IgnoreRules>(
    batch: &ChangeBatch,
    layout: &WatchLayout,
    rules: Option<&I>,
) -> Option<ChangeBatch> {
    ChangeBatch::new(
        batch
            .iter()
            .filter(|path| {
                !(under(path, &layout.out)
                    || under(path, &layout.state)
                    || under_text(path.as_str(), ".git")
                    || is_ignored(rules, path))
            })
            .cloned()
            .collect(),
    )
}

/// Whether `rules` report `path` ignored. No rules, or a failing question, report nothing.
fn is_ignored<I: IgnoreRules>(rules: Option<&I>, path: &ChangedPath) -> bool {
    rules.is_some_and(|rules| {
        matches!(
            rules.ignored(path.as_str(), EntryKind::Unknown),
            Ok(Some(_))
        )
    })
}

/// What `batch` starts, given what the last build and the last open of the workspace read.
///
/// Only the paths left after the drop count: those under the output directory, the state
/// directory or `.git`, and those `rules` report ignored, are dropped. A batch with none left is
/// [`Reaction::Ignore`]. Otherwise it is [`Reaction::Build`] when the last cycle left no graph,
/// when a path kept is an input of the last build, or when it is one of the files that decide the
/// workspace's settings; any other batch is [`Reaction::Check`], for the build files may now
/// declare a different graph.
#[must_use]
pub fn react<I: IgnoreRules>(
    batch: &ChangeBatch,
    reference: &Reference,
    rules: Option<&I>,
) -> Reaction {
    let layout = &reference.layout;
    let Some(kept) = kept(batch, layout, rules) else {
        return Reaction::Ignore;
    };
    let Some(built) = &reference.built else {
        return Reaction::Build;
    };
    let starts_build = kept.iter().any(|path| {
        layout.config.contains(path)
            || built
                .inputs
                .iter()
                .any(|input| input.as_str() == path.as_str())
    });
    if starts_build {
        Reaction::Build
    } else {
        Reaction::Check
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{kept, react, under};
    use crate::error::{Error, Result};
    use crate::ports::IgnoreRules;
    use crate::types::{
        Built, ChangeBatch, ChangedPath, Directory, EntryKind, Ignoring, Reaction, Reference,
        SourcePath, TargetGraph, WatchLayout,
    };

    /// Rules ignoring the paths in `ignored`, or failing every question when `fails` is set, and
    /// remembering the kind of each question.
    #[derive(Default)]
    struct Rules {
        ignored: Vec<&'static str>,
        fails: bool,
        kinds: Mutex<Vec<EntryKind>>,
    }

    impl Rules {
        fn ignoring(ignored: &[&'static str]) -> Self {
            Self {
                ignored: ignored.to_vec(),
                ..Self::default()
            }
        }

        fn failing() -> Self {
            Self {
                fails: true,
                ..Self::default()
            }
        }

        fn kinds(&self) -> Vec<EntryKind> {
            self.kinds.lock().unwrap().clone()
        }
    }

    impl IgnoreRules for Rules {
        fn ignored(&self, path: &str, kind: EntryKind) -> Result<Option<Ignoring>> {
            self.kinds.lock().unwrap().push(kind);
            if self.fails {
                return Err(Error::IgnoreFile {
                    file: ".buildlignore".to_owned(),
                    line: Some(1),
                    reason: "unreadable".to_owned(),
                });
            }
            Ok(self
                .ignored
                .contains(&path)
                .then(|| Ignoring::new(".buildlignore", path)))
        }
    }

    fn path(raw: &str) -> ChangedPath {
        ChangedPath::parse(raw).unwrap()
    }

    fn batch(raws: &[&str]) -> ChangeBatch {
        ChangeBatch::new(raws.iter().map(|raw| path(raw)).collect()).unwrap()
    }

    fn reference(out: &str, inputs: &[&str], built: bool) -> Reference {
        Reference {
            layout: WatchLayout {
                out: Directory::parse(out).unwrap(),
                state: Directory::parse(".buildl").unwrap(),
                config: ["buildl.toml", "buildl.lock", "plugins/go/extension.toml"]
                    .iter()
                    .map(|raw| path(raw))
                    .collect(),
            },
            built: built.then(|| Built {
                graph: TargetGraph::new(Vec::new(), Vec::new(), BTreeMap::new(), BTreeMap::new()),
                inputs: inputs
                    .iter()
                    .map(|raw| SourcePath::parse(*raw).unwrap())
                    .collect(),
            }),
        }
    }

    #[test]
    fn under_holds_for_the_directory_and_below_it_only() {
        let out = Directory::parse("out").unwrap();
        assert!(under(&path("out/x.o"), &out));
        assert!(under(&path("out"), &out));
        assert!(!under(&path("outer/x"), &out));
        assert!(under(
            &path("anything/at/all"),
            &Directory::parse("").unwrap()
        ));
    }

    #[test]
    fn kept_holds_the_paths_outside_the_ignored_directories() {
        let reference = reference("out", &[], true);
        let changes = batch(&[
            "out/x.o",
            ".buildl/log.jsonl",
            ".git/index",
            "b.txt",
            "a.txt",
        ]);
        assert_eq!(
            kept(&changes, &reference.layout, None::<&Rules>),
            Some(batch(&["a.txt", "b.txt"]))
        );
    }

    #[test]
    fn kept_is_none_when_every_path_is_dropped() {
        let reference = reference("out", &[], true);
        let changes = batch(&["out/x.o", ".buildl/log.jsonl", ".git/index"]);
        assert_eq!(kept(&changes, &reference.layout, None::<&Rules>), None);
    }

    #[test]
    fn only_ignored_directories_is_ignore() {
        let reference = reference("out", &[], true);
        let changes = batch(&["out/x.o", ".buildl/log.jsonl", ".git/index"]);
        assert_eq!(
            react(&changes, &reference, None::<&Rules>),
            Reaction::Ignore
        );
    }

    #[test]
    fn a_changed_out_directory_moves_the_drop() {
        let reference = reference("build", &[], true);
        assert_eq!(
            react(&batch(&["build/x.o"]), &reference, None::<&Rules>),
            Reaction::Ignore
        );
        assert_eq!(
            react(&batch(&["out/x.o"]), &reference, None::<&Rules>),
            Reaction::Check
        );
    }

    #[test]
    fn no_graph_builds() {
        let reference = reference("out", &[], false);
        assert_eq!(
            react(&batch(&["README.md"]), &reference, None::<&Rules>),
            Reaction::Build
        );
    }

    #[test]
    fn an_input_builds() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        assert_eq!(
            react(&batch(&["lib/words/a.txt"]), &reference, None::<&Rules>),
            Reaction::Build
        );
    }

    #[test]
    fn a_config_file_builds() {
        let reference = reference("out", &[], true);
        for raw in ["buildl.toml", "buildl.lock", "plugins/go/extension.toml"] {
            assert_eq!(
                react(&batch(&[raw]), &reference, None::<&Rules>),
                Reaction::Build,
                "{raw}"
            );
        }
    }

    #[test]
    fn anything_else_checks() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        for raw in ["README.md", ".#a.c"] {
            assert_eq!(
                react(&batch(&[raw]), &reference, None::<&Rules>),
                Reaction::Check,
                "{raw}"
            );
        }
    }

    #[test]
    fn an_input_beside_ignored_paths_still_builds() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        let changes = batch(&["out/x.o", "lib/words/a.txt"]);
        assert_eq!(react(&changes, &reference, None::<&Rules>), Reaction::Build);
    }

    #[test]
    fn an_ignored_path_is_dropped() {
        let reference = reference("out", &[], true);
        let rules = Rules::ignoring(&["notes/a.md"]);
        let changes = batch(&["notes/a.md", "src/a.c"]);
        assert_eq!(
            kept(&changes, &reference.layout, Some(&rules)),
            Some(batch(&["src/a.c"]))
        );
    }

    #[test]
    fn a_batch_of_only_ignored_paths_is_no_change() {
        let reference = reference("out", &[], true);
        let rules = Rules::ignoring(&["notes/a.md"]);
        let changes = batch(&["notes/a.md"]);
        assert_eq!(kept(&changes, &reference.layout, Some(&rules)), None);
        assert_eq!(react(&changes, &reference, Some(&rules)), Reaction::Ignore);
    }

    #[test]
    fn without_rules_nothing_is_dropped_by_rules() {
        let reference = reference("out", &[], true);
        let changes = batch(&["notes/a.md"]);
        assert_eq!(
            kept(&changes, &reference.layout, None::<&Rules>),
            Some(batch(&["notes/a.md"]))
        );
    }

    #[test]
    fn a_rule_error_keeps_the_path() {
        let reference = reference("out", &[], true);
        let rules = Rules::failing();
        let changes = batch(&["notes/a.md"]);
        assert_eq!(
            kept(&changes, &reference.layout, Some(&rules)),
            Some(batch(&["notes/a.md"]))
        );
    }

    #[test]
    fn paths_are_asked_as_unknown() {
        let reference = reference("out", &[], true);
        let rules = Rules::ignoring(&["notes/a.md"]);
        let changes = batch(&["notes/a.md", "src/a.c"]);
        let _ = kept(&changes, &reference.layout, Some(&rules));
        assert_eq!(rules.kinds(), [EntryKind::Unknown, EntryKind::Unknown]);
    }

    #[test]
    fn the_built_in_drops_are_not_asked_about() {
        let reference = reference("out", &[], true);
        let rules = Rules::ignoring(&[]);
        let changes = batch(&["out/x.o", ".git/index", "src/a.c"]);
        let _ = kept(&changes, &reference.layout, Some(&rules));
        assert_eq!(rules.kinds(), [EntryKind::Unknown]);
    }
}
