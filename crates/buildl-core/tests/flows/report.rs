//! What a build reports while it runs, what it records of its graph, and building part of a
//! workspace: `Pipeline::build` and `Pipeline::plan` driven by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    Channel, Digest, EntryName, Error, FileStat, Label, LiveOutput, OnFailure, OutcomeClass,
    Pipeline, PortName, Scope, SettingName, SettingValue, SourcePath, Summary, Tally, Timestamp,
    Written, json,
};

use crate::common::{
    Ends, FakeContent, FakePorts, FakeSource, Pace, Reported, Script, file, ports, program,
    target_with, workspace,
};

type Overrides = BTreeMap<SettingName, SettingValue>;

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

fn only(labels: &[&str]) -> Scope {
    Scope::Targets(labels.iter().map(|raw| label(raw)).collect())
}

fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str) {
    let path = SourcePath::parse(raw).unwrap();
    let content = FakeContent {
        bytes: raw.as_bytes().to_vec(),
        stat: FileStat {
            modified: Timestamp::from_unix_nanos(100),
            size: u64::try_from(raw.len()).unwrap(),
        },
    };
    let ports = pipeline.ports_mut();
    ports.digester.files.insert(path.clone(), content);
    ports.strategy.files.insert(path, raw.as_bytes().to_vec());
}

/// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone.
fn chain() -> Pipeline<FakePorts> {
    let reading = |name: &str, input: &str, deps: &[&str]| {
        target_with(name, |target| {
            target.inputs = vec![Written::new(input)];
            target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
        })
    };
    let source = FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                reading("gen", "gen.c", &[]),
                reading("lib", "lib.c", &["gen"]),
                reading("app", "main.c", &["lib"]),
                reading("docs", "README", &[]),
            ],
            &[],
        ),
    )]));
    let mut pipeline = Pipeline::new(ports(source));
    for name in ["gen.c", "lib.c", "main.c", "README"] {
        put(&mut pipeline, name);
    }
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

fn build(pipeline: &mut Pipeline<FakePorts>, scope: &Scope, live: LiveOutput) -> Summary {
    let outcomes = pipeline
        .build(&entry(), scope, &Overrides::new(), OnFailure::Stop, live)
        .unwrap();
    Summary::of(&outcomes)
}

fn started(raw: &str) -> Reported {
    Reported::Started(label(raw), None)
}

fn finished(raw: &str, class: OutcomeClass, stdout: &str, stderr: &str) -> Reported {
    Reported::Finished {
        label: label(raw),
        class,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

fn printed(raw: &str, channel: Channel, bytes: &str) -> Reported {
    Reported::Printed(label(raw), channel, bytes.as_bytes().to_vec())
}

const ALL_BUILT: Summary = Summary {
    all: Tally {
        failed: 0,
        skipped: 0,
        built: 4,
        cached: 0,
        fetched: 0,
    },
    tests: Tally {
        failed: 0,
        skipped: 0,
        built: 0,
        cached: 0,
        fetched: 0,
    },
};

#[test]
fn a_build_reports_each_action_as_it_begins_and_ends_and_the_summary_last() {
    let mut pipeline = chain();

    build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

    assert_eq!(
        pipeline.ports().reporter.calls,
        [
            Reported::Begun(4),
            started("//:docs"),
            finished("//:docs", OutcomeClass::Built, "//:docs\n", ""),
            started("//:gen"),
            finished("//:gen", OutcomeClass::Built, "//:gen\n", ""),
            started("//:lib"),
            finished("//:lib", OutcomeClass::Built, "//:lib\n", ""),
            started("//:app"),
            finished("//:app", OutcomeClass::Built, "//:app\n", ""),
            Reported::Summary(ALL_BUILT),
        ]
    );
}

#[test]
fn a_cached_target_never_starts_and_is_reported_with_what_it_printed_when_it_ran() {
    let mut pipeline = chain();
    pipeline.ports_mut().strategy.scripts.insert(
        label("//:gen"),
        Script {
            stderr: Some(b"gen.c:1: warning: unused\n".to_vec()),
            ..Script::default()
        },
    );
    build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);
    pipeline.ports_mut().reporter.calls.clear();

    build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

    assert_eq!(
        pipeline.ports().reporter.calls,
        [
            Reported::Begun(4),
            finished("//:docs", OutcomeClass::Cached, "//:docs\n", ""),
            finished(
                "//:gen",
                OutcomeClass::Cached,
                "//:gen\n",
                "gen.c:1: warning: unused\n"
            ),
            finished("//:lib", OutcomeClass::Cached, "//:lib\n", ""),
            finished("//:app", OutcomeClass::Cached, "//:app\n", ""),
            Reported::Summary(Summary {
                all: Tally {
                    failed: 0,
                    skipped: 0,
                    built: 0,
                    cached: 4,
                    fetched: 0,
                },
                tests: Tally::default()
            }),
        ]
    );
}

#[test]
fn a_failure_and_what_it_skips_are_reported_and_a_skipped_target_never_starts() {
    let mut pipeline = chain();
    pipeline.ports_mut().strategy.scripts.insert(
        label("//:gen"),
        Script {
            ends: Ends::Signaled(11),
            stderr: Some(b"segmentation fault\n".to_vec()),
            ..Script::default()
        },
    );

    let outcomes = pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &Overrides::new(),
            OnFailure::KeepGoing,
            LiveOutput::Off,
        )
        .unwrap();

    let killed = OutcomeClass::Killed { signal: 11 };
    let because = || OutcomeClass::Skipped {
        because: label("//:gen"),
    };
    assert_eq!(
        pipeline.ports().reporter.calls,
        [
            Reported::Begun(4),
            started("//:docs"),
            finished("//:docs", OutcomeClass::Built, "//:docs\n", ""),
            started("//:gen"),
            finished("//:gen", killed, "//:gen\n", "segmentation fault\n"),
            finished("//:app", because(), "", ""),
            finished("//:lib", because(), "", ""),
            Reported::Summary(Summary {
                all: Tally {
                    failed: 1,
                    skipped: 2,
                    built: 1,
                    cached: 0,
                    fetched: 0,
                },
                tests: Tally::default()
            }),
        ]
    );
    assert_eq!(Summary::of(&outcomes).all.failed, 1);
    assert!(
        !pipeline.ports().cache.rows.contains_key(&label("//:gen")),
        "a killed action leaves no row"
    );
}

#[test]
fn live_output_shows_an_action_as_it_prints_and_stores_exactly_what_a_quiet_build_stores() {
    let script = || Script {
        stdout: Some(b"one\ntwo\n".to_vec()),
        stderr: Some(b"warn\n".to_vec()),
        ..Script::default()
    };
    let mut quiet = chain();
    let mut live = chain();
    for pipeline in [&mut quiet, &mut live] {
        pipeline
            .ports_mut()
            .strategy
            .scripts
            .insert(label("//:docs"), script());
    }

    build(&mut quiet, &Scope::Workspace, LiveOutput::Off);
    build(&mut live, &Scope::Workspace, LiveOutput::OneAction);

    let calls = &live.ports().reporter.calls;
    assert_eq!(
        calls.get(..6).unwrap(),
        [
            Reported::Begun(4),
            started("//:docs"),
            printed("//:docs", Channel::Stdout, "o"),
            printed("//:docs", Channel::Stderr, "warn\n"),
            printed("//:docs", Channel::Stdout, "ne\ntwo\n"),
            finished("//:docs", OutcomeClass::Built, "", ""),
        ],
        "what was shown live is not handed over again"
    );
    assert_eq!(calls.last(), Some(&Reported::Summary(ALL_BUILT)));

    assert_eq!(live.ports().cache.rows, quiet.ports().cache.rows);
    assert_eq!(live.ports().log.events, quiet.ports().log.events);
    assert_eq!(live.ports().cas.digests(), quiet.ports().cas.digests());
    assert_eq!(
        live.ports().cas.blob(&Digest::of(b"one\ntwo\n")),
        Some(b"one\ntwo\n".to_vec())
    );
}

/// Whether, from the first `Printed` for a target until that target's `Finished`, every
/// `Printed` and `Finished` is about that target.
fn never_interleaved(calls: &[Reported]) -> bool {
    let mut live: Option<&Label> = None;
    calls.iter().all(|call| match call {
        Reported::Printed(label, ..) => {
            let same = live.is_none_or(|open| open == label);
            live = Some(label);
            same
        }
        Reported::Finished { label, .. } => {
            let same = live.is_none_or(|open| open == label);
            live = None;
            same
        }
        Reported::Begun(_) | Reported::Started(..) | Reported::Summary(_) => true,
    })
}

#[test]
fn while_one_action_is_shown_live_the_outcomes_of_the_others_wait_for_it_to_end() {
    let mut pipeline = chain();
    pipeline.ports_mut().dispatcher.pace = Pace::Together;

    build(&mut pipeline, &Scope::Workspace, LiveOutput::OneAction);

    let calls = &pipeline.ports().reporter.calls;
    assert_eq!(
        calls.get(..7).unwrap(),
        [
            Reported::Begun(4),
            started("//:docs"),
            printed("//:docs", Channel::Stdout, "/"),
            printed("//:docs", Channel::Stdout, "/:docs\n"),
            started("//:gen"),
            finished("//:docs", OutcomeClass::Built, "", ""),
            finished("//:gen", OutcomeClass::Built, "//:gen\n", ""),
        ],
        "//:gen ended first, and is reported once //:docs, shown live, has ended"
    );
    assert!(never_interleaved(calls), "{calls:?}");
    assert_eq!(calls.last(), Some(&Reported::Summary(ALL_BUILT)));
    let logged: Vec<String> = pipeline
        .ports()
        .log
        .events
        .iter()
        .map(|event| event.outcome.label.to_string())
        .collect();
    assert_eq!(
        logged.get(..2).unwrap(),
        ["//:gen", "//:docs"],
        "the log is written as each target ends, whatever is being shown"
    );
}

#[test]
fn a_build_of_one_target_builds_what_it_needs_and_nothing_else() {
    let mut pipeline = chain();
    // What the targets outside the request read does not have to exist.
    for unneeded in ["main.c", "README"] {
        let path = SourcePath::parse(unneeded).unwrap();
        pipeline.ports_mut().digester.files.remove(&path);
    }
    let whole = json::canonical::to_vec(&pipeline.graph(&entry()).unwrap()).unwrap();

    let outcomes = pipeline
        .build(
            &entry(),
            &only(&["//:lib"]),
            &Overrides::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();

    let built: Vec<String> = outcomes
        .iter()
        .map(|outcome| outcome.label.to_string())
        .collect();
    assert_eq!(built, ["//:gen", "//:lib"]);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), [label("//:gen"), label("//:lib")]);
    assert_eq!(
        ports.cache.rows.keys().collect::<Vec<_>>(),
        [&label("//:gen"), &label("//:lib")]
    );
    assert_eq!(ports.log.events.len(), 2);
    assert_eq!(
        ports.reporter.calls.last(),
        Some(&Reported::Summary(Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 2,
                cached: 0,
                fetched: 0,
            },
            tests: Tally::default()
        }))
    );
    assert_eq!(
        ports.graph.recorded,
        [whole],
        "the graph recorded is the whole workspace's, in the bytes `graph` gives"
    );
}

#[test]
fn a_whole_build_after_a_build_of_one_target_finds_that_part_cached() {
    let mut pipeline = chain();
    build(&mut pipeline, &only(&["//:lib"]), LiveOutput::Off);

    let summary = build(&mut pipeline, &Scope::Workspace, LiveOutput::Off);

    assert_eq!(
        summary,
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 2,
                cached: 2,
                fetched: 0,
            },
            tests: Tally::default()
        }
    );
    assert_eq!(
        pipeline.ports().graph.recorded.first(),
        pipeline.ports().graph.recorded.get(1),
        "both builds record the same bytes"
    );
}

#[test]
fn a_plan_of_one_target_holds_only_what_that_target_needs() {
    let mut pipeline = chain();

    let plan = pipeline
        .plan(&entry(), &only(&["//:lib"]), &Overrides::new())
        .unwrap();

    assert_eq!(plan.len(), 2);
    let text = json::canonical::to_string(&plan).unwrap();
    assert!(text.contains("\"//:gen\"") && text.contains("\"//:lib\""));
    assert!(!text.contains("\"//:app\"") && !text.contains("\"//:docs\""));
    assert!(
        pipeline.ports().graph.recorded.is_empty(),
        "a plan records no graph"
    );
}

#[test]
fn asking_for_a_target_nothing_declares_is_refused_and_nothing_runs() {
    let mut pipeline = chain();

    let error = pipeline
        .build(
            &entry(),
            &only(&["//:libs"]),
            &Overrides::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap_err();

    match error {
        Error::UnknownTarget {
            label: asked,
            suggestion,
        } => {
            assert_eq!(asked, label("//:libs"));
            assert_eq!(suggestion, Some(label("//:lib")));
        }
        other => unreachable!("expected UnknownTarget, got {other:?}"),
    }
    assert_eq!(pipeline.ports().strategy.ran(), []);
    assert_eq!(pipeline.ports().reporter.calls, []);
}

#[test]
fn two_targets_claiming_one_place_are_refused_even_when_only_one_is_asked_for() {
    let source = FakeSource::new(workspace(vec![
        (
            "",
            file(
                vec![target_with("a", |target| {
                    target.outputs = Some(vec![Written::new("lib")]);
                })],
                &["lib"],
            ),
        ),
        ("lib", file(vec![target_with("t", |_| {})], &[])),
    ]));
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));

    let error = pipeline
        .build(
            &entry(),
            &only(&["//:a"]),
            &Overrides::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap_err();

    assert!(
        matches!(error, Error::OutputConflict { .. }),
        "expected OutputConflict, got {error:?}"
    );
    assert_eq!(pipeline.ports().strategy.ran(), []);
}

#[test]
fn a_graph_that_cannot_be_recorded_fails_the_build_before_anything_is_planned() {
    let mut pipeline = chain();
    pipeline.ports_mut().graph.broken = true;

    let error = pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &Overrides::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap_err();

    assert!(
        matches!(
            error,
            Error::Adapter {
                port: PortName::GraphRecord,
                ..
            }
        ),
        "expected the graph record's failure, got {error:?}"
    );
    assert_eq!(pipeline.ports().tools.lookups(), [], "nothing was planned");
}

#[test]
fn a_build_that_fails_as_a_whole_reports_no_summary() {
    let mut pipeline = chain();
    pipeline.ports_mut().log.broken = true;

    let failed = pipeline.build(
        &entry(),
        &Scope::Workspace,
        &Overrides::new(),
        OnFailure::Stop,
        LiveOutput::Off,
    );

    assert!(failed.is_err());
    let calls = &pipeline.ports().reporter.calls;
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Reported::Summary(_))),
        "{calls:?}"
    );
}

#[test]
fn asking_for_no_target_builds_nothing() {
    let mut pipeline = chain();

    let summary = build(
        &mut pipeline,
        &Scope::Targets(BTreeSet::new()),
        LiveOutput::Off,
    );

    assert_eq!(
        summary,
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 0,
                cached: 0,
                fetched: 0,
            },
            tests: Tally::default()
        }
    );
}
