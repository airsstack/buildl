//! How a declared environment variable takes part in an action key, validated at construction.
//!
//! Its own file because the mark is a closed set a build file names by a word, and every phase
//! that reads a declared variable carries it beside the name.
//!
//! Responsibilities: [`EnvMark`], its [`EnvMark::parse`] constructor and its rendering.
//!
//! Non-responsibilities: what a mark does to a key or to a remote action. The phases that build
//! those read the mark; this is only its name.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// How a declared environment variable takes part in an action key.
///
/// # Examples
///
/// ```
/// use buildl_core::EnvMark;
///
/// assert_eq!(EnvMark::parse("key")?, EnvMark::Key);
/// assert_eq!(EnvMark::parse("unkeyed")?, EnvMark::Unkeyed);
/// assert_eq!(EnvMark::Unkeyed.to_string(), "unkeyed");
/// assert!(EnvMark::parse("pass").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvMark {
    /// `key`: the command is given the value, and the key holds the name and the value.
    Key,
    /// `unkeyed`: the command is given the value, and the key holds the name only.
    Unkeyed,
}

impl EnvMark {
    /// Every mark.
    const ALL: [Self; 2] = [Self::Key, Self::Unkeyed];

    /// The mark the word `raw` names.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not exactly `key` or `unkeyed`.
    pub fn parse(raw: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|mark| mark.as_str() == raw)
            .ok_or_else(|| Error::InvalidName {
                kind: NameKind::EnvMark,
                value: raw.to_owned(),
                reason: "must be one of key and unkeyed",
            })
    }

    /// The word a build file names this mark by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Unkeyed => "unkeyed",
        }
    }
}

impl fmt::Display for EnvMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EnvMark {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::EnvMark;
    use crate::error::{Error, NameKind};

    #[test]
    fn env_mark_parses_key_and_unkeyed() {
        assert_eq!(EnvMark::parse("key").unwrap(), EnvMark::Key);
        assert_eq!(EnvMark::parse("unkeyed").unwrap(), EnvMark::Unkeyed);
        assert_eq!("key".parse::<EnvMark>().unwrap(), EnvMark::Key);
        assert_eq!(EnvMark::Key.to_string(), "key");
        assert_eq!(EnvMark::Unkeyed.to_string(), "unkeyed");
        for raw in ["pass", "", "Key", "UNKEYED", " key"] {
            assert!(
                matches!(
                    EnvMark::parse(raw),
                    Err(Error::InvalidName {
                        kind: NameKind::EnvMark,
                        ..
                    })
                ),
                "{raw:?}"
            );
        }
    }
}
