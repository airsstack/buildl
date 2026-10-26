//! A workspace-relative path as a file event named it.
//!
//! Its own file, and not a [`SourcePath`](crate::types::SourcePath), a
//! [`Directory`](crate::types::Directory) or a [`TreePath`](crate::types::TreePath), because it
//! answers a different question. A source path is a path a build file declared, held to the
//! narrow grammar a declaration may use. A path a file event names is whatever the file system
//! holds: an editor's swap file (`.#a.c`, `a.c~`) or a README with a space in its name must still
//! reach the watcher, which sends it to a check pass, so the declared grammar would refuse paths
//! that matter. A tree path is named by an action below one of its outputs; this one is named by
//! the workspace, relative to its root.
//!
//! Responsibilities: [`ChangedPath`], its [`ChangedPath::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: the filesystem, and whether the path matters to any target.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// A workspace-relative path a file event named, such as `src/main.c` or `docs/read me.md`.
///
/// Valid paths are non-empty text made of `/`-separated segments none of which is empty, `.` or
/// `..` — so a path never starts or ends with `/` and can never leave the workspace. Any other
/// character is allowed. Paths order by their bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct ChangedPath(String);

/// Why `raw` is not a workspace-relative path, or `Ok` when it is one.
fn check(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
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

impl ChangedPath {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, starts or ends with `/`, or holds an
    /// empty, `.` or `..` segment.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match check(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::ChangedPath,
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

impl fmt::Display for ChangedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ChangedPath {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<ChangedPath> for String {
    fn from(value: ChangedPath) -> Self {
        value.0
    }
}

impl TryFrom<String> for ChangedPath {
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

    use super::ChangedPath;
    use crate::error::{Error, NameKind};

    #[test]
    fn a_relative_path_with_a_space_or_tilde_is_accepted() {
        for raw in ["docs/read me.md", "a.c~", ".#a.c"] {
            assert_eq!(ChangedPath::parse(raw).unwrap().as_str(), raw);
        }
    }

    #[test]
    fn an_absolute_empty_dotted_or_doubled_path_is_refused() {
        for raw in ["/a", "", "a/../b", "./a", "a//b", "a/"] {
            assert!(
                matches!(
                    ChangedPath::parse(raw),
                    Err(Error::InvalidName {
                        kind: NameKind::ChangedPath,
                        ..
                    })
                ),
                "{raw:?} should be refused"
            );
        }
    }

    #[test]
    fn a_changed_path_round_trips_through_json_as_a_string() {
        let value = ChangedPath::parse("docs/read me.md").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<ChangedPath>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "docs/read me.md");
        assert!(serde_json::from_str::<ChangedPath>("\"../x\"").is_err());
    }

    #[test]
    fn a_changed_path_from_str_routes_through_parse() {
        assert_eq!("a/b".parse::<ChangedPath>().unwrap().as_str(), "a/b");
        assert!("".parse::<ChangedPath>().is_err());
    }
}
