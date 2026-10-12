//! The assembly: from a plan, through the ports, to one outcome per target.
//!
//! Its own file because it is where this phase's steps meet and where their order is fixed.
//! Everything that can refuse a build is checked before any of it runs: the plan belongs to the
//! graph, no two outputs collide, and every command, once expanded, names a program the
//! workspace allows. Only then are targets handed out to be built, each one settled when it may
//! start and recorded when it is done.
//!
//! Responsibilities: [`execute`]; the flag that keeps workers from beginning anything once the
//! build has stopped; and the bookkeeping between two results — which targets wait to be
//! settled, which have their outcome, and the one error that fails the whole build.
//!
//! Non-responsibilities: what each step decides. Each has its own sibling file.

use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::{BTreeMap, VecDeque};

use crate::error::{Error, Result};
use crate::execute::admission::admit;
use crate::execute::expand::{Lists, expand};
use crate::execute::layout::Layout;
use crate::execute::perform::{Ended, Performed, Task, Worked, perform};
use crate::execute::record::{Progress, append, failed, restore, skipped, store};
use crate::execute::schedule::{Finish, Schedule};
use crate::execute::settle::{Settled, settle};
use crate::ports::{Dispatcher, Manifest, PortSet, Ports};
use crate::types::{
    ActionOutcome, Ceiling, Command, EnvName, EnvValue, Label, Node, NodeId, OnFailure, OutputPath,
    OwnComponents, Plan, PlannedKey, ReadyAction, TargetGraph, Verdict,
};

/// Builds every target of `graph` as `plan` says, and returns one outcome per target, in id
/// order.
///
/// `plan` must be the plan made from `graph`. A target starts once everything it depends on
/// has succeeded. Its key is then finished from what those dependencies produced: when its
/// last build recorded that key and everything it produced is still stored, the target is not
/// run and its outputs are put back in place; otherwise its action runs. What an action
/// produced is stored first, its row recorded after, and its outcome logged whatever it is, so
/// a row never names content that is not held and a failure is never recorded as a result.
///
/// An action that fails is an outcome, not an error. What a failure does to the rest of the
/// build is `on_failure`'s to say: under [`OnFailure::Stop`] nothing more is begun, and under
/// [`OnFailure::KeepGoing`] only the targets that depend on the failed one are skipped.
///
/// # Errors
///
/// Before anything runs, returns the first failure met, in this order:
///
/// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
/// 2. whatever the manifest port returns;
/// 3. [`Error::OutputConflict`] when two outputs would be written to one place;
/// 4. for the first target, in id order, whose expanded command cannot run:
///    [`Error::NoProgram`] when it names no program, and [`Error::Refused`] when the program
///    is outside the workspace's ceiling.
///
/// Once the build is under way, returns the first of: the log port's failure to append an
/// event; [`Error::CanonicalJson`] for a key that cannot be serialized; and
/// [`Error::Adapter`], naming the action cache, for a dependency that succeeded and has no
/// row. After such an error no port is asked anything more. Otherwise returns whatever the
/// dispatcher port itself returns.
pub fn execute<P: Ports>(
    graph: &TargetGraph,
    plan: &Plan,
    on_failure: OnFailure,
    ports: &mut PortSet<P>,
) -> Result<Vec<ActionOutcome>> {
    if !made_from(plan, graph) {
        return Err(Error::ForeignPlan);
    }
    let workspace = ports.manifest.workspace()?;
    let layout = Layout::of(graph, &workspace.out)?;
    let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;

    let PortSet {
        cache,
        cas,
        log,
        strategy,
        dispatcher,
        clock,
        ..
    } = ports;
    let (cas, strategy, clock) = (&*cas, &*strategy, &*clock);
    let stop = AtomicBool::new(false);
    let mut build = Build::<P> {
        graph,
        targets,
        layout: &layout,
        schedule: Schedule::new(graph, on_failure),
        outcomes: vec![None; graph.len()],
        queue: VecDeque::new(),
        fatal: None,
        cache,
        cas,
        log,
        clock,
        stop: &stop,
    };
    let dispatched = dispatcher.dispatch(
        |task| perform(task, strategy, cas, clock, &stop),
        |worked| build.step(worked),
    );
    build.outcomes(dispatched)
}

/// Whether `plan` holds exactly the targets of `graph`, under the same ids.
fn made_from(plan: &Plan, graph: &TargetGraph) -> bool {
    plan.len() == graph.len()
        && graph
            .nodes()
            .iter()
            .zip((0_u32..).map(NodeId::new))
            .all(|(node, id)| plan.label(id) == Some(&node.label))
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
/// `outputs` where its own outputs live and `deps` where its dependencies' do.
///
/// The environment holds each variable the target reads that has a value; one that is not set
/// is left out, as it is absent where the action runs.
fn ready(
    node: &Node,
    own: &OwnComponents,
    command: Command,
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
        env,
        inputs: node.inputs.clone(),
        deps,
        outputs: outputs.to_vec(),
        network: node.network,
    }
}

/// One target, with everything about it that is settled before anything runs.
struct Prepared<'a> {
    /// The target as the graph holds it.
    node: &'a Node,
    /// Its own key components, from its verdict.
    own: &'a OwnComponents,
    /// The labels of the targets it depends on, in the order they were written.
    deps: Vec<&'a Label>,
    /// Its action, ready to hand to a strategy.
    action: ReadyAction,
}

/// Every target of `graph` prepared, in id order: its command expanded and admitted.
///
/// # Errors
///
/// Returns [`Error::NoProgram`] or [`Error::Refused`] for the first target, in id order, whose
/// expanded command cannot run under `ceiling`.
fn prepared<'a>(
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
                action: ready(node, own, command, outputs, dep_paths),
            })
        })
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
    /// Set once nothing more may begin; the workers read it.
    stop: &'a AtomicBool,
}

impl<P: Ports> Build<'_, P> {
    /// Takes one turn: `None` to begin the build, then the result of each task. Returns the
    /// tasks that became ready.
    ///
    /// Once an error has failed the build, a turn does nothing: no port is asked anything, a
    /// result is dropped, and no task is returned.
    fn step(&mut self, worked: Option<Worked>) -> Vec<Task> {
        if self.fatal.is_some() {
            return Vec::new();
        }
        let mut tasks = Vec::new();
        let stepped = match worked {
            None => {
                let started = self.schedule.start();
                self.queue.extend(started);
                self.drain(&mut tasks)
            }
            Some(Worked::NotBegun(node)) => self.abandon(node),
            Some(Worked::Performed(performed)) => self
                .record(*performed)
                .and_then(|()| self.drain(&mut tasks)),
        };
        match stepped {
            Ok(()) => tasks,
            Err(error) => {
                self.fatal = Some(error);
                self.stop.store(true, Ordering::SeqCst);
                Vec::new()
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
                target.node.freshness,
                target.own,
                &target.deps,
                &*self.cache,
                self.cas,
            )?;
            match settled {
                Settled::Runs(key) => tasks.push(Task {
                    node,
                    key,
                    action: target.action.clone(),
                }),
                Settled::Cached(row) => {
                    let outcome = restore(
                        target.node.label.clone(),
                        &row,
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

    /// Records what a worker did for one target, and finishes the target.
    fn record(&mut self, performed: Performed) -> Result<()> {
        let node = performed.node;
        let Some(target) = self.targets.get(node.index()) else {
            return Ok(());
        };
        let label = target.node.label.clone();
        let progress = Progress::Began {
            key: performed.key.digest(),
            span: performed.span,
        };
        let outcome = match performed.ended {
            Ended::Produced { outputs, printed } => store(
                label,
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
            } => failed(label, class, diagnostic, printed),
        };
        self.finish(node, progress, outcome)
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
            self.stop.store(true, Ordering::SeqCst);
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
        let outcome = skipped(target.node.label.clone(), cause.node.label.clone());
        self.conclude(node, Progress::Skipped, outcome)
    }

    /// Keeps `outcome` as the outcome of `node`, and appends its event to the log.
    fn conclude(&mut self, node: NodeId, progress: Progress, outcome: ActionOutcome) -> Result<()> {
        let (Some(target), Some(slot)) = (
            self.targets.get(node.index()),
            self.outcomes.get_mut(node.index()),
        ) else {
            return Ok(());
        };
        *slot = Some(outcome.clone());
        append(
            self.log,
            self.clock,
            &target.node.provenance,
            &target.action.command,
            progress,
            outcome,
        )
    }

    /// What the build comes to, once the dispatcher has returned with `dispatched`: the error
    /// that failed it, or the dispatcher's own, or else every target's outcome in id order.
    fn outcomes(self, dispatched: Result<()>) -> Result<Vec<ActionOutcome>> {
        if let Some(error) = self.fatal {
            return Err(error);
        }
        dispatched?;
        Ok(self.outcomes.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::{made_from, own_of, ready};
    use crate::execute::fixtures::{command, graph, key, label, node, output_path, own, path};
    use crate::types::{
        DirtyReason, EnvName, EnvValue, NetworkAccess, NodeId, Plan, PlannedKey, Unfinished,
        Verdict, WantedSet,
    };

    /// A plan holding one clean verdict per label, in the order given.
    fn plan(labels: &[&str]) -> Plan {
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
        )
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
            &outputs,
            deps.clone(),
        );

        assert_eq!(action.label, label("//:app"));
        assert_eq!(action.command, command(&["cc", "-o", "out/app"]));
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
