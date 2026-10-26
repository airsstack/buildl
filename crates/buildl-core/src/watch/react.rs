//! What one batch of changed paths starts.
//!
//! Responsibilities: [`react`], which sorts a batch into a build, a check of the build files, or
//! nothing.
//!
//! Non-responsibilities: what kind of event named a path. A path created, written, renamed or
//! removed is the same change here.

use crate::types::{ChangeBatch, ChangedPath, Directory, Reaction, Reference};

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

/// What `batch` starts, given what the last build and the last open of the workspace read.
///
/// Paths under the output directory, the state directory or `.git` are dropped first: the build
/// itself writes the first two, and none of the three holds a source. A batch with nothing left
/// is [`Reaction::Ignore`]. Otherwise it is [`Reaction::Build`] when the last cycle left no
/// graph, when a path left is an input of the last build, or when it is one of the files that
/// decide the workspace's settings; any other batch is [`Reaction::Check`], for the build files may now
/// declare a different graph.
#[must_use]
pub fn react(batch: &ChangeBatch, reference: &Reference) -> Reaction {
    let layout = &reference.layout;
    let mut kept = batch
        .iter()
        .filter(|path| {
            !(under(path, &layout.out)
                || under(path, &layout.state)
                || under_text(path.as_str(), ".git"))
        })
        .peekable();
    if kept.peek().is_none() {
        return Reaction::Ignore;
    }
    let Some(built) = &reference.built else {
        return Reaction::Build;
    };
    let starts_build = kept.any(|path| {
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

    use super::{react, under};
    use crate::types::{
        Built, ChangeBatch, ChangedPath, Directory, Reaction, Reference, SourcePath, TargetGraph,
        WatchLayout,
    };

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
    fn only_ignored_directories_is_ignore() {
        let reference = reference("out", &[], true);
        let changes = batch(&["out/x.o", ".buildl/log.jsonl", ".git/index"]);
        assert_eq!(react(&changes, &reference), Reaction::Ignore);
    }

    #[test]
    fn a_changed_out_directory_moves_the_drop() {
        let reference = reference("build", &[], true);
        assert_eq!(react(&batch(&["build/x.o"]), &reference), Reaction::Ignore);
        assert_eq!(react(&batch(&["out/x.o"]), &reference), Reaction::Check);
    }

    #[test]
    fn no_graph_builds() {
        let reference = reference("out", &[], false);
        assert_eq!(react(&batch(&["README.md"]), &reference), Reaction::Build);
    }

    #[test]
    fn an_input_builds() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        assert_eq!(
            react(&batch(&["lib/words/a.txt"]), &reference),
            Reaction::Build
        );
    }

    #[test]
    fn a_config_file_builds() {
        let reference = reference("out", &[], true);
        for raw in ["buildl.toml", "buildl.lock", "plugins/go/extension.toml"] {
            assert_eq!(react(&batch(&[raw]), &reference), Reaction::Build, "{raw}");
        }
    }

    #[test]
    fn anything_else_checks() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        for raw in ["README.md", ".#a.c"] {
            assert_eq!(react(&batch(&[raw]), &reference), Reaction::Check, "{raw}");
        }
    }

    #[test]
    fn an_input_beside_ignored_paths_still_builds() {
        let reference = reference("out", &["lib/words/a.txt"], true);
        let changes = batch(&["out/x.o", "lib/words/a.txt"]);
        assert_eq!(react(&changes, &reference), Reaction::Build);
    }
}
