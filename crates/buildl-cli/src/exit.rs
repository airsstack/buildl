//! The status the process ends with, and which one each ending of a command gets.
//!
//! Its own file because the numbers are a contract: a script, a CI job and an editor read them
//! to tell a mistake in the code from a mistake in the build files and from a sick builder.
//! They are chosen from what happened as a value, the class of an outcome or the variant of an
//! error, and never from the text of a message.
//!
//! Responsibilities: [`Exit`], and the four mappings onto it: [`Exit::of_class`],
//! [`Exit::of_outcomes`], [`Exit::of_verification`] and [`Exit::of_error`].
//!
//! Non-responsibilities: what a refused command line gets, which the argument parser's own
//! verdict decides; and ending the process, which `main` does.

use buildl::buildl_core::{ActionOutcome, Check, Error, OutcomeClass, PortName, Verification};

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
    /// The build wants more than the workspace allows or than was approved.
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

    /// The status of a verify that found `verification`: the most severe one any check has.
    ///
    /// A mismatch is the code's fault. A re-run that failed gets the status `build` would give
    /// its class. A check that could not be made, because the content the cache names was not
    /// held or not put back, is the store's failure and not the action's. A verify that checked
    /// nothing succeeds.
    pub(crate) fn of_verification(verification: &Verification) -> Self {
        verification
            .checked()
            .iter()
            .map(|(_, check)| match check {
                Check::Verified => Self::Success,
                Check::Mismatched { .. } => Self::ActionFailure,
                Check::Failed { class, .. } => Self::of_class(class),
                Check::Skipped { .. } | Check::Unplaced { .. } => Self::Infrastructure,
            })
            .max_by_key(|exit| exit.severity())
            .unwrap_or(Self::Success)
    }

    /// The status of a command that ended in `error`.
    ///
    /// An error this binary has no row for is buildl's fault, never the user's.
    pub(crate) const fn of_error(error: &Error) -> Self {
        match error {
            Error::UnknownOverride { .. }
            | Error::UnknownTarget { .. }
            | Error::InvalidQuery { .. }
            | Error::NoRun { .. }
            | Error::NotATest { .. } => Self::Usage,
            Error::MissingTool { .. } | Error::MissingOutput { .. } | Error::PluginFetch { .. } => {
                Self::Infrastructure
            }
            // A manifest that is missing, refused, or asks for what this buildl cannot run is
            // fixed by editing it; every other port failing is the builder's trouble.
            Error::Adapter { port, .. } => match port {
                PortName::Manifest => Self::Definition,
                _ => Self::Infrastructure,
            },
            Error::Refused { .. } | Error::PluginRefused { .. } | Error::NotApproved { .. } => {
                Self::Refused
            }
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
            | Error::PinnedEnv { .. }
            | Error::OutputConflict { .. }
            | Error::NoProgram { .. }
            | Error::PluginTree { .. }
            | Error::PluginNotFetched { .. } => Self::Definition,
            Error::CorruptState { .. }
            | Error::UnreadableJson { .. }
            | Error::NonCanonicalJson { .. }
            | Error::PluginChanged { .. } => Self::CorruptState,
            Error::InvalidName { .. }
            | Error::CanonicalJson { .. }
            | Error::FloatRejected { .. }
            | Error::ForeignPlan
            | Error::DispatcherFault { .. }
            | Error::LostBuildFiles { .. }
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
        ActionFound, ActionOutcome, Ceiling, Check, DeclarationField, DeclarationOrder,
        DeclarationSite, DeclaredKind, DenialReason, Diagnostic, Directory, EnvName, EnvValue,
        Error, EvaluationFailure, FileChange, InfraKind, InputKind, Label, Origin, OutcomeClass,
        OutputDifference, OutputName, OutputPath, OutputRefusal, PluginName, PluginRef,
        PluginVersion, PluginViolation, PortName, Program, Provenance, SettingName, SourcePath,
        TargetRole, TreeFault, Verification, Want, WantedSet,
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
            origin: Origin::BuildFile,
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
            Error::InvalidQuery {
                expression: "dep(//:app)".to_owned(),
            },
            Error::NoRun {
                label: label("//:app"),
                suggestion: None,
                damaged: Vec::new(),
            },
            Error::NotATest {
                asked: label("//:app"),
                site: Box::new(site()),
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
            Error::MissingOutput {
                label: label("//:app"),
            },
            plugin_fetch(),
        ]
    }

    /// A value of each variant that is a want the build may not have.
    fn refusals() -> Vec<Error> {
        vec![
            Error::Refused {
                want: Want::Program(Program::parse("cc").unwrap()),
                site: Some(Box::new(site())),
                ceiling: Box::new(nothing()),
            },
            plugin_refused(),
            not_approved(),
        ]
    }

    /// A plugin's refusal of a target that wants to run `curl`.
    fn plugin_refused() -> Error {
        let go = PluginRef {
            name: PluginName::parse("go").unwrap(),
            version: PluginVersion::parse("1.3.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        };
        Error::PluginRefused {
            violation: Box::new(PluginViolation {
                want: Want::Program(Program::parse("curl").unwrap()),
                site: DeclarationSite {
                    origin: Origin::plugins(vec![go.clone()]).unwrap(),
                    ..site()
                },
                plugin: go,
                allowed: BTreeSet::new(),
            }),
        }
    }

    /// The approver's refusal of a build that wants to run `cc`.
    fn not_approved() -> Error {
        Error::NotApproved {
            unapproved: Box::new(WantedSet {
                programs: BTreeSet::from([Program::parse("cc").unwrap()]),
                env: BTreeSet::new(),
                write: BTreeSet::new(),
                network: BTreeSet::new(),
            }),
            reason: DenialReason::new("not approved"),
        }
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
                origin: Origin::BuildFile,
                order: DeclarationOrder::new(1),
                field: DeclarationField::Name,
                source: Box::new(Label::parse("app").unwrap_err()),
            },
            Error::ActionConflict {
                provenance: root_file(),
                origin: Origin::BuildFile,
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
                sites: vec![site(), site()],
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

    /// A value of each variant that is a mistake in what a workspace's plugins hold.
    fn plugin_errors() -> Vec<Error> {
        vec![plugin_tree(), plugin_not_fetched()]
    }

    /// A plugin the lock does not pin.
    fn plugin_not_fetched() -> Error {
        Error::PluginNotFetched {
            plugin: PluginName::parse("go").unwrap(),
            reason: "run `buildl plugins fetch`".to_owned(),
        }
    }

    /// A plugin whose store was edited after it was fetched.
    fn plugin_changed() -> Error {
        Error::PluginChanged {
            plugin: PluginName::parse("go").unwrap(),
            reason: "its files are not the ones that were locked".to_owned(),
        }
    }

    /// A plugin `git` could not fetch.
    fn plugin_fetch() -> Error {
        Error::PluginFetch {
            plugin: PluginName::parse("go").unwrap(),
            diagnostic: Diagnostic::new("git was not found"),
        }
    }

    /// A plugin whose directory holds a link.
    fn plugin_tree() -> Error {
        Error::PluginTree {
            plugin: PluginName::parse("go").unwrap(),
            path: PathBuf::from(".buildl/plugins/go/c0ffee/bin/link"),
        }
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
            Error::PinnedEnv {
                name: EnvName::parse("TZ").unwrap(),
                value: EnvValue::parse("UTC").unwrap(),
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
            Error::LostBuildFiles {
                directories: vec![Directory::root()],
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
            role: TargetRole::Build,
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
    fn no_run_is_a_usage_error() {
        let error = Error::NoRun {
            label: label("//:app"),
            suggestion: Some(label("//:apps")),
            damaged: Vec::new(),
        };
        assert_eq!(Exit::of_error(&error), Exit::Usage);
    }

    #[test]
    fn not_a_test_is_a_usage_error() {
        let error = Error::NotATest {
            asked: label("//:default"),
            site: Box::new(site()),
        };
        assert_eq!(Exit::of_error(&error), Exit::Usage);
    }

    #[test]
    fn a_lost_build_file_is_an_internal_fault() {
        let lost = Error::LostBuildFiles {
            directories: vec![Directory::root()],
        };
        let fault = Error::DispatcherFault {
            targets: vec![label("//:app")],
        };
        assert_eq!(Exit::of_error(&lost), Exit::of_error(&fault));
        assert_eq!(Exit::of_error(&lost), Exit::Internal);
    }

    #[test]
    fn not_approved_exits_four() {
        assert_eq!(Exit::of_error(&not_approved()), Exit::Refused);
        assert_eq!(Exit::Refused as u8, 4);
    }

    #[test]
    fn plugin_refused_exits_4() {
        assert_eq!(Exit::of_error(&plugin_refused()), Exit::Refused);
        assert_eq!(Exit::Refused as u8, 4);
    }

    #[test]
    fn plugin_fetch_exits_3() {
        assert_eq!(Exit::of_error(&plugin_fetch()), Exit::Infrastructure);
        assert_eq!(Exit::Infrastructure as u8, 3);
    }

    #[test]
    fn plugin_tree_exits_5() {
        assert_eq!(Exit::of_error(&plugin_tree()), Exit::Definition);
        assert_eq!(Exit::Definition as u8, 5);
    }

    #[test]
    fn plugin_not_fetched_exits_5_and_changed_exits_6() {
        assert_eq!(Exit::of_error(&plugin_not_fetched()), Exit::Definition);
        assert_eq!(Exit::of_error(&plugin_changed()), Exit::CorruptState);
        assert_eq!(Exit::Definition as u8, 5);
        assert_eq!(Exit::CorruptState as u8, 6);
    }

    #[test]
    fn missing_output_is_an_infrastructure_failure() {
        let error = Error::MissingOutput {
            label: label("//:app"),
        };
        assert_eq!(Exit::of_error(&error), Exit::Infrastructure);
    }

    #[test]
    fn every_error_variant_has_its_code() {
        let rows = [
            (Exit::Usage, usage_errors()),
            (Exit::Infrastructure, infrastructure_errors()),
            (Exit::Refused, refusals()),
            (
                Exit::Definition,
                [
                    load_errors(),
                    resolve_errors(),
                    plan_errors(),
                    plugin_errors(),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            (
                Exit::CorruptState,
                [state_errors(), vec![plugin_changed()]]
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
            (Exit::Internal, internal_errors()),
        ];
        let counts: Vec<usize> = rows.iter().map(|(_, errors)| errors.len()).collect();
        assert_eq!(counts, [5, 4, 3, 19, 4, 7]);
        let variants: HashSet<_> = rows
            .iter()
            .flat_map(|(_, errors)| errors.iter().map(mem::discriminant))
            .collect();
        assert_eq!(variants.len(), 42, "a variant is listed twice");
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
            PortName::Approver,
            PortName::GraphRecord,
            PortName::RunHistory,
            PortName::PluginStore,
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

    /// The status of a verify whose only check, of `//:app`, is `check`.
    fn verified_as(check: Check) -> Exit {
        Exit::of_verification(&Verification::new(vec![(label("//:app"), check)], 0, 0))
    }

    /// A check of a re-run that ended in `class`.
    fn failed_as(class: OutcomeClass) -> Check {
        Check::Failed {
            class,
            diagnostic: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn a_mismatch_exits_as_an_action_failure() {
        let mismatch = Check::Mismatched {
            output: OutputName::parse("app").unwrap(),
            difference: OutputDifference::File(FileChange::Bytes),
        };
        assert_eq!(verified_as(mismatch), Exit::ActionFailure);
    }

    #[test]
    fn a_skip_or_an_unplaced_exits_as_infrastructure() {
        let skipped = Check::Skipped {
            because: label("//:lib"),
        };
        let unplaced = Check::Unplaced {
            diagnostic: Diagnostic::new("the blob is gone"),
        };
        assert_eq!(verified_as(skipped), Exit::Infrastructure);
        assert_eq!(verified_as(unplaced), Exit::Infrastructure);
    }

    #[test]
    fn a_failed_rerun_exits_as_its_class() {
        for (class, exit) in every_class() {
            assert_eq!(verified_as(failed_as(class.clone())), exit, "{class:?}");
        }
    }

    #[test]
    fn the_most_severe_check_decides() {
        let checks = vec![
            (label("//:a"), Check::Verified),
            (
                label("//:b"),
                failed_as(OutcomeClass::ActionFailure { status: 1 }),
            ),
            (
                label("//:c"),
                Check::Skipped {
                    because: label("//:d"),
                },
            ),
        ];
        assert_eq!(
            Exit::of_verification(&Verification::new(checks, 0, 0)),
            Exit::Infrastructure
        );
        let milder = vec![
            (label("//:a"), Check::Verified),
            (
                label("//:b"),
                failed_as(OutcomeClass::ActionFailure { status: 1 }),
            ),
        ];
        assert_eq!(
            Exit::of_verification(&Verification::new(milder, 0, 0)),
            Exit::ActionFailure
        );
        assert_eq!(verified_as(Check::Verified), Exit::Success);
    }

    #[test]
    fn nothing_checked_is_success() {
        assert_eq!(
            Exit::of_verification(&Verification::new(Vec::new(), 3, 1)),
            Exit::Success
        );
    }
}
