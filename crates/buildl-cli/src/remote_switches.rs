//! The switches that decide how a run uses the remote cache.
//!
//! Its own file because two sources answer the same questions: the variables a machine sets and
//! the flags a run passes. They are combined here, once, into the [`RemotePolicy`] the build
//! takes, and the combinations the command line refuses are refused here. The variables are
//! handed in as values, so the whole decision is a pure function a test can ask.
//!
//! Responsibilities: [`RemoteSwitches`], with [`RemoteSwitches::resolve`], and [`RemoteTable`].
//!
//! Non-responsibilities: reading the variables, which `main` does; the flags themselves, which
//! `cli` owns; and acting on the policy, which the build does.

use std::ffi::OsString;

use buildl::buildl_core::{Error, RemoteFailure, RemotePolicy, RemoteUse};

use crate::cli::RemoteArgs;

/// The variable that turns uploading on for a machine.
const UPLOAD_VARIABLE: &str = "BUILDL_REMOTE_UPLOAD";
/// The variable that makes a remote failure fail the build, for a machine.
const STRICT_VARIABLE: &str = "BUILDL_REMOTE_STRICT";

/// Whether the workspace's `buildl.toml` has a `[remote]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteTable {
    /// The workspace names a remote cache.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "nothing reads the workspace's remote table into a policy until build and test do"
        )
    )]
    Named,
    /// The workspace names none.
    Absent,
}

/// What this machine's variables say about the remote cache, as the process read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RemoteSwitches {
    /// The value of `BUILDL_REMOTE_UPLOAD`, when it is set.
    pub upload: Option<OsString>,
    /// The value of `BUILDL_REMOTE_STRICT`, when it is set.
    pub strict: Option<OsString>,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing builds a policy from the switches until build and test take one"
    )
)]
impl RemoteSwitches {
    /// The policy of a run whose command line is `args`, on a machine these variables describe,
    /// in a workspace that has `table`.
    ///
    /// A variable is `1` or `0`; a flag turns its switch on whatever its variable says, and
    /// `--no-remote` turns the remote off whatever the variables say.
    ///
    /// # Errors
    ///
    /// [`Error::RemoteSwitch`] when a variable holds anything but `1` or `0`, unless
    /// `--no-remote` is given; [`Error::RemoteFlags`] when `--no-remote` comes with an
    /// upload or strict flag, or such a flag comes with a workspace that names no remote.
    pub(crate) fn resolve(
        &self,
        args: &RemoteArgs,
        table: RemoteTable,
    ) -> Result<RemotePolicy, Error> {
        if args.no_remote {
            return if args.upload || args.strict {
                Err(Error::RemoteFlags {
                    reason: "--no-remote cannot be combined with --remote-upload or \
                             --remote-strict",
                })
            } else {
                Ok(RemotePolicy::OFF)
            };
        }
        let upload = args.upload || switched_on(UPLOAD_VARIABLE, self.upload.as_ref())?;
        let strict = args.strict || switched_on(STRICT_VARIABLE, self.strict.as_ref())?;
        if table == RemoteTable::Absent {
            return if args.upload || args.strict {
                Err(Error::RemoteFlags {
                    reason: "--remote-upload and --remote-strict need a [remote] table in \
                             buildl.toml, and this workspace has none",
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
fn switched_on(name: &'static str, value: Option<&OsString>) -> Result<bool, Error> {
    match value.map(|value| value.to_str()) {
        None | Some(Some("0")) => Ok(false),
        Some(Some("1")) => Ok(true),
        Some(_) => Err(Error::RemoteSwitch {
            name,
            value: value
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use buildl::buildl_core::{Error, RemoteFailure, RemotePolicy, RemoteUse};

    use super::{RemoteSwitches, RemoteTable};
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
            assert!(matches!(refused, Error::RemoteFlags { .. }), "{refused}");
        }
    }

    #[test]
    fn a_flag_without_a_remote_table_is_refused() {
        for args in [flags(true, false, false), flags(false, true, false)] {
            let refused = machine(None, None)
                .resolve(&args, RemoteTable::Absent)
                .unwrap_err();
            assert!(matches!(refused, Error::RemoteFlags { .. }), "{refused}");
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
    fn a_variable_other_than_one_or_zero_is_refused_naming_it() {
        let refused = machine(Some("yes"), None)
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch { name, value }
                    if *name == "BUILDL_REMOTE_UPLOAD" && value == "yes"
            ),
            "{refused}"
        );

        let refused = machine(None, Some(""))
            .resolve(&flags(false, false, false), RemoteTable::Named)
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch { name, .. } if *name == "BUILDL_REMOTE_STRICT"
            ),
            "{refused}"
        );
    }
}
