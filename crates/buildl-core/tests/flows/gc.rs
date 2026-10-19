//! `pipeline::gc`, driven through every port by the fakes of [`crate::common`], after a real
//! `Pipeline::build` has filled the cache and the store.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    Argument, Collected, Command, ContentStore, Digest, EntryName, LiveOutput, LogLine, LoggedRun,
    OnFailure, OutcomeClass, OutputManifest, Pipeline, Scope, StagedItem, Timestamp, pipeline,
};

use crate::common::{
    FakeHistory, FakePorts, FakeSource, Script, file, label, root_only, script, target, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// A workspace of `lib` and `app`, which depends on `lib`, built once. `app` prints a line to
/// standard error, which no other target does.
fn built() -> Pipeline<FakePorts> {
    let mut pipeline = root_only(declared(true));
    script(
        &mut pipeline,
        "//:app",
        Script {
            stderr: Some(b"warning\n".to_vec()),
            ..Script::default()
        },
    );
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();
    pipeline
}

/// The declarations of the workspace, with or without the leaf `app`.
fn declared(with_leaf: bool) -> Vec<StagedItem> {
    let mut items = vec![target("lib", &[])];
    if with_leaf {
        items.push(target("app", &["lib"]));
    }
    items
}

/// A source that evaluates to `items` in the root build file.
fn source(items: Vec<StagedItem>) -> FakeSource {
    FakeSource::new(workspace(vec![("", file(items, &[]))]))
}

/// A `gc` over the pipeline's cache and store, the one store passed as both store and sweep.
fn gc(pipeline: &mut Pipeline<FakePorts>, source: &FakeSource, history: &FakeHistory) -> Collected {
    let ports = pipeline.ports_mut();
    pipeline::gc(
        source,
        &entry(),
        &mut ports.cache,
        &ports.cas,
        &ports.cas,
        history,
    )
    .unwrap()
}

/// The digests of the files the stored `manifests` name.
fn named_files(cas: &impl ContentStore, manifests: &[Digest]) -> BTreeSet<Digest> {
    let mut files = BTreeSet::new();
    for at in manifests {
        let bytes = cas.get(at).unwrap().unwrap();
        match OutputManifest::from_bytes(&bytes).unwrap() {
            OutputManifest::File(file) => {
                files.insert(file.digest);
            }
            OutputManifest::Tree(tree) => {
                files.extend(tree.entries().iter().map(|entry| entry.digest));
            }
        }
    }
    files
}

/// A run of `//:app` that printed `stdout` and `stderr`.
fn logged(stdout: Digest, stderr: Digest) -> LogLine {
    LogLine::Event(LoggedRun {
        at: Timestamp::from_unix_nanos(1),
        class: OutcomeClass::Built,
        command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
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

const NOTHING: Collected = Collected {
    rows_dropped: 0,
    blobs_removed: 0,
    bytes_removed: 0,
};

#[test]
fn gc_after_build_keeps_everything_still_declared() {
    let mut pipeline = built();
    let stored = pipeline.ports().cas.digests();
    assert!(!stored.is_empty());

    let collected = gc(
        &mut pipeline,
        &source(declared(true)),
        &FakeHistory::default(),
    );

    assert_eq!(collected, NOTHING);
    assert_eq!(pipeline.ports().cas.digests(), stored);
    assert_eq!(pipeline.ports().cache.rows.len(), 2);
}

#[test]
fn gc_after_a_target_is_removed_drops_its_row_and_only_its_blobs() {
    let mut pipeline = built();
    let app = label("//:app");
    let lib = label("//:lib");
    let app_row = pipeline.ports().cache.rows[&app].clone();
    let lib_row = pipeline.ports().cache.rows[&lib].clone();
    let before = pipeline.ports().cas.digests();
    let lib_files = named_files(&pipeline.ports().cas, lib_row.outputs());
    assert!(!lib_files.is_empty());
    let history = FakeHistory {
        lines: vec![logged(app_row.stdout(), app_row.stderr())],
        broken: false,
    };

    let collected = gc(&mut pipeline, &source(declared(false)), &history);

    let ports = pipeline.ports();
    assert!(!ports.cache.rows.contains_key(&app), "its row is gone");
    assert_eq!(ports.cache.rows.get(&lib), Some(&lib_row));
    let left = ports.cas.digests();
    let removed: BTreeSet<Digest> = before.difference(&left).copied().collect();
    let manifests: BTreeSet<Digest> = app_row.outputs().iter().copied().collect();
    assert!(
        manifests.is_subset(&removed),
        "its output manifests are gone"
    );
    assert!(
        removed.len() > manifests.len(),
        "the files its manifests named are gone too"
    );
    assert!(
        left.contains(&app_row.stdout()) && left.contains(&app_row.stderr()),
        "what the log names stays"
    );
    assert!(left.contains(&lib_row.stdout()) && left.contains(&lib_row.stderr()));
    assert!(lib_row.outputs().iter().all(|digest| left.contains(digest)));
    assert!(
        removed.is_disjoint(&lib_files),
        "the files the kept target's manifests name stay"
    );
    assert_eq!(collected.rows_dropped, 1);
    assert_eq!(collected.blobs_removed, removed.len());
    assert!(collected.bytes_removed > 0);
}

#[test]
fn a_second_gc_removes_nothing() {
    let mut pipeline = built();
    let reduced = source(declared(false));

    let first = gc(&mut pipeline, &reduced, &FakeHistory::default());
    let second = gc(&mut pipeline, &reduced, &FakeHistory::default());

    assert_eq!(first.rows_dropped, 1);
    assert!(first.blobs_removed > 0);
    assert_eq!(second, NOTHING);
}
