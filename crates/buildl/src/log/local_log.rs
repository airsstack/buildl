//! [`LocalLog`], the build log of a local workspace.
//!
//! Its own file because the log is the one state file that is only ever added to: one line for
//! each target of each build, kept across builds, so the newest line for a target says how its
//! latest build ended. A line cut short by a run that died is removed before the next is added,
//! which keeps every line of the file one whole event.
//!
//! Responsibilities: [`LocalLog`] and its [`EventLog`] implementation.
//!
//! Non-responsibilities: building the events, which the build does; and reading the log back
//! or bounding its size, which nothing here does.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{Diagnostic, Error, EventLog, OutcomeEvent, PortName, Result};

use crate::state::StateLayout;

/// How many bytes one read of the scan for the last newline takes.
const SCAN: usize = 4096;

/// Appends one line per event to `log.jsonl`, across builds.
///
/// Each event is written as its canonical JSON and a newline, in one write, with nothing held
/// back in memory: an event is in the file once [`append`](EventLog::append) has returned.
#[derive(Debug)]
pub struct LocalLog {
    path: PathBuf,
    file: Option<File>,
}

/// The log's failure doing `what` to `path`.
fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::EventLog,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
    }
}

/// Where the whole lines of `file` end: just after its last newline, or at its start when it
/// holds none. The file is read from its end, a piece at a time.
fn end_of_whole_lines(file: &File) -> io::Result<u64> {
    let mut piece = [0_u8; SCAN];
    let mut end = file.metadata()?.len();
    while end > 0 {
        let length = usize::try_from(end).map_or(SCAN, |left| left.min(SCAN));
        let start = end - length as u64;
        let read = &mut piece[..length];
        file.read_exact_at(read, start)?;
        if let Some(newline) = read.iter().rposition(|byte| *byte == b'\n') {
            return Ok(start + newline as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}

/// Opens the log at `path` for appending, making it when it is missing.
///
/// A last line with no newline is removed: the run that was writing it died, so the event was
/// never acknowledged, and a line added after it would join it.
fn open(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o644)
        .open(path)?;
    let whole = end_of_whole_lines(&file)?;
    if whole != file.metadata()?.len() {
        file.set_len(whole)?;
    }
    Ok(file)
}

impl LocalLog {
    /// A log where `layout` keeps it. Touches nothing on disk until the first event.
    #[must_use]
    pub fn new(layout: &StateLayout) -> Self {
        Self {
            path: layout.log(),
            file: None,
        }
    }

    /// The file, opened on first use.
    fn opened(&mut self) -> io::Result<&mut File> {
        let file = match self.file.take() {
            Some(file) => file,
            None => open(&self.path)?,
        };
        Ok(self.file.insert(file))
    }
}

impl EventLog for LocalLog {
    fn append(&mut self, event: &OutcomeEvent) -> Result<()> {
        let mut line = canonical::to_vec(event)?;
        line.push(b'\n');
        self.opened()
            .and_then(|file| file.write_all(&line))
            .map_err(|error| failed("appending to", &self.path, &error))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::PathBuf;

    use buildl_core::{
        ActionOutcome, Argument, Command, Digest, Directory, Error, EventLog, Label, OutcomeClass,
        OutcomeEvent, PortName, Provenance, TargetRole, Timestamp,
    };

    use super::LocalLog;
    use crate::state::StateLayout;

    /// The line the log holds for the built target `//:a`.
    const BUILT_A: &str = concat!(
        r#"{"at":30,"class":"Built","command":["cc"],"diagnostic":null,"file":"build.lua","#,
        r#""finished":20,"#,
        r#""key":"2c70e12b7a0646f92279f427c7b38e7334d8e5389cff167a1dc30e73f826b683","#,
        r#""label":"//:a","outputs":[],"started":10,"#,
        r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
        r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
        "\n"
    );

    /// The line the log holds for the target `//:b`, whose action a signal ended.
    const KILLED_B: &str = concat!(
        r#"{"at":30,"class":{"Killed":{"signal":11}},"command":["cc"],"diagnostic":null,"#,
        r#""file":"build.lua","finished":20,"#,
        r#""key":"2c70e12b7a0646f92279f427c7b38e7334d8e5389cff167a1dc30e73f826b683","#,
        r#""label":"//:b","outputs":[],"started":10,"#,
        r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
        r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
        "\n"
    );

    /// The layout of a new workspace whose state directory exists and is empty.
    fn workspace() -> (tempfile::TempDir, StateLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
        fs::create_dir_all(layout.dir()).unwrap();
        (dir, layout)
    }

    /// How the build of the target `//:<name>` ended, for a target that ran `cc` and printed
    /// nothing.
    fn event(name: &str, class: OutcomeClass) -> OutcomeEvent {
        let nothing = Digest::of(b"");
        OutcomeEvent {
            at: Timestamp::from_unix_nanos(30),
            provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            key: Some(Digest::of(b"key")),
            started: Some(Timestamp::from_unix_nanos(10)),
            finished: Some(Timestamp::from_unix_nanos(20)),
            outcome: ActionOutcome {
                label: Label::parse(&format!("//:{name}")).unwrap(),
                role: TargetRole::Build,
                class,
                diagnostic: None,
                outputs: Vec::new(),
                stdout: Some(nothing),
                stderr: Some(nothing),
            },
        }
    }

    /// The event [`BUILT_A`] is the line of.
    fn built_a() -> OutcomeEvent {
        event("a", OutcomeClass::Built)
    }

    /// The event [`KILLED_B`] is the line of.
    fn killed_b() -> OutcomeEvent {
        event("b", OutcomeClass::Killed { signal: 11 })
    }

    #[test]
    fn nothing_is_written_before_the_first_event() {
        let (_dir, layout) = workspace();

        let _log = LocalLog::new(&layout);

        assert_eq!(fs::read_dir(layout.dir()).unwrap().count(), 0);
    }

    #[test]
    fn each_event_is_one_canonical_line() {
        let (_dir, layout) = workspace();
        let mut log = LocalLog::new(&layout);

        log.append(&built_a()).unwrap();
        assert_eq!(fs::read_to_string(layout.log()).unwrap(), BUILT_A);

        log.append(&killed_b()).unwrap();
        assert_eq!(
            fs::read_to_string(layout.log()).unwrap(),
            [BUILT_A, KILLED_B].concat()
        );
    }

    #[test]
    fn a_second_run_appends() {
        let (_dir, layout) = workspace();
        let mut first = LocalLog::new(&layout);
        first.append(&built_a()).unwrap();
        drop(first);

        let mut second = LocalLog::new(&layout);
        second.append(&killed_b()).unwrap();
        second.append(&built_a()).unwrap();

        assert_eq!(
            fs::read_to_string(layout.log()).unwrap(),
            [BUILT_A, KILLED_B, BUILT_A].concat()
        );
    }

    #[test]
    fn a_cut_last_line_is_removed_before_the_first_append() {
        // A line cut short after whole ones, a cut line longer than one read of the scan, and a
        // file that holds nothing but a cut line.
        let filler = "x".repeat(10_000);
        for (left, kept) in [
            (format!("{BUILT_A}{}", &KILLED_B[..40]), BUILT_A),
            (format!("{BUILT_A}{filler}"), BUILT_A),
            (KILLED_B[..40].to_owned(), ""),
            (filler, ""),
        ] {
            let (_dir, layout) = workspace();
            fs::write(layout.log(), &left).unwrap();
            let mut log = LocalLog::new(&layout);

            log.append(&killed_b()).unwrap();
            log.append(&built_a()).unwrap();

            assert_eq!(
                fs::read_to_string(layout.log()).unwrap(),
                [kept, KILLED_B, BUILT_A].concat()
            );
        }
    }

    #[test]
    fn a_log_that_cannot_be_opened_is_an_adapter_error() {
        let (_dir, layout) = workspace();
        fs::create_dir(layout.log()).unwrap();
        let mut log = LocalLog::new(&layout);

        let error = log.append(&built_a()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::EventLog);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("appending to {}: ", layout.log().display())),
            "{diagnostic}"
        );
    }
}
