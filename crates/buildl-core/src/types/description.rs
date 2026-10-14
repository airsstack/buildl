//! A rule's one-line description as it is written, and as it is shown once expanded.
//!
//! Its own file because a description is shown on the status line while an action runs, so it
//! must fit on one line, and because the two forms of it are read together: the text a build
//! file wrote, with its placeholders, and the text those placeholders stand for.
//!
//! Responsibilities: [`Description`], its [`Description::parse`] constructor, its rendering and
//! the build settings it refers to; and [`ExpandedDescription`].
//!
//! Non-responsibilities: placeholder expansion. A `$in` inside a description stays text here;
//! the executor expands it. Whether a setting a description refers to is declared belongs to
//! the phase that builds the graph.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::setting_reference;

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

    /// The build-setting names this description refers to, in the order they appear.
    ///
    /// A reference is found by the same rule as
    /// [`Argument::setting_references`](crate::Argument::setting_references): `$opt:` followed by
    /// the longest run of ASCII letters, digits, `-` and `_`, and an `$opt:` with no such
    /// character after it is text. A name is returned as written, so it may be longer than a
    /// setting name can be.
    ///
    /// # Examples
    ///
    /// ```
    /// use buildl_core::Description;
    ///
    /// let description = Description::parse("test $opt:test_filter")?;
    /// assert_eq!(description.setting_references(), ["test_filter"]);
    /// # Ok::<(), buildl_core::Error>(())
    /// ```
    #[must_use]
    pub fn setting_references(&self) -> Vec<&str> {
        setting_reference::names(&self.0)
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

/// A description once its placeholders are expanded: the text shown for one action.
///
/// One line of any length. A control character in what the placeholders stood for, such as a
/// newline in a setting's value, is shown as a space, so that the text always fits on one line;
/// no other character changes. There is no length limit, since a list of paths in a description
/// has none.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpandedDescription(String);

impl ExpandedDescription {
    /// Wraps `text`, with each control character in it replaced by a space.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self(
            text.chars()
                .map(|letter| if letter.is_control() { ' ' } else { letter })
                .collect(),
        )
    }

    /// The text as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExpandedDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ExpandedDescription {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{Description, ExpandedDescription};
    use crate::types::Argument;

    #[test]
    fn a_description_lists_the_settings_it_refers_to_as_a_command_argument_does() {
        for raw in [
            "compile $in",
            "run $opt:test_filter",
            "$opt:a-b.$opt:c_d $opt:",
            "$opt:$opt:x $opt:é $opt:y,z",
            "$opt:a$opt:b",
            "no reference $opt",
            "$opt:name$",
        ] {
            let description = Description::parse(raw).unwrap();
            let argument = Argument::parse(raw).unwrap();
            assert_eq!(
                description.setting_references(),
                argument.setting_references(),
                "{raw:?}"
            );
        }
        assert_eq!(
            Description::parse("$opt:a-b.$opt:c_d")
                .unwrap()
                .setting_references(),
            ["a-b", "c_d"]
        );
    }

    #[test]
    fn an_expanded_description_shows_each_control_character_as_a_space() {
        assert_eq!(
            ExpandedDescription::new("compile a.c\nthen\tlink\0").as_str(),
            "compile a.c then link "
        );
        assert_eq!(
            ExpandedDescription::new("link → app, no change").to_string(),
            "link → app, no change"
        );
        assert_eq!(ExpandedDescription::new("").as_str(), "");
    }

    #[test]
    fn an_expanded_description_has_no_length_limit() {
        let long = "a.c ".repeat(1000);
        assert_eq!(ExpandedDescription::new(&long).as_str(), long);
        assert!(Description::parse(long).is_err(), "a written one has");
    }

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
