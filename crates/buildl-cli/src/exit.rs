//! The status the process ends with, and which one each ending of a command gets.
//!
//! Its own file because the numbers are a contract: a script, a CI job and an editor read them
//! to tell a mistake in the code from a mistake in the build files and from a sick builder.
//! They are chosen from what happened as a value, the class of an outcome or the variant of an
//! error, and never from the text of a message.
//!
//! Responsibilities: [`Exit`], and the three mappings onto it: [`Exit::of_class`],
//! [`Exit::of_outcomes`] and [`Exit::of_error`].
//!
//! Non-responsibilities: what a refused command line gets, which the argument parser's own
//! verdict decides; and ending the process, which `main` does.

use buildl::buildl_core::{ActionOutcome, Error, OutcomeClass, PortName};

/// The status the process ends with.
///
/// The numbers stay clear of 126, 127 and everything above 128, to which a shell gives meanings
/// of its own. The status an action exited with is never the process's: it is in the report of
/// the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Exit {
    /// The command did what it was asked.
    Success = 0,
    /// An action failed: the code being built is at fault.
    ActionFailure = 1,
    /// The command line is at fault.
    Usage = 2,
    /// The builder or the machine is at fault; running again may help.
    Infrastructure = 3,
    /// The build wants more than the workspace's ceiling allows.
    Refused = 4,
    /// A build file or `buildl.toml` is at fault.
    Definition = 5,
    /// A state file is not what buildl writes.
    CorruptState = 6,
    /// buildl itself is at fault.
    Internal = 7,
}

impl Exit {
    /// The status of a build in which a target ended in `class`.
    ///
    /// A skipped target is a success here: it was skipped because another target failed, and
    /// that failure decides the status. A class this binary has no row for is buildl's fault.
    pub(crate) const fn of_class(class: &OutcomeClass) -> Self {
        match class {
            OutcomeClass::Cached | OutcomeClass::Built | OutcomeClass::Skipped { .. } => {
                Self::Success
            }
            OutcomeClass::ActionFailure { .. }
            | OutcomeClass::Killed { .. }
            | OutcomeClass::OutputMissing { .. }
            | OutcomeClass::OutputRefused { .. } => Self::ActionFailure,
            OutcomeClass::InfraFailure { .. } => Self::Infrastructure,
            _ => Self::Internal,
        }
    }

    /// The status of a build that ended with `outcomes`: the most severe one any of them has.
    ///
    /// A fault of buildl outranks a failure of the builder, which outranks a failed action: a
    /// sick builder means the run says nothing reliable about the code.
    pub(crate) fn of_outcomes(outcomes: &[ActionOutcome]) -> Self {
        outcomes
            .iter()
            .map(|outcome| Self::of_class(&outcome.class))
            .max_by_key(|exit| exit.severity())
            .unwrap_or(Self::Success)
    }

    /// The status of a command that ended in `error`.
    ///
    /// An error this binary has no row for is buildl's fault, never the user's.
    pub(crate) const fn of_error(error: &Error) -> Self {
        match error {
            Error::UnknownOverride { .. } | Error::UnknownTarget { .. } => Self::Usage,
            Error::MissingTool { .. } => Self::Infrastructure,
            // A manifest that is missing, refused, or asks for what this buildl cannot run is
            // fixed by editing it; every other port failing is the builder's trouble.
            Error::Adapter { port, .. } => match port {
                PortName::Manifest => Self::Definition,
                _ => Self::Infrastructure,
            },
            Error::Refused { .. } => Self::Refused,
            Error::Evaluation { .. }
            | Error::MissingBuildFile { .. }
            | Error::InvalidDeclaration { .. }
            | Error::ActionConflict { .. }
            | Error::Nondeterministic { .. }
            | Error::DuplicateLabel { .. }
            | Error::DuplicateSetting { .. }
            | Error::UnknownReference { .. }
            | Error::WrongReferenceKind { .. }
            | Error::UnknownSetting { .. }
            | Error::DependencyCycle { .. }
            | Error::TooManyTargets { .. }
            | Error::MissingInput { .. }
            | Error::InputNotAFile { .. }
            | Error::OutputConflict { .. }
            | Error::NoProgram { .. } => Self::Definition,
            Error::CorruptState { .. }
            | Error::UnreadableJson { .. }
            | Error::NonCanonicalJson { .. } => Self::CorruptState,
            Error::InvalidName { .. }
            | Error::CanonicalJson { .. }
            | Error::FloatRejected { .. }
            | Error::ForeignPlan
            | Error::DispatcherFault { .. }
            | Error::InvalidTree { .. }
            | _ => Self::Internal,
        }
    }

    /// How much a status outranks another when one build has both.
    const fn severity(self) -> u8 {
        match self {
            Self::Internal => 3,
            Self::Infrastructure => 2,
            Self::ActionFailure => 1,
            Self::Success | Self::Usage | Self::Refused | Self::Definition | Self::CorruptState => {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::mem;
    use std::path::PathBuf;
    use std::slice;

    use buildl::buildl_core::json::canonical;
    use buildl::buildl_core::{
        ActionFound, ActionOutcome, Ceiling, DeclarationField, DeclarationOrder, DeclarationSite,
        DeclaredKind, Diagnostic, Directory, Error, EvaluationFailure, InfraKind, InputKind, Label,
        OutcomeClass, OutputName, OutputPath, OutputRefusal, PortName, Program, Provenance,
        SettingName, SourcePath, TreeFault, Want,
    };

    use super::Exit;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// The root build file.
    fn root_file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::root())
    }

    /// The declaration of `//:app` in the root build file.
    fn site() -> DeclarationSite {
        DeclarationSite {
            label: label("//:app"),
            provenance: root_file(),
        }
    }

    /// A ceiling that allows nothing.
    fn nothing() -> Ceiling {
        Ceiling {
            run: BTreeSet::new(),
            env: BTreeSet::new(),
            read: BTreeSet::new(),
            write: BTreeSet::new(),
        }
    }

    /// The error of a port's adapter.
    fn adapter(port: PortName) -> Error {
        Error::Adapter {
            port,
            diagnostic: Diagnostic::new("it failed"),
        }
    }

    /// A value of each variant that is a mistake on the command line.
    fn usage_errors() -> Vec<Error> {
        vec![
            Error::UnknownOverride {
                name: SettingName::parse("mode").unwrap(),
            },
            Error::UnknownTarget {
                label: label("//:nope"),
                suggestion: None,
            },
        ]
    }

    /// A value of each variant that is the builder's trouble: a missing program, and an adapter
    /// other than the manifest's.
    fn infrastructure_errors() -> Vec<Error> {
        vec![
            Error::MissingTool {
                site: Box::new(site()),
                program: Program::parse("cc").unwrap(),
            },
            adapter(PortName::Digester),
        ]
    }

    /// A value of the one variant that is a want beyond the ceiling.
    fn refusals() -> Vec<Error> {
        vec![Error::Refused {
            want: Want::Program(Program::parse("cc").unwrap()),
            site: Some(Box::new(site())),
            ceiling: Box::new(nothing()),
        }]
    }

    /// A value of each variant that is a mistake of a build file found while loading it.
    fn load_errors() -> Vec<Error> {
        vec![
            Error::Evaluation {
                provenance: root_file(),
                failure: EvaluationFailure::Syntax,
                diagnostic: Diagnostic::new("unexpected symbol"),
            },
            Error::MissingBuildFile {
                directory: Directory::parse("lib").unwrap(),
                requested_by: Some(root_file()),
            },
            Error::InvalidDeclaration {
                provenance: root_file(),
                order: DeclarationOrder::new(1),
                field: DeclarationField::Name,
                source: Box::new(Label::parse("app").unwrap_err()),
            },
            Error::ActionConflict {
                provenance: root_file(),
                order: DeclarationOrder::new(1),
                found: ActionFound::Neither,
            },
            Error::Nondeterministic {
                provenance: root_file(),
                first_run: None,
                second_run: None,
            },
        ]
    }

    /// A value of each variant that is a mistake of a build file found while resolving the
    /// graph.
    fn resolve_errors() -> Vec<Error> {
        vec![
            Error::DuplicateLabel {
                label: label("//:app"),
                sites: vec![root_file(), root_file()],
            },
            Error::DuplicateSetting {
                name: SettingName::parse("mode").unwrap(),
                sites: vec![root_file(), root_file()],
            },
            Error::UnknownReference {
                site: Box::new(site()),
                field: DeclarationField::Dep,
                reference: label("//:lib"),
                suggestion: None,
            },
            Error::WrongReferenceKind {
                site: Box::new(site()),
                field: DeclarationField::Dep,
                reference: label("//:cc"),
                found: DeclaredKind::Rule,
                declared_in: root_file(),
            },
            Error::UnknownSetting {
                site: Box::new(site()),
                name: "mode".to_owned(),
                suggestion: None,
            },
            Error::DependencyCycle { path: vec![site()] },
            Error::TooManyTargets { count: 7 },
        ]
    }

    /// A value of each variant that is a mistake of a build file found while planning, or
    /// before anything runs.
    fn plan_errors() -> Vec<Error> {
        let output = OutputPath::of(
            &Directory::parse("out").unwrap(),
            &label("//:app"),
            &OutputName::parse("app").unwrap(),
        );
        vec![
            Error::MissingInput {
                site: Box::new(site()),
                path: SourcePath::parse("main.c").unwrap(),
            },
            Error::InputNotAFile {
                site: Box::new(site()),
                path: SourcePath::parse("main.c").unwrap(),
                found: InputKind::Directory,
            },
            Error::OutputConflict {
                path: output,
                first: Box::new(site()),
                second: Box::new(site()),
            },
            Error::NoProgram {
                site: Box::new(site()),
            },
        ]
    }

    /// A value of each variant that is a state file buildl did not write.
    fn state_errors() -> Vec<Error> {
        let unreadable = canonical::from_slice::<Label>(b"{").unwrap_err();
        assert!(
            matches!(unreadable, Error::UnreadableJson { .. }),
            "{unreadable:?}"
        );
        vec![
            unreadable,
            Error::NonCanonicalJson { offset: 7 },
            Error::CorruptState {
                file: PathBuf::from(".buildl/cache.json"),
                source: Box::new(Error::NonCanonicalJson { offset: 7 }),
            },
        ]
    }

    /// A value of each variant that only a fault of buildl itself can bring about.
    fn internal_errors() -> Vec<Error> {
        let name = Label::parse("app").unwrap_err();
        assert!(matches!(name, Error::InvalidName { .. }), "{name:?}");
        let unwritable = canonical::to_vec(&BTreeMap::from([((1_u8, 2_u8), 3_u8)])).unwrap_err();
        assert!(
            matches!(unwritable, Error::CanonicalJson { .. }),
            "{unwritable:?}"
        );
        let float = canonical::to_vec(&1.5_f64).unwrap_err();
        assert!(matches!(float, Error::FloatRejected { .. }), "{float:?}");
        vec![
            name,
            unwritable,
            float,
            Error::ForeignPlan,
            Error::DispatcherFault {
                targets: vec![label("//:app")],
            },
            Error::InvalidTree {
                fault: TreeFault::Empty,
            },
        ]
    }

    /// What became of a target whose build ended in `class`.
    fn ended(class: OutcomeClass) -> ActionOutcome {
        ActionOutcome {
            label: label("//:app"),
            class,
            diagnostic: None,
            outputs: Vec::new(),
            stdout: None,
            stderr: None,
        }
    }

    /// One value of every class of [`OutcomeClass`], each with the status it gives a build.
    fn every_class() -> Vec<(OutcomeClass, Exit)> {
        let name = OutputName::parse("app").unwrap();
        vec![
            (OutcomeClass::Cached, Exit::Success),
            (OutcomeClass::Built, Exit::Success),
            (
                OutcomeClass::Skipped {
                    because: label("//:lib"),
                },
                Exit::Success,
            ),
            (
                OutcomeClass::ActionFailure { status: 2 },
                Exit::ActionFailure,
            ),
            (OutcomeClass::Killed { signal: 9 }, Exit::ActionFailure),
            (
                OutcomeClass::OutputMissing { name: name.clone() },
                Exit::ActionFailure,
            ),
            (
                OutcomeClass::OutputRefused {
                    name,
                    refusal: OutputRefusal::Symlink,
                },
                Exit::ActionFailure,
            ),
            (
                OutcomeClass::InfraFailure {
                    kind: InfraKind::TimedOut,
                },
                Exit::Infrastructure,
            ),
        ]
    }

    #[test]
    fn the_codes_are_the_published_numbers() {
        let codes: Vec<u8> = [
            Exit::Success,
            Exit::ActionFailure,
            Exit::Usage,
            Exit::Infrastructure,
            Exit::Refused,
            Exit::Definition,
            Exit::CorruptState,
            Exit::Internal,
        ]
        .into_iter()
        .map(|exit| exit as u8)
        .collect();
        assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn every_error_variant_has_its_code() {
        let rows = [
            (Exit::Usage, usage_errors()),
            (Exit::Infrastructure, infrastructure_errors()),
            (Exit::Refused, refusals()),
            (
                Exit::Definition,
                [load_errors(), resolve_errors(), plan_errors()]
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
            (Exit::CorruptState, state_errors()),
            (Exit::Internal, internal_errors()),
        ];
        let counts: Vec<usize> = rows.iter().map(|(_, errors)| errors.len()).collect();
        assert_eq!(counts, [2, 2, 1, 16, 3, 6]);
        let variants: HashSet<_> = rows
            .iter()
            .flat_map(|(_, errors)| errors.iter().map(mem::discriminant))
            .collect();
        assert_eq!(variants.len(), 30, "a variant is listed twice");
        for (exit, errors) in &rows {
            for error in errors {
                assert_eq!(Exit::of_error(error), *exit, "{error:?}");
            }
        }
    }

    #[test]
    fn only_the_manifests_adapter_is_a_definition_error() {
        for port in [
            PortName::Host,
            PortName::Digester,
            PortName::StatCache,
            PortName::ToolResolver,
            PortName::ActionCache,
            PortName::ContentStore,
            PortName::EventLog,
            PortName::ExecStrategy,
            PortName::Dispatcher,
            PortName::GraphRecord,
        ] {
            assert_eq!(
                Exit::of_error(&adapter(port)),
                Exit::Infrastructure,
                "{port}"
            );
        }
        assert_eq!(
            Exit::of_error(&adapter(PortName::Manifest)),
            Exit::Definition
        );
    }

    #[test]
    fn every_class_has_its_code() {
        for (class, exit) in every_class() {
            assert_eq!(Exit::of_class(&class), exit, "{class:?}");
            assert_eq!(
                Exit::of_outcomes(&[ended(class.clone())]),
                exit,
                "{class:?}"
            );
        }
    }

    #[test]
    fn infrastructure_beats_action_failure() {
        let outcomes = [
            ended(OutcomeClass::ActionFailure { status: 1 }),
            ended(OutcomeClass::InfraFailure {
                kind: InfraKind::Run,
            }),
            ended(OutcomeClass::Built),
        ];
        assert_eq!(Exit::of_outcomes(&outcomes), Exit::Infrastructure);
        let reversed: Vec<ActionOutcome> = outcomes.into_iter().rev().collect();
        assert_eq!(Exit::of_outcomes(&reversed), Exit::Infrastructure);
    }

    #[test]
    fn an_action_failure_beats_success() {
        let outcomes = [
            ended(OutcomeClass::Cached),
            ended(OutcomeClass::Killed { signal: 11 }),
            ended(OutcomeClass::Built),
        ];
        assert_eq!(Exit::of_outcomes(&outcomes), Exit::ActionFailure);
    }

    #[test]
    fn a_skip_never_decides_the_code() {
        let skipped = ended(OutcomeClass::Skipped {
            because: label("//:lib"),
        });
        assert_eq!(Exit::of_outcomes(slice::from_ref(&skipped)), Exit::Success);
        let failed = ended(OutcomeClass::OutputMissing {
            name: OutputName::parse("lib").unwrap(),
        });
        assert_eq!(Exit::of_outcomes(&[skipped, failed]), Exit::ActionFailure);
    }

    #[test]
    fn a_build_of_nothing_succeeds() {
        assert_eq!(Exit::of_outcomes(&[]), Exit::Success);
    }
}
