//! What a verify found as lines for a person.
//!
//! Its own file because it renders a [`Verification`], which no other rendering shows: one line
//! for each target that was looked at, then a line that counts them. A failed re-run opens with
//! the header of a build's block and words how it ended as a build does, then shows the
//! diagnostic and what the action printed.
//!
//! Responsibilities: [`verify_text`].
//!
//! Non-responsibilities: making the checks, which the core's verify does; and choosing the exit
//! status, which belongs to the caller.

use buildl_core::{
    Captured, Check, Diagnostic, EntryChange, FileChange, Label, OutcomeClass, OutputDifference,
    Verification,
};

use crate::report::block::{failure_text, header};
use crate::report::paint::Paint;

/// How a file differed, in words.
const fn file_change_text(change: FileChange) -> &'static str {
    match change {
        FileChange::Bytes => "bytes",
        FileChange::Mode => "mode",
    }
}

/// How an entry of a tree differed, in words.
const fn entry_change_text(change: EntryChange) -> &'static str {
    match change {
        EntryChange::Bytes => "bytes",
        EntryChange::Mode => "mode",
        EntryChange::OnlyCached => "only in the cache",
        EntryChange::OnlyRerun => "only in the re-run",
    }
}

/// The first difference of an output, in words.
fn difference_text(difference: &OutputDifference) -> String {
    match difference {
        OutputDifference::Kind => "a file in one, a directory in the other".to_owned(),
        OutputDifference::File(change) => file_change_text(*change).to_owned(),
        OutputDifference::Entry { path, change } => {
            format!("{path}: {}", entry_change_text(*change))
        }
    }
}

/// Adds `bytes` to `text` as text, then a newline when they do not end with one. Bytes that are
/// not valid UTF-8 are shown as the replacement character.
fn push_printed(text: &mut String, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    text.push_str(&String::from_utf8_lossy(bytes));
    if bytes.last().is_some_and(|last| *last != b'\n') {
        text.push('\n');
    }
}

/// The lines of one failed re-run: the header of a block, how it ended, the diagnostic indented
/// by four spaces, then what the action printed, standard output first.
fn failed_text(
    label: &Label,
    class: &OutcomeClass,
    diagnostic: Option<&Diagnostic>,
    printed: Captured<'_>,
    paint: Paint,
) -> String {
    let failed = paint.failure("FAILED");
    let mut lines = vec![
        header(label, paint),
        failure_text(class).map_or_else(
            || format!("{failed} {label}"),
            |how| format!("{failed} {label}: {how}"),
        ),
    ];
    lines.extend(
        diagnostic
            .map(Diagnostic::as_str)
            .unwrap_or_default()
            .lines()
            .map(|line| format!("    {line}")),
    );
    let mut text = lines.join("\n");
    text.push('\n');
    push_printed(&mut text, printed.stdout);
    push_printed(&mut text, printed.stderr);
    text
}

/// The text for one target, ending with a newline.
fn check_text(label: &Label, check: &Check, paint: Paint) -> String {
    match check {
        Check::Verified => format!("verified {label}\n"),
        Check::Mismatched { output, difference } => format!(
            "{} {label}: output {output} differs: {}\n",
            paint.failure("MISMATCH"),
            difference_text(difference)
        ),
        Check::Failed {
            class,
            diagnostic,
            stdout,
            stderr,
        } => failed_text(
            label,
            class,
            diagnostic.as_ref(),
            Captured { stdout, stderr },
            paint,
        ),
        Check::Skipped { because } => format!(
            "{} {label}: needs {because}, which could not be checked\n",
            paint.failure("skipped")
        ),
        Check::Unplaced { diagnostic } => {
            format!("{} {label}: {diagnostic}\n", paint.failure("unplaced"))
        }
    }
}

/// What a verify found as text: a line for each target that was looked at, in label order, then
/// the line that counts them. Ends with a newline.
///
/// - A target whose re-run reproduced its outputs: `verified //lib:text`.
/// - A target whose re-run differed: `MISMATCH //lib:text: output text differs: bytes`, where
///   the difference is `bytes` or `mode` for a file, `{path}: ` and `bytes`, `mode`, `only in
///   the cache` or `only in the re-run` for an entry of a tree, and `a file in one, a directory
///   in the other` when one is a file and the other a tree.
/// - A target whose re-run failed: the header of a build's block, then how it ended as a build's
///   trailer words it, the diagnostic and what the action printed.
/// - A target that was not re-run because a target it needs could not be checked:
///   `skipped //lib:text: needs //lib:base, which could not be checked`.
/// - A target whose cached outputs could not be put back: `unplaced //lib:text: {diagnostic}`.
///
/// The closing line is `N verified, N mismatched, N failed, N skipped, N not cached, N not
/// sampled`, with every count written. It counts an unplaced target as skipped: neither could be
/// checked. Under [`Paint::Ansi`] the word that opens each of the lines for a mismatch, a failure,
/// a skip and an unplaced target is bold red, as the word `FAILED` is in a build.
#[must_use]
pub fn verify_text(verification: &Verification, paint: Paint) -> String {
    let checked = verification.checked();
    let count =
        |wanted: fn(&Check) -> bool| checked.iter().filter(|(_, check)| wanted(check)).count();
    let mut text: String = checked
        .iter()
        .map(|(label, check)| check_text(label, check, paint))
        .collect();
    text.push_str(&summary(
        &Counts {
            verified: count(|check| matches!(check, Check::Verified)),
            mismatched: count(|check| matches!(check, Check::Mismatched { .. })),
            failed: count(|check| matches!(check, Check::Failed { .. })),
            skipped: count(|check| matches!(check, Check::Skipped { .. } | Check::Unplaced { .. })),
        },
        verification,
    ));
    text
}

/// How many of the checked targets ended each way.
struct Counts {
    /// Targets whose re-run reproduced their outputs.
    verified: usize,
    /// Targets whose re-run differed.
    mismatched: usize,
    /// Targets whose re-run failed.
    failed: usize,
    /// Targets that could not be checked, whether skipped or not put back.
    skipped: usize,
}

/// The line that counts a verify: the `checked` counts, then the targets left out.
fn summary(checked: &Counts, verification: &Verification) -> String {
    let Counts {
        verified,
        mismatched,
        failed,
        skipped,
    } = checked;
    format!(
        "{verified} verified, {mismatched} mismatched, {failed} failed, {skipped} skipped, {} not cached, {} not sampled\n",
        verification.not_cached(),
        verification.not_sampled()
    )
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use buildl_core::{
        Check, Diagnostic, EntryChange, FileChange, Label, OutcomeClass, OutputDifference,
        OutputName, TreePath, Verification,
    };

    use super::verify_text;
    use crate::report::paint::Paint;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn mismatched(difference: OutputDifference) -> Check {
        Check::Mismatched {
            output: OutputName::parse("text").unwrap(),
            difference,
        }
    }

    fn failed(diagnostic: Option<&str>, stdout: &[u8], stderr: &[u8]) -> Check {
        Check::Failed {
            class: OutcomeClass::ActionFailure { status: 2 },
            diagnostic: diagnostic.map(Diagnostic::new),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn each_check_has_its_line() {
        let verification = Verification::new(
            vec![
                (label("//a:ok"), Check::Verified),
                (
                    label("//b:bad"),
                    mismatched(OutputDifference::File(FileChange::Bytes)),
                ),
                (
                    label("//c:late"),
                    Check::Skipped {
                        because: label("//d:gone"),
                    },
                ),
                (
                    label("//d:gone"),
                    Check::Unplaced {
                        diagnostic: Diagnostic::new("content store: blob is missing"),
                    },
                ),
            ],
            0,
            0,
        );
        assert_eq!(
            verify_text(&verification, Paint::Plain),
            concat!(
                "verified //a:ok\n",
                "MISMATCH //b:bad: output text differs: bytes\n",
                "skipped //c:late: needs //d:gone, which could not be checked\n",
                "unplaced //d:gone: content store: blob is missing\n",
                "1 verified, 1 mismatched, 0 failed, 2 skipped, 0 not cached, 0 not sampled\n",
            )
        );
    }

    #[test]
    fn a_file_mismatch_names_bytes_or_mode() {
        for (change, word) in [(FileChange::Bytes, "bytes"), (FileChange::Mode, "mode")] {
            let verification = Verification::new(
                vec![(label("//b:bad"), mismatched(OutputDifference::File(change)))],
                0,
                0,
            );
            let text = verify_text(&verification, Paint::Plain);
            assert_eq!(
                text.lines().next().unwrap(),
                format!("MISMATCH //b:bad: output text differs: {word}")
            );
        }
    }

    #[test]
    fn a_kind_mismatch_says_a_file_met_a_directory() {
        let verification = Verification::new(
            vec![(label("//b:bad"), mismatched(OutputDifference::Kind))],
            0,
            0,
        );
        assert_eq!(
            verify_text(&verification, Paint::Plain)
                .lines()
                .next()
                .unwrap(),
            "MISMATCH //b:bad: output text differs: a file in one, a directory in the other"
        );
    }

    #[test]
    fn a_tree_mismatch_names_the_path_and_the_change() {
        for (change, words) in [
            (EntryChange::Bytes, "bytes"),
            (EntryChange::Mode, "mode"),
            (EntryChange::OnlyCached, "only in the cache"),
            (EntryChange::OnlyRerun, "only in the re-run"),
        ] {
            let verification = Verification::new(
                vec![(
                    label("//b:bad"),
                    mismatched(OutputDifference::Entry {
                        path: TreePath::parse("lib/text.o").unwrap(),
                        change,
                    }),
                )],
                0,
                0,
            );
            assert_eq!(
                verify_text(&verification, Paint::Plain)
                    .lines()
                    .next()
                    .unwrap(),
                format!("MISMATCH //b:bad: output text differs: lib/text.o: {words}")
            );
        }
    }

    #[test]
    fn lines_are_in_label_order() {
        let verification = Verification::new(
            vec![
                (label("//z:last"), Check::Verified),
                (label("//a:first"), Check::Verified),
                (label("//m:mid"), Check::Verified),
            ],
            0,
            0,
        );
        let text = verify_text(&verification, Paint::Plain);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            &lines[..3],
            [
                "verified //a:first",
                "verified //m:mid",
                "verified //z:last"
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_kind() {
        let verification = Verification::new(
            vec![
                (label("//a:1"), Check::Verified),
                (label("//a:2"), Check::Verified),
                (label("//a:3"), Check::Verified),
                (label("//b:1"), mismatched(OutputDifference::Kind)),
                (label("//b:2"), mismatched(OutputDifference::Kind)),
                (label("//c:1"), failed(None, b"", b"")),
                (
                    label("//d:1"),
                    Check::Skipped {
                        because: label("//e:1"),
                    },
                ),
                (
                    label("//e:1"),
                    Check::Unplaced {
                        diagnostic: Diagnostic::new("gone"),
                    },
                ),
            ],
            5,
            6,
        );
        let text = verify_text(&verification, Paint::Plain);
        assert_eq!(
            text.lines().last().unwrap(),
            "3 verified, 2 mismatched, 1 failed, 2 skipped, 5 not cached, 6 not sampled"
        );
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn nothing_to_check_says_so() {
        assert_eq!(
            verify_text(&Verification::new(Vec::new(), 4, 0), Paint::Plain),
            "0 verified, 0 mismatched, 0 failed, 0 skipped, 4 not cached, 0 not sampled\n"
        );
    }

    #[test]
    fn a_failed_re_run_is_worded_as_a_failed_build() {
        let verification = Verification::new(
            vec![(
                label("//lib:text"),
                failed(
                    Some("exec strategy: it broke\nsecond line"),
                    b"compiled 1 file\n",
                    b"error: expected ';'",
                ),
            )],
            0,
            0,
        );
        assert_eq!(
            verify_text(&verification, Paint::Plain),
            concat!(
                "--- //lib:text\n",
                "FAILED //lib:text: action failure (exit status 2)\n",
                "    exec strategy: it broke\n",
                "    second line\n",
                "compiled 1 file\n",
                "error: expected ';'\n",
                "0 verified, 0 mismatched, 1 failed, 0 skipped, 0 not cached, 0 not sampled\n",
            )
        );
    }

    #[test]
    fn a_failure_with_nothing_to_add_is_two_lines() {
        let verification =
            Verification::new(vec![(label("//lib:text"), failed(None, b"", b""))], 0, 0);
        let text = verify_text(&verification, Paint::Plain);
        assert_eq!(
            text.lines().take(2).collect::<Vec<_>>(),
            [
                "--- //lib:text",
                "FAILED //lib:text: action failure (exit status 2)"
            ]
        );
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn a_failure_of_another_class_has_no_trailing_separator() {
        let verification = Verification::new(
            vec![(
                label("//lib:text"),
                Check::Failed {
                    class: OutcomeClass::Cached,
                    diagnostic: None,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
            )],
            0,
            0,
        );
        assert_eq!(
            verify_text(&verification, Paint::Plain)
                .lines()
                .nth(1)
                .unwrap(),
            "FAILED //lib:text"
        );
    }

    #[test]
    fn failure_class_lines_are_painted_as_a_build_paints_a_failure() {
        let verification = Verification::new(
            vec![
                (label("//a:ok"), Check::Verified),
                (label("//b:bad"), mismatched(OutputDifference::Kind)),
                (
                    label("//c:late"),
                    Check::Skipped {
                        because: label("//d:gone"),
                    },
                ),
                (
                    label("//d:gone"),
                    Check::Unplaced {
                        diagnostic: Diagnostic::new("gone"),
                    },
                ),
                (label("//e:run"), failed(None, b"", b"")),
            ],
            0,
            0,
        );
        let text = verify_text(&verification, Paint::Ansi);
        assert!(text.contains("verified //a:ok\n"));
        assert!(text.contains("\x1b[1;31mMISMATCH\x1b[0m //b:bad: "));
        assert!(text.contains("\x1b[1;31mskipped\x1b[0m //c:late: "));
        assert!(text.contains("\x1b[1;31munplaced\x1b[0m //d:gone: gone\n"));
        assert!(text.contains("\x1b[1m--- //e:run\x1b[0m\n\x1b[1;31mFAILED\x1b[0m //e:run: "));
    }
}
