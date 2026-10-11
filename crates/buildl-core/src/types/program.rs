//! The name of a program a command runs, validated at construction.
//!
//! Its own file because a program is not any argument: it is the one a workspace must allow
//! before anything runs, and the one whose identity is part of an action key. Naming it with its
//! own type keeps an argument that names nothing out of both.
//!
//! Responsibilities: [`Program`], its constructors, and its rendering.
//!
//! Non-responsibilities: finding the program. Whether it exists, and what it is, is answered
//! through a port.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};
use crate::types::Argument;
use crate::types::grammar;

/// The grammar of a program name, returning the reason a candidate is rejected.
fn program_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    grammar::text(raw)?;
    if raw.contains('$') {
        return Err("must not hold '$'");
    }
    Ok(())
}

/// The name of a program to run, such as `cc`, `/usr/bin/cc` or `scripts/gen.sh`.
///
/// Any non-empty UTF-8 text without a `NUL` byte and without `$`. A `$` starts a placeholder,
/// and text holding one names nothing until the placeholder is expanded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Program(String);

impl Program {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds a `NUL` byte, or holds `$`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match program_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::Program,
                value: raw,
                reason,
            }),
        }
    }

    /// The program `argument` names; `None` when it is empty or holds `$`.
    ///
    /// # Examples
    ///
    /// ```
    /// use buildl_core::{Argument, Program};
    ///
    /// let named = Program::of(&Argument::parse("cc")?);
    /// assert_eq!(named.as_ref().map(Program::as_str), Some("cc"));
    /// assert_eq!(Program::of(&Argument::parse("$out/app")?), None);
    /// # Ok::<(), buildl_core::Error>(())
    /// ```
    #[must_use]
    pub fn of(argument: &Argument) -> Option<Self> {
        Self::parse(argument.as_str()).ok()
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Program {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Program {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Program> for String {
    fn from(value: Program) -> Self {
        value.0
    }
}

impl TryFrom<String> for Program {
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

    use super::Program;
    use crate::types::Argument;

    #[test]
    fn program_accepts_the_documented_forms() {
        for raw in ["cc", "/usr/bin/cc", "scripts/gen.sh", "my tool", "c++"] {
            assert!(Program::parse(raw).is_ok(), "{raw:?} should parse");
        }
    }

    #[test]
    fn program_rejects_each_documented_form_with_its_reason() {
        let cases = [
            ("", "must not be empty"),
            ("a\0b", "must not contain a NUL byte"),
            ("$out/app", "must not hold '$'"),
            ("bin/$opt:tool", "must not hold '$'"),
        ];
        for (raw, reason) in cases {
            let err = Program::parse(raw).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("invalid program: {raw:?} — {reason}")
            );
        }
    }

    #[test]
    fn of_names_a_program_only_for_an_argument_that_is_one() {
        let cases = [
            ("", None),
            ("bin/$opt:tool", None),
            ("cc", Some("cc")),
            ("/usr/bin/cc", Some("/usr/bin/cc")),
            ("$out/app", None),
            ("scripts/gen.sh", Some("scripts/gen.sh")),
        ];
        for (raw, expected) in cases {
            let named = Program::of(&Argument::parse(raw).unwrap());
            assert_eq!(named.as_ref().map(Program::as_str), expected, "{raw:?}");
        }
    }

    #[test]
    fn program_round_trips_through_json_as_a_string() {
        let value = Program::parse("/usr/bin/cc").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""/usr/bin/cc""#);
        assert_eq!(serde_json::from_str::<Program>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "/usr/bin/cc");
        assert!(serde_json::from_str::<Program>(r#""$in""#).is_err());
    }

    #[test]
    fn program_from_str_routes_through_parse() {
        assert_eq!("cc".parse::<Program>().unwrap().as_str(), "cc");
        assert!("".parse::<Program>().is_err());
    }

    #[test]
    fn programs_order_by_their_text() {
        assert!(Program::parse("cc").unwrap() < Program::parse("ld").unwrap());
    }
}
