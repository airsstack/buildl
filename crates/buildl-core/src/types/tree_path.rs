//! The path of one file inside a directory an action left as an output.
//!
//! Its own file because such a path is named by the action, not by a build file: a tool may
//! write `@scope/pkg/index.js` or a name with a space in it, so the grammar of a declared path
//! is too narrow for it. What it must still never do is leave the directory it is under.
//!
//! Responsibilities: [`TreePath`], its [`TreePath::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: whether two paths of one tree collide, which needs the whole tree; and
//! the filesystem.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// The path of one file below a directory output, such as `assets/app.js`.
///
/// Valid paths are non-empty text with no `NUL`, made of `/`-separated segments none of which is
/// empty, `.` or `..` — so a path never starts or ends with `/` and can never leave the
/// directory it is under. Paths order by their bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct TreePath(String);

/// Why `raw` is not a path below a directory, or `Ok` when it is one.
fn check(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if raw.contains('\0') {
        return Err("must not contain a NUL byte");
    }
    for segment in raw.split('/') {
        if segment.is_empty() {
            return Err("must not contain an empty path segment");
        }
        if segment == "." || segment == ".." {
            return Err("must not contain a '.' or '..' segment");
        }
    }
    Ok(())
}

impl TreePath {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds a `NUL`, starts or ends with
    /// `/`, or holds an empty, `.` or `..` segment.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match check(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::TreePath,
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

impl fmt::Display for TreePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for TreePath {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<TreePath> for String {
    fn from(value: TreePath) -> Self {
        value.0
    }
}

impl TryFrom<String> for TreePath {
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

    use super::TreePath;
    use crate::error::{Error, NameKind};

    #[test]
    fn accepts_the_names_a_tool_writes() {
        for raw in [
            "index.html",
            "assets/app.js",
            "@scope/pkg/index.js",
            "My Documents/naïve résumé.txt",
            ".bin/tsc",
            "a..b/...",
            "back\\slash",
        ] {
            assert_eq!(TreePath::parse(raw).unwrap().as_str(), raw);
        }
    }

    #[test]
    fn rejects_every_path_that_is_not_below_its_directory() {
        for (raw, reason) in [
            ("", "must not be empty"),
            ("a\0b", "must not contain a NUL byte"),
            ("/etc/passwd", "must not contain an empty path segment"),
            ("a/", "must not contain an empty path segment"),
            ("a//b", "must not contain an empty path segment"),
            ("..", "must not contain a '.' or '..' segment"),
            ("../sibling", "must not contain a '.' or '..' segment"),
            ("a/../../b", "must not contain a '.' or '..' segment"),
            ("a/./b", "must not contain a '.' or '..' segment"),
        ] {
            match TreePath::parse(raw).unwrap_err() {
                Error::InvalidName {
                    kind,
                    value,
                    reason: found,
                } => {
                    assert_eq!(kind, NameKind::TreePath, "{raw:?}");
                    assert_eq!(value, raw);
                    assert_eq!(found, reason, "{raw:?}");
                }
                other => unreachable!("expected InvalidName, got {other:?}"),
            }
        }
    }

    #[test]
    fn displays_as_its_text() {
        assert_eq!(
            TreePath::parse("assets/app.js").unwrap().to_string(),
            "assets/app.js"
        );
    }

    #[test]
    fn orders_by_its_bytes() {
        let mut paths: Vec<TreePath> = ["b", "a/b", "a-x", "a", "B"]
            .into_iter()
            .map(|raw| TreePath::parse(raw).unwrap())
            .collect();
        paths.sort();
        let sorted: Vec<&str> = paths.iter().map(TreePath::as_str).collect();
        assert_eq!(sorted, ["B", "a", "a-x", "a/b", "b"]);
    }

    #[test]
    fn round_trips_through_json_as_a_string() {
        let value = TreePath::parse("assets/app.js").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""assets/app.js""#);
        assert_eq!(serde_json::from_str::<TreePath>(&json).unwrap(), value);
        assert!(serde_json::from_str::<TreePath>(r#""../app.js""#).is_err());
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!("a/b".parse::<TreePath>().unwrap().as_str(), "a/b");
        assert!("a//b".parse::<TreePath>().is_err());
    }
}
