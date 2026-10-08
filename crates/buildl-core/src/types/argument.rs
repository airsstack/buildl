//! One argument of a command, and a whole command, each validated at construction.
//!
//! Its own file because a command is nothing but its arguments: the two types are read and
//! changed together, and a command's only rule beyond its arguments' is that it has one.
//!
//! Responsibilities: [`Argument`] and [`Command`], their constructors, and their rendering.
//!
//! Non-responsibilities: placeholder expansion. `$in`, `$out`, `$deps` and `$opt:name` stay text
//! here; the executor expands them when the action runs.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::grammar;

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

    use super::{Argument, Command};

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
