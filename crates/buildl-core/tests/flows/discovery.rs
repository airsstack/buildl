//! The commands that run nothing, over a workspace with a discovery target: they declare from the
//! last result the log and the store hold, driven through the ports by the fakes.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    Argument, Command, ContentStore, Digest, Discovered, EntryName, Error, FileMode, Label,
    LiveOutput, LogLine, LoggedRun, OnFailure, OutcomeClass, OutputManifest, OutputName, Pipeline,
    Produced, Sample, Scope, StagedItem, StoredFile, TargetGraph, TargetRole, Timestamp, Written,
    pipeline,
};

use crate::common::{
    FakeCas, FakeDispatcher, FakeFile, FakeHistory, FakeManifest, FakePorts, FakeSource, file,
    label, open_workspace, ports, program, target, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// `//:pkgs`, a discovery target.
fn discovery() -> StagedItem {
    target_with("pkgs", |target| target.role = TargetRole::Discovery)
}

/// A discovery target named `name`.
fn discovery_named(name: &str) -> StagedItem {
    target_with(name, |target| target.role = TargetRole::Discovery)
}

/// A root build file making `items`.
fn root(items: Vec<StagedItem>) -> BTreeMap<buildl_core::Directory, FakeFile> {
    workspace(vec![("", file(items, &[]))])
}

/// What the build files declare before any discovery result is read.
fn before() -> BTreeMap<buildl_core::Directory, FakeFile> {
    root(vec![discovery()])
}

/// What the build files declare once `//:pkgs` has a result: `//:a` and `//:b` as well.
fn after() -> BTreeMap<buildl_core::Directory, FakeFile> {
    root(vec![discovery(), target("a", &[]), target("b", &[])])
}

/// A source that declares `//:pkgs` alone, and `//:a` and `//:b` too once it is handed a result.
fn declaring() -> FakeSource {
    FakeSource::declaring_from_results(before(), after())
}

/// A successful run of `raw` that left `bytes` as its one output, stored in `cas`.
fn recorded(cas: &FakeCas, raw: &str, bytes: &[u8]) -> LogLine {
    let file = cas.put(bytes).unwrap();
    let manifest = OutputManifest::File(StoredFile {
        digest: file,
        mode: FileMode::Plain,
    });
    let manifest = cas.put(&manifest.to_bytes().unwrap()).unwrap();
    LogLine::Event(LoggedRun {
        at: Timestamp::from_unix_nanos(1),
        class: OutcomeClass::Built,
        command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
        diagnostic: None,
        file: "build.lua".to_owned(),
        finished: None,
        key: None,
        label: label(raw),
        outputs: vec![Produced {
            name: OutputName::parse("pkgs.json").unwrap(),
            digest: manifest,
        }],
        started: None,
        stderr: None,
        stdout: None,
    })
}

/// A log and a store holding the one result `//:pkgs` last produced.
fn with_a_result() -> (FakeHistory, FakeCas) {
    let cas = FakeCas::default();
    let history = FakeHistory {
        lines: vec![recorded(&cas, "//:pkgs", b"[\"a\",\"b\"]")],
        broken: false,
    };
    (history, cas)
}

/// What `with_a_result` hands a second load.
fn the_result() -> Discovered {
    Discovered::new(BTreeMap::from([(
        label("//:pkgs"),
        b"[\"a\",\"b\"]".to_vec(),
    )]))
}

fn labels_of(graph: &TargetGraph) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .map(|node| node.label.to_string())
        .collect()
}

fn graph_of(
    source: &FakeSource,
    history: &FakeHistory,
    cas: &FakeCas,
) -> Result<(TargetGraph, Vec<Label>), Error> {
    pipeline::graph(
        source,
        &mut FakeDispatcher::default(),
        &entry(),
        history,
        cas,
    )
}

fn checked_with(
    source: &FakeSource,
    history: &FakeHistory,
    cas: &FakeCas,
) -> Result<buildl_core::Loaded, Error> {
    pipeline::check(
        source,
        &mut FakeDispatcher::default(),
        &entry(),
        history,
        cas,
    )
}

/// A pipeline over `source` that can find `cc`.
fn pipeline_over(source: FakeSource) -> Pipeline<FakePorts> {
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

/// Builds `raw` in `pipeline`.
fn build_named(pipeline: &mut Pipeline<FakePorts>, raw: &[&str]) {
    pipeline
        .build(
            &entry(),
            &Scope::Targets(raw.iter().map(|raw| label(raw)).collect()),
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();
}

#[test]
fn no_discovery_means_one_load() {
    let source = FakeSource::new(root(vec![target("app", &[])]));
    let (history, cas) = (FakeHistory::default(), FakeCas::default());

    let (graph, missing) = graph_of(&source, &history, &cas).unwrap();

    assert_eq!(labels_of(&graph), ["//:app"]);
    assert_eq!(missing, Vec::<Label>::new());
    assert_eq!(source.root_loads(), [Discovered::empty()]);

    let mut pipeline = pipeline_over(FakeSource::new(root(vec![target("app", &[])])));
    let planned = pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
    assert_eq!(planned.stale_discovery(), Vec::<Label>::new());
    assert_eq!(pipeline.ports().source.root_loads(), [Discovered::empty()]);
}

#[test]
fn graph_includes_targets_declared_from_last_results() {
    let (history, cas) = with_a_result();
    let source = declaring();

    let (graph, missing) = graph_of(&source, &history, &cas).unwrap();

    assert_eq!(labels_of(&graph), ["//:a", "//:b", "//:pkgs"]);
    assert_eq!(missing, Vec::<Label>::new());
    assert_eq!(source.root_loads(), [Discovered::empty(), the_result()]);
}

#[test]
fn check_loads_pass_two_twice_and_pass_one_once() {
    let (history, cas) = with_a_result();
    let source = declaring();

    let loaded = checked_with(&source, &history, &cas).unwrap();

    let names: Vec<&str> = loaded
        .declarations
        .iter()
        .map(|declaration| declaration.item().name())
        .collect();
    assert_eq!(names, ["a", "b", "pkgs"]);
    assert_eq!(loaded.missing, Vec::<Label>::new());
    assert_eq!(
        source.root_loads(),
        [Discovered::empty(), the_result(), the_result()]
    );
}

#[test]
fn a_discovery_added_in_pass_two_is_refused() {
    let (history, cas) = with_a_result();
    let more = root(vec![discovery(), discovery_named("more")]);
    let source = || FakeSource::declaring_from_results(before(), more.clone());

    match graph_of(&source(), &history, &cas).unwrap_err() {
        Error::DiscoveryChanged { label, provenance } => {
            assert_eq!(label.to_string(), "//:more");
            assert_eq!(provenance.file(), std::path::Path::new("build.lua"));
        }
        other => unreachable!("expected DiscoveryChanged, got {other:?}"),
    }
    assert!(matches!(
        checked_with(&source(), &history, &cas).unwrap_err(),
        Error::DiscoveryChanged { .. }
    ));
}

#[test]
fn a_discovery_changed_in_pass_two_is_refused() {
    let (history, cas) = with_a_result();
    let changed = root(vec![target_with("pkgs", |target| {
        target.role = TargetRole::Discovery;
        target.env = vec![Written::new("HOME")];
    })]);
    let source = FakeSource::declaring_from_results(before(), changed);

    match graph_of(&source, &history, &cas).unwrap_err() {
        Error::DiscoveryChanged { label, .. } => assert_eq!(label.to_string(), "//:pkgs"),
        other => unreachable!("expected DiscoveryChanged, got {other:?}"),
    }
}

#[test]
fn missing_results_are_reported_and_declare_nothing_more() {
    let (history, cas) = (FakeHistory::default(), FakeCas::default());

    let (graph, missing) = graph_of(&declaring(), &history, &cas).unwrap();
    assert_eq!(labels_of(&graph), ["//:pkgs"]);
    assert_eq!(missing, [label("//:pkgs")]);

    let loaded = checked_with(&declaring(), &history, &cas).unwrap();
    assert_eq!(loaded.declarations.len(), 1);
    assert_eq!(loaded.missing, [label("//:pkgs")]);

    let unreadable = FakeHistory {
        lines: Vec::new(),
        broken: true,
    };
    let loaded = checked_with(&declaring(), &unreadable, &cas).unwrap();
    assert_eq!(loaded.declarations.len(), 1);
    assert_eq!(loaded.missing, [label("//:pkgs")]);
}

#[test]
fn gc_keeps_rows_of_targets_declared_from_discovery() {
    let mut pipeline = pipeline_over(FakeSource::new(root(vec![
        discovery(),
        target("a", &[]),
        target("b", &[]),
    ])));
    build_named(&mut pipeline, &["//:pkgs", "//:a", "//:b"]);

    let ports = pipeline.ports_mut();
    let collected = pipeline::gc(
        &declaring(),
        &mut FakeDispatcher::default(),
        &entry(),
        &mut ports.cache,
        &ports.cas,
        &ports.cas,
        &ports.log,
    )
    .unwrap();

    assert_eq!(collected.rows_dropped, 0);
    let rows: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
    assert_eq!(rows, ["//:a", "//:b", "//:pkgs"]);
}

#[test]
fn plan_lists_a_dirty_discovery_target_as_stale() {
    let mut pipeline = pipeline_over(declaring());

    let fresh = pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
    assert_eq!(fresh.stale_discovery(), [label("//:pkgs")]);
    assert_eq!(fresh.len(), 0);

    build_named(&mut pipeline, &["//:pkgs"]);
    let built = pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
    assert_eq!(built.stale_discovery(), Vec::<Label>::new());
    assert_eq!(built.len(), 2);
}

#[test]
fn doctor_wants_include_discovered_targets() {
    let declares_env = root(vec![
        discovery(),
        target_with("a", |target| target.env = vec![Written::new("HOME")]),
    ]);
    let source = || FakeSource::declaring_from_results(before(), declares_env.clone());
    let manifest = FakeManifest {
        workspace: open_workspace(),
    };
    let doctor = |history: &FakeHistory, cas: &FakeCas| {
        pipeline::doctor(
            &source(),
            &mut FakeDispatcher::default(),
            &manifest,
            &entry(),
            history,
            cas,
        )
        .unwrap()
    };

    let (history, cas) = with_a_result();
    let (checkup, missing) = doctor(&history, &cas);
    assert_eq!(missing, Vec::<Label>::new());
    let home = buildl_core::EnvName::parse("HOME").unwrap();
    assert_eq!(checkup.wanted().env, BTreeSet::from([home]));

    let (checkup, missing) = doctor(&FakeHistory::default(), &FakeCas::default());
    assert_eq!(missing, [label("//:pkgs")]);
    assert!(checkup.wanted().env.is_empty());
}

#[test]
fn verify_checks_targets_declared_from_last_results() {
    let mut pipeline = pipeline_over(FakeSource::new(root(vec![
        discovery(),
        target("a", &[]),
        target("b", &[]),
    ])));
    build_named(&mut pipeline, &["//:pkgs", "//:a", "//:b"]);
    pipeline.ports_mut().source = declaring();

    let verification = pipeline
        .verify(&entry(), &Scope::Workspace, &BTreeMap::new(), Sample::All)
        .unwrap();

    let checked: Vec<String> = verification
        .checked()
        .iter()
        .map(|(label, _)| label.to_string())
        .collect();
    assert_eq!(checked, ["//:a", "//:b"]);
}
