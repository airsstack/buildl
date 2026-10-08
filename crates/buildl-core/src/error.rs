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
//! - [`Result`] — the crate-wide alias.
//!
//! Non-responsibilities: presentation. [`Error`]'s `Display` renders one diagnostic line; how a
//! command surfaces it belongs to the `Reporter` port.

use core::fmt;

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
    use super::{Error, NameKind};

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
}
