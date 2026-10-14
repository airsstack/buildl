//! The pieces of text a build's report is made of.
//!
//! Its own file because these are pure renderings: each turns a value a build hands over into
//! the text a person reads, and none of them writes anywhere. The reporter that owns the
//! terminal decides when something is written; what it reads is decided here, so a caller with
//! a reporter of its own can show the same text.
//!
//! Responsibilities:
//!
//! - [`command_text`] — a command on one line.
//! - [`summary_line`] — the line that closes a build.
//! - [`trailer`] — what closes the report of a failed target.
//! - [`block`] — everything shown for one target, and the header it opens with.
//!
//! Non-responsibilities: writing, and anything a terminal needs beyond colour. The status line
//! and the order blocks appear in belong to the reporter.

use buildl_core::{
    Captured, Command, InfraKind, Label, OutcomeClass, OutcomeEvent, OutputRefusal, Summary,
};

use crate::report::paint::Paint;

/// A command on one line: its arguments joined by a space.
///
/// An argument that is empty, or that holds whitespace, a control character, `"` or `\`, is
/// written as a Rust string literal, so that the line shows where each argument begins and
/// ends. Every other argument is written as it is.
#[must_use]
pub fn command_text(command: &Command) -> String {
    let shown: Vec<String> = command
        .arguments()
        .iter()
        .map(|argument| {
            let text = argument.as_str();
            if reads_as_one_word(text) {
                text.to_owned()
            } else {
                format!("{text:?}")
            }
        })
        .collect();
    shown.join(" ")
}

/// Whether `text` can be written unquoted and still be read as one argument.
fn reads_as_one_word(text: &str) -> bool {
    !text.is_empty()
        && !text.chars().any(|letter| {
            letter.is_whitespace() || letter.is_control() || letter == '"' || letter == '\\'
        })
}

/// The line that closes a build: each count that is not zero, as `2 failed, 1 skipped, 3 built,
/// 4 cached`, in that order, and `nothing to build` when every count is zero. Ends with a
/// newline.
#[must_use]
pub fn summary_line(summary: Summary) -> String {
    let counts: Vec<String> = [
        (summary.failed, "failed"),
        (summary.skipped, "skipped"),
        (summary.built, "built"),
        (summary.cached, "cached"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect();
    if counts.is_empty() {
        "nothing to build\n".to_owned()
    } else {
        format!("{}\n", counts.join(", "))
    }
}

/// What the builder could not do, in words.
const fn infra_text(kind: InfraKind) -> &'static str {
    match kind {
        InfraKind::Prepare => "the directory to run it in could not be made",
        InfraKind::Run => "the command could not be started",
        InfraKind::TimedOut => "the command ran longer than the timeout",
        InfraKind::Store => "what it produced could not be stored",
        _ => "the builder could not build it",
    }
}

/// Why an output cannot be kept, in words.
const fn refusal_text(refusal: OutputRefusal) -> &'static str {
    match refusal {
        OutputRefusal::Symlink => "it is or holds a symbolic link",
        OutputRefusal::Special => "it is or holds a device, a socket or a named pipe",
        OutputRefusal::EmptyDirectory => "it is or holds an empty directory",
        OutputRefusal::NotText => "it holds a name that is not valid Unicode",
        _ => "it cannot be put back exactly as it was",
    }
}

/// How a failed target ended, in words; `None` for a target that did not fail: one that was
/// built, cached or skipped.
fn failure_text(class: &OutcomeClass) -> Option<String> {
    match class {
        OutcomeClass::Cached | OutcomeClass::Built | OutcomeClass::Skipped { .. } => None,
        OutcomeClass::ActionFailure { status } => {
            Some(format!("action failure (exit status {status})"))
        }
        OutcomeClass::Killed { signal } => {
            Some(format!("action failure (ended by signal {signal})"))
        }
        OutcomeClass::OutputMissing { name } => Some(format!(
            "action failure (declared output {name} was not produced)"
        )),
        OutcomeClass::OutputRefused { name, refusal } => Some(format!(
            "action failure (output {name} cannot be kept: {})",
            refusal_text(*refusal)
        )),
        OutcomeClass::InfraFailure { kind } => {
            Some(format!("infrastructure failure ({})", infra_text(*kind)))
        }
        _ => Some("a failure this buildl cannot name".to_owned()),
    }
}

/// What closes the report of a failed target; empty for a target that did not fail.
///
/// The first line is `FAILED`, the label and how the target ended. Then, each indented by four
/// spaces: the outcome's diagnostic when it has one, the build file that declared the target,
/// and its command. Every line ends with a newline. Under [`Paint::Ansi`] the word `FAILED` is
/// bold red.
#[must_use]
pub fn trailer(event: &OutcomeEvent, paint: Paint) -> String {
    let Some(ended) = failure_text(&event.outcome.class) else {
        return String::new();
    };
    let mut lines = vec![format!(
        "{} {}: {ended}\n",
        paint.failure("FAILED"),
        event.outcome.label
    )];
    if let Some(diagnostic) = &event.outcome.diagnostic {
        lines.extend(
            diagnostic
                .as_str()
                .lines()
                .map(|line| format!("    {line}\n")),
        );
    }
    lines.push(format!("    declared in {}\n", event.provenance));
    lines.push(format!("    command: {}\n", command_text(&event.command)));
    lines.concat()
}

/// The first line of a target's block, without its newline: `--- ` and the label, bold under
/// [`Paint::Ansi`].
pub(crate) fn header(label: &Label, paint: Paint) -> String {
    paint.header(&format!("--- {label}"))
}

/// Adds `bytes` to `block` as they are, then a newline when they do not end with one.
fn push_lines(block: &mut Vec<u8>, bytes: &[u8]) {
    block.extend_from_slice(bytes);
    if bytes.last().is_some_and(|last| *last != b'\n') {
        block.push(b'\n');
    }
}

/// Everything shown for one target once it has its outcome.
///
/// A target that printed something gets a header, marked ` (cached)` when it did not run, then
/// what it wrote to standard output, then what it wrote to standard error. The bytes are
/// written as they are, valid text or not, and each of the two is given a newline when it does
/// not end with one. A target that failed then gets its [`trailer`], whether or not it printed.
/// A target that printed nothing and did not fail gets nothing: the result is empty.
#[must_use]
pub fn block(event: &OutcomeEvent, printed: Captured<'_>, paint: Paint) -> Vec<u8> {
    let mut block = Vec::new();
    if !(printed.stdout.is_empty() && printed.stderr.is_empty()) {
        block.extend_from_slice(header(&event.outcome.label, paint).as_bytes());
        if matches!(event.outcome.class, OutcomeClass::Cached) {
            block.extend_from_slice(b" (cached)");
        }
        block.push(b'\n');
        push_lines(&mut block, printed.stdout);
        push_lines(&mut block, printed.stderr);
    }
    block.extend_from_slice(trailer(event, paint).as_bytes());
    block
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use buildl_core::{
        ActionOutcome, Argument, Captured, Command, Diagnostic, Directory, InfraKind, Label,
        OutcomeClass, OutcomeEvent, OutputName, OutputRefusal, Provenance, Summary, Timestamp,
    };

    use super::{block, command_text, summary_line, trailer};
    use crate::report::paint::Paint;

    fn command(arguments: &[&str]) -> Command {
        Command::new(
            arguments
                .iter()
                .map(|argument| Argument::parse(*argument).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn an_argument_with_a_space_is_quoted() {
        assert_eq!(
            command_text(&command(&["cc", "--doc", "README", "a b"])),
            r#"cc --doc README "a b""#
        );
    }

    #[test]
    fn an_argument_that_would_not_read_as_one_word_is_a_string_literal() {
        assert_eq!(
            command_text(&command(&["echo", "", "a\tb", r#"say "hi""#, r"a\b"])),
            r#"echo "" "a\tb" "say \"hi\"" "a\\b""#
        );
    }

    #[test]
    fn plain_arguments_are_joined_by_one_space() {
        assert_eq!(
            command_text(&command(&["cc", "-c", "lib/text.c", "-o", "out/lib/text"])),
            "cc -c lib/text.c -o out/lib/text"
        );
        assert_eq!(command_text(&command(&["true"])), "true");
    }

    const fn counts(failed: usize, skipped: usize, built: usize, cached: usize) -> Summary {
        Summary {
            failed,
            skipped,
            built,
            cached,
        }
    }

    #[test]
    fn the_summary_leaves_out_zero_counts() {
        assert_eq!(
            summary_line(counts(2, 1, 0, 1)),
            "2 failed, 1 skipped, 1 cached\n"
        );
        assert_eq!(
            summary_line(counts(1, 2, 0, 1)),
            "1 failed, 2 skipped, 1 cached\n"
        );
        assert_eq!(summary_line(counts(0, 0, 4, 0)), "4 built\n");
        assert_eq!(summary_line(counts(0, 0, 0, 4)), "4 cached\n");
        assert_eq!(summary_line(counts(0, 0, 2, 1)), "2 built, 1 cached\n");
    }

    #[test]
    fn the_summary_names_every_count_in_a_fixed_order() {
        assert_eq!(
            summary_line(counts(1, 2, 3, 4)),
            "1 failed, 2 skipped, 3 built, 4 cached\n"
        );
    }

    #[test]
    fn a_build_of_nothing_says_so() {
        assert_eq!(summary_line(counts(0, 0, 0, 0)), "nothing to build\n");
    }

    /// The event of `//lib:text`, declared in `lib/build.lua`, ending in `class`.
    fn event(class: OutcomeClass, diagnostic: Option<&str>) -> OutcomeEvent {
        OutcomeEvent {
            at: Timestamp::UNIX_EPOCH,
            provenance: Provenance::new(
                PathBuf::from("lib/build.lua"),
                Directory::parse("lib").unwrap(),
            ),
            command: command(&["cc", "-c", "lib/text.c", "-o", "out/lib/text"]),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: Label::parse("//lib:text").unwrap(),
                class,
                diagnostic: diagnostic.map(Diagnostic::new),
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        }
    }

    /// The first line of the trailer of a target that ended in `class`.
    fn first_line(class: OutcomeClass) -> String {
        trailer(&event(class, None), Paint::Plain)
            .lines()
            .next()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn a_target_that_did_not_fail_has_no_trailer() {
        for class in [
            OutcomeClass::Built,
            OutcomeClass::Cached,
            OutcomeClass::Skipped {
                because: Label::parse("//:docs").unwrap(),
            },
        ] {
            assert_eq!(trailer(&event(class, None), Paint::Plain), "");
        }
    }

    #[test]
    fn a_trailer_names_the_class_the_build_file_and_the_command() {
        assert_eq!(
            trailer(
                &event(OutcomeClass::ActionFailure { status: 1 }, None),
                Paint::Plain
            ),
            concat!(
                "FAILED //lib:text: action failure (exit status 1)\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
        );
    }

    #[test]
    fn a_diagnostic_comes_between_the_class_and_the_build_file() {
        let failed = event(
            OutcomeClass::InfraFailure {
                kind: InfraKind::Run,
            },
            Some("exec strategy: //lib:text: the command cannot be started"),
        );
        assert_eq!(
            trailer(&failed, Paint::Plain),
            concat!(
                "FAILED //lib:text: infrastructure failure (the command could not be started)\n",
                "    exec strategy: //lib:text: the command cannot be started\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
        );
    }

    #[test]
    fn every_line_of_a_diagnostic_is_indented() {
        let failed = event(
            OutcomeClass::InfraFailure {
                kind: InfraKind::Store,
            },
            Some("content store: disk full\nwhile storing out/lib/text"),
        );
        assert_eq!(
            trailer(&failed, Paint::Plain),
            concat!(
                "FAILED //lib:text: infrastructure failure (what it produced could not be stored)\n",
                "    content store: disk full\n",
                "    while storing out/lib/text\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
        );
    }

    #[test]
    fn an_action_a_signal_ended_says_which() {
        assert_eq!(
            first_line(OutcomeClass::Killed { signal: 11 }),
            "FAILED //lib:text: action failure (ended by signal 11)"
        );
    }

    #[test]
    fn every_failure_class_has_its_words() {
        let name = OutputName::parse("text").unwrap();
        assert_eq!(
            first_line(OutcomeClass::ActionFailure { status: 3 }),
            "FAILED //lib:text: action failure (exit status 3)"
        );
        assert_eq!(
            first_line(OutcomeClass::OutputMissing { name: name.clone() }),
            "FAILED //lib:text: action failure (declared output text was not produced)"
        );
        for (kind, words) in [
            (
                InfraKind::Prepare,
                "the directory to run it in could not be made",
            ),
            (InfraKind::Run, "the command could not be started"),
            (
                InfraKind::TimedOut,
                "the command ran longer than the timeout",
            ),
            (InfraKind::Store, "what it produced could not be stored"),
        ] {
            assert_eq!(
                first_line(OutcomeClass::InfraFailure { kind }),
                format!("FAILED //lib:text: infrastructure failure ({words})")
            );
        }
        for (refusal, words) in [
            (OutputRefusal::Symlink, "it is or holds a symbolic link"),
            (
                OutputRefusal::Special,
                "it is or holds a device, a socket or a named pipe",
            ),
            (
                OutputRefusal::EmptyDirectory,
                "it is or holds an empty directory",
            ),
            (
                OutputRefusal::NotText,
                "it holds a name that is not valid Unicode",
            ),
        ] {
            assert_eq!(
                first_line(OutcomeClass::OutputRefused {
                    name: name.clone(),
                    refusal,
                }),
                format!("FAILED //lib:text: action failure (output text cannot be kept: {words})")
            );
        }
    }

    #[test]
    fn the_word_failed_is_bold_red_when_painted() {
        assert_eq!(
            trailer(
                &event(OutcomeClass::ActionFailure { status: 1 }, None),
                Paint::Ansi
            ),
            concat!(
                "\x1b[1;31mFAILED\x1b[0m //lib:text: action failure (exit status 1)\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
        );
    }

    const fn printed<'a>(stdout: &'a [u8], stderr: &'a [u8]) -> Captured<'a> {
        Captured { stdout, stderr }
    }

    #[test]
    fn a_silent_success_prints_nothing() {
        for class in [
            OutcomeClass::Built,
            OutcomeClass::Cached,
            OutcomeClass::Skipped {
                because: Label::parse("//:docs").unwrap(),
            },
        ] {
            assert_eq!(
                block(&event(class, None), Captured::default(), Paint::Plain),
                b""
            );
        }
    }

    #[test]
    fn a_block_is_a_header_then_standard_output_then_standard_error() {
        assert_eq!(
            block(
                &event(OutcomeClass::Built, None),
                printed(
                    b"compiled 1 file\n",
                    b"lib/text.c:4:9: warning: unused 'n'\n"
                ),
                Paint::Plain
            ),
            concat!(
                "--- //lib:text\n",
                "compiled 1 file\n",
                "lib/text.c:4:9: warning: unused 'n'\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn a_cached_target_is_marked() {
        assert_eq!(
            block(
                &event(OutcomeClass::Cached, None),
                printed(b"", b"lib/text.c:4:9: warning: unused 'n'\n"),
                Paint::Plain
            ),
            concat!(
                "--- //lib:text (cached)\n",
                "lib/text.c:4:9: warning: unused 'n'\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn a_failure_with_output_has_its_trailer_after_what_it_printed() {
        assert_eq!(
            block(
                &event(OutcomeClass::ActionFailure { status: 1 }, None),
                printed(b"", b"lib/text.c:3:1: error: expected ';'\n"),
                Paint::Plain
            ),
            concat!(
                "--- //lib:text\n",
                "lib/text.c:3:1: error: expected ';'\n",
                "FAILED //lib:text: action failure (exit status 1)\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn a_failure_without_output_still_has_a_trailer() {
        assert_eq!(
            block(
                &event(
                    OutcomeClass::InfraFailure {
                        kind: InfraKind::TimedOut
                    },
                    None
                ),
                Captured::default(),
                Paint::Plain
            ),
            concat!(
                "FAILED //lib:text: infrastructure failure (the command ran longer than the timeout)\n",
                "    declared in lib/build.lua\n",
                "    command: cc -c lib/text.c -o out/lib/text\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn bytes_that_are_not_text_pass_unchanged() {
        let shown = block(
            &event(OutcomeClass::Cached, None),
            printed(b"wrote 3 pages\ncaf\xe9\n", b""),
            Paint::Plain,
        );
        assert_eq!(shown, b"--- //lib:text (cached)\nwrote 3 pages\ncaf\xe9\n");
        let last_line = shown
            .strip_suffix(b"\n")
            .unwrap()
            .rsplit(|byte| *byte == b'\n')
            .next()
            .unwrap();
        assert_eq!(last_line, [99, 97, 102, 233]);
    }

    #[test]
    fn a_last_line_without_a_newline_gets_one() {
        assert_eq!(
            block(
                &event(OutcomeClass::Built, None),
                printed(b"wrote 3 pages\ncaf\xe9", b"1 warning"),
                Paint::Plain
            ),
            b"--- //lib:text\nwrote 3 pages\ncaf\xe9\n1 warning\n"
        );
    }

    #[test]
    fn a_painted_block_has_a_bold_header_and_a_plain_mark() {
        assert_eq!(
            block(
                &event(OutcomeClass::Cached, None),
                printed(b"wrote 3 pages\n", b""),
                Paint::Ansi
            ),
            b"\x1b[1m--- //lib:text\x1b[0m (cached)\nwrote 3 pages\n"
        );
    }
}
