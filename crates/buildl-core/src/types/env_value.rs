//! An environment variable's value, validated at construction.
//!
//! Its own file because a value is not a name: a name has a grammar a shell imposes, a value is
//! free text, and the two reach an action key as the two halves of one entry.
//!
//! Responsibilities: [`EnvValue`], its [`EnvValue::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: the environment. Nothing here reads a variable; a value arrives through
//! a port.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// The value of an environment variable, such as `/usr/bin:/bin`.
///
/// Any UTF-8 text without a `NUL` byte, the empty string included: a variable set to nothing is
/// not an unset variable, and an environment block cannot carry `NUL`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct EnvValue(String);

impl EnvValue {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` holds a `NUL` byte.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::text(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::EnvValue,
                value: raw,
                reason,
            }),
        }
    }

    /// Wraps a literal without checking it.
    ///
    /// For literals a test proves valid; anything read at run time goes through
    /// [`EnvValue::parse`].
    pub(crate) fn from_literal(raw: &'static str) -> Self {
        Self(raw.to_owned())
    }

    /// The value as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EnvValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EnvValue {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for EnvValue {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<EnvValue> for String {
    fn from(value: EnvValue) -> Self {
        value.0
    }
}

impl TryFrom<String> for EnvValue {
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

    use super::EnvValue;

    #[test]
    fn env_value_accepts_any_text_without_a_nul_byte() {
        for raw in ["", "/usr/bin:/bin", "a b", "ünïcödé", "x=y"] {
            assert!(EnvValue::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn env_value_rejects_a_nul_byte() {
        let err = EnvValue::parse("a\0b").unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid environment variable value: "a\0b" — must not contain a NUL byte"#
        );
    }

    #[test]
    fn env_value_round_trips_through_json_as_a_string() {
        let value = EnvValue::parse("/usr/bin:/bin").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""/usr/bin:/bin""#);
        assert_eq!(serde_json::from_str::<EnvValue>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "/usr/bin:/bin");
        assert_eq!(value.as_str(), "/usr/bin:/bin");
        assert!(serde_json::from_str::<EnvValue>(r#""a\u0000b""#).is_err());
    }

    #[test]
    fn env_value_from_str_routes_through_parse() {
        assert_eq!("".parse::<EnvValue>().unwrap().as_str(), "");
        assert!("a\0b".parse::<EnvValue>().is_err());
    }
}
