//! A workspace-relative path to a source file, validated at construction.
//!
//! Its own file because a source file is a different concept from a directory even though both
//! are written in the same characters: a source path is never the workspace root.
//!
//! Responsibilities: [`SourcePath`], its [`SourcePath::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: the filesystem. A `SourcePath` names a file; whether it exists is an
//! adapter's question.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

/// A workspace-relative path to a file a target reads, such as `src/main.c`.
///
/// Valid paths are non-empty, at most 1024 bytes, hold no leading or trailing `/`, and consist of
/// `/`-separated non-empty segments of ASCII letters, digits, `.`, `-` and `_`, none of which is
/// `.` or `..` — so a source path can never leave the workspace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct SourcePath(String);

impl SourcePath {
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
                kind: NameKind::SourcePath,
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

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SourcePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for SourcePath {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<SourcePath> for String {
    fn from(value: SourcePath) -> Self {
        value.0
    }
}

impl TryFrom<String> for SourcePath {
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

    use super::SourcePath;

    #[test]
    fn source_path_accepts_the_documented_forms() {
        for raw in ["src/main.c", "lib/text/a-b_c.d", "main.o"] {
            assert!(SourcePath::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn source_path_rejects_the_documented_forms() {
        for raw in ["", "/etc/passwd", "../x", "src/../x", "a b", "src/"] {
            assert!(
                SourcePath::parse(raw).is_err(),
                "{raw:?} should be rejected"
            );
        }
    }

    #[test]
    fn source_path_round_trips_through_json_as_a_string() {
        let value = SourcePath::parse("src/main.c").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<SourcePath>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "src/main.c");
        assert!(serde_json::from_str::<SourcePath>("\"../x\"").is_err());
    }

    #[test]
    fn source_path_from_str_routes_through_parse() {
        assert_eq!(
            "src/main.c".parse::<SourcePath>().unwrap().as_str(),
            "src/main.c"
        );
        assert!("".parse::<SourcePath>().is_err());
    }
}
