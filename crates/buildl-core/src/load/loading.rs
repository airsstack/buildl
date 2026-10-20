//! Load's step: which directories to hand out next, and what the results add up to.
//!
//! Its own file because these decisions need no dispatcher: [`Loading`] is fed one result at a
//! time, in whatever order they arrive, and answers with the directories to evaluate next. That
//! keeps the order-independence of Load provable with plain values.
//!
//! Responsibilities: [`Loading`], its hand-out of every newly named directory as soon as a result
//! names it, and its replay of the serial walk over the results received so far.
//!
//! Non-responsibilities: evaluating a build file, which `evaluation` does; and running tasks,
//! which the [`Dispatcher`](crate::ports::Dispatcher) port does.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};

use super::evaluation::FileResult;
use super::traversal::{build_file, declaration_order};
use crate::error::{Error, Result};
use crate::types::{Declaration, Directory, EntryName, Provenance};

/// Load's step over a stream of results that arrive in any order.
///
/// Every directory a result names is handed out at once, so workers stay busy. Independently of
/// that, a replay walks the directories in the order a serial load would, reading each result when
/// the walk reaches it: the replay alone decides the declarations and the error, so neither depends
/// on arrival order. The first error the replay meets sets the stop flag, and every later result is
/// dropped.
#[derive(Debug)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`load` starts calling it in the next change")
)]
pub(crate) struct Loading<'a> {
    entry: EntryName,
    stop: &'a AtomicBool,
    /// Every directory handed out so far, the root included.
    dispatched: BTreeSet<Directory>,
    /// Directories whose result has been received and kept.
    arrived: BTreeSet<Directory>,
    /// Results the replay has not read yet.
    results: BTreeMap<Directory, FileResult>,
    replay: Replay,
}

/// The serial walk, reading results instead of evaluating files.
#[derive(Debug)]
struct Replay {
    /// Directories in the order a serial load would evaluate them, each with its requester.
    queue: VecDeque<(Directory, Option<Provenance>)>,
    seen: BTreeSet<Directory>,
    declarations: Vec<Declaration>,
    error: Option<Error>,
}

impl Replay {
    fn new() -> Self {
        Self {
            queue: VecDeque::from([(Directory::root(), None)]),
            seen: BTreeSet::from([Directory::root()]),
            declarations: Vec::new(),
            error: None,
        }
    }

    /// Reads results from the front of the queue until one is missing or an error is met.
    fn advance(&mut self, results: &mut BTreeMap<Directory, FileResult>, entry: &EntryName) {
        while self.error.is_none() {
            let Some(result) = self
                .queue
                .front()
                .and_then(|(next, _)| results.remove(next))
            else {
                return;
            };
            let Some((directory, requested_by)) = self.queue.pop_front() else {
                return;
            };
            match result {
                FileResult::Loaded {
                    subdirs,
                    declarations,
                } => {
                    let requester = build_file(&directory, entry).provenance().clone();
                    for next in subdirs {
                        if self.seen.insert(next.clone()) {
                            self.queue.push_back((next, Some(requester.clone())));
                        }
                    }
                    self.declarations.extend(declarations);
                }
                FileResult::Absent => {
                    self.error = Some(Error::MissingBuildFile {
                        directory,
                        requested_by,
                    });
                }
                FileResult::Failed(error) => self.error = Some(error),
                // A stopped file was never evaluated, so the walk cannot pass it.
                FileResult::Stopped => {
                    self.queue.push_front((directory, requested_by));
                    return;
                }
            }
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`load` starts calling it in the next change")
)]
impl<'a> Loading<'a> {
    /// A load of the workspace that reads `entry` in each directory and raises `stop` on its
    /// first error.
    pub(crate) fn new(entry: &EntryName, stop: &'a AtomicBool) -> Self {
        Self {
            entry: entry.clone(),
            stop,
            dispatched: BTreeSet::new(),
            arrived: BTreeSet::new(),
            results: BTreeMap::new(),
            replay: Replay::new(),
        }
    }

    /// The first directories to evaluate: the workspace root.
    pub(crate) fn begin(&mut self) -> Vec<Directory> {
        self.dispatched.insert(Directory::root());
        vec![Directory::root()]
    }

    /// Takes the result of evaluating `directory` and returns the directories to evaluate next.
    ///
    /// Returns nothing once the replay has met its error, then or earlier. A result that says the
    /// file was never evaluated is ignored.
    pub(crate) fn done(&mut self, directory: Directory, result: FileResult) -> Vec<Directory> {
        if self.replay.error.is_some() || matches!(result, FileResult::Stopped) {
            return Vec::new();
        }
        let next = match &result {
            FileResult::Loaded { subdirs, .. } => subdirs
                .iter()
                .filter(|subdir| self.dispatched.insert((*subdir).clone()))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        self.arrived.insert(directory.clone());
        self.results.insert(directory, result);
        self.replay.advance(&mut self.results, &self.entry);
        if self.replay.error.is_some() {
            self.stop.store(true, Ordering::Release);
            return Vec::new();
        }
        next
    }

    /// What the load came to: the declarations sorted, or the error serial load would give.
    ///
    /// # Errors
    ///
    /// Returns the error the replay met, or [`Error::LostBuildFiles`] naming every handed-out
    /// directory that never reported when the replay is still waiting for one.
    pub(crate) fn finish(self) -> Result<Vec<Declaration>> {
        let Self {
            dispatched,
            arrived,
            replay,
            ..
        } = self;
        if let Some(error) = replay.error {
            return Err(error);
        }
        if replay.queue.is_empty() {
            let mut declarations = replay.declarations;
            declarations.sort_by(declaration_order);
            return Ok(declarations);
        }
        let directories = dispatched
            .into_iter()
            .filter(|directory| !arrived.contains(directory))
            .collect();
        Err(Error::LostBuildFiles { directories })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::sync::atomic::{AtomicBool, Ordering};

    use super::Loading;
    use crate::error::{Error, EvaluationFailure};
    use crate::load::evaluation::FileResult;
    use crate::load::traversal::{build_file, convert};
    use crate::types::{
        Declaration, DeclarationOrder, Diagnostic, Directory, EntryName, StagedDeclaration,
        StagedItem, StagedSetting, Written,
    };

    /// What one directory's build file does in a test tree.
    #[derive(Clone, Copy)]
    enum Shape {
        /// Loads, requesting these directories.
        Loads(&'static [&'static str]),
        /// Holds no build file.
        Missing,
        /// Fails to evaluate.
        Fails,
    }

    type Tree = &'static [(&'static str, Shape)];

    fn entry() -> EntryName {
        EntryName::parse("build.lua").unwrap()
    }

    fn dir(raw: &str) -> Directory {
        Directory::parse(raw).unwrap()
    }

    fn dirs(raw: &[&str]) -> Vec<Directory> {
        raw.iter().map(|raw| dir(raw)).collect()
    }

    /// The one declaration a loading directory makes, named for the directory.
    fn declaration_in(directory: &Directory) -> Declaration {
        let name = format!("in_{}", directory.as_str().replace('/', "_"));
        let staged = StagedDeclaration {
            order: DeclarationOrder::new(0),
            item: StagedItem::Setting(StagedSetting {
                name: Written::new(name),
                default: Written::new(""),
            }),
        };
        convert(staged, build_file(directory, &entry()).provenance()).unwrap()
    }

    fn failure_in(directory: &Directory) -> Error {
        Error::Evaluation {
            provenance: build_file(directory, &entry()).provenance().clone(),
            failure: EvaluationFailure::Syntax,
            diagnostic: Diagnostic::new("broken"),
        }
    }

    fn loaded(subdirs: &[&str], directory: &Directory) -> FileResult {
        FileResult::Loaded {
            subdirs: dirs(subdirs),
            declarations: vec![declaration_in(directory)],
        }
    }

    fn result_of(tree: Tree, directory: &Directory) -> FileResult {
        let shape = tree
            .iter()
            .find(|(name, _)| *name == directory.as_str())
            .map(|(_, shape)| *shape)
            .unwrap();
        match shape {
            Shape::Loads(subdirs) => loaded(subdirs, directory),
            Shape::Missing => FileResult::Absent,
            Shape::Fails => FileResult::Failed(failure_in(directory)),
        }
    }

    /// Feeds `tree`'s results to `loading`, each arriving as soon as it has been handed out, in
    /// the order `order` lists the directories.
    fn feed(loading: &mut Loading<'_>, tree: Tree, order: &[Directory]) {
        let mut pending = loading.begin();
        while !pending.is_empty() {
            let next = order
                .iter()
                .find(|directory| pending.contains(directory))
                .unwrap()
                .clone();
            pending.retain(|directory| *directory != next);
            let result = result_of(tree, &next);
            pending.extend(loading.done(next, result));
        }
    }

    /// Every ordering of `items`.
    fn permutations(items: &[Directory]) -> Vec<Vec<Directory>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut all = Vec::new();
        for index in 0..items.len() {
            let mut rest = items.to_vec();
            let first = rest.remove(index);
            for mut tail in permutations(&rest) {
                tail.insert(0, first.clone());
                all.push(tail);
            }
        }
        all
    }

    fn names(tree: Tree) -> Vec<Directory> {
        tree.iter().map(|(name, _)| dir(name)).collect()
    }

    #[test]
    fn a_loaded_root_hands_out_every_subdir_at_once() {
        let stop = AtomicBool::new(false);
        let mut loading = Loading::new(&entry(), &stop);
        assert_eq!(loading.begin(), [Directory::root()]);
        let handed = loading.done(
            Directory::root(),
            loaded(&["a", "b", "c"], &Directory::root()),
        );
        assert_eq!(handed, dirs(&["a", "b", "c"]));
    }

    #[test]
    fn a_subdir_already_handed_out_is_not_handed_out_again() {
        let stop = AtomicBool::new(false);
        let mut loading = Loading::new(&entry(), &stop);
        loading.begin();
        assert_eq!(
            loading.done(Directory::root(), loaded(&["a", "b"], &Directory::root())),
            dirs(&["a", "b"])
        );
        assert_eq!(
            loading.done(dir("a"), loaded(&["c"], &dir("a"))),
            dirs(&["c"])
        );
        assert_eq!(loading.done(dir("b"), loaded(&["c"], &dir("b"))), []);
    }

    #[test]
    fn every_arrival_order_gives_serial_declarations() {
        const TREE: Tree = &[
            ("", Shape::Loads(&["a", "b"])),
            ("a", Shape::Loads(&["c"])),
            ("b", Shape::Loads(&["c", "d"])),
            ("c", Shape::Loads(&[])),
            ("d", Shape::Loads(&[])),
        ];
        let expected: Vec<Declaration> = names(TREE).iter().map(declaration_in).collect();
        for order in permutations(&names(TREE)) {
            let stop = AtomicBool::new(false);
            let mut loading = Loading::new(&entry(), &stop);
            feed(&mut loading, TREE, &order);
            assert_eq!(loading.finish().unwrap(), expected, "order {order:?}");
            assert!(!stop.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn every_arrival_order_gives_serial_error() {
        const TREE: Tree = &[
            ("", Shape::Loads(&["a", "b", "c"])),
            ("a", Shape::Loads(&["d"])),
            ("b", Shape::Fails),
            ("c", Shape::Fails),
            ("d", Shape::Missing),
        ];
        let expected = failure_in(&dir("b")).to_string();
        for order in permutations(&names(TREE)) {
            let stop = AtomicBool::new(false);
            let mut loading = Loading::new(&entry(), &stop);
            feed(&mut loading, TREE, &order);
            assert_eq!(
                loading.finish().unwrap_err().to_string(),
                expected,
                "order {order:?}"
            );
            assert!(stop.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn a_directory_named_by_two_parents_reports_serials_requester() {
        let stop = AtomicBool::new(false);
        let mut loading = Loading::new(&entry(), &stop);
        loading.begin();
        loading.done(Directory::root(), loaded(&["p", "q"], &Directory::root()));
        // `q` finishes first and names `m`; `m` is reported missing before `p` reports at all.
        assert_eq!(
            loading.done(dir("q"), loaded(&["m"], &dir("q"))),
            dirs(&["m"])
        );
        assert_eq!(loading.done(dir("m"), FileResult::Absent), []);
        assert!(!stop.load(Ordering::SeqCst));
        assert_eq!(loading.done(dir("p"), loaded(&["m"], &dir("p"))), []);
        match loading.finish().unwrap_err() {
            Error::MissingBuildFile {
                directory,
                requested_by,
            } => {
                assert_eq!(directory, dir("m"));
                assert_eq!(
                    requested_by.as_ref(),
                    Some(build_file(&dir("p"), &entry()).provenance())
                );
            }
            other => unreachable!("expected MissingBuildFile, got {other:?}"),
        }
    }

    #[test]
    fn the_error_sets_stop_and_later_results_hand_out_nothing() {
        let stop = AtomicBool::new(false);
        let mut loading = Loading::new(&entry(), &stop);
        loading.begin();
        let handed = loading.done(Directory::root(), loaded(&["a", "b"], &Directory::root()));
        assert_eq!(handed, dirs(&["a", "b"]));
        assert!(!stop.load(Ordering::SeqCst));
        assert_eq!(
            loading.done(dir("a"), FileResult::Failed(failure_in(&dir("a")))),
            []
        );
        assert!(stop.load(Ordering::SeqCst));
        assert_eq!(loading.done(dir("b"), loaded(&["c", "d"], &dir("b"))), []);
        assert_eq!(
            loading.finish().unwrap_err().to_string(),
            failure_in(&dir("a")).to_string()
        );
    }

    #[test]
    fn a_directory_without_a_result_is_lost() {
        let stop = AtomicBool::new(false);
        let mut loading = Loading::new(&entry(), &stop);
        loading.begin();
        loading.done(Directory::root(), loaded(&["a"], &Directory::root()));
        match loading.finish().unwrap_err() {
            Error::LostBuildFiles { directories } => assert_eq!(directories, dirs(&["a"])),
            other => unreachable!("expected LostBuildFiles, got {other:?}"),
        }
    }
}
