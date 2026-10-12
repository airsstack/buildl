//! `Pipeline::build`, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;
use std::path::Path;

use buildl_core::{
    ActionOutcome, Digest, EntryName, Error, EvaluationFailure, FileStat, Freshness, InfraKind,
    Label, OnFailure, OutcomeClass, OutcomeEvent, OutputName, OutputPath, Pipeline, PortName,
    SettingName, SettingValue, SourcePath, StagedItem, Summary, TargetRole, Timestamp, Want,
    Written, json,
};

use crate::common::{
    Content, Ends, FakeContent, FakeFile, FakePorts, FakeSource, Order, Script, alias, dir, file,
    ports, program, rule, setting, target, target_with, workspace,
};

/// The events of [`two_directories`] built once, one canonical JSON object per line, pinned.
const GOLDEN: &str = include_str!("golden/log.jsonl");

/// The values given to settings for one run.
type Overrides = BTreeMap<SettingName, SettingValue>;

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
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
        .build(&entry(), &Overrides::new(), on_failure)
        .unwrap()
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
            failed: 0,
            skipped: 0,
            built: 4,
            cached: 0
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
            assert!(ports.cas.blob(&produced.digest).is_some());
            let at = OutputPath::of(&dir("out"), &outcome.label, &produced.name);
            assert_eq!(placed.get(&at), Some(&produced.digest), "{at}");
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
            failed: 0,
            skipped: 0,
            built: 0,
            cached: 4
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

/// Every target's class, by label.
fn classes(outcomes: &[ActionOutcome]) -> BTreeMap<String, OutcomeClass> {
    outcomes
        .iter()
        .map(|outcome| (outcome.label.to_string(), outcome.class.clone()))
        .collect()
}

/// The classes a test expects, by label.
fn expected(pairs: &[(&str, OutcomeClass)]) -> BTreeMap<String, OutcomeClass> {
    pairs
        .iter()
        .map(|(raw, class)| ((*raw).to_owned(), class.clone()))
        .collect()
}

/// A pipeline over one root build file making `items`, with `cc` findable.
fn root_only(items: Vec<StagedItem>) -> Pipeline<FakePorts> {
    let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
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

/// The class of a target skipped because the target `raw` names failed.
fn skipped(raw: &str) -> OutcomeClass {
    OutcomeClass::Skipped {
        because: label(raw),
    }
}

/// The class of a target the builder itself failed, at `kind`.
const fn infra(kind: InfraKind) -> OutcomeClass {
    OutcomeClass::InfraFailure { kind }
}

/// A script in which the action goes as `ends` says and everything else is unscripted.
fn ending(ends: Ends) -> Script {
    Script {
        ends,
        ..Script::default()
    }
}

/// Scripts the action of the target `raw` names.
fn script(pipeline: &mut Pipeline<FakePorts>, raw: &str, script: Script) {
    pipeline
        .ports_mut()
        .strategy
        .scripts
        .insert(label(raw), script);
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
            failed: 1,
            skipped: 2,
            built: 2,
            cached: 0
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
            failed: 8,
            skipped: 0,
            built: 0,
            cached: 0
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
        .build(&entry(), &Overrides::new(), OnFailure::KeepGoing)
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
        .build(&entry(), &Overrides::new(), OnFailure::Stop)
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
fn a_test_that_runs_what_its_dependency_built_is_built_when_the_workspace_allows_that_program() {
    let mut pipeline = tested();
    let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
    ceiling.run.insert(program("out/app"));

    let outcomes = build(&mut pipeline, OnFailure::Stop);

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

#[test]
fn a_program_first_named_by_expansion_is_refused_before_anything_runs() {
    let mut pipeline = tested();

    let error = refused(&mut pipeline);

    assert_eq!(
        error.to_string(),
        "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
    );
    match &error {
        Error::Refused { want, site, .. } => {
            assert_eq!(want, &Want::Program(program("out/app")));
            let site = site.as_deref().unwrap();
            assert_eq!(site.label, label("//:app_test"));
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

/// The workspace of the graph flows, with one change: its test target runs what it depends on,
/// `$deps`, where the graph flows' runs `$out/app`. The root holds a rule, a target using it, a
/// target with dependencies in two directories, an alias, the test target and a setting; `lib`
/// holds one target. `cc` can be found, and the workspace allows `out/app` to run.
fn two_directories(order: Order) -> Pipeline<FakePorts> {
    let root = vec![
        rule(
            "cc",
            &["cc", "-c", "$in", "-o", "$out"],
            Some("compile $in"),
        ),
        target_with("main.o", |target| {
            target.run = None;
            target.rule = Some(Written::new("cc"));
            target.inputs = vec![Written::new("main.c")];
        }),
        target_with("app", |target| {
            target.run = Some(["cc", "$deps", "-o", "$out"].map(Written::new).to_vec());
            target.deps = vec![Written::new("main.o"), Written::new("//lib:text")];
        }),
        alias("default", "app"),
        target_with("app_test", |target| {
            target.role = TargetRole::Test;
            target.run = Some(
                ["$deps", "--filter=$opt:test_filter"]
                    .map(Written::new)
                    .to_vec(),
            );
            target.deps = vec![Written::new("default")];
        }),
        setting("test_filter"),
    ];
    let lib = vec![target_with("text", |target| {
        target.inputs = vec![Written::new("text.c")];
    })];
    let source = FakeSource::new(workspace(vec![
        ("", file(root, &["lib"])),
        ("lib", file(lib, &[])),
    ]));
    let mut pipeline = Pipeline::new(ports(source));
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

/// A build of [`two_directories`] with `test_filter` set to `Login`.
fn build_filtered(pipeline: &mut Pipeline<FakePorts>) -> Vec<ActionOutcome> {
    let overrides = Overrides::from([(
        SettingName::parse("test_filter").unwrap(),
        SettingValue::parse("Login").unwrap(),
    )]);
    pipeline
        .build(&entry(), &overrides, OnFailure::Stop)
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
            failed: 0,
            skipped: 0,
            built: 4,
            cached: 0
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
            texts(&["out/app_test"]),
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
