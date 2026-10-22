//! How a workspace settles whether a build may run, validated at construction.
//!
//! Its own file because the choice is a closed set a manifest names by a word, and no phase
//! reads it: whoever assembles the adapters picks the approver from it.
//!
//! Responsibilities: [`ApproverMode`], its [`ApproverMode::parse`] constructor and its rendering.
//!
//! Non-responsibilities: approving anything. An approver is an adapter; this is only the name
//! the manifest picks one by.

use core::fmt;
use core::str::FromStr;

use crate::error::{Error, NameKind, Result};

/// The way a workspace settles whether a build may run.
///
/// # Examples
///
/// ```
/// use buildl_core::ApproverMode;
///
/// assert_eq!(ApproverMode::default(), ApproverMode::Manifest);
/// assert_eq!(ApproverMode::parse("deny")?, ApproverMode::Deny);
/// assert_eq!(ApproverMode::Interactive.to_string(), "interactive");
/// assert!(ApproverMode::parse("ask").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ApproverMode {
    /// `manifest`: the workspace's ceiling is all the approval there is. The default.
    #[default]
    Manifest,
    /// `interactive`: the user is asked about what is new, and the answer is remembered.
    Interactive,
    /// `deny`: no build runs.
    Deny,
}

impl ApproverMode {
    /// Every mode.
    const ALL: [Self; 3] = [Self::Manifest, Self::Interactive, Self::Deny];

    /// The mode the word `raw` names.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not exactly `manifest`, `interactive` or
    /// `deny`.
    pub fn parse(raw: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == raw)
            .ok_or_else(|| Error::InvalidName {
                kind: NameKind::Approver,
                value: raw.to_owned(),
                reason: "must be one of manifest, interactive and deny",
            })
    }

    /// The word a manifest names this mode by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Interactive => "interactive",
            Self::Deny => "deny",
        }
    }
}

impl fmt::Display for ApproverMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ApproverMode {
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

    use super::ApproverMode;
    use crate::error::{Error, NameKind};

    #[test]
    fn the_default_is_manifest() {
        assert_eq!(ApproverMode::default(), ApproverMode::Manifest);
    }

    #[test]
    fn each_word_parses_and_displays_back() {
        let cases = [
            ("manifest", ApproverMode::Manifest),
            ("interactive", ApproverMode::Interactive),
            ("deny", ApproverMode::Deny),
        ];
        for (raw, expected) in cases {
            assert_eq!(ApproverMode::parse(raw).unwrap(), expected, "{raw:?}");
            assert_eq!(raw.parse::<ApproverMode>().unwrap(), expected);
            assert_eq!(expected.to_string(), raw);
        }
        assert_eq!(cases.map(|(_, mode)| mode), ApproverMode::ALL);
    }

    #[test]
    fn another_word_is_refused() {
        for raw in ["", "Manifest", "ask", " deny", "allow"] {
            let err = ApproverMode::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::Approver,
                        ..
                    }
                ),
                "{raw:?}"
            );
            assert_eq!(
                err.to_string(),
                format!(
                    "invalid approver mode: {raw:?} — must be one of manifest, interactive \
                     and deny"
                )
            );
        }
    }
}
