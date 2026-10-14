//! How the build of one target ended, and the count of a build's outcomes.
//!
//! Its own file because an outcome is what a build hands back: one per target, whether the
//! target ran, was taken from the cache, failed, or was never begun. A target that fails is an
//! outcome and not an error, so one failure does not hide what the rest of the build did.
//!
//! Responsibilities: [`ActionOutcome`], [`OutcomeClass`] with [`OutcomeClass::succeeded`],
//! [`InfraKind`], [`OutputRefusal`], and [`Summary`] with [`Summary::of`].
//!
//! Non-responsibilities: deciding which class a target's build ends in, which the phase that
//! executes does; and rendering an outcome for a person, which belongs to whoever shows it.

use serde::{Deserialize, Serialize};

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

/// Why what an action left at an output path cannot be kept.
///
/// An output is a regular file, or a directory of regular files with no empty directory in it.
/// Anything else could not be put back exactly as it was, so it is refused rather than stored
/// as something close. Serializes as its name. Non-exhaustive: an output that can be refused
/// another way gains a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OutputRefusal {
    /// The output, or something in its directory, is a symbolic link.
    Symlink,
    /// The output, or something in its directory, is neither a file, a directory nor a
    /// symbolic link: a device, a socket, a named pipe.
    Special,
    /// The output is a directory holding nothing, or holds one.
    EmptyDirectory,
    /// Something in the output's directory has a name that is not valid Unicode.
    NotText,
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
    /// It ran and a signal ended it before it exited.
    Killed {
        /// The signal's number.
        signal: i32,
    },
    /// It ran and exited with status zero, and left nothing where an output it declares should
    /// be.
    OutputMissing {
        /// The first such output, in declared order.
        name: OutputName,
    },
    /// It ran and exited with status zero, and left where an output it declares should be
    /// something that cannot be kept.
    OutputRefused {
        /// The first such output, in declared order.
        name: OutputName,
        /// Why it cannot be kept.
        refusal: OutputRefusal,
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
    /// and for an [`OutcomeClass::OutputRefused`], where it names what was refused; `None` for
    /// every other outcome.
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
    /// Targets whose action failed or was ended by a signal, whose output was missing or
    /// refused, or that the builder could not build.
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
                | OutcomeClass::Killed { .. }
                | OutcomeClass::OutputMissing { .. }
                | OutcomeClass::OutputRefused { .. }
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

    use super::{ActionOutcome, InfraKind, OutcomeClass, OutputRefusal, Summary};
    use crate::json::canonical;
    use crate::types::{Label, OutputName};

    /// One class of every kind, in the order the enum declares them.
    fn every_class() -> [OutcomeClass; 8] {
        [
            OutcomeClass::Cached,
            OutcomeClass::Built,
            OutcomeClass::ActionFailure { status: 2 },
            OutcomeClass::Killed { signal: 11 },
            OutcomeClass::OutputMissing {
                name: OutputName::parse("bin/app").unwrap(),
            },
            OutcomeClass::OutputRefused {
                name: OutputName::parse("dist").unwrap(),
                refusal: OutputRefusal::Symlink,
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
        assert_eq!(
            succeeded,
            [true, true, false, false, false, false, false, false]
        );
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
                failed: 5,
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
                r#"{"Killed":{"signal":11}}"#,
                r#"{"OutputMissing":{"name":"bin/app"}}"#,
                r#"{"OutputRefused":{"name":"dist","refusal":"Symlink"}}"#,
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

    #[test]
    fn every_refusal_serializes_as_its_name_and_reads_back() {
        for (refusal, text) in [
            (OutputRefusal::Symlink, r#""Symlink""#),
            (OutputRefusal::Special, r#""Special""#),
            (OutputRefusal::EmptyDirectory, r#""EmptyDirectory""#),
            (OutputRefusal::NotText, r#""NotText""#),
        ] {
            assert_eq!(canonical::to_string(&refusal).unwrap(), text);
            assert_eq!(
                serde_json::from_str::<OutputRefusal>(text).unwrap(),
                refusal
            );
        }
        assert!(serde_json::from_str::<OutputRefusal>(r#""Hardlink""#).is_err());
    }
}
