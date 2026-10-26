//! Building with a remote cache, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    ActionOutcome, Digest, EntryName, Error, InfraKind, Isolation, LiveOutput, OnFailure,
    OutcomeClass, OutputName, OutputPath, Pipeline, PlatformProperties, PlatformPropertyName,
    PortName, RemoteCall, RemoteExecution, RemoteFailure, RemoteNotice, RemotePolicy,
    RemoteSettings, RemoteUrl, RemoteUse, RootPath, Sample, Scope, SourcePath, TargetRole, json,
};

use crate::common::{
    Content, Ends, FakePorts, FakeSource, Held, Pace, Reported, Script, Step, Trace, dir, ending,
    file, label, ports, program, root_only, script, target, target_with, workspace,
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
