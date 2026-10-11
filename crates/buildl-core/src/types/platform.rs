//! The platform actions run on, validated at construction.
//!
//! Its own file because an operating system and an architecture are one fact about a machine:
//! the same command on another platform is another action, so the pair is part of an action key.
//!
//! Responsibilities: [`Platform`], its [`Platform::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: detecting the platform. This crate never asks what machine it is on;
//! the value arrives through a port.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, NameKind, Result};

/// Longest accepted operating system or architecture, in bytes.
const PART_MAX_LEN: usize = 64;

/// Whether `byte` may appear in an operating system or an architecture.
const fn is_part_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

/// The grammar of one half of a platform, returning the reason a candidate is rejected.
fn part_grammar(part: &str) -> core::result::Result<(), &'static str> {
    if part.is_empty() {
        return Err("the operating system and the architecture must not be empty");
    }
    if part.len() > PART_MAX_LEN {
        return Err("the operating system and the architecture must be at most 64 bytes each");
    }
    if !part.bytes().all(is_part_byte) {
        return Err(
            "the operating system and the architecture may hold only ASCII letters, digits, \
             '_', '-' and '.'",
        );
    }
    Ok(())
}

/// The grammar of a platform, returning the reason a candidate is rejected.
fn platform_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    let Some((os, arch)) = raw.split_once('/') else {
        return Err("must be an operating system and an architecture joined by '/'");
    };
    part_grammar(os)?;
    part_grammar(arch)
}

/// An operating system and an architecture, such as `linux/x86_64` or `macos/aarch64`.
///
/// Two parts joined by one `/`, each 1 to 64 bytes of ASCII letters, digits, `_`, `-` and `.`.
/// The spelling of each part is whatever the host reports: two platforms are the same exactly
/// when their text is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Platform(String);

impl Platform {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not two parts joined by one `/`, or when a
    /// part is empty, exceeds 64 bytes, or holds a character outside ASCII alphanumerics, `_`,
    /// `-` and `.`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match platform_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::Platform,
                value: raw,
                reason,
            }),
        }
    }

    /// The platform as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Platform {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Platform {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<Platform> for String {
    fn from(value: Platform) -> Self {
        value.0
    }
}

impl TryFrom<String> for Platform {
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

    use super::Platform;

    #[test]
    fn platform_accepts_the_documented_forms() {
        for raw in ["linux/x86_64", "macos/aarch64", "a/b", "free-bsd.14/arm_v7"] {
            assert!(Platform::parse(raw).is_ok(), "{raw:?} should parse");
        }
        let longest = format!("{}/{}", "a".repeat(64), "b".repeat(64));
        assert!(Platform::parse(longest).is_ok());
    }

    #[test]
    fn platform_rejects_each_documented_form_with_its_reason() {
        let joined = "must be an operating system and an architecture joined by '/'";
        let empty = "the operating system and the architecture must not be empty";
        let characters = "the operating system and the architecture may hold only ASCII letters, \
                          digits, '_', '-' and '.'";
        let cases = [
            ("", joined),
            ("linux", joined),
            ("/x86_64", empty),
            ("linux/", empty),
            ("linux/x86/64", characters),
            ("lin ux/x86_64", characters),
            ("linux/x86:64", characters),
        ];
        for (raw, reason) in cases {
            let err = Platform::parse(raw).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("invalid platform: {raw:?} — {reason}")
            );
        }
    }

    #[test]
    fn platform_rejects_a_part_longer_than_64_bytes() {
        let long_os = format!("{}/x86_64", "a".repeat(65));
        let long_arch = format!("linux/{}", "b".repeat(65));
        for raw in [long_os, long_arch] {
            let err = Platform::parse(raw).unwrap_err();
            assert!(
                err.to_string().ends_with("must be at most 64 bytes each"),
                "{err}"
            );
        }
    }

    #[test]
    fn platform_round_trips_through_json_as_a_string() {
        let value = Platform::parse("linux/x86_64").unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""linux/x86_64""#);
        assert_eq!(serde_json::from_str::<Platform>(&json).unwrap(), value);
        assert_eq!(value.to_string(), "linux/x86_64");
        assert!(serde_json::from_str::<Platform>(r#""linux""#).is_err());
    }

    #[test]
    fn platform_from_str_routes_through_parse() {
        let parsed = "macos/aarch64".parse::<Platform>().unwrap();
        assert_eq!(parsed.as_str(), "macos/aarch64");
        assert!("macos".parse::<Platform>().is_err());
    }
}
