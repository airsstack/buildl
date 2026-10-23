//! Where a plugin comes from: a directory of the workspace, or a revision of a git repository.
//!
//! Its own file because a manifest reader produces these values and a store checker consumes
//! them, and neither may import the other. Each part is parsed once, where a manifest names it,
//! so no later phase re-checks a commit id or a tag.
//!
//! Responsibilities: [`PluginSource`], [`Rev`], [`GitUrl`], [`TagName`] and [`CommitId`], and the
//! text each is written as in `buildl.lock`.
//!
//! Non-responsibilities: telling a tag from a branch, which only the repository can, and
//! fetching anything.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::{Directory, grammar};

/// The length of a full commit id, in characters.
const COMMIT_LEN: usize = 40;

/// Whether `byte` is a lowercase hexadecimal digit.
const fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
}

/// Why `raw` is not a tag name, or `None` when it is one.
fn tag_fault(raw: &str) -> Option<&'static str> {
    if raw.is_empty() {
        Some("must not be empty")
    } else if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Some("must not hold whitespace or a control character")
    } else if raw.contains("..") {
        Some("must not contain '..'")
    } else if raw.starts_with('-') || raw.starts_with('/') {
        Some("must not start with '-' or '/'")
    } else if raw.ends_with('/') || raw.strip_suffix(".lock").is_some() {
        Some("must not end with '/' or '.lock'")
    } else {
        None
    }
}

/// Whether `raw` begins `<word>::`, which `git` reads as a transport helper to run.
fn is_helper(raw: &str) -> bool {
    raw.split_once("::").is_some_and(|(word, _)| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    })
}

/// A full git commit id: exactly 40 lowercase hexadecimal characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct CommitId(String);

impl CommitId {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not exactly 40 lowercase hexadecimal
    /// characters.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if raw.len() == COMMIT_LEN && raw.bytes().all(is_lower_hex) {
            Ok(Self(raw))
        } else {
            Err(Error::InvalidName {
                kind: NameKind::CommitId,
                value: raw,
                reason: "must be exactly 40 lowercase hexadecimal characters",
            })
        }
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CommitId> for String {
    fn from(value: CommitId) -> Self {
        value.0
    }
}

impl TryFrom<String> for CommitId {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// A git tag name, such as `v1.2.0`.
///
/// Holds the parts of git's ref-name rules a manifest can get wrong; git refuses the rest when
/// the tag is fetched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct TagName(String);

impl TagName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds whitespace, a control
    /// character or `..`, starts with `-` or `/`, or ends with `/` or `.lock`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match tag_fault(&raw) {
            None => Ok(Self(raw)),
            Some(reason) => Err(Error::InvalidName {
                kind: NameKind::TagName,
                value: raw,
                reason,
            }),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TagName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<TagName> for String {
    fn from(value: TagName) -> Self {
        value.0
    }
}

impl TryFrom<String> for TagName {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// The URL of a git repository, as `git` is given it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct GitUrl(String);

impl GitUrl {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, starts with `-`, names a git
    /// transport helper (`<word>::<address>`, which can make `git` run a program), or holds
    /// whitespace or a control character.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        let reason = if raw.is_empty() {
            Some("must not be empty")
        } else if raw.starts_with('-') {
            Some("must not start with '-'")
        } else if is_helper(&raw) {
            Some("must not name a transport helper ('<word>::')")
        } else if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
            Some("must not hold whitespace or a control character")
        } else {
            None
        };
        match reason {
            None => Ok(Self(raw)),
            Some(reason) => Err(Error::InvalidName {
                kind: NameKind::GitUrl,
                value: raw,
                reason,
            }),
        }
    }

    /// The URL of a GitHub repository written `owner/repo`: `https://github.com/owner/repo`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `short` is not exactly an owner and a repository
    /// name, each of ASCII letters, digits, `.`, `-` and `_`, separated by one `/`.
    pub fn github(short: &str) -> Result<Self> {
        let refused = |reason| Error::InvalidName {
            kind: NameKind::GitUrl,
            value: short.to_owned(),
            reason,
        };
        let (owner, repo) = short
            .split_once('/')
            .ok_or_else(|| refused("must be written owner/repo"))?;
        if repo.contains('/') {
            return Err(refused("must be written owner/repo"));
        }
        for part in [owner, repo] {
            grammar::name(part).map_err(refused)?;
        }
        Ok(Self(format!("https://github.com/{owner}/{repo}")))
    }

    /// The URL as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<GitUrl> for String {
    fn from(value: GitUrl) -> Self {
        value.0
    }
}

impl TryFrom<String> for GitUrl {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// The revision of a git repository a plugin is pinned to.
///
/// Serializes as the revision as written, so a tag stays a tag and a commit a commit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum Rev {
    /// A tag.
    Tag(TagName),
    /// A full commit.
    Commit(CommitId),
}

impl Rev {
    /// Reads a revision as written: forty hexadecimal characters are a commit, anything else is
    /// a tag.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is neither a [`CommitId`] nor a [`TagName`].
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.len() == COMMIT_LEN && raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            CommitId::parse(raw).map(Self::Commit)
        } else {
            TagName::parse(raw).map(Self::Tag)
        }
    }
}

/// The revision as written.
impl fmt::Display for Rev {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(tag) => tag.fmt(f),
            Self::Commit(commit) => commit.fmt(f),
        }
    }
}

impl From<Rev> for String {
    fn from(value: Rev) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for Rev {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(&raw)
    }
}

impl FromStr for Rev {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

/// Where a plugin's files come from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginSource {
    /// A directory of the workspace.
    Path(Directory),
    /// A revision of a git repository.
    Git {
        /// The repository.
        url: GitUrl,
        /// The tag or commit the plugin is pinned to.
        rev: Rev,
    },
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{CommitId, GitUrl, Rev, TagName};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn commit_id_accepts_40_hex_only() {
        assert_eq!(CommitId::parse(COMMIT).unwrap().as_str(), COMMIT);
        for bad in [
            "",
            "abc123",
            &COMMIT[..39],
            &format!("{COMMIT}0"),
            &COMMIT.to_uppercase(),
            &format!("{}g", &COMMIT[..39]),
        ] {
            assert!(CommitId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tag_name_refuses_dotdot_and_leading_dash() {
        for good in ["v1.2.0", "release/2024-1", "1", "a.b"] {
            assert_eq!(TagName::parse(good).unwrap().as_str(), good);
        }
        for bad in [
            "", "a..b", "..", "-v1", "/v1", "v1/", "v1.lock", "v 1", "v1\t", "v1\n", "v\u{7f}1",
        ] {
            assert!(TagName::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn github_short_form_expands() {
        assert_eq!(
            GitUrl::github("acme/buildl-go").unwrap().as_str(),
            "https://github.com/acme/buildl-go"
        );
        for bad in [
            "", "acme", "acme/", "/repo", "a/b/c", "a b/c", "a/..", "../b",
        ] {
            assert!(GitUrl::github(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            GitUrl::parse("file:///tmp/repo").unwrap().as_str(),
            "file:///tmp/repo"
        );
        assert!(GitUrl::parse("").is_err());
        assert!(GitUrl::parse("--upload-pack=x").is_err());
        for helper in [
            "ext::sh -c id",
            "ext::sh%20-c%20id",
            "fd::3",
            "git-remote+x::y",
        ] {
            assert!(GitUrl::parse(helper).is_err(), "{helper:?}");
        }
        for plain in ["ssh://[::1]/repo", "git@[::1]:repo", "https://host/a::b"] {
            assert!(GitUrl::parse(plain).is_ok(), "{plain:?}");
        }
        assert!(GitUrl::parse("https://x/y z").is_err());
    }

    #[test]
    fn the_values_serialize_as_their_text_and_read_back() {
        use crate::json::canonical;

        assert_eq!(
            canonical::to_string(&Rev::parse("v1.2.0").unwrap()).unwrap(),
            r#""v1.2.0""#
        );
        assert_eq!(
            canonical::to_string(&GitUrl::github("acme/buildl-go").unwrap()).unwrap(),
            r#""https://github.com/acme/buildl-go""#
        );
        let commit = Rev::parse(COMMIT).unwrap();
        let bytes = canonical::to_vec(&commit).unwrap();
        assert_eq!(canonical::from_slice::<Rev>(&bytes).unwrap(), commit);
        assert!(canonical::from_slice::<CommitId>(br#""abc""#).is_err());
    }

    #[test]
    fn forty_hex_is_a_commit_else_a_tag() {
        assert_eq!(
            Rev::parse(COMMIT).unwrap(),
            Rev::Commit(CommitId::parse(COMMIT).unwrap())
        );
        assert_eq!(
            Rev::parse("v1.2.0").unwrap(),
            Rev::Tag(TagName::parse("v1.2.0").unwrap())
        );
        assert_eq!(
            Rev::parse("abcdef0").unwrap(),
            Rev::Tag(TagName::parse("abcdef0").unwrap())
        );
        assert!(Rev::parse(&COMMIT.to_uppercase()).is_err());
        assert!(Rev::parse("").is_err());
        assert_eq!(Rev::parse(COMMIT).unwrap().to_string(), COMMIT);
        assert_eq!(Rev::parse("v1.2.0").unwrap().to_string(), "v1.2.0");
    }
}
