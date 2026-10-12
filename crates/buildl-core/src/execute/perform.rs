//! One action on a worker: what is handed over, what comes back, and how the action ended.
//!
//! Its own file because an action is run away from the thread that schedules the build: a
//! task goes out and a result comes back, and both must be whole values that borrow nothing.
//! How the action ended is decided here from what the worker saw, in a fixed order, so the
//! same exit and the same collected outputs always end the same way.
//!
//! Responsibilities: [`Task`], [`Worked`], [`Performed`], [`Span`], [`Printed`] and [`Ended`];
//! the decisions [`exited`], [`collected`] and [`failed_to`]; and [`perform`], which takes one
//! task through the ports in that fixed order.
//!
//! Non-responsibilities: how an action is run and where what it left is stored, which are the
//! ports'; and recording the result, which happens on the thread that schedules the build.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};
use crate::ports::{Clock, ContentStore, ExecStrategy};
use crate::types::{
    ActionKey, Diagnostic, Digest, Exit, InfraKind, NodeId, OutcomeClass, OutputName, RawOutcome,
    ReadyAction, Timestamp,
};

/// One action to run, as it is handed to a worker.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Task {
    /// The target.
    pub(crate) node: NodeId,
    /// The target's finished key.
    pub(crate) key: ActionKey,
    /// What to run, and where its inputs and outputs are.
    pub(crate) action: ReadyAction,
}

/// What a worker hands back for one task.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Worked {
    /// The build had stopped before the worker began the action: nothing was run, and nothing
    /// is known but the target.
    NotBegun(NodeId),
    /// The worker began the action. Boxed, so that a task that was not begun is not as large
    /// as one that ran.
    Performed(Box<Performed>),
}

/// One action a worker began, with how it ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Performed {
    /// The target.
    pub(crate) node: NodeId,
    /// The key the task was handed over with.
    pub(crate) key: ActionKey,
    /// When the worker began and when it was done.
    pub(crate) span: Span,
    /// How the action ended.
    pub(crate) ended: Ended,
}

/// When a worker began an action, and when it was done with it.
///
/// One value and not two optional instants: an action a worker began has both, and a target no
/// worker began has neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    /// Read before the worker did anything for the action.
    pub(crate) started: Timestamp,
    /// Read after the last thing the worker did for the action.
    pub(crate) finished: Timestamp,
}

/// The digests of what an action printed, once both are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Printed {
    /// What it wrote to standard output.
    pub(crate) stdout: Digest,
    /// What it wrote to standard error.
    pub(crate) stderr: Digest,
}

/// How an action a worker began ended, before anything about it is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ended {
    /// It exited with status zero and left every output it declares, and all of it is stored.
    Produced {
        /// The digest of each declared output, in declared order.
        outputs: Vec<Digest>,
        /// What it printed.
        printed: Printed,
    },
    /// It did not get that far.
    Failed {
        /// Why.
        class: OutcomeClass,
        /// A message for display, when the builder itself failed and said how.
        diagnostic: Option<Diagnostic>,
        /// What it printed, when the action ran and that was stored.
        printed: Option<Printed>,
    },
}

/// How an action ended because a port failed with `error` while the builder did `kind` for it.
///
/// The error's text becomes the message. `printed` is what the action printed, when it had
/// already run and that was stored.
pub(crate) fn failed_to(kind: InfraKind, error: &Error, printed: Option<Printed>) -> Ended {
    Ended::Failed {
        class: OutcomeClass::InfraFailure { kind },
        diagnostic: Some(Diagnostic::new(error.to_string())),
        printed,
    }
}

/// How an action that ended with `exit` ended, when the exit alone decides it.
///
/// `None` for an exit with status zero: what the action left then decides. An action that ran
/// out of time is the builder's failure; any other status is the action's own.
pub(crate) const fn exited(exit: Exit, printed: Printed) -> Option<Ended> {
    let class = match exit {
        Exit::Exited(0) => return None,
        Exit::Exited(status) => OutcomeClass::ActionFailure { status },
        Exit::TimedOut => OutcomeClass::InfraFailure {
            kind: InfraKind::TimedOut,
        },
    };
    Some(Ended::Failed {
        class,
        diagnostic: None,
        printed: Some(printed),
    })
}

/// How an action that exited with status zero ended, from what was found at each of its
/// outputs.
///
/// `names` are the outputs the target declares and `entries` what was found for them, in the
/// same order: a digest, or `None` where the action left nothing. An answer that does not hold
/// one entry per output is the builder's failure. Otherwise the first output with nothing
/// there, in declared order, is the one named as missing.
pub(crate) fn collected(
    names: &[OutputName],
    entries: &[Option<Digest>],
    printed: Printed,
) -> Ended {
    if entries.len() != names.len() {
        return Ended::Failed {
            class: OutcomeClass::InfraFailure {
                kind: InfraKind::Store,
            },
            diagnostic: Some(Diagnostic::new(format!(
                "the strategy answered for {} outputs of {}",
                entries.len(),
                names.len()
            ))),
            printed: Some(printed),
        };
    }
    names
        .iter()
        .zip(entries)
        .find_map(|(name, entry)| entry.is_none().then_some(name))
        .map_or_else(
            || Ended::Produced {
                outputs: entries.iter().flatten().copied().collect(),
                printed,
            },
            |missing| Ended::Failed {
                class: OutcomeClass::OutputMissing {
                    name: missing.clone(),
                },
                diagnostic: None,
                printed: Some(printed),
            },
        )
}

/// Runs one task on the thread it is called from, and returns what came of it.
///
/// A task handed over after `stop` was set is not begun: the clock is not read and no port is
/// asked anything. Otherwise the clock is read, the action is taken through `strategy` and
/// `cas`, and the clock is read again, however the action ended: an action a worker began
/// always carries both instants.
pub(crate) fn perform<S, C, K>(
    task: Task,
    strategy: &S,
    cas: &C,
    clock: &K,
    stop: &AtomicBool,
) -> Worked
where
    S: ExecStrategy,
    C: ContentStore,
    K: Clock,
{
    if stop.load(Ordering::SeqCst) {
        return Worked::NotBegun(task.node);
    }
    let started = clock.now();
    let ended = run(&task, strategy, cas);
    let finished = clock.now();
    Worked::Performed(Box::new(Performed {
        node: task.node,
        key: task.key,
        span: Span { started, finished },
        ended,
    }))
}

/// Takes the action of `task` through its steps, and says how it ended at the first step that
/// decides it: a directory to run in, the run, storing what it printed, its exit, and storing
/// what it left.
fn run<S: ExecStrategy, C: ContentStore>(task: &Task, strategy: &S, cas: &C) -> Ended {
    let action = &task.action;
    let dir = match strategy.prepare(action) {
        Ok(dir) => dir,
        Err(error) => return failed_to(InfraKind::Prepare, &error, None),
    };
    let raw = match strategy.run(&dir, action) {
        Ok(raw) => raw,
        Err(error) => return failed_to(InfraKind::Run, &error, None),
    };
    let printed = match stored(&raw, cas) {
        Ok(printed) => printed,
        Err(error) => return failed_to(InfraKind::Store, &error, None),
    };
    if let Some(ended) = exited(raw.exit, printed) {
        return ended;
    }
    match strategy.collect(&dir, action, cas) {
        Ok(entries) => collected(&task.key.components().own.outputs, &entries, printed),
        Err(error) => failed_to(InfraKind::Store, &error, Some(printed)),
    }
}

/// Stores what an action printed, and returns the two digests.
fn stored<C: ContentStore>(raw: &RawOutcome, cas: &C) -> Result<Printed> {
    Ok(Printed {
        stdout: cas.put(&raw.stdout)?,
        stderr: cas.put(&raw.stderr)?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Ended, Performed, Printed, Span, Task, Worked, collected, exited, failed_to};
    use crate::error::{Error, PortName};
    use crate::execute::fixtures::{command, key, label, output_path, outputs};
    use crate::types::{
        Diagnostic, Digest, Exit, InfraKind, NetworkAccess, NodeId, OutcomeClass, ReadyAction,
        Timestamp,
    };

    fn printed() -> Printed {
        Printed {
            stdout: Digest::of(b"compiling\n"),
            stderr: Digest::of(b""),
        }
    }

    fn strategy_error(text: &str) -> Error {
        Error::Adapter {
            port: PortName::ExecStrategy,
            diagnostic: Diagnostic::new(text),
        }
    }

    /// Stands for handing a value to another thread: it must be `Send` and borrow nothing.
    fn handed_over<T: Send + 'static>(value: T) -> T {
        value
    }

    #[test]
    fn an_exit_with_status_zero_decides_nothing_yet() {
        assert_eq!(exited(Exit::Exited(0), printed()), None);
    }

    #[test]
    fn an_exit_with_another_status_is_the_actions_failure() {
        for status in [1, 2, -1, 255] {
            assert_eq!(
                exited(Exit::Exited(status), printed()),
                Some(Ended::Failed {
                    class: OutcomeClass::ActionFailure { status },
                    diagnostic: None,
                    printed: Some(printed()),
                })
            );
        }
    }

    #[test]
    fn a_timeout_is_the_builders_failure_and_carries_no_message() {
        assert_eq!(
            exited(Exit::TimedOut, printed()),
            Some(Ended::Failed {
                class: OutcomeClass::InfraFailure {
                    kind: InfraKind::TimedOut,
                },
                diagnostic: None,
                printed: Some(printed()),
            })
        );
    }

    #[test]
    fn a_ports_failure_is_the_builders_and_its_text_is_the_message() {
        let error = strategy_error("//:app: no directory to run in");
        for (kind, printed) in [
            (InfraKind::Prepare, None),
            (InfraKind::Run, None),
            (InfraKind::Store, None),
            (InfraKind::Store, Some(printed())),
        ] {
            assert_eq!(
                failed_to(kind, &error, printed),
                Ended::Failed {
                    class: OutcomeClass::InfraFailure { kind },
                    diagnostic: Some(Diagnostic::new(
                        "exec strategy: //:app: no directory to run in"
                    )),
                    printed,
                }
            );
        }
    }

    #[test]
    fn an_action_that_left_every_output_produced_them() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        assert_eq!(
            collected(
                &outputs(&["app", "app.map"]),
                &[Some(app), Some(map)],
                printed()
            ),
            Ended::Produced {
                outputs: vec![app, map],
                printed: printed(),
            }
        );
        assert_eq!(
            collected(&[], &[], printed()),
            Ended::Produced {
                outputs: Vec::new(),
                printed: printed(),
            },
            "a target that declares no output"
        );
    }

    #[test]
    fn an_answer_of_the_wrong_length_is_the_builders_failure() {
        let app = Digest::of(b"app");
        for (entries, text) in [
            (vec![Some(app)], "the strategy answered for 1 outputs of 2"),
            (
                vec![Some(app), None, Some(app)],
                "the strategy answered for 3 outputs of 2",
            ),
            (Vec::new(), "the strategy answered for 0 outputs of 2"),
        ] {
            assert_eq!(
                collected(&outputs(&["app", "app.map"]), &entries, printed()),
                Ended::Failed {
                    class: OutcomeClass::InfraFailure {
                        kind: InfraKind::Store,
                    },
                    diagnostic: Some(Diagnostic::new(text)),
                    printed: Some(printed()),
                }
            );
        }
    }

    #[test]
    fn an_output_the_action_did_not_leave_is_named() {
        let names = outputs(&["app", "app.map", "app.sym"]);
        let app = Digest::of(b"app");
        let missing = |name: &str| Ended::Failed {
            class: OutcomeClass::OutputMissing {
                name: outputs(&[name]).remove(0),
            },
            diagnostic: None,
            printed: Some(printed()),
        };
        assert_eq!(
            collected(&names, &[Some(app), None, Some(app)], printed()),
            missing("app.map"),
            "one missing"
        );
        assert_eq!(
            collected(&names, &[Some(app), None, None], printed()),
            missing("app.map"),
            "two missing: the first, in the order the outputs were written"
        );
        assert_eq!(
            collected(&names, &[None, None, None], printed()),
            missing("app"),
            "none left"
        );
    }

    #[test]
    fn a_task_and_its_result_can_be_handed_to_another_thread() {
        let node = NodeId::new(3);
        let span = Span {
            started: Timestamp::from_unix_nanos(10),
            finished: Timestamp::from_unix_nanos(20),
        };
        let action = ReadyAction {
            label: label("//:app"),
            command: command(&["cc", "-o", "out/app"]),
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: vec![output_path("//:app", "app")],
            network: NetworkAccess::Sealed,
        };

        let task = handed_over(Task {
            node,
            key: key(&["app"]),
            action: action.clone(),
        });
        assert_eq!(task.node, node);
        assert_eq!(task.key, key(&["app"]));
        assert_eq!(task.action, action);

        match handed_over(Worked::NotBegun(task.node)) {
            Worked::NotBegun(not_begun) => assert_eq!(not_begun, node),
            Worked::Performed(performed) => unreachable!("expected NotBegun, got {performed:?}"),
        }

        let ended = Ended::Produced {
            outputs: vec![Digest::of(b"app")],
            printed: printed(),
        };
        let worked = handed_over(Worked::Performed(Box::new(Performed {
            node: task.node,
            key: task.key,
            span,
            ended: ended.clone(),
        })));
        match worked {
            Worked::Performed(performed) => {
                assert_eq!(performed.node, node);
                assert_eq!(performed.key, key(&["app"]));
                assert_eq!(performed.span.started, Timestamp::from_unix_nanos(10));
                assert_eq!(performed.span.finished, Timestamp::from_unix_nanos(20));
                assert_eq!(performed.ended, ended);
            }
            Worked::NotBegun(not_begun) => unreachable!("expected Performed, got {not_begun:?}"),
        }
    }
}
