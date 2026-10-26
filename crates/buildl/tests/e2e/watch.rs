//! A watch over real workspaces: edits made between cycles, and what each cycle then does.
//!
//! A scripted [`Changes`] stands in for the file system's events. Each of its steps edits the
//! fixture the way a person would and then names the paths it touched; after the last step it
//! fails the way a watcher that can no longer watch does, which ends the watch. A test cannot
//! hang, since the script is all the changes there will be.
//!
//! The chain fixture is copied with `greet.sh` executable, so every one of its targets builds.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::rc::Rc;

use buildl::RunOptions;
use buildl::approver::NoTerminal;
use buildl::buildl_core::{
    ActionOutcome, Captured, ChangeBatch, ChangedPath, Changes, Channel, Diagnostic, Error, Label,
    LiveOutput, OnFailure, OutcomeClass, OutcomeEvent, PortName, ReadyAction, RemoteNotice,
    Reporter, Result, Scope, Summary, Trigger, WatchReport,
};

use crate::common::Fixture;

/// What one step of a script does to the workspace, and the paths it names as changed.
type Step<'a> = Box<dyn FnOnce(&Fixture) -> Vec<&'static str> + 'a>;

/// Something a watch said or did, in the order it happened.
#[derive(Debug, Clone, PartialEq)]
enum Event {
    /// Step `n` of the script, counted from zero, is about to make its edits.
    Step(usize),
    /// A cycle began; the paths that started it, none for the first.
    Building(Vec<String>),
    /// A target ended as `class`.
    Finished(String, OutcomeClass),
    /// A build ended and counted its targets.
    Summary(Summary),
    /// A cycle ended with an error.
    Failed(String),
    /// The watch went to wait for the next change.
    Waiting,
}

type Events = Rc<RefCell<Vec<Event>>>;

/// A [`Changes`] that makes the edits of its steps, one batch per call.
struct Script<'a> {
    fixture: &'a Fixture,
    events: Events,
    steps: VecDeque<Step<'a>>,
    taken: usize,
}

impl Changes for Script<'_> {
    fn next(&mut self) -> Result<ChangeBatch> {
        let Some(step) = self.steps.pop_front() else {
            return Err(Error::Adapter {
                port: PortName::Changes,
                diagnostic: Diagnostic::new("the script has no more changes"),
            });
        };
        self.events.borrow_mut().push(Event::Step(self.taken));
        self.taken += 1;
        let paths = step(self.fixture)
            .into_iter()
            .map(|path| ChangedPath::parse(path).unwrap())
            .collect::<BTreeSet<_>>();
        Ok(ChangeBatch::new(paths).unwrap())
    }
}

/// A reporter that keeps what a watch tells it, and shows nothing.
struct Recorded {
    events: Events,
}

impl Reporter for Recorded {
    fn begun(&mut self, _targets: usize) {}

    fn started(&mut self, _action: &ReadyAction) {}

    fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

    fn finished(&mut self, event: &OutcomeEvent, _printed: Captured<'_>) {
        self.events.borrow_mut().push(Event::Finished(
            event.outcome.label.to_string(),
            event.outcome.class.clone(),
        ));
    }

    fn remote(&mut self, _notice: &RemoteNotice) {}

    fn summary(&mut self, summary: Summary) {
        self.events.borrow_mut().push(Event::Summary(summary));
    }
}

impl WatchReport for Recorded {
    fn building(&mut self, trigger: &Trigger) {
        let paths = match trigger {
            Trigger::Start => Vec::new(),
            Trigger::Changed(batch) => batch.iter().map(|path| path.as_str().to_owned()).collect(),
        };
        self.events.borrow_mut().push(Event::Building(paths));
    }

    fn failed(&mut self, error: &Error) {
        self.events
            .borrow_mut()
            .push(Event::Failed(error.to_string()));
    }

    fn waiting(&mut self) {
        self.events.borrow_mut().push(Event::Waiting);
    }
}

/// One cycle of a watch, as it was told.
struct Round {
    /// The step whose edits came last before the cycle began; none for the first cycle.
    after: Option<usize>,
    /// The paths that started it.
    paths: Vec<String>,
    /// How each target ended, in label order.
    ended: Vec<(String, OutcomeClass)>,
    /// How the build counted its targets.
    summary: Option<Summary>,
    /// The errors it ended with.
    failed: Vec<String>,
}

impl Round {
    /// The labels of the targets that ended as `class`.
    fn labels(&self, class: &OutcomeClass) -> Vec<&str> {
        self.ended
            .iter()
            .filter(|(_, ended)| ended == class)
            .map(|(label, _)| label.as_str())
            .collect()
    }
}

/// Groups `events` into the cycles they tell of.
fn rounds(events: &[Event]) -> Vec<Round> {
    let mut rounds: Vec<Round> = Vec::new();
    let mut step = None;
    for event in events {
        match event {
            Event::Step(n) => step = Some(*n),
            Event::Building(paths) => rounds.push(Round {
                after: step,
                paths: paths.clone(),
                ended: Vec::new(),
                summary: None,
                failed: Vec::new(),
            }),
            Event::Finished(label, class) => {
                rounds
                    .last_mut()
                    .unwrap()
                    .ended
                    .push((label.clone(), class.clone()));
            }
            Event::Summary(summary) => rounds.last_mut().unwrap().summary = Some(*summary),
            Event::Failed(error) => rounds.last_mut().unwrap().failed.push(error.clone()),
            Event::Waiting => {}
        }
    }
    for round in &mut rounds {
        round.ended.sort_by(|left, right| left.0.cmp(&right.0));
    }
    rounds
}

/// Watches `scope` in `fixture` until `steps` are spent, and tells what happened.
fn watch_scoped<'a>(fixture: &'a Fixture, scope: &Scope, steps: Vec<Step<'a>>) -> Vec<Round> {
    let events: Events = Rc::default();
    let mut script = Script {
        fixture,
        events: Rc::clone(&events),
        steps: steps.into(),
        taken: 0,
    };
    let mut reporter = Recorded {
        events: Rc::clone(&events),
    };
    let ended = fixture.open().watch(
        &mut script,
        scope,
        &BTreeMap::new(),
        RunOptions::new(OnFailure::Stop, LiveOutput::Off),
        &mut reporter,
        &mut NoTerminal,
    );
    assert!(
        matches!(
            ended,
            Error::Adapter {
                port: PortName::Changes,
                ..
            }
        ),
        "the watch ended with {ended:?}"
    );
    let events = events.borrow();
    rounds(&events)
}

/// As [`watch_scoped`], for every target.
fn watch<'a>(fixture: &'a Fixture, steps: Vec<Step<'a>>) -> Vec<Round> {
    watch_scoped(fixture, &Scope::Workspace, steps)
}

/// A copy of `chain` with the executable bit it needs.
fn chain() -> Fixture {
    Fixture::copy_of("chain", &["greet.sh"])
}

/// How each target of `outcomes` ended, in label order.
fn classes(outcomes: &[ActionOutcome]) -> Vec<(String, OutcomeClass)> {
    let mut classes: Vec<(String, OutcomeClass)> = outcomes
        .iter()
        .map(|outcome| (outcome.label.to_string(), outcome.class.clone()))
        .collect();
    classes.sort_by(|left, right| left.0.cmp(&right.0));
    classes
}

/// The number of cycles that began after the edits of step `n`.
fn rounds_after(rounds: &[Round], n: usize) -> usize {
    rounds.iter().filter(|round| round.after == Some(n)).count()
}

#[test]
fn start_builds_as_a_plain_build_does() {
    let watched = chain();
    let plain = chain();

    let rounds = watch(&watched, Vec::new());

    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].paths, Vec::<String>::new());
    assert_eq!(rounds[0].ended, classes(&plain.build()));
    assert_eq!(rounds[0].failed, Vec::<String>::new());
}

#[test]
fn a_source_edit_rebuilds_only_its_target() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            fixture.append("lib/words/a.txt", b"zebra\n");
            vec!["lib/words/a.txt"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    let second = &rounds[1];
    let summary = second.summary.unwrap().all;
    assert!(summary.built >= 1, "{summary:?}");
    assert!(summary.cached >= 1, "{summary:?}");
    assert_eq!(summary.failed, 0);
    let built = second.labels(&OutcomeClass::Built);
    assert!(built.contains(&"//lib:words"), "{built:?}");
    for label in built {
        assert!(
            ["//lib:words", "//:greeting", "//:report"].contains(&label),
            "{label} does not read lib/words/a.txt or depend on what does"
        );
    }
    for label in ["//:greet", "//lib:assets", "//:site"] {
        assert!(second.labels(&OutcomeClass::Cached).contains(&label));
    }
}

#[test]
fn a_build_file_edit_adding_a_target_builds_it() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            fixture.append(
                "lib/build.lua",
                b"b.target(\"extra\", { inputs = { \"words/a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
            );
            vec!["lib/build.lua"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[1].labels(&OutcomeClass::Built), ["//lib:extra"]);
    assert_eq!(rounds[1].failed, Vec::<String>::new());
}

#[test]
fn a_manifest_edit_is_read_again() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![
            Box::new(|fixture| {
                fixture.replace_line("buildl.toml", "out    = \"out\"", "out    = \"build\"");
                fixture.replace_line(
                    "buildl.toml",
                    "fs.write = [\"out\", \".buildl\"]",
                    "fs.write = [\"build\", \".buildl\"]",
                );
                vec!["buildl.toml"]
            }),
            Box::new(|fixture| {
                fs::write(fixture.root().join("build/touched.txt"), "touched\n").unwrap();
                vec!["build/touched.txt"]
            }),
        ],
    );

    assert_eq!(rounds.len(), 2);
    assert!(rounds[1].failed.is_empty(), "{:?}", rounds[1].failed);
    assert_eq!(rounds[1].after, Some(0));
    let outputs = fixture.files("build");
    assert!(outputs.contains(&"build/greet".to_owned()), "{outputs:?}");
    assert_eq!(rounds_after(&rounds, 1), 0);
}

#[test]
fn a_new_globbed_file_is_built() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            fs::write(fixture.root().join("lib/words/new.txt"), "newest\n").unwrap();
            vec!["lib/words/new.txt"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    assert!(
        rounds[1]
            .labels(&OutcomeClass::Built)
            .contains(&"//lib:words"),
        "{:?}",
        rounds[1].ended
    );
}

#[test]
fn a_readme_edit_adds_no_log_line() {
    let fixture = chain();
    let before = Cell::new(0);

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            before.set(fixture.log_lines().len());
            fs::write(fixture.root().join("README.md"), "about this\n").unwrap();
            vec!["README.md"]
        })],
    );

    assert_eq!(rounds.len(), 1);
    assert!(before.get() > 0);
    assert_eq!(fixture.log_lines().len(), before.get());
}

#[test]
fn a_build_file_comment_edit_runs_no_cycle() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            fixture.append("lib/build.lua", b"-- note\n");
            vec!["lib/build.lua"]
        })],
    );

    assert_eq!(rounds.len(), 1);
}

#[test]
fn a_save_of_the_same_bytes_runs_no_action() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            let path = fixture.root().join("lib/words/a.txt");
            fs::write(&path, fs::read(&path).unwrap()).unwrap();
            vec!["lib/words/a.txt"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    assert!(
        rounds[1].labels(&OutcomeClass::Built).is_empty(),
        "{:?}",
        rounds[1].ended
    );
    assert_eq!(rounds[1].ended.len(), rounds[0].ended.len());
}

#[test]
fn a_batch_of_changes_makes_one_cycle() {
    let fixture = chain();

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            fixture.append("lib/words/a.txt", b"zebra\n");
            fixture.append("lib/words/b.txt", b"yak\n");
            fixture.append("footer.txt", b"end\n");
            vec!["footer.txt", "lib/words/a.txt", "lib/words/b.txt"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[1].paths.len(), 3);
}

#[test]
fn a_discovery_input_outside_the_scope_runs_a_cycle() {
    let fixture = Fixture::copy_of("discovery", &[]);
    let scope = Scope::Targets(BTreeSet::from([fixture.open().label("//:alpha").unwrap()]));

    let rounds = watch_scoped(
        &fixture,
        &scope,
        vec![Box::new(|fixture| {
            fixture.replace_line(
                "packages.json",
                "[\"alpha\", \"beta\"]",
                "[\"alpha\", \"beta\", \"gamma\"]",
            );
            vec!["packages.json"]
        })],
    );

    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[1].after, Some(0));
}

#[test]
fn a_readme_edit_on_the_discovery_fixture_adds_no_log_line() {
    let fixture = Fixture::copy_of("discovery", &[]);
    let before = Cell::new(0);

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            before.set(fixture.log_lines().len());
            fs::write(fixture.root().join("README.md"), "about this\n").unwrap();
            vec!["README.md"]
        })],
    );

    assert_eq!(rounds.len(), 1);
    assert!(before.get() > 0);
    assert_eq!(fixture.log_lines().len(), before.get());
}

#[test]
fn plan_and_log_work_between_cycles() {
    let fixture = chain();
    let seen = Cell::new(false);

    let rounds = watch(
        &fixture,
        vec![Box::new(|fixture| {
            let workspace = fixture.open();
            workspace.plan(&Scope::Workspace, &BTreeMap::new()).unwrap();
            workspace
                .log(&workspace.label("//lib:words").unwrap())
                .unwrap();
            seen.set(true);
            vec!["README.md"]
        })],
    );

    assert!(seen.get());
    assert_eq!(rounds.len(), 1);
}

#[test]
fn a_watch_records_what_plain_builds_record() {
    let watched = chain();
    let plain = chain();
    let edits: [(&'static str, &'static [u8]); 2] =
        [("lib/words/a.txt", b"zebra\n"), ("footer.txt", b"end\n")];

    let rounds = watch(
        &watched,
        edits
            .iter()
            .map(|&(path, bytes)| -> Step<'_> {
                Box::new(move |fixture| {
                    fixture.append(path, bytes);
                    vec![path]
                })
            })
            .collect(),
    );
    plain.build();
    for (path, bytes) in edits {
        plain.append(path, bytes);
        plain.build();
    }

    assert_eq!(rounds.len(), 3);
    assert_eq!(watched.rows(), plain.rows());
    assert_eq!(watched.snapshot().first_difference(&plain.snapshot()), None);
}
