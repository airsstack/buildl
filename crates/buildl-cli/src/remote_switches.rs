//! The switches that decide how a run uses the remote cache.
//!
//! Its own file because two sources answer the same questions: the variables a machine sets and
//! the flags a run passes. They are combined here, once, into the [`RemotePolicy`] the build
//! takes, and the combinations the command line refuses are refused here. The variables are
//! handed in as values, so the whole decision is a pure function a test can ask.
//!
//! Responsibilities: [`RemoteSwitches`], with [`RemoteSwitches::resolve`], [`RemoteTable`] and
//! [`Switch`], how one variable reads.
//!
//! Non-responsibilities: reading the variables, which `main` does; the flags themselves, which
//! `cli` owns; and acting on the policy, which the build does.

use core::fmt;
use std::ffi::OsString;

use buildl::buildl_core::{
    Error, RemoteFailure, RemoteFlagConflict, RemotePolicy, RemoteUse, RemoteVariable,
};

use crate::cli::RemoteArgs;

/// Whether the workspace's `buildl.toml` has a `[remote]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteTable {
    /// The workspace names a remote cache.
    Named,
    /// The workspace names none.
    Absent,
}

/// How one switch's variable reads on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Switch {
    /// The variable is unset or `0`.
    Off,
    /// The variable is `1`.
    On,
    /// The variable holds anything else, as the text it holds.
    Invalid(String),
}

impl Switch {
    /// How a variable holding `value` reads; `None` is a variable that is not set.
    fn of(value: Option<&OsString>) -> Self {
        match value.map(|value| value.to_str()) {
            None | Some(Some("0")) => Self::Off,
            Some(Some("1")) => Self::On,
            Some(_) => Self::Invalid(
                value
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        }
    }
}

impl fmt::Display for Switch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::On => f.write_str("on"),
            Self::Invalid(value) => write!(f, "invalid ({value:?} is neither 1 nor 0)"),
        }
    }
}

/// What this machine's variables say about the remote cache, as the process read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RemoteSwitches {
    /// The value of `BUILDL_REMOTE_UPLOAD`, when it is set.
    pub upload: Option<OsString>,
    /// The value of `BUILDL_REMOTE_STRICT`, when it is set.
    pub strict: Option<OsString>,
}

impl RemoteSwitches {
    /// How `BUILDL_REMOTE_UPLOAD` reads.
    pub(crate) fn upload_switch(&self) -> Switch {
        Switch::of(self.upload.as_ref())
    }

    /// How `BUILDL_REMOTE_STRICT` reads.
    pub(crate) fn strict_switch(&self) -> Switch {
        Switch::of(self.strict.as_ref())
    }

    /// The policy of a run whose command line is `args`, on a machine these variables describe,
    /// in a workspace that has `table`.
    ///
    /// A variable is `1` or `0`; a flag turns its switch on whatever its variable says, and
    /// `--no-remote` turns the remote off whatever the variables say.
    ///
    /// # Errors
    ///
    /// [`Error::RemoteSwitch`] when a variable holds anything but `1` or `0`, unless
    /// `--no-remote` is given or the switch's own flag is, since the flag settles the switch
    /// without reading the variable; [`Error::RemoteFlags`] when `--no-remote` comes with an
    /// upload or strict flag, or such a flag comes with a workspace that names no remote.
    pub(crate) fn resolve(
        &self,
        args: &RemoteArgs,
        table: RemoteTable,
    ) -> Result<RemotePolicy, Error> {
        if args.no_remote {
            return if args.upload || args.strict {
                Err(Error::RemoteFlags {
                    conflict: RemoteFlagConflict::NoRemoteWithSwitch,
                })
            } else {
                Ok(RemotePolicy::OFF)
            };
        }
        let upload = args.upload || switched_on(RemoteVariable::Upload, self.upload.as_ref())?;
        let strict = args.strict || switched_on(RemoteVariable::Strict, self.strict.as_ref())?;
        if table == RemoteTable::Absent {
            return if args.upload || args.strict {
                Err(Error::RemoteFlags {
                    conflict: RemoteFlagConflict::SwitchWithoutRemote,
                })
            } else {
                Ok(RemotePolicy::OFF)
            };
        }
        Ok(RemotePolicy {
            use_: if upload {
                RemoteUse::ReadWrite
            } else {
                RemoteUse::Read
            },
            failure: if strict {
                RemoteFailure::Strict
            } else {
                RemoteFailure::Lenient
            },
        })
    }
}

/// Whether the variable `name`, holding `value`, is on: `1` is, `0` and unset are not.
fn switched_on(name: RemoteVariable, value: Option<&OsString>) -> Result<bool, Error> {
    match Switch::of(value) {
        Switch::Off => Ok(false),
        Switch::On => Ok(true),
        Switch::Invalid(value) => Err(Error::RemoteSwitch { name, value }),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use buildl::buildl_core::{
        Error, RemoteFailure, RemoteFlagConflict, RemotePolicy, RemoteUse, RemoteVariable,
    };

    use super::{RemoteSwitches, RemoteTable, Switch};
    use crate::cli::RemoteArgs;

    /// Variables as a machine sets them: `None` is unset.
    fn machine(upload: Option<&str>, strict: Option<&str>) -> RemoteSwitches {
        RemoteSwitches {
            upload: upload.map(Into::into),
            strict: strict.map(Into::into),
        }
    }

    /// Flags as a command line passes them.
    const fn flags(upload: bool, strict: bool, no_remote: bool) -> RemoteArgs {
        RemoteArgs {
            upload,
            strict,
            no_remote,
        }
    }

    fn policy(use_: RemoteUse, failure: RemoteFailure) -> RemotePolicy {
        RemotePolicy { use_, failure }
    }

    #[test]
    fn no_switch_reads_leniently() {
        let resolved = machine(None, None)
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Lenient));
    }

    #[test]
    fn the_upload_variable_turns_upload_on() {
        let resolved = machine(Some("1"), None)
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap();
        assert_eq!(
            resolved,
            policy(RemoteUse::ReadWrite, RemoteFailure::Lenient)
        );
    }

    #[test]
    fn the_strict_variable_turns_strict_on() {
        let resolved = machine(None, Some("1"))
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Strict));
    }

    #[test]
    fn the_upload_flag_overrides_a_zero_variable() {
        let resolved = machine(Some("0"), None)
            .resolve(&flags(true, false, false), RemoteTable::Named)
            .unwrap();
        assert_eq!(
            resolved,
            policy(RemoteUse::ReadWrite, RemoteFailure::Lenient)
        );
    }

    #[test]
    fn no_remote_overrides_the_variables() {
        let resolved = machine(Some("1"), Some("1"))
            .resolve(&flags(false, false, true), RemoteTable::Named)
            .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn no_remote_with_an_upload_flag_is_refused() {
        for args in [flags(true, false, true), flags(false, true, true)] {
            let refused = machine(None, None)
                .resolve(&args, RemoteTable::Named)
                .unwrap_err();
            assert!(
                matches!(
                    refused,
                    Error::RemoteFlags {
                        conflict: RemoteFlagConflict::NoRemoteWithSwitch
                    }
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn no_remote_never_reads_the_variables() {
        let resolved = machine(Some("yes"), Some("yes"))
            .resolve(&flags(false, false, true), RemoteTable::Named)
            .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn a_flag_settles_its_switch_without_reading_its_variable() {
        let upload = machine(Some("yes"), None)
            .resolve(&flags(true, false, false), RemoteTable::Named)
            .unwrap();
        let strict = machine(None, Some("yes"))
            .resolve(&flags(false, true, false), RemoteTable::Named)
            .unwrap();
        assert_eq!(upload, policy(RemoteUse::ReadWrite, RemoteFailure::Lenient));
        assert_eq!(strict, policy(RemoteUse::Read, RemoteFailure::Strict));
    }

    #[test]
    fn the_other_switchs_bad_variable_is_still_refused_beside_a_flag() {
        let refused = machine(None, Some("yes"))
            .resolve(&flags(true, false, false), RemoteTable::Named)
            .unwrap_err();
        assert!(
            matches!(
                refused,
                Error::RemoteSwitch {
                    name: RemoteVariable::Strict,
                    ..
                }
            ),
            "{refused}"
        );
    }

    #[test]
    fn a_flag_without_a_remote_table_is_refused() {
        for args in [flags(true, false, false), flags(false, true, false)] {
            let refused = machine(None, None)
                .resolve(&args, RemoteTable::Absent)
                .unwrap_err();
            assert!(
                matches!(
                    refused,
                    Error::RemoteFlags {
                        conflict: RemoteFlagConflict::SwitchWithoutRemote
                    }
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn variables_without_a_remote_table_leave_the_remote_off() {
        let resolved = machine(Some("1"), Some("1"))
            .resolve(&flags(false, false, false), RemoteTable::Absent)
            .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn a_variable_reads_as_on_off_or_invalid() {
        let on = machine(Some("1"), None);
        let off = machine(Some("0"), None);
        let unset = machine(None, None);
        let bad = machine(Some("yes"), Some(""));

        assert_eq!(on.upload_switch(), Switch::On);
        assert_eq!(off.upload_switch(), Switch::Off);
        assert_eq!(unset.upload_switch(), Switch::Off);
        assert_eq!(unset.strict_switch(), Switch::Off);
        assert_eq!(bad.upload_switch(), Switch::Invalid("yes".to_owned()));
        assert_eq!(bad.strict_switch(), Switch::Invalid(String::new()));
    }

    #[test]
    fn a_switch_is_worded_for_doctor() {
        assert_eq!(Switch::On.to_string(), "on");
        assert_eq!(Switch::Off.to_string(), "off");
        assert_eq!(
            Switch::Invalid("yes".to_owned()).to_string(),
            "invalid (\"yes\" is neither 1 nor 0)"
        );
    }

    #[test]
    fn a_variable_other_than_one_or_zero_is_refused_naming_it() {
        let refused = machine(Some("yes"), None)
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch { name: RemoteVariable::Upload, value }
                    if value == "yes"
            ),
            "{refused}"
        );

        let refused = machine(None, Some(""))
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch {
                    name: RemoteVariable::Strict,
                    ..
                }
            ),
            "{refused}"
        );
    }
}
