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
//! target's outputs in place, and [`append`] logs an outcome whatever it is.
//!
//! Non-responsibilities: how content is put in place, how a row is kept and how the log is
//! written, which are the ports'.

use crate::error::{Error, Result};
use crate::execute::perform::{Printed, Span};
use crate::ports::{ActionCache, Clock, ContentStore, EventLog};
use crate::types::{
    ActionKey, ActionOutcome, CacheRow, Command, Diagnostic, Digest, InfraKind, Label,
    OutcomeClass, OutcomeEvent, OutputPath, Provenance, Timestamp,
};

/// The class of a target whose outputs, or whose row, could not be stored or put in place.
const NOT_STORED: OutcomeClass = OutcomeClass::InfraFailure {
    kind: InfraKind::Store,
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

/// The row to record for an action that ran under `key`, left `outputs` and printed `printed`;
/// `None` when `outputs` does not hold one digest per output the key declares.
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
/// A port that fails either step makes the outcome a failure to store, and no row is kept.
pub(crate) fn store<A: ActionCache, C: ContentStore>(
    label: Label,
    key: ActionKey,
    outputs: Vec<Digest>,
    printed: Printed,
    paths: &[OutputPath],
    cas: &C,
    cache: &mut A,
) -> ActionOutcome {
    let Some(row) = row(key, outputs, printed) else {
        return mismatched(label, printed);
    };
    let outcome = built(label.clone(), &row);
    let stored = place(&row, paths, cas).and_then(|()| cache.record(label.clone(), row));
    match stored {
        Ok(()) => outcome,
        Err(error) => unstored(label, &error, printed),
    }
}

/// Puts the outputs of a target that need not run back in place, from what `row` recorded,
/// and returns its outcome. Nothing is written to the cache.
pub(crate) fn restore<C: ContentStore>(
    label: Label,
    row: &CacheRow,
    paths: &[OutputPath],
    cas: &C,
) -> ActionOutcome {
    match place(row, paths, cas) {
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

/// Puts each output `row` holds at its path of `paths`, pairwise, in the order the outputs
/// were written.
fn place<C: ContentStore>(row: &CacheRow, paths: &[OutputPath], cas: &C) -> Result<()> {
    row.produced()
        .iter()
        .zip(paths)
        .try_for_each(|(produced, at)| cas.place(&produced.digest, at))
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

    use super::{
        Progress, built, cached, event, failed, mismatched, row, skipped, unplaced, unstored,
    };
    use crate::error::{Error, PortName};
    use crate::execute::fixtures::{command, key, label, node, row as recorded};
    use crate::execute::perform::{Printed, Span};
    use crate::types::{
        ActionOutcome, Diagnostic, Digest, InfraKind, OutcomeClass, OutcomeEvent, OutputName,
        Timestamp,
    };

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
        assert_eq!(
            row.digests().collect::<Vec<_>>(),
            [app, map, printed().stdout, printed().stderr]
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
