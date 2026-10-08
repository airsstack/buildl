//! A path inside a target's output directory, validated at construction.
//!
//! Its own file because an output is named relative to the target that produces it, not to the
//! workspace: the same grammar as a source path, a different concept.
//!
//! Responsibilities: [`OutputName`], its [`OutputName::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: where the output directory is. The executor decides that.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// A path inside a target's output directory, such as `app` or `bin/app`.
///
/// Valid names follow the source-path grammar: non-empty, at most 1024 bytes, `/`-separated
/// non-empty segments of ASCII letters, digits, `.`, `-` and `_`, none of which is `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct OutputName(String);

impl OutputName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 1024 bytes, starts or ends with `/`, holds an
    /// empty, `.` or `..` segment, or holds a character outside ASCII alphanumerics, `.`, `-`
    /// and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::path(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::OutputName,
                value: raw,
                reason,
            }),
        }
    }

    /// The value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for OutputName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for OutputName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<OutputName> for String {
    fn from(value: OutputName) -> Self {
        value.0
    }
}

impl TryFrom<String> for OutputName {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::OutputName;

    #[test]
    fn output_name_accepts_the_documented_forms() {
        for raw in ["app", "bin/app", "main.o"] {
            assert!(OutputName::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn output_name_rejects_the_documented_forms() {
        for raw in ["", "/app", "../app", "a b"] {
            assert!(
                OutputName::parse(raw).is_err(),
                "{raw:?} should be rejected"
            );
        }
    }

    #[test]
    fn output_name_round_trips_through_json_as_a_string() {
        let value = OutputName::parse("bin/app").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<OutputName>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "bin/app");
        assert!(serde_json::from_str::<OutputName>("\"/app\"").is_err());
    }

    #[test]
    fn output_name_from_str_routes_through_parse() {
        assert_eq!("bin/app".parse::<OutputName>().unwrap().as_str(), "bin/app");
        assert!("".parse::<OutputName>().is_err());
    }
}
