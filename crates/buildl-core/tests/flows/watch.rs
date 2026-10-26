//! `Pipeline::build_watched`, driven through every port by the fakes of [`crate::common`]: a build
//! that also hands back the graph and the input set it ran on.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    Built, Digest, EntryName, Error, EvaluationFailure, FileStat, LiveOutput, OnFailure,
    OutcomeClass, Pipeline, Scope, SourcePath, TargetRole, Timestamp, Watched, Written, json,
};

use crate::common::{
    Ends, FakeContent, FakeFile, FakePorts, FakeSource, classes, ending, file, label, ports,
    program, root_only, script, target, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn path(raw: &str) -> SourcePath {
    SourcePath::parse(raw).unwrap()
}

/// Gives the workspace file `raw` content, for the digester that hashes it and the actions that
/// read it.
fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str) {
    let ports = pipeline.ports_mut();
    ports.digester.files.insert(
        path(raw),
        FakeContent {
            bytes: raw.as_bytes().to_vec(),
            stat: FileStat {
                modified: Timestamp::from_unix_nanos(100),
                size: u64::try_from(raw.len()).unwrap(),
            },
        },
    );
    ports
        .strategy
        .files
        .insert(path(raw), raw.as_bytes().to_vec());
}

/// `//:gen` and `//:app` over it, each reading one file, and `//:docs`, which stands alone.
fn chain() -> Pipeline<FakePorts> {
    let reading = |name: &str, input: &str, deps: &[&str]| {
        target_with(name, |target| {
            target.inputs = vec![Written::new(input)];
            target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
        })
    };
    let mut pipeline = root_only(vec![
        reading("gen", "gen.c", &[]),
        reading("app", "main.c", &["gen"]),
        reading("docs", "README", &[]),
    ]);
    for name in ["gen.c", "main.c", "README"] {
        put(&mut pipeline, name);
    }
    pipeline
}

/// A build of `scope` through `build_watched`, with no setting overridden.
fn watched(pipeline: &mut Pipeline<FakePorts>, scope: &Scope) -> Watched {
    pipeline.build_watched(
        &entry(),
        scope,
        &BTreeMap::new(),
        OnFailure::Stop,
        LiveOutput::Off,
    )
}

/// The graph and inputs a build ran on, which the test expects to exist.
fn built_of(watched: Watched) -> Built {
    watched.built.unwrap()
}

#[test]
fn build_watched_returns_builds_outcomes() {
    let mut plain = chain();
    let outcomes = plain
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();

    let mut through = chain();
    let ended = watched(&mut through, &Scope::Workspace).ended.unwrap();

    assert_eq!(ended, outcomes);
    assert_eq!(ended.len(), 3);
}

#[test]
fn build_watched_returns_the_whole_graph() {
    let mut pipeline = chain();

    let built = built_of(watched(
        &mut pipeline,
        &Scope::Targets(BTreeSet::from([label("//:gen")])),
    ));

    let recorded = pipeline.ports().graph.recorded.last().unwrap();
    assert_eq!(&json::canonical::to_vec(&built.graph).unwrap(), recorded);
    assert_eq!(built.graph.nodes().len(), 3);
}

#[test]
fn the_inputs_are_those_of_the_scope_and_what_it_needs() {
    let mut pipeline = chain();

    let built = built_of(watched(
        &mut pipeline,
        &Scope::Targets(BTreeSet::from([label("//:app")])),
    ));

    assert_eq!(
        built.inputs,
        BTreeSet::from([path("gen.c"), path("main.c")])
    );
}

#[test]
fn a_load_failure_has_no_graph() {
    let source = FakeSource::new(workspace(vec![(
        "",
        FakeFile::Fails(EvaluationFailure::Syntax),
    )]));
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

    let result = watched(&mut pipeline, &Scope::Workspace);

    assert!(result.built.is_none());
    assert!(matches!(result.ended, Err(Error::Evaluation { .. })));
}

#[test]
fn a_failed_discovery_target_has_no_graph() {
    let items = vec![target_with("pkgs", |target| {
        target.role = TargetRole::Discovery;
    })];
    let source = FakeSource::declaring_from_results(
        workspace(vec![("", file(items.clone(), &[]))]),
        workspace(vec![("", file(items, &[]))]),
    );
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    script(&mut pipeline, "//:pkgs", ending(Ends::Exits(1)));

    let result = watched(&mut pipeline, &Scope::Workspace);

    assert!(result.built.is_none());
    let outcomes = result.ended.unwrap();
    assert_eq!(
        classes(&outcomes),
        BTreeMap::from([(
            "//:pkgs".to_owned(),
            OutcomeClass::ActionFailure { status: 1 }
        )])
    );
}

#[test]
fn a_refusal_keeps_the_graph() {
    let mut pipeline = chain();
    pipeline.ports_mut().manifest.workspace.ceiling.run.clear();

    let result = watched(&mut pipeline, &Scope::Workspace);

    assert!(matches!(result.ended, Err(Error::Refused { .. })));
    let built = built_of(result);
    assert_eq!(built.graph.nodes().len(), 3);
    assert_eq!(
        built.inputs,
        BTreeSet::from([path("gen.c"), path("main.c"), path("README")])
    );
}

#[test]
fn a_discovery_target_outside_the_scope_still_has_its_inputs_watched() {
    let discovery = target_with("pkgs", |target| {
        target.role = TargetRole::Discovery;
        target.inputs = vec![Written::new("pkgs.in")];
    });
    let reading = target_with("app", |target| target.inputs = vec![Written::new("main.c")]);
    let items = vec![discovery, reading];
    let source = FakeSource::declaring_from_results(
        workspace(vec![("", file(items.clone(), &[]))]),
        workspace(vec![("", file(items, &[]))]),
    );
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    for name in ["pkgs.in", "main.c"] {
        put(&mut pipeline, name);
    }

    let result = watched(
        &mut pipeline,
        &Scope::Targets(BTreeSet::from([label("//:app")])),
    );

    let outcomes = result.ended.as_ref().unwrap();
    assert!(
        outcomes
            .iter()
            .all(|outcome| !matches!(outcome.class, OutcomeClass::ActionFailure { .. })),
        "{outcomes:?}"
    );
    assert_eq!(
        built_of(result).inputs,
        BTreeSet::from([path("main.c"), path("pkgs.in")])
    );
}

#[test]
fn an_unknown_scope_label_has_no_graph() {
    let mut pipeline = root_only(vec![target("app", &[])]);

    let result = watched(
        &mut pipeline,
        &Scope::Targets(BTreeSet::from([label("//:nothing")])),
    );

    assert!(result.built.is_none());
    assert!(matches!(result.ended, Err(Error::UnknownTarget { .. })));
}
