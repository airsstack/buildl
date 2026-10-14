//! One argument of a command, and a whole command, each validated at construction.
//!
//! Its own file because a command is nothing but its arguments: the two types are read and
//! changed together, and a command's only rule beyond its arguments' is that it has one.
//!
//! Responsibilities: [`Argument`] and [`Command`], their constructors, their rendering, finding
//! the build settings an argument refers to, and replacing those references by the settings'
//! values.
//!
//! Non-responsibilities: expanding `$in`, `$out` and `$deps`. They stand for paths, which only
//! the executor knows, so they stay text here. Whether a setting an argument refers to is
//! declared belongs to the phase that builds the graph.

use core::fmt;
use core::str::FromStr;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;
use crate::types::setting_reference;
use crate::types::{SettingName, SettingValue};

/// One element of a command's argument vector, such as `cc` or `$in`.
///
/// Any UTF-8 text without a `NUL` byte, the empty string included: an argument vector cannot
/// carry `NUL`, and an empty argument is a legitimate thing to pass.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Argument(String);

impl Argument {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` holds a `NUL` byte.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::text(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::Argument,
                value: raw,
                reason,
            }),
        }
    }

    /// The argument as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The build-setting names this argument refers to, in the order they appear.
    ///
    /// A reference is `$opt:` followed by a name: the longest run of ASCII letters, digits, `-`
    /// and `_`. An `$opt:` with no such character after it is not a reference and stays text.
    /// A name is returned as written, so it may be longer than a setting name can be.
    ///
    /// # Examples
    ///
    /// ```
    /// use buildl_core::Argument;
    ///
    /// let argument = Argument::parse("--run=$opt:test_filter")?;
    /// assert_eq!(argument.setting_references(), ["test_filter"]);
    /// # Ok::<(), buildl_core::Error>(())
    /// ```
    #[must_use]
    pub fn setting_references(&self) -> Vec<&str> {
        setting_reference::names(&self.0)
    }

    /// This argument with each reference to a setting `values` holds replaced by that setting's
    /// value.
    ///
    /// The references are the ones [`Argument::setting_references`] finds. One naming a setting
    /// `values` does not hold stays as written, and so does every other `$`: `$in`, `$out` and
    /// `$deps` are not replaced here. A value is put in as it is and never read again, so a
    /// setting whose value is `$opt:other` yields that text.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use buildl_core::{Argument, SettingName, SettingValue};
    ///
    /// let values = BTreeMap::from([(SettingName::parse("cc")?, SettingValue::parse("clang")?)]);
    /// let argument = Argument::parse("/usr/bin/$opt:cc")?;
    /// assert_eq!(argument.with_settings(&values).as_str(), "/usr/bin/clang");
    /// assert_eq!(argument.with_settings(&BTreeMap::new()), argument);
    /// # Ok::<(), buildl_core::Error>(())
    /// ```
    #[must_use]
    pub fn with_settings(&self, values: &BTreeMap<SettingName, SettingValue>) -> Self {
        let mut text = String::new();
        let mut rest = self.0.as_str();
        while let Some((before, name, tail)) = setting_reference::cut(rest) {
            text.push_str(before);
            let value = SettingName::parse(name)
                .ok()
                .and_then(|name| values.get(&name));
            match value {
                Some(value) => text.push_str(value.as_str()),
                None => text.extend([setting_reference::MARKER, name]),
            }
            rest = tail;
        }
        text.push_str(rest);
        // Neither this argument nor a setting's value can hold a `NUL` byte, so the result is
        // text an argument can hold.
        Self(text)
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Argument {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Argument {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Argument> for String {
    fn from(value: Argument) -> Self {
        value.0
    }
}

impl TryFrom<String> for Argument {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// A command to run: a non-empty argument vector whose first element names the program.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "Vec<Argument>", try_from = "Vec<Argument>")]
pub struct Command(Vec<Argument>);

impl Command {
    /// Wraps `arguments` as a command.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `arguments` is empty: a command with no program is
    /// meaningless.
    pub fn new(arguments: Vec<Argument>) -> Result<Self> {
        if arguments.is_empty() {
            return Err(Error::InvalidName {
                kind: NameKind::Command,
                value: String::new(),
                reason: "must hold at least one argument",
            });
        }
        Ok(Self(arguments))
    }

    /// The arguments, program first.
    #[must_use]
    pub fn arguments(&self) -> &[Argument] {
        &self.0
    }
}

impl From<Command> for Vec<Argument> {
    fn from(value: Command) -> Self {
        value.0
    }
}

impl TryFrom<Vec<Argument>> for Command {
    type Error = Error;
    fn try_from(arguments: Vec<Argument>) -> Result<Self> {
        Self::new(arguments)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{Argument, Command};
    use crate::types::{SettingName, SettingValue};

    fn args(raw: &[&str]) -> Vec<Argument> {
        raw.iter().map(|a| Argument::parse(*a).unwrap()).collect()
    }

    #[test]
    fn argument_accepts_the_documented_forms() {
        for raw in ["cc", "$in", "", "-o", "a b"] {
            assert!(Argument::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn argument_rejects_a_nul_byte() {
        assert!(Argument::parse("a\0b").is_err());
        assert!("a\0b".parse::<Argument>().is_err());
    }

    #[test]
    fn argument_round_trips_through_json_as_a_string() {
        let value = Argument::parse("$out").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""$out""#);
        assert_eq!(serde_json::from_str::<Argument>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "$out");
        assert_eq!(value.as_str(), "$out");
        assert!(serde_json::from_str::<Argument>(r#""a\u0000b""#).is_err());
    }

    fn references(raw: &str) -> Vec<String> {
        Argument::parse(raw)
            .unwrap()
            .setting_references()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn setting_references_are_found_in_order() {
        assert_eq!(references("$opt:test_filter"), ["test_filter"]);
        assert_eq!(references("--run=$opt:a.$opt:b-c"), ["a", "b-c"]);
        assert_eq!(references("$opt:A9_x/tail"), ["A9_x"]);
        assert_eq!(references("$opt:a$opt:b"), ["a", "b"]);
    }

    #[test]
    fn text_that_names_no_setting_is_not_a_reference() {
        for raw in ["$opt:", "$opt:.x", "$option", "opt:x", "$in", "", "$opt"] {
            assert!(references(raw).is_empty(), "{raw:?} holds no reference");
        }
    }

    #[test]
    fn a_marker_with_no_name_does_not_hide_a_later_reference() {
        assert_eq!(references("$opt:.$opt:x"), ["x"]);
        assert_eq!(references("$opt:$opt:x"), ["x"]);
    }

    #[test]
    fn a_reference_longer_than_a_setting_name_is_returned_whole() {
        let name = "a".repeat(257);
        assert_eq!(references(&format!("$opt:{name}")), [name]);
    }

    /// `raw` with its references to these `(name, value)` settings replaced.
    fn replaced(raw: &str, pairs: &[(&str, &str)]) -> String {
        let values: BTreeMap<SettingName, SettingValue> = pairs
            .iter()
            .map(|(name, value)| {
                (
                    SettingName::parse(*name).unwrap(),
                    SettingValue::parse(*value).unwrap(),
                )
            })
            .collect();
        Argument::parse(raw)
            .unwrap()
            .with_settings(&values)
            .as_str()
            .to_owned()
    }

    #[test]
    fn a_reference_to_a_held_setting_becomes_its_value() {
        let held = [("tool", "clang"), ("mode", "debug"), ("empty", "")];
        assert_eq!(replaced("$opt:tool", &held), "clang");
        assert_eq!(replaced("/usr/bin/$opt:tool", &held), "/usr/bin/clang");
        assert_eq!(replaced("$opt:tool.$opt:mode", &held), "clang.debug");
        assert_eq!(replaced("$opt:tool$opt:tool", &held), "clangclang");
        assert_eq!(replaced("--x=$opt:empty", &held), "--x=");
    }

    #[test]
    fn a_reference_to_a_setting_that_is_not_held_stays_as_written() {
        let held = [("mode", "debug")];
        assert_eq!(replaced("$opt:missing", &held), "$opt:missing");
        assert_eq!(
            replaced("$opt:missing,$opt:mode", &held),
            "$opt:missing,debug"
        );
        assert_eq!(
            replaced("-O$opt:mode-$opt:mode", &held),
            "-O$opt:mode-debug",
            "a name runs through `-`, so `mode-` is one name, and it is not held"
        );
        let long = format!("$opt:{}", "a".repeat(257));
        assert_eq!(replaced(&long, &held), long);
    }

    #[test]
    fn text_that_is_no_reference_is_left_alone() {
        let held = [("in", "I"), ("x", "X")];
        for raw in [
            "", "cc", "$in", "$out/app", "$deps", "$opt:", "$opt:/x", "$", "$opt",
        ] {
            assert_eq!(replaced(raw, &held), raw, "{raw:?}");
        }
        assert_eq!(replaced("$opt:in$in", &held), "I$in");
        assert_eq!(replaced("$opt:.$opt:x", &held), "$opt:.X");
    }

    #[test]
    fn a_value_put_in_is_never_read_again() {
        let held = [("flag", "$opt:tool"), ("tool", "cc"), ("half", "$opt")];
        assert_eq!(replaced("$opt:flag", &held), "$opt:tool");
        assert_eq!(replaced("$opt:half:tool", &held), "$opt:tool");
    }

    #[test]
    fn command_keeps_its_arguments_in_order() {
        let command = Command::new(args(&["cc", "-c", "$in"])).unwrap();
        let rendered: Vec<&str> = command.arguments().iter().map(Argument::as_str).collect();
        assert_eq!(rendered, ["cc", "-c", "$in"]);
    }

    #[test]
    fn command_rejects_an_empty_argument_vector() {
        let err = Command::new(Vec::new()).unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid command: "" — must hold at least one argument"#
        );
    }

    #[test]
    fn command_round_trips_through_json_as_an_array() {
        let command = Command::new(args(&["go", "test"])).unwrap();
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(json, r#"["go","test"]"#);
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        assert!(serde_json::from_str::<Command>("[]").is_err());
    }
}
