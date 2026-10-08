//! The crate's one error type and the `Result` alias every fallible call returns.
//!
//! A single file rather than one error type per module: a caller branches on a field, never on
//! the text of a message, and every module in this crate returns the same `Result` without a
//! conversion at the boundary.
//!
//! Responsibilities:
//!
//! - [`Error`] — every way a call into this crate can fail.
//! - [`NameKind`] — which domain name an [`Error::InvalidName`] is about.
//! - [`EvaluationFailure`] and [`EvaluationLimit`] — how an adapter failed to evaluate a build
//!   file.
//! - [`DeclarationField`] — which field of a declaration an [`Error::InvalidDeclaration`] is
//!   about.
//! - [`ActionFound`] — what an [`Error::ActionConflict`] found instead of one action.
//! - [`Result`] — the crate-wide alias.
//!
//! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic, on one line
//! unless an adapter's own message spans several; how a
//! command surfaces it belongs to the `Reporter` port.

use core::fmt;

use crate::types::{
    Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, Written,
};

/// The result of any fallible call in this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// Every way a call into this crate can fail.
///
/// Non-exhaustive: adding a variant is not a breaking change for downstream crates.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A domain name did not satisfy its grammar.
    #[error("invalid {kind}: {value:?} — {reason}")]
    InvalidName {
        /// Which kind of name was being built.
        kind: NameKind,
        /// The rejected input, as given.
        value: String,
        /// Why the grammar rejected it.
        reason: &'static str,
    },
    /// A value could not be serialized as JSON at all.
    #[error("value could not be serialized as canonical JSON")]
    CanonicalJson {
        /// The underlying `serde_json` failure.
        #[source]
        source: serde_json::Error,
    },
    /// A floating-point number reached the canonical serializer.
    #[error("floating-point value at {path} cannot appear in canonical JSON")]
    FloatRejected {
        /// Where in the value the float was found, as a JSONPath-style location.
        path: String,
    },
    /// An adapter could not evaluate a build file.
    #[error("{provenance}: {failure}: {diagnostic}")]
    Evaluation {
        /// The build file.
        provenance: Provenance,
        /// What kind of failure it was.
        failure: EvaluationFailure,
        /// The adapter's message, for display only.
        diagnostic: Diagnostic,
    },
    /// A directory that had to be evaluated holds no build file.
    #[error(
        "no build file in //{directory}{}",
        requested_by_suffix(.requested_by.as_ref())
    )]
    MissingBuildFile {
        /// The directory.
        directory: Directory,
        /// The build file whose `subdir` call requested it; `None` for the workspace root.
        requested_by: Option<Provenance>,
    },
    /// A value in a declaration did not satisfy its grammar.
    #[error("{provenance}: declaration {order}: invalid {field}: {source}")]
    InvalidDeclaration {
        /// The build file.
        provenance: Provenance,
        /// The call's position in the file.
        order: DeclarationOrder,
        /// The field holding the value.
        field: DeclarationField,
        /// The grammar failure.
        #[source]
        source: Box<Self>,
    },
    /// A target named both or neither of `rule` and `run`.
    #[error("{provenance}: declaration {order}: a target names {found}")]
    ActionConflict {
        /// The build file.
        provenance: Provenance,
        /// The call's position in the file.
        order: DeclarationOrder,
        /// What was found instead of exactly one.
        found: ActionFound,
    },
    /// Two evaluations of the same workspace declared different things.
    #[error("{provenance}: declarations differ between two evaluations")]
    Nondeterministic {
        /// The build file to fix: the one whose declaration the other evaluation lacks.
        provenance: Provenance,
        /// The first evaluation's declaration at the first difference; `None` past its end.
        first_run: Option<Box<Declaration>>,
        /// The second evaluation's declaration at the first difference; `None` past its end.
        second_run: Option<Box<Declaration>>,
    },
}

/// The suffix naming who required a missing build file.
fn requested_by_suffix(requested_by: Option<&Provenance>) -> String {
    requested_by.map_or_else(
        || " (the workspace root)".to_owned(),
        |provenance| format!(" (requested by {provenance})"),
    )
}

/// A ceiling an evaluation can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluationLimit {
    /// The instruction ceiling.
    Instructions,
    /// The memory ceiling.
    Memory,
    /// The cap on how many declarations one file may stage.
    Staging,
}

impl fmt::Display for EvaluationLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instructions => "instruction ceiling",
            Self::Memory => "memory ceiling",
            Self::Staging => "staging cap",
        })
    }
}

/// How an adapter failed to evaluate one build file.
///
/// Closed for an adapter, which reports every failure through it without a new variant; open
/// for a caller matching on it, which keeps a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluationFailure {
    /// The build file does not parse.
    Syntax,
    /// Evaluation raised an error.
    Runtime,
    /// The runtime refused an operation the declaration policy does not grant.
    Refused,
    /// A ceiling was reached.
    LimitReached {
        /// Which ceiling.
        limit: EvaluationLimit,
    },
    /// An option table holds a field its primitive does not accept.
    UnknownField {
        /// The field, as written.
        field: Written<FieldName>,
    },
    /// A field holds a value of the wrong type.
    WrongFieldType {
        /// The field, as written.
        field: Written<FieldName>,
    },
}

impl fmt::Display for EvaluationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("syntax error"),
            Self::Runtime => f.write_str("runtime error"),
            Self::Refused => f.write_str("operation refused"),
            Self::LimitReached { limit } => write!(f, "{limit} reached"),
            Self::UnknownField { field } => write!(f, "unknown field {:?}", field.as_written()),
            Self::WrongFieldType { field } => {
                write!(f, "field {:?} has the wrong type", field.as_written())
            }
        }
    }
}

/// Which field of a declaration an [`Error::InvalidDeclaration`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationField {
    /// A target's, rule's or alias's name.
    Name,
    /// An entry of `deps`.
    Dep,
    /// An entry of `inputs`.
    Input,
    /// An entry of `outputs`.
    Output,
    /// An entry of `env`.
    Env,
    /// The `run` command or one of its arguments.
    Run,
    /// The `rule` reference.
    Rule,
    /// A rule's `desc`.
    Description,
    /// An alias's target.
    Target,
    /// A setting's name.
    SettingName,
    /// A setting's default value.
    SettingValue,
    /// A `subdir` request.
    Subdir,
}

impl fmt::Display for DeclarationField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "name",
            Self::Dep => "dep",
            Self::Input => "input",
            Self::Output => "output",
            Self::Env => "env",
            Self::Run => "run",
            Self::Rule => "rule",
            Self::Description => "description",
            Self::Target => "target",
            Self::SettingName => "setting name",
            Self::SettingValue => "setting value",
            Self::Subdir => "subdir",
        })
    }
}

/// What an [`Error::ActionConflict`] found instead of exactly one of `rule` and `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFound {
    /// Both were given.
    Both,
    /// Neither was given.
    Neither,
}

impl fmt::Display for ActionFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Both => "both 'rule' and 'run'",
            Self::Neither => "neither 'rule' nor 'run'",
        })
    }
}

/// Which domain name an [`Error::InvalidName`] is about.
///
/// A type rather than a string so classification is read as a field. Non-exhaustive for the same
/// reason [`Error`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NameKind {
    /// A workspace-relative directory path.
    Directory,
    /// The name half of a label.
    TargetName,
    /// A whole label.
    Label,
    /// A content digest.
    Digest,
    /// A workspace-relative source file path.
    SourcePath,
    /// A path inside a target's output directory.
    OutputName,
    /// An environment variable name.
    EnvName,
    /// One element of a command's argument vector.
    Argument,
    /// A whole command: a non-empty argument vector.
    Command,
    /// A rule's one-line description.
    Description,
    /// A build setting's name.
    SettingName,
    /// A build setting's value.
    SettingValue,
    /// The file name every directory's build file has.
    EntryName,
    /// A field name in a declaration's option table.
    FieldName,
}

impl fmt::Display for NameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Directory => "directory",
            Self::TargetName => "target name",
            Self::Label => "label",
            Self::Digest => "digest",
            Self::SourcePath => "source path",
            Self::OutputName => "output name",
            Self::EnvName => "environment variable name",
            Self::Argument => "argument",
            Self::Command => "command",
            Self::Description => "description",
            Self::SettingName => "setting name",
            Self::SettingValue => "setting value",
            Self::EntryName => "entry file name",
            Self::FieldName => "field name",
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::{
        ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind,
    };
    use crate::types::{
        DeclarationOrder, Diagnostic, Directory, FieldName, Provenance, TargetName, Written,
    };

    fn lib_file() -> Provenance {
        Provenance::new(
            PathBuf::from("lib/build.lua"),
            Directory::parse("lib").unwrap(),
        )
    }

    #[test]
    fn name_kind_renders_a_human_phrase() {
        assert_eq!(NameKind::Directory.to_string(), "directory");
        assert_eq!(NameKind::TargetName.to_string(), "target name");
        assert_eq!(NameKind::Label.to_string(), "label");
        assert_eq!(NameKind::Digest.to_string(), "digest");
        assert_eq!(NameKind::SourcePath.to_string(), "source path");
        assert_eq!(NameKind::OutputName.to_string(), "output name");
        assert_eq!(NameKind::EnvName.to_string(), "environment variable name");
        assert_eq!(NameKind::Argument.to_string(), "argument");
        assert_eq!(NameKind::Command.to_string(), "command");
        assert_eq!(NameKind::Description.to_string(), "description");
        assert_eq!(NameKind::SettingName.to_string(), "setting name");
        assert_eq!(NameKind::SettingValue.to_string(), "setting value");
        assert_eq!(NameKind::EntryName.to_string(), "entry file name");
        assert_eq!(NameKind::FieldName.to_string(), "field name");
    }

    #[test]
    fn invalid_name_states_what_was_found() {
        let err = Error::InvalidName {
            kind: NameKind::Label,
            value: "//lib".to_owned(),
            reason: "must hold ':' and a target name",
        };
        assert_eq!(
            err.to_string(),
            r#"invalid label: "//lib" — must hold ':' and a target name"#
        );
    }

    #[test]
    fn canonical_json_states_the_value_could_not_be_serialized() {
        let Err(source) = serde_json::to_value(u128::MAX) else {
            unreachable!("u128::MAX has no JSON number representation")
        };
        let err = Error::CanonicalJson { source };
        assert_eq!(
            err.to_string(),
            "value could not be serialized as canonical JSON"
        );
    }

    #[test]
    fn float_rejected_states_where_the_float_was_found() {
        let err = Error::FloatRejected {
            path: "$.timings.elapsed".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "floating-point value at $.timings.elapsed cannot appear in canonical JSON"
        );
    }

    #[test]
    fn evaluation_names_the_file_the_failure_and_the_message() {
        let err = Error::Evaluation {
            provenance: lib_file(),
            failure: EvaluationFailure::Syntax,
            diagnostic: Diagnostic::new("unexpected symbol near '}'"),
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: syntax error: unexpected symbol near '}'"
        );
    }

    #[test]
    fn every_evaluation_failure_renders_a_human_phrase() {
        let field = || Written::<FieldName>::new("dep");
        let cases = [
            (EvaluationFailure::Syntax, "syntax error"),
            (EvaluationFailure::Runtime, "runtime error"),
            (EvaluationFailure::Refused, "operation refused"),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Instructions,
                },
                "instruction ceiling reached",
            ),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Memory,
                },
                "memory ceiling reached",
            ),
            (
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Staging,
                },
                "staging cap reached",
            ),
            (
                EvaluationFailure::UnknownField { field: field() },
                r#"unknown field "dep""#,
            ),
            (
                EvaluationFailure::WrongFieldType { field: field() },
                r#"field "dep" has the wrong type"#,
            ),
        ];
        for (failure, rendered) in cases {
            assert_eq!(failure.to_string(), rendered);
        }
    }

    #[test]
    fn missing_build_file_names_the_root_or_the_requester() {
        let root = Error::MissingBuildFile {
            directory: Directory::root(),
            requested_by: None,
        };
        assert_eq!(root.to_string(), "no build file in // (the workspace root)");
        let requested = Error::MissingBuildFile {
            directory: Directory::parse("lib/text").unwrap(),
            requested_by: Some(lib_file()),
        };
        assert_eq!(
            requested.to_string(),
            "no build file in //lib/text (requested by lib/build.lua)"
        );
    }

    #[test]
    fn invalid_declaration_names_the_site_the_field_and_the_grammar() {
        let source = TargetName::parse("a/b").unwrap_err();
        let source_text = source.to_string();
        let err = Error::InvalidDeclaration {
            provenance: lib_file(),
            order: DeclarationOrder::new(2),
            field: DeclarationField::Dep,
            source: Box::new(source),
        };
        assert_eq!(
            err.to_string(),
            r#"lib/build.lua: declaration #2: invalid dep: invalid target name: "a/b" — may hold only ASCII letters, digits, '.', '-' and '_'"#
        );
        assert_eq!(
            std::error::Error::source(&err).unwrap().to_string(),
            source_text
        );
    }

    #[test]
    fn every_declaration_field_renders_a_human_phrase() {
        let cases = [
            (DeclarationField::Name, "name"),
            (DeclarationField::Dep, "dep"),
            (DeclarationField::Input, "input"),
            (DeclarationField::Output, "output"),
            (DeclarationField::Env, "env"),
            (DeclarationField::Run, "run"),
            (DeclarationField::Rule, "rule"),
            (DeclarationField::Description, "description"),
            (DeclarationField::Target, "target"),
            (DeclarationField::SettingName, "setting name"),
            (DeclarationField::SettingValue, "setting value"),
            (DeclarationField::Subdir, "subdir"),
        ];
        for (field, rendered) in cases {
            assert_eq!(field.to_string(), rendered);
        }
    }

    #[test]
    fn action_conflict_states_what_was_found() {
        let both = Error::ActionConflict {
            provenance: lib_file(),
            order: DeclarationOrder::new(0),
            found: ActionFound::Both,
        };
        assert_eq!(
            both.to_string(),
            "lib/build.lua: declaration #0: a target names both 'rule' and 'run'"
        );
        let neither = Error::ActionConflict {
            provenance: lib_file(),
            order: DeclarationOrder::new(1),
            found: ActionFound::Neither,
        };
        assert_eq!(
            neither.to_string(),
            "lib/build.lua: declaration #1: a target names neither 'rule' nor 'run'"
        );
    }

    #[test]
    fn nondeterministic_names_the_file_to_fix() {
        let err = Error::Nondeterministic {
            provenance: lib_file(),
            first_run: None,
            second_run: None,
        };
        assert_eq!(
            err.to_string(),
            "lib/build.lua: declarations differ between two evaluations"
        );
    }
}
