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
//! - [`DeclaredKind`] — what a label an [`Error::WrongReferenceKind`] is about was declared as.
//! - [`DeclarationSite`] — a declared label and its build file: the declaration holding a bad
//!   reference, or one target on a dependency cycle.
//! - [`Result`] — the crate-wide alias.
//!
//! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic, on one line
//! unless an adapter's own message spans several; how a
//! command surfaces it belongs to the `Reporter` port.

use core::fmt;

use crate::types::{
    Declaration, DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance,
    SettingName, Written,
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
    /// A label was declared more than once, as any mix of target, rule and alias.
    #[error("{label} is declared more than once: {}", sites_list(.sites))]
    DuplicateLabel {
        /// The label.
        label: Label,
        /// The build file of every declaration of it, in the declarations' own order.
        sites: Vec<Provenance>,
    },
    /// A build setting was declared more than once.
    #[error("setting {name} is declared more than once: {}", sites_list(.sites))]
    DuplicateSetting {
        /// The setting.
        name: SettingName,
        /// The build file of every declaration of it, in the declarations' own order.
        sites: Vec<Provenance>,
    },
    /// A reference names a label nothing declares.
    #[error(
        "{}: {}: {field} {reference} is not declared{}",
        .site.provenance,
        .site.label,
        suggestion_suffix(.suggestion.as_ref())
    )]
    UnknownReference {
        /// The declaration holding the reference, and its build file.
        site: Box<DeclarationSite>,
        /// Which field holds it: a dependency, a rule reference or an alias's target.
        field: DeclarationField,
        /// The label that was named.
        reference: Label,
        /// The nearest declared label the field could have named, if one is near enough.
        suggestion: Option<Label>,
    },
    /// A reference names a declaration of a kind its field cannot name.
    #[error(
        "{}: {}: {field} {reference} names {}, declared in {declared_in}",
        .site.provenance,
        .site.label,
        .found.with_article()
    )]
    WrongReferenceKind {
        /// The declaration holding the reference, and its build file.
        site: Box<DeclarationSite>,
        /// Which field holds it: a dependency, a rule reference or an alias's target.
        field: DeclarationField,
        /// The label that was named.
        reference: Label,
        /// What that label was declared as.
        found: DeclaredKind,
        /// The build file that declared it.
        declared_in: Provenance,
    },
    /// A command references a build setting nothing declares.
    #[error(
        "{}: {}: setting {name} is not declared{}",
        .site.provenance,
        .site.label,
        suggestion_suffix(.suggestion.as_ref())
    )]
    UnknownSetting {
        /// The target or rule whose command holds the reference, and its build file.
        site: Box<DeclarationSite>,
        /// The name after `$opt:`, as written.
        name: String,
        /// The nearest declared setting, if one is near enough.
        suggestion: Option<SettingName>,
    },
    /// The targets' dependencies form a cycle.
    #[error("dependency cycle: {}", cycle_path(.path))]
    DependencyCycle {
        /// The targets on the cycle, lowest label first; each depends on the next, and the last
        /// on the first.
        path: Vec<DeclarationSite>,
    },
    /// The workspace declares more targets than the graph has ids for.
    #[error("{count} targets are more than a graph can number")]
    TooManyTargets {
        /// How many targets were declared.
        count: usize,
    },
}

/// The suffix naming who required a missing build file.
fn requested_by_suffix(requested_by: Option<&Provenance>) -> String {
    requested_by.map_or_else(
        || " (the workspace root)".to_owned(),
        |provenance| format!(" (requested by {provenance})"),
    )
}

/// The build files of a duplicated name, separated by commas.
fn sites_list(sites: &[Provenance]) -> String {
    let rendered: Vec<String> = sites.iter().map(ToString::to_string).collect();
    rendered.join(", ")
}

/// The suffix offering the nearest declared name, when there is one.
fn suggestion_suffix<T: fmt::Display>(suggestion: Option<&T>) -> String {
    suggestion.map_or_else(String::new, |nearest| format!(" (did you mean {nearest}?)"))
}

/// A cycle as its labels and build files, closed by repeating the first label.
fn cycle_path(path: &[DeclarationSite]) -> String {
    let mut rendered: Vec<String> = path
        .iter()
        .map(|site| format!("{} ({})", site.label, site.provenance))
        .collect();
    if let Some(first) = path.first() {
        rendered.push(first.label.to_string());
    }
    rendered.join(" -> ")
}

/// What a label was declared as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclaredKind {
    /// A build or test target.
    Target,
    /// A rule.
    Rule,
    /// An alias.
    Alias,
}

impl DeclaredKind {
    /// The kind with its indefinite article, for a sentence.
    const fn with_article(self) -> &'static str {
        match self {
            Self::Target => "a target",
            Self::Rule => "a rule",
            Self::Alias => "an alias",
        }
    }
}

impl fmt::Display for DeclaredKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Target => "target",
            Self::Rule => "rule",
            Self::Alias => "alias",
        })
    }
}

/// A declared label and the build file that declared it.
///
/// Names the declaration an error is about: the one holding a bad reference, or one target on
/// a dependency cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationSite {
    /// The declared label.
    pub label: Label,
    /// The build file that declared it.
    pub provenance: Provenance,
}

/// A ceiling an evaluation can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluationLimit {
    /// The instruction ceiling.
    Instructions,
    /// The memory ceiling.
    Memory,
    /// The byte budget on what one file may stage: the in-memory size of each staged record plus
    /// the byte length of its text, up to the memory ceiling.
    Staging,
    /// The cap on directory entries the host-side source walk may visit for one build file.
    Walk,
}

impl fmt::Display for EvaluationLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instructions => "instruction ceiling",
            Self::Memory => "memory ceiling",
            Self::Staging => "staging cap",
            Self::Walk => "walk cap",
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
        ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
        EvaluationLimit, NameKind,
    };
    use crate::types::{
        DeclarationOrder, Diagnostic, Directory, FieldName, Label, Provenance, SettingName,
        TargetName, Written,
    };

    fn lib_file() -> Provenance {
        Provenance::new(
            PathBuf::from("lib/build.lua"),
            Directory::parse("lib").unwrap(),
        )
    }

    fn root_file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::root())
    }

    fn tools_file() -> Provenance {
        Provenance::new(
            PathBuf::from("tools/build.lua"),
            Directory::parse("tools").unwrap(),
        )
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// The declaration `raw`, declared in the root build file.
    fn root_site(raw: &str) -> DeclarationSite {
        DeclarationSite {
            label: label(raw),
            provenance: root_file(),
        }
    }

    #[test]
    fn duplicate_label_names_every_site() {
        let err = Error::DuplicateLabel {
            label: label("//:a"),
            sites: vec![root_file(), root_file()],
        };
        assert_eq!(
            err.to_string(),
            "//:a is declared more than once: build.lua, build.lua"
        );
    }

    #[test]
    fn duplicate_setting_names_every_site() {
        let err = Error::DuplicateSetting {
            name: SettingName::parse("test_filter").unwrap(),
            sites: vec![root_file(), lib_file()],
        };
        assert_eq!(
            err.to_string(),
            "setting test_filter is declared more than once: build.lua, lib/build.lua"
        );
    }

    #[test]
    fn unknown_reference_names_the_site_the_field_and_the_nearest_label() {
        let err = Error::UnknownReference {
            site: Box::new(root_site("//:app")),
            field: DeclarationField::Dep,
            reference: label("//:mian.o"),
            suggestion: Some(label("//:main.o")),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: dep //:mian.o is not declared (did you mean //:main.o?)"
        );
    }

    #[test]
    fn unknown_reference_without_a_near_label_offers_nothing() {
        let err = Error::UnknownReference {
            site: Box::new(root_site("//:default")),
            field: DeclarationField::Target,
            reference: label("//:nope"),
            suggestion: None,
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:default: target //:nope is not declared"
        );
    }

    #[test]
    fn wrong_reference_kind_names_what_was_found_and_where() {
        let err = Error::WrongReferenceKind {
            site: Box::new(root_site("//:app")),
            field: DeclarationField::Dep,
            reference: label("//:cc"),
            found: DeclaredKind::Rule,
            declared_in: tools_file(),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:app: dep //:cc names a rule, declared in tools/build.lua"
        );
    }

    #[test]
    fn every_declared_kind_renders_alone_and_in_a_sentence() {
        let cases = [
            (DeclaredKind::Target, "target", "names a target,"),
            (DeclaredKind::Rule, "rule", "names a rule,"),
            (DeclaredKind::Alias, "alias", "names an alias,"),
        ];
        for (found, alone, in_a_sentence) in cases {
            assert_eq!(found.to_string(), alone);
            let err = Error::WrongReferenceKind {
                site: Box::new(root_site("//:app")),
                field: DeclarationField::Rule,
                reference: label("//:x"),
                found,
                declared_in: root_file(),
            };
            assert!(err.to_string().contains(in_a_sentence), "{err}");
        }
    }

    #[test]
    fn unknown_setting_names_the_site_and_the_nearest_setting() {
        let err = Error::UnknownSetting {
            site: Box::new(root_site("//:go_test")),
            name: "test_fliter".to_owned(),
            suggestion: Some(SettingName::parse("test_filter").unwrap()),
        };
        assert_eq!(
            err.to_string(),
            "build.lua: //:go_test: setting test_fliter is not declared \
             (did you mean test_filter?)"
        );
    }

    #[test]
    fn dependency_cycle_lists_every_target_and_closes_on_the_first() {
        let err = Error::DependencyCycle {
            path: vec![
                DeclarationSite {
                    label: label("//:a"),
                    provenance: root_file(),
                },
                DeclarationSite {
                    label: label("//lib:b"),
                    provenance: lib_file(),
                },
            ],
        };
        assert_eq!(
            err.to_string(),
            "dependency cycle: //:a (build.lua) -> //lib:b (lib/build.lua) -> //:a"
        );
    }

    #[test]
    fn a_cycle_of_one_names_its_target_twice() {
        let err = Error::DependencyCycle {
            path: vec![DeclarationSite {
                label: label("//:a"),
                provenance: root_file(),
            }],
        };
        assert_eq!(
            err.to_string(),
            "dependency cycle: //:a (build.lua) -> //:a"
        );
    }

    #[test]
    fn too_many_targets_states_the_count() {
        let err = Error::TooManyTargets { count: 7 };
        assert_eq!(
            err.to_string(),
            "7 targets are more than a graph can number"
        );
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
                EvaluationFailure::LimitReached {
                    limit: EvaluationLimit::Walk,
                },
                "walk cap reached",
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
