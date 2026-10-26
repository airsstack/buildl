//! Building with a remote cache, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use core::num::NonZeroUsize;
use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    ActionOutcome, DecodedTree, Digest, EntryName, Error, InfraKind, Isolation, LiveOutput,
    OnFailure, OutcomeClass, OutputName, OutputPath, OutputRefusal, Pipeline, PlatformProperties,
    PlatformPropertyName, PortName, RemoteCall, RemoteExecution, RemoteFailure, RemoteJobs,
    RemoteNotice, RemotePolicy, RemoteSettings, RemoteUrl, RemoteUse, RootPath, Sample, Scope,
    SourcePath, TargetRole, json,
};

use crate::common::{
    Content, Ends, FakePorts, FakeSource, Held, Pace, Reported, Runs, Script, Step, Trace, dir,
    ending, file, label, ports, program, root_only, script, target, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// Where the output of the target `raw` names, called as the target is, is kept.
fn out(raw: &str) -> OutputPath {
    let target = label(raw);
    let name = OutputName::parse(target.name().to_string()).unwrap();
    OutputPath::of(&dir("out"), &target, &name)
}

/// Reading from the remote, and carrying on without it when it fails.
const LENIENT: RemotePolicy = RemotePolicy {
    use_: RemoteUse::Read,
    failure: RemoteFailure::Lenient,
    execution: RemoteExecution::Off,
};

/// Reading from the remote, and failing the build when it fails.
const STRICT: RemotePolicy = RemotePolicy {
    use_: RemoteUse::Read,
    failure: RemoteFailure::Strict,
    execution: RemoteExecution::Off,
};

/// Reading from and writing to the remote, and carrying on without it when it fails.
const WRITING: RemotePolicy = RemotePolicy {
    use_: RemoteUse::ReadWrite,
    failure: RemoteFailure::Lenient,
    execution: RemoteExecution::Off,
};

/// Reading from and writing to the remote, and failing the build when it fails.
const WRITING_STRICT: RemotePolicy = RemotePolicy {
    use_: RemoteUse::ReadWrite,
    failure: RemoteFailure::Strict,
    execution: RemoteExecution::Off,
};

/// A build of the whole workspace under `policy`.
fn build(
    pipeline: &mut Pipeline<FakePorts>,
    policy: RemotePolicy,
) -> Result<Vec<ActionOutcome>, Error> {
    pipeline.set_remote(policy);
    pipeline.build(
        &entry(),
        &Scope::Workspace,
        &BTreeMap::new(),
        OnFailure::Stop,
        LiveOutput::Off,
    )
}

/// The class of the target `raw` names.
fn class(outcomes: &[ActionOutcome], raw: &str) -> OutcomeClass {
    let wanted = label(raw);
    outcomes
        .iter()
        .find(|outcome| outcome.label == wanted)
        .unwrap()
        .class
        .clone()
}

/// Makes the remote hold a successful run of the target `raw` names that left `bytes`.
fn hold(pipeline: &mut Pipeline<FakePorts>, raw: &str, bytes: &[u8]) {
    pipeline
        .ports_mut()
        .remote
        .holds
        .insert(out(raw), Held::of(out(raw), bytes, b"remote log\n"));
}

/// The notices the reporter was handed, in order.
fn notices(pipeline: &Pipeline<FakePorts>) -> Vec<RemoteNotice> {
    pipeline
        .ports()
        .reporter
        .calls
        .iter()
        .filter_map(|call| match call {
            Reported::Remote(notice) => Some(notice.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_target_the_remote_holds_is_fetched_not_run() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Fetched);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), []);
    assert_eq!(
        ports.cas.placed().get(&out("//:a")),
        Some(&Digest::of(b"remote bytes")),
        "the output is in place"
    );
    let row = ports.cache.rows.get(&label("//:a")).unwrap();
    assert_eq!(row.stdout(), Digest::of(b"remote log\n"));
    assert_eq!(ports.remote.lookups().len(), 1);
    assert_eq!(ports.remote.lookups()[0].outputs, [out("//:a")]);
}

#[test]
fn the_row_is_written_only_after_the_content() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");
    build(&mut pipeline, LENIENT).unwrap();
    let ports = pipeline.ports();
    assert_eq!(ports.cache.recorded(), [label("//:a")]);
    for held in [&b"remote bytes"[..], b"remote log\n"] {
        assert!(ports.cas.blob(&Digest::of(held)).is_some());
    }

    let mut refused = root_only(vec![target("a", &[])]);
    hold(&mut refused, "//:a", b"remote bytes");
    refused.ports().cas.fail_put(b"remote bytes".to_vec());
    let outcomes = build(&mut refused, STRICT).unwrap();

    assert_eq!(stored_failure(&outcomes, "//:a"), Some(()));
    let ports = refused.ports();
    assert_eq!(ports.cache.recorded(), [], "no row names content not held");
    assert_eq!(ports.cas.placements(), []);
}

/// `Some(())` when the target `raw` names failed because the builder could not store.
fn stored_failure(outcomes: &[ActionOutcome], raw: &str) -> Option<()> {
    matches!(
        class(outcomes, raw),
        OutcomeClass::InfraFailure {
            kind: InfraKind::Store
        }
    )
    .then_some(())
}

#[test]
fn a_content_store_that_cannot_keep_a_fetched_result_is_not_a_remote_failure() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    hold(&mut pipeline, "//:a", b"first bytes");
    hold(&mut pipeline, "//:b", b"second bytes");
    pipeline.ports().cas.fail_put(b"first bytes".to_vec());

    pipeline.set_remote(LENIENT);
    let outcomes = pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::KeepGoing,
            LiveOutput::Off,
        )
        .unwrap();

    assert_eq!(stored_failure(&outcomes, "//:a"), Some(()));
    assert_eq!(
        class(&outcomes, "//:b"),
        OutcomeClass::Fetched,
        "the remote is still on"
    );
    assert_eq!(
        notices(&pipeline),
        [],
        "the remote has nothing to answer for"
    );
    assert_eq!(pipeline.ports().strategy.ran(), []);
}

#[test]
fn a_content_store_that_cannot_keep_a_fetched_result_fails_a_strict_build_as_a_store_failure() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");
    pipeline.ports().cas.fail_put(b"remote bytes".to_vec());

    let outcomes = build(&mut pipeline, STRICT).unwrap();

    assert_eq!(stored_failure(&outcomes, "//:a"), Some(()));
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn a_miss_runs_the_action() {
    let mut pipeline = root_only(vec![target("a", &[])]);

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:a")), 1);
    assert_eq!(pipeline.ports().remote.lookups().len(), 1);
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn the_off_policy_never_calls_the_remote() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");

    let outcomes = build(&mut pipeline, RemotePolicy::OFF).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert!(pipeline.ports().remote.untouched());
}

#[test]
fn a_fetched_target_is_not_reported_as_started() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");

    build(&mut pipeline, LENIENT).unwrap();

    let calls = &pipeline.ports().reporter.calls;
    assert!(
        calls
            .iter()
            .all(|call| !matches!(call, Reported::Started(..))),
        "{calls:?}"
    );
    assert!(calls.iter().any(|call| matches!(
        call,
        Reported::Finished { label: finished, class: OutcomeClass::Fetched, .. }
            if *finished == label("//:a")
    )));
}

#[test]
fn a_lookup_failure_warns_once_and_turns_the_remote_off() {
    let mut pipeline = root_only(vec![
        target("a", &[]),
        target("b", &[]),
        target("c", &[]),
        target("d", &[]),
    ]);
    let ports = pipeline.ports_mut();
    ports.dispatcher.pace = Pace::Together;
    ports.remote.lookup_fails = true;

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    for raw in ["//:a", "//:b", "//:c", "//:d"] {
        assert_eq!(class(&outcomes, raw), OutcomeClass::Built, "{raw}");
    }
    assert_eq!(pipeline.ports().remote.lookups().len(), 1);
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Lookup, diagnostic }
            if diagnostic.to_string().contains("cannot be reached")
    ));
}

#[test]
fn a_failed_download_warns_once_and_builds_locally() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    hold(&mut pipeline, "//:a", b"first");
    hold(&mut pipeline, "//:b", b"second");
    pipeline.ports_mut().remote.download_fails = true;

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    assert_eq!(pipeline.ports().remote.lookups().len(), 1, "then it is off");
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Download, diagnostic }
            if diagnostic.to_string().contains("cut off")
    ));
    assert_eq!(pipeline.ports().strategy.ran().len(), 2);
}

#[test]
fn bytes_that_differ_from_their_digest_are_not_used() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");
    pipeline
        .ports_mut()
        .remote
        .tampered
        .insert(Digest::of(b"remote bytes"), b"REMOTE BYTES".to_vec());

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:a")), 1);
    assert!(
        pipeline
            .ports()
            .cas
            .blob(&Digest::of(b"REMOTE BYTES"))
            .is_none(),
        "the tampered bytes were not stored"
    );
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Download, diagnostic }
            if diagnostic.to_string().contains("SHA-256")
    ));
}

/// A workspace that declares `//:pkgs`, a discovery target, alone, and `//:a` and `//:b` too once
/// it is handed the result of `//:pkgs`.
fn discovering() -> Pipeline<FakePorts> {
    let discovery = || target_with("pkgs", |target| target.role = TargetRole::Discovery);
    let before = workspace(vec![("", file(vec![discovery()], &[]))]);
    let after = workspace(vec![(
        "",
        file(vec![discovery(), target("a", &[]), target("b", &[])], &[]),
    )]);
    let mut pipeline = Pipeline::new(ports(FakeSource::declaring_from_results(before, after)));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

#[test]
fn a_remote_turned_off_by_the_discovery_run_stays_off_for_the_build_and_warns_once() {
    let mut pipeline = discovering();
    pipeline.ports_mut().remote.lookup_fails = true;

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(
        outcomes.len(),
        3,
        "the discovery target, then the two it declared"
    );
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.class == OutcomeClass::Built)
    );
    assert_eq!(
        pipeline.ports().remote.lookups().len(),
        1,
        "the second run did not ask a remote that failed in the first"
    );
    assert_eq!(notices(&pipeline).len(), 1, "one warning for the command");
}

#[test]
fn a_remote_that_failed_in_one_command_is_asked_again_by_the_next() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    pipeline.ports_mut().remote.lookup_fails = true;
    pipeline.set_remote(LENIENT);

    for raw in ["//:a", "//:b"] {
        pipeline
            .build(
                &entry(),
                &Scope::Targets(BTreeSet::from([label(raw)])),
                &BTreeMap::new(),
                OnFailure::Stop,
                LiveOutput::Off,
            )
            .unwrap();
    }

    assert_eq!(pipeline.ports().remote.lookups().len(), 2);
    assert_eq!(notices(&pipeline).len(), 2, "one warning for each command");
}

#[test]
fn a_strict_lookup_failure_fails_the_build() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    pipeline.ports_mut().remote.lookup_fails = true;

    let error = build(&mut pipeline, STRICT).unwrap_err();

    match error {
        Error::Adapter { port, diagnostic } => {
            assert_eq!(port, PortName::RemoteCache);
            assert!(diagnostic.to_string().contains("lookup"), "{diagnostic}");
        }
        other => unreachable!("expected an adapter error, got {other:?}"),
    }
    assert_eq!(pipeline.ports().strategy.ran(), []);
}

#[test]
fn an_unusable_hit_is_a_remote_failure() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    pipeline.ports_mut().remote.holds.insert(
        out("//:a"),
        Held {
            exit_code: 1,
            ..Held::of(out("//:a"), b"remote bytes", b"")
        },
    );

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(notices(&pipeline).len(), 1);
}

#[test]
fn a_fetched_dependency_gives_dependents_the_keys_a_local_build_would() {
    let items = || vec![target("a", &[]), target("b", &["a"])];
    let mut local = root_only(items());
    script(
        &mut local,
        "//:a",
        Script {
            outputs: BTreeMap::from([(out("//:a"), Content::Holds(b"shared bytes".to_vec()))]),
            ..Script::default()
        },
    );
    build(&mut local, RemotePolicy::OFF).unwrap();

    let mut fetching = root_only(items());
    hold(&mut fetching, "//:a", b"shared bytes");
    let outcomes = build(&mut fetching, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Fetched);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    let key_of = |pipeline: &Pipeline<FakePorts>, raw: &str| {
        let wanted = label(raw);
        pipeline
            .ports()
            .log
            .events
            .iter()
            .find(|event| event.outcome.label == wanted)
            .unwrap()
            .key
    };
    assert!(key_of(&local, "//:b").is_some());
    assert_eq!(key_of(&fetching, "//:b"), key_of(&local, "//:b"));
    assert_eq!(key_of(&fetching, "//:a"), key_of(&local, "//:a"));
}

#[test]
fn a_graph_the_remote_holds_entirely_runs_nothing() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &["a"])]);
    hold(&mut pipeline, "//:a", b"first");
    hold(&mut pipeline, "//:b", b"second");

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Fetched);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Fetched);
    assert_eq!(pipeline.ports().strategy.ran(), []);
    assert_eq!(pipeline.ports().remote.lookups().len(), 2);
}

#[test]
fn remote_on_or_off_gives_identical_keys_and_rows() {
    let items = || vec![target("a", &[]), target("b", &["a"])];
    let mut without = root_only(items());
    let mut with = root_only(items());

    build(&mut without, RemotePolicy::OFF).unwrap();
    build(&mut with, LENIENT).unwrap();

    let canonical = |pipeline: &Pipeline<FakePorts>| {
        let ports = pipeline.ports();
        let rows = json::canonical::to_string(&ports.cache.rows).unwrap();
        let events: Vec<String> = ports
            .log
            .events
            .iter()
            .map(|event| json::canonical::to_string(event).unwrap())
            .collect();
        (rows, events)
    };
    assert_eq!(canonical(&with), canonical(&without));
}

#[test]
fn the_log_records_fetched_with_its_key_and_span() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");

    build(&mut pipeline, LENIENT).unwrap();

    let ports = pipeline.ports();
    let logged = ports.log.events.last().unwrap();
    let row = ports.cache.rows.get(&label("//:a")).unwrap();
    assert_eq!(logged.outcome.class, OutcomeClass::Fetched);
    assert_eq!(logged.key, Some(row.key().digest()));
    let (started, finished) = (logged.started.unwrap(), logged.finished.unwrap());
    assert!(started < finished);
}

#[test]
fn verify_never_calls_the_remote() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");
    build(&mut pipeline, LENIENT).unwrap();
    let lookups = pipeline.ports().remote.lookups().len();
    let downloads = pipeline.ports().remote.downloads().len();

    pipeline
        .verify(&entry(), &Scope::Workspace, &BTreeMap::new(), Sample::All)
        .unwrap();

    assert_eq!(pipeline.ports().remote.lookups().len(), lookups);
    assert_eq!(pipeline.ports().remote.downloads().len(), downloads);
}

#[test]
fn a_target_that_ran_is_uploaded_under_read_write() {
    let mut pipeline = root_only(vec![target("a", &[])]);

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    let uploads = pipeline.ports().remote.uploads();
    assert_eq!(uploads.len(), 1);
    let (action, sent) = &uploads[0];
    assert_eq!(
        pipeline.ports().remote.lookups(),
        std::slice::from_ref(action),
        "the result is stored under the action that was looked up"
    );
    assert_eq!(action.outputs, [out("//:a")]);
    let paths: Vec<&OutputPath> = sent.outputs.iter().map(|output| &output.path).collect();
    assert_eq!(paths, [&out("//:a")]);
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn the_properties_the_workspace_names_are_sent_with_every_lookup_and_upload() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    let properties = PlatformProperties::new(BTreeMap::from([(
        PlatformPropertyName::parse("pool").unwrap(),
        "large".to_owned(),
    )]));
    pipeline.ports_mut().manifest.workspace.remote = Some(
        RemoteSettings::builder(RemoteUrl::parse("grpcs://cache").unwrap())
            .properties(properties.clone())
            .build()
            .unwrap(),
    );

    build(&mut pipeline, WRITING).unwrap();

    let ports = pipeline.ports();
    let lookups = ports.remote.lookups();
    assert_eq!(lookups.len(), 1);
    assert_eq!(lookups[0].properties, properties);
    let uploads = ports.remote.uploads();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].0.properties, properties);
}

#[test]
fn nothing_is_uploaded_under_read() {
    let mut pipeline = root_only(vec![target("a", &[])]);

    build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(pipeline.ports().remote.uploads(), []);
}

#[test]
fn a_failed_action_is_never_uploaded() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    script(&mut pipeline, "//:a", ending(Ends::Exits(1)));

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert!(matches!(
        class(&outcomes, "//:a"),
        OutcomeClass::ActionFailure { status: 1 }
    ));
    assert_eq!(pipeline.ports().remote.uploads(), []);
}

#[test]
fn a_target_restored_from_the_local_cache_is_not_uploaded() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    build(&mut pipeline, WRITING).unwrap();
    assert_eq!(pipeline.ports().remote.uploads().len(), 1);

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Cached);
    assert_eq!(pipeline.ports().remote.uploads().len(), 1);
}

#[test]
fn a_fetched_target_is_not_uploaded_again() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    hold(&mut pipeline, "//:a", b"remote bytes");

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Fetched);
    assert_eq!(pipeline.ports().remote.uploads(), []);
}

#[test]
fn the_upload_happens_before_the_row_is_recorded() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    let trace = Trace::default();
    pipeline.ports_mut().remote.trace = trace.clone();
    pipeline.ports_mut().cache.trace = trace.clone();

    build(&mut pipeline, WRITING).unwrap();

    assert_eq!(
        trace.steps(),
        [Step::Uploaded(out("//:a")), Step::Recorded(label("//:a"))]
    );
}

#[test]
fn an_upload_failure_warns_and_keeps_the_target_built() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    pipeline.ports_mut().remote.upload_fails = true;

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    let ports = pipeline.ports();
    assert_eq!(ports.remote.lookups().len(), 1, "then it is off");
    assert_eq!(ports.remote.uploads().len(), 1, "and nothing more is sent");
    assert!(ports.cache.rows.contains_key(&label("//:a")));
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Upload, diagnostic }
            if diagnostic.to_string().contains("refused")
    ));
}

#[test]
fn a_strict_upload_failure_fails_the_build() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    pipeline.ports_mut().remote.upload_fails = true;

    let error = build(&mut pipeline, WRITING_STRICT).unwrap_err();

    match error {
        Error::Adapter { port, diagnostic } => {
            assert_eq!(port, PortName::RemoteCache);
            assert!(
                diagnostic.to_string().starts_with("upload: "),
                "{diagnostic}"
            );
        }
        other => unreachable!("expected an adapter error, got {other:?}"),
    }
}

#[test]
fn a_remote_that_declines_an_upload_for_a_changed_input_is_only_a_notice_and_stays_on() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    let changed = RootPath::from(&SourcePath::parse("src/main.c").unwrap());
    pipeline.ports_mut().remote.input_changed = Some(changed.clone());

    let outcomes = build(&mut pipeline, WRITING).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    assert_eq!(
        pipeline.ports().remote.uploads().len(),
        2,
        "the call is made for each target, and the adapter declines it"
    );
    assert_eq!(
        pipeline.ports().remote.lookups().len(),
        2,
        "the next target still looks up"
    );
    let told = notices(&pipeline);
    assert_eq!(
        told,
        [
            RemoteNotice::InputChanged {
                label: label("//:a"),
                path: changed.clone(),
            },
            RemoteNotice::InputChanged {
                label: label("//:b"),
                path: changed,
            },
        ]
    );
}

#[test]
fn uploading_under_the_bare_strategy_is_refused_before_anything_runs() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    pipeline.ports_mut().manifest.workspace.execution.strategy = Isolation::Bare;

    let error = build(&mut pipeline, WRITING).unwrap_err();

    assert!(matches!(error, Error::UploadUnsandboxed), "{error:?}");
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.handed(), []);
    assert!(ports.remote.untouched());
    assert!(
        ports.approver.asked.is_empty(),
        "a refused build is not put to the approver"
    );

    let outcomes = build(&mut pipeline, LENIENT).unwrap();
    assert_eq!(
        class(&outcomes, "//:a"),
        OutcomeClass::Built,
        "reading alone is allowed"
    );
}

/// Reading from the remote, sending what it cannot answer to be run there, and carrying on
/// without it when it fails.
fn executing() -> RemotePolicy {
    RemotePolicy {
        execution: RemoteExecution::On {
            jobs: RemoteJobs::default(),
            local: NonZeroUsize::MIN,
        },
        ..LENIENT
    }
}

/// As [`executing`], and failing the build when the remote fails.
fn executing_strictly() -> RemotePolicy {
    RemotePolicy {
        failure: RemoteFailure::Strict,
        ..executing()
    }
}

/// As [`executing`], and sending what ran here to the remote too.
fn executing_and_writing() -> RemotePolicy {
    RemotePolicy {
        use_: RemoteUse::ReadWrite,
        ..executing()
    }
}

/// Makes the remote run the target `raw` names as `run` says.
fn run_there(pipeline: &mut Pipeline<FakePorts>, raw: &str, run: Runs) {
    pipeline.ports_mut().remote.runs.insert(out(raw), run);
}

/// A run on the remote that exited with zero and left `bytes` where the target `raw` names
/// keeps its output.
fn ran(raw: &str, bytes: &[u8]) -> Runs {
    Runs::Ran(Held::of(out(raw), bytes, b"remote log\n"))
}

/// A build of the whole workspace under `policy` that carries on past a failed target.
fn build_keeping_going(
    pipeline: &mut Pipeline<FakePorts>,
    policy: RemotePolicy,
) -> Result<Vec<ActionOutcome>, Error> {
    pipeline.set_remote(policy);
    pipeline.build(
        &entry(),
        &Scope::Workspace,
        &BTreeMap::new(),
        OnFailure::KeepGoing,
        LiveOutput::Off,
    )
}

/// The outcome of the target `raw` names.
fn outcome<'a>(outcomes: &'a [ActionOutcome], raw: &str) -> &'a ActionOutcome {
    let wanted = label(raw);
    outcomes
        .iter()
        .find(|outcome| outcome.label == wanted)
        .unwrap()
}

/// How many times the reporter was told the target `raw` names began.
fn started(pipeline: &Pipeline<FakePorts>, raw: &str) -> usize {
    pipeline
        .ports()
        .reporter
        .calls
        .iter()
        .filter(|call| matches!(call, Reported::Started(began, _) if *began == label(raw)))
        .count()
}

#[test]
fn a_missed_target_is_executed_when_the_policy_executes() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(&mut pipeline, "//:a", ran("//:a", b"remote bytes"));

    let outcomes = build(&mut pipeline, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Remote);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), [], "nothing ran here");
    assert_eq!(ports.strategy.handed(), []);
    assert_eq!(ports.remote.lookups().len(), 1, "it was asked first");
    let executions = ports.remote.executions();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0], ports.remote.lookups()[0]);
    assert_eq!(
        ports.cas.placed().get(&out("//:a")),
        Some(&Digest::of(b"remote bytes")),
        "the output is in place"
    );
    let row = ports.cache.rows.get(&label("//:a")).unwrap();
    assert_eq!(row.stdout(), Digest::of(b"remote log\n"));
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn a_missed_target_runs_locally_when_execution_is_off() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(&mut pipeline, "//:a", ran("//:a", b"remote bytes"));

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:a")), 1);
    assert_eq!(pipeline.ports().remote.executions(), []);
}

#[test]
fn a_cached_answer_ends_fetched() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::Cached(Held::of(out("//:a"), b"cached bytes", b"cached log\n")),
    );

    let outcomes = build(&mut pipeline, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Fetched);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), []);
    assert_eq!(
        ports.cas.placed().get(&out("//:a")),
        Some(&Digest::of(b"cached bytes"))
    );
}

#[test]
fn a_remote_non_zero_exit_is_an_action_failure_with_its_logs() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::Ran(Held {
            exit_code: 3,
            outputs: Vec::new(),
            stderr: b"it broke\n".to_vec(),
            ..Held::of(out("//:a"), b"", b"progress\n")
        }),
    );

    let outcomes = build(&mut pipeline, executing()).unwrap();

    let failed = outcome(&outcomes, "//:a");
    assert_eq!(failed.class, OutcomeClass::ActionFailure { status: 3 });
    assert_eq!(failed.stdout, Some(Digest::of(b"progress\n")));
    assert_eq!(failed.stderr, Some(Digest::of(b"it broke\n")));
    let ports = pipeline.ports();
    assert!(ports.cas.blob(&Digest::of(b"progress\n")).is_some());
    assert!(ports.cas.blob(&Digest::of(b"it broke\n")).is_some());
    assert_eq!(ports.strategy.ran(), [], "a failure there is not run here");
    assert_eq!(ports.cache.recorded(), [], "a failure is not cached");
    assert_eq!(notices(&pipeline), [], "the remote did nothing wrong");
}

#[test]
fn a_remote_timeout_is_a_timeout_and_not_run_locally() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::TimedOut(Some(b"so far\n".to_vec())),
    );

    let outcomes = build(&mut pipeline, executing()).unwrap();

    let failed = outcome(&outcomes, "//:a");
    assert_eq!(
        failed.class,
        OutcomeClass::InfraFailure {
            kind: InfraKind::TimedOut
        }
    );
    assert_eq!(failed.stdout, Some(Digest::of(b"so far\n")));
    assert_eq!(failed.stderr, Some(Digest::of(b"")), "no log kept is empty");
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), []);
    assert_eq!(ports.remote.executions().len(), 1);
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn a_remote_missing_output_is_output_missing_and_the_remote_stays_on() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::Ran(Held {
            outputs: Vec::new(),
            ..Held::of(out("//:a"), b"", b"remote log\n")
        }),
    );
    run_there(&mut pipeline, "//:b", ran("//:b", b"second"));

    let outcomes = build_keeping_going(&mut pipeline, executing()).unwrap();

    assert_eq!(
        class(&outcomes, "//:a"),
        OutcomeClass::OutputMissing {
            name: OutputName::parse("a").unwrap()
        }
    );
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Remote);
    assert_eq!(
        outcome(&outcomes, "//:a").stdout,
        Some(Digest::of(b"remote log\n"))
    );
    let ports = pipeline.ports();
    assert_eq!(
        ports.remote.executions().len(),
        2,
        "still asked for the next"
    );
    assert_eq!(ports.strategy.ran(), []);
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn a_remote_symlink_output_is_refused_naming_it() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::Ran(Held {
            outputs: Vec::new(),
            symlinks: vec![out("//:a")],
            ..Held::of(out("//:a"), b"", b"remote log\n")
        }),
    );

    let outcomes = build(&mut pipeline, executing()).unwrap();

    let refused = outcome(&outcomes, "//:a");
    assert_eq!(
        refused.class,
        OutcomeClass::OutputRefused {
            name: OutputName::parse("a").unwrap(),
            refusal: OutputRefusal::Symlink,
        }
    );
    assert!(
        refused
            .diagnostic
            .as_ref()
            .unwrap()
            .to_string()
            .contains("symbolic link"),
        "{:?}",
        refused.diagnostic
    );
    assert_eq!(pipeline.ports().strategy.ran(), []);
    assert_eq!(notices(&pipeline), []);
}

#[test]
fn a_path_not_asked_for_is_a_failed_call() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(
        &mut pipeline,
        "//:a",
        Runs::Ran(Held {
            outputs: vec![
                (out("//:a"), b"bytes".to_vec()),
                (out("//:elsewhere"), b"other".to_vec()),
            ],
            ..Held::of(out("//:a"), b"", b"remote log\n")
        }),
    );

    let outcomes = build(&mut pipeline, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built, "run here");
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed {
            call: RemoteCall::Execute,
            ..
        }
    ));
}

#[test]
fn an_input_changed_runs_locally_with_a_notice() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    let changed = RootPath::from(&SourcePath::parse("src/main.c").unwrap());
    run_there(&mut pipeline, "//:a", Runs::InputChanged(changed.clone()));
    run_there(&mut pipeline, "//:b", ran("//:b", b"second"));

    let outcomes = build(&mut pipeline, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Remote, "still on");
    assert_eq!(
        notices(&pipeline),
        [RemoteNotice::InputChanged {
            label: label("//:a"),
            path: changed,
        }]
    );
    assert_eq!(started(&pipeline, "//:a"), 1, "reported as begun once");
}

#[test]
fn a_failed_execute_runs_locally_and_turns_the_remote_off() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    pipeline.ports_mut().dispatcher.pace = Pace::Together;
    run_there(&mut pipeline, "//:a", Runs::Fails);

    let outcomes = build(&mut pipeline, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    assert_eq!(
        pipeline.ports().remote.executions().len(),
        1,
        "then it is off"
    );
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Execute, diagnostic }
            if diagnostic.to_string().contains("cannot run")
    ));
    assert_eq!(started(&pipeline, "//:a"), 1, "begun once, not twice");
    assert_eq!(started(&pipeline, "//:b"), 1);
}

#[test]
fn a_strict_failed_execute_fails_the_build() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(&mut pipeline, "//:a", Runs::Fails);

    let error = build(&mut pipeline, executing_strictly()).unwrap_err();

    match error {
        Error::Adapter { port, diagnostic } => {
            assert_eq!(port, PortName::RemoteCache);
            assert!(
                diagnostic.to_string().starts_with("execute: "),
                "{diagnostic}"
            );
        }
        other => unreachable!("expected an adapter error, got {other:?}"),
    }
    assert_eq!(pipeline.ports().strategy.ran(), []);
}

#[test]
fn an_executed_target_is_not_uploaded() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(&mut pipeline, "//:a", ran("//:a", b"remote bytes"));

    let outcomes = build(&mut pipeline, executing_and_writing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Remote);
    assert_eq!(pipeline.ports().remote.uploads(), []);
}

#[test]
fn a_remote_job_takes_no_live_place() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    pipeline.ports_mut().dispatcher.pace = Pace::Together;
    run_there(&mut pipeline, "//:a", ran("//:a", b"remote bytes"));
    pipeline.set_remote(executing());

    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::OneAction,
        )
        .unwrap();

    let shown =
        |raw: &str| {
            pipeline.ports().reporter.calls.iter().any(
                |call| matches!(call, Reported::Printed(printed, ..) if *printed == label(raw)),
            )
        };
    assert!(!shown("//:a"), "a remote job prints nothing live");
    assert!(
        shown("//:b"),
        "the place was free for the local action that came after"
    );
}

#[test]
fn a_target_sent_to_be_executed_is_reported_as_started() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    run_there(&mut pipeline, "//:a", ran("//:a", b"remote bytes"));

    build(&mut pipeline, executing()).unwrap();

    let calls = &pipeline.ports().reporter.calls;
    let began = calls
        .iter()
        .position(|call| matches!(call, Reported::Started(began, _) if *began == label("//:a")));
    let finished = calls.iter().position(|call| {
        matches!(
            call,
            Reported::Finished { label: finished, class: OutcomeClass::Remote, .. }
                if *finished == label("//:a")
        )
    });
    assert!(began.unwrap() < finished.unwrap(), "{calls:?}");
    assert_eq!(started(&pipeline, "//:a"), 1);
}

#[test]
fn an_executed_dependency_gives_dependents_the_keys_a_local_build_would() {
    let items = || vec![target("a", &[]), target("b", &["a"])];
    let mut local = root_only(items());
    script(
        &mut local,
        "//:a",
        Script {
            outputs: BTreeMap::from([(out("//:a"), Content::Holds(b"shared bytes".to_vec()))]),
            ..Script::default()
        },
    );
    build(&mut local, RemotePolicy::OFF).unwrap();

    let mut remotely = root_only(items());
    run_there(&mut remotely, "//:a", ran("//:a", b"shared bytes"));
    let outcomes = build(&mut remotely, executing()).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Remote);
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Built);
    let key_of = |pipeline: &Pipeline<FakePorts>, raw: &str| {
        let wanted = label(raw);
        pipeline
            .ports()
            .log
            .events
            .iter()
            .find(|event| event.outcome.label == wanted)
            .unwrap()
            .key
    };
    assert!(key_of(&local, "//:b").is_some());
    assert_eq!(key_of(&remotely, "//:b"), key_of(&local, "//:b"));
    assert_eq!(key_of(&remotely, "//:a"), key_of(&local, "//:a"));
}

/// A run on the remote of the target `raw` names that exited with zero and left the directory
/// that is its output holding what `tree` decodes to.
fn ran_into(raw: &str, tree: DecodedTree) -> Runs {
    Runs::Ran(Held {
        outputs: Vec::new(),
        directories: vec![(out(raw), tree)],
        ..Held::of(out(raw), b"", b"remote log\n")
    })
}

/// Builds two targets, the first of which the remote ran into a directory holding `tree`, and
/// checks that it ends as a local run leaving the same would, with the remote still on.
fn assert_directory_refused(tree: DecodedTree, refusal: OutputRefusal, said: &str) {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    run_there(&mut pipeline, "//:a", ran_into("//:a", tree));
    run_there(&mut pipeline, "//:b", ran("//:b", b"second"));

    let outcomes = build_keeping_going(&mut pipeline, executing()).unwrap();

    let refused = outcome(&outcomes, "//:a");
    assert_eq!(
        refused.class,
        OutcomeClass::OutputRefused {
            name: OutputName::parse("a").unwrap(),
            refusal,
        }
    );
    let diagnostic = refused.diagnostic.as_ref().unwrap().to_string();
    assert!(diagnostic.contains(said), "{diagnostic}");
    assert_eq!(refused.stdout, Some(Digest::of(b"remote log\n")));
    assert_eq!(class(&outcomes, "//:b"), OutcomeClass::Remote, "still on");
    let ports = pipeline.ports();
    assert_eq!(ports.remote.executions().len(), 2);
    assert_eq!(ports.strategy.ran(), []);
    assert_eq!(notices(&pipeline), [], "the remote did nothing wrong");
}

#[test]
fn a_remote_directory_holding_a_symlink_is_refused_naming_it() {
    assert_directory_refused(
        DecodedTree::Symlink("sub/link".to_owned()),
        OutputRefusal::Symlink,
        "a/sub/link is a symbolic link",
    );
}

#[test]
fn a_remote_directory_holding_an_empty_directory_is_refused() {
    assert_directory_refused(
        DecodedTree::EmptyDirectory("sub/none".to_owned()),
        OutputRefusal::EmptyDirectory,
        "a/sub/none is an empty directory",
    );
    assert_directory_refused(
        DecodedTree::EmptyDirectory(String::new()),
        OutputRefusal::EmptyDirectory,
        "a is an empty directory",
    );
}

#[test]
fn a_held_result_with_a_symlink_in_a_directory_is_a_failed_download() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    pipeline.ports_mut().remote.holds.insert(
        out("//:a"),
        Held {
            outputs: Vec::new(),
            directories: vec![(out("//:a"), DecodedTree::Symlink("link".to_owned()))],
            ..Held::of(out("//:a"), b"", b"remote log\n")
        },
    );

    let outcomes = build(&mut pipeline, LENIENT).unwrap();

    assert_eq!(class(&outcomes, "//:a"), OutcomeClass::Built, "run here");
    let told = notices(&pipeline);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(
        &told[0],
        RemoteNotice::Failed { call: RemoteCall::Download, diagnostic }
            if diagnostic.to_string().contains("a/link")
    ));
}

/// The ways a blob the remote answers a request to run with can fail its check.
#[derive(Debug, Clone, Copy)]
enum Tampered {
    /// An output of a run that exited with zero.
    Output,
    /// The standard output of a run that exited with zero.
    LogOfSuccess,
    /// The standard output of a run that exited with status 3.
    LogOfFailure,
}

/// Two targets, the first of which the remote ran, with a blob that fails its check as
/// `tampered` says, and the second of which it ran correctly.
fn run_with_a_bad_blob(tampered: Tampered) -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    let (run, original, replaced): (_, &[u8], &[u8]) = match tampered {
        Tampered::Output => (
            ran("//:a", b"remote bytes"),
            b"remote bytes",
            b"REMOTE BYTES",
        ),
        Tampered::LogOfSuccess => (
            ran("//:a", b"remote bytes"),
            b"remote log\n",
            b"REMOTE LOG\n",
        ),
        Tampered::LogOfFailure => (
            Runs::Ran(Held {
                exit_code: 3,
                outputs: Vec::new(),
                ..Held::of(out("//:a"), b"", b"progress\n")
            }),
            b"progress\n",
            b"PROGRESS\n",
        ),
    };
    run_there(&mut pipeline, "//:a", run);
    run_there(&mut pipeline, "//:b", ran("//:b", b"second"));
    pipeline
        .ports_mut()
        .remote
        .tampered
        .insert(Digest::of(original), replaced.to_vec());
    pipeline
}

#[test]
fn a_blob_that_fails_its_check_in_an_execute_is_a_failed_call_to_execute() {
    for tampered in [
        Tampered::Output,
        Tampered::LogOfSuccess,
        Tampered::LogOfFailure,
    ] {
        let mut pipeline = run_with_a_bad_blob(tampered);

        let outcomes = build_keeping_going(&mut pipeline, executing()).unwrap();

        assert_eq!(
            class(&outcomes, "//:a"),
            OutcomeClass::Built,
            "{tampered:?}: run here"
        );
        assert_eq!(
            class(&outcomes, "//:b"),
            OutcomeClass::Built,
            "{tampered:?}: the remote is off"
        );
        let ports = pipeline.ports();
        assert_eq!(ports.remote.executions().len(), 1, "{tampered:?}");
        assert_eq!(ports.strategy.runs(&label("//:a")), 1, "{tampered:?}");
        let told = notices(&pipeline);
        assert_eq!(told.len(), 1, "{tampered:?}: {told:?}");
        assert!(
            matches!(
                &told[0],
                RemoteNotice::Failed { call: RemoteCall::Execute, diagnostic }
                    if diagnostic.to_string().contains("SHA-256")
            ),
            "{tampered:?}: {told:?}"
        );
        assert_eq!(started(&pipeline, "//:a"), 1, "{tampered:?}: begun once");
    }
}

#[test]
fn a_strict_blob_that_fails_its_check_in_an_execute_fails_the_build() {
    for tampered in [
        Tampered::Output,
        Tampered::LogOfSuccess,
        Tampered::LogOfFailure,
    ] {
        let mut pipeline = run_with_a_bad_blob(tampered);

        let error = build(&mut pipeline, executing_strictly()).unwrap_err();

        match error {
            Error::Adapter { port, diagnostic } => {
                assert_eq!(port, PortName::RemoteCache, "{tampered:?}");
                let text = diagnostic.to_string();
                assert!(text.starts_with("execute: "), "{tampered:?}: {text}");
                assert!(text.contains("SHA-256"), "{tampered:?}: {text}");
            }
            other => unreachable!("{tampered:?}: expected an adapter error, got {other:?}"),
        }
        assert_eq!(pipeline.ports().strategy.ran(), [], "{tampered:?}");
    }
}
