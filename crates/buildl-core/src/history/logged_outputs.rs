//! The blobs the build log still reads.
//!
//! Its own file because it is one pure decision over a log's lines, made by the same rule as
//! [`last_run`](fn@super::last_run): the run it picks for a label is the run `log` would show.
//!
//! Responsibilities: `logged_outputs`.
//!
//! Non-responsibilities: reading the lines, which [`RunHistory`](crate::RunHistory) does; and
//! removing blobs, which the caller does.

use std::collections::BTreeSet;

use super::last_run::{events, has_output};
use crate::types::{Digest, LogLine};

/// The stdout and stderr digests of each label's newest run that kept both streams.
///
/// That is the run [`last_run`](fn@super::last_run) would show for the label. Damaged lines, and runs
/// missing either digest, add nothing.
pub(crate) fn logged_outputs(lines: &[LogLine]) -> BTreeSet<Digest> {
    let mut seen = BTreeSet::new();
    let mut digests = BTreeSet::new();
    for run in events(lines).rev() {
        if !has_output(run) || !seen.insert(&run.label) {
            continue;
        }
        digests.extend(run.stdout.iter().chain(&run.stderr).copied());
    }
    digests
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::num::NonZeroUsize;

    use super::logged_outputs;
    use crate::types::{
        Argument, Command, Digest, Label, LogLine, LoggedRun, OutcomeClass, Timestamp,
    };

    /// A run of `raw` whose streams are `out` and `err`; a `None` stream is a `null` digest.
    fn run(raw: &str, out: Option<&str>, err: Option<&str>) -> LogLine {
        LogLine::Event(LoggedRun {
            at: Timestamp::from_unix_nanos(1),
            class: OutcomeClass::Built,
            command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
            diagnostic: None,
            file: "build.lua".to_owned(),
            finished: None,
            key: None,
            label: Label::parse(raw).unwrap(),
            outputs: Vec::new(),
            started: None,
            stderr: err.map(|text| Digest::of(text.as_bytes())),
            stdout: out.map(|text| Digest::of(text.as_bytes())),
        })
    }

    fn digests(texts: &[&str]) -> BTreeSet<Digest> {
        texts
            .iter()
            .map(|text| Digest::of(text.as_bytes()))
            .collect()
    }

    #[test]
    fn takes_each_labels_newest_run_with_output() {
        let lines = [
            run("//:app", Some("app-out-1"), Some("app-err-1")),
            run("//:lib", Some("lib-out-1"), Some("lib-err-1")),
            run("//:app", Some("app-out-2"), Some("app-err-2")),
            run("//:lib", Some("lib-out-2"), Some("lib-err-2")),
        ];
        assert_eq!(
            logged_outputs(&lines),
            digests(&["app-out-2", "app-err-2", "lib-out-2", "lib-err-2"])
        );
    }

    #[test]
    fn an_older_runs_digests_are_not_taken() {
        let lines = [
            run("//:app", Some("old-out"), Some("old-err")),
            run("//:app", Some("new-out"), Some("new-err")),
        ];
        let taken = logged_outputs(&lines);
        assert!(!taken.contains(&Digest::of(b"old-out")));
        assert!(!taken.contains(&Digest::of(b"old-err")));
        assert_eq!(taken, digests(&["new-out", "new-err"]));
    }

    #[test]
    fn a_newer_run_without_output_leaves_the_older_one() {
        let lines = [
            run("//:app", Some("old-out"), Some("old-err")),
            run("//:app", Some("half-out"), None),
        ];
        assert_eq!(logged_outputs(&lines), digests(&["old-out", "old-err"]));
    }

    #[test]
    fn skipped_null_and_damaged_lines_add_nothing() {
        let lines = [
            run("//:skipped", None, None),
            run("//:half", Some("half-out"), None),
            run("//:other-half", None, Some("other-half-err")),
            LogLine::Damaged {
                line: NonZeroUsize::new(4).unwrap(),
            },
        ];
        assert_eq!(logged_outputs(&lines), BTreeSet::new());
    }
}
