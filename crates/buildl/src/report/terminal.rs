//! The reporter that shows a build on a terminal, or in a stream nobody watches.
//!
//! Its own file because it is the one place of this crate that decides when something is
//! written: a block when a target ends, a status line between blocks, one action's output as it
//! arrives, the summary last. It is given the stream it writes to as a value, so what it writes
//! can be captured in memory.
//!
//! Responsibilities: [`TerminalReporter`], its [`Reporter`] implementation and
//! [`TerminalReporter::close`]; [`Mode`], whether a status line is kept; and [`Report`] and
//! [`TestBlocks`], what kind of run is shown and which tests of a test run get a block.
//!
//! Non-responsibilities: what a block or the summary holds, which the block renderings decide;
//! whether to colour and whether the stream is a terminal, which the caller decides; and the
//! order of the calls, which the build that makes them guarantees. Between an action's first
//! live bytes and its end the build reports nothing about another target, so nothing is held
//! back here.

use std::io::Write;

use buildl_core::{
    Captured, Channel, Label, OutcomeClass, OutcomeEvent, ReadyAction, Reporter, Rerun, Summary,
    Tally, TargetRole,
};

use crate::report::block::{block, header, summary_line, test_line, trailer};
use crate::report::paint::Paint;

/// Moves to the start of the row and clears it.
const ERASE: &[u8] = b"\r\x1b[2K";
/// Stops the terminal from wrapping a long row onto the next one.
const WRAP_OFF: &[u8] = b"\x1b[?7l";
/// Lets the terminal wrap again.
const WRAP_ON: &[u8] = b"\x1b[?7h";

/// Whether a status line is kept below what a build prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Blocks and the summary only: for a pipe, a file, a log.
    Plain,
    /// A status line on the last row, erased before anything is printed and drawn again after:
    /// for a terminal.
    Live,
}

/// What kind of run a reporter shows, which decides what it prints of a passing test and how it
/// sums up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// A build: every target is shown as it is, and the summary counts targets.
    Build,
    /// A test run: a dependency is shown as in a build, a test that passed is shown as
    /// `blocks` says, and the summary counts tests; a run of discovery targets before it is
    /// counted as a build is.
    Test {
        /// Which tests get a block.
        blocks: TestBlocks,
        /// Whether the run forced every test to run again, which its summary says.
        rerun: Rerun,
    },
}

/// Which tests of a test run get a block: those that failed, or every one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestBlocks {
    /// Only a test that failed. A test that was built, cached or fetched shows nothing.
    Failed,
    /// Every test, a passing one with what it printed, and a cached or fetched one marked as such.
    Every,
}

/// Writes a build's blocks, status line and summary to `out`.
///
/// Each call writes at most once: everything it has to say is gathered and handed to `out` in
/// one write, then `out` is flushed. A write or a flush that fails ends the writing: nothing
/// more is written, and nothing else changes, since a build does not depend on being shown.
///
/// The status line reads `[2/4] 1 running: compile main.c`: the targets that have their
/// outcome of all the targets, how many actions run, and the one that has run longest, by its
/// description or else its label. It is drawn again only when a call arrives, with the
/// terminal's wrapping switched off for the length of that one write, so a long line is cut at
/// the edge instead of taking a second row. It is never drawn while one action's output is
/// shown as it arrives.
#[derive(Debug)]
pub struct TerminalReporter<W: Write> {
    /// Where everything is written.
    out: W,
    /// Whether a status line is kept.
    mode: Mode,
    /// Whether what is written is coloured.
    paint: Paint,
    /// What kind of run is shown.
    report: Report,
    /// How many targets the build has.
    total: usize,
    /// How many of them have their outcome.
    done: usize,
    /// The actions that run, the one begun first at the front, each with its status text.
    running: Vec<(Label, String)>,
    /// The target whose output is shown as it arrives.
    live: Option<Label>,
    /// Whether the last byte shown of the live target's output was not a newline.
    mid_line: bool,
    /// Whether a status line stands on the last row.
    drawn: bool,
    /// Whether a write failed.
    broken: bool,
}

impl<W: Write> TerminalReporter<W> {
    /// A reporter that writes to `out`, with or without a status line, coloured or not, showing
    /// the kind of run `report` names.
    pub const fn new(out: W, mode: Mode, paint: Paint, report: Report) -> Self {
        Self {
            out,
            mode,
            paint,
            report,
            total: 0,
            done: 0,
            running: Vec::new(),
            live: None,
            mid_line: false,
            drawn: false,
            broken: false,
        }
    }

    /// Erases the status line and ends a line left open.
    ///
    /// For a build that ended in an error before its summary: whatever is written next then
    /// starts on an empty row.
    pub fn close(&mut self) {
        let mut frame = Vec::new();
        self.erase(&mut frame);
        if self.mid_line {
            frame.push(b'\n');
        }
        self.live = None;
        self.mid_line = false;
        self.emit(&frame);
    }

    /// Whether `event` is a test that passed in a run that shows only the tests that failed.
    fn hides(&self, event: &OutcomeEvent) -> bool {
        matches!(
            self.report,
            Report::Test {
                blocks: TestBlocks::Failed,
                ..
            }
        ) && event.outcome.role == TargetRole::Test
            && matches!(
                event.outcome.class,
                OutcomeClass::Built | OutcomeClass::Cached | OutcomeClass::Fetched
            )
    }

    /// Adds to `frame` what erases the status line, when one stands.
    fn erase(&mut self, frame: &mut Vec<u8>) {
        if self.drawn {
            frame.extend_from_slice(ERASE);
            self.drawn = false;
        }
    }

    /// Adds the status line to `frame`, when one is kept and no action is shown live.
    fn draw(&mut self, frame: &mut Vec<u8>) {
        if self.mode == Mode::Live && self.live.is_none() {
            frame.extend_from_slice(WRAP_OFF);
            frame.extend_from_slice(self.status().as_bytes());
            frame.extend_from_slice(WRAP_ON);
            self.drawn = true;
        }
    }

    /// The text of the status line.
    fn status(&self) -> String {
        let progress = format!("[{}/{}]", self.done, self.total);
        match self.running.first() {
            Some((_, text)) => format!("{progress} {} running: {text}", self.running.len()),
            None => progress,
        }
    }

    /// Hands `frame` to the stream in one write and flushes it. Nothing is written for an empty
    /// frame, nor once a write has failed.
    fn emit(&mut self, frame: &[u8]) {
        if frame.is_empty() || self.broken {
            return;
        }
        if self
            .out
            .write_all(frame)
            .and_then(|()| self.out.flush())
            .is_err()
        {
            self.broken = true;
        }
    }
}

impl<W: Write> Reporter for TerminalReporter<W> {
    /// Writes nothing: keeps the number of targets, and forgets what an earlier build left.
    fn begun(&mut self, targets: usize) {
        self.total = targets;
        self.done = 0;
        self.running.clear();
        self.live = None;
        self.mid_line = false;
    }

    /// Counts the action as running, and draws the status line again.
    fn started(&mut self, action: &ReadyAction) {
        let text = action
            .description
            .as_ref()
            .map_or_else(|| action.label.to_string(), ToString::to_string);
        self.running.push((action.label.clone(), text));
        let mut frame = Vec::new();
        self.erase(&mut frame);
        self.draw(&mut frame);
        self.emit(&frame);
    }

    /// Writes the bytes as they are, under the target's header the first time.
    fn printed(&mut self, label: &Label, _channel: Channel, bytes: &[u8]) {
        let mut frame = Vec::new();
        if self.live.as_ref() != Some(label) {
            self.erase(&mut frame);
            frame.extend_from_slice(header(label, self.paint).as_bytes());
            frame.push(b'\n');
            self.live = Some(label.clone());
            self.mid_line = false;
        }
        frame.extend_from_slice(bytes);
        if let Some(last) = bytes.last() {
            self.mid_line = *last != b'\n';
        }
        self.emit(&frame);
    }

    /// Writes the target's block; or, for the target shown live, ends its last line and writes
    /// its trailer. The status line is erased before and drawn again after. A test that passed
    /// in a run that shows only failed tests gets no block, only the status line's redraw.
    fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
        let label = &event.outcome.label;
        self.done += 1;
        self.running.retain(|(running, _)| running != label);
        let mut frame = Vec::new();
        if self.live.as_ref() == Some(label) {
            if self.mid_line {
                frame.push(b'\n');
            }
            frame.extend_from_slice(trailer(event, self.paint).as_bytes());
            self.live = None;
            self.mid_line = false;
        } else {
            self.erase(&mut frame);
            if !self.hides(event) {
                frame.extend_from_slice(&block(event, printed, self.paint));
            }
        }
        self.draw(&mut frame);
        self.emit(&frame);
    }

    /// Erases the status line and writes the summary line: the count of targets for a build, the
    /// count of tests for a test run.
    ///
    /// A test command's summary says nothing of which of its runs it ends, so the counts decide:
    /// a run that held targets and no test is the run of discovery targets that comes first, and
    /// is counted as a build is. The run of the tests always holds a test, or nothing at all, so
    /// the two are never mistaken for each other.
    fn summary(&mut self, summary: Summary) {
        let mut frame = Vec::new();
        self.erase(&mut frame);
        let line = match self.report {
            Report::Test { rerun, .. }
                if summary.tests != Tally::default() || summary.all == Tally::default() =>
            {
                test_line(summary.tests, rerun)
            }
            Report::Build | Report::Test { .. } => summary_line(summary.all),
        };
        frame.extend_from_slice(line.as_bytes());
        self.emit(&frame);
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::path::PathBuf;

    use buildl_core::{
        ActionOutcome, Argument, Captured, Channel, Command, Diagnostic, ExpandedDescription,
        InfraKind, Label, NetworkAccess, OutcomeClass, OutcomeEvent, OutputName, Provenance,
        ReadyAction, Reporter, Rerun, Summary, Tally, TargetRole, Timestamp,
    };

    use super::{Mode, Report, TerminalReporter, TestBlocks, block};
    use crate::report::paint::Paint;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn command(arguments: &[&str]) -> Command {
        Command::new(
            arguments
                .iter()
                .map(|argument| Argument::parse(*argument).unwrap())
                .collect(),
        )
        .unwrap()
    }

    /// The action of `raw`, shown as `description` while it runs.
    fn action(raw: &str, description: Option<&str>) -> ReadyAction {
        ReadyAction {
            label: label(raw),
            command: command(&["cc"]),
            description: description.map(ExpandedDescription::new),
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            network: NetworkAccess::Sealed,
        }
    }

    /// The event of `raw`, declared in the build file of its directory, running `arguments`.
    fn event(raw: &str, class: OutcomeClass, arguments: &[&str]) -> OutcomeEvent {
        let label = label(raw);
        let directory = label.directory().clone();
        let file = if directory.as_str().is_empty() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(directory.as_str()).join("build.lua")
        };
        OutcomeEvent {
            at: Timestamp::UNIX_EPOCH,
            provenance: Provenance::new(file, directory),
            command: command(arguments),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label,
                role: TargetRole::Build,
                class,
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        }
    }

    /// `event` with the message a builder gave for it.
    fn with_diagnostic(mut event: OutcomeEvent, text: &str) -> OutcomeEvent {
        event.outcome.diagnostic = Some(Diagnostic::new(text));
        event
    }

    const fn nothing() -> Captured<'static> {
        Captured {
            stdout: b"",
            stderr: b"",
        }
    }

    const fn on_stdout(bytes: &[u8]) -> Captured<'_> {
        Captured {
            stdout: bytes,
            stderr: b"",
        }
    }

    const fn on_stderr(bytes: &[u8]) -> Captured<'_> {
        Captured {
            stdout: b"",
            stderr: bytes,
        }
    }

    const fn counts(failed: usize, skipped: usize, built: usize, cached: usize) -> Summary {
        Summary {
            all: Tally {
                failed,
                skipped,
                built,
                cached,
                fetched: 0,
            },
            tests: Tally {
                failed: 0,
                skipped: 0,
                built: 0,
                cached: 0,
                fetched: 0,
            },
        }
    }

    fn reporter(mode: Mode, paint: Paint) -> TerminalReporter<Vec<u8>> {
        TerminalReporter::new(Vec::new(), mode, paint, Report::Build)
    }

    /// `event` for a target declared with `b.test`.
    fn test_event(raw: &str, class: OutcomeClass) -> OutcomeEvent {
        let mut event = event(raw, class, &["check"]);
        event.outcome.role = TargetRole::Test;
        event
    }

    /// A plain reporter that shows a test run, giving `blocks` their blocks.
    fn test_reporter(blocks: TestBlocks, rerun: Rerun) -> TerminalReporter<Vec<u8>> {
        TerminalReporter::new(
            Vec::new(),
            Mode::Plain,
            Paint::Plain,
            Report::Test { blocks, rerun },
        )
    }

    /// What a build-role reporter writes for `event` printing `printed`.
    fn shown_by_a_build(event: &OutcomeEvent, printed: Captured<'_>) -> Vec<u8> {
        let mut build = reporter(Mode::Plain, Paint::Plain);
        build.begun(1);
        build.finished(event, printed);
        written(&mut build)
    }

    /// What `reporter` wrote since this was last asked.
    fn written(reporter: &mut TerminalReporter<Vec<u8>>) -> Vec<u8> {
        std::mem::take(&mut reporter.out)
    }

    /// A stream that keeps each write apart and counts its flushes.
    #[derive(Debug, Default)]
    struct Writes {
        writes: Vec<Vec<u8>>,
        flushes: usize,
    }

    impl Write for Writes {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.push(bytes.to_vec());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    /// Makes the calls of a build in which `//:main.o` is found cached with a warning,
    /// `//:docs` cannot be started, `//lib:text` exits 1 and `//:app` is skipped.
    fn report_a_build_with_two_failures<W: Write>(reporter: &mut TerminalReporter<W>) {
        reporter.begun(4);
        reporter.finished(
            &event("//:main.o", OutcomeClass::Cached, &["cc", "-c", "main.c"]),
            on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
        );
        reporter.started(&action("//:docs", None));
        reporter.finished(
            &with_diagnostic(
                event(
                    "//:docs",
                    OutcomeClass::InfraFailure {
                        kind: InfraKind::Run,
                    },
                    &["cc", "--doc", "README", "a b"],
                ),
                "exec strategy: //:docs: the command cannot be started",
            ),
            nothing(),
        );
        reporter.started(&action("//lib:text", Some("compile lib/text.c")));
        reporter.finished(
            &event(
                "//lib:text",
                OutcomeClass::ActionFailure { status: 1 },
                &["cc", "-c", "lib/text.c", "-o", "out/lib/text"],
            ),
            on_stderr(b"lib/text.c:3:1: error: expected ';'\n"),
        );
        reporter.finished(
            &event(
                "//:app",
                OutcomeClass::Skipped {
                    because: label("//:docs"),
                },
                &["cc", "-o", "out/app"],
            ),
            nothing(),
        );
        reporter.summary(counts(2, 1, 0, 1));
    }

    #[test]
    fn a_test_run_hides_a_passing_test_that_printed() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.finished(
            &test_event("//:check", OutcomeClass::Built),
            on_stdout(b"ok\n"),
        );
        assert_eq!(written(&mut reporter), b"");
    }

    #[test]
    fn a_test_run_hides_the_replay_of_a_cached_pass() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.finished(
            &test_event("//:check", OutcomeClass::Cached),
            on_stdout(b"ok\n"),
        );
        assert_eq!(written(&mut reporter), b"");
    }

    #[test]
    fn a_test_run_shows_the_block_of_a_failing_test() {
        let failing = test_event("//:check", OutcomeClass::ActionFailure { status: 1 });
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.finished(&failing, on_stdout(b"not ok\n"));
        let wanted = block(&failing, on_stdout(b"not ok\n"), Paint::Plain);
        assert_ne!(wanted, b"");
        assert_eq!(written(&mut reporter), wanted);
    }

    #[test]
    fn a_test_run_shows_the_block_of_a_test_whose_output_is_missing() {
        let missing = test_event(
            "//:check",
            OutcomeClass::OutputMissing {
                name: OutputName::parse("report.txt").unwrap(),
            },
        );
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.finished(&missing, nothing());
        let wanted = block(&missing, nothing(), Paint::Plain);
        assert_ne!(wanted, b"");
        assert_eq!(written(&mut reporter), wanted);
    }

    #[test]
    fn a_test_run_shows_a_dependency_as_a_build_does() {
        let dependency = event("//:lib", OutcomeClass::Built, &["cc"]);
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.finished(&dependency, on_stdout(b"compiled\n"));
        let shown = written(&mut reporter);
        assert_eq!(shown, b"--- //:lib\ncompiled\n");
        assert_eq!(
            shown,
            shown_by_a_build(&dependency, on_stdout(b"compiled\n"))
        );
    }

    #[test]
    fn a_verbose_test_run_shows_every_test() {
        let mut reporter = test_reporter(TestBlocks::Every, Rerun::Off);
        reporter.begun(2);
        reporter.finished(
            &test_event("//:fresh", OutcomeClass::Built),
            on_stdout(b"ok\n"),
        );
        reporter.finished(
            &test_event("//:stale", OutcomeClass::Cached),
            on_stdout(b"ok\n"),
        );
        assert_eq!(
            String::from_utf8(written(&mut reporter)).unwrap(),
            "--- //:fresh\nok\n--- //:stale (cached)\nok\n"
        );
    }

    #[test]
    fn a_test_run_ends_with_the_test_line() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(2);
        let mut summary = counts(1, 0, 1, 0);
        summary.tests = Tally {
            failed: 1,
            skipped: 0,
            built: 1,
            cached: 0,
            fetched: 0,
        };
        reporter.summary(summary);
        assert_eq!(written(&mut reporter), b"tests: 1 passed, 1 failed\n");
    }

    #[test]
    fn the_discovery_run_of_a_test_command_ends_with_the_target_line() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.summary(counts(0, 0, 1, 0));
        assert_eq!(written(&mut reporter), b"1 built\n");
    }

    #[test]
    fn the_summary_line_counts_fetched() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(3);
        let mut summary = counts(0, 0, 1, 0);
        summary.all.fetched = 2;
        reporter.summary(summary);
        assert_eq!(written(&mut reporter), b"1 built, 2 fetched\n");
    }

    #[test]
    fn a_test_run_of_nothing_ends_with_the_test_line() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(0);
        reporter.summary(counts(0, 0, 0, 0));
        assert_eq!(written(&mut reporter), b"no tests\n");
    }

    #[test]
    fn a_test_run_of_cached_tests_ends_with_the_test_line() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(2);
        let mut summary = counts(0, 0, 0, 2);
        summary.tests = Tally {
            failed: 0,
            skipped: 0,
            built: 0,
            cached: 1,
            fetched: 0,
        };
        reporter.summary(summary);
        assert_eq!(written(&mut reporter), b"tests: 1 passed (1 cached)\n");
    }

    #[test]
    fn a_rerun_test_run_ends_with_the_rerun_line() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Tests);
        reporter.begun(1);
        let mut summary = counts(0, 0, 1, 0);
        summary.tests = Tally {
            failed: 0,
            skipped: 0,
            built: 1,
            cached: 0,
            fetched: 0,
        };
        reporter.summary(summary);
        assert_eq!(written(&mut reporter), b"tests (rerun): 1 passed\n");
    }

    #[test]
    fn a_live_test_run_still_advances_the_status_line_past_a_hidden_pass() {
        let mut reporter = TerminalReporter::new(
            Vec::new(),
            Mode::Live,
            Paint::Plain,
            Report::Test {
                blocks: TestBlocks::Failed,
                rerun: Rerun::Off,
            },
        );
        reporter.begun(2);
        reporter.started(&action("//:check", None));
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/2] 1 running: //:check\x1b[?7h"
        );
        reporter.finished(
            &test_event("//:check", OutcomeClass::Built),
            on_stdout(b"ok\n"),
        );
        assert_eq!(written(&mut reporter), b"\r\x1b[2K\x1b[?7l[1/2]\x1b[?7h");
    }

    #[test]
    fn a_test_shown_live_ends_as_a_live_action_does_even_when_it_passes() {
        let mut reporter = test_reporter(TestBlocks::Failed, Rerun::Off);
        reporter.begun(1);
        reporter.started(&action("//:check", None));
        reporter.printed(&label("//:check"), Channel::Stdout, b"ok");
        assert_eq!(written(&mut reporter), b"--- //:check\nok");
        reporter.finished(&test_event("//:check", OutcomeClass::Built), nothing());
        assert_eq!(written(&mut reporter), b"\n");
    }

    #[test]
    fn a_build_with_failures_is_blocks_then_the_summary() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        report_a_build_with_two_failures(&mut reporter);
        assert_eq!(
            String::from_utf8(written(&mut reporter)).unwrap(),
            concat!(
                "--- //:main.o (cached)\n",
                "main.c:4:9: warning: unused variable 'n'\n",
                "FAILED //:docs: infrastructure failure (the command could not be started)\n",
                "    exec strategy: //:docs: the command cannot be started\n",
                "    declared in build.lua\n",
                "    command: cc --doc README \"a b\"\n",
                "--- //lib:text\n",
                "lib/text.c:3:1: error: expected ';'\n",
                "FAILED //lib:text: action failure (exit status 1)\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
                "2 failed, 1 skipped, 1 cached\n",
            )
        );
    }

    #[test]
    fn plain_never_writes_an_escape() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        report_a_build_with_two_failures(&mut reporter);
        let bytes = written(&mut reporter);
        assert!(bytes.ends_with(b"2 failed, 1 skipped, 1 cached\n"));
        assert!(!bytes.contains(&0x1b));
        assert!(!bytes.contains(&b'\r'));
    }

    #[test]
    fn a_plain_stream_asked_for_colour_gets_colour_and_no_status_line() {
        let mut reporter = reporter(Mode::Plain, Paint::Ansi);
        reporter.begun(1);
        reporter.started(&action("//lib:text", None));
        reporter.finished(
            &event(
                "//lib:text",
                OutcomeClass::ActionFailure { status: 1 },
                &["cc"],
            ),
            on_stderr(b"lib/text.c:3:1: error: expected ';'\n"),
        );
        reporter.summary(counts(1, 0, 0, 0));
        assert_eq!(
            written(&mut reporter),
            concat!(
                "\x1b[1m--- //lib:text\x1b[0m\n",
                "lib/text.c:3:1: error: expected ';'\n",
                "\x1b[1;31mFAILED\x1b[0m //lib:text: action failure (exit status 1)\n",
                "    declared in lib/build.lua\n",
                "    command: cc\n",
                "1 failed\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn a_cached_build_replays_what_was_printed_byte_for_byte() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(4);
        reporter.finished(&event("//:app", OutcomeClass::Cached, &["cc"]), nothing());
        reporter.finished(
            &event("//:docs", OutcomeClass::Cached, &["cc"]),
            on_stdout(b"wrote 3 pages\ncaf\xe9"),
        );
        reporter.finished(
            &event("//:main.o", OutcomeClass::Cached, &["cc"]),
            on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
        );
        reporter.finished(
            &event("//lib:text", OutcomeClass::Cached, &["cc"]),
            nothing(),
        );
        reporter.summary(counts(0, 0, 0, 4));
        assert_eq!(
            written(&mut reporter),
            b"--- //:docs (cached)\n\
              wrote 3 pages\n\
              caf\xe9\n\
              --- //:main.o (cached)\n\
              main.c:4:9: warning: unused variable 'n'\n\
              4 cached\n"
        );
    }

    #[test]
    fn a_build_of_nothing_writes_only_its_summary() {
        let mut reporter = reporter(Mode::Live, Paint::Ansi);
        reporter.begun(0);
        assert_eq!(written(&mut reporter), b"");
        reporter.summary(counts(0, 0, 0, 0));
        assert_eq!(written(&mut reporter), b"nothing to build\n");
    }

    #[test]
    fn the_status_line_counts_what_is_done_and_names_the_action_begun_first() {
        let mut reporter = reporter(Mode::Live, Paint::Plain);
        reporter.begun(4);
        assert_eq!(written(&mut reporter), b"");
        reporter.started(&action("//:main.o", Some("compile main.c")));
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/4] 1 running: compile main.c\x1b[?7h"
        );
        reporter.started(&action("//:docs", None));
        assert_eq!(
            written(&mut reporter),
            b"\r\x1b[2K\x1b[?7l[0/4] 2 running: compile main.c\x1b[?7h"
        );
        reporter.finished(&event("//:main.o", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(
            written(&mut reporter),
            b"\r\x1b[2K\x1b[?7l[1/4] 1 running: //:docs\x1b[?7h"
        );
        reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(written(&mut reporter), b"\r\x1b[2K\x1b[?7l[2/4]\x1b[?7h");
    }

    #[test]
    fn live_erases_before_a_block_and_redraws_after() {
        let mut reporter = reporter(Mode::Live, Paint::Ansi);
        reporter.begun(4);
        reporter.finished(&event("//:docs", OutcomeClass::Cached, &["cc"]), nothing());
        reporter.finished(
            &event("//lib:text", OutcomeClass::Cached, &["cc"]),
            nothing(),
        );
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[1/4]\x1b[?7h\r\x1b[2K\x1b[?7l[2/4]\x1b[?7h"
        );
        reporter.started(&action("//:main.o", Some("compile main.c")));
        reporter.finished(
            &event("//:main.o", OutcomeClass::Built, &["cc"]),
            on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
        );
        assert_eq!(
            written(&mut reporter),
            concat!(
                "\r\x1b[2K\x1b[?7l[2/4] 1 running: compile main.c\x1b[?7h",
                "\r\x1b[2K\x1b[1m--- //:main.o\x1b[0m\n",
                "main.c:4:9: warning: unused variable 'n'\n",
                "\x1b[?7l[3/4]\x1b[?7h",
            )
            .as_bytes()
        );
    }

    #[test]
    fn nothing_is_drawn_after_the_summary() {
        let mut reporter = reporter(Mode::Live, Paint::Plain);
        reporter.begun(1);
        reporter.started(&action("//:app", None));
        reporter.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/1] 1 running: //:app\x1b[?7h\r\x1b[2K\x1b[?7l[1/1]\x1b[?7h"
        );
        reporter.summary(counts(0, 0, 1, 0));
        assert_eq!(written(&mut reporter), b"\r\x1b[2K1 built\n");
        assert!(!reporter.drawn);
    }

    #[test]
    fn each_call_is_one_write_and_one_flush() {
        let mut reporter =
            TerminalReporter::new(Writes::default(), Mode::Live, Paint::Ansi, Report::Build);
        report_a_build_with_two_failures(&mut reporter);
        // One per call that had something to say: four `finished`, two `started`, the summary.
        // `begun` says nothing.
        assert_eq!(reporter.out.writes.len(), 7);
        assert_eq!(reporter.out.flushes, 7);
        assert_eq!(
            reporter.out.writes.first().unwrap(),
            concat!(
                "\x1b[1m--- //:main.o\x1b[0m (cached)\n",
                "main.c:4:9: warning: unused variable 'n'\n",
                "\x1b[?7l[1/4]\x1b[?7h",
            )
            .as_bytes()
        );
        assert_eq!(
            reporter.out.writes.last().unwrap(),
            b"\r\x1b[2K2 failed, 1 skipped, 1 cached\n"
        );
    }

    #[test]
    fn a_call_with_nothing_to_say_writes_nothing() {
        let mut reporter =
            TerminalReporter::new(Writes::default(), Mode::Plain, Paint::Plain, Report::Build);
        reporter.begun(2);
        reporter.started(&action("//:app", None));
        reporter.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
        reporter.finished(
            &event(
                "//:docs",
                OutcomeClass::Skipped {
                    because: label("//:app"),
                },
                &["cc"],
            ),
            nothing(),
        );
        assert_eq!(reporter.out.writes.len(), 0);
        assert_eq!(reporter.out.flushes, 0);
    }

    /// A stream that takes `good` writes, then refuses every one after, counting what it was
    /// asked.
    #[derive(Debug, Default)]
    struct Failing {
        good: usize,
        writes: usize,
        flushes: usize,
        fail_flush: bool,
    }

    impl Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            if self.writes > self.good {
                return Err(io::Error::other("the stream is closed"));
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                return Err(io::Error::other("the stream cannot be flushed"));
            }
            Ok(())
        }
    }

    #[test]
    fn a_failed_write_stops_writing_and_does_not_panic() {
        let failing = Failing {
            good: 1,
            ..Failing::default()
        };
        let mut reporter = TerminalReporter::new(failing, Mode::Live, Paint::Ansi, Report::Build);
        report_a_build_with_two_failures(&mut reporter);
        // The first write is taken, the second is refused, and no third is tried.
        assert_eq!(reporter.out.writes, 2);
        assert_eq!(reporter.out.flushes, 1);
    }

    #[test]
    fn a_failed_flush_stops_writing_too() {
        let failing = Failing {
            good: usize::MAX,
            fail_flush: true,
            ..Failing::default()
        };
        let mut reporter = TerminalReporter::new(failing, Mode::Plain, Paint::Plain, Report::Build);
        report_a_build_with_two_failures(&mut reporter);
        assert_eq!(reporter.out.writes, 1);
        assert_eq!(reporter.out.flushes, 1);
    }

    #[test]
    fn a_live_action_gets_its_header_once_and_its_bytes_as_they_come() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(1);
        reporter.started(&action("//:docs", None));
        assert_eq!(written(&mut reporter), b"");
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
        assert_eq!(written(&mut reporter), b"--- //:docs\npage 1\n");
        reporter.printed(&label("//:docs"), Channel::Stderr, b"1 warn");
        assert_eq!(written(&mut reporter), b"1 warn");
        reporter.printed(&label("//:docs"), Channel::Stderr, b"ing\ncaf\xe9\n");
        assert_eq!(written(&mut reporter), b"ing\ncaf\xe9\n");
        reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(written(&mut reporter), b"");
    }

    #[test]
    fn a_live_action_that_ends_mid_line_gets_a_newline_before_its_trailer() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(1);
        reporter.started(&action("//:docs", None));
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\npage 2");
        assert_eq!(written(&mut reporter), b"--- //:docs\npage 1\npage 2");
        reporter.finished(
            &event(
                "//:docs",
                OutcomeClass::ActionFailure { status: 2 },
                &["cc", "--doc", "README"],
            ),
            nothing(),
        );
        assert_eq!(
            written(&mut reporter),
            concat!(
                "\n",
                "FAILED //:docs: action failure (exit status 2)\n",
                "    declared in build.lua\n",
                "    command: cc --doc README\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn a_live_action_that_ends_mid_line_and_succeeds_gets_only_the_newline() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(1);
        reporter.started(&action("//:docs", None));
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
        reporter.printed(&label("//:docs"), Channel::Stdout, b"");
        assert_eq!(written(&mut reporter), b"--- //:docs\npage 1");
        reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(written(&mut reporter), b"\n");
    }

    #[test]
    fn no_status_line_is_drawn_while_an_action_is_shown_live() {
        let mut reporter = reporter(Mode::Live, Paint::Ansi);
        reporter.begun(3);
        reporter.started(&action("//:docs", Some("write the docs")));
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/3] 1 running: write the docs\x1b[?7h"
        );
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
        assert_eq!(
            written(&mut reporter),
            b"\r\x1b[2K\x1b[1m--- //:docs\x1b[0m\npage 1\n"
        );
        reporter.started(&action("//:main.o", Some("compile main.c")));
        assert_eq!(written(&mut reporter), b"");
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 2\n");
        assert_eq!(written(&mut reporter), b"page 2\n");
        reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[1/3] 1 running: compile main.c\x1b[?7h"
        );
    }

    #[test]
    fn blocks_of_other_actions_follow_the_live_one_and_the_next_is_shown_live() {
        let mut reporter = reporter(Mode::Plain, Paint::Plain);
        reporter.begun(3);
        reporter.started(&action("//:docs", None));
        reporter.started(&action("//:main.o", None));
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1\n");
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 2");
        // `//:main.o` ended meanwhile; the build reports it once the live action has ended.
        reporter.finished(&event("//:docs", OutcomeClass::Built, &["cc"]), nothing());
        reporter.finished(
            &event("//:main.o", OutcomeClass::Built, &["cc"]),
            on_stderr(b"main.c:4:9: warning: unused variable 'n'\n"),
        );
        reporter.started(&action("//lib:text", None));
        reporter.printed(&label("//lib:text"), Channel::Stderr, b"text: done\n");
        reporter.finished(
            &event("//lib:text", OutcomeClass::Built, &["cc"]),
            nothing(),
        );
        reporter.summary(counts(0, 0, 3, 0));
        assert_eq!(
            String::from_utf8(written(&mut reporter)).unwrap(),
            concat!(
                "--- //:docs\n",
                "page 1\n",
                "page 2\n",
                "--- //:main.o\n",
                "main.c:4:9: warning: unused variable 'n'\n",
                "--- //lib:text\n",
                "text: done\n",
                "3 built\n",
            )
        );
    }

    #[test]
    fn close_erases_the_status_line() {
        let mut reporter = reporter(Mode::Live, Paint::Plain);
        reporter.begun(2);
        reporter.started(&action("//:app", None));
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/2] 1 running: //:app\x1b[?7h"
        );
        reporter.close();
        assert_eq!(written(&mut reporter), b"\r\x1b[2K");
        reporter.close();
        assert_eq!(written(&mut reporter), b"");
    }

    #[test]
    fn close_ends_a_line_a_live_action_left_open() {
        let mut reporter = reporter(Mode::Live, Paint::Plain);
        reporter.begun(1);
        reporter.started(&action("//:docs", None));
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/1] 1 running: //:docs\x1b[?7h\r\x1b[2K--- //:docs\npage 1"
        );
        reporter.close();
        assert_eq!(written(&mut reporter), b"\n");
        reporter.close();
        assert_eq!(written(&mut reporter), b"");
    }

    #[test]
    fn a_second_build_starts_from_nothing() {
        let mut reporter = reporter(Mode::Live, Paint::Plain);
        reporter.begun(5);
        reporter.started(&action("//:docs", None));
        reporter.printed(&label("//:docs"), Channel::Stdout, b"page 1");
        reporter.close();
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/5] 1 running: //:docs\x1b[?7h\r\x1b[2K--- //:docs\npage 1\n"
        );
        reporter.begun(2);
        reporter.started(&action("//:app", None));
        assert_eq!(
            written(&mut reporter),
            b"\x1b[?7l[0/2] 1 running: //:app\x1b[?7h"
        );
    }

    #[test]
    fn close_writes_nothing_when_nothing_is_left_open() {
        let mut plain = reporter(Mode::Plain, Paint::Plain);
        plain.begun(1);
        plain.started(&action("//:app", None));
        plain.close();
        assert_eq!(written(&mut plain), b"");

        let mut live = reporter(Mode::Live, Paint::Plain);
        live.begun(1);
        live.finished(&event("//:app", OutcomeClass::Built, &["cc"]), nothing());
        live.summary(counts(0, 0, 1, 0));
        assert_eq!(
            written(&mut live),
            b"\x1b[?7l[1/1]\x1b[?7h\r\x1b[2K1 built\n"
        );
        live.close();
        assert_eq!(written(&mut live), b"");
    }
}
