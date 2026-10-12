//! The schedule: which targets may start, and which never will.
//!
//! Its own file because it is the one place that knows how far a build has come: a target
//! starts when everything it depends on has succeeded, and a failure decides what happens to
//! the targets that have not started, by the policy the build was given.
//!
//! Responsibilities: [`Schedule`], [`Finish`] and [`Effects`].
//!
//! Non-responsibilities: running anything, and deciding whether a target succeeded. The
//! schedule is told how each target finished and answers with what that changes; it asks no
//! port. Its caller is its only writer, so the counts it keeps are plain integers.

use crate::types::{NodeId, OnFailure, TargetGraph};

/// How far one target has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Waiting on a dependency.
    Pending,
    /// Handed to its caller to be built.
    Started,
    /// Built, or found already built.
    Succeeded,
    /// It could not be built.
    Failed,
    /// It will not be built: something it needed failed, or the build stopped first.
    Skipped,
}

/// How a started target finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Finish {
    /// Its outputs are in place.
    Succeeded,
    /// It could not be built.
    Failed,
}

/// What one target finishing changed for the others.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Effects {
    /// The targets that may start now, in id order.
    pub(crate) started: Vec<NodeId>,
    /// The targets that will never start, in id order.
    pub(crate) skipped: Vec<NodeId>,
}

/// The state of every target of one build.
///
/// A target is pending until everything it depends on has succeeded, then started, and from
/// there it succeeds or fails. A pending target is skipped when something it depends on fails,
/// or, under [`OnFailure::Stop`], when anything fails.
#[derive(Debug)]
pub(crate) struct Schedule {
    /// One state per node, in id order.
    states: Vec<State>,
    /// Per node, in id order: how many of its dependencies have not succeeded yet.
    waiting: Vec<usize>,
    /// What a failure does to the rest of the build.
    on_failure: OnFailure,
    /// The failure that stopped the build, once one has.
    stopped: Option<NodeId>,
}

impl Schedule {
    /// A schedule in which every target of `graph` is pending.
    pub(crate) fn new(graph: &TargetGraph, on_failure: OnFailure) -> Self {
        let waiting: Vec<usize> = (0_u32..)
            .take(graph.len())
            .map(|id| graph.deps(NodeId::new(id)).map_or(0, <[NodeId]>::len))
            .collect();
        Self {
            states: vec![State::Pending; waiting.len()],
            waiting,
            on_failure,
            stopped: None,
        }
    }

    /// Starts every pending target that waits on nothing, and returns them in id order.
    pub(crate) fn start(&mut self) -> Vec<NodeId> {
        let mut started = Vec::new();
        let ids = (0_u32..).map(NodeId::new);
        for ((state, count), id) in self.states.iter_mut().zip(&self.waiting).zip(ids) {
            if *state == State::Pending && *count == 0 {
                *state = State::Started;
                started.push(id);
            }
        }
        started
    }

    /// Records that the started target `node` finished, and returns what that changes.
    ///
    /// After a success, every target that waited only on `node` starts, unless the build has
    /// stopped. After a failure, under [`OnFailure::Stop`] the build stops and every pending
    /// target is skipped; under [`OnFailure::KeepGoing`] every pending target that depends on
    /// `node`, directly or through others, is skipped.
    ///
    /// A `node` that is not started changes nothing.
    pub(crate) fn complete(
        &mut self,
        graph: &TargetGraph,
        node: NodeId,
        finish: Finish,
    ) -> Effects {
        match self.states.get_mut(node.index()) {
            Some(state) if *state == State::Started => {
                *state = match finish {
                    Finish::Succeeded => State::Succeeded,
                    Finish::Failed => State::Failed,
                };
            }
            _ => return Effects::default(),
        }
        match (finish, self.on_failure) {
            (Finish::Succeeded, _) if self.stopped.is_some() => Effects::default(),
            (Finish::Succeeded, _) => Effects {
                started: self.release(graph, node),
                skipped: Vec::new(),
            },
            (Finish::Failed, OnFailure::Stop) => Effects {
                started: Vec::new(),
                skipped: self.stop(node),
            },
            (Finish::Failed, OnFailure::KeepGoing) => Effects {
                started: Vec::new(),
                skipped: self.skip_from(graph, node),
            },
        }
    }

    /// Gives up on the started target `node`: it is skipped, and nothing that waits on it will
    /// start.
    ///
    /// For a target that was started and that no one began building before the build stopped.
    /// A `node` that is not started changes nothing.
    pub(crate) fn abandon(&mut self, node: NodeId) {
        if let Some(state) = self.states.get_mut(node.index())
            && *state == State::Started
        {
            *state = State::Skipped;
        }
    }

    /// The failure that stopped the build, if it has stopped.
    ///
    /// A build stops only under [`OnFailure::Stop`], at its first failure.
    pub(crate) const fn stopped(&self) -> Option<NodeId> {
        self.stopped
    }

    /// Counts `node` as succeeded for each target that depends on it, and starts those left
    /// waiting on nothing. Returns them in id order.
    fn release(&mut self, graph: &TargetGraph, node: NodeId) -> Vec<NodeId> {
        let mut started = Vec::new();
        for dependent in graph.dependents(node).unwrap_or_default() {
            let index = dependent.index();
            let (Some(count), Some(state)) =
                (self.waiting.get_mut(index), self.states.get_mut(index))
            else {
                continue;
            };
            *count = count.saturating_sub(1);
            if *count == 0 && *state == State::Pending {
                *state = State::Started;
                started.push(*dependent);
            }
        }
        started.sort_unstable();
        started
    }

    /// Stops the build at `failed`, unless it has stopped already, and skips every pending
    /// target. Returns them in id order.
    fn stop(&mut self, failed: NodeId) -> Vec<NodeId> {
        if self.stopped.is_none() {
            self.stopped = Some(failed);
        }
        let mut skipped = Vec::new();
        let ids = (0_u32..).map(NodeId::new);
        for (state, id) in self.states.iter_mut().zip(ids) {
            if *state == State::Pending {
                *state = State::Skipped;
                skipped.push(id);
            }
        }
        skipped
    }

    /// Skips every pending target that depends on `failed`, directly or through others, and
    /// returns them in id order.
    ///
    /// Walks with an explicit stack, never by recursion, so a long dependency chain cannot
    /// exhaust the call stack. Only a target skipped here is walked through: one skipped
    /// earlier had everything behind it skipped then.
    fn skip_from(&mut self, graph: &TargetGraph, failed: NodeId) -> Vec<NodeId> {
        let mut skipped = Vec::new();
        let mut stack = vec![failed];
        while let Some(node) = stack.pop() {
            for dependent in graph.dependents(node).unwrap_or_default() {
                if let Some(state) = self.states.get_mut(dependent.index())
                    && *state == State::Pending
                {
                    *state = State::Skipped;
                    skipped.push(*dependent);
                    stack.push(*dependent);
                }
            }
        }
        skipped.sort_unstable();
        skipped
    }
}

#[cfg(test)]
mod tests {
    use super::{Effects, Finish, Schedule};
    use crate::execute::fixtures::{graph, node};
    use crate::types::{NodeId, OnFailure, TargetGraph};

    fn ids(raw: &[u32]) -> Vec<NodeId> {
        raw.iter().copied().map(NodeId::new).collect()
    }

    fn effects(started: &[u32], skipped: &[u32]) -> Effects {
        Effects {
            started: ids(started),
            skipped: ids(skipped),
        }
    }

    /// `//:a` depends on `//:b` and `//:c`, and both of those on `//:d`.
    fn diamond() -> TargetGraph {
        graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
                node("//:d", |_| {}),
            ],
            &[&[1, 2], &[3], &[3], &[]],
        )
    }

    /// `//:c` depends on `//:b`, which depends on `//:a`; apart from them, `//:y` depends on
    /// `//:x`.
    fn two_chains() -> TargetGraph {
        graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
                node("//:x", |_| {}),
                node("//:y", |_| {}),
            ],
            &[&[], &[0], &[1], &[], &[3]],
        )
    }

    #[test]
    fn an_empty_graph_starts_nothing() {
        let graph = graph(Vec::new(), &[]);
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), []);
        assert_eq!(schedule.stopped(), None);
    }

    #[test]
    fn a_target_starts_once_everything_it_depends_on_has_succeeded() {
        let graph = diamond();
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), ids(&[3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
            effects(&[1, 2], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(2), Finish::Succeeded),
            effects(&[], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(1), Finish::Succeeded),
            effects(&[0], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
            effects(&[], &[])
        );
        assert_eq!(schedule.stopped(), None);
    }

    #[test]
    fn nothing_starts_twice() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        assert_eq!(schedule.start(), []);
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
            effects(&[1], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
            effects(&[], &[]),
            "a target that already finished"
        );
        assert_eq!(schedule.start(), []);
    }

    #[test]
    fn a_target_that_was_not_started_cannot_finish() {
        let graph = diamond();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Failed),
            effects(&[], &[]),
            "before anything started"
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(9), Finish::Succeeded),
            effects(&[], &[]),
            "an id the graph did not issue"
        );
        assert_eq!(schedule.start(), ids(&[3]));
    }

    #[test]
    fn a_failure_stops_the_build_when_told_to_stop() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        assert_eq!(schedule.stopped(), None);
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Failed),
            effects(&[], &[1, 2, 4]),
            "every target still waiting is skipped, downstream of the failure or not"
        );
        assert_eq!(schedule.stopped(), Some(NodeId::new(0)));
    }

    #[test]
    fn a_success_after_the_build_stopped_starts_nothing() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Failed),
            effects(&[], &[1, 2, 4])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
            effects(&[], &[])
        );
        assert_eq!(schedule.start(), []);
    }

    #[test]
    fn a_stopped_build_remembers_its_first_failure() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Failed),
            effects(&[], &[1, 2, 4])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Failed),
            effects(&[], &[])
        );
        assert_eq!(schedule.stopped(), Some(NodeId::new(3)));
    }

    #[test]
    fn a_failure_skips_only_what_depends_on_it_when_told_to_keep_going() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Failed),
            effects(&[], &[1, 2]),
            "the target that depends on it, and the one that depends on that"
        );
        assert_eq!(schedule.stopped(), None);
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
            effects(&[4], &[]),
            "the rest of the build goes on"
        );
    }

    #[test]
    fn a_second_failure_skips_only_what_was_still_waiting() {
        let graph = diamond();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
            effects(&[1, 2], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(1), Finish::Failed),
            effects(&[], &[0])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(2), Finish::Failed),
            effects(&[], &[]),
            "the one target downstream was skipped by the first failure"
        );
        assert_eq!(schedule.stopped(), None);
    }

    #[test]
    fn a_skipped_target_does_not_start_when_its_other_dependency_succeeds() {
        let graph = diamond();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[3]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(3), Finish::Succeeded),
            effects(&[1, 2], &[])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(1), Finish::Failed),
            effects(&[], &[0])
        );
        assert_eq!(
            schedule.complete(&graph, NodeId::new(2), Finish::Succeeded),
            effects(&[], &[])
        );
    }

    #[test]
    fn an_abandoned_target_is_skipped_and_releases_nothing() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        schedule.abandon(NodeId::new(0));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
            effects(&[], &[]),
            "an abandoned target is no longer started, so it cannot finish"
        );
        assert_eq!(schedule.start(), []);
    }

    #[test]
    fn only_a_started_target_can_be_abandoned() {
        let graph = two_chains();
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), ids(&[0, 3]));
        schedule.abandon(NodeId::new(1));
        schedule.abandon(NodeId::new(9));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Succeeded),
            effects(&[1], &[]),
            "the pending target was left pending"
        );
    }

    #[test]
    fn started_targets_are_abandoned_once_the_build_has_stopped() {
        let graph = graph(
            vec![
                node("//:a", |_| {}),
                node("//:b", |_| {}),
                node("//:c", |_| {}),
            ],
            &[&[], &[], &[]],
        );
        let mut schedule = Schedule::new(&graph, OnFailure::Stop);
        assert_eq!(schedule.start(), ids(&[0, 1, 2]));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(0), Finish::Failed),
            effects(&[], &[]),
            "nothing was pending, so the schedule itself skips nothing"
        );
        schedule.abandon(NodeId::new(1));
        schedule.abandon(NodeId::new(2));
        assert_eq!(
            schedule.complete(&graph, NodeId::new(1), Finish::Failed),
            effects(&[], &[])
        );
        assert_eq!(schedule.stopped(), Some(NodeId::new(0)));
    }

    #[test]
    fn a_long_chain_is_skipped_without_recursion() {
        let length = 100_000_u32;
        let nodes = (0..length)
            .map(|index| node(&format!("//:n{index:06}"), |_| {}))
            .collect();
        // Each node depends on the next, so only the last one can start.
        let lists: Vec<Vec<u32>> = (0..length)
            .map(|index| {
                if index + 1 < length {
                    vec![index + 1]
                } else {
                    Vec::new()
                }
            })
            .collect();
        let edges: Vec<&[u32]> = lists.iter().map(Vec::as_slice).collect();
        let graph = graph(nodes, &edges);
        let last = NodeId::new(length - 1);
        let mut schedule = Schedule::new(&graph, OnFailure::KeepGoing);
        assert_eq!(schedule.start(), [last]);

        let effects = schedule.complete(&graph, last, Finish::Failed);
        assert_eq!(effects.started, []);
        assert_eq!(effects.skipped.len(), 99_999);
        assert_eq!(effects.skipped.first(), Some(&NodeId::new(0)));
        assert_eq!(effects.skipped.last(), Some(&NodeId::new(length - 2)));
        assert!(effects.skipped.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
