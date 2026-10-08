//! A build setting's name and value, each validated at construction.
//!
//! Its own file because the two halves of `b.option(name, { default })` are read together: the
//! name is written bare in `--set name=value` and inside `$opt:name`, and the value is substituted
//! into an argument vector.
//!
//! Responsibilities: [`SettingName`] and [`SettingValue`], their constructors, and their
//! rendering.
//!
//! Non-responsibilities: whether a setting is declared. Matching a `$opt:name` reference to its
//! declaration needs every build file, so it belongs to the phase that builds the graph.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// A build setting's name, such as `test_filter`.
///
/// Valid names are 1 to 256 bytes of ASCII letters, digits, `-` and `_`. Excluding `.` and `:`
/// is what lets a `$opt:name` reference end cleanly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct SettingName(String);

impl SettingName {
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
                kind: NameKind::SettingName,
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

impl fmt::Display for SettingName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SettingName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for SettingName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<SettingName> for String {
    fn from(value: SettingName) -> Self {
        value.0
    }
}

impl TryFrom<String> for SettingName {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// A build setting's value, such as its declared default.
///
/// Any UTF-8 text without a `NUL` byte, the empty string included: the value is substituted into
/// an argument vector, which cannot carry `NUL`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct SettingValue(String);

impl SettingValue {
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
                kind: NameKind::SettingValue,
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

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SettingValue {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for SettingValue {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<SettingValue> for String {
    fn from(value: SettingValue) -> Self {
        value.0
    }
}

impl TryFrom<String> for SettingValue {
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

    use super::{SettingName, SettingValue};

    #[test]
    fn setting_name_accepts_the_documented_forms() {
        for raw in ["test_filter", "a-b", "X1"] {
            assert!(SettingName::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn setting_name_rejects_the_documented_forms() {
        for raw in ["", "a.b", "a:b", "a b"] {
            assert!(
                SettingName::parse(raw).is_err(),
                "{raw:?} should be rejected"
            );
        }
    }

    #[test]
    fn setting_name_round_trips_through_json_as_a_string() {
        let value = SettingName::parse("test_filter").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<SettingName>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "test_filter");
        assert!(serde_json::from_str::<SettingName>("\"a.b\"").is_err());
    }

    #[test]
    fn setting_name_from_str_routes_through_parse() {
        assert_eq!(
            "test_filter".parse::<SettingName>().unwrap().as_str(),
            "test_filter"
        );
        assert!("".parse::<SettingName>().is_err());
    }

    #[test]
    fn setting_value_accepts_the_documented_forms() {
        for raw in ["", "TestLogin", "a b c"] {
            assert!(SettingValue::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn setting_value_rejects_the_documented_forms() {
        assert!(SettingValue::parse("a\0b").is_err());
        assert!(SettingValue::parse("\0").is_err());
    }

    #[test]
    fn setting_value_round_trips_through_json_as_a_string() {
        let value = SettingValue::parse("TestLogin").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<SettingValue>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "TestLogin");
        assert!(serde_json::from_str::<SettingValue>("\"a\\u0000b\"").is_err());
    }

    #[test]
    fn setting_value_from_str_routes_through_parse() {
        assert_eq!(
            "TestLogin".parse::<SettingValue>().unwrap().as_str(),
            "TestLogin"
        );
        assert!("a\0b".parse::<SettingValue>().is_err());
    }
}
