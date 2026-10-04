//! A workspace-relative directory path, validated at construction.
//!
//! Its own file because a label's directory half has its own grammar, independent of the name
//! half it is paired with. Parsing it once here means the label never re-checks it.
//!
//! Responsibilities: [`Directory`], its [`Directory::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: the filesystem. A `Directory` is a name, and nothing here asks whether
//! the directory exists — that question belongs to a storage adapter.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// A workspace-relative directory path, such as `lib` or `lib/text`.
///
/// The empty string is the workspace root. Valid paths are at most 1024 bytes, hold no leading
/// or trailing `/`, and consist of `/`-separated non-empty segments of ASCII letters, digits,
/// `.`, `-` and `_`, none of which is `.` or `..`. The character set deliberately excludes
/// whitespace and `:`: the first would be a quoting hazard in the executor's argument
/// expansion, and the second is a label's one unambiguous split point.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Directory(String);

impl Directory {
    /// Longest accepted path, in bytes.
    const MAX_LEN: usize = 1024;

    /// The workspace root.
    #[must_use]
    pub const fn root() -> Self {
        Self(String::new())
    }

    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` exceeds 1024 bytes, starts or ends with `/`,
    /// holds an empty, `.` or `..` segment, or holds a character outside ASCII alphanumerics,
    /// `.`, `-` and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        let invalid = |reason: &'static str| Error::InvalidName {
            kind: NameKind::Directory,
            value: raw.clone(),
            reason,
        };

        if raw.len() > Self::MAX_LEN {
            return Err(invalid("must be at most 1024 bytes"));
        }
        if raw.is_empty() {
            return Ok(Self(raw));
        }
        if raw.starts_with('/') || raw.ends_with('/') {
            return Err(invalid("must not start or end with '/'"));
        }
        for segment in raw.split('/') {
            if segment.is_empty() {
                return Err(invalid("must not contain an empty path segment"));
            }
            if segment == "." || segment == ".." {
                return Err(invalid("must not contain a '.' or '..' segment"));
            }
            if !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
            {
                return Err(invalid(
                    "segments may hold only ASCII letters, digits, '.', '-' and '_'",
                ));
            }
        }
        Ok(Self(raw))
    }

    /// The path as a string slice; empty for the workspace root.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the workspace root.
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Directory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Directory {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Directory {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Directory> for String {
    fn from(value: Directory) -> Self {
        value.0
    }
}

impl TryFrom<String> for Directory {
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

    use super::Directory;

    #[test]
    fn accepts_the_documented_forms() {
        for raw in ["", "lib", "lib/text", "a-b_c.d"] {
            assert!(Directory::parse(raw).is_ok(), "{raw} should parse");
        }
    }

    #[test]
    fn rejects_the_documented_forms() {
        for raw in [
            "/lib",
            "lib/",
            "lib//text",
            ".",
            "..",
            "lib/../x",
            "lib:x",
            "a b",
        ] {
            assert!(Directory::parse(raw).is_err(), "{raw} should be rejected");
        }
        assert!(Directory::parse("a".repeat(1025)).is_err());
    }

    #[test]
    fn the_empty_path_is_the_workspace_root() {
        let root = Directory::parse("").unwrap();
        assert!(root.is_root());
        assert_eq!(root, Directory::root());
        assert_eq!(root.as_str(), "");
        assert!(!Directory::parse("lib").unwrap().is_root());
    }

    #[test]
    fn round_trips_through_json_as_a_string() {
        let dir = Directory::parse("lib/text").unwrap();
        let json = serde_json::to_string(&dir).unwrap();
        assert_eq!(json, r#""lib/text""#);
        assert_eq!(serde_json::from_str::<Directory>(&json).unwrap(), dir);
        assert!(serde_json::from_str::<Directory>(r#""/lib""#).is_err());
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!(
            "lib/text".parse::<Directory>().unwrap().as_str(),
            "lib/text"
        );
        assert!("/lib".parse::<Directory>().is_err());
    }
}
