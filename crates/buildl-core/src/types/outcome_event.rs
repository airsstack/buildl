//! One line of the build log: how the build of one target ended, when, and under which key.
//!
//! Its own file because the log is a second reader of an outcome, with needs of its own: the
//! build that returns an outcome already holds the graph, but a log is read later and alone, so
//! an event carries the build file, the command and the key beside the outcome, and names every
//! target by its label.
//!
//! Responsibilities: [`OutcomeEvent`] and its serialized form.
//!
//! Non-responsibilities: appending the event anywhere, which is a port's; and reading a log
//! back, which is a port's too: a line reads back as a [`LoggedRun`](crate::types::LoggedRun).

use serde::{Serialize, Serializer};

use crate::types::{
    ActionOutcome, Command, Diagnostic, Digest, Label, OutcomeClass, Produced, Provenance,
    Timestamp,
};

/// What a build log records for one target.
///
/// One event is written for every target of a build, whatever its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeEvent {
    /// When the outcome was recorded.
    pub at: Timestamp,
    /// The build file that declared the target.
    pub provenance: Provenance,
    /// The target's command, with every placeholder expanded.
    pub command: Command,
    /// The digest of the key the target was built or found cached under; `None` for a target
    /// that was skipped, whether or not a key had been finished for it.
    pub key: Option<Digest>,
    /// When a worker began the action; `None` for a target no worker began.
    pub started: Option<Timestamp>,
    /// When the worker was done with the action; `None` for a target no worker began. The span
    /// from `started` is how long the action took.
    pub finished: Option<Timestamp>,
    /// How the build of the target ended.
    pub outcome: ActionOutcome,
}

/// An event as it is written to JSON: one flat object, with the outcome's fields beside the
/// event's own.
#[derive(Serialize)]
struct EventView<'a> {
    at: Timestamp,
    class: &'a OutcomeClass,
    command: &'a Command,
    diagnostic: Option<&'a str>,
    file: String,
    finished: Option<Timestamp>,
    key: Option<Digest>,
    label: &'a Label,
    outputs: &'a [Produced],
    started: Option<Timestamp>,
    stderr: Option<Digest>,
    stdout: Option<Digest>,
}

/// Serializes as one flat object, so that a line of the log can be read on its own.
///
/// The build file is written as its path, and every absent value as `null`, so every event
/// holds the same twelve keys. It is read back as a [`LoggedRun`](crate::types::LoggedRun),
/// whose keys are these.
impl Serialize for OutcomeEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EventView {
            at: self.at,
            class: &self.outcome.class,
            command: &self.command,
            diagnostic: self.outcome.diagnostic.as_ref().map(Diagnostic::as_str),
            file: self.provenance.to_string(),
            finished: self.finished,
            key: self.key,
            label: &self.outcome.label,
            outputs: &self.outcome.outputs,
            started: self.started,
            stderr: self.outcome.stderr,
            stdout: self.outcome.stdout,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::OutcomeEvent;
    use crate::json::canonical;
    use crate::types::{
        ActionOutcome, Argument, Command, Diagnostic, Digest, Directory, InfraKind, Label,
        OutcomeClass, OutputName, Produced, Provenance, Timestamp,
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

    fn root_file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::root())
    }

    const fn instant(nanos: u64) -> Timestamp {
        Timestamp::from_unix_nanos(nanos)
    }

    #[test]
    fn a_built_target_is_logged_with_its_key_its_outputs_and_both_instants() {
        let key = Digest::of(b"key");
        let text = Digest::of(b"text");
        let printed = Digest::of(b"compiling\n");
        let nothing = Digest::of(b"");
        let event = OutcomeEvent {
            at: instant(30),
            provenance: lib_file(),
            command: command(&["cc", "-c", "lib/a.c", "-o", "out/lib/text"]),
            key: Some(key),
            started: Some(instant(10)),
            finished: Some(instant(20)),
            outcome: ActionOutcome {
                label: Label::parse("//lib:text").unwrap(),
                class: OutcomeClass::Built,
                diagnostic: None,
                outputs: vec![Produced {
                    name: OutputName::parse("text").unwrap(),
                    digest: text,
                }],
                stdout: Some(printed),
                stderr: Some(nothing),
            },
        };
        assert_eq!(
            canonical::to_string(&event).unwrap(),
            format!(
                concat!(
                    r#"{{"at":30,"class":"Built","#,
                    r#""command":["cc","-c","lib/a.c","-o","out/lib/text"],"#,
                    r#""diagnostic":null,"file":"lib/build.lua","finished":20,"#,
                    r#""key":"{key}","label":"//lib:text","#,
                    r#""outputs":[{{"digest":"{text}","name":"text"}}],"#,
                    r#""started":10,"stderr":"{nothing}","stdout":"{printed}"}}"#
                ),
                key = key,
                text = text,
                nothing = nothing,
                printed = printed
            )
        );
    }

    #[test]
    fn a_failed_target_is_logged_with_its_class_and_the_message() {
        let key = Digest::of(b"key");
        let printed = Digest::of(b"compiling\n");
        let nothing = Digest::of(b"");
        let event = OutcomeEvent {
            at: instant(30),
            provenance: root_file(),
            command: command(&["cc", "out/lib/text", "-o", "out/app"]),
            key: Some(key),
            started: Some(instant(10)),
            finished: Some(instant(20)),
            outcome: ActionOutcome {
                label: Label::parse("//:app").unwrap(),
                class: OutcomeClass::InfraFailure {
                    kind: InfraKind::Store,
                },
                diagnostic: Some(Diagnostic::new("content store: disk full")),
                outputs: Vec::new(),
                stdout: Some(printed),
                stderr: Some(nothing),
            },
        };
        assert_eq!(
            canonical::to_string(&event).unwrap(),
            format!(
                concat!(
                    r#"{{"at":30,"class":{{"InfraFailure":{{"kind":"Store"}}}},"#,
                    r#""command":["cc","out/lib/text","-o","out/app"],"#,
                    r#""diagnostic":"content store: disk full","file":"build.lua","#,
                    r#""finished":20,"key":"{key}","label":"//:app","outputs":[],"#,
                    r#""started":10,"stderr":"{nothing}","stdout":"{printed}"}}"#
                ),
                key = key,
                nothing = nothing,
                printed = printed
            )
        );
    }

    #[test]
    fn a_skipped_target_is_logged_without_a_key_an_instant_or_a_digest() {
        let event = OutcomeEvent {
            at: instant(31),
            provenance: root_file(),
            command: command(&["cc", "out/lib/text", "-o", "out/app"]),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: Label::parse("//:app").unwrap(),
                class: OutcomeClass::Skipped {
                    because: Label::parse("//lib:text").unwrap(),
                },
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        };
        assert_eq!(
            canonical::to_string(&event).unwrap(),
            concat!(
                r#"{"at":31,"class":{"Skipped":{"because":"//lib:text"}},"#,
                r#""command":["cc","out/lib/text","-o","out/app"],"#,
                r#""diagnostic":null,"file":"build.lua","finished":null,"key":null,"#,
                r#""label":"//:app","outputs":[],"started":null,"stderr":null,"stdout":null}"#
            )
        );
    }

    #[test]
    fn an_event_holds_exactly_the_twelve_keys_of_the_log_format() {
        let event = OutcomeEvent {
            at: instant(1),
            provenance: root_file(),
            command: command(&["cc"]),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: Label::parse("//:app").unwrap(),
                class: OutcomeClass::Cached,
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        };
        let tree = serde_json::to_value(&event).unwrap();
        let keys: Vec<&str> = tree
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "at",
                "class",
                "command",
                "diagnostic",
                "file",
                "finished",
                "key",
                "label",
                "outputs",
                "started",
                "stderr",
                "stdout"
            ]
        );
    }
}
