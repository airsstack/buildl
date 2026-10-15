//! Which run of a target a reader of the build log is shown.
//!
//! Its own file because the choice is one pure decision over a log's lines: it needs no port, so
//! it is tested here against hand-built lines.
//!
//! Responsibilities: [`LastRun`] and [`last_run`].
//!
//! Non-responsibilities: reading the lines, which [`RunHistory`](crate::RunHistory) does; and
//! reading what the run printed, which [`pipeline::log`](fn@crate::pipeline::log) does.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use crate::error::{Error, Result};
use crate::suggest::nearest;
use crate::types::{Label, LogLine, LoggedRun, OutcomeClass};

/// The newest logged run of a target that has stored output, and what the log said around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastRun {
    /// The run shown.
    pub run: LoggedRun,
    /// The class of each newer line for the label that had no stored output, newest first.
    pub newer: Vec<OutcomeClass>,
    /// Every damaged line of the file, in line order.
    pub damaged: Vec<NonZeroUsize>,
}

/// Chooses the newest line of `label` that has both its output digests.
///
/// A line with either digest `None` ran nothing that kept output, so it is passed and its class
/// noted in [`LastRun::newer`]. A line of another label is passed unnoted.
///
/// # Errors
///
/// Returns [`Error::NoRun`] when no line of `label` has both digests. Its suggestion is the
/// nearest label that does have a run, so a label the log holds only skipped lines for is never
/// offered as its own correction.
pub(crate) fn last_run(lines: &[LogLine], label: &Label) -> Result<LastRun> {
    let damaged: Vec<NonZeroUsize> = lines
        .iter()
        .filter_map(|line| match line {
            LogLine::Damaged { line } => Some(*line),
            LogLine::Event(_) => None,
        })
        .collect();
    let mut newer = Vec::new();
    for run in events(lines).rev() {
        if run.label != *label {
            continue;
        }
        if has_output(run) {
            return Ok(LastRun {
                run: run.clone(),
                newer,
                damaged,
            });
        }
        newer.push(run.class.clone());
    }
    let with_run: BTreeSet<Label> = events(lines)
        .filter(|run| has_output(run))
        .map(|run| run.label.clone())
        .collect();
    let suggestion = nearest(&label.to_string(), &with_run).cloned();
    Err(Error::NoRun {
        label: label.clone(),
        suggestion,
        damaged,
    })
}

fn events(lines: &[LogLine]) -> impl DoubleEndedIterator<Item = &LoggedRun> {
    lines.iter().filter_map(|line| match line {
        LogLine::Event(run) => Some(run),
        LogLine::Damaged { .. } => None,
    })
}

const fn has_output(run: &LoggedRun) -> bool {
    run.stdout.is_some() && run.stderr.is_some()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::num::NonZeroUsize;

    use super::last_run;
    use crate::error::Error;
    use crate::types::{
        Argument, Command, Digest, Label, LogLine, LoggedRun, OutcomeClass, Timestamp,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn line(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    fn damaged(n: usize) -> LogLine {
        LogLine::Damaged { line: line(n) }
    }

    /// A run of `raw` ending as `class`, with both streams kept when `stdout` is `Some`.
    fn run(raw: &str, class: OutcomeClass, stdout: Option<&str>) -> LogLine {
        LogLine::Event(LoggedRun {
            at: Timestamp::from_unix_nanos(1),
            class,
            command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
            diagnostic: None,
            file: "build.lua".to_owned(),
            finished: None,
            key: None,
            label: label(raw),
            outputs: Vec::new(),
            started: None,
            stderr: stdout.map(|_| Digest::of(b"err")),
            stdout: stdout.map(|text| Digest::of(text.as_bytes())),
        })
    }

    fn built(raw: &str, stdout: &str) -> LogLine {
        run(raw, OutcomeClass::Built, Some(stdout))
    }

    /// A run of `raw` that kept its stdout but no stderr.
    fn stdout_only(raw: &str, stdout: &str) -> LogLine {
        let LogLine::Event(mut kept) = built(raw, stdout) else {
            unreachable!("`built` returns an event");
        };
        kept.stderr = None;
        LogLine::Event(kept)
    }

    fn skipped(raw: &str, because: &str) -> LogLine {
        run(
            raw,
            OutcomeClass::Skipped {
                because: label(because),
            },
            None,
        )
    }

    /// The suggestion and damaged lines of the `NoRun` that `last_run` returned.
    fn refused(lines: &[LogLine], asked: &str) -> (Option<Label>, Vec<NonZeroUsize>) {
        match last_run(lines, &label(asked)) {
            Err(Error::NoRun {
                label: named,
                suggestion,
                damaged,
            }) => {
                assert_eq!(named, label(asked));
                (suggestion, damaged)
            }
            other => unreachable!("expected NoRun, found {other:?}"),
        }
    }

    #[test]
    fn the_newest_run_with_output_is_chosen() {
        let lines = [built("//:app", "a"), built("//:app", "b")];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.stdout, Some(Digest::of(b"b")));
        assert_eq!(found.newer, Vec::<OutcomeClass>::new());
    }

    #[test]
    fn a_newer_skipped_line_is_noted_not_chosen() {
        let lines = [built("//:app", "a"), skipped("//:app", "//lib:text")];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.class, OutcomeClass::Built);
        assert_eq!(
            found.newer,
            [OutcomeClass::Skipped {
                because: label("//lib:text")
            }]
        );
    }

    #[test]
    fn a_line_with_only_one_stream_is_noted_not_chosen() {
        let lines = [built("//:app", "a"), stdout_only("//:app", "b")];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.stdout, Some(Digest::of(b"a")));
        assert_eq!(found.newer, [OutcomeClass::Built]);
    }

    #[test]
    fn newer_lines_are_listed_newest_first_and_older_ones_left_out() {
        let lines = [
            skipped("//:app", "//lib:old"),
            built("//:app", "a"),
            skipped("//:app", "//lib:x"),
            skipped("//:app", "//lib:y"),
        ];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.stdout, Some(Digest::of(b"a")));
        assert_eq!(
            found.newer,
            [
                OutcomeClass::Skipped {
                    because: label("//lib:y")
                },
                OutcomeClass::Skipped {
                    because: label("//lib:x")
                },
            ]
        );
    }

    #[test]
    fn a_cached_line_is_a_run() {
        let lines = [
            built("//:app", "a"),
            run("//:app", OutcomeClass::Cached, Some("b")),
        ];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.class, OutcomeClass::Cached);
        assert_eq!(found.newer, Vec::<OutcomeClass>::new());
    }

    #[test]
    fn a_failed_line_is_a_run() {
        let lines = [
            built("//:app", "a"),
            run(
                "//:app",
                OutcomeClass::ActionFailure { status: 1 },
                Some("b"),
            ),
        ];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.class, OutcomeClass::ActionFailure { status: 1 });
    }

    #[test]
    fn another_labels_lines_are_passed() {
        let lines = [built("//:app", "a"), built("//:lib", "b")];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.run.label, label("//:app"));
        assert_eq!(found.newer, Vec::<OutcomeClass>::new());
    }

    #[test]
    fn damaged_lines_of_the_whole_file_are_collected_in_order() {
        let lines = [damaged(1), built("//:app", "a"), damaged(3)];
        let found = last_run(&lines, &label("//:app")).unwrap();
        assert_eq!(found.damaged, [line(1), line(3)]);
    }

    #[test]
    fn a_label_with_only_skipped_lines_has_no_run() {
        let lines = [skipped("//:app", "//lib:text"), built("//:apq", "a")];
        assert_eq!(refused(&lines, "//:app").0, Some(label("//:apq")));

        let alone = [skipped("//:app", "//lib:text")];
        assert_eq!(refused(&alone, "//:app").0, None);
    }

    #[test]
    fn a_label_never_logged_has_no_run_and_a_suggestion() {
        let lines = [built("//:app", "a")];
        assert_eq!(refused(&lines, "//:ap").0, Some(label("//:app")));
    }

    #[test]
    fn no_run_carries_the_damaged_lines() {
        let lines = [damaged(2)];
        assert_eq!(refused(&lines, "//:app").1, [line(2)]);
    }
}
