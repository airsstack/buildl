//! An environment variable name, validated at construction.
//!
//! Its own file because an environment variable a target reads is part of its action key and is
//! later checked against the workspace ceiling, so the name must be well formed before either.
//!
//! Responsibilities: [`EnvName`], its [`EnvName::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: the environment. Nothing here reads a variable's value.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// Longest accepted name, in bytes.
const MAX_LEN: usize = 256;

/// The portable shell-variable grammar, returning the reason a candidate is rejected.
fn env_name_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    let Some(first) = raw.bytes().next() else {
        return Err("must not be empty");
    };
    if raw.len() > MAX_LEN {
        return Err("must be at most 256 bytes");
    }
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return Err("must start with an ASCII letter or '_'");
    }
    if !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("may hold only ASCII letters, digits and '_'");
    }
    Ok(())
}

/// An environment variable name, such as `PATH` or `GOCACHE`.
///
/// Valid names are 1 to 256 bytes, start with an ASCII letter or `_`, and hold only ASCII letters,
/// digits and `_` — the portable shell-variable grammar.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct EnvName(String);

impl EnvName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, starts with a character other than
    /// an ASCII letter or `_`, or holds a character outside ASCII alphanumerics and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match env_name_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::EnvName,
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

impl fmt::Display for EnvName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EnvName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for EnvName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<EnvName> for String {
    fn from(value: EnvName) -> Self {
        value.0
    }
}

impl TryFrom<String> for EnvName {
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

    use super::EnvName;

    #[test]
    fn env_name_accepts_the_documented_forms() {
        for raw in ["PATH", "GOCACHE", "_x1", "cargo_home"] {
            assert!(EnvName::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn env_name_rejects_the_documented_forms() {
        for raw in ["", "1PATH", "A-B", "A B", "A.B"] {
            assert!(EnvName::parse(raw).is_err(), "{raw:?} should be rejected");
        }
        assert!(EnvName::parse("A".repeat(257)).is_err());
        assert!(EnvName::parse("A".repeat(256)).is_ok());
    }

    #[test]
    fn env_name_round_trips_through_json_as_a_string() {
        let value = EnvName::parse("GOCACHE").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<EnvName>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "GOCACHE");
        assert!(serde_json::from_str::<EnvName>("\"1PATH\"").is_err());
    }

    #[test]
    fn env_name_from_str_routes_through_parse() {
        assert_eq!("GOCACHE".parse::<EnvName>().unwrap().as_str(), "GOCACHE");
        assert!("".parse::<EnvName>().is_err());
    }
}
