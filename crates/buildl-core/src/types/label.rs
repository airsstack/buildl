//! A whole, absolute target name, validated at construction.
//!
//! Its own file because it is the type the rest of the pipeline names a target by, and because
//! it owns the grammar that joins the two halves. There is deliberately no representation of an
//! unresolved reference: a `Label` is absolute, so no later phase asks whether the one it holds
//! still needs resolving.
//!
//! Responsibilities: [`Label`], the absolute [`Label::parse`] form, and its rendering.
//!
//! Non-responsibilities: existence. A `Label` names a target; whether one was declared under
//! that name belongs to the phase that builds the graph.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::{Directory, TargetName};

/// A target's absolute name, written `//<directory>:<name>`.
///
/// Rendering is exact and round-trips: `Label::parse(&label.to_string())` returns an equal value
/// for every `Label`. A directory-only reference such as `//lib` is rejected rather than read as
/// `//lib:lib`, so one target never has two spellings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Label {
    directory: Directory,
    name: TargetName,
}

impl Label {
    /// Joins an already-validated directory and name.
    #[must_use]
    pub const fn new(directory: Directory, name: TargetName) -> Self {
        Self { directory, name }
    }

    /// Parses the absolute form `//<directory>:<name>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` lacks the `//` prefix or a `:`, and propagates
    /// the failure of either half's own grammar.
    pub fn parse(raw: &str) -> Result<Self> {
        let invalid = |reason: &'static str| Error::InvalidName {
            kind: NameKind::Label,
            value: raw.to_owned(),
            reason,
        };

        let body = raw
            .strip_prefix("//")
            .ok_or_else(|| invalid("must start with '//'"))?;
        let (directory, name) = body
            .split_once(':')
            .ok_or_else(|| invalid("must hold ':' and a target name"))?;

        Ok(Self {
            directory: Directory::parse(directory)?,
            name: TargetName::parse(name)?,
        })
    }

    /// Resolves a reference as a build file may write it, against the declaring directory.
    ///
    /// Three forms are accepted: the absolute `//<directory>:<name>`, which ignores `base`;
    /// `:<name>`, a sibling in `base`; and a bare `<name>`, also in `base`. A single dependency
    /// list may mix them, which is why all three resolve here rather than at the call site.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when the reference fails its grammar — for the absolute form,
    /// whatever [`Label::parse`] rejects; otherwise whatever [`TargetName::parse`] rejects.
    pub fn resolve(raw: &str, base: &Directory) -> Result<Self> {
        if raw.starts_with("//") {
            return Self::parse(raw);
        }
        let name = raw.strip_prefix(':').unwrap_or(raw);
        Ok(Self {
            directory: base.clone(),
            name: TargetName::parse(name)?,
        })
    }

    /// The directory half.
    #[must_use]
    pub const fn directory(&self) -> &Directory {
        &self.directory
    }

    /// The name half.
    #[must_use]
    pub const fn name(&self) -> &TargetName {
        &self.name
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "//{}:{}", self.directory, self.name)
    }
}

impl FromStr for Label {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Label> for String {
    fn from(value: Label) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for Label {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(&raw)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Label;
    use crate::types::{Directory, TargetName};

    #[test]
    fn new_joins_the_directory_and_name_in_order() {
        let directory = Directory::parse("lib/text").unwrap();
        let name = TargetName::parse("core").unwrap();
        let label = Label::new(directory, name);
        assert_eq!(label.to_string(), "//lib/text:core");
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!(
            "//lib:text".parse::<Label>().unwrap().to_string(),
            "//lib:text"
        );
        assert!("//lib".parse::<Label>().is_err());
    }

    #[test]
    fn parses_and_round_trips_the_absolute_form() {
        for raw in ["//lib:text", "//lib/text:core", "//:top"] {
            let label = Label::parse(raw).unwrap();
            assert_eq!(label.to_string(), raw);
            assert_eq!(Label::parse(&label.to_string()).unwrap(), label);
        }
    }

    #[test]
    fn exposes_both_halves() {
        let label = Label::parse("//lib/text:core").unwrap();
        assert_eq!(label.directory().as_str(), "lib/text");
        assert_eq!(label.name().as_str(), "core");
        assert!(Label::parse("//:top").unwrap().directory().is_root());
    }

    #[test]
    fn rejects_every_form_that_is_not_one_absolute_label() {
        for raw in [
            "//lib", "//lib:", "//:", "//a:b:c", "lib:text", ":sibling", "main.o",
        ] {
            assert!(Label::parse(raw).is_err(), "{raw} should be rejected");
        }
    }

    #[test]
    fn orders_by_directory_then_name() {
        let mut labels = ["//lib:z", "//:a", "//lib:a", "//app:m"]
            .map(|raw| Label::parse(raw).unwrap())
            .to_vec();
        labels.sort();
        let rendered: Vec<String> = labels.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, ["//:a", "//app:m", "//lib:a", "//lib:z"]);
    }

    #[test]
    fn round_trips_through_json_as_a_string() {
        let label = Label::parse("//lib:text").unwrap();
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, r#""//lib:text""#);
        assert_eq!(serde_json::from_str::<Label>(&json).unwrap(), label);
        assert!(serde_json::from_str::<Label>(r#""//lib""#).is_err());
    }

    #[test]
    fn resolves_every_reference_form_a_build_file_may_write() {
        let base = Directory::parse("app").unwrap();
        assert_eq!(
            Label::resolve("//lib:text", &base).unwrap().to_string(),
            "//lib:text"
        );
        assert_eq!(
            Label::resolve(":sibling", &base).unwrap().to_string(),
            "//app:sibling"
        );
        assert_eq!(
            Label::resolve("main.o", &base).unwrap().to_string(),
            "//app:main.o"
        );
        assert_eq!(
            Label::resolve("main.o", &Directory::root())
                .unwrap()
                .to_string(),
            "//:main.o"
        );
    }

    #[test]
    fn resolve_rejects_a_malformed_reference_in_either_form() {
        let base = Directory::parse("app").unwrap();
        assert!(Label::resolve("//lib", &base).is_err());
        assert!(Label::resolve(":", &base).is_err());
        assert!(Label::resolve("a/b", &base).is_err());
        assert!(Label::resolve("", &base).is_err());
    }

    #[test]
    fn resolve_is_usable_as_a_json_map_key() {
        let base = Directory::root();
        let mut map = std::collections::BTreeMap::new();
        map.insert(Label::resolve("main.o", &base).unwrap(), 7_u32);
        map.insert(Label::resolve("//lib:text", &base).unwrap(), 9_u32);
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"//:main.o":7,"//lib:text":9}"#
        );
    }
}
