//! One event of the build log as it reads back.
//!
//! Its own file because reading is the twin of writing: [`OutcomeEvent`](crate::types::OutcomeEvent)
//! serializes one flat object of twelve keys, and a reader needs a value with those same keys,
//! each with its own type, to read it into. The two are pinned together by a round trip in this
//! file's tests, so neither can gain, lose or rename a key without the other.
//!
//! Responsibilities: [`LoggedRun`].
//!
//! Non-responsibilities: reading the log file line by line, which a port's adapter does; and
//! turning a run back into an [`ActionOutcome`](crate::types::ActionOutcome), which a reader
//! that needs one does from the fields.

use serde::{Deserialize, Serialize};

use crate::types::{Command, Digest, Label, OutcomeClass, Produced, Timestamp};

/// One event of the build log as it reads back: the twelve keys a build writes, each with its
/// own type.
///
/// A line is one of these exactly when [`canonical::from_slice`](crate::json::canonical::from_slice)
/// reads it, so a line missing a key, holding another, or not in canonical form is not an
/// event. `file` is the build file's path as written, not a
/// [`Provenance`](crate::types::Provenance): the log does not hold the directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggedRun {
    /// When the outcome was recorded.
    pub at: Timestamp,
    /// How the build of the target ended.
    pub class: OutcomeClass,
    /// The target's command, with every placeholder expanded.
    pub command: Command,
    /// The message of a failure; `None` for a target that did not fail.
    pub diagnostic: Option<String>,
    /// The path of the build file that declared the target, as written.
    pub file: String,
    /// When the worker was done with the action; `None` for a target no worker began.
    pub finished: Option<Timestamp>,
    /// The digest of the key the target was built or found cached under; `None` for a target
    /// that was skipped.
    pub key: Option<Digest>,
    /// The target the event is about.
    pub label: Label,
    /// What the target produced, one entry per output.
    pub outputs: Vec<Produced>,
    /// When a worker began the action; `None` for a target no worker began.
    pub started: Option<Timestamp>,
    /// The digest of what the action printed on its error channel; `None` when nothing was
    /// kept.
    pub stderr: Option<Digest>,
    /// The digest of what the action printed on its output channel; `None` when nothing was
    /// kept.
    pub stdout: Option<Digest>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::LoggedRun;
    use crate::error::Error;
    use crate::json::canonical;
    use crate::types::{
        ActionOutcome, Argument, Command, Diagnostic, Digest, Directory, InfraKind, Label,
        OutcomeClass, OutcomeEvent, OutputName, Produced, Provenance, TargetRole, Timestamp,
    };

    fn command(arguments: &[&str]) -> Command {
        Command::new(
            arguments
                .iter()
                .map(|argument| Argument::parse(*argument).unwrap())
                .collect(),
        )
        .unwrap()
    }

    fn lib_file() -> Provenance {
        Provenance::new(
            PathBuf::from("lib/build.lua"),
            Directory::parse("lib").unwrap(),
        )
    }

    const fn instant(nanos: u64) -> Timestamp {
        Timestamp::from_unix_nanos(nanos)
    }

    fn built() -> OutcomeEvent {
        OutcomeEvent {
            at: instant(30),
            provenance: lib_file(),
            command: command(&["cc", "-c", "lib/a.c", "-o", "out/lib/text"]),
            key: Some(Digest::of(b"key")),
            started: Some(instant(10)),
            finished: Some(instant(20)),
            outcome: ActionOutcome {
                label: Label::parse("//lib:text").unwrap(),
                role: TargetRole::Build,
                class: OutcomeClass::Built,
                diagnostic: None,
                outputs: vec![Produced {
                    name: OutputName::parse("text").unwrap(),
                    digest: Digest::of(b"text"),
                }],
                stdout: Some(Digest::of(b"compiling\n")),
                stderr: Some(Digest::of(b"")),
            },
        }
    }

    fn failed() -> OutcomeEvent {
        OutcomeEvent {
            at: instant(30),
            provenance: lib_file(),
            command: command(&["cc", "out/lib/text", "-o", "out/app"]),
            key: Some(Digest::of(b"key")),
            started: Some(instant(10)),
            finished: Some(instant(20)),
            outcome: ActionOutcome {
                label: Label::parse("//lib:app").unwrap(),
                role: TargetRole::Build,
                class: OutcomeClass::InfraFailure {
                    kind: InfraKind::Store,
                },
                diagnostic: Some(Diagnostic::new("content store: disk full")),
                outputs: Vec::new(),
                stdout: Some(Digest::of(b"compiling\n")),
                stderr: Some(Digest::of(b"")),
            },
        }
    }

    fn skipped() -> OutcomeEvent {
        OutcomeEvent {
            at: instant(31),
            provenance: lib_file(),
            command: command(&["cc", "out/lib/text", "-o", "out/app"]),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: Label::parse("//lib:app").unwrap(),
                role: TargetRole::Build,
                class: OutcomeClass::Skipped {
                    because: Label::parse("//lib:text").unwrap(),
                },
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        }
    }

    #[test]
    fn an_event_line_reads_back_as_a_logged_run_with_the_same_bytes() {
        for event in [built(), failed(), skipped()] {
            let written = canonical::to_vec(&event).unwrap();
            let run: LoggedRun = canonical::from_slice(&written).unwrap();
            assert_eq!(canonical::to_vec(&run).unwrap(), written);
            assert_eq!(run.at, event.at);
            assert_eq!(run.class, event.outcome.class);
            assert_eq!(run.command, event.command);
            assert_eq!(
                run.diagnostic.as_deref(),
                event.outcome.diagnostic.as_ref().map(Diagnostic::as_str)
            );
            assert_eq!(run.file, "lib/build.lua");
            assert_eq!(run.finished, event.finished);
            assert_eq!(run.key, event.key);
            assert_eq!(run.label, event.outcome.label);
            assert_eq!(run.outputs, event.outcome.outputs);
            assert_eq!(run.started, event.started);
            assert_eq!(run.stderr, event.outcome.stderr);
            assert_eq!(run.stdout, event.outcome.stdout);
        }
        let run: LoggedRun = canonical::from_slice(&canonical::to_vec(&failed()).unwrap()).unwrap();
        assert_eq!(run.diagnostic.as_deref(), Some("content store: disk full"));
    }

    #[test]
    fn a_line_with_a_key_missing_is_not_an_event() {
        let line = canonical::to_string(&built())
            .unwrap()
            .replace(r#""finished":20,"#, "");
        assert!(matches!(
            canonical::from_slice::<LoggedRun>(line.as_bytes()),
            Err(Error::NonCanonicalJson { .. })
        ));
    }

    #[test]
    fn a_line_with_an_unknown_key_is_not_an_event() {
        let line = canonical::to_string(&built())
            .unwrap()
            .replace(r#""started""#, r#""role":"Build","started""#);
        assert!(line.contains(r#""role":"Build""#));
        assert!(matches!(
            canonical::from_slice::<LoggedRun>(line.as_bytes()),
            Err(Error::UnreadableJson { .. })
        ));
    }

    #[test]
    fn a_line_out_of_canonical_form_is_not_an_event() {
        let line = canonical::to_string(&built())
            .unwrap()
            .replacen(':', ": ", 1);
        assert!(matches!(
            canonical::from_slice::<LoggedRun>(line.as_bytes()),
            Err(Error::NonCanonicalJson { .. })
        ));
    }
}
