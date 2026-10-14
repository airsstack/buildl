//! How strongly a workspace isolates its actions, validated at construction.
//!
//! Its own file because the choice is a closed set with an order: each strategy isolates an
//! action at least as much as the one before it, and a manifest names one of them by a word.
//!
//! Responsibilities: [`Isolation`], its [`Isolation::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: isolating anything. An execution strategy is an adapter; this is only
//! the name the manifest picks one by.

use core::fmt;
use core::str::FromStr;

use crate::error::{Error, NameKind, Result};

/// The execution strategy a workspace runs its actions under.
///
/// The variants are in order of increasing isolation, and compare that way, so a caller that
/// needs at least an input sandbox asks for `isolation >= Isolation::Sandbox`.
///
/// # Examples
///
/// ```
/// use buildl_core::Isolation;
///
/// let isolation = Isolation::parse("os-sandbox")?;
/// assert_eq!(isolation, Isolation::OsSandbox);
/// assert!(isolation >= Isolation::Sandbox);
/// assert_eq!(isolation.to_string(), "os-sandbox");
/// assert!(Isolation::parse("docker").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Isolation {
    /// `bare`: a plain process in the workspace, with only the environment it was granted.
    Bare,
    /// `sandbox`: a directory of its own, holding only the inputs the action declared. The
    /// default.
    #[default]
    Sandbox,
    /// `os-sandbox`: an input sandbox the operating system also seals from the network and from
    /// the rest of the machine's files.
    OsSandbox,
    /// `container`: a container made from a pinned image.
    Container,
}

impl Isolation {
    /// Every strategy, least isolated first.
    const ALL: [Self; 4] = [Self::Bare, Self::Sandbox, Self::OsSandbox, Self::Container];

    /// The strategy the word `raw` names.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not exactly `bare`, `sandbox`, `os-sandbox`
    /// or `container`.
    pub fn parse(raw: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|isolation| isolation.as_str() == raw)
            .ok_or_else(|| Error::InvalidName {
                kind: NameKind::Isolation,
                value: raw.to_owned(),
                reason: "must be one of bare, sandbox, os-sandbox and container",
            })
    }

    /// The word a manifest names this strategy by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bare => "bare",
            Self::Sandbox => "sandbox",
            Self::OsSandbox => "os-sandbox",
            Self::Container => "container",
        }
    }
}

impl fmt::Display for Isolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Isolation {
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

    use super::Isolation;

    #[test]
    fn each_word_names_its_strategy_and_renders_back() {
        let cases = [
            ("bare", Isolation::Bare),
            ("sandbox", Isolation::Sandbox),
            ("os-sandbox", Isolation::OsSandbox),
            ("container", Isolation::Container),
        ];
        for (raw, expected) in cases {
            assert_eq!(Isolation::parse(raw).unwrap(), expected, "{raw:?}");
            assert_eq!(expected.as_str(), raw);
            assert_eq!(expected.to_string(), raw);
        }
        assert_eq!(cases.map(|(_, isolation)| isolation), Isolation::ALL);
    }

    #[test]
    fn any_other_text_is_rejected_with_the_words_that_are_allowed() {
        for raw in ["", "Sandbox", "os_sandbox", "ossandbox", " bare", "docker"] {
            let err = Isolation::parse(raw).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!(
                    "invalid execution strategy: {raw:?} — must be one of bare, sandbox, \
                     os-sandbox and container"
                )
            );
        }
    }

    #[test]
    fn strategies_order_by_how_much_they_isolate() {
        assert!(Isolation::Bare < Isolation::Sandbox);
        assert!(Isolation::Sandbox < Isolation::OsSandbox);
        assert!(Isolation::OsSandbox < Isolation::Container);
    }

    #[test]
    fn the_default_is_the_input_sandbox() {
        assert_eq!(Isolation::default(), Isolation::Sandbox);
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!(
            "container".parse::<Isolation>().unwrap(),
            Isolation::Container
        );
        assert!("vm".parse::<Isolation>().is_err());
    }
}
