//! The gc command: drop the cache rows of targets no longer declared, then remove the stored
//! blobs nothing still reads.
//!
//! Its own file because it is a function over six ports, the build file source, the dispatcher
//! that runs its evaluations, the action cache, the content store, the store's sweep and the
//! build log's history, that sits beside the [`Pipeline`](super::Pipeline) driver and builds
//! nothing: it loads once, as `build` does, and changes only what the cache and the store hold.
//!
//! Responsibilities: [`Collected`] and [`gc`](fn@gc).
//!
//! Non-responsibilities: reading the build file, which [`load`](fn@crate::load) does; deciding
//! which blobs a build log names, which the `history` module does; and reporting the counts,
//! which the caller does.

use std::collections::BTreeSet;

use crate::error::Result;
use crate::history::logged_outputs;
use crate::load::load;
use crate::ports::{ActionCache, ContentStore, DeclarationSource, Dispatcher, RunHistory, Sweep};
use crate::resolve::resolve;
use crate::types::{CacheRow, Digest, EntryName, Label, OutputManifest};

/// What a [`gc`](fn@gc) removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collected {
    /// The cache rows forgotten because their target is no longer declared.
    pub rows_dropped: usize,
    /// The digests whose blob and twin were removed from the store.
    pub blobs_removed: usize,
    /// The bytes those blobs and twins took, as the sweep reports them.
    pub bytes_removed: u64,
}

/// Drops the cache rows of targets the build files no longer declare, then removes every blob in
/// the store that no remaining row and no logged run still reads.
///
/// In order:
///
/// 1. Loads the workspace once through `source`, run by `dispatcher`, and resolves it into the
///    target graph.
/// 2. Forgets each row of `cache` whose label is not a target of that graph.
/// 3. Marks what is still read: for each remaining row, its output manifests, the files each
///    manifest names, and its stdout and stderr; and what the build log names, through `history`.
///    A manifest that `store` does not hold, or whose bytes are not a manifest, is marked by its
///    own digest alone.
/// 4. Removes, through `sweep`, every digest it holds that is not marked.
///
/// Rows are forgotten before any blob is removed, so a row that survives never names a removed
/// blob: a `gc` that stops part way leaves blobs behind, never a row without its blobs.
///
/// # Errors
///
/// Returns whatever the load returns, then whatever [`resolve`](fn@crate::resolve) returns, then
/// the first port failure of steps 2 and 3, then a failure of `sweep.held()` in step 4, all
/// before any blob is removed. A failure while removing returns at once; what was already removed stays removed.
pub fn gc<S, D, A, C, W, H>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    cache: &mut A,
    store: &C,
    sweep: &W,
    history: &H,
) -> Result<Collected>
where
    S: DeclarationSource,
    D: Dispatcher,
    A: ActionCache,
    C: ContentStore,
    W: Sweep,
    H: RunHistory,
{
    let graph = resolve(load(source, dispatcher, entry)?)?;
    let declared: BTreeSet<&Label> = graph.nodes().iter().map(|node| &node.label).collect();
    let mut rows_dropped = 0;
    for label in cache.labels() {
        if !declared.contains(&label) {
            cache.forget(&label)?;
            rows_dropped += 1;
        }
    }
    let mut marked = logged_outputs(&history.lines()?);
    for label in cache.labels() {
        if let Some(row) = cache.row(&label) {
            mark_row(store, row, &mut marked)?;
        }
    }
    let mut blobs_removed = 0;
    let mut bytes_removed = 0;
    for digest in sweep.held()?.difference(&marked) {
        bytes_removed += sweep.remove(digest)?;
        blobs_removed += 1;
    }
    Ok(Collected {
        rows_dropped,
        blobs_removed,
        bytes_removed,
    })
}

/// Adds to `marked` every digest `row` still reads: its two streams, each output's manifest, and
/// the files each manifest names.
fn mark_row<C: ContentStore>(
    store: &C,
    row: &CacheRow,
    marked: &mut BTreeSet<Digest>,
) -> Result<()> {
    marked.insert(row.stdout());
    marked.insert(row.stderr());
    for manifest in row.outputs() {
        marked.insert(*manifest);
        let held = store
            .get(manifest)?
            .and_then(|bytes| OutputManifest::from_bytes(&bytes));
        match held {
            Some(OutputManifest::File(file)) => {
                marked.insert(file.digest);
            }
            Some(OutputManifest::Tree(tree)) => {
                marked.extend(tree.entries().iter().map(|entry| entry.digest));
            }
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::rc::Rc;

    use super::{Collected, gc};
    use crate::error::{Error, EvaluationFailure, PortName, Result};
    use crate::ports::{
        ActionCache, ContentStore, DeclarationSource, Dispatcher, RunHistory, Sweep,
    };
    use crate::types::{
        ActionKey, Argument, BuildFile, CacheRow, Command, DeclarationOrder, Diagnostic, Digest,
        EntryName, Evaluated, FileMode, Freshness, KeyComponents, Label, LogLine, LoggedRun,
        NetworkAccess, Origin, OutcomeClass, OutputManifest, OutputName, OutputPath, OwnComponents,
        Platform, Provenance, StagedDeclaration, StagedFile, StagedItem, StagedTarget, StoredFile,
        TargetName, TargetRole, Timestamp, Tree, TreeEntry, TreePath, Turn, Written,
    };

    /// The calls the cache and the sweep received, in order, shared by both.
    type Journal = Rc<RefCell<Vec<&'static str>>>;

    fn adapter(port: PortName) -> Error {
        Error::Adapter {
            port,
            diagnostic: Diagnostic::new("fake failure"),
        }
    }

    /// A dispatcher that runs each task on the calling thread, oldest first. A load tells nothing
    /// while it works, so a message is dropped.
    struct Inline;

    impl Dispatcher for Inline {
        fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
        where
            T: Send,
            M: Send,
            R: Send,
            W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
            S: FnMut(Turn<M, R>) -> Vec<T>,
        {
            let mut waiting: VecDeque<T> = step(Turn::Begin).into();
            while let Some(task) = waiting.pop_front() {
                let result = work(task, &|_message| {});
                waiting.extend(step(Turn::Done(result)));
            }
            Ok(())
        }
    }

    /// A source whose root build file declares the named build targets; `None` fails to load.
    struct Source(Option<Vec<&'static str>>);

    impl DeclarationSource for Source {
        fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
            let names = self.0.as_ref().ok_or_else(|| Error::Evaluation {
                provenance: Provenance::new(file.file().to_path_buf(), file.directory().clone()),
                failure: EvaluationFailure::Runtime,
                diagnostic: Diagnostic::new("fake failure"),
            })?;
            let declarations = names
                .iter()
                .zip(0..)
                .map(|(name, order)| StagedDeclaration {
                    order: DeclarationOrder::new(order),
                    origin: Origin::BuildFile,
                    item: StagedItem::Target(StagedTarget {
                        name: Written::<TargetName>::new(*name),
                        role: TargetRole::Build,
                        rule: None,
                        run: Some(vec![Written::<Argument>::new("cc")]),
                        inputs: Vec::new(),
                        deps: Vec::new(),
                        outputs: None,
                        env: Vec::new(),
                        network: NetworkAccess::Sealed,
                        freshness: Freshness::Cached,
                    }),
                })
                .collect();
            Ok(Evaluated::Staged(StagedFile {
                declarations,
                subdirs: Vec::new(),
            }))
        }
    }

    /// A cache of rows by label; `forget` fails when `broken`.
    struct Cache {
        rows: BTreeMap<Label, CacheRow>,
        forgotten: Vec<Label>,
        journal: Journal,
        broken: bool,
    }

    impl ActionCache for Cache {
        fn row(&self, label: &Label) -> Option<&CacheRow> {
            self.rows.get(label)
        }

        fn labels(&self) -> Vec<Label> {
            self.rows.keys().cloned().collect()
        }

        fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
            self.rows.insert(label, row);
            Ok(())
        }

        fn forget(&mut self, label: &Label) -> Result<()> {
            if self.broken {
                return Err(adapter(PortName::ActionCache));
            }
            self.journal.borrow_mut().push("forget");
            self.rows.remove(label);
            self.forgotten.push(label.clone());
            Ok(())
        }
    }

    /// A store of blobs by digest; `get` fails when `broken`.
    struct Store {
        blobs: BTreeMap<Digest, Vec<u8>>,
        broken: bool,
    }

    impl ContentStore for Store {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            Ok(Digest::of(bytes))
        }

        fn contains(&self, digest: &Digest) -> bool {
            self.blobs.contains_key(digest)
        }

        fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            if self.broken {
                return Err(adapter(PortName::ContentStore));
            }
            Ok(self.blobs.get(digest).cloned())
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Ok(())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Ok(())
        }
    }

    /// A sweep holding digests of known sizes, recording what it removes.
    struct Held {
        sizes: BTreeMap<Digest, u64>,
        removed: RefCell<Vec<Digest>>,
        journal: Journal,
    }

    impl Sweep for Held {
        fn held(&self) -> Result<BTreeSet<Digest>> {
            Ok(self.sizes.keys().copied().collect())
        }

        fn remove(&self, digest: &Digest) -> Result<u64> {
            self.journal.borrow_mut().push("remove");
            self.removed.borrow_mut().push(*digest);
            Ok(self.sizes.get(digest).copied().unwrap_or(0))
        }
    }

    /// A build log of lines; `lines` fails when `broken`.
    struct History {
        lines: Vec<LogLine>,
        broken: bool,
    }

    impl RunHistory for History {
        fn lines(&self) -> Result<Vec<LogLine>> {
            if self.broken {
                return Err(adapter(PortName::EventLog));
            }
            Ok(self.lines.clone())
        }
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn digest(text: &str) -> Digest {
        Digest::of(text.as_bytes())
    }

    fn file_manifest(content: &str) -> OutputManifest {
        OutputManifest::File(StoredFile {
            digest: digest(content),
            mode: FileMode::Plain,
        })
    }

    fn tree_manifest(contents: &[&str]) -> OutputManifest {
        let entries = contents
            .iter()
            .map(|content| TreeEntry {
                path: TreePath::parse(*content).unwrap(),
                digest: digest(content),
                mode: FileMode::Plain,
            })
            .collect();
        OutputManifest::Tree(Tree::new(entries).unwrap())
    }

    fn manifest_digest(manifest: &OutputManifest) -> Digest {
        Digest::of(&manifest.to_bytes().unwrap())
    }

    /// A row with one output per `outputs` digest, printing `out` and `err`.
    fn row(outputs: Vec<Digest>) -> CacheRow {
        let names = (0..outputs.len())
            .map(|index| OutputName::parse(format!("o{index}")).unwrap())
            .collect();
        let key = ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                inputs: Vec::new(),
                outputs: names,
                env: BTreeMap::new(),
                tool: None,
                settings: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap();
        CacheRow::new(key, outputs, digest("out"), digest("err")).unwrap()
    }

    /// The whole fixture: ports sharing one journal, run through `gc`.
    struct Rig {
        source: Source,
        cache: Cache,
        store: Store,
        sweep: Held,
        history: History,
        journal: Journal,
    }

    impl Rig {
        /// A rig whose build file declares `declared`, whose cache holds `rows`, whose store
        /// holds `blobs`, and whose sweep holds `held` with the given sizes.
        fn new(
            declared: &[&'static str],
            rows: Vec<(&str, CacheRow)>,
            blobs: Vec<(Digest, Vec<u8>)>,
            held: Vec<(Digest, u64)>,
        ) -> Self {
            let journal = Journal::default();
            Self {
                source: Source(Some(declared.to_vec())),
                cache: Cache {
                    rows: rows
                        .into_iter()
                        .map(|(raw, row)| (label(raw), row))
                        .collect(),
                    forgotten: Vec::new(),
                    journal: Rc::clone(&journal),
                    broken: false,
                },
                store: Store {
                    blobs: blobs.into_iter().collect(),
                    broken: false,
                },
                sweep: Held {
                    sizes: held.into_iter().collect(),
                    removed: RefCell::new(Vec::new()),
                    journal: Rc::clone(&journal),
                },
                history: History {
                    lines: Vec::new(),
                    broken: false,
                },
                journal,
            }
        }

        fn run(&mut self) -> Result<Collected> {
            gc(
                &self.source,
                &mut Inline,
                &EntryName::parse("build.lua").unwrap(),
                &mut self.cache,
                &self.store,
                &self.sweep,
                &self.history,
            )
        }

        fn removed(&self) -> BTreeSet<Digest> {
            self.sweep.removed.borrow().iter().copied().collect()
        }
    }

    /// A row of one plain-file output `content`.
    fn file_row(content: &str) -> CacheRow {
        row(vec![manifest_digest(&file_manifest(content))])
    }

    #[test]
    fn undeclared_rows_are_forgotten_declared_kept() {
        let mut rig = Rig::new(
            &["a"],
            vec![("//:a", file_row("a")), ("//:gone", file_row("gone"))],
            vec![],
            vec![],
        );
        let collected = rig.run().unwrap();
        assert_eq!(rig.cache.forgotten, [label("//:gone")]);
        assert_eq!(rig.cache.labels(), [label("//:a")]);
        assert_eq!(collected.rows_dropped, 1);
    }

    #[test]
    fn every_forget_comes_before_the_first_remove() {
        let mut rig = Rig::new(
            &["a"],
            vec![
                ("//:gone-a", file_row("gone-a")),
                ("//:gone-b", file_row("gone-b")),
            ],
            vec![],
            vec![(digest("x"), 1), (digest("y"), 1)],
        );
        rig.run().unwrap();
        let journal = rig.journal.borrow();
        assert_eq!(*journal, ["forget", "forget", "remove", "remove"]);
    }

    #[test]
    fn a_kept_rows_manifest_files_and_output_streams_are_kept() {
        let manifest = tree_manifest(&["one", "two"]);
        let at = manifest_digest(&manifest);
        let kept = [
            at,
            digest("one"),
            digest("two"),
            digest("out"),
            digest("err"),
        ];
        let unrelated = digest("unrelated");
        let mut held: Vec<(Digest, u64)> = kept.iter().map(|d| (*d, 1)).collect();
        held.push((unrelated, 1));
        let mut rig = Rig::new(
            &["a"],
            vec![("//:a", row(vec![at]))],
            vec![(at, manifest.to_bytes().unwrap())],
            held,
        );
        rig.run().unwrap();
        assert_eq!(rig.removed(), BTreeSet::from([unrelated]));
    }

    #[test]
    fn log_only_digests_are_kept() {
        let line = LogLine::Event(LoggedRun {
            at: Timestamp::from_unix_nanos(1),
            class: OutcomeClass::Built,
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            diagnostic: None,
            file: "build.lua".to_owned(),
            finished: None,
            key: None,
            label: label("//:a"),
            outputs: Vec::new(),
            started: None,
            stderr: Some(digest("logged-err")),
            stdout: Some(digest("logged-out")),
        });
        let mut rig = Rig::new(
            &["a"],
            vec![],
            vec![],
            vec![
                (digest("logged-out"), 1),
                (digest("logged-err"), 1),
                (digest("unlogged"), 1),
            ],
        );
        rig.history = History {
            lines: vec![line],
            broken: false,
        };
        rig.run().unwrap();
        assert_eq!(rig.removed(), BTreeSet::from([digest("unlogged")]));
    }

    #[test]
    fn a_missing_or_foreign_manifest_keeps_the_row_and_marks_only_itself() {
        let missing = digest("not-in-the-store");
        let foreign = digest("foreign-manifest");
        let file_only = digest("file-those-would-not-name");
        let mut rig = Rig::new(
            &["a", "b"],
            vec![("//:a", row(vec![missing])), ("//:b", row(vec![foreign]))],
            vec![(foreign, b"not a manifest".to_vec())],
            vec![
                (missing, 1),
                (foreign, 1),
                (file_only, 1),
                (digest("out"), 1),
                (digest("err"), 1),
            ],
        );
        let collected = rig.run().unwrap();
        assert_eq!(rig.cache.labels(), [label("//:a"), label("//:b")]);
        assert_eq!(rig.removed(), BTreeSet::from([file_only]));
        assert_eq!(collected.rows_dropped, 0);
    }

    #[test]
    fn a_load_error_forgets_and_removes_nothing() {
        let mut rig = Rig::new(
            &[],
            vec![("//:gone", file_row("gone"))],
            vec![],
            vec![(digest("x"), 1)],
        );
        rig.source = Source(None);
        let err = rig.run().unwrap_err();
        assert!(matches!(
            err,
            Error::Evaluation {
                failure: EvaluationFailure::Runtime,
                ..
            }
        ));
        assert!(rig.journal.borrow().is_empty());
    }

    #[test]
    fn a_store_error_while_marking_removes_nothing() {
        let mut rig = Rig::new(
            &["a"],
            vec![("//:a", file_row("a"))],
            vec![],
            vec![(digest("x"), 1)],
        );
        rig.store.broken = true;
        let err = rig.run().unwrap_err();
        assert!(matches!(
            err,
            Error::Adapter {
                port: PortName::ContentStore,
                ..
            }
        ));
        assert!(!rig.journal.borrow().contains(&"remove"));
    }

    #[test]
    fn a_history_error_removes_nothing() {
        let mut rig = Rig::new(
            &["a"],
            vec![("//:a", file_row("a"))],
            vec![],
            vec![(digest("x"), 1)],
        );
        rig.history.broken = true;
        let err = rig.run().unwrap_err();
        assert!(matches!(
            err,
            Error::Adapter {
                port: PortName::EventLog,
                ..
            }
        ));
        assert!(!rig.journal.borrow().contains(&"remove"));
    }

    #[test]
    fn a_forget_error_removes_nothing() {
        let mut rig = Rig::new(
            &["a"],
            vec![("//:gone", file_row("gone"))],
            vec![],
            vec![(digest("x"), 1)],
        );
        rig.cache.broken = true;
        let err = rig.run().unwrap_err();
        assert!(matches!(
            err,
            Error::Adapter {
                port: PortName::ActionCache,
                ..
            }
        ));
        assert!(!rig.journal.borrow().contains(&"remove"));
    }

    #[test]
    fn counts_and_bytes_are_summed() {
        let mut rig = Rig::new(
            &["a"],
            vec![],
            vec![],
            vec![(digest("x"), 1), (digest("y"), 2), (digest("z"), 4)],
        );
        assert_eq!(
            rig.run().unwrap(),
            Collected {
                rows_dropped: 0,
                blobs_removed: 3,
                bytes_removed: 7,
            }
        );
    }
}
