//! `Pipeline::build`, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use buildl_core::{
    ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileMode, FileStat, Freshness,
    InfraKind, OnFailure, Origin, OutcomeClass, OutcomeEvent, OutputManifest, OutputName,
    OutputPath, OutputRefusal, Pipeline, PluginCap, PluginName, PortName, SettingName,
    SettingValue, SourcePath, StagedItem, StagedSetting, StoredFile, Summary, Tally, TargetRole,
    Timestamp, TreePath, Want, WantedSet, Written, json,
};
use buildl_core::{LiveOutput, Scope};

use crate::common::{
    Answer, Content, Ends, FakeApprover, FakeContent, FakeFile, FakePorts, FakeSource, Leaf, Loss,
    Order, Script, classes, dir, ending, expected, file, file_by, label, plugin, ports, program,
    root_only, rule, script, setting, skipped, target, target_with, tree, workspace,
};

/// The events of [`two_directories`] built once, one canonical JSON object per line, pinned.
const GOLDEN: &str = include_str!("golden/log.jsonl");

/// The values given to settings for one run.
type Overrides = BTreeMap<SettingName, SettingValue>;

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn path(raw: &str) -> SourcePath {
    SourcePath::parse(raw).unwrap()
}

/// Writes or replaces a workspace file holding `text`, last modified at `modified`: for the
/// digester that hashes it, and for the actions that read it.
fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str, text: &str, modified: u64) {
    let content = FakeContent {
        bytes: text.as_bytes().to_vec(),
        stat: FileStat {
            modified: Timestamp::from_unix_nanos(modified),
            size: u64::try_from(text.len()).unwrap(),
        },
    };
    let ports = pipeline.ports_mut();
    ports.digester.files.insert(path(raw), content);
    ports
        .strategy
        .files
        .insert(path(raw), text.as_bytes().to_vec());
}

/// A build of the workspace with no setting overridden.
fn build(pipeline: &mut Pipeline<FakePorts>, on_failure: OnFailure) -> Vec<ActionOutcome> {
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &Overrides::new(),
            on_failure,
            LiveOutput::Off,
        )
        .unwrap()
}

/// A build of the targets `labels` name, with no setting overridden.
fn build_named(
    pipeline: &mut Pipeline<FakePorts>,
    labels: &[&str],
    on_failure: OnFailure,
) -> Result<Vec<ActionOutcome>, Error> {
    pipeline.build(
        &entry(),
        &Scope::Targets(labels.iter().map(|raw| label(raw)).collect()),
        &Overrides::new(),
        on_failure,
        LiveOutput::Off,
    )
}

/// The last event logged for the target `raw` names.
fn event<'p>(pipeline: &'p Pipeline<FakePorts>, raw: &str) -> &'p OutcomeEvent {
    let wanted = label(raw);
    pipeline
        .ports()
        .log
        .events
        .iter()
        .rev()
        .find(|event| event.outcome.label == wanted)
        .unwrap()
}

/// What the output with that digest is: its manifest, read back from the store.
fn manifest(pipeline: &Pipeline<FakePorts>, digest: &Digest) -> OutputManifest {
    let bytes = pipeline.ports().cas.blob(digest).unwrap();
    OutputManifest::from_bytes(&bytes).unwrap()
}

/// The one file the output with that digest is.
fn file_of(pipeline: &Pipeline<FakePorts>, digest: &Digest) -> StoredFile {
    match manifest(pipeline, digest) {
        OutputManifest::File(file) => file,
        OutputManifest::Tree(tree) => unreachable!("expected a file, got {tree:?}"),
    }
}

/// The files of the directory the output with that digest is, as `(path, digest, mode)`.
fn tree_of(pipeline: &Pipeline<FakePorts>, digest: &Digest) -> Vec<(String, Digest, FileMode)> {
    match manifest(pipeline, digest) {
        OutputManifest::Tree(tree) => tree
            .entries()
            .iter()
            .map(|entry| (entry.path.to_string(), entry.digest, entry.mode))
            .collect(),
        OutputManifest::File(file) => unreachable!("expected a tree, got {file:?}"),
    }
}

/// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone. Each reads one file and
/// runs `cc`, which the fake resolver can find.
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
        put(&mut pipeline, name, name, 100);
    }
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

#[test]
fn a_workspace_never_built_builds_every_target_after_what_it_depends_on() {
    let mut pipeline = chain();

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    let labels: Vec<String> = outcomes
        .iter()
        .map(|outcome| outcome.label.to_string())
        .collect();
    assert_eq!(
        labels,
        ["//:app", "//:docs", "//:gen", "//:lib"],
        "one outcome per target, in id order"
    );
    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 4,
                cached: 0
            },
            tests: Tally::default()
        }
    );

    let ports = pipeline.ports();
    let placed = ports.cas.placed();
    for outcome in &outcomes {
        assert_eq!(outcome.class, OutcomeClass::Built, "{}", outcome.label);
        let row = ports.cache.rows.get(&outcome.label).unwrap();
        assert_eq!(row.produced(), outcome.outputs, "{}", outcome.label);
        assert_eq!(outcome.outputs.len(), 1, "{}", outcome.label);
        for produced in &outcome.outputs {
            let file = file_of(&pipeline, &produced.digest);
            assert_eq!(file.mode, FileMode::Plain);
            assert!(ports.cas.blob(&file.digest).is_some());
            let at = OutputPath::of(&dir("out"), &outcome.label, &produced.name);
            assert_eq!(placed.get(&at), Some(&file.digest), "{at}");
        }
        let printed = format!("{}\n", outcome.label).into_bytes();
        assert_eq!(ports.cas.blob(&outcome.stdout.unwrap()), Some(printed));
        assert_eq!(outcome.stderr, Some(Digest::of(b"")));
    }
    assert_eq!(placed.len(), 4);
    assert_eq!(ports.log.events.len(), 4, "one event per target");
    assert_eq!(ports.cache.recorded().len(), 4);

    for (target, dependency) in [("//:lib", "//:gen"), ("//:app", "//:lib")] {
        let began = event(&pipeline, target).started.unwrap();
        let done = event(&pipeline, dependency).finished.unwrap();
        assert!(began > done, "{target} began after {dependency} was done");
    }
    assert_eq!(
        ports.strategy.ran(),
        [
            label("//:docs"),
            label("//:gen"),
            label("//:lib"),
            label("//:app")
        ],
        "the oldest waiting task is taken first"
    );
}

#[test]
fn a_second_build_with_nothing_changed_runs_nothing_and_places_every_output_again() {
    let mut pipeline = chain();
    build(&mut pipeline, OnFailure::Stop);
    let rows = pipeline.ports().cache.rows.clone();
    let placed = pipeline.ports().cas.placed();
    let reads = pipeline.ports().clock.reads();

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 0,
                cached: 4
            },
            tests: Tally::default()
        }
    );
    let ports = pipeline.ports();
    for outcome in &outcomes {
        assert_eq!(outcome.class, OutcomeClass::Cached, "{}", outcome.label);
        let row = rows.get(&outcome.label).unwrap();
        assert_eq!(outcome.outputs, row.produced(), "{}", outcome.label);
        assert_eq!(outcome.stdout, Some(row.stdout()));
        let logged = event(&pipeline, &outcome.label.to_string());
        assert_eq!(logged.key, Some(row.key().digest()));
        assert_eq!((logged.started, logged.finished), (None, None));
    }
    assert_eq!(ports.strategy.ran().len(), 4, "no action ran again");
    assert_eq!(ports.cache.recorded().len(), 4, "no row was written again");
    assert_eq!(ports.cache.rows, rows);
    assert_eq!(ports.cas.placed(), placed);
    assert_eq!(
        ports.cas.placements().len(),
        8,
        "every output was placed again"
    );
    assert_eq!(ports.log.events.len(), 8);
    assert_eq!(
        ports.clock.reads() - reads,
        4,
        "the clock was read once per event and by no worker"
    );
}

/// Where the output `name` of the target `raw` lives in a workspace whose outputs go under `out`.
fn out(raw: &str, name: &str) -> OutputPath {
    OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
}

/// The outcome of the target `raw` names.
fn outcome<'o>(outcomes: &'o [ActionOutcome], raw: &str) -> &'o ActionOutcome {
    let wanted = label(raw);
    outcomes
        .iter()
        .find(|outcome| outcome.label == wanted)
        .unwrap()
}

#[test]
fn an_edited_input_rebuilds_its_target_and_what_depends_on_it_and_nothing_else() {
    let mut pipeline = chain();
    build(&mut pipeline, OnFailure::Stop);
    put(&mut pipeline, "lib.c", "lib.c, edited", 200);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:docs", OutcomeClass::Cached),
            ("//:gen", OutcomeClass::Cached),
            ("//:lib", OutcomeClass::Built),
        ])
    );
    let ran = pipeline.ports().strategy.ran();
    assert_eq!(
        ran.get(4..),
        Some(&[label("//:lib"), label("//:app")][..]),
        "the second build ran the edited target, then the one that depends on it"
    );
}

#[test]
fn a_target_rebuilt_to_the_same_output_leaves_what_depends_on_it_cached() {
    let mut pipeline = chain();
    let same = Script {
        outputs: BTreeMap::from([(out("//:gen", "gen"), Content::Holds(b"generated".to_vec()))]),
        ..Script::default()
    };
    pipeline
        .ports_mut()
        .strategy
        .scripts
        .insert(label("//:gen"), same);
    build(&mut pipeline, OnFailure::Stop);
    put(&mut pipeline, "gen.c", "gen.c, edited", 200);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Cached),
            ("//:docs", OutcomeClass::Cached),
            ("//:gen", OutcomeClass::Built),
            ("//:lib", OutcomeClass::Cached),
        ])
    );
    let strategy = &pipeline.ports().strategy;
    assert_eq!(strategy.runs(&label("//:gen")), 2);
    assert_eq!(strategy.runs(&label("//:lib")), 1, "early cutoff");
    assert_eq!(strategy.runs(&label("//:app")), 1, "early cutoff");
}

#[test]
fn a_target_whose_output_left_the_store_runs_again_and_the_others_stay_cached() {
    let mut pipeline = chain();
    let first = build(&mut pipeline, OnFailure::Stop);
    let lost = outcome(&first, "//:lib").outputs.first().unwrap().digest;
    pipeline.ports().cas.forget(&lost);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Cached),
            ("//:docs", OutcomeClass::Cached),
            ("//:gen", OutcomeClass::Cached),
            ("//:lib", OutcomeClass::Built),
        ])
    );
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.runs(&label("//:lib")), 2);
    assert_eq!(ports.strategy.ran().len(), 5, "no other action ran again");
    assert!(
        ports.cas.blob(&lost).is_some(),
        "the output is stored again"
    );
}

#[test]
fn a_cached_target_whose_output_cannot_be_placed_fails_and_keeps_its_row() {
    let mut pipeline = chain();
    build(&mut pipeline, OnFailure::KeepGoing);
    let rows = pipeline.ports().cache.rows.clone();
    pipeline.ports().cas.fail_place(out("//:docs", "docs"));

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    let store = OutcomeClass::InfraFailure {
        kind: InfraKind::Store,
    };
    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Cached),
            ("//:docs", store),
            ("//:gen", OutcomeClass::Cached),
            ("//:lib", OutcomeClass::Cached),
        ])
    );
    let row = rows.get(&label("//:docs")).unwrap();
    let docs = outcome(&outcomes, "//:docs");
    assert_eq!(
        docs.diagnostic.as_ref().map(ToString::to_string).as_deref(),
        Some("content store: out/docs cannot be written")
    );
    assert_eq!(docs.outputs, []);
    assert_eq!(
        (docs.stdout, docs.stderr),
        (Some(row.stdout()), Some(row.stderr())),
        "what it printed is taken from its row"
    );
    let logged = event(&pipeline, "//:docs");
    assert_eq!(logged.key, Some(row.key().digest()));
    assert_eq!((logged.started, logged.finished), (None, None));
    let ports = pipeline.ports();
    assert_eq!(ports.cache.rows, rows, "its row is unchanged");
    assert_eq!(ports.strategy.runs(&label("//:docs")), 1, "it did not run");
}

#[test]
fn a_target_that_always_runs_runs_on_every_build_and_its_dependent_stays_cached() {
    let mut pipeline = root_only(vec![
        target_with("gen", |target| target.freshness = Freshness::Always),
        target("lib", &["gen"]),
    ]);

    let first = build(&mut pipeline, OnFailure::Stop);
    let second = build(&mut pipeline, OnFailure::Stop);

    let both = [
        ("//:gen", OutcomeClass::Built),
        ("//:lib", OutcomeClass::Built),
    ];
    assert_eq!(classes(&first), expected(&both));
    assert_eq!(
        classes(&second),
        expected(&[
            ("//:gen", OutcomeClass::Built),
            ("//:lib", OutcomeClass::Cached)
        ])
    );
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.runs(&label("//:gen")), 2);
    assert_eq!(ports.strategy.runs(&label("//:lib")), 1);
    assert_eq!(
        ports.cache.recorded(),
        [label("//:gen"), label("//:lib"), label("//:gen")],
        "a target that always runs records a row like any other"
    );
}

#[test]
fn an_unplaced_cached_target_forgets_nothing() {
    let mut pipeline = root_only(vec![target("a", &[])]);
    build(&mut pipeline, OnFailure::Stop);
    pipeline.ports().cas.fail_place(out("//:a", "a"));

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:a", infra(InfraKind::Store))])
    );
    let cache = &pipeline.ports().cache;
    assert_eq!(cache.forgotten(), []);
    assert!(cache.rows.contains_key(&label("//:a")));
}

#[test]
fn a_failed_always_target_forgets_the_row_it_left() {
    let mut pipeline = root_only(vec![target_with("a", |target| {
        target.freshness = Freshness::Always;
    })]);
    let first = build(&mut pipeline, OnFailure::Stop);
    assert_eq!(classes(&first), expected(&[("//:a", OutcomeClass::Built)]));
    assert!(pipeline.ports().cache.rows.contains_key(&label("//:a")));
    script(&mut pipeline, "//:a", ending(Ends::Exits(1)));

    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[("//:a", OutcomeClass::ActionFailure { status: 1 })])
    );
    let cache = &pipeline.ports().cache;
    assert_eq!(cache.forgotten(), [label("//:a")]);
    assert!(!cache.rows.contains_key(&label("//:a")));
}

#[test]
fn a_forget_that_fails_is_a_store_failure() {
    let mut pipeline = root_only(vec![target_with("a", |target| {
        target.freshness = Freshness::Always;
    })]);
    let first = build(&mut pipeline, OnFailure::Stop);
    assert_eq!(classes(&first), expected(&[("//:a", OutcomeClass::Built)]));
    script(&mut pipeline, "//:a", ending(Ends::Exits(1)));
    pipeline
        .ports_mut()
        .cache
        .unforgettable
        .insert(label("//:a"));

    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[("//:a", infra(InfraKind::Store))])
    );
    assert_eq!(
        diagnostic(outcome(&second, "//:a")).as_deref(),
        Some("action cache: //:a cannot be forgotten")
    );
    let cache = &pipeline.ports().cache;
    assert_eq!(cache.forgotten(), []);
    assert!(cache.rows.contains_key(&label("//:a")), "its row stays");
}

/// The class of a target the builder itself failed, at `kind`.
const fn infra(kind: InfraKind) -> OutcomeClass {
    OutcomeClass::InfraFailure { kind }
}

/// The message of an outcome, as text.
fn diagnostic(outcome: &ActionOutcome) -> Option<String> {
    outcome.diagnostic.as_ref().map(ToString::to_string)
}

/// `//:a` exits with status 1. `//:b` needs it and `//:c` needs `//:b`; `//:x` and `//:y` stand
/// apart.
fn failing() -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![
        target("a", &[]),
        target("b", &["a"]),
        target("c", &["b"]),
        target("x", &[]),
        target("y", &[]),
    ]);
    script(&mut pipeline, "//:a", ending(Ends::Exits(1)));
    pipeline
}

#[test]
fn under_keep_going_a_failure_skips_only_what_depends_on_it() {
    let mut pipeline = failing();

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:a", OutcomeClass::ActionFailure { status: 1 }),
            ("//:b", skipped("//:a")),
            ("//:c", skipped("//:a")),
            ("//:x", OutcomeClass::Built),
            ("//:y", OutcomeClass::Built),
        ])
    );
    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 1,
                skipped: 2,
                built: 2,
                cached: 0
            },
            tests: Tally::default()
        }
    );
    let ports = pipeline.ports();
    let recorded: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
    assert_eq!(recorded, ["//:x", "//:y"], "no row for the other three");
    assert_eq!(ports.log.events.len(), 5, "one event per target");
    assert_eq!(
        event(&pipeline, "//:b").key,
        None,
        "a skipped target has no key"
    );
}

#[test]
fn under_stop_nothing_begins_after_the_first_failure() {
    let mut pipeline = failing();

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:a", OutcomeClass::ActionFailure { status: 1 }),
            ("//:b", skipped("//:a")),
            ("//:c", skipped("//:a")),
            ("//:x", skipped("//:a")),
            ("//:y", skipped("//:a")),
        ])
    );
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), [label("//:a")]);
    assert_eq!(
        ports.strategy.handed().len(),
        1,
        "the two already handed over came back not begun"
    );
    assert_eq!(
        ports.clock.reads(),
        7,
        "two reads by the worker of //:a and one per event"
    );
    assert_eq!(ports.cache.rows.len(), 0);
    assert_eq!(ports.log.events.len(), 5, "one event per target");
}

#[test]
fn under_stop_a_cached_target_that_cannot_be_placed_stops_what_waits_behind_it() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[]), target("c", &[])]);
    build(&mut pipeline, OnFailure::Stop);
    pipeline.ports().cas.fail_place(out("//:a", "a"));

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:a", infra(InfraKind::Store)),
            ("//:b", skipped("//:a")),
            ("//:c", skipped("//:a")),
        ])
    );
    let ports = pipeline.ports();
    assert_eq!(
        ports.strategy.ran().len(),
        3,
        "the second build ran nothing"
    );
    assert_eq!(ports.strategy.handed().len(), 3);
}

#[test]
fn a_row_that_cannot_be_recorded_fails_its_target_and_the_next_build_runs_it_again() {
    let mut pipeline = chain();
    pipeline.ports_mut().cache.failing.insert(label("//:gen"));

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", skipped("//:gen")),
            ("//:docs", OutcomeClass::Built),
            ("//:gen", infra(InfraKind::Store)),
            ("//:lib", skipped("//:gen")),
        ])
    );
    let gen_outcome = outcome(&outcomes, "//:gen");
    assert_eq!(
        diagnostic(gen_outcome).as_deref(),
        Some("action cache: //:gen cannot be recorded")
    );
    assert_eq!(gen_outcome.outputs, []);
    assert!(gen_outcome.stdout.is_some(), "what it printed was stored");
    {
        let ports = pipeline.ports();
        assert!(
            ports.cas.placed().contains_key(&out("//:gen", "gen")),
            "its output reached the store and its place before the row was tried"
        );
        assert!(!ports.cache.rows.contains_key(&label("//:gen")));
    }

    pipeline.ports_mut().cache.failing.clear();
    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:docs", OutcomeClass::Cached),
            ("//:gen", OutcomeClass::Built),
            ("//:lib", OutcomeClass::Built),
        ])
    );
    assert_eq!(pipeline.ports().strategy.runs(&label("//:gen")), 2);
}

#[test]
fn every_way_an_action_can_fail_is_a_class_of_its_own_and_none_is_recorded() {
    let names = [
        "collect", "exit", "missing", "prepare", "refused", "run", "short", "timeout",
    ];
    let mut pipeline = root_only(names.iter().map(|name| target(name, &[])).collect());
    script(&mut pipeline, "//:collect", ending(Ends::CollectFails));
    script(&mut pipeline, "//:exit", ending(Ends::Exits(3)));
    script(&mut pipeline, "//:prepare", ending(Ends::PrepareFails));
    script(&mut pipeline, "//:run", ending(Ends::RunFails));
    script(&mut pipeline, "//:short", ending(Ends::CollectShort));
    script(&mut pipeline, "//:timeout", ending(Ends::TimesOut));
    let leaving_nothing = Script {
        outputs: BTreeMap::from([(out("//:missing", "missing"), Content::Missing)]),
        ..Script::default()
    };
    script(&mut pipeline, "//:missing", leaving_nothing);
    let printing = Script {
        stdout: Some(b"cannot be kept".to_vec()),
        ..Script::default()
    };
    script(&mut pipeline, "//:refused", printing);
    pipeline.ports().cas.fail_put(b"cannot be kept".to_vec());

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    let missing = OutcomeClass::OutputMissing {
        name: OutputName::parse("missing").unwrap(),
    };
    // The class, the message, and whether what the action printed was stored.
    let table = [
        (
            "//:collect",
            infra(InfraKind::Store),
            Some("exec strategy: //:collect: the outputs cannot be read"),
            true,
        ),
        (
            "//:exit",
            OutcomeClass::ActionFailure { status: 3 },
            None,
            true,
        ),
        ("//:missing", missing, None, true),
        (
            "//:prepare",
            infra(InfraKind::Prepare),
            Some("exec strategy: //:prepare: no directory to run in"),
            false,
        ),
        (
            "//:refused",
            infra(InfraKind::Store),
            Some("content store: the content cannot be stored"),
            false,
        ),
        (
            "//:run",
            infra(InfraKind::Run),
            Some("exec strategy: //:run: the command cannot be started"),
            false,
        ),
        (
            "//:short",
            infra(InfraKind::Store),
            Some("the strategy answered for 0 outputs of 1"),
            true,
        ),
        ("//:timeout", infra(InfraKind::TimedOut), None, true),
    ];
    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 8,
                skipped: 0,
                built: 0,
                cached: 0
            },
            tests: Tally::default()
        }
    );
    let ports = pipeline.ports();
    assert_eq!(ports.cache.rows.len(), 0, "a failure is never recorded");
    for (raw, class, message, stored) in table {
        let outcome = outcome(&outcomes, raw);
        assert_eq!(outcome.class, class, "{raw}");
        assert_eq!(diagnostic(outcome).as_deref(), message, "{raw}");
        assert_eq!(outcome.outputs, [], "{raw}");
        assert_eq!(outcome.stdout.is_some(), stored, "{raw}");
        assert_eq!(outcome.stderr.is_some(), stored, "{raw}");
        if let Some(stdout) = outcome.stdout {
            let printed = format!("{raw}\n").into_bytes();
            assert_eq!(ports.cas.blob(&stdout), Some(printed), "{raw}");
        }
        let logged = event(&pipeline, raw);
        assert_eq!(&logged.outcome, outcome, "{raw}");
        assert!(logged.key.is_some(), "{raw}");
        assert!(
            logged.started.is_some() && logged.finished.is_some(),
            "a worker began {raw}"
        );
    }
}

#[test]
fn a_log_that_cannot_be_written_fails_the_build_and_keeps_what_was_already_recorded() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    pipeline.ports_mut().log.broken = true;

    let error = pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &Overrides::new(),
            OnFailure::KeepGoing,
            LiveOutput::Off,
        )
        .unwrap_err();

    assert_eq!(error.to_string(), "event log: the log cannot be written");
    assert!(matches!(error, Error::Adapter { port, .. } if port == PortName::EventLog));
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran(), [label("//:a")]);
    assert_eq!(
        ports.strategy.handed().len(),
        1,
        "the second task came back not begun"
    );
    assert_eq!(
        ports.clock.reads(),
        3,
        "two reads by the worker of //:a, one for its event, none for //:b"
    );
    let recorded: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
    assert_eq!(
        recorded,
        ["//:a"],
        "finished work survives a build that dies"
    );
}

/// Asserts that the build was refused before any of it ran: no action, nothing stored or placed,
/// no row, no event, and the clock never read.
fn assert_nothing_ran(pipeline: &Pipeline<FakePorts>) {
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.handed(), []);
    assert_eq!(ports.cas.digests().len(), 0);
    assert_eq!(ports.cas.placements(), []);
    assert_eq!(ports.cache.recorded(), []);
    assert_eq!(ports.log.events, []);
    assert_eq!(ports.clock.reads(), 0);
}

/// The error a build of the workspace ends in.
fn refused(pipeline: &mut Pipeline<FakePorts>) -> Error {
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &Overrides::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap_err()
}

/// `//:app`, and a test target that runs what `//:app` produces.
fn tested() -> Pipeline<FakePorts> {
    root_only(vec![
        target("app", &[]),
        target_with("app_test", |target| {
            target.role = TargetRole::Test;
            target.run = Some(vec![Written::new("$deps")]);
            target.deps = vec![Written::new("app")];
        }),
    ])
}

#[test]
fn a_test_that_runs_what_its_dependency_built_is_built_with_no_ceiling_entry() {
    let mut pipeline = tested();

    let outcomes = build_named(&mut pipeline, &["//:app_test"], OnFailure::Stop).unwrap();

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:app_test", OutcomeClass::Built),
        ])
    );
    let handed = pipeline.ports().strategy.handed();
    let test = handed
        .iter()
        .find(|action| action.label == label("//:app_test"))
        .unwrap();
    let command: Vec<String> = test
        .command
        .arguments()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(command, ["out/app"]);
    assert_eq!(test.deps, [out("//:app", "app")]);
}

/// `//:gen`, built from `gen.go` by `go`, and `//:banner`, which depends on it and runs
/// `command`. The ceiling allows `go` and nothing else, and `go` can be found.
fn generated(command: &[&str]) -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![
        target_with("gen", |target| {
            target.run = Some(["go", "build"].map(Written::new).to_vec());
            target.inputs = vec![Written::new("gen.go")];
        }),
        target_with("banner", |target| {
            target.run = Some(
                command
                    .iter()
                    .map(|argument| Written::new(*argument))
                    .collect(),
            );
            target.deps = vec![Written::new("gen")];
        }),
    ]);
    put(&mut pipeline, "gen.go", "package main", 100);
    let ports = pipeline.ports_mut();
    ports.manifest.workspace.ceiling.run = BTreeSet::from([program("go")]);
    ports
        .tools
        .fingerprints
        .insert(program("go"), Digest::of(b"go 1.23"));
    pipeline
}

#[test]
fn a_tool_a_dependency_built_runs_without_a_ceiling_entry() {
    let mut pipeline = generated(&["$deps", "$out"]);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:banner", OutcomeClass::Built),
            ("//:gen", OutcomeClass::Built),
        ])
    );
    assert_eq!(
        handed(&pipeline, "//:banner")[0],
        texts(&["out/gen", "out/banner"])
    );
}

#[test]
fn a_written_out_tool_a_dependency_built_runs_on_a_clean_build() {
    let mut pipeline = generated(&["out/gen", "$out"]);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:banner", OutcomeClass::Built),
            ("//:gen", OutcomeClass::Built),
        ])
    );
    assert_eq!(
        handed(&pipeline, "//:banner")[0],
        texts(&["out/gen", "out/banner"])
    );
}

#[test]
fn a_test_with_no_outputs_that_exits_zero_is_built() {
    let mut pipeline = root_only(vec![target_with("check", |target| {
        target.role = TargetRole::Test;
    })]);
    script(
        &mut pipeline,
        "//:check",
        leaving(out("//:check", "check"), Content::Missing),
    );

    let outcomes = build_named(&mut pipeline, &["//:check"], OnFailure::Stop).unwrap();

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:check", OutcomeClass::Built)])
    );
    assert_eq!(outcome(&outcomes, "//:check").outputs, []);
    assert!(pipeline.ports().cache.rows.contains_key(&label("//:check")));
}

/// A test target with nothing to run but the default command, which depends on `deps`.
fn testing(name: &str, deps: &[&str]) -> StagedItem {
    target_with(name, |target| {
        target.role = TargetRole::Test;
        target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
    })
}

#[test]
fn a_test_targets_outcomes_carry_its_role_whether_built_cached_failed_or_skipped() {
    let mut pipeline = root_only(vec![
        target("broken", &[]),
        testing("t_cached", &[]),
        testing("t_built", &[]),
        testing("t_failed", &[]),
        testing("t_skipped", &["broken"]),
    ]);
    script(&mut pipeline, "//:broken", ending(Ends::Exits(1)));
    script(&mut pipeline, "//:t_failed", ending(Ends::Exits(1)));
    build_named(&mut pipeline, &["//:t_cached"], OnFailure::KeepGoing).unwrap();

    let outcomes = build_named(
        &mut pipeline,
        &[
            "//:broken",
            "//:t_cached",
            "//:t_built",
            "//:t_failed",
            "//:t_skipped",
        ],
        OnFailure::KeepGoing,
    )
    .unwrap();

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:broken", OutcomeClass::ActionFailure { status: 1 }),
            ("//:t_cached", OutcomeClass::Cached),
            ("//:t_built", OutcomeClass::Built),
            ("//:t_failed", OutcomeClass::ActionFailure { status: 1 }),
            ("//:t_skipped", skipped("//:broken")),
        ])
    );
    let roles: BTreeMap<String, TargetRole> = outcomes
        .iter()
        .map(|outcome| (outcome.label.to_string(), outcome.role))
        .collect();
    assert_eq!(
        roles,
        BTreeMap::from([
            ("//:broken".to_owned(), TargetRole::Build),
            ("//:t_cached".to_owned(), TargetRole::Test),
            ("//:t_built".to_owned(), TargetRole::Test),
            ("//:t_failed".to_owned(), TargetRole::Test),
            ("//:t_skipped".to_owned(), TargetRole::Test),
        ])
    );
    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 2,
                skipped: 1,
                built: 1,
                cached: 1
            },
            tests: Tally {
                failed: 1,
                skipped: 1,
                built: 1,
                cached: 1
            }
        }
    );
}

#[test]
fn a_program_first_named_by_expansion_is_refused_before_anything_runs() {
    let mut pipeline = root_only(vec![target_with("gen", |target| {
        target.run = Some(vec![Written::new("$in")]);
        target.inputs = vec![Written::new("scripts/gen.sh")];
    })]);
    put(&mut pipeline, "scripts/gen.sh", "echo gen", 100);

    let error = refused(&mut pipeline);

    assert_eq!(
        error.to_string(),
        "build.lua: //:gen: program scripts/gen.sh is outside the ceiling (proc.run allows: cc)"
    );
    match &error {
        Error::Refused { want, site, .. } => {
            assert_eq!(want, &Want::Program(program("scripts/gen.sh")));
            let site = site.as_deref().unwrap();
            assert_eq!(site.label, label("//:gen"));
        }
        other => unreachable!("expected Refused, got {other:?}"),
    }
    assert_nothing_ran(&pipeline);
}

#[test]
fn a_command_that_expands_to_nothing_names_no_program() {
    let mut pipeline = root_only(vec![target_with("gen", |target| {
        target.run = Some(vec![Written::new("$in")]);
    })]);

    let error = refused(&mut pipeline);

    assert_eq!(
        error.to_string(),
        "build.lua: //:gen: the command names no program once expanded"
    );
    assert!(matches!(error, Error::NoProgram { .. }));
    assert_nothing_ran(&pipeline);
}

/// `//:app`, running the program the setting `tool` names, which is `cc`.
fn tool_by_setting() -> Pipeline<FakePorts> {
    root_only(vec![
        StagedItem::Setting(StagedSetting {
            name: Written::new("tool"),
            default: Written::new("cc"),
        }),
        target_with("app", |target| {
            target.run = Some(["$opt:tool", "-o", "$out"].map(Written::new).to_vec());
        }),
    ])
}

#[test]
fn a_tool_named_through_a_setting_runs_under_its_name_and_runs_again_once_replaced() {
    let mut pipeline = tool_by_setting();

    let outcomes = build(&mut pipeline, OnFailure::Stop);
    assert_eq!(
        classes(&outcomes),
        expected(&[("//:app", OutcomeClass::Built)])
    );
    assert_eq!(
        handed(&pipeline, "//:app")[0],
        texts(&["cc", "-o", "out/app"])
    );
    let outcomes = build(&mut pipeline, OnFailure::Stop);
    assert_eq!(
        classes(&outcomes),
        expected(&[("//:app", OutcomeClass::Cached)])
    );

    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 15.1"));
    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:app", OutcomeClass::Built)])
    );
    assert_eq!(pipeline.ports().strategy.runs(&label("//:app")), 2);
}

#[test]
fn a_tool_named_through_a_setting_that_cannot_be_found_is_refused_before_anything_runs() {
    let mut pipeline = tool_by_setting();
    pipeline.ports_mut().tools.fingerprints.clear();

    let error = refused(&mut pipeline);

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program cc was not found"
    );
    assert!(matches!(error, Error::MissingTool { .. }));
    assert_nothing_ran(&pipeline);
}

#[test]
fn two_outputs_that_would_be_written_to_one_place_are_refused() {
    let writing_x = |name: &str| {
        target_with(name, |target| {
            target.outputs = Some(vec![Written::new("x")]);
        })
    };
    let mut same_path = root_only(vec![writing_x("a"), writing_x("b")]);

    let error = refused(&mut same_path);

    assert_eq!(
        error.to_string(),
        "out/x: //:a (build.lua) and //:b (build.lua) cannot both write here"
    );
    assert!(matches!(error, Error::OutputConflict { .. }));
    assert_nothing_ran(&same_path);

    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("lib", &[])], &["lib"])),
        ("lib", file(vec![target("text", &[])], &[])),
    ]));
    let mut file_and_directory = Pipeline::new(ports(source));
    let tools = &mut file_and_directory.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));

    let error = refused(&mut file_and_directory);

    assert_eq!(
        error.to_string(),
        "out/lib: //:lib (build.lua) and //lib:text (lib/build.lua) cannot both write here"
    );
    assert!(matches!(error, Error::OutputConflict { .. }));
    assert_nothing_ran(&file_and_directory);
}

#[test]
fn a_load_error_and_a_refusal_of_the_plan_reach_the_caller_unchanged() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("app", &[])], &["lib"])),
        ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
    ]));
    let mut unreadable = Pipeline::<FakePorts>::new(ports(source));

    match refused(&mut unreadable) {
        Error::Evaluation {
            provenance,
            failure,
            ..
        } => {
            assert_eq!(provenance.file(), Path::new("lib/build.lua"));
            assert_eq!(failure, EvaluationFailure::Syntax);
        }
        other => unreachable!("expected Evaluation, got {other:?}"),
    }
    assert_nothing_ran(&unreadable);

    let mut disallowed = chain();
    let ceiling = &mut disallowed.ports_mut().manifest.workspace.ceiling;
    ceiling.run.clear();

    let error = refused(&mut disallowed);

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program cc is outside the ceiling (proc.run allows: nothing)"
    );
    assert!(matches!(error, Error::Refused { .. }));
    assert_nothing_ran(&disallowed);
}

/// The workspace of the graph flows, [`crate::graph::fixture`]: a rule, a target using it, a target
/// with dependencies in two directories, an alias, the test target and a setting. `cc` can be
/// found, and the workspace allows `out/app` to run.
fn two_directories(order: Order) -> Pipeline<FakePorts> {
    let mut pipeline = Pipeline::new(ports(crate::graph::fixture()));
    put(&mut pipeline, "main.c", "main.c", 100);
    put(&mut pipeline, "lib/text.c", "lib/text.c", 100);
    let ports = pipeline.ports_mut();
    ports
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    ports
        .manifest
        .workspace
        .ceiling
        .run
        .insert(program("out/app"));
    ports.dispatcher.order = order;
    pipeline
}

/// A build of `//:app_test` of [`two_directories`], whose closure is the whole workspace, with
/// `test_filter` set to `Login`.
fn build_filtered(pipeline: &mut Pipeline<FakePorts>) -> Vec<ActionOutcome> {
    let overrides = Overrides::from([(
        SettingName::parse("test_filter").unwrap(),
        SettingValue::parse("Login").unwrap(),
    )]);
    pipeline
        .build(
            &entry(),
            &Scope::Targets(BTreeSet::from([label("//:app_test")])),
            &overrides,
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap()
}

/// The command and the three lists of paths an action was handed, as text.
fn handed(pipeline: &Pipeline<FakePorts>, raw: &str) -> [Vec<String>; 4] {
    let wanted = label(raw);
    let actions = pipeline.ports().strategy.handed();
    let action = actions
        .iter()
        .find(|action| action.label == wanted)
        .unwrap();
    [
        action
            .command
            .arguments()
            .iter()
            .map(ToString::to_string)
            .collect(),
        action.inputs.iter().map(ToString::to_string).collect(),
        action.deps.iter().map(ToString::to_string).collect(),
        action.outputs.iter().map(ToString::to_string).collect(),
    ]
}

/// Texts as owned strings.
fn texts(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|text| (*text).to_owned()).collect()
}

#[test]
fn a_two_directory_workspace_hands_each_action_its_expanded_command_and_logs_the_pinned_events() {
    let mut pipeline = two_directories(Order::OldestFirst);

    let outcomes = build_filtered(&mut pipeline);

    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 4,
                cached: 0
            },
            tests: Tally {
                failed: 0,
                skipped: 0,
                built: 1,
                cached: 0
            }
        }
    );
    assert_eq!(
        handed(&pipeline, "//:main.o"),
        [
            texts(&["cc", "-c", "main.c", "-o", "out/main.o"]),
            texts(&["main.c"]),
            texts(&[]),
            texts(&["out/main.o"]),
        ]
    );
    assert_eq!(
        handed(&pipeline, "//lib:text"),
        [
            texts(&["cc"]),
            texts(&["lib/text.c"]),
            texts(&[]),
            texts(&["out/lib/text"]),
        ]
    );
    assert_eq!(
        handed(&pipeline, "//:app"),
        [
            texts(&["cc", "out/main.o", "out/lib/text", "-o", "out/app"]),
            texts(&[]),
            texts(&["out/main.o", "out/lib/text"]),
            texts(&["out/app"]),
        ]
    );
    assert_eq!(
        handed(&pipeline, "//:app_test"),
        [
            texts(&["out/app", "--filter=Login"]),
            texts(&[]),
            texts(&["out/app"]),
            texts(&[]),
        ]
    );

    let lines: Vec<String> = pipeline
        .ports()
        .log
        .events
        .iter()
        .map(|event| json::canonical::to_string(event).unwrap())
        .collect();
    assert_eq!(lines.join("\n"), GOLDEN.trim_end());
}

#[test]
fn the_order_tasks_are_taken_in_changes_neither_a_row_nor_what_is_stored() {
    let mut oldest = two_directories(Order::OldestFirst);
    let mut newest = two_directories(Order::NewestFirst);

    build_filtered(&mut oldest);
    build_filtered(&mut newest);

    assert_ne!(
        oldest.ports().strategy.ran(),
        newest.ports().strategy.ran(),
        "the two builds ran their actions in different orders"
    );
    assert_eq!(oldest.ports().cache.rows, newest.ports().cache.rows);
    assert_eq!(oldest.ports().cas.digests(), newest.ports().cas.digests());
    assert_eq!(oldest.ports().cas.placed(), newest.ports().cas.placed());
}

/// The targets of the fault a build ended in, when it ended in a dispatcher that lost some.
fn lost(error: Error) -> Vec<String> {
    match error {
        Error::DispatcherFault { targets } => targets.iter().map(ToString::to_string).collect(),
        other => unreachable!("expected a dispatcher fault, got {other:?}"),
    }
}

#[test]
fn a_dispatcher_that_loses_a_task_fails_the_build_naming_every_target_left_without_an_outcome() {
    for loss in [Loss::Unrun, Loss::Unreported] {
        for on_failure in [OnFailure::Stop, OnFailure::KeepGoing] {
            let mut pipeline = chain();
            // `//:docs` and `//:gen` start first, in that order: the second task is `//:gen`.
            pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

            let error = pipeline
                .build(
                    &entry(),
                    &Scope::Workspace,
                    &Overrides::new(),
                    on_failure,
                    LiveOutput::Off,
                )
                .unwrap_err();

            assert_eq!(
                error.to_string(),
                "dispatcher: returned without an outcome for //:app, //:gen, //:lib",
                "{loss:?}, {on_failure:?}"
            );
            assert_eq!(
                lost(error),
                ["//:app", "//:gen", "//:lib"],
                "the lost target, and what waited on it, in label order"
            );
        }
    }
}

#[test]
fn the_targets_that_ended_before_a_task_was_lost_keep_their_content_row_and_event() {
    for loss in [Loss::Unrun, Loss::Unreported] {
        let mut pipeline = chain();
        pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

        let error = pipeline
            .build(
                &entry(),
                &Scope::Workspace,
                &Overrides::new(),
                OnFailure::Stop,
                LiveOutput::Off,
            )
            .unwrap_err();

        assert!(matches!(error, Error::DispatcherFault { .. }), "{loss:?}");
        let ports = pipeline.ports();
        let recorded: Vec<String> = ports.cache.rows.keys().map(ToString::to_string).collect();
        assert_eq!(recorded, ["//:docs"], "{loss:?}: a row only for what ended");
        let logged: Vec<String> = ports
            .log
            .events
            .iter()
            .map(|event| event.outcome.label.to_string())
            .collect();
        assert_eq!(
            logged,
            ["//:docs"],
            "{loss:?}: an event only for what ended"
        );
        let row = ports.cache.rows.get(&label("//:docs")).unwrap();
        let held = row.outputs().iter().copied();
        for digest in held.chain([row.stdout(), row.stderr()]) {
            assert!(ports.cas.blob(&digest).is_some(), "{loss:?}: {digest}");
        }
        assert_eq!(ports.cas.placed().len(), 1, "{loss:?}");

        // The next build, with a dispatcher that loses nothing, finds `//:docs` where it was
        // left and builds the rest.
        pipeline.ports_mut().dispatcher.lose.clear();
        let outcomes = build(&mut pipeline, OnFailure::Stop);
        assert_eq!(
            classes(&outcomes),
            expected(&[
                ("//:app", OutcomeClass::Built),
                ("//:docs", OutcomeClass::Cached),
                ("//:gen", OutcomeClass::Built),
                ("//:lib", OutcomeClass::Built),
            ]),
            "{loss:?}"
        );
    }
}

#[test]
fn a_dispatcher_that_loses_nothing_loses_no_target() {
    let mut pipeline = chain();
    pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(99, Loss::Unrun)]);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(outcomes.len(), 4);
}

/// The description the action of the target `raw` names was handed, as text.
fn described(pipeline: &Pipeline<FakePorts>, raw: &str) -> Option<String> {
    let wanted = label(raw);
    let actions = pipeline.ports().strategy.handed();
    let action = actions
        .iter()
        .find(|action| action.label == wanted)
        .unwrap();
    action.description.as_ref().map(ToString::to_string)
}

/// The declarations of one root build file: the rule `cc` running `cc -c $in -o $out` and shown
/// as `description`, the setting `mode`, the target `lib` whose own command is inline, and the
/// targets `one` and `two` using the rule, `one` over `inputs`.
fn described_items(description: Option<&str>, inputs: &[&str]) -> Vec<StagedItem> {
    let using = |name: &str, inputs: &[&str]| {
        target_with(name, |target| {
            target.run = None;
            target.rule = Some(Written::new("cc"));
            target.inputs = inputs.iter().map(|input| Written::new(*input)).collect();
            if name == "two" {
                target.deps = vec![Written::new("one"), Written::new("lib")];
            }
        })
    };
    vec![
        rule("cc", &["cc", "-c", "$in", "-o", "$out"], description),
        setting("mode"),
        target("lib", &[]),
        using("one", inputs),
        using("two", &["z.c"]),
    ]
}

/// A pipeline over [`described_items`], with the files they read.
fn described_rule(description: Option<&str>, inputs: &[&str]) -> Pipeline<FakePorts> {
    let mut pipeline = root_only(described_items(description, inputs));
    for name in ["a.c", "b.c", "z.c"] {
        put(&mut pipeline, name, name, 100);
    }
    pipeline
}

#[test]
fn a_description_is_expanded_by_the_rules_of_a_command_and_handed_with_the_action() {
    let mut pipeline = described_rule(
        Some("cc $in as $out after $deps [$opt:mode] $input"),
        &["b.c", "a.c"],
    );
    let overrides = Overrides::from([(
        SettingName::parse("mode").unwrap(),
        SettingValue::parse("fast").unwrap(),
    )]);
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &overrides,
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();

    assert_eq!(
        described(&pipeline, "//:one").as_deref(),
        Some("cc b.c a.c as out/one after  [fast] $input"),
        "lists in written order, joined by one space; `$deps` of nothing is empty"
    );
    assert_eq!(
        described(&pipeline, "//:two").as_deref(),
        Some("cc z.c as out/two after out/one out/lib [fast] $input"),
        "every output of every dependency"
    );
    assert_eq!(
        described(&pipeline, "//:lib"),
        None,
        "an inline command has no description, and none is made up"
    );
}

#[test]
fn a_description_naming_a_setting_its_command_does_not_is_expanded_with_the_overridden_value() {
    let description = Some("compile $in [$opt:mode]");
    let mut defaulted = described_rule(description, &["a.c"]);
    let mut overridden = described_rule(description, &["a.c"]);
    let overrides = Overrides::from([(
        SettingName::parse("mode").unwrap(),
        SettingValue::parse("fast").unwrap(),
    )]);

    build(&mut defaulted, OnFailure::Stop);
    overridden
        .build(
            &entry(),
            &Scope::Workspace,
            &overrides,
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();

    assert_eq!(
        described(&defaulted, "//:one").as_deref(),
        Some("compile a.c []"),
        "the default, which is empty"
    );
    assert_eq!(
        described(&overridden, "//:one").as_deref(),
        Some("compile a.c [fast]")
    );
    let row = overridden.ports().cache.rows.get(&label("//:one")).unwrap();
    assert!(
        row.key().components().own.settings.is_empty(),
        "the command names no setting, so the key holds none"
    );
    assert_eq!(
        overridden.ports().cache.rows,
        defaulted.ports().cache.rows,
        "and the override changed no key"
    );
}

#[test]
fn a_target_using_a_rule_without_a_description_is_handed_none() {
    let mut pipeline = described_rule(None, &["a.c"]);

    build(&mut pipeline, OnFailure::Stop);

    assert_eq!(described(&pipeline, "//:one"), None);
    assert_eq!(described(&pipeline, "//:two"), None);
}

#[test]
fn a_setting_value_cannot_break_a_description_across_lines() {
    let mut pipeline = described_rule(Some("mode $opt:mode."), &["a.c"]);
    let overrides = Overrides::from([(
        SettingName::parse("mode").unwrap(),
        SettingValue::parse("fast\nand\tloose").unwrap(),
    )]);

    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &overrides,
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();

    assert_eq!(
        described(&pipeline, "//:one").as_deref(),
        Some("mode fast and loose."),
        "a control character in a substituted value is shown as a space"
    );
}

#[test]
fn a_description_naming_an_undeclared_setting_is_refused_like_a_command_before_anything_runs() {
    let mut pipeline = described_rule(Some("compile $in in $opt:mod"), &["a.c"]);

    let error = refused(&mut pipeline);

    match error {
        Error::UnknownSetting {
            site,
            name,
            suggestion,
        } => {
            assert_eq!(site.label, label("//:cc"), "the rule holds the reference");
            assert_eq!(name, "mod");
            assert_eq!(suggestion, Some(SettingName::parse("mode").unwrap()));
        }
        other => unreachable!("expected UnknownSetting, got {other:?}"),
    }
    assert_nothing_ran(&pipeline);
}

#[test]
fn editing_only_a_description_leaves_every_target_cached() {
    let mut pipeline = described_rule(Some("compile $in"), &["a.c"]);
    build(&mut pipeline, OnFailure::Stop);
    let rows = pipeline.ports().cache.rows.clone();
    let ran = pipeline.ports().strategy.ran().len();

    pipeline.ports_mut().source = FakeSource::new(workspace(vec![(
        "",
        file(
            described_items(Some("building $in for $out"), &["a.c"]),
            &[],
        ),
    )]));
    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 0,
                cached: 3
            },
            tests: Tally::default()
        }
    );
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.ran().len(), ran, "no action ran again");
    assert_eq!(ports.cache.rows, rows, "no key changed");
}

/// A script in which the action leaves `content` at `at` and is otherwise unscripted.
fn leaving(at: OutputPath, content: Content) -> Script {
    Script {
        outputs: BTreeMap::from([(at, content)]),
        ..Script::default()
    }
}

/// What is at each workspace path now: the digest of the file there, by the path's text.
fn placed(pipeline: &Pipeline<FakePorts>) -> BTreeMap<String, Digest> {
    let placed = pipeline.ports().cas.placed();
    placed
        .iter()
        .map(|(path, digest)| (path.to_string(), *digest))
        .collect()
}

/// Files by path, as [`placed`] reports them: each `(path, text)` a file holding that text.
fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, Digest> {
    pairs
        .iter()
        .map(|(path, text)| ((*path).to_owned(), Digest::of(text.as_bytes())))
        .collect()
}

/// The digest of the one output the target `raw` names produced.
fn output_of(outcomes: &[ActionOutcome], raw: &str) -> Digest {
    let outputs = &outcome(outcomes, raw).outputs;
    assert_eq!(outputs.len(), 1, "{raw}");
    outputs.first().unwrap().digest
}

#[test]
fn a_directory_output_is_stored_file_by_file_and_put_back_whole_without_running() {
    let mut pipeline = root_only(vec![target("site", &[])]);
    let listing = tree(&[
        ("index.html", "page"),
        ("assets/app.js", "script"),
        ("assets/app.css", "style"),
    ]);
    script(
        &mut pipeline,
        "//:site",
        leaving(out("//:site", "site"), listing),
    );

    let first = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&first),
        expected(&[("//:site", OutcomeClass::Built)])
    );
    let digest = output_of(&first, "//:site");
    let row = pipeline.ports().cache.rows.get(&label("//:site")).unwrap();
    assert_eq!(row.outputs(), [digest], "one digest for the whole tree");
    assert_eq!(
        tree_of(&pipeline, &digest),
        [
            (
                "assets/app.css".to_owned(),
                Digest::of(b"style"),
                FileMode::Plain
            ),
            (
                "assets/app.js".to_owned(),
                Digest::of(b"script"),
                FileMode::Plain
            ),
            (
                "index.html".to_owned(),
                Digest::of(b"page"),
                FileMode::Plain
            ),
        ],
        "the manifest lists every file, in path order"
    );
    let whole = files(&[
        ("out/site/assets/app.css", "style"),
        ("out/site/assets/app.js", "script"),
        ("out/site/index.html", "page"),
    ]);
    assert_eq!(placed(&pipeline), whole);
    for text in ["page", "script", "style"] {
        let held = pipeline.ports().cas.blob(&Digest::of(text.as_bytes()));
        assert_eq!(held, Some(text.as_bytes().to_vec()));
    }

    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[("//:site", OutcomeClass::Cached)])
    );
    assert_eq!(output_of(&second, "//:site"), digest);
    let ports = pipeline.ports();
    assert_eq!(ports.strategy.runs(&label("//:site")), 1, "it did not run");
    assert_eq!(placed(&pipeline), whole);
    assert_eq!(
        ports.cas.placements().len(),
        6,
        "every file of the tree was placed again"
    );
}

/// `//:gen` reads `gen.c` and leaves a directory; `//:lib` needs it.
fn generating() -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![
        target_with("gen", |target| {
            target.inputs = vec![Written::new("gen.c")];
        }),
        target("lib", &["gen"]),
    ]);
    put(&mut pipeline, "gen.c", "v1", 100);
    pipeline
}

#[test]
fn a_directory_rebuilt_to_the_same_files_leaves_its_dependent_cached_and_one_changed_file_does_not()
{
    let mut pipeline = generating();
    let at = out("//:gen", "gen");
    let headers = tree(&[("a.h", "one"), ("sub/b.h", "two")]);
    script(&mut pipeline, "//:gen", leaving(at.clone(), headers));
    let first = build(&mut pipeline, OnFailure::Stop);

    put(&mut pipeline, "gen.c", "v2", 200);
    let reordered = tree(&[("sub/b.h", "two"), ("a.h", "one")]);
    script(&mut pipeline, "//:gen", leaving(at.clone(), reordered));
    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[
            ("//:gen", OutcomeClass::Built),
            ("//:lib", OutcomeClass::Cached),
        ])
    );
    assert_eq!(
        output_of(&second, "//:gen"),
        output_of(&first, "//:gen"),
        "the same files in another order are the same output"
    );
    assert_eq!(
        pipeline.ports().strategy.runs(&label("//:lib")),
        1,
        "early cutoff"
    );

    put(&mut pipeline, "gen.c", "v3", 300);
    let edited = tree(&[("a.h", "one"), ("sub/b.h", "two, edited")]);
    script(&mut pipeline, "//:gen", leaving(at, edited));
    let third = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&third),
        expected(&[
            ("//:gen", OutcomeClass::Built),
            ("//:lib", OutcomeClass::Built),
        ])
    );
    assert_ne!(output_of(&third, "//:gen"), output_of(&first, "//:gen"));
    assert_eq!(pipeline.ports().strategy.runs(&label("//:lib")), 2);
}

#[test]
fn a_recorded_directory_missing_one_file_is_built_again_and_never_half_put_back() {
    let mut pipeline = root_only(vec![target("docs", &[]), target("site", &[])]);
    let at = out("//:site", "site");
    let listing = tree(&[("index.html", "page"), ("assets/app.js", "script")]);
    script(
        &mut pipeline,
        "//:site",
        leaving(at.clone(), listing.clone()),
    );
    let first = build(&mut pipeline, OnFailure::KeepGoing);
    let digest = output_of(&first, "//:site");
    let before = pipeline.ports().cas.placements().len();

    pipeline.ports().cas.forget(&Digest::of(b"script"));
    assert!(
        pipeline.ports().cas.blob(&digest).is_some(),
        "the manifest is still held"
    );
    script(&mut pipeline, "//:site", ending(Ends::Exits(1)));
    let second = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&second),
        expected(&[
            ("//:docs", OutcomeClass::Cached),
            ("//:site", OutcomeClass::ActionFailure { status: 1 }),
        ]),
        "the row was no reason to skip the action"
    );
    {
        let ports = pipeline.ports();
        assert_eq!(ports.strategy.runs(&label("//:site")), 2);
        let since: Vec<String> = ports
            .cas
            .placements()
            .iter()
            .skip(before)
            .map(|(path, _)| path.to_string())
            .collect();
        assert_eq!(since, ["out/docs"], "no file of the tree was put back");
    }

    script(&mut pipeline, "//:site", leaving(at, listing));
    let third = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&third),
        expected(&[
            ("//:docs", OutcomeClass::Cached),
            ("//:site", OutcomeClass::Built),
        ])
    );
    assert_eq!(output_of(&third, "//:site"), digest);
    assert!(
        pipeline.ports().cas.blob(&Digest::of(b"script")).is_some(),
        "the missing file is stored again"
    );
}

#[test]
fn a_target_may_leave_a_file_and_a_directory_side_by_side() {
    let mut pipeline = root_only(vec![target_with("app", |target| {
        target.outputs = Some(vec![Written::new("app"), Written::new("share")]);
    })]);
    let both = Script {
        outputs: BTreeMap::from([
            (out("//:app", "app"), Content::Runs(b"binary".to_vec())),
            (out("//:app", "share"), tree(&[("man/app.1", "manual")])),
        ]),
        ..Script::default()
    };
    script(&mut pipeline, "//:app", both);

    let first = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&first),
        expected(&[("//:app", OutcomeClass::Built)])
    );
    let outputs = &outcome(&first, "//:app").outputs;
    let names: Vec<String> = outputs
        .iter()
        .map(|output| output.name.to_string())
        .collect();
    assert_eq!(names, ["app", "share"]);
    let digests: Vec<Digest> = outputs.iter().map(|output| output.digest).collect();
    assert_eq!(
        file_of(&pipeline, &digests[0]),
        StoredFile {
            digest: Digest::of(b"binary"),
            mode: FileMode::Executable,
        }
    );
    assert_eq!(
        tree_of(&pipeline, &digests[1]),
        [(
            "man/app.1".to_owned(),
            Digest::of(b"manual"),
            FileMode::Plain
        )]
    );
    let both_placed = files(&[("out/app", "binary"), ("out/share/man/app.1", "manual")]);
    assert_eq!(placed(&pipeline), both_placed);

    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[("//:app", OutcomeClass::Cached)])
    );
    assert_eq!(outcome(&second, "//:app").outputs, *outputs);
    assert_eq!(placed(&pipeline), both_placed);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:app")), 1);
}

#[test]
fn a_file_that_may_be_run_keeps_that_through_the_store_and_back() {
    let mut pipeline = root_only(vec![
        target_with("app", |target| {
            target.inputs = vec![Written::new("main.c")];
        }),
        target("check", &["app"]),
    ]);
    put(&mut pipeline, "main.c", "v1", 100);
    let at = out("//:app", "app");
    let runnable = Content::Runs(b"binary".to_vec());
    script(&mut pipeline, "//:app", leaving(at.clone(), runnable));

    let first = build(&mut pipeline, OnFailure::Stop);

    let digest = output_of(&first, "//:app");
    assert_eq!(
        file_of(&pipeline, &digest),
        StoredFile {
            digest: Digest::of(b"binary"),
            mode: FileMode::Executable,
        }
    );
    let mode = |pipeline: &Pipeline<FakePorts>| pipeline.ports().cas.modes().get(&at).copied();
    assert_eq!(mode(&pipeline), Some(FileMode::Executable));

    let second = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&second),
        expected(&[
            ("//:app", OutcomeClass::Cached),
            ("//:check", OutcomeClass::Cached),
        ])
    );
    assert_eq!(
        mode(&pipeline),
        Some(FileMode::Executable),
        "put back from the store as a file that may be run"
    );

    put(&mut pipeline, "main.c", "v2", 200);
    let inert = Content::Holds(b"binary".to_vec());
    script(&mut pipeline, "//:app", leaving(at.clone(), inert));
    let third = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&third),
        expected(&[
            ("//:app", OutcomeClass::Built),
            ("//:check", OutcomeClass::Built),
        ]),
        "the same bytes that may no longer be run are another output"
    );
    assert_ne!(output_of(&third, "//:app"), digest);
    assert_eq!(mode(&pipeline), Some(FileMode::Plain));
}

#[test]
fn a_directory_put_in_place_leaves_nothing_of_what_was_there_before() {
    let mut pipeline = root_only(vec![target_with("site", |target| {
        target.inputs = vec![Written::new("site.md")];
    })]);
    let at = out("//:site", "site");
    put(&mut pipeline, "site.md", "v1", 100);
    let two = tree(&[("index.html", "page"), ("old.html", "old")]);
    script(&mut pipeline, "//:site", leaving(at.clone(), two));
    build(&mut pipeline, OnFailure::Stop);
    assert_eq!(
        placed(&pipeline),
        files(&[
            ("out/site/index.html", "page"),
            ("out/site/old.html", "old")
        ])
    );

    put(&mut pipeline, "site.md", "v2", 200);
    let one = tree(&[("index.html", "page")]);
    script(&mut pipeline, "//:site", leaving(at.clone(), one));
    build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        placed(&pipeline),
        files(&[("out/site/index.html", "page")]),
        "the file the tree no longer holds is gone"
    );

    put(&mut pipeline, "site.md", "v3", 300);
    let flat = Content::Holds(b"page".to_vec());
    script(&mut pipeline, "//:site", leaving(at, flat));
    build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        placed(&pipeline),
        files(&[("out/site", "page")]),
        "a file where the directory was"
    );
}

/// The path of the file `entry` below the directory output `name` of the target `raw`.
fn below(raw: &str, name: &str, entry: &str) -> OutputPath {
    out(raw, name).entry(&TreePath::parse(entry).unwrap())
}

#[test]
fn a_built_directory_that_cannot_be_placed_whole_leaves_nothing_at_its_path() {
    let mut pipeline = root_only(vec![target("site", &[])]);
    let listing = tree(&[("assets/app.js", "script"), ("index.html", "page")]);
    script(
        &mut pipeline,
        "//:site",
        leaving(out("//:site", "site"), listing),
    );
    let page = below("//:site", "site", "index.html");
    pipeline.ports().cas.fail_place(page);

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:site", infra(InfraKind::Store))])
    );
    assert_eq!(
        diagnostic(outcome(&outcomes, "//:site")).as_deref(),
        Some("content store: out/site/index.html cannot be written"),
        "the failure reported is the placement's"
    );
    let ports = pipeline.ports();
    assert_eq!(
        ports.cas.placements(),
        [(
            below("//:site", "site", "assets/app.js"),
            Digest::of(b"script")
        )],
        "the file before the failure was placed"
    );
    assert_eq!(placed(&pipeline), files(&[]), "and then removed again");
    assert_eq!(ports.cache.rows.len(), 0, "no row is recorded");
}

#[test]
fn a_cached_directory_that_cannot_be_put_back_whole_leaves_nothing_at_its_path() {
    let mut pipeline = root_only(vec![target("docs", &[]), target("site", &[])]);
    let listing = tree(&[("assets/app.js", "script"), ("index.html", "page")]);
    script(
        &mut pipeline,
        "//:site",
        leaving(out("//:site", "site"), listing),
    );
    build(&mut pipeline, OnFailure::KeepGoing);
    let rows = pipeline.ports().cache.rows.clone();
    let page = below("//:site", "site", "index.html");
    pipeline.ports().cas.fail_place(page);

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    assert_eq!(
        classes(&outcomes),
        expected(&[
            ("//:docs", OutcomeClass::Cached),
            ("//:site", infra(InfraKind::Store)),
        ])
    );
    assert_eq!(
        diagnostic(outcome(&outcomes, "//:site")).as_deref(),
        Some("content store: out/site/index.html cannot be written")
    );
    let docs = pipeline.ports().cas.placed();
    assert_eq!(
        docs.keys().map(ToString::to_string).collect::<Vec<_>>(),
        ["out/docs"],
        "nothing of the directory is left, from this build or the last"
    );
    let ports = pipeline.ports();
    assert_eq!(ports.cache.rows, rows, "its row is unchanged");
    assert_eq!(ports.strategy.runs(&label("//:site")), 1, "it did not run");
}

#[test]
fn an_output_that_cannot_be_put_back_exactly_is_refused_and_never_recorded() {
    let names = [
        "empty", "escape", "hollow", "link", "linked", "pipe", "unnamed",
    ];
    let mut pipeline = root_only(names.iter().map(|name| target(name, &[])).collect());
    let page = || Leaf::Holds(b"page".to_vec());
    let scripts = [
        ("empty", Content::Tree(Vec::new())),
        ("escape", tree(&[("../stolen", "page")])),
        (
            "hollow",
            Content::Tree(vec![
                ("index.html".to_owned(), page()),
                ("cache".to_owned(), Leaf::Empty),
            ]),
        ),
        ("link", Content::Link),
        (
            "linked",
            Content::Tree(vec![
                ("index.html".to_owned(), page()),
                ("bin/tool".to_owned(), Leaf::Link),
            ]),
        ),
        ("pipe", Content::Special),
        (
            "unnamed",
            Content::Tree(vec![("caf\u{fffd}".to_owned(), Leaf::NotText)]),
        ),
    ];
    for (name, content) in scripts {
        let raw = format!("//:{name}");
        script(&mut pipeline, &raw, leaving(out(&raw, name), content));
    }

    let outcomes = build(&mut pipeline, OnFailure::KeepGoing);

    let refused = |name: &str, refusal| OutcomeClass::OutputRefused {
        name: OutputName::parse(name).unwrap(),
        refusal,
    };
    let table = [
        (
            "//:empty",
            refused("empty", OutputRefusal::EmptyDirectory),
            "empty is an empty directory",
        ),
        (
            "//:escape",
            infra(InfraKind::Store),
            "the strategy reported escape: invalid tree path: \"../stolen\" — \
             must not contain a '.' or '..' segment",
        ),
        (
            "//:hollow",
            refused("hollow", OutputRefusal::EmptyDirectory),
            "hollow/cache is an empty directory",
        ),
        (
            "//:link",
            refused("link", OutputRefusal::Symlink),
            "link is a symbolic link",
        ),
        (
            "//:linked",
            refused("linked", OutputRefusal::Symlink),
            "linked/bin/tool is a symbolic link",
        ),
        (
            "//:pipe",
            refused("pipe", OutputRefusal::Special),
            "pipe is neither a file nor a directory",
        ),
        (
            "//:unnamed",
            refused("unnamed", OutputRefusal::NotText),
            "unnamed/caf\u{fffd} has a name that is not valid Unicode",
        ),
    ];
    assert_eq!(
        Summary::of(&outcomes),
        Summary {
            all: Tally {
                failed: 7,
                skipped: 0,
                built: 0,
                cached: 0
            },
            tests: Tally::default()
        }
    );
    for (raw, class, message) in table {
        let outcome = outcome(&outcomes, raw);
        assert_eq!(outcome.class, class, "{raw}");
        assert_eq!(diagnostic(outcome).as_deref(), Some(message), "{raw}");
        assert_eq!(outcome.outputs, [], "{raw}");
        assert!(outcome.stdout.is_some(), "what {raw} printed was stored");
        assert_eq!(&event(&pipeline, raw).outcome, outcome, "{raw}");
    }
    let ports = pipeline.ports();
    assert_eq!(ports.cache.rows.len(), 0, "a refusal is never recorded");
    assert_eq!(ports.cas.placements(), [], "and nothing is put in place");
}

/// The set a build of `pipeline` is planned to want.
fn planned_wants(pipeline: &mut Pipeline<FakePorts>) -> WantedSet {
    pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap()
        .wanted()
        .clone()
}

#[test]
fn a_build_asks_the_approver_once_with_the_planned_wanted_set() {
    let mut pipeline = chain();
    let wanted = planned_wants(&mut pipeline);
    assert!(!wanted.is_empty(), "the fixture runs a program");

    build(&mut pipeline, OnFailure::Stop);

    assert_eq!(pipeline.ports().approver.asked, [wanted]);
}

#[test]
fn a_build_hands_the_approver_the_attribution_of_the_plan() {
    let mut go = plugin("go", "1.2.0");
    go.run = BTreeSet::from([program("cc")]);
    let origin = Origin::plugins(vec![go]).unwrap();
    let source = FakeSource::new(workspace(vec![(
        "",
        file_by(&origin, vec![target("app", &[])], &[]),
    )]));
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));
    let ports = pipeline.ports_mut();
    ports.manifest.workspace.plugins.insert(
        PluginName::parse("go").unwrap(),
        PluginCap {
            run: BTreeSet::from([program("cc")]),
            ..PluginCap::default()
        },
    );
    ports
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    let planned = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap()
        .attribution()
        .clone();
    assert_eq!(
        planned.to_string(),
        "  cc: declared by plugin go@1.2.0 for //:app"
    );

    build(&mut pipeline, OnFailure::Stop);

    assert_eq!(pipeline.ports().approver.attributions, [planned]);
}

#[test]
fn a_denied_build_runs_nothing_records_nothing_and_logs_nothing() {
    let mut pipeline = chain();
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Deny("not today".to_owned()));

    let error = refused(&mut pipeline);

    let Error::NotApproved { unapproved, reason } = error else {
        unreachable!("expected a refusal by the approver, got {error:?}");
    };
    assert_eq!(reason.to_string(), "not today");
    assert_eq!(*unapproved, pipeline.ports().approver.asked[0]);
    assert_nothing_ran(&pipeline);
}

#[test]
fn a_denied_build_of_cached_targets_is_still_refused() {
    let mut pipeline = chain();
    build(&mut pipeline, OnFailure::Stop);
    let runs = pipeline.ports().strategy.ran().len();
    let events = pipeline.ports().log.events.len();
    let rows = pipeline.ports().cache.recorded().len();
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Deny("no".to_owned()));

    let error = refused(&mut pipeline);

    assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
    assert_eq!(pipeline.ports().strategy.ran().len(), runs);
    assert_eq!(pipeline.ports().log.events.len(), events);
    assert_eq!(pipeline.ports().cache.recorded().len(), rows);
}

#[test]
fn an_approver_error_stops_the_build() {
    let mut pipeline = chain();
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Fail("no terminal".to_owned()));

    let error = refused(&mut pipeline);

    assert!(
        matches!(
            error,
            Error::Adapter {
                port: PortName::Approver,
                ..
            }
        ),
        "{error:?}"
    );
    assert_nothing_ran(&pipeline);
}

/// `//:app`, and `//:pkgs`, a discovery target.
fn discovering() -> Pipeline<FakePorts> {
    root_only(vec![
        target("app", &[]),
        target_with("pkgs", |target| target.role = TargetRole::Discovery),
    ])
}

#[test]
fn a_workspace_build_skips_discovery_targets() {
    let mut pipeline = discovering();

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:app", OutcomeClass::Built)])
    );
    assert_eq!(pipeline.ports().strategy.ran(), [label("//:app")]);
}

#[test]
fn a_named_discovery_target_builds() {
    let mut pipeline = discovering();

    let outcomes = build_named(&mut pipeline, &["//:pkgs"], OnFailure::Stop).unwrap();

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:pkgs", OutcomeClass::Built)])
    );
    let done = outcome(&outcomes, "//:pkgs");
    assert_eq!(done.role, TargetRole::Discovery);
    let names: Vec<&str> = done
        .outputs
        .iter()
        .map(|produced| produced.name.as_str())
        .collect();
    assert_eq!(names, ["pkgs.json"]);
    assert!(pipeline.ports().cache.rows.contains_key(&label("//:pkgs")));
}

#[test]
fn a_discovery_that_leaves_a_directory_fails_and_is_not_cached() {
    let mut pipeline = discovering();
    script(
        &mut pipeline,
        "//:pkgs",
        leaving(
            out("//:pkgs", "pkgs.json"),
            tree(&[("a.json", "{}"), ("b.json", "{}")]),
        ),
    );

    let outcomes = build_named(&mut pipeline, &["//:pkgs"], OnFailure::Stop).unwrap();

    let failed = outcome(&outcomes, "//:pkgs");
    assert_eq!(
        failed.class,
        OutcomeClass::OutputRefused {
            name: OutputName::parse("pkgs.json").unwrap(),
            refusal: OutputRefusal::NotAFile,
        }
    );
    assert_eq!(
        diagnostic(failed).as_deref(),
        Some("pkgs.json is a directory, and a discovery output must be a file")
    );
    assert_eq!(failed.outputs, []);
    assert!(failed.stdout.is_some(), "what it printed was stored");
    assert_eq!(&event(&pipeline, "//:pkgs").outcome, failed);
    let ports = pipeline.ports();
    assert_eq!(ports.cache.rows.len(), 0, "a refusal is never recorded");
    assert_eq!(ports.cas.placements(), [], "and nothing is put in place");
}

#[test]
fn a_build_target_may_still_leave_a_directory() {
    let mut pipeline = root_only(vec![target("site", &[])]);
    script(
        &mut pipeline,
        "//:site",
        leaving(
            out("//:site", "site"),
            tree(&[("index.html", "page"), ("app.js", "script")]),
        ),
    );

    let outcomes = build(&mut pipeline, OnFailure::Stop);

    assert_eq!(
        classes(&outcomes),
        expected(&[("//:site", OutcomeClass::Built)])
    );
    assert!(pipeline.ports().cache.rows.contains_key(&label("//:site")));
}
