//! The loop of a watching run.
//!
//! Responsibilities: [`watch`], which runs a build, then waits for changes and answers each
//! batch with a build, a check of the build files, or nothing.
//!
//! Non-responsibilities: noticing and settling changes, opening the workspace, running a build
//! and showing what the loop says. Each is a port, and this file only sequences them.

use crate::error::Error;
use crate::ports::{Changes, Cycles, WatchReport};
use crate::types::{Cycle, Reaction, Reference, Trigger, WatchLayout};
use crate::watch::react::{kept, react};

/// Watches until changes can no longer be noticed, and returns the error that said so.
///
/// Runs a first build, told it was started by [`Trigger::Start`], then takes one batch at a time
/// from `changes`. A batch is cut to the paths that are changes to the workspace; one with none
/// left does nothing and says nothing. [`react`] sorts the rest against what the last cycle left
/// behind:
/// [`Reaction::Build`] runs a build, [`Reaction::Ignore`] does nothing and says nothing, and
/// [`Reaction::Check`] loads the build files again. A check that fails to load, or that finds a
/// graph unequal to the one the last build ran on, runs a build; an equal graph runs nothing. A
/// build started by a batch is told it was started by [`Trigger::Changed`] with the paths kept,
/// not the whole batch.
///
/// After each cycle an error the build ended with is reported as failed and watching goes on; the
/// loop then says it is waiting. Changes that arrive while a cycle runs wait in `changes`, which
/// hands them back one batch per call.
///
/// `cycles` is also the report, so the one value that runs a cycle is the one that says what it
/// did. `layout` is what the workspace's first open read; a cycle that read another replaces it.
pub fn watch<C: Changes, Y: Cycles + WatchReport>(
    changes: &mut C,
    cycles: &mut Y,
    layout: WatchLayout,
) -> Error {
    let mut reference = cycle(
        cycles,
        Reference {
            layout,
            built: None,
        },
        &Trigger::Start,
    );
    loop {
        let batch = match changes.next() {
            Ok(batch) => batch,
            Err(error) => return error,
        };
        let Some(batch) = kept(&batch, &reference.layout) else {
            continue;
        };
        let build = match react(&batch, &reference) {
            Reaction::Build => true,
            Reaction::Ignore => false,
            Reaction::Check => cycles.graph().map_or(true, |graph| {
                reference
                    .built
                    .as_ref()
                    .is_none_or(|built| built.graph != graph)
            }),
        };
        if build {
            reference = cycle(cycles, reference, &Trigger::Changed(batch));
        }
    }
}

/// Runs one build, reports it, and returns what the next batch is compared against.
fn cycle<Y: Cycles + WatchReport>(
    cycles: &mut Y,
    reference: Reference,
    trigger: &Trigger,
) -> Reference {
    cycles.building(trigger);
    let ended = cycles.build();
    if let Err(error) = &ended.watched.ended {
        cycles.failed(error);
    }
    let reference = renew(reference, ended);
    cycles.waiting();
    reference
}

/// What `cycle` leaves to compare the next batch against.
///
/// The layout is the one the cycle read, or the old one when the workspace could not be opened.
/// The build is the cycle's own: a cycle that ended before it had a graph leaves none.
fn renew(reference: Reference, cycle: Cycle) -> Reference {
    Reference {
        layout: cycle.layout.unwrap_or(reference.layout),
        built: cycle.watched.built,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};
    use std::rc::Rc;

    use super::watch;
    use crate::error::{Error, PortName, Result};
    use crate::ports::{Changes, Cycles, WatchReport};
    use crate::types::{
        Built, ChangeBatch, ChangedPath, Cycle, Diagnostic, Directory, SettingName, SettingValue,
        TargetGraph, Trigger, WatchLayout, Watched,
    };

    #[derive(Debug, PartialEq)]
    enum Call {
        Next,
        Building(Trigger),
        Build,
        Graph,
        Failed(String),
        Waiting,
    }

    type Log = Rc<RefCell<Vec<Call>>>;

    struct Scripted {
        log: Log,
        batches: VecDeque<ChangeBatch>,
    }

    impl Changes for Scripted {
        fn next(&mut self) -> Result<ChangeBatch> {
            self.log.borrow_mut().push(Call::Next);
            self.batches.pop_front().ok_or_else(|| Error::Adapter {
                port: PortName::Changes,
                diagnostic: Diagnostic::new("no more changes"),
            })
        }
    }

    struct Fake {
        log: Log,
        cycles: VecDeque<Cycle>,
        graphs: VecDeque<Result<TargetGraph>>,
    }

    impl Cycles for Fake {
        fn build(&mut self) -> Cycle {
            self.log.borrow_mut().push(Call::Build);
            self.cycles.pop_front().unwrap_or_else(|| cycle(None, None))
        }

        fn graph(&mut self) -> Result<TargetGraph> {
            self.log.borrow_mut().push(Call::Graph);
            self.graphs.pop_front().unwrap()
        }
    }

    impl WatchReport for Fake {
        fn building(&mut self, trigger: &Trigger) {
            self.log.borrow_mut().push(Call::Building(trigger.clone()));
        }

        fn failed(&mut self, error: &Error) {
            self.log.borrow_mut().push(Call::Failed(error.to_string()));
        }

        fn waiting(&mut self) {
            self.log.borrow_mut().push(Call::Waiting);
        }
    }

    fn path(raw: &str) -> ChangedPath {
        ChangedPath::parse(raw).unwrap()
    }

    fn batch(raws: &[&str]) -> ChangeBatch {
        ChangeBatch::new(raws.iter().map(|raw| path(raw)).collect()).unwrap()
    }

    fn layout(out: &str) -> WatchLayout {
        WatchLayout {
            out: Directory::parse(out).unwrap(),
            state: Directory::parse(".buildl").unwrap(),
            config: std::iter::once(path("buildl.toml")).collect(),
        }
    }

    /// A graph with no target, told apart from another by one setting.
    fn graph(mark: &str) -> TargetGraph {
        TargetGraph::new(
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
            BTreeMap::from([(
                SettingName::parse("mark").unwrap(),
                SettingValue::parse(mark).unwrap(),
            )]),
        )
    }

    fn built(mark: &str) -> Built {
        Built {
            graph: graph(mark),
            inputs: std::collections::BTreeSet::new(),
        }
    }

    fn cycle(layout: Option<WatchLayout>, built: Option<Built>) -> Cycle {
        Cycle {
            layout,
            watched: Watched {
                built,
                ended: Ok(Vec::new()),
            },
        }
    }

    /// Runs the loop over `batches` and the scripted cycles and graphs, and returns the error it
    /// ended with and every call in order.
    fn run(
        batches: Vec<ChangeBatch>,
        cycles: Vec<Cycle>,
        graphs: Vec<Result<TargetGraph>>,
    ) -> (Error, Vec<Call>) {
        let log = Log::default();
        let mut changes = Scripted {
            log: Rc::clone(&log),
            batches: batches.into(),
        };
        let mut fake = Fake {
            log: Rc::clone(&log),
            cycles: cycles.into(),
            graphs: graphs.into(),
        };
        let error = watch(&mut changes, &mut fake, layout("out"));
        let calls = log.take();
        (error, calls)
    }

    #[test]
    fn the_first_cycle_runs_before_any_change() {
        let (_, calls) = run(Vec::new(), vec![cycle(None, None)], Vec::new());
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn an_ignored_batch_runs_nothing_and_says_nothing() {
        let first = cycle(Some(layout("out")), Some(built("a")));
        let (_, calls) = run(vec![batch(&["out/x.o"])], vec![first], Vec::new());
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_check_with_an_unchanged_graph_runs_no_build() {
        let first = cycle(Some(layout("out")), Some(built("a")));
        let (_, calls) = run(
            vec![batch(&["README.md"])],
            vec![first],
            vec![Ok(graph("a"))],
        );
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Graph,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_check_with_a_changed_graph_builds() {
        let changes = batch(&["README.md"]);
        let first = cycle(Some(layout("out")), Some(built("a")));
        let second = cycle(Some(layout("out")), Some(built("b")));
        let (_, calls) = run(
            vec![changes.clone()],
            vec![first, second],
            vec![Ok(graph("b"))],
        );
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Graph,
                Call::Building(Trigger::Changed(changes)),
                Call::Build,
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_check_that_fails_builds() {
        let changes = batch(&["README.md"]);
        let first = cycle(Some(layout("out")), Some(built("a")));
        let failed = Err(Error::Adapter {
            port: PortName::Host,
            diagnostic: Diagnostic::new("build file did not load"),
        });
        let (_, calls) = run(
            vec![changes.clone()],
            vec![first, cycle(None, None)],
            vec![failed],
        );
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Graph,
                Call::Building(Trigger::Changed(changes)),
                Call::Build,
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_failed_cycle_is_reported_and_watching_goes_on() {
        let mut first = cycle(None, None);
        first.watched.ended = Err(Error::Adapter {
            port: PortName::Host,
            diagnostic: Diagnostic::new("boom"),
        });
        let (_, calls) = run(Vec::new(), vec![first], Vec::new());
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Failed("host: boom".to_owned()),
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn after_a_cycle_without_a_graph_any_change_builds() {
        let changes = batch(&["README.md"]);
        let (_, calls) = run(
            vec![changes.clone()],
            vec![cycle(Some(layout("out")), None), cycle(None, None)],
            Vec::new(),
        );
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Building(Trigger::Changed(changes)),
                Call::Build,
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_cycle_without_a_graph_forgets_the_last_one() {
        let changes = batch(&["README.md"]);
        let first = cycle(Some(layout("out")), Some(built("a")));
        let second = cycle(Some(layout("out")), None);
        let (_, calls) = run(
            vec![changes.clone(), changes.clone()],
            vec![first, second, cycle(None, None)],
            vec![Ok(graph("b"))],
        );
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Graph,
                Call::Building(Trigger::Changed(changes.clone())),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Building(Trigger::Changed(changes)),
                Call::Build,
                Call::Waiting,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_cycle_that_could_not_open_keeps_the_old_layout() {
        let first = cycle(None, Some(built("a")));
        let (_, calls) = run(vec![batch(&["out/x.o"])], vec![first], Vec::new());
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Next,
            ]
        );
    }

    #[test]
    fn a_cycle_with_a_new_layout_moves_the_drop() {
        let first = cycle(Some(layout("build")), Some(built("a")));
        let (_, calls) = run(vec![batch(&["build/x.o"])], vec![first], Vec::new());
        assert_eq!(
            calls,
            [
                Call::Building(Trigger::Start),
                Call::Build,
                Call::Waiting,
                Call::Next,
                Call::Next,
            ]
        );
    }

    #[test]
    fn the_trigger_names_the_batch() {
        let changes = batch(&["b.txt", "a.txt"]);
        let (_, calls) = run(vec![changes], vec![cycle(None, None)], Vec::new());
        let named = calls.iter().find_map(|call| match call {
            Call::Building(Trigger::Changed(named)) => Some(named),
            _ => None,
        });
        let named = named.unwrap();
        assert_eq!(named.first().as_str(), "a.txt");
        assert_eq!(named.len(), 2);
    }

    #[test]
    fn the_trigger_names_only_the_paths_kept() {
        let changes = batch(&["out/x.o", ".buildl/log.jsonl", "lib/words/a.txt"]);
        let first = cycle(Some(layout("out")), Some(built("a")));
        let (_, calls) = run(
            vec![changes],
            vec![first, cycle(None, None)],
            vec![Ok(graph("b"))],
        );
        assert_eq!(
            calls[5],
            Call::Building(Trigger::Changed(batch(&["lib/words/a.txt"])))
        );
    }

    #[test]
    fn a_changes_error_ends_the_loop_with_that_error() {
        let (error, _) = run(Vec::new(), vec![cycle(None, None)], Vec::new());
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Changes,
                ..
            }
        ));
        assert_eq!(error.to_string(), "file watcher: no more changes");
    }
}
