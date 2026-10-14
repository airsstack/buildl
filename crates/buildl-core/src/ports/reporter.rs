//! The port through which a build shows a person what it is doing.
//!
//! Its own file because showing means writing to a terminal, and this crate writes nowhere: it
//! says what began, what was printed and how each target ended, and an adapter decides what a
//! person sees of it.
//!
//! Responsibilities: [`Reporter`].
//!
//! Non-responsibilities: implementing it, and deciding what is reported and when. The first
//! belongs to an adapter crate; the second to the phase that runs actions. Showing an error that
//! fails a whole command is not here either: an error is returned to the caller.
//!
//! A reporter borrowed mutably is a reporter too, so a caller that keeps its reporter can lend
//! it to one build.

use crate::types::{Captured, Channel, Label, OutcomeEvent, ReadyAction, Summary};

/// Is told how a build goes, one call at a time.
///
/// Every call is made on the thread that called the build, never on one that runs an action, so
/// an implementation needs no lock and is the only writer of what it writes to. No call can
/// fail: what a reporter cannot show is its own to deal with, and changes nothing a build
/// stores or returns.
///
/// One build makes its calls in this order: one [`begun`], any number of [`started`],
/// [`printed`] and [`finished`], then one [`summary`]. A build that is refused before it runs
/// anything makes no call, and one that ends in an error after that calls [`summary`] never.
///
/// From the first [`printed`] for a target until that target's [`finished`], no [`printed`] and
/// no [`finished`] is about another target: what one action prints is never interleaved with
/// anything another printed.
///
/// [`begun`]: Reporter::begun
/// [`started`]: Reporter::started
/// [`printed`]: Reporter::printed
/// [`finished`]: Reporter::finished
/// [`summary`]: Reporter::summary
pub trait Reporter {
    /// A build of `targets` targets is about to hand out its first action: everything that can
    /// refuse a build has passed. Called once, first. Each of those targets is reported by one
    /// [`finished`](Reporter::finished).
    fn begun(&mut self, targets: usize);

    /// A worker began `action`. Called for a target that runs, and never for one that is cached
    /// or skipped.
    fn started(&mut self, action: &ReadyAction);

    /// The running action of the target `label` wrote `bytes` to `channel`. Called only under
    /// [`LiveOutput::OneAction`](crate::LiveOutput::OneAction), and only for the one action
    /// shown live at the time.
    fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]);

    /// A target has its outcome: `event` is what the build log holds for it. Called once for
    /// every target of the build, whatever its class.
    ///
    /// `printed` is what its action wrote that [`printed`](Reporter::printed) was not handed:
    /// everything it wrote when it last ran, for a target that is cached as for one that ran
    /// now; and nothing for a target whose output was shown live, or that has none stored.
    fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>);

    /// Every target has its outcome, counted in `summary`. Called once, last.
    fn summary(&mut self, summary: Summary);
}

impl<R: Reporter + ?Sized> Reporter for &mut R {
    fn begun(&mut self, targets: usize) {
        (**self).begun(targets);
    }

    fn started(&mut self, action: &ReadyAction) {
        (**self).started(action);
    }

    fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]) {
        (**self).printed(label, channel, bytes);
    }

    fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
        (**self).finished(event, printed);
    }

    fn summary(&mut self, summary: Summary) {
        (**self).summary(summary);
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::Reporter;
    use crate::types::{
        ActionOutcome, Argument, Captured, Channel, Command, Directory, Label, NetworkAccess,
        OutcomeClass, OutcomeEvent, Provenance, ReadyAction, Summary, Timestamp,
    };

    /// Keeps the name of each call it receives, in order.
    #[derive(Default)]
    struct Names(Vec<&'static str>);

    impl Reporter for Names {
        fn begun(&mut self, _targets: usize) {
            self.0.push("begun");
        }

        fn started(&mut self, _action: &ReadyAction) {
            self.0.push("started");
        }

        fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {
            self.0.push("printed");
        }

        fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {
            self.0.push("finished");
        }

        fn summary(&mut self, _summary: Summary) {
            self.0.push("summary");
        }
    }

    /// Makes one call of each kind on a reporter it owns.
    fn report_once<R: Reporter>(mut reporter: R) {
        let label = Label::parse("//:app").unwrap();
        let command = Command::new(vec![Argument::parse("cc").unwrap()]).unwrap();
        reporter.begun(1);
        reporter.started(&ReadyAction {
            label: label.clone(),
            command: command.clone(),
            description: None,
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            network: NetworkAccess::Sealed,
        });
        reporter.printed(&label, Channel::Stdout, b"hi");
        reporter.finished(
            &OutcomeEvent {
                at: Timestamp::UNIX_EPOCH,
                provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                command,
                key: None,
                started: None,
                finished: None,
                outcome: ActionOutcome {
                    label,
                    class: OutcomeClass::Built,
                    diagnostic: None,
                    outputs: Vec::new(),
                    stdout: None,
                    stderr: None,
                },
            },
            Captured::default(),
        );
        reporter.summary(Summary {
            failed: 0,
            skipped: 0,
            built: 1,
            cached: 0,
        });
    }

    #[test]
    fn a_borrowed_reporter_receives_every_call_made_through_the_borrow() {
        let mut names = Names::default();
        report_once(&mut names);
        assert_eq!(
            names.0,
            ["begun", "started", "printed", "finished", "summary"]
        );
    }
}
