//! The name of a field in a declaration's option table, validated at construction.
//!
//! Its own file because a field name appears in an evaluation error — an unknown field, or a field
//! holding the wrong type — where it is reported back to the author of the build file.
//!
//! Responsibilities: [`FieldName`], its [`FieldName::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: knowing which fields exist. The adapter that reads the option table
//! decides that.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// A field name in a declaration's option table, such as `deps` or `outputs`.
///
/// Valid names are 1 to 256 bytes of ASCII letters, digits, `-` and `_`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct FieldName(String);

impl FieldName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, or holds a character outside ASCII
    /// alphanumerics, `-` and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::key(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::FieldName,
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

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for FieldName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for FieldName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<FieldName> for String {
    fn from(value: FieldName) -> Self {
        value.0
    }
}

impl TryFrom<String> for FieldName {
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

    use super::FieldName;

    #[test]
    fn field_name_accepts_the_documented_forms() {
        for raw in ["deps", "outputs", "always"] {
            assert!(FieldName::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn field_name_rejects_the_documented_forms() {
        for raw in ["", "a.b", "a b"] {
            assert!(FieldName::parse(raw).is_err(), "{raw:?} should be rejected");
        }
    }

    #[test]
    fn field_name_round_trips_through_json_as_a_string() {
        let value = FieldName::parse("deps").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<FieldName>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "deps");
        assert!(serde_json::from_str::<FieldName>("\"a.b\"").is_err());
    }

    #[test]
    fn field_name_from_str_routes_through_parse() {
        assert_eq!("deps".parse::<FieldName>().unwrap().as_str(), "deps");
        assert!("".parse::<FieldName>().is_err());
    }
}
