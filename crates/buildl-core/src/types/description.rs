//! A rule's one-line description, validated at construction.
//!
//! Its own file because a description is shown on the status line while an action runs, so it
//! must fit on one line.
//!
//! Responsibilities: [`Description`], its [`Description::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: placeholder expansion. A `$in` inside a description stays text here.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// Longest accepted description, in bytes.
const MAX_LEN: usize = 1024;

/// One status line, returning the reason a candidate is rejected.
fn description_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.len() > MAX_LEN {
        return Err("must be at most 1024 bytes");
    }
    if raw.chars().any(char::is_control) {
        return Err("must be one line with no control characters");
    }
    Ok(())
}

/// A rule's description, such as `compile $in`.
///
/// Any UTF-8 text of at most 1024 bytes with no control character, the empty string included.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Description(String);

impl Description {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` exceeds 1024 bytes or holds a control character such as a newline.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match description_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::Description,
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

impl fmt::Display for Description {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Description {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Description {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Description> for String {
    fn from(value: Description) -> Self {
        value.0
    }
}

impl TryFrom<String> for Description {
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

    use super::Description;

    #[test]
    fn description_accepts_the_documented_forms() {
        for raw in ["", "compile $in", "link → app"] {
            assert!(Description::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn description_rejects_the_documented_forms() {
        for raw in ["a\nb", "a\tb", "a\0b"] {
            assert!(
                Description::parse(raw).is_err(),
                "{raw:?} should be rejected"
            );
        }
        assert!(Description::parse("a".repeat(1025)).is_err());
        assert!(Description::parse("a".repeat(1024)).is_ok());
    }

    #[test]
    fn description_round_trips_through_json_as_a_string() {
        let value = Description::parse("compile $in").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Description>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "compile $in");
        assert!(serde_json::from_str::<Description>("\"a\\nb\"").is_err());
    }

    #[test]
    fn description_from_str_routes_through_parse() {
        assert_eq!(
            "compile $in".parse::<Description>().unwrap().as_str(),
            "compile $in"
        );
        assert!("a\nb".parse::<Description>().is_err());
    }
}
