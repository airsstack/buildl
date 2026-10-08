//! The name half of a label, validated at construction.
//!
//! Its own file because the two halves of a label have different grammars and are validated
//! independently — resolving `:sibling` against a base directory checks this half alone.
//!
//! Responsibilities: [`TargetName`], its [`TargetName::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: uniqueness. A `TargetName` is well formed, not unclaimed; whether two
//! declarations fight over one belongs to the phase that builds the graph.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// The name half of a label, such as `app` or `main.o`.
///
/// Valid names are non-empty, at most 256 bytes, are neither `.` nor `..`, and hold only ASCII
/// letters, digits, `.`, `-` and `_`. Excluding `/` and `:` is what makes a label's single `:`
/// an unambiguous split point.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct TargetName(String);

impl TargetName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`,
    /// or holds a character outside ASCII alphanumerics, `.`, `-` and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::name(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::TargetName,
                value: raw,
                reason,
            }),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TargetName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for TargetName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<TargetName> for String {
    fn from(value: TargetName) -> Self {
        value.0
    }
}

impl TryFrom<String> for TargetName {
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

    use super::TargetName;

    #[test]
    fn accepts_every_name_the_design_declares() {
        for raw in [
            "app", "main.o", "util.o", "app_test", "default", "text", "a-b",
        ] {
            assert!(TargetName::parse(raw).is_ok(), "{raw} should parse");
        }
    }

    #[test]
    fn rejects_the_documented_forms() {
        for raw in ["", "a/b", "a:b", ".", "..", " x"] {
            assert!(TargetName::parse(raw).is_err(), "{raw} should be rejected");
        }
        assert!(TargetName::parse("a".repeat(257)).is_err());
    }

    #[test]
    fn round_trips_through_json_as_a_string() {
        let name = TargetName::parse("main.o").unwrap();
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, r#""main.o""#);
        assert_eq!(serde_json::from_str::<TargetName>(&json).unwrap(), name);
        assert!(serde_json::from_str::<TargetName>(r#""a/b""#).is_err());
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!("main.o".parse::<TargetName>().unwrap().as_str(), "main.o");
        assert!("a/b".parse::<TargetName>().is_err());
    }
}
