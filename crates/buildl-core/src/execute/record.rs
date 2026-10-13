//! Recording: the row, the outcome and the log event of one target, built from values.
//!
//! Its own file because recording is the last step of building a target and the one a later
//! build depends on: the row is what the next build compares against, the outcome is what the
//! build hands back, and the event is what the log keeps. Each is put together here in one
//! place, so that what a class of outcome carries is decided once.
//!
//! Responsibilities: [`row`]; the outcomes [`built`], [`cached`], [`failed`], [`unstored`],
//! [`unplaced`], [`mismatched`] and [`skipped`]; [`event`] with [`Progress`]; and the three
//! steps that reach a port, in the order that keeps a crash harmless: [`store`] puts a built
//! target's outputs in place and only then records its row, [`restore`] puts a cached
//! target's outputs in place, and [`append`] logs an outcome whatever it is. An output that
//! cannot be put in place whole is removed from its path again.
//!
//! Non-responsibilities: how content is put in place, how a row is kept and how the log is
//! written, which are the ports'.

use crate::error::{Error, Result};
use crate::execute::perform::{Kept, NOT_STORED, Printed, Span};
use crate::ports::{ActionCache, Clock, ContentStore, EventLog};
use crate::types::{
    ActionKey, ActionOutcome, CacheRow, Command, Diagnostic, Digest, Label, OutcomeClass,
    OutcomeEvent, OutputManifest, OutputPath, Provenance, Timestamp,
};

/// How far the build of one target got, which decides what its event can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Progress {
    /// A worker began its action.
    Began {
        /// The digest of the key the action ran under.
        key: Digest,
        /// When the worker began and when it was done.
        span: Span,
    },
    /// Its key was finished and no worker began it: what its last build recorded still stood.
    Keyed {
        /// The digest of that key.
        key: Digest,
    },
    /// It was skipped. A skipped target is logged without a key, whether or not one had been
    /// finished for it.
    Skipped,
}

/// The row to record for an action that ran under `key`, left outputs whose manifests have the
/// digests `outputs`, and printed `printed`; `None` when `outputs` does not hold one digest per
/// output the key declares.
pub(crate) fn row(key: ActionKey, outputs: Vec<Digest>, printed: Printed) -> Option<CacheRow> {
    CacheRow::new(key, outputs, printed.stdout, printed.stderr)
}

/// The outcome of a target that ran and whose `row` was recorded.
pub(crate) fn built(label: Label, row: &CacheRow) -> ActionOutcome {
    succeeded(label, OutcomeClass::Built, row)
}

/// The outcome of a target that did not run: everything it reports is what `row`, recorded by
/// its last build, holds.
pub(crate) fn cached(label: Label, row: &CacheRow) -> ActionOutcome {
    succeeded(label, OutcomeClass::Cached, row)
}

/// The outcome of a target whose action did not get as far as producing its outputs: `class`
/// and `diagnostic` as the worker decided them, and what the action printed when that was
/// stored.
pub(crate) fn failed(
    label: Label,
    class: OutcomeClass,
    diagnostic: Option<Diagnostic>,
    printed: Option<Printed>,
) -> ActionOutcome {
    ActionOutcome {
        label,
        class,
        diagnostic,
        outputs: Vec::new(),
        stdout: printed.map(|printed| printed.stdout),
        stderr: printed.map(|printed| printed.stderr),
    }
}

/// The outcome of a target that ran and produced its outputs, when a port then failed with
/// `error` while an output was put in place or the row was stored.
pub(crate) fn unstored(label: Label, error: &Error, printed: Printed) -> ActionOutcome {
    failed(
        label,
        NOT_STORED,
        Some(Diagnostic::new(error.to_string())),
        Some(printed),
    )
}

/// The outcome of a target that did not need to run, when a port failed with `error` while one
/// of its outputs was put in place. What it printed is taken from `row`.
pub(crate) fn unplaced(label: Label, error: &Error, row: &CacheRow) -> ActionOutcome {
    unstored(
        label,
        error,
        Printed {
            stdout: row.stdout(),
            stderr: row.stderr(),
        },
    )
}

/// The outcome of a target that ran, when no row could be made from its key and what it left.
pub(crate) fn mismatched(label: Label, printed: Printed) -> ActionOutcome {
    failed(
        label,
        NOT_STORED,
        Some(Diagnostic::new("the row does not match its key")),
        Some(printed),
    )
}

/// The outcome of a target that was never begun because the target `because` failed.
pub(crate) fn skipped(label: Label, because: Label) -> ActionOutcome {
    failed(label, OutcomeClass::Skipped { because }, None, None)
}

/// The log event for `outcome`, recorded at `at`, of a target declared in `provenance` whose
/// expanded command is `command`.
///
/// `progress` decides the rest: the key's digest is logged unless the target was skipped, and
/// the two instants only when a worker began the action.
pub(crate) fn event(
    at: Timestamp,
    provenance: &Provenance,
    command: &Command,
    progress: Progress,
    outcome: ActionOutcome,
) -> OutcomeEvent {
    let (key, span) = match progress {
        Progress::Began { key, span } => (Some(key), Some(span)),
        Progress::Keyed { key } => (Some(key), None),
        Progress::Skipped => (None, None),
    };
    OutcomeEvent {
        at,
        provenance: provenance.clone(),
        command: command.clone(),
        key,
        started: span.map(|span| span.started),
        finished: span.map(|span| span.finished),
        outcome,
    }
}

/// Records a target that ran under `key`, left `outputs` and printed `printed`, and returns
/// its outcome.
///
/// Each output is put at its path of `paths`, in the order the outputs were written, and only
/// then is the row stored: a row therefore exists only for content the store already holds.
/// A port that fails either step makes the outcome a failure to store, and no row is kept. An
/// output the store could not place whole is cleared from its path first.
pub(crate) fn store<A: ActionCache, C: ContentStore>(
    label: Label,
    key: ActionKey,
    outputs: Vec<Kept>,
    printed: Printed,
    paths: &[OutputPath],
    cas: &C,
    cache: &mut A,
) -> ActionOutcome {
    let (digests, manifests): (Vec<Digest>, Vec<OutputManifest>) = outputs
        .into_iter()
        .map(|kept| (kept.digest, kept.manifest))
        .unzip();
    let Some(row) = row(key, digests, printed) else {
        return mismatched(label, printed);
    };
    let outcome = built(label.clone(), &row);
    let stored = place(&manifests, paths, cas).and_then(|()| cache.record(label.clone(), row));
    match stored {
        Ok(()) => outcome,
        Err(error) => unstored(label, &error, printed),
    }
}

/// Puts the outputs of a target that need not run back in place, and returns its outcome:
/// `manifests` says what each output `row` recorded is. Nothing is written to the cache. A
/// store that fails makes the outcome a failure to store, and an output it could not place
/// whole is cleared from its path first.
pub(crate) fn restore<C: ContentStore>(
    label: Label,
    row: &CacheRow,
    manifests: &[OutputManifest],
    paths: &[OutputPath],
    cas: &C,
) -> ActionOutcome {
    match place(manifests, paths, cas) {
        Ok(()) => cached(label, row),
        Err(error) => unplaced(label, &error, row),
    }
}

/// Appends the event for `outcome` to `log`, reading `clock` once for when it was recorded.
///
/// # Errors
///
/// Returns whatever `log` returns.
pub(crate) fn append<L: EventLog, K: Clock>(
    log: &mut L,
    clock: &K,
    provenance: &Provenance,
    command: &Command,
    progress: Progress,
    outcome: ActionOutcome,
) -> Result<()> {
    log.append(&event(clock.now(), provenance, command, progress, outcome))
}

/// Puts each output of `manifests` at its path of `paths`, pairwise, in the order the outputs
/// were written.
///
/// Whatever is at an output's path is removed first, so that nothing a previous build left
/// there outlives it: a file where a directory now belongs, or a file of a tree that no longer
/// holds it. Then a file is placed at the path, and a tree file by file, in path order, each
/// below the path.
///
/// When the store fails to place a file of an output, that output's path is cleared again and
/// no later output is touched, so a failed placement leaves nothing of the output behind. The
/// outputs placed before it stay.
///
/// # Errors
///
/// Returns the store's first failure. When clearing a part-placed output away fails too, that
/// second failure is not reported: the part stays at the path, and the next placement there
/// clears it before it places anything.
fn place<C: ContentStore>(
    manifests: &[OutputManifest],
    paths: &[OutputPath],
    cas: &C,
) -> Result<()> {
    manifests.iter().zip(paths).try_for_each(|(manifest, at)| {
        cas.clear(at)?;
        fill(manifest, at, cas).inspect_err(|_| {
            // The placement failure is the one to report, whatever this second call returns.
            let _cleared = cas.clear(at);
        })
    })
}

/// Places the files of one output at and below `at`, which holds nothing, stopping at the
/// first the store fails on.
fn fill<C: ContentStore>(manifest: &OutputManifest, at: &OutputPath, cas: &C) -> Result<()> {
    match manifest {
        OutputManifest::File(file) => cas.place(&file.digest, file.mode, at),
        OutputManifest::Tree(tree) => tree
            .entries()
            .iter()
            .try_for_each(|entry| cas.place(&entry.digest, entry.mode, &at.entry(&entry.path))),
    }
}

/// The outcome of a target whose outputs are in place: `class`, with what `row` holds.
fn succeeded(label: Label, class: OutcomeClass, row: &CacheRow) -> ActionOutcome {
    ActionOutcome {
        label,
        class,
        diagnostic: None,
        outputs: row.produced(),
        stdout: Some(row.stdout()),
        stderr: Some(row.stderr()),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::sync::{Mutex, PoisonError};

    use super::{
        Progress, built, cached, event, failed, mismatched, place, row, skipped, unplaced, unstored,
    };
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{
        command, file, key, label, node, output_path, row as recorded, tree,
    };
    use crate::execute::perform::{Printed, Span};
    use crate::ports::ContentStore;
    use crate::types::{
        ActionOutcome, Diagnostic, Digest, FileMode, InfraKind, OutcomeClass, OutcomeEvent,
        OutputManifest, OutputName, OutputPath, StoredFile, Timestamp, Tree, TreeEntry, TreePath,
    };

    /// A store that writes down what it is asked to clear and to place, whether or not it
    /// does it.
    #[derive(Default)]
    struct Recording {
        asked: Mutex<Vec<String>>,
        /// The path it fails to place a file at.
        unplaceable: Option<OutputPath>,
        /// Whether it fails to clear a path once it was asked to place anything.
        keeps_what_it_placed: bool,
    }

    impl Recording {
        fn asked(&self) -> Vec<String> {
            self.asked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        /// Writes `what` down and returns whether anything was asked to be placed before it.
        fn ask(&self, what: String) -> bool {
            let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
            let placed = asked.iter().any(|earlier| earlier.starts_with("place "));
            asked.push(what);
            placed
        }
    }

    impl ContentStore for Recording {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            Ok(Digest::of(bytes))
        }

        fn contains(&self, _digest: &Digest) -> bool {
            true
        }

        fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }

        fn clear(&self, at: &OutputPath) -> Result<()> {
            if self.ask(format!("clear {at}")) && self.keeps_what_it_placed {
                return Err(store_error(&format!("{at} cannot be removed")));
            }
            Ok(())
        }

        fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
            let content = [("app", "app"), ("page", "page"), ("script", "script")]
                .into_iter()
                .find_map(|(name, bytes)| (Digest::of(bytes.as_bytes()) == *digest).then_some(name))
                .unwrap_or("?");
            self.ask(format!("place {content} {mode:?} {at}"));
            if self.unplaceable.as_ref() == Some(at) {
                return Err(store_error(&format!("{at} cannot be written")));
            }
            Ok(())
        }
    }

    fn printed() -> Printed {
        Printed {
            stdout: Digest::of(b"compiling\n"),
            stderr: Digest::of(b""),
        }
    }

    fn store_error(text: &str) -> Error {
        Error::Adapter {
            port: PortName::ContentStore,
            diagnostic: Diagnostic::new(text),
        }
    }

    /// The outcome of `//:app` with `class` and nothing else.
    fn bare(class: OutcomeClass) -> ActionOutcome {
        ActionOutcome {
            label: label("//:app"),
            class,
            diagnostic: None,
            outputs: Vec::new(),
            stdout: None,
            stderr: None,
        }
    }

    const STORE: OutcomeClass = OutcomeClass::InfraFailure {
        kind: InfraKind::Store,
    };

    #[test]
    fn a_row_holds_the_key_the_outputs_and_what_was_printed() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        let row = row(key(&["app", "app.map"]), vec![app, map], printed()).unwrap();
        assert_eq!(row.key(), &key(&["app", "app.map"]));
        assert_eq!(row.outputs(), [app, map]);
        assert_eq!(
            (row.stdout(), row.stderr()),
            (printed().stdout, printed().stderr)
        );
    }

    #[test]
    fn a_row_is_not_made_for_the_wrong_number_of_outputs() {
        let app = Digest::of(b"app");
        assert_eq!(row(key(&["app", "app.map"]), vec![app], printed()), None);
        assert_eq!(row(key(&["app"]), vec![app, app], printed()), None);
    }

    #[test]
    fn a_built_targets_outcome_is_what_its_row_holds() {
        let row = recorded(&["app", "app.map"]);
        assert_eq!(
            built(label("//:app"), &row),
            ActionOutcome {
                outputs: row.produced(),
                stdout: Some(row.stdout()),
                stderr: Some(row.stderr()),
                ..bare(OutcomeClass::Built)
            }
        );
    }

    #[test]
    fn a_cached_targets_outcome_is_taken_from_its_row() {
        let row = recorded(&["app", "app.map"]);
        assert_eq!(
            cached(label("//:app"), &row),
            ActionOutcome {
                outputs: row.produced(),
                stdout: Some(row.stdout()),
                stderr: Some(row.stderr()),
                ..bare(OutcomeClass::Cached)
            }
        );
    }

    #[test]
    fn a_failure_on_a_worker_keeps_its_class_and_has_no_outputs() {
        let missing = OutcomeClass::OutputMissing {
            name: OutputName::parse("app.map").unwrap(),
        };
        for class in [OutcomeClass::ActionFailure { status: 2 }, missing] {
            assert_eq!(
                failed(label("//:app"), class.clone(), None, Some(printed())),
                ActionOutcome {
                    stdout: Some(printed().stdout),
                    stderr: Some(printed().stderr),
                    ..bare(class)
                }
            );
        }

        let prepare = OutcomeClass::InfraFailure {
            kind: InfraKind::Prepare,
        };
        let message = Diagnostic::new("exec strategy: //:app: no directory to run in");
        assert_eq!(
            failed(
                label("//:app"),
                prepare.clone(),
                Some(message.clone()),
                None
            ),
            ActionOutcome {
                diagnostic: Some(message),
                ..bare(prepare)
            },
            "an action that never ran printed nothing"
        );
    }

    #[test]
    fn a_built_target_that_could_not_be_stored_is_the_builders_failure() {
        assert_eq!(
            unstored(
                label("//:app"),
                &store_error("out/app cannot be written"),
                printed()
            ),
            ActionOutcome {
                diagnostic: Some(Diagnostic::new("content store: out/app cannot be written")),
                stdout: Some(printed().stdout),
                stderr: Some(printed().stderr),
                ..bare(STORE)
            }
        );
    }

    #[test]
    fn a_cached_target_that_could_not_be_placed_reports_what_its_row_printed() {
        let row = recorded(&["app"]);
        assert_eq!(
            unplaced(
                label("//:app"),
                &store_error("out/app cannot be written"),
                &row
            ),
            ActionOutcome {
                diagnostic: Some(Diagnostic::new("content store: out/app cannot be written")),
                stdout: Some(row.stdout()),
                stderr: Some(row.stderr()),
                ..bare(STORE)
            }
        );
    }

    #[test]
    fn a_row_that_does_not_match_its_key_is_the_builders_failure() {
        assert_eq!(
            mismatched(label("//:app"), printed()),
            ActionOutcome {
                diagnostic: Some(Diagnostic::new("the row does not match its key")),
                stdout: Some(printed().stdout),
                stderr: Some(printed().stderr),
                ..bare(STORE)
            }
        );
    }

    #[test]
    fn a_file_is_placed_at_its_path_once_the_path_is_cleared() {
        let store = Recording::default();
        let app = OutputManifest::File(StoredFile {
            digest: Digest::of(b"app"),
            mode: FileMode::Executable,
        });
        let paths = [
            output_path("//:app", "app"),
            output_path("//:app", "app.html"),
        ];

        place(&[app, file("page")], &paths, &store).unwrap();

        assert_eq!(
            store.asked(),
            [
                "clear out/app",
                "place app Executable out/app",
                "clear out/app.html",
                "place page Plain out/app.html",
            ]
        );
    }

    #[test]
    fn a_tree_is_placed_file_by_file_below_its_path_in_path_order() {
        let store = Recording::default();
        let entry = |path: &str, content: &str, mode| TreeEntry {
            path: TreePath::parse(path).unwrap(),
            digest: Digest::of(content.as_bytes()),
            mode,
        };
        let site = OutputManifest::Tree(
            Tree::new(vec![
                entry("index.html", "page", FileMode::Plain),
                entry("bin/serve", "app", FileMode::Executable),
                entry("assets/app.js", "script", FileMode::Plain),
            ])
            .unwrap(),
        );

        place(&[site], &[output_path("//web:site", "site")], &store).unwrap();

        assert_eq!(
            store.asked(),
            [
                "clear out/web/site",
                "place script Plain out/web/site/assets/app.js",
                "place app Executable out/web/site/bin/serve",
                "place page Plain out/web/site/index.html",
            ]
        );
    }

    #[test]
    fn a_tree_that_cannot_be_placed_whole_is_cleared_and_nothing_after_it_is_placed() {
        let site_path = output_path("//:site", "site");
        let store = Recording {
            unplaceable: Some(site_path.entry(&TreePath::parse("b").unwrap())),
            ..Recording::default()
        };
        let site = tree(&[("a", "app"), ("b", "page"), ("c", "script")]);
        let paths = [site_path, output_path("//:site", "app")];

        let error = place(&[site, file("app")], &paths, &store).unwrap_err();

        assert_eq!(
            error.to_string(),
            "content store: out/site/b cannot be written"
        );
        assert_eq!(
            store.asked(),
            [
                "clear out/site",
                "place app Plain out/site/a",
                "place page Plain out/site/b",
                "clear out/site",
            ],
            "what was placed is removed, and nothing follows in this output or the next"
        );
    }

    #[test]
    fn a_file_that_cannot_be_placed_has_its_path_cleared() {
        let app = output_path("//:app", "app");
        let store = Recording {
            unplaceable: Some(app.clone()),
            ..Recording::default()
        };

        let error = place(&[file("app")], &[app], &store).unwrap_err();

        assert_eq!(
            error.to_string(),
            "content store: out/app cannot be written"
        );
        assert_eq!(
            store.asked(),
            ["clear out/app", "place app Plain out/app", "clear out/app"]
        );
    }

    #[test]
    fn an_output_placed_whole_stays_when_a_later_one_fails() {
        let app = output_path("//:app", "app");
        let page = output_path("//:app", "app.html");
        let store = Recording {
            unplaceable: Some(page.clone()),
            ..Recording::default()
        };

        place(&[file("app"), file("page")], &[app, page], &store).unwrap_err();

        assert_eq!(
            store.asked(),
            [
                "clear out/app",
                "place app Plain out/app",
                "clear out/app.html",
                "place page Plain out/app.html",
                "clear out/app.html",
            ]
        );
    }

    #[test]
    fn the_placement_failure_is_the_one_reported_when_clearing_fails_too() {
        let site_path = output_path("//:site", "site");
        let store = Recording {
            unplaceable: Some(site_path.entry(&TreePath::parse("b").unwrap())),
            keeps_what_it_placed: true,
            ..Recording::default()
        };
        let site = tree(&[("a", "app"), ("b", "page")]);

        let error = place(&[site], &[site_path], &store).unwrap_err();

        assert_eq!(
            error.to_string(),
            "content store: out/site/b cannot be written"
        );
        assert_eq!(
            store.asked().last().map(String::as_str),
            Some("clear out/site"),
            "the store was asked to remove what it placed"
        );
    }

    #[test]
    fn a_skipped_target_names_the_failure_and_nothing_else() {
        assert_eq!(
            skipped(label("//:app"), label("//lib:text")),
            bare(OutcomeClass::Skipped {
                because: label("//lib:text"),
            })
        );
    }

    #[test]
    fn an_event_says_how_far_the_target_got() {
        let at = Timestamp::from_unix_nanos(30);
        let node = node("//lib:text", |_| {});
        let command = command(&["cc", "-c", "lib/a.c", "-o", "out/lib/text"]);
        let key = Digest::of(b"key");
        let span = Span {
            started: Timestamp::from_unix_nanos(10),
            finished: Timestamp::from_unix_nanos(20),
        };
        let outcome = bare(OutcomeClass::Built);
        let logged = |progress| event(at, &node.provenance, &command, progress, outcome.clone());
        let expected = |key, started, finished| OutcomeEvent {
            at,
            provenance: node.provenance.clone(),
            command: command.clone(),
            key,
            started,
            finished,
            outcome: outcome.clone(),
        };

        assert_eq!(
            logged(Progress::Began { key, span }),
            expected(Some(key), Some(span.started), Some(span.finished)),
            "a target a worker began has its key and both instants"
        );
        assert_eq!(
            logged(Progress::Keyed { key }),
            expected(Some(key), None, None),
            "a target no worker began has its key and no instant"
        );
        assert_eq!(
            logged(Progress::Skipped),
            expected(None, None, None),
            "a skipped target has neither"
        );
    }
}
