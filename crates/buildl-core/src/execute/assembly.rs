//! The assembly: from a plan, through the ports, to one outcome per target.
//!
//! Its own file because it is where this phase's steps meet and where their order is fixed.
//! Everything that can refuse a build is checked before any of it runs: the plan belongs to the
//! graph, no two outputs collide, and every command, once expanded, names a program the
//! workspace allows. Only then are targets handed out to be built, each one settled when it may
//! start and recorded when it is done.
//!
//! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
//! build has stopped, and the place of the one action shown as it prints; the bookkeeping
//! between two results — which targets wait to be settled, which have their outcome, and the
//! one error that fails the whole build; having the build reported as it goes, an outcome only
//! once its event is logged; and refusing to report a build whose dispatcher returned with a
//! target left without an outcome.
//!
//! Non-responsibilities: what each step decides. Each has its own sibling file.

use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::{BTreeMap, VecDeque};

use crate::error::{Error, Result};
use crate::execute::admission::admit;
use crate::execute::perform::{Ended, Performed, Shared, Task, Told, Worked, perform};
use crate::execute::record::{
    Progress, append, failed, forget_contradicted, restore, shown, skipped, store,
};
use crate::execute::schedule::{Finish, Schedule};
use crate::execute::settle::{Settled, settle};
use crate::placement::expand::{Lists, expand, expand_description};
use crate::placement::layout::Layout;
use crate::ports::{Dispatcher, Manifest, PortSet, Ports, Reporter};
use crate::types::{
    ActionOutcome, Captured, Ceiling, Command, EnvName, EnvValue, ExpandedDescription, Forcing,
    Label, LiveOutput, Node, NodeId, OnFailure, OutcomeEvent, OutputPath, OwnComponents, Plan,
    PlannedKey, ReadyAction, Rerun, Summary, TargetGraph, Turn, Verdict,
};

/// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
/// order.
///
/// `plan` must be the plan made from `graph`; the value of every build setting is the one
/// `plan` holds. A target starts once everything it depends on has succeeded. Its key is then
/// finished from what those dependencies produced: when its last build recorded that key and
/// everything it produced is still stored, the target is not run and its outputs are put back
/// in place; otherwise its action runs. What an action produced is stored first, its row
/// recorded after, and its outcome logged whatever it is, so a row never names content that is
/// not held and a failure is never recorded as a result.
///
/// An action may leave a regular file or a directory of regular files at each output path. A
/// directory is stored file by file with a manifest of its paths, is put back whole or not at
/// all, and is one digest in the key of every target that depends on it. An output that is
/// anything else, or a directory holding anything else, is refused for one of the reasons of
/// [`OutputRefusal`](crate::types::OutputRefusal), and the target fails.
///
/// An action that fails is an outcome, not an error. What a failure does to the rest of the
/// build is `on_failure`'s to say: under [`OnFailure::Stop`] nothing more is begun, and under
/// [`OnFailure::KeepGoing`] only the targets that depend on the failed one are skipped.
///
/// A target's description is expanded with its command, from the same paths and with the
/// settings `plan` holds, and handed to the strategy with it; expanding one cannot fail, and it
/// is no part of the key.
///
/// `whole` is the graph of the whole workspace, the one `graph` was narrowed from, or `graph`
/// itself. A target of `whole` that `graph` leaves out is no part of the build: it has no
/// outcome, no event and no row, and whatever it left in the workspace stays.
///
/// Under [`Rerun::Tests`] a test runs whatever its recorded result says, and its new result is
/// recorded; any other target settles as before.
///
/// The build is shown through the reporter port, always from the calling thread: that it begins,
/// once nothing can refuse it any more; each action as a worker begins it; each target's outcome,
/// once its event is logged; and the count of the outcomes last, unless the build ends in an
/// error. An outcome is reported with what its action printed, read back from the content store,
/// for a target that ran as for one that is cached.
///
/// Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints: what it
/// writes is reported while it runs, and its outcome is then reported with nothing left to show.
/// The outcome of a target that ends meanwhile is logged at once and reported after that
/// action's. What is stored and what is returned are the same under either choice.
///
/// # Errors
///
/// Before anything runs, returns the first failure met, in this order:
///
/// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
/// 2. whatever the manifest port returns;
/// 3. [`Error::OutputConflict`] when two outputs of `whole` would be written to one place;
/// 4. for the first target, in id order, whose expanded command cannot run:
///    [`Error::NoProgram`] when it names no program, and [`Error::Refused`] when the program
///    is outside the workspace's ceiling.
///
/// Once the build is under way, returns the first of: the log port's failure to append an
/// event; [`Error::CanonicalJson`] for a key that cannot be serialized; [`Error::Adapter`],
/// naming the action cache, for a dependency that succeeded and has no row; the content
/// store's failure to read the manifest of a recorded output; and its failure to read what a
/// target printed, which is [`Error::Adapter`] naming the content store when that content is
/// no longer held. After such an error no port is asked anything more, and no count is
/// reported. Otherwise returns whatever the dispatcher port itself returns.
///
/// When the dispatcher returns success and a target has no outcome, returns
/// [`Error::DispatcherFault`] naming those targets: the port's contract is that every task is
/// run and every result reaches the build, so a build never reports success, or a summary,
/// with a target missing. What ended before is recorded and logged already, and stays.
pub fn execute<P: Ports>(
    whole: &TargetGraph,
    graph: &TargetGraph,
    plan: &Plan,
    on_failure: OnFailure,
    live: LiveOutput,
    rerun: Rerun,
    ports: &mut PortSet<P>,
) -> Result<Vec<ActionOutcome>> {
    if !made_from(plan, graph) {
        return Err(Error::ForeignPlan);
    }
    let workspace = ports.manifest.workspace()?;
    Layout::of(whole, &workspace.out)?;
    let layout = Layout::of(graph, &workspace.out)?;
    let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;

    let PortSet {
        cache,
        cas,
        log,
        strategy,
        dispatcher,
        clock,
        reporter,
        ..
    } = ports;
    let (cas, strategy, clock) = (&*cas, &*strategy, &*clock);
    let stop = AtomicBool::new(false);
    let place = AtomicBool::new(false);
    let shared = Shared {
        stop: &stop,
        live: match live {
            LiveOutput::Off => None,
            LiveOutput::OneAction => Some(&place),
        },
    };
    let mut build = Build::<P> {
        graph,
        targets,
        layout: &layout,
        schedule: Schedule::new(graph, on_failure),
        rerun,
        outcomes: vec![None; graph.len()],
        queue: VecDeque::new(),
        fatal: None,
        cache,
        cas,
        log,
        clock,
        reporter,
        shared,
        live: None,
        held: Vec::new(),
    };
    build.reporter.begun(graph.len());
    let dispatched = dispatcher.dispatch(
        |task, tell| perform(task, strategy, cas, clock, shared, tell),
        |turn| build.step(turn),
    );
    build.outcomes(dispatched)
}

/// Whether `plan` holds exactly the targets of `graph`, under the same ids, and a value for
/// exactly the settings `graph` declares.
pub(crate) fn made_from(plan: &Plan, graph: &TargetGraph) -> bool {
    plan.len() == graph.len()
        && graph
            .nodes()
            .iter()
            .zip((0_u32..).map(NodeId::new))
            .all(|(node, id)| plan.label(id) == Some(&node.label))
        && plan.settings().keys().eq(graph.settings().keys())
}

/// The components a verdict holds for its target, whatever the verdict says.
const fn own_of(verdict: &Verdict) -> &OwnComponents {
    match verdict {
        Verdict::Clean { key }
        | Verdict::Dirty {
            key: PlannedKey::Finished(key),
            ..
        } => &key.components().own,
        Verdict::Dirty {
            key: PlannedKey::Unfinished(unfinished),
            ..
        }
        | Verdict::Conditional(unfinished) => &unfinished.own,
    }
}

/// The action of `node` as a strategy is handed it: `command` is its expanded command,
/// `description` its description expanded the same way, `outputs` where its own outputs live
/// and `deps` where its dependencies' do.
///
/// The environment holds each variable the target reads that has a value; one that is not set
/// is left out, as it is absent where the action runs.
fn ready(
    node: &Node,
    own: &OwnComponents,
    command: Command,
    description: Option<ExpandedDescription>,
    outputs: &[OutputPath],
    deps: Vec<OutputPath>,
) -> ReadyAction {
    let env: BTreeMap<EnvName, EnvValue> = own
        .env
        .iter()
        .filter_map(|(name, value)| Some((name.clone(), value.clone()?)))
        .collect();
    ReadyAction {
        label: node.label.clone(),
        command,
        description,
        env,
        inputs: node.inputs.clone(),
        deps,
        outputs: outputs.to_vec(),
        network: node.network,
    }
}

/// One target, with everything about it that is settled before anything runs.
pub(crate) struct Prepared<'a> {
    /// The target as the graph holds it.
    pub(crate) node: &'a Node,
    /// Its own key components, from its verdict.
    own: &'a OwnComponents,
    /// The labels of the targets it depends on, in the order they were written.
    pub(crate) deps: Vec<&'a Label>,
    /// Its action, ready to hand to a strategy.
    pub(crate) action: ReadyAction,
}

/// Every target of `graph` prepared, in id order: its command expanded and admitted, and its
/// description expanded.
///
/// A command is expanded with the settings its key holds. A description may name a setting its
/// command does not, which its key does not hold, so it is expanded with the settings `plan`
/// holds: the value of every setting.
///
/// # Errors
///
/// Returns [`Error::NoProgram`] or [`Error::Refused`] for the first target, in id order, whose
/// expanded command cannot run under `ceiling`.
pub(crate) fn prepared<'a>(
    graph: &'a TargetGraph,
    plan: &'a Plan,
    layout: &Layout,
    ceiling: &Ceiling,
) -> Result<Vec<Prepared<'a>>> {
    graph
        .nodes()
        .iter()
        .zip(plan.verdicts())
        .zip((0_u32..).map(NodeId::new))
        .map(|((node, verdict), id)| {
            let own = own_of(verdict);
            let outputs = layout.outputs(id);
            let dep_paths = layout.deps(graph, id);
            let lists = Lists {
                inputs: &node.inputs,
                outputs,
                deps: &dep_paths,
            };
            let command = admit(expand(&node.run, &own.settings, &lists), node, ceiling)?;
            let description = node
                .description
                .as_ref()
                .map(|description| expand_description(description, plan.settings(), &lists));
            let deps = graph
                .deps(id)
                .unwrap_or_default()
                .iter()
                .filter_map(|dep| graph.node(*dep))
                .map(|dep| &dep.label)
                .collect();
            Ok(Prepared {
                node,
                own,
                deps,
                action: ready(node, own, command, description, outputs, dep_paths),
            })
        })
        .collect()
}

/// The labels of the targets of `graph` whose slot in `slots` is empty, in label order.
///
/// A build that ran to its end gave every target an outcome: one that succeeded, one that
/// failed, or one that was skipped. A target left without is one whose task, or whose result,
/// the dispatcher lost, or one that waited on such a target. Targets are numbered in label
/// order, so walking the slots in order yields the labels sorted.
fn without_outcome<T>(graph: &TargetGraph, slots: &[Option<T>]) -> Vec<Label> {
    graph
        .nodes()
        .iter()
        .zip(slots)
        .filter(|(_, slot)| slot.is_none())
        .map(|(node, _)| node.label.clone())
        .collect()
}

/// One build under way: everything the calling thread keeps between two results.
struct Build<'a, P: Ports> {
    /// The graph being built.
    graph: &'a TargetGraph,
    /// Every target, in id order.
    targets: Vec<Prepared<'a>>,
    /// Where every output lives.
    layout: &'a Layout,
    /// Which targets may start, and which never will.
    schedule: Schedule,
    /// Whether the run's tests run again whatever their rows say.
    rerun: Rerun,
    /// One slot per target, in id order, filled when the target has its outcome.
    outcomes: Vec<Option<ActionOutcome>>,
    /// The started targets waiting to be settled, oldest first.
    queue: VecDeque<NodeId>,
    /// The error that fails the build, once there is one.
    fatal: Option<Error>,
    /// The rows of the last build and of this one.
    cache: &'a mut P::Cache,
    /// The content store, shared with the workers.
    cas: &'a P::Cas,
    /// The build log.
    log: &'a mut P::Log,
    /// The clock, shared with the workers.
    clock: &'a P::Clock,
    /// What shows the build. Only this thread calls it.
    reporter: &'a mut P::Reporter,
    /// What the workers read: whether anything more may begin, and the place of the action
    /// shown live.
    shared: Shared<'a>,
    /// The target whose action is being shown live: set when the first of what it printed is
    /// reported, cleared when its outcome is.
    live: Option<NodeId>,
    /// The events of targets that ended while another was being shown live, oldest first:
    /// logged already, and reported once that one has ended.
    held: Vec<OutcomeEvent>,
}

impl<P: Ports> Build<'_, P> {
    /// Takes one turn: the beginning of the build, then each thing a worker says and the result
    /// of each task. Returns the tasks that became ready.
    ///
    /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
    /// message or a result is dropped, and no task is returned.
    fn step(&mut self, turn: Turn<Told, Worked>) -> Vec<Task> {
        if self.fatal.is_some() {
            return Vec::new();
        }
        let mut tasks = Vec::new();
        let stepped = match turn {
            Turn::Begin => {
                let started = self.schedule.start();
                self.queue.extend(started);
                self.drain(&mut tasks)
            }
            Turn::Told(told) => {
                self.told(told);
                Ok(())
            }
            Turn::Done(Worked::NotBegun(node)) => self.abandon(node),
            Turn::Done(Worked::Performed(performed)) => self
                .record(*performed)
                .and_then(|()| self.drain(&mut tasks)),
        };
        match stepped {
            Ok(()) => tasks,
            Err(error) => {
                self.fatal = Some(error);
                self.shared.stop.store(true, Ordering::SeqCst);
                Vec::new()
            }
        }
    }

    /// Reports what a worker said about an action it is running: that it began, or what the
    /// action shown live just wrote.
    fn told(&mut self, told: Told) {
        match told {
            Told::Began(node) => {
                if let Some(target) = self.targets.get(node.index()) {
                    self.reporter.started(&target.action);
                }
            }
            Told::Printed {
                node,
                channel,
                bytes,
            } => {
                if let Some(target) = self.targets.get(node.index()) {
                    self.live = Some(node);
                    self.reporter.printed(&target.node.label, channel, &bytes);
                }
            }
        }
    }

    /// Settles every started target waiting in the queue, oldest first: one whose recorded
    /// result still stands is finished at once, which may start others that join the queue;
    /// one that must run becomes a task. Once the build has stopped, what waits is abandoned.
    fn drain(&mut self, tasks: &mut Vec<Task>) -> Result<()> {
        while let Some(node) = self.queue.pop_front() {
            if self.schedule.stopped().is_some() {
                self.abandon(node)?;
                continue;
            }
            let Some(target) = self.targets.get(node.index()) else {
                continue;
            };
            let settled = settle(
                &target.node.label,
                Forcing::of(target.node.freshness, target.node.role, self.rerun),
                target.own,
                &target.deps,
                &*self.cache,
                self.cas,
            )?;
            match settled {
                Settled::Runs(key) => {
                    tasks.push(Task {
                        node,
                        key,
                        action: target.action.clone(),
                    });
                }
                Settled::Cached { row, manifests } => {
                    let outcome = restore(
                        target.node.label.clone(),
                        target.node.role,
                        &row,
                        &manifests,
                        self.layout.outputs(node),
                        self.cas,
                    );
                    let progress = Progress::Keyed {
                        key: row.key().digest(),
                    };
                    self.finish(node, progress, outcome)?;
                }
            }
        }
        Ok(())
    }

    /// Records what a worker did for one target, and finishes the target. A failure under the
    /// key the target's row was recorded under removes that row first.
    fn record(&mut self, performed: Performed) -> Result<()> {
        let node = performed.node;
        let Some(target) = self.targets.get(node.index()) else {
            return Ok(());
        };
        let role = target.node.role;
        let was_live = performed.live;
        let progress = Progress::Began {
            key: performed.key.digest(),
            span: performed.span,
        };
        let key = performed.key.clone();
        let outcome = match performed.ended {
            Ended::Produced { outputs, printed } => store(
                target.node,
                performed.key,
                outputs,
                printed,
                self.layout.outputs(node),
                self.cas,
                self.cache,
            ),
            Ended::Failed {
                class,
                diagnostic,
                printed,
            } => failed(target.node.label.clone(), role, class, diagnostic, printed),
        };
        // Before the outcome is finished, so the row is gone before the log line is written.
        let outcome = forget_contradicted(outcome, &key, self.cache);
        self.finish(node, progress, outcome)?;
        if was_live && let Some(place) = self.shared.live {
            // Given back only now, with the outcome reported: the next action to take the
            // place cannot have anything it prints shown before this one has ended.
            place.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Gives `node` its outcome, and tells the schedule how it finished: every target that
    /// can no longer be built is given its outcome too, and every target that may start now
    /// joins the queue. When the build has stopped, the workers are told to begin nothing.
    fn finish(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
        let finish = if outcome.class.succeeded() {
            Finish::Succeeded
        } else {
            Finish::Failed
        };
        self.conclude(node, progress, outcome)?;
        let effects = self.schedule.complete(self.graph, node, finish);
        for skipped_node in effects.skipped {
            self.skip(skipped_node, node)?;
        }
        if self.schedule.stopped().is_some() {
            self.shared.stop.store(true, Ordering::SeqCst);
        }
        self.queue.extend(effects.started);
        Ok(())
    }

    /// Gives up on the started target `node` because the build has stopped: it is skipped,
    /// naming the failure that stopped the build. A build that has not stopped abandons
    /// nothing.
    fn abandon(&mut self, node: NodeId) -> Result<()> {
        let Some(failed) = self.schedule.stopped() else {
            return Ok(());
        };
        self.schedule.abandon(node);
        self.skip(node, failed)
    }

    /// Gives `node` the outcome of a target never begun because the target `because` failed.
    fn skip(&mut self, node: NodeId, because: NodeId) -> Result<()> {
        let (Some(target), Some(cause)) = (
            self.targets.get(node.index()),
            self.targets.get(because.index()),
        ) else {
            return Ok(());
        };
        let outcome = skipped(
            target.node.label.clone(),
            target.node.role,
            cause.node.label.clone(),
        );
        self.conclude(node, Progress::Skipped, outcome)
    }

    /// Keeps `outcome` as the outcome of `node`, appends its event to the log, and only then
    /// has it reported.
    fn conclude(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
        let (Some(target), Some(slot)) = (
            self.targets.get(node.index()),
            self.outcomes.get_mut(node.index()),
        ) else {
            return Ok(());
        };
        *slot = Some(outcome.clone());
        let event = append(
            self.log,
            self.clock,
            &target.node.provenance,
            &target.action.command,
            progress,
            outcome,
        )?;
        self.show(node, event)
    }

    /// Reports the event of `node`, at once unless another target is being shown live.
    ///
    /// While one is, the event is held, so that nothing is shown in the middle of what that
    /// action prints. When `node` is the one shown live, its event is reported with nothing
    /// left to show, since all of it was shown as it was written, and then every held event is
    /// reported, oldest first.
    fn show(&mut self, node: NodeId, event: OutcomeEvent) -> Result<()> {
        match self.live {
            Some(live) if live != node => {
                self.held.push(event);
                Ok(())
            }
            Some(_) => {
                self.reporter.finished(&event, Captured::default());
                self.live = None;
                let held = core::mem::take(&mut self.held);
                held.iter().try_for_each(|event| self.report(event))
            }
            None => self.report(&event),
        }
    }

    /// Reports `event` with what its action printed, read back from the content store: for a
    /// target that ran as for one that is cached.
    fn report(&mut self, event: &OutcomeEvent) -> Result<()> {
        let label = &event.outcome.label;
        let stdout = shown(self.cas, label, event.outcome.stdout)?;
        let stderr = shown(self.cas, label, event.outcome.stderr)?;
        self.reporter.finished(
            event,
            Captured {
                stdout: &stdout,
                stderr: &stderr,
            },
        );
        Ok(())
    }

    /// What the build comes to, once the dispatcher has returned with `dispatched`: the error
    /// that failed it, or the dispatcher's own, or else every target's outcome in id order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DispatcherFault`] when the dispatcher returned success and a target has
    /// no outcome. Whatever ended before that was recorded and logged as it ended, and stays.
    fn outcomes(self, dispatched: Result<()>) -> Result<Vec<ActionOutcome>> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        dispatched?;
        let targets = without_outcome(self.graph, &self.outcomes);
        if !targets.is_empty() {
            return Err(Error::DispatcherFault { targets });
        }
        let outcomes: Vec<ActionOutcome> = self.outcomes.into_iter().flatten().collect();
        self.reporter.summary(Summary::of(&outcomes));
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::{made_from, own_of, ready, without_outcome};
    use crate::execute::fixtures::{
        command, graph, key, label, node, output_path, own, path, settings,
    };
    use crate::types::{
        DirtyReason, EnvName, EnvValue, ExpandedDescription, NetworkAccess, NodeId, Plan,
        PlannedKey, TargetGraph, Unfinished, Verdict, WantedSet,
    };

    /// A plan holding one clean verdict per label, in the order given, and the settings
    /// `values`.
    fn plan_with(labels: &[&str], values: &[(&str, &str)]) -> Plan {
        let verdicts = labels
            .iter()
            .map(|_| Verdict::Clean { key: key(&["o"]) })
            .collect();
        let wanted = WantedSet {
            programs: BTreeSet::new(),
            env: BTreeSet::new(),
            write: BTreeSet::new(),
            network: BTreeSet::new(),
        };
        Plan::new(
            labels.iter().map(|raw| label(raw)).collect(),
            verdicts,
            wanted,
            settings(values),
        )
    }

    /// A plan holding one clean verdict per label, in the order given, and no setting.
    fn plan(labels: &[&str]) -> Plan {
        plan_with(labels, &[])
    }

    #[test]
    fn a_plan_is_made_from_a_graph_when_it_holds_its_targets_under_the_same_ids() {
        let graph = graph(
            vec![node("//:a", |_| {}), node("//:b", |_| {})],
            &[&[], &[]],
        );
        assert!(made_from(&plan(&["//:a", "//:b"]), &graph));
        assert!(!made_from(&plan(&["//:a"]), &graph), "a target short");
        assert!(
            !made_from(&plan(&["//:a", "//:b", "//:c"]), &graph),
            "a target more"
        );
        assert!(
            !made_from(&plan(&["//:a", "//:c"]), &graph),
            "another target"
        );
        assert!(
            !made_from(&plan(&["//:b", "//:a"]), &graph),
            "the same targets under other ids"
        );
    }

    #[test]
    fn the_targets_without_an_outcome_are_named_in_label_order() {
        let graph = graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
            ],
            &[&[], &[], &[]],
        );
        assert_eq!(
            without_outcome(&graph, &[Some(()), None, None]),
            [label("//:b"), label("//:c")]
        );
        assert_eq!(
            without_outcome(&graph, &[None, Some(()), Some(())]),
            [label("//:a")]
        );
        assert_eq!(without_outcome(&graph, &[Some(()); 3]), []);
        assert_eq!(without_outcome::<()>(&graph, &[]), [], "no slot, no target");
    }

    #[test]
    fn a_plan_is_made_from_a_graph_when_it_holds_a_value_for_exactly_its_settings() {
        let graph = TargetGraph::new(
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
            settings(&[("level", "1"), ("mode", "debug")]),
        );
        assert!(
            made_from(
                &plan_with(&[], &[("level", "1"), ("mode", "debug")]),
                &graph
            ),
            "the defaults"
        );
        assert!(
            made_from(&plan_with(&[], &[("level", "1"), ("mode", "fast")]), &graph),
            "an overridden value"
        );
        assert!(
            !made_from(&plan_with(&[], &[("mode", "debug")]), &graph),
            "a setting short"
        );
        assert!(
            !made_from(
                &plan_with(&[], &[("arch", "arm"), ("level", "1"), ("mode", "debug")]),
                &graph
            ),
            "a setting more"
        );
        assert!(
            !made_from(&plan_with(&[], &[("level", "1"), ("tool", "cc")]), &graph),
            "another setting"
        );
    }

    #[test]
    fn every_verdict_yields_the_components_of_its_target() {
        let finished = key(&["app"]);
        let unfinished = Unfinished {
            own: own(&["lib"]),
            awaiting: vec![NodeId::new(0)],
        };
        let clean = Verdict::Clean {
            key: finished.clone(),
        };
        let dirty = Verdict::Dirty {
            reason: DirtyReason::NeverBuilt,
            key: PlannedKey::Finished(finished),
        };
        let waiting = Verdict::Dirty {
            reason: DirtyReason::NeverBuilt,
            key: PlannedKey::Unfinished(unfinished.clone()),
        };
        let conditional = Verdict::Conditional(unfinished);

        assert_eq!(own_of(&clean), &own(&["app"]));
        assert_eq!(own_of(&dirty), &own(&["app"]));
        assert_eq!(own_of(&waiting), &own(&["lib"]));
        assert_eq!(own_of(&conditional), &own(&["lib"]));
    }

    #[test]
    fn an_action_is_handed_its_paths_in_written_order_and_only_the_variables_that_are_set() {
        let node = node("//:app", |node| {
            node.inputs = vec![path("z.c"), path("a.c")];
            node.network = NetworkAccess::Declared;
        });
        let cc = EnvName::parse("CC").unwrap();
        let unset = EnvName::parse("CFLAGS").unwrap();
        let mut own = own(&["app"]);
        own.env = BTreeMap::from([
            (cc.clone(), Some(EnvValue::parse("clang").unwrap())),
            (unset, None),
        ]);
        let outputs = [output_path("//:app", "app")];
        let deps = vec![output_path("//:z", "z"), output_path("//:a", "a")];

        let action = ready(
            &node,
            &own,
            command(&["cc", "-o", "out/app"]),
            Some(ExpandedDescription::new("compile z.c a.c")),
            &outputs,
            deps.clone(),
        );

        assert_eq!(action.label, label("//:app"));
        assert_eq!(action.command, command(&["cc", "-o", "out/app"]));
        assert_eq!(
            action.description,
            Some(ExpandedDescription::new("compile z.c a.c"))
        );
        assert_eq!(
            action.env,
            BTreeMap::from([(cc, EnvValue::parse("clang").unwrap())])
        );
        assert_eq!(action.inputs, [path("z.c"), path("a.c")]);
        assert_eq!(action.deps, deps);
        assert_eq!(action.outputs, outputs);
        assert_eq!(action.network, NetworkAccess::Declared);
    }
}
