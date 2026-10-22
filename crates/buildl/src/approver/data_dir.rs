//! Where the user's data directory is.
//!
//! Its own file because the approvals file lies in it, outside every workspace, and the lookup of
//! the variables that name it must be replaceable: a test cannot set environment variables of its
//! own process.
//!
//! Responsibilities: [`DataDir`].

use std::ffi::OsString;
use std::path::PathBuf;

use buildl_core::{Diagnostic, Error, PortName, Result};

/// The directory buildl keeps what belongs to the user rather than to a workspace in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DataDir {
    /// The user's own, found from the environment.
    #[default]
    User,
    /// This directory, as given.
    At(PathBuf),
}

impl DataDir {
    /// The directory.
    ///
    /// For [`DataDir::User`] it is `$XDG_DATA_HOME` when that is an absolute path, else
    /// `$HOME/.local/share` when `$HOME` is an absolute path. A value that is empty or relative
    /// is ignored, since a relative directory would lie wherever the process happens to be,
    /// possibly inside a workspace. `lookup` gives the value of an environment variable, so a caller
    /// that is not the process itself can say what the environment holds; the process passes
    /// `|name| std::env::var_os(name)`.
    ///
    /// # Errors
    ///
    /// Returns an approver error when `self` is [`DataDir::User`] and neither variable holds an
    /// absolute path.
    pub fn resolve(&self, lookup: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
        match self {
            Self::At(path) => Ok(path.clone()),
            Self::User => {
                let absolute =
                    |name: &str| lookup(name).map(PathBuf::from).filter(|p| p.is_absolute());
                if let Some(xdg) = absolute("XDG_DATA_HOME") {
                    return Ok(xdg);
                }
                absolute("HOME")
                    .map(|home| home.join(".local").join("share"))
                    .ok_or_else(|| Error::Adapter {
                        port: PortName::Approver,
                        diagnostic: Diagnostic::new(
                            "neither XDG_DATA_HOME nor HOME is set to an absolute path, so \
                             there is nowhere to keep approvals",
                        ),
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::path::PathBuf;

    use buildl_core::{Error, PortName};

    use super::DataDir;

    /// A lookup that sees only `vars`.
    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        let map: BTreeMap<&str, &str> = vars.iter().copied().collect();
        move |name| map.get(name).map(OsString::from)
    }

    #[test]
    fn xdg_data_home_wins() {
        let dir = DataDir::User
            .resolve(env(&[("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/me")]))
            .unwrap();
        assert_eq!(dir, PathBuf::from("/xdg"));
    }

    #[test]
    fn an_empty_xdg_data_home_is_ignored() {
        let dir = DataDir::User
            .resolve(env(&[("XDG_DATA_HOME", ""), ("HOME", "/home/me")]))
            .unwrap();
        assert_eq!(dir, PathBuf::from("/home/me/.local/share"));
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        let dir = DataDir::User
            .resolve(env(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/me")]))
            .unwrap();
        assert_eq!(dir, PathBuf::from("/home/me/.local/share"));
    }

    #[test]
    fn an_empty_or_relative_home_is_an_approver_error() {
        for home in ["", "home/me", "."] {
            let error = DataDir::User.resolve(env(&[("HOME", home)])).unwrap_err();
            assert!(
                matches!(
                    &error,
                    Error::Adapter {
                        port: PortName::Approver,
                        ..
                    }
                ),
                "{home:?}: {error:?}"
            );
        }
        // A relative XDG_DATA_HOME does not make up for it.
        let error = DataDir::User
            .resolve(env(&[("XDG_DATA_HOME", "data"), ("HOME", "")]))
            .unwrap_err();
        assert!(matches!(&error, Error::Adapter { .. }), "{error:?}");
    }

    #[test]
    fn home_gives_its_local_share() {
        let dir = DataDir::User.resolve(env(&[("HOME", "/home/me")])).unwrap();
        assert_eq!(dir, PathBuf::from("/home/me/.local/share"));
    }

    #[test]
    fn neither_set_is_an_approver_error() {
        let error = DataDir::User.resolve(env(&[])).unwrap_err();
        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Approver,
                    ..
                }
            ),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(
            text.contains("XDG_DATA_HOME") && text.contains("HOME"),
            "{text}"
        );
    }

    #[test]
    fn a_given_directory_is_used_as_is() {
        let given = DataDir::At(PathBuf::from("/given"));
        // Not even looked at: a lookup that sees nothing changes nothing.
        assert_eq!(given.resolve(env(&[])).unwrap(), PathBuf::from("/given"));
        assert_eq!(
            given
                .resolve(env(&[("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/me")]))
                .unwrap(),
            PathBuf::from("/given")
        );
    }
}
