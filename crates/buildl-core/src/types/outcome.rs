//! How the build of one target ended, and the count of a build's outcomes.
//!
//! Its own file because an outcome is what a build hands back: one per target, whether the
//! target ran, was taken from the cache, failed, or was never begun. A target that fails is an
//! outcome and not an error, so one failure does not hide what the rest of the build did.
//!
//! Responsibilities: [`ActionOutcome`], [`OutcomeClass`] with [`OutcomeClass::succeeded`],
//! [`InfraKind`], and [`Summary`] with [`Summary::of`].
//!
//! Non-responsibilities: deciding which class a target's build ends in, which the phase that
//! executes does; and rendering an outcome for a person, which belongs to whoever shows it.

use serde::Serialize;

use crate::types::{Diagnostic, Digest, Label, OutputName, Produced};

/// Which part of building one target the builder itself could not do.
///
/// Non-exhaustive: a builder that can fail another way gains a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum InfraKind {
    /// The directory to run the action in could not be made.
    Prepare,
    /// The action could not be run.
    Run,
    /// The action ran longer than it was allowed to.
    TimedOut,
    /// An output, something the action printed, or the record of the build could not be stored
    /// or put in place.
    Store,
}

/// How the build of one target ended.
///
/// Serializes as its name when it has no field, and otherwise as an object of one key, the
/// name, holding the fields. Non-exhaustive: a build that can end another way gains a variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum OutcomeClass {
    /// It did not run: what its last build recorded still stands.
    Cached,
    /// It ran, exited with status zero, and every output it declares was stored and recorded.
    Built,
    /// It ran and exited with a status other than zero.
    ActionFailure {
        /// The exit status.
        status: i32,
    },
    /// It ran and exited with status zero, and left nothing where an output it declares should
    /// be.
    OutputMissing {
        /// The first such output, in declared order.
        name: OutputName,
    },
    /// The builder could not build it. Nothing is said about the target's own action.
    InfraFailure {
        /// What the builder could not do.
        kind: InfraKind,
    },
    /// It was never begun, because another target failed.
    Skipped {
        /// The target whose failure caused this one to be skipped.
        because: Label,
    },
}

impl OutcomeClass {
    /// Whether the target's outputs are in place: true for [`OutcomeClass::Cached`] and
    /// [`OutcomeClass::Built`], false for every other class.
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        matches!(self, Self::Cached | Self::Built)
    }
}

/// How the build of one target ended, with what it left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOutcome {
    /// The target.
    pub label: Label,
    /// How its build ended.
    pub class: OutcomeClass,
    /// A message for display: set for an [`OutcomeClass::InfraFailure`] other than a timeout,
    /// and `None` for every other outcome.
    pub diagnostic: Option<Diagnostic>,
    /// What the target produced, in declared order. Empty unless the target succeeded.
    pub outputs: Vec<Produced>,
    /// The digest of what the action wrote to standard output, when that was stored.
    pub stdout: Option<Digest>,
    /// The digest of what the action wrote to standard error, when that was stored.
    pub stderr: Option<Digest>,
}

/// How many targets of one build ended in each way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Targets whose action failed, whose output was missing, or that the builder could not
    /// build.
    pub failed: usize,
    /// Targets never begun because another target failed.
    pub skipped: usize,
    /// Targets that ran and were recorded.
    pub built: usize,
    /// Targets that did not need to run.
    pub cached: usize,
}

impl Summary {
    /// Counts `outcomes` by how each ended.
    #[must_use]
    pub fn of(outcomes: &[ActionOutcome]) -> Self {
        let mut summary = Self {
            failed: 0,
            skipped: 0,
            built: 0,
            cached: 0,
        };
        for outcome in outcomes {
            match outcome.class {
                OutcomeClass::Cached => summary.cached += 1,
                OutcomeClass::Built => summary.built += 1,
                OutcomeClass::ActionFailure { .. }
                | OutcomeClass::OutputMissing { .. }
                | OutcomeClass::InfraFailure { .. } => summary.failed += 1,
                OutcomeClass::Skipped { .. } => summary.skipped += 1,
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{ActionOutcome, InfraKind, OutcomeClass, Summary};
    use crate::json::canonical;
    use crate::types::{Label, OutputName};

    /// One class of every kind, in the order the enum declares them.
    fn every_class() -> [OutcomeClass; 6] {
        [
            OutcomeClass::Cached,
            OutcomeClass::Built,
            OutcomeClass::ActionFailure { status: 2 },
            OutcomeClass::OutputMissing {
                name: OutputName::parse("bin/app").unwrap(),
            },
            OutcomeClass::InfraFailure {
                kind: InfraKind::Store,
            },
            OutcomeClass::Skipped {
                because: Label::parse("//lib:text").unwrap(),
            },
        ]
    }

    /// An outcome of `class` for `//:app`, holding nothing else.
    fn outcome(class: OutcomeClass) -> ActionOutcome {
        ActionOutcome {
            label: Label::parse("//:app").unwrap(),
            class,
            diagnostic: None,
            outputs: Vec::new(),
            stdout: None,
            stderr: None,
        }
    }

    #[test]
    fn only_a_cached_and_a_built_target_succeeded() {
        let succeeded = every_class().map(|class| class.succeeded());
        assert_eq!(succeeded, [true, true, false, false, false, false]);
    }

    #[test]
    fn every_kind_of_infrastructure_failure_is_a_failure() {
        for kind in [
            InfraKind::Prepare,
            InfraKind::Run,
            InfraKind::TimedOut,
            InfraKind::Store,
        ] {
            assert!(!OutcomeClass::InfraFailure { kind }.succeeded());
        }
    }

    #[test]
    fn a_summary_counts_each_class_once() {
        let outcomes: Vec<ActionOutcome> = every_class().into_iter().map(outcome).collect();
        assert_eq!(
            Summary::of(&outcomes),
            Summary {
                failed: 3,
                skipped: 1,
                built: 1,
                cached: 1,
            }
        );
    }

    #[test]
    fn a_summary_of_no_outcome_counts_nothing() {
        assert_eq!(
            Summary::of(&[]),
            Summary {
                failed: 0,
                skipped: 0,
                built: 0,
                cached: 0,
            }
        );
    }

    #[test]
    fn a_summary_adds_outcomes_of_one_class() {
        let outcomes = [
            outcome(OutcomeClass::Built),
            outcome(OutcomeClass::Built),
            outcome(OutcomeClass::ActionFailure { status: 1 }),
            outcome(OutcomeClass::Cached),
            outcome(OutcomeClass::Built),
        ];
        assert_eq!(
            Summary::of(&outcomes),
            Summary {
                failed: 1,
                skipped: 0,
                built: 3,
                cached: 1,
            }
        );
    }

    #[test]
    fn a_class_without_fields_serializes_as_its_name() {
        assert_eq!(
            canonical::to_string(&OutcomeClass::Cached).unwrap(),
            r#""Cached""#
        );
        assert_eq!(
            canonical::to_string(&OutcomeClass::Built).unwrap(),
            r#""Built""#
        );
    }

    #[test]
    fn a_class_with_a_field_serializes_as_an_object_of_one_key() {
        let rendered: Vec<String> = every_class()
            .iter()
            .skip(2)
            .map(|class| canonical::to_string(class).unwrap())
            .collect();
        assert_eq!(
            rendered,
            [
                r#"{"ActionFailure":{"status":2}}"#,
                r#"{"OutputMissing":{"name":"bin/app"}}"#,
                r#"{"InfraFailure":{"kind":"Store"}}"#,
                r#"{"Skipped":{"because":"//lib:text"}}"#,
            ]
        );
    }

    #[test]
    fn every_kind_of_infrastructure_failure_serializes_as_its_name() {
        let rendered: Vec<String> = [
            InfraKind::Prepare,
            InfraKind::Run,
            InfraKind::TimedOut,
            InfraKind::Store,
        ]
        .iter()
        .map(|kind| canonical::to_string(kind).unwrap())
        .collect();
        assert_eq!(
            rendered,
            [r#""Prepare""#, r#""Run""#, r#""TimedOut""#, r#""Store""#]
        );
    }
}
