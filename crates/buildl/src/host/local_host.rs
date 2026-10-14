//! [`LocalHost`], the [`Host`] that is the machine buildl itself runs on.
//!
//! Its own file because the platform and the environment are the two ambient facts an action
//! key folds, and this is the one place either is read. A variable is read when it is asked for
//! and never before: a snapshot of the environment would read names the workspace's ceiling
//! does not allow.
//!
//! Responsibilities: [`LocalHost`] and its [`Host`] implementation.
//!
//! Non-responsibilities: deciding which variables may be read. A name reaches this adapter only
//! after the ceiling allowed it.

use std::env;
use std::ffi::OsString;

use buildl_core::{Diagnostic, EnvName, EnvValue, Error, Host, Platform, PortName, Result};

/// The machine buildl runs on: the platform it was compiled for, and its own environment.
///
/// # Examples
///
/// ```
/// use buildl::host::LocalHost;
/// use buildl_core::{EnvName, Host};
///
/// let host = LocalHost::new()?;
/// assert_eq!(host.platform().as_str().split('/').count(), 2);
///
/// let unset = EnvName::parse("BUILDL_DOC_A_NAME_NOTHING_SETS")?;
/// assert_eq!(host.env(&unset)?, None);
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHost {
    platform: Platform,
}

impl LocalHost {
    /// The host this binary was compiled for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] if the compiler's text for the operating system or the
    /// architecture is not a platform part.
    pub fn new() -> Result<Self> {
        Ok(Self {
            platform: Platform::parse(format!("{}/{}", env::consts::OS, env::consts::ARCH))?,
        })
    }
}

impl Host for LocalHost {
    /// The operating system and the architecture, such as `linux/x86_64` or `macos/aarch64`.
    fn platform(&self) -> Platform {
        self.platform.clone()
    }

    /// The value the variable `name` has in this process now. A variable set to nothing is
    /// `Some("")`, not `None`.
    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
        value(name, env::var_os(name.as_str()))
    }
}

/// What a variable `held` in the environment is as a value: `None` when it is unset.
fn value(name: &EnvName, held: Option<OsString>) -> Result<Option<EnvValue>> {
    held.map(|held| {
        let text = held
            .into_string()
            .map_err(|_| failed(name, "does not hold valid Unicode"))?;
        EnvValue::parse(text).map_err(|_| failed(name, "holds a NUL byte"))
    })
    .transpose()
}

/// A variable whose value cannot be handed on, as the host adapter's error.
fn failed(name: &EnvName, reason: &str) -> Error {
    Error::Adapter {
        port: PortName::Host,
        diagnostic: Diagnostic::new(format!("environment variable {name} {reason}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::env;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use buildl_core::{EnvName, Error, Host, PortName};

    use super::{LocalHost, value};

    #[cfg(target_os = "linux")]
    const OS: &str = "linux";
    #[cfg(target_os = "macos")]
    const OS: &str = "macos";

    #[cfg(target_arch = "x86_64")]
    const ARCH: &str = "x86_64";
    #[cfg(target_arch = "aarch64")]
    const ARCH: &str = "aarch64";

    fn name(text: &str) -> EnvName {
        EnvName::parse(text).unwrap()
    }

    #[test]
    fn the_platform_is_the_compiled_os_and_architecture() {
        let host = LocalHost::new().unwrap();
        assert_eq!(host.platform().as_str(), format!("{OS}/{ARCH}"));
        assert_eq!(host.platform(), host.platform());
    }

    #[test]
    fn a_variable_reads_as_the_process_holds_it() {
        let host = LocalHost::new().unwrap();
        for variable in ["PATH", "HOME", "BUILDL_TEST_A_NAME_NOTHING_SETS"] {
            let answer = host.env(&name(variable));
            match env::var_os(variable).map(OsString::into_string) {
                None => assert_eq!(answer.unwrap(), None, "{variable}"),
                Some(Ok(text)) => {
                    assert_eq!(answer.unwrap().unwrap().as_str(), text, "{variable}");
                }
                Some(Err(_)) => assert!(answer.is_err(), "{variable}"),
            }
        }
        assert_eq!(
            host.env(&name("BUILDL_TEST_A_NAME_NOTHING_SETS")).unwrap(),
            None
        );
    }

    #[test]
    fn an_unset_an_empty_and_a_non_unicode_value_are_told_apart() {
        let variable = name("P_VALUE");
        assert_eq!(value(&variable, None).unwrap(), None);
        assert_eq!(
            value(&variable, Some(OsString::new()))
                .unwrap()
                .unwrap()
                .as_str(),
            ""
        );
        assert_eq!(
            value(&variable, Some(OsString::from("a b=c")))
                .unwrap()
                .unwrap()
                .as_str(),
            "a b=c"
        );

        let not_unicode = OsString::from_vec(b"caf\xE9\xFF".to_vec());
        let error = value(&variable, Some(not_unicode)).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Host,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            "host: environment variable P_VALUE does not hold valid Unicode"
        );

        let with_nul = OsString::from_vec(b"a\0b".to_vec());
        assert_eq!(
            value(&variable, Some(with_nul)).unwrap_err().to_string(),
            "host: environment variable P_VALUE holds a NUL byte"
        );
    }
}
