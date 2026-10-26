//! Building and verifying through the facade against a remote cache.
//!
//! Each test works on copies of the fixture `verified`, whose manifest is given a `[remote]`
//! table that names the in-memory server of `buildl::remote::test_server`, so what a build sent
//! to the cache and what it asked of it can be read back from the server. The fixture's
//! `stamp` target writes its process id, so the byte-identical outputs of a fetched build show
//! the bytes came from the cache and were not made again.

use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;

use buildl::RunOptions;
use buildl::approver::NoTerminal;
use buildl::buildl_core::{
    ActionOutcome, Captured, Channel, Error, Label, LiveOutput, OnFailure, OutcomeClass,
    OutcomeEvent, ReadyAction, RemoteCall, RemoteFailure, RemoteNotice, RemotePolicy, RemoteUse,
    Reporter, Result, Sample, Scope, Summary,
};
use buildl::remote::test_server::TestServer;
use buildl::report::Silent;

use crate::common::Fixture;

/// Results looked up and downloaded, and what a run produced uploaded; a failed call is a
/// notice and the build goes on.
const UPLOADING: RemotePolicy = RemotePolicy {
    use_: RemoteUse::ReadWrite,
    failure: RemoteFailure::Lenient,
};

/// Results looked up and downloaded, and nothing uploaded.
const READING: RemotePolicy = RemotePolicy {
    use_: RemoteUse::Read,
    failure: RemoteFailure::Lenient,
};

/// A copy of `verified` whose manifest names the remote cache at `url`.
fn checkout(url: &str) -> Fixture {
    let fixture = Fixture::copy_of("verified", &[]);
    fixture.append(
        "buildl.toml",
        format!("\n[remote]\nurl = \"{url}\"\n").as_bytes(),
    );
    fixture
}

/// The address of `server` as a `[remote]` url.
fn url_of(server: &TestServer) -> String {
    format!("grpc://{}", server.address())
}

/// What a build told its reporter.
#[derive(Default)]
struct Recorder {
    started: usize,
    notices: Vec<RemoteNotice>,
}

impl Reporter for Recorder {
    fn begun(&mut self, _targets: usize) {}

    fn started(&mut self, _action: &ReadyAction) {
        self.started += 1;
    }

    fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

    fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}

    fn remote(&mut self, notice: &RemoteNotice) {
        self.notices.push(notice.clone());
    }

    fn summary(&mut self, _summary: Summary) {}
}

/// Builds every target of `fixture` under `policy`.
fn build(
    fixture: &Fixture,
    policy: RemotePolicy,
    recorder: &mut Recorder,
) -> Result<Vec<ActionOutcome>> {
    fixture.open().build(
        &Scope::Workspace,
        &BTreeMap::new(),
        RunOptions::new(OnFailure::Stop, LiveOutput::Off).with_remote(policy),
        recorder,
        &mut NoTerminal,
    )
}

/// Whether every target ended as `class`.
fn all_ended(outcomes: &[ActionOutcome], class: &OutcomeClass) -> bool {
    outcomes.iter().all(|outcome| &outcome.class == class)
}

#[test]
fn a_second_checkout_fetches_what_the_first_uploaded() {
    let server = TestServer::start().unwrap();
    let first = checkout(&url_of(&server));
    let second = checkout(&url_of(&server));

    let mut uploading = Recorder::default();
    let built = build(&first, UPLOADING, &mut uploading).unwrap();
    assert!(all_ended(&built, &OutcomeClass::Built), "{built:?}");
    assert!(uploading.notices.is_empty(), "{:?}", uploading.notices);

    let mut reading = Recorder::default();
    let fetched = build(&second, READING, &mut reading).unwrap();

    assert_eq!(fetched.len(), built.len());
    assert!(all_ended(&fetched, &OutcomeClass::Fetched), "{fetched:?}");
    assert_eq!(reading.started, 0, "an action ran");
    assert!(reading.notices.is_empty(), "{:?}", reading.notices);
    assert_eq!(first.files("out"), second.files("out"));
    for path in first.files("out") {
        assert_eq!(
            fs::read(first.root().join(&path)).unwrap(),
            fs::read(second.root().join(&path)).unwrap(),
            "{path} differs"
        );
    }
}

#[test]
fn a_server_that_is_down_warns_and_builds_locally() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("grpc://{}", closed.local_addr().unwrap());
    drop(closed);
    let fixture = checkout(&url);

    let mut recorder = Recorder::default();
    let built = build(&fixture, UPLOADING, &mut recorder).unwrap();

    assert!(all_ended(&built, &OutcomeClass::Built), "{built:?}");
    assert_eq!(recorder.notices.len(), 1, "{:?}", recorder.notices);
    assert!(
        matches!(
            recorder.notices[0],
            RemoteNotice::Failed {
                call: RemoteCall::Lookup,
                ..
            }
        ),
        "{:?}",
        recorder.notices
    );
}

#[test]
fn a_missing_token_fails_before_anything_runs() {
    let fixture = checkout("grpcs://127.0.0.1:1");
    fixture.append(
        "buildl.toml",
        b"\n[remote.token]\nenv = \"BUILDL_E2E_TOKEN_THAT_IS_NOT_SET\"\n",
    );

    let mut recorder = Recorder::default();
    let error = build(&fixture, READING, &mut recorder).unwrap_err();

    assert!(matches!(error, Error::RemoteToken { .. }), "{error:?}");
    assert_eq!(recorder.started, 0);
    assert!(!fixture.state().log().exists(), "an outcome was logged");
    assert_eq!(fixture.files("out"), Vec::<String>::new());
}

#[test]
fn verify_never_connects() {
    let server = TestServer::start().unwrap();
    let fixture = checkout(&url_of(&server));
    fixture.build();

    let verification = fixture
        .open()
        .verify(
            &Scope::Workspace,
            &BTreeMap::new(),
            Sample::All,
            &mut NoTerminal,
        )
        .unwrap();

    assert_eq!(verification.checked().len(), 4, "{verification:?}");
    assert!(
        server.calls().is_empty(),
        "verify called the server: {:?}",
        server.calls()
    );
}

#[test]
fn a_build_given_no_remote_policy_never_connects() {
    let server = TestServer::start().unwrap();
    let fixture = checkout(&url_of(&server));

    fixture
        .open()
        .build(
            &Scope::Workspace,
            &BTreeMap::new(),
            RunOptions::new(OnFailure::Stop, LiveOutput::Off),
            &mut Silent,
            &mut NoTerminal,
        )
        .unwrap();

    assert!(server.calls().is_empty(), "{:?}", server.calls());
}
