//! The file name of every directory's build file, validated at construction.
//!
//! Its own file because the entry name is a file name, not a target name, even though the two
//! share a grammar: it is joined to a directory to locate a build file.
//!
//! Responsibilities: [`EntryName`], its [`EntryName::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: choosing it. The workspace manifest names it; this type only checks it.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// The file name a build file has in every directory, such as `build.lua`.
///
/// Valid names are non-empty, at most 256 bytes, are neither `.` nor `..`, and hold only ASCII
/// letters, digits, `.`, `-` and `_` — so an entry name can never point into another directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct EntryName(String);

impl EntryName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`, or holds a character
    /// outside ASCII alphanumerics, `.`, `-` and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::name(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::EntryName,
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

impl fmt::Display for EntryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EntryName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for EntryName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<EntryName> for String {
    fn from(value: EntryName) -> Self {
        value.0
    }
}

impl TryFrom<String> for EntryName {
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

    use super::EntryName;

    #[test]
    fn entry_name_accepts_the_documented_forms() {
        for raw in ["build.lua", "BUILD", "build-file.lua"] {
            assert!(EntryName::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn entry_name_rejects_the_documented_forms() {
        for raw in ["", "lib/build.lua", "..", "a b"] {
            assert!(EntryName::parse(raw).is_err(), "{raw:?} should be rejected");
        }
    }

    #[test]
    fn entry_name_round_trips_through_json_as_a_string() {
        let value = EntryName::parse("build.lua").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<EntryName>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "build.lua");
        assert!(serde_json::from_str::<EntryName>("\"lib/build.lua\"").is_err());
    }

    #[test]
    fn entry_name_from_str_routes_through_parse() {
        assert_eq!(
            "build.lua".parse::<EntryName>().unwrap().as_str(),
            "build.lua"
        );
        assert!("".parse::<EntryName>().is_err());
    }
}
