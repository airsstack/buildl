//! The words `buildl log` writes besides the bytes a run stored.
//!
//! Its own file because these are pure renderings of what the core's history hands over: a time
//! in UTC, the line that heads a last run, and the warning for lines of the log that could not be
//! read. None of them writes anywhere, and none reads the clock.
//!
//! Responsibilities:
//!
//! - [`utc`] — an instant as `YYYY-MM-DD HH:MM:SS UTC`.
//! - [`header`] — the line that heads a last run: the target, how the run ended and when, and
//!   what newer lines had no output.
//! - [`damage`] — the warning for damaged lines of the log, when there are any.
//!
//! Non-responsibilities: writing, and choosing the run, which the core's `history` does.

use std::num::NonZeroUsize;

use buildl_core::{LastRun, OutcomeClass, Timestamp};

use crate::report::block::failure_text;

const NANOS_PER_SECOND: u64 = 1_000_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

/// An instant as `YYYY-MM-DD HH:MM:SS UTC`, for example `2024-02-29 12:00:00 UTC`.
///
/// Whole seconds: the nanoseconds are dropped, never rounded up, so a time is never later than
/// the instant it names.
#[must_use]
pub fn utc(at: Timestamp) -> String {
    let seconds = at.as_unix_nanos() / NANOS_PER_SECOND;
    let (year, month, day) = civil_from_days(seconds / SECONDS_PER_DAY);
    let of_day = seconds % SECONDS_PER_DAY;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// The year, month and day of the day `days` after 1970-01-01, in the proleptic Gregorian
/// calendar.
///
/// Counts in eras of 400 years from 0000-03-01, so the leap day is the last day of its year.
const fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// How a run ended, in words: `built`, `cached`, `fetched`, `skipped ({label} failed)` naming the target
/// that failed, or a failure worded as a build's trailer words it.
fn ended(class: &OutcomeClass) -> String {
    match class {
        OutcomeClass::Built => "built".to_owned(),
        OutcomeClass::Cached => "cached".to_owned(),
        OutcomeClass::Fetched => "fetched".to_owned(),
        OutcomeClass::Skipped { because } => format!("skipped ({because} failed)"),
        _ => failure_text(class).unwrap_or_default(),
    }
}

/// The line that heads a last run: `{label}: {how it ended} at {time}`, with a newline.
///
/// When newer lines for the target stored no output, ` · newer: ` follows and says how the one
/// ended, or, for several, how many there were and how the newest ended.
#[must_use]
pub fn header(last: &LastRun) -> String {
    let newer = match last.newer.as_slice() {
        [] => String::new(),
        [only] => format!(" · newer: {}", ended(only)),
        [newest, ..] => format!(
            " · newer: {} runs without output, newest {}",
            last.newer.len(),
            ended(newest)
        ),
    };
    format!(
        "{}: {} at {}{newer}\n",
        last.run.label,
        ended(&last.run.class),
        utc(last.run.at)
    )
}

/// The warning for damaged lines of the log: `log.jsonl: 2 damaged lines skipped (lines 14,
/// 90)`, with a newline; `None` when no line is damaged.
#[must_use]
pub fn damage(lines: &[NonZeroUsize]) -> Option<String> {
    let numbers = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match lines.len() {
        0 => None,
        1 => Some(format!(
            "log.jsonl: 1 damaged line skipped (line {numbers})\n"
        )),
        count => Some(format!(
            "log.jsonl: {count} damaged lines skipped (lines {numbers})\n"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use buildl_core::{
        ActionOutcome, Argument, Command, Directory, InfraKind, Label, LastRun, LoggedRun,
        OutcomeClass, OutcomeEvent, Provenance, TargetRole, Timestamp,
    };

    use super::{damage, header, utc};
    use crate::report::trailer;

    const fn seconds(whole: u64) -> Timestamp {
        Timestamp::from_unix_nanos(whole * 1_000_000_000)
    }

    #[test]
    fn the_log_words_a_fetched_run_fetched() {
        assert_eq!(
            header(&last(OutcomeClass::Fetched, Vec::new())),
            "//:app: fetched at 2026-10-15 07:03:12 UTC\n"
        );
    }

    #[test]
    fn utc_of_known_instants() {
        assert_eq!(utc(Timestamp::UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(seconds(1_709_208_000)), "2024-02-29 12:00:00 UTC");
        assert_eq!(utc(seconds(1_704_067_199)), "2023-12-31 23:59:59 UTC");
        assert_eq!(
            utc(Timestamp::from_unix_nanos(
                1_792_047_792 * 1_000_000_000 + 999_999_999
            )),
            "2026-10-15 07:03:12 UTC"
        );
    }

    fn skipped() -> OutcomeClass {
        OutcomeClass::Skipped {
            because: Label::parse("//lib:text").unwrap(),
        }
    }

    /// The last run of `//:app`, at 2026-10-15 07:03:12, that ended in `class`, with `newer`
    /// lines after it.
    fn last(class: OutcomeClass, newer: Vec<OutcomeClass>) -> LastRun {
        LastRun {
            run: LoggedRun {
                at: seconds(1_792_047_792),
                class,
                command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
                diagnostic: None,
                file: "build.lua".to_owned(),
                finished: None,
                key: None,
                label: Label::parse("//:app").unwrap(),
                outputs: Vec::new(),
                started: None,
                stderr: None,
                stdout: None,
            },
            newer,
            damaged: Vec::new(),
        }
    }

    #[test]
    fn a_header_names_the_class_and_the_utc_time() {
        assert_eq!(
            header(&last(OutcomeClass::Built, Vec::new())),
            "//:app: built at 2026-10-15 07:03:12 UTC\n"
        );
        assert_eq!(
            header(&last(OutcomeClass::Cached, Vec::new())),
            "//:app: cached at 2026-10-15 07:03:12 UTC\n"
        );
    }

    #[test]
    fn a_header_words_a_failure_as_a_trailer_does() {
        let class = OutcomeClass::ActionFailure { status: 1 };
        let heading = header(&last(class.clone(), Vec::new()));
        let words = heading
            .strip_prefix("//:app: ")
            .and_then(|rest| rest.split(" at ").next())
            .unwrap();
        let event = OutcomeEvent {
            at: Timestamp::UNIX_EPOCH,
            provenance: Provenance::new(PathBuf::from("build.lua"), Directory::parse("").unwrap()),
            command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: Label::parse("//:app").unwrap(),
                role: TargetRole::Build,
                class,
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        };
        let first = trailer(&event, crate::report::Paint::Plain)
            .lines()
            .next()
            .unwrap()
            .to_owned();
        assert_eq!(first, format!("FAILED //:app: {words}"));
        assert_eq!(words, "action failure (exit status 1)");
    }

    #[test]
    fn a_header_notes_one_newer_line_without_output() {
        assert_eq!(
            header(&last(
                OutcomeClass::ActionFailure { status: 1 },
                vec![skipped()]
            )),
            "//:app: action failure (exit status 1) at 2026-10-15 07:03:12 UTC · newer: skipped (//lib:text failed)\n"
        );
    }

    #[test]
    fn a_header_counts_several_newer_lines() {
        assert_eq!(
            header(&last(
                OutcomeClass::Cached,
                vec![
                    OutcomeClass::InfraFailure {
                        kind: InfraKind::Run
                    },
                    skipped(),
                    skipped()
                ]
            )),
            "//:app: cached at 2026-10-15 07:03:12 UTC · newer: 3 runs without output, newest infrastructure failure (the command could not be started)\n"
        );
    }

    fn line(number: usize) -> NonZeroUsize {
        NonZeroUsize::new(number).unwrap()
    }

    #[test]
    fn the_damage_warning_lists_the_lines() {
        assert_eq!(
            damage(&[line(14), line(90)]),
            Some("log.jsonl: 2 damaged lines skipped (lines 14, 90)\n".to_owned())
        );
        assert_eq!(
            damage(&[line(14)]),
            Some("log.jsonl: 1 damaged line skipped (line 14)\n".to_owned())
        );
    }

    #[test]
    fn no_damage_no_warning() {
        assert_eq!(damage(&[]), None);
    }
}
