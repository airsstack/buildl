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
use core::num::NonZeroUsize;
use std::ffi::OsString;

use buildl::buildl_core::{
    Error, RemoteExecution, RemoteFailure, RemoteFlagConflict, RemotePolicy, RemoteSettings,
    RemoteUse, RemoteVariable,
};

use crate::cli::RemoteArgs;

/// The workspace's `[remote]` table in `buildl.toml`, when it has one.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RemoteTable<'a> {
    /// The workspace names a remote cache, with these settings.
    Named(&'a RemoteSettings),
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
    /// The value of `BUILDL_REMOTE_EXECUTE`, when it is set.
    pub execute: Option<OsString>,
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

    /// How `BUILDL_REMOTE_EXECUTE` reads; `None` when it is not set, since an unset variable
    /// leaves the decision to `buildl.toml` and `0` does not.
    pub(crate) fn execute_switch(&self) -> Option<Switch> {
        self.execute.as_ref().map(|value| Switch::of(Some(value)))
    }

    /// The policy of a run whose command line is `args`, on a machine these variables describe,
    /// in a workspace whose `[remote]` table is `table`, and whose `[execution] workers` come to
    /// `local` here.
    ///
    /// A variable is `1` or `0`; a flag turns its switch on whatever its variable says, and
    /// `--no-remote` turns the remote off whatever the variables say. Execution is decided from
    /// the highest of these: `--no-remote`; `--no-remote-execute` or `--remote-execute`;
    /// `BUILDL_REMOTE_EXECUTE`; `[remote] execute`. `BUILDL_REMOTE_EXECUTE` is not read in a
    /// workspace with no `[remote]` table.
    ///
    /// # Errors
    ///
    /// [`Error::RemoteSwitch`] when a variable holds anything but `1` or `0`, unless
    /// `--no-remote` is given or the switch's own flag is, since the flag settles the switch
    /// without reading the variable; [`Error::RemoteFlags`] when `--no-remote` comes with an
    /// upload, strict or `--remote-execute` flag or with `--remote-jobs`, when
    /// `--remote-execute` comes with `--no-remote-execute`, when an upload, strict, execution or
    /// jobs flag comes with a workspace that names no remote, or when `--remote-jobs` comes with
    /// a run that does not execute remotely.
    pub(crate) fn resolve(
        &self,
        args: &RemoteArgs,
        table: RemoteTable<'_>,
        local: NonZeroUsize,
    ) -> Result<RemotePolicy, Error> {
        let refuse = |conflict| Err(Error::RemoteFlags { conflict });
        if args.no_remote {
            return if args.upload || args.strict {
                refuse(RemoteFlagConflict::NoRemoteWithSwitch)
            } else if args.remote_execute {
                refuse(RemoteFlagConflict::NoRemoteWithExecute)
            } else if args.remote_jobs.is_some() {
                refuse(RemoteFlagConflict::JobsWithoutExecution)
            } else {
                Ok(RemotePolicy::OFF)
            };
        }
        if args.remote_execute && args.no_remote_execute {
            return refuse(RemoteFlagConflict::ExecuteBothWays);
        }
        let upload = args.upload || switched_on(RemoteVariable::Upload, self.upload.as_ref())?;
        let strict = args.strict || switched_on(RemoteVariable::Strict, self.strict.as_ref())?;
        let RemoteTable::Named(settings) = table else {
            return if args.upload || args.strict {
                refuse(RemoteFlagConflict::SwitchWithoutRemote)
            } else if args.remote_execute || args.no_remote_execute || args.remote_jobs.is_some() {
                refuse(RemoteFlagConflict::ExecutionWithoutRemote)
            } else {
                Ok(RemotePolicy::OFF)
            };
        };
        let executing = if args.remote_execute {
            true
        } else if args.no_remote_execute {
            false
        } else if self.execute.is_some() {
            switched_on(RemoteVariable::Execute, self.execute.as_ref())?
        } else {
            settings.execute()
        };
        let execution = if executing {
            RemoteExecution::On {
                jobs: args.remote_jobs.unwrap_or_else(|| settings.jobs()),
                local,
            }
        } else if args.remote_jobs.is_some() {
            return refuse(RemoteFlagConflict::JobsWithoutExecution);
        } else {
            RemoteExecution::Off
        };
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
            execution,
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

    use core::num::NonZeroUsize;
    use std::ffi::OsString;

    use buildl::buildl_core::{
        Error, RemoteExecution, RemoteFailure, RemoteFlagConflict, RemoteJobs, RemotePolicy,
        RemoteSettings, RemoteUrl, RemoteUse, RemoteVariable,
    };

    use super::{RemoteSwitches, RemoteTable, Switch};
    use crate::cli::RemoteArgs;

    /// Variables as a machine sets them: `None` is unset.
    fn machine(upload: Option<&str>, strict: Option<&str>) -> RemoteSwitches {
        RemoteSwitches {
            upload: upload.map(Into::into),
            strict: strict.map(Into::into),
            execute: None,
        }
    }

    /// A machine that sets only `BUILDL_REMOTE_EXECUTE`.
    fn executing(execute: &str) -> RemoteSwitches {
        RemoteSwitches {
            execute: Some(execute.into()),
            ..machine(None, None)
        }
    }

    /// Flags as a command line passes them.
    const fn flags(upload: bool, strict: bool, no_remote: bool) -> RemoteArgs {
        RemoteArgs {
            upload,
            strict,
            no_remote,
            remote_execute: false,
            no_remote_execute: false,
            remote_jobs: None,
        }
    }

    /// No flag at all.
    const fn plain() -> RemoteArgs {
        flags(false, false, false)
    }

    fn jobs(count: i64) -> RemoteJobs {
        RemoteJobs::from_integer(count).unwrap()
    }

    /// How many actions run here at once, in these tests.
    fn local() -> NonZeroUsize {
        NonZeroUsize::new(4).unwrap()
    }

    /// The `[remote]` table of a workspace whose `execute` is `execute` and whose `jobs` is
    /// `jobs`, left out when `None`.
    fn table(execute: bool, jobs: Option<RemoteJobs>) -> RemoteSettings {
        let builder =
            RemoteSettings::builder(RemoteUrl::parse("grpcs://cache").unwrap()).execute(execute);
        match jobs {
            Some(jobs) => builder.jobs(jobs),
            None => builder,
        }
        .build()
        .unwrap()
    }

    /// The policy of `args` on `machine`, in a workspace with `remote` as its `[remote]` table.
    fn decide(
        machine: &RemoteSwitches,
        args: &RemoteArgs,
        remote: Option<&RemoteSettings>,
    ) -> Result<RemotePolicy, Error> {
        machine.resolve(
            args,
            remote.map_or(RemoteTable::Absent, RemoteTable::Named),
            local(),
        )
    }

    /// The same, in a workspace whose table says nothing about execution.
    fn in_workspace(machine: &RemoteSwitches, args: &RemoteArgs) -> Result<RemotePolicy, Error> {
        decide(machine, args, Some(&table(false, None)))
    }

    fn policy(use_: RemoteUse, failure: RemoteFailure) -> RemotePolicy {
        RemotePolicy {
            use_,
            failure,
            execution: RemoteExecution::Off,
        }
    }

    /// A reading, lenient policy whose execution is on with `jobs` here and `local()` locally.
    fn executes(jobs: RemoteJobs) -> RemotePolicy {
        RemotePolicy {
            execution: RemoteExecution::On {
                jobs,
                local: local(),
            },
            ..policy(RemoteUse::Read, RemoteFailure::Lenient)
        }
    }

    /// Asserts `result` is a refusal for `expected`.
    fn assert_refused(result: Result<RemotePolicy, Error>, expected: RemoteFlagConflict) {
        let refused = result.unwrap_err();
        assert!(
            matches!(&refused, Error::RemoteFlags { conflict } if *conflict == expected),
            "{refused}"
        );
    }

    #[test]
    fn no_switch_reads_leniently() {
        let resolved = in_workspace(&machine(None, None), &plain()).unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Lenient));
    }

    #[test]
    fn the_upload_variable_turns_upload_on() {
        let resolved = in_workspace(&machine(Some("1"), None), &plain()).unwrap();
        assert_eq!(
            resolved,
            policy(RemoteUse::ReadWrite, RemoteFailure::Lenient)
        );
    }

    #[test]
    fn the_strict_variable_turns_strict_on() {
        let resolved = in_workspace(&machine(None, Some("1")), &plain()).unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Strict));
    }

    #[test]
    fn the_upload_flag_overrides_a_zero_variable() {
        let resolved = in_workspace(&machine(Some("0"), None), &flags(true, false, false)).unwrap();
        assert_eq!(
            resolved,
            policy(RemoteUse::ReadWrite, RemoteFailure::Lenient)
        );
    }

    #[test]
    fn no_remote_overrides_the_variables() {
        let resolved = in_workspace(
            &RemoteSwitches {
                execute: Some("1".into()),
                ..machine(Some("1"), Some("1"))
            },
            &flags(false, false, true),
        )
        .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn no_remote_with_an_upload_flag_is_refused() {
        for args in [flags(true, false, true), flags(false, true, true)] {
            assert_refused(
                in_workspace(&machine(None, None), &args),
                RemoteFlagConflict::NoRemoteWithSwitch,
            );
        }
    }

    #[test]
    fn no_remote_never_reads_the_variables() {
        let resolved = in_workspace(
            &RemoteSwitches {
                execute: Some("yes".into()),
                ..machine(Some("yes"), Some("yes"))
            },
            &flags(false, false, true),
        )
        .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn a_flag_settles_its_switch_without_reading_its_variable() {
        let upload = in_workspace(&machine(Some("yes"), None), &flags(true, false, false)).unwrap();
        let strict = in_workspace(&machine(None, Some("yes")), &flags(false, true, false)).unwrap();
        assert_eq!(upload, policy(RemoteUse::ReadWrite, RemoteFailure::Lenient));
        assert_eq!(strict, policy(RemoteUse::Read, RemoteFailure::Strict));
    }

    #[test]
    fn the_other_switchs_bad_variable_is_still_refused_beside_a_flag() {
        let refused =
            in_workspace(&machine(None, Some("yes")), &flags(true, false, false)).unwrap_err();
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
            assert_refused(
                decide(&machine(None, None), &args, None),
                RemoteFlagConflict::SwitchWithoutRemote,
            );
        }
    }

    #[test]
    fn variables_without_a_remote_table_leave_the_remote_off() {
        let resolved = decide(&machine(Some("1"), Some("1")), &plain(), None).unwrap();
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
    fn the_execute_variable_reads_as_unset_off_on_or_invalid() {
        let read = |value: Option<&str>| RemoteSwitches {
            execute: value.map(OsString::from),
            ..RemoteSwitches::default()
        };

        assert_eq!(read(None).execute_switch(), None);
        assert_eq!(read(Some("0")).execute_switch(), Some(Switch::Off));
        assert_eq!(read(Some("1")).execute_switch(), Some(Switch::On));
        assert_eq!(
            read(Some("yes")).execute_switch(),
            Some(Switch::Invalid("yes".to_owned()))
        );
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
        let refused = in_workspace(&machine(Some("yes"), None), &plain()).unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch { name: RemoteVariable::Upload, value }
                    if value == "yes"
            ),
            "{refused}"
        );

        let refused = in_workspace(&machine(None, Some("")), &plain()).unwrap_err();
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

    #[test]
    fn the_manifest_turns_execution_on() {
        let resolved = decide(
            &machine(None, None),
            &plain(),
            Some(&table(true, Some(jobs(32)))),
        )
        .unwrap();
        assert_eq!(resolved, executes(jobs(32)));
    }

    #[test]
    fn the_manifest_default_is_off() {
        let resolved = decide(&machine(None, None), &plain(), Some(&table(false, None))).unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Lenient));
    }

    #[test]
    fn the_variable_overrides_the_manifest() {
        let off = decide(&executing("0"), &plain(), Some(&table(true, None))).unwrap();
        let on = decide(&executing("1"), &plain(), Some(&table(false, None))).unwrap();
        assert_eq!(off, policy(RemoteUse::Read, RemoteFailure::Lenient));
        assert_eq!(on, executes(RemoteJobs::default()));
    }

    #[test]
    fn the_flag_overrides_the_variable() {
        let args = RemoteArgs {
            remote_execute: true,
            ..plain()
        };
        let resolved = decide(&executing("0"), &args, Some(&table(false, None))).unwrap();
        assert_eq!(resolved, executes(RemoteJobs::default()));

        let args = RemoteArgs {
            no_remote_execute: true,
            ..plain()
        };
        let resolved = decide(&executing("1"), &args, Some(&table(true, None))).unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Lenient));
    }

    #[test]
    fn the_execute_variable_is_not_read_when_a_flag_decides() {
        let args = RemoteArgs {
            remote_execute: true,
            ..plain()
        };
        let resolved = decide(&executing("yes"), &args, Some(&table(false, None))).unwrap();
        assert_eq!(resolved, executes(RemoteJobs::default()));

        let args = RemoteArgs {
            no_remote_execute: true,
            ..plain()
        };
        let resolved = decide(&executing("yes"), &args, Some(&table(true, None))).unwrap();
        assert_eq!(resolved, policy(RemoteUse::Read, RemoteFailure::Lenient));
    }

    #[test]
    fn no_remote_execute_keeps_the_cache() {
        let args = RemoteArgs {
            no_remote_execute: true,
            ..flags(true, false, false)
        };
        let resolved = decide(&machine(None, None), &args, Some(&table(true, None))).unwrap();
        assert_eq!(
            resolved,
            policy(RemoteUse::ReadWrite, RemoteFailure::Lenient)
        );
    }

    #[test]
    fn no_remote_turns_execution_off() {
        let resolved = decide(
            &executing("1"),
            &flags(false, false, true),
            Some(&table(true, None)),
        )
        .unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);

        let args = RemoteArgs {
            no_remote_execute: true,
            ..flags(false, false, true)
        };
        let resolved = decide_executing(&args);
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    /// The policy of `args` in a workspace whose `[remote]` says `execute = true`.
    fn decide_executing(args: &RemoteArgs) -> RemotePolicy {
        decide(&machine(None, None), args, Some(&table(true, None))).unwrap()
    }

    #[test]
    fn remote_execute_with_no_remote_is_refused() {
        let args = RemoteArgs {
            remote_execute: true,
            ..flags(false, false, true)
        };
        assert_refused(
            decide(&machine(None, None), &args, Some(&table(false, None))),
            RemoteFlagConflict::NoRemoteWithExecute,
        );
    }

    #[test]
    fn remote_execute_with_no_remote_execute_is_refused() {
        let args = RemoteArgs {
            remote_execute: true,
            no_remote_execute: true,
            ..plain()
        };
        assert_refused(
            decide(&machine(None, None), &args, Some(&table(false, None))),
            RemoteFlagConflict::ExecuteBothWays,
        );
    }

    #[test]
    fn an_execution_flag_without_a_remote_table_is_refused() {
        for args in [
            RemoteArgs {
                remote_execute: true,
                ..plain()
            },
            RemoteArgs {
                no_remote_execute: true,
                ..plain()
            },
            RemoteArgs {
                remote_jobs: Some(jobs(8)),
                ..plain()
            },
        ] {
            assert_refused(
                decide(&machine(None, None), &args, None),
                RemoteFlagConflict::ExecutionWithoutRemote,
            );
        }
    }

    #[test]
    fn the_variable_without_a_remote_table_is_not_read() {
        let resolved = decide(&executing("yes"), &plain(), None).unwrap();
        assert_eq!(resolved, RemotePolicy::OFF);
    }

    #[test]
    fn remote_jobs_without_execution_is_refused() {
        let args = RemoteArgs {
            remote_jobs: Some(jobs(8)),
            ..plain()
        };
        assert_refused(
            decide(&machine(None, None), &args, Some(&table(false, None))),
            RemoteFlagConflict::JobsWithoutExecution,
        );
        assert_refused(
            decide(&executing("0"), &args, Some(&table(true, None))),
            RemoteFlagConflict::JobsWithoutExecution,
        );

        let no_execute = RemoteArgs {
            no_remote_execute: true,
            ..args
        };
        assert_refused(
            decide(&machine(None, None), &no_execute, Some(&table(true, None))),
            RemoteFlagConflict::JobsWithoutExecution,
        );

        let no_remote = RemoteArgs {
            no_remote: true,
            ..args
        };
        assert_refused(
            decide(&machine(None, None), &no_remote, Some(&table(true, None))),
            RemoteFlagConflict::JobsWithoutExecution,
        );
    }

    #[test]
    fn remote_jobs_overrides_the_manifest() {
        let args = RemoteArgs {
            remote_jobs: Some(jobs(8)),
            ..plain()
        };
        let resolved = decide(
            &machine(None, None),
            &args,
            Some(&table(true, Some(jobs(64)))),
        )
        .unwrap();
        assert_eq!(resolved, executes(jobs(8)));
    }

    #[test]
    fn jobs_default_to_the_manifest() {
        let manifest = decide(
            &machine(None, None),
            &plain(),
            Some(&table(true, Some(jobs(64)))),
        )
        .unwrap();
        assert_eq!(manifest, executes(jobs(64)));

        let default = decide_executing(&plain());
        assert_eq!(default, executes(RemoteJobs::default()));
    }

    #[test]
    fn a_bad_execute_variable_is_refused_naming_it() {
        let refused = decide(&executing("yes"), &plain(), Some(&table(false, None))).unwrap_err();
        assert!(
            matches!(
                &refused,
                Error::RemoteSwitch { name: RemoteVariable::Execute, value } if value == "yes"
            ),
            "{refused}"
        );
        assert!(
            refused.to_string().contains("BUILDL_REMOTE_EXECUTE"),
            "{refused}"
        );
    }
}
