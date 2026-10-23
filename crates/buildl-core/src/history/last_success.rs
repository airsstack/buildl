//! Which output a discovery target last produced.
//!
//! Its own file because the choice is one pure decision over a log's lines: it needs no port, so
//! it is tested here against hand-built lines.
//!
//! Responsibilities: [`last_success`].
//!
//! Non-responsibilities: reading the lines, which [`RunHistory`](crate::RunHistory) does; and
//! reading the output, which the caller does through a [`ContentStore`](crate::ContentStore).

use super::last_run::events;
use crate::types::{Digest, Label, LogLine};

/// The digest of the one output of the newest run of `label` that succeeded and produced exactly
/// one output.
///
/// A run that failed, was skipped, or produced any other number of outputs is passed, as is a run
/// of another label and a damaged line.
pub(crate) fn last_success(lines: &[LogLine], label: &Label) -> Option<Digest> {
    events(lines)
        .rev()
        .filter(|run| run.label == *label && run.class.succeeded())
        .find_map(|run| match run.outputs.as_slice() {
            [only] => Some(only.digest),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::num::NonZeroUsize;

    use super::last_success;
    use crate::types::{
        Argument, Command, Digest, Label, LogLine, LoggedRun, OutcomeClass, OutputName, Produced,
        Timestamp,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn produced(name: &str, content: &str) -> Produced {
        Produced {
            name: OutputName::parse(name).unwrap(),
            digest: Digest::of(content.as_bytes()),
        }
    }

    /// A run of `raw` ending as `class`, with one output per `(name, content)` entry.
    fn run(raw: &str, class: OutcomeClass, outputs: &[(&str, &str)]) -> LogLine {
        LogLine::Event(LoggedRun {
            at: Timestamp::from_unix_nanos(1),
            class,
            command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
            diagnostic: None,
            file: "build.lua".to_owned(),
            finished: None,
            key: None,
            label: label(raw),
            outputs: outputs
                .iter()
                .map(|(name, content)| produced(name, content))
                .collect(),
            started: None,
            stderr: None,
            stdout: None,
        })
    }

    fn built(raw: &str, content: &str) -> LogLine {
        run(raw, OutcomeClass::Built, &[("pkgs.json", content)])
    }

    #[test]
    fn last_success_takes_the_newest_run() {
        let lines = [built("//svc:pkgs", "a"), built("//svc:pkgs", "b")];
        assert_eq!(
            last_success(&lines, &label("//svc:pkgs")),
            Some(Digest::of(b"b"))
        );
    }

    #[test]
    fn last_success_counts_a_cached_run() {
        let lines = [
            built("//svc:pkgs", "a"),
            run("//svc:pkgs", OutcomeClass::Cached, &[("pkgs.json", "c")]),
        ];
        assert_eq!(
            last_success(&lines, &label("//svc:pkgs")),
            Some(Digest::of(b"c"))
        );
    }

    #[test]
    fn last_success_skips_a_newer_failure() {
        let lines = [
            built("//svc:pkgs", "a"),
            run(
                "//svc:pkgs",
                OutcomeClass::ActionFailure { status: 1 },
                &[("pkgs.json", "b")],
            ),
            run(
                "//svc:pkgs",
                OutcomeClass::Skipped {
                    because: label("//lib:x"),
                },
                &[],
            ),
        ];
        assert_eq!(
            last_success(&lines, &label("//svc:pkgs")),
            Some(Digest::of(b"a"))
        );
    }

    #[test]
    fn last_success_ignores_other_labels() {
        let lines = [built("//svc:pkgs", "a"), built("//svc:other", "b")];
        assert_eq!(
            last_success(&lines, &label("//svc:pkgs")),
            Some(Digest::of(b"a"))
        );
        assert_eq!(last_success(&lines, &label("//svc:none")), None);
    }

    #[test]
    fn last_success_needs_exactly_one_output() {
        let none = [run("//svc:pkgs", OutcomeClass::Built, &[])];
        let two = [run(
            "//svc:pkgs",
            OutcomeClass::Built,
            &[("a.json", "a"), ("b.json", "b")],
        )];
        assert_eq!(last_success(&none, &label("//svc:pkgs")), None);
        assert_eq!(last_success(&two, &label("//svc:pkgs")), None);
    }

    #[test]
    fn last_success_skips_damaged_lines() {
        let lines = [
            built("//svc:pkgs", "a"),
            LogLine::Damaged {
                line: NonZeroUsize::new(2).unwrap(),
            },
        ];
        assert_eq!(
            last_success(&lines, &label("//svc:pkgs")),
            Some(Digest::of(b"a"))
        );
    }
}
