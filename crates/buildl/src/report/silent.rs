//! The reporter that shows nothing.
//!
//! Its own file because a build always needs a reporter, and some builds have nobody to show
//! anything to: a caller that only wants the outcomes, a test, a dry run. A type with no state
//! says that at the call site, where a writer that discards everything would not.
//!
//! Responsibilities: [`Silent`] and its [`Reporter`] implementation.
//!
//! Non-responsibilities: recording what a build did. The outcomes a build returns and the log
//! it writes hold that; this type keeps nothing.

use buildl_core::{
    Captured, Channel, Label, OutcomeEvent, ReadyAction, RemoteNotice, Reporter, Summary,
};

/// A reporter that shows nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl Reporter for Silent {
    fn begun(&mut self, _targets: usize) {}

    fn started(&mut self, _action: &ReadyAction) {}

    fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

    fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}

    fn remote(&mut self, _notice: &RemoteNotice) {}

    fn summary(&mut self, _summary: Summary) {}
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use buildl_core::{
        ActionOutcome, Argument, Captured, Channel, Command, Diagnostic, Directory, Label,
        NetworkAccess, OutcomeClass, OutcomeEvent, OutputSource, Provenance, ReadyAction,
        RemoteCall, RemoteNotice, Reporter, Summary, Tally, TargetRole, Timestamp,
    };

    use super::Silent;

    #[test]
    fn every_call_is_taken_and_nothing_happens() {
        let label = Label::parse("//:app").unwrap();
        let command = Command::new(vec![Argument::parse("cc").unwrap()]).unwrap();
        let mut silent = Silent;
        silent.begun(1);
        silent.started(&ReadyAction {
            label: label.clone(),
            command: command.clone(),
            description: None,
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            network: NetworkAccess::Sealed,
            output_source: OutputSource::Command,
        });
        silent.printed(&label, Channel::Stdout, b"hi");
        silent.finished(
            &OutcomeEvent {
                at: Timestamp::UNIX_EPOCH,
                provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                command,
                key: None,
                started: None,
                finished: None,
                outcome: ActionOutcome {
                    label,
                    role: TargetRole::Build,
                    class: OutcomeClass::Built,
                    diagnostic: None,
                    outputs: Vec::new(),
                    stdout: None,
                    stderr: None,
                },
            },
            Captured {
                stdout: b"hi",
                stderr: b"",
            },
        );
        silent.remote(&RemoteNotice::Failed {
            call: RemoteCall::Lookup,
            diagnostic: Diagnostic::new("unreachable"),
        });
        silent.summary(Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 1,
                cached: 0,
                fetched: 0,
                remote: 0,
            },
            tests: Tally::default(),
        });
        // It has nowhere to keep anything it was told.
        assert_eq!(size_of_val(&silent), 0);
    }
}
