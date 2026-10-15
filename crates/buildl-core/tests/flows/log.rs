//! `pipeline::log`, driven through the history and content-store ports by [`FakeHistory`] and
//! [`FakeCas`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use buildl_core::{
    Argument, Command, ContentStore, Digest, Error, Label, LogLine, LoggedRun, OutcomeClass,
    PortName, Timestamp, pipeline,
};

use crate::common::{FakeCas, FakeHistory};

fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

/// A `Built` event of `//:app` naming `stdout` and `stderr` as what it printed.
fn built(stdout: Digest, stderr: Digest) -> LogLine {
    LogLine::Event(LoggedRun {
        at: Timestamp::from_unix_nanos(1),
        class: OutcomeClass::Built,
        command: Command::new(vec![Argument::parse("true").unwrap()]).unwrap(),
        diagnostic: None,
        file: "build.lua".to_owned(),
        finished: None,
        key: None,
        label: label("//:app"),
        outputs: Vec::new(),
        started: None,
        stderr: Some(stderr),
        stdout: Some(stdout),
    })
}

/// A store holding `out\n` and `err\n`, and their digests.
fn printed() -> (FakeCas, Digest, Digest) {
    let cas = FakeCas::default();
    let stdout = cas.put(b"out\n").unwrap();
    let stderr = cas.put(b"err\n").unwrap();
    (cas, stdout, stderr)
}

#[test]
fn log_returns_both_streams_of_the_last_run() {
    let (cas, stdout, stderr) = printed();
    let history = FakeHistory {
        lines: vec![built(stdout, stderr)],
        broken: false,
    };
    let shown = pipeline::log(&history, &cas, &label("//:app")).unwrap();
    assert_eq!(shown.stdout, b"out\n");
    assert_eq!(shown.stderr, b"err\n");
    assert_eq!(shown.last.run.label, label("//:app"));
}

#[test]
fn a_blob_gone_from_the_store_is_missing_output() {
    let (cas, stdout, stderr) = printed();
    cas.forget(&stderr);
    let history = FakeHistory {
        lines: vec![built(stdout, stderr)],
        broken: false,
    };
    let err = pipeline::log(&history, &cas, &label("//:app")).unwrap_err();
    assert!(matches!(err, Error::MissingOutput { label: named } if named == label("//:app")));
}

#[test]
fn an_empty_log_has_no_run() {
    let history = FakeHistory::default();
    let err = pipeline::log(&history, &FakeCas::default(), &label("//:app")).unwrap_err();
    assert!(matches!(
        err,
        Error::NoRun { label: named, suggestion: None, damaged }
            if named == label("//:app") && damaged.is_empty()
    ));
}

#[test]
fn a_history_that_cannot_be_read_is_its_ports_failure() {
    let history = FakeHistory {
        lines: Vec::new(),
        broken: true,
    };
    let err = pipeline::log(&history, &FakeCas::default(), &label("//:app")).unwrap_err();
    assert!(matches!(
        err,
        Error::Adapter {
            port: PortName::RunHistory,
            ..
        }
    ));
}
