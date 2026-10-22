//! The approver that asks the person at the console, and remembers what they allowed.
//!
//! Its own file because it is the one approver with a conversation: it works out what is new,
//! asks about that and nothing else, and keeps a yes so the same want is not asked about twice.
//!
//! Responsibilities: [`InteractiveApprover`].
//!
//! Non-responsibilities: where the answer is read from, which the [`Console`] is; and where
//! approvals are kept, which [`Approvals`] and [`DataDir`] are.

use std::path::PathBuf;

use buildl_core::{
    Approval, Approver, DenialReason, Diagnostic, Error, PortName, Result, WantedSet,
};

use crate::approver::approvals::Approvals;
use crate::approver::console::Console;
use crate::approver::data_dir::DataDir;

/// Why a build is refused when there is nobody to ask: it names both ways out.
const NO_TERMINAL: &str = "no terminal to ask on; run the build in a terminal once to approve, \
                           or set [approver] default = \"manifest\"";

/// Why a build is refused when the person did not allow it.
const NOT_APPROVED: &str = "not approved";

/// Asks about what a build wants that has not been allowed before.
#[derive(Debug)]
pub struct InteractiveApprover<C: Console> {
    console: C,
    data: DataDir,
    root: PathBuf,
}

impl<C: Console> InteractiveApprover<C> {
    /// An approver for the workspace at `root` that asks on `console` and keeps its answers in
    /// `data`. The directory is looked up when a build asks, so a workspace that never
    /// reaches this approver never needs it.
    #[must_use]
    pub const fn new(console: C, data: DataDir, root: PathBuf) -> Self {
        Self {
            console,
            data,
            root,
        }
    }
}

/// The approver's failure to ask.
fn unasked(error: &std::io::Error) -> Error {
    Error::Adapter {
        port: PortName::Approver,
        diagnostic: Diagnostic::new(format!("asking for approval: {error}")),
    }
}

/// Whether `answer` is a yes: `y` or `yes` in any case, around which space is ignored.
fn is_yes(answer: &str) -> bool {
    let answer = answer.trim();
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

impl<C: Console> Approver for InteractiveApprover<C> {
    fn approve(&mut self, wanted: &WantedSet) -> Result<Approval> {
        let data = self.data.resolve(|name| std::env::var_os(name))?;
        let approvals = Approvals::new(&data, &self.root);
        let remembered = approvals.read()?;
        let new = wanted.minus(&remembered);
        if new.is_empty() {
            return Ok(Approval::Granted);
        }
        if !self.console.is_terminal() {
            return Ok(Approval::Denied {
                unapproved: new,
                reason: DenialReason::new(NO_TERMINAL),
            });
        }
        let answer = self
            .console
            .ask(&format!("this build wants to\n{new}\nallow? [y/N] "))
            .map_err(|error| unasked(&error))?;
        if !is_yes(&answer) {
            return Ok(Approval::Denied {
                unapproved: new,
                reason: DenialReason::new(NOT_APPROVED),
            });
        }
        approvals.write(&remembered.union(wanted))?;
        Ok(Approval::Granted)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use buildl_core::{
        Approval, Approver, DenialReason, Directory, EnvName, Error, PortName, Program, WantedSet,
    };

    use super::InteractiveApprover;
    use crate::approver::approvals::Approvals;
    use crate::approver::console::Scripted;
    use crate::approver::data_dir::DataDir;

    const ROOT: &str = "/work/one";

    fn wants(programs: &[&str], env: &[&str], write: &[&str]) -> WantedSet {
        WantedSet {
            programs: programs
                .iter()
                .map(|&p| Program::parse(p).unwrap())
                .collect(),
            env: env.iter().map(|&e| EnvName::parse(e).unwrap()).collect(),
            write: write
                .iter()
                .map(|&d| Directory::parse(d).unwrap())
                .collect(),
            network: BTreeSet::new(),
        }
    }

    /// An approver asking on `console`, keeping approvals below `data`.
    fn approver<'a>(
        console: &'a mut Scripted,
        data: &Path,
    ) -> InteractiveApprover<&'a mut Scripted> {
        InteractiveApprover::new(
            console,
            DataDir::At(data.to_path_buf()),
            PathBuf::from(ROOT),
        )
    }

    fn remembered(data: &Path) -> WantedSet {
        Approvals::new(data, Path::new(ROOT)).read().unwrap()
    }

    fn denied(unapproved: WantedSet, reason: &str) -> Approval {
        Approval::Denied {
            unapproved,
            reason: DenialReason::new(reason),
        }
    }

    #[test]
    fn nothing_new_is_granted_without_asking() {
        let data = tempfile::tempdir().unwrap();
        let known = wants(&["cc"], &["PATH"], &[]);
        Approvals::new(data.path(), Path::new(ROOT))
            .write(&known)
            .unwrap();
        let mut console = Scripted::answering(&[]);

        let asked = approver(&mut console, data.path())
            .approve(&wants(&["cc"], &[], &[]))
            .unwrap();
        let empty = approver(&mut console, data.path())
            .approve(&WantedSet::default())
            .unwrap();

        assert_eq!(asked, Approval::Granted);
        assert_eq!(empty, Approval::Granted);
        assert_eq!(console.prompts.len(), 0);
    }

    #[test]
    fn a_yes_is_remembered_and_granted() {
        let data = tempfile::tempdir().unwrap();
        let wanted = wants(&["cc"], &["PATH"], &["out"]);
        let mut console = Scripted::answering(&["y"]);

        let first = approver(&mut console, data.path())
            .approve(&wanted)
            .unwrap();
        let second = approver(&mut console, data.path())
            .approve(&wanted)
            .unwrap();

        assert_eq!(first, Approval::Granted);
        assert_eq!(second, Approval::Granted);
        assert_eq!(remembered(data.path()), wanted);
        assert_eq!(console.prompts.len(), 1, "the second build asked nothing");
    }

    #[test]
    fn yes_in_any_case_is_a_yes() {
        for answer in ["y", "Y", "yes", "YES", "Yes", " yes \n"] {
            let data = tempfile::tempdir().unwrap();
            let mut console = Scripted::answering(&[answer]);

            let approval = approver(&mut console, data.path())
                .approve(&wants(&["cc"], &[], &[]))
                .unwrap();

            assert_eq!(approval, Approval::Granted, "{answer:?}");
        }
    }

    #[test]
    fn anything_else_is_not_approved_and_nothing_is_remembered() {
        for answer in ["", "n", "yeah"] {
            let data = tempfile::tempdir().unwrap();
            let wanted = wants(&["cc"], &[], &[]);
            let mut console = Scripted::answering(&[answer]);

            let approval = approver(&mut console, data.path())
                .approve(&wanted)
                .unwrap();

            assert_eq!(approval, denied(wanted, "not approved"), "{answer:?}");
            assert_eq!(remembered(data.path()), WantedSet::default(), "{answer:?}");
        }
    }

    #[test]
    fn the_prompt_lists_only_the_new_wants() {
        let data = tempfile::tempdir().unwrap();
        Approvals::new(data.path(), Path::new(ROOT))
            .write(&wants(&["cc"], &["PATH"], &[]))
            .unwrap();
        let mut console = Scripted::answering(&["n"]);

        let approval = approver(&mut console, data.path())
            .approve(&wants(&["cc"], &["PATH", "HOME"], &["out"]))
            .unwrap();

        assert_eq!(
            console.prompts,
            ["this build wants to\n  read variables: HOME\n  write into:     out\nallow? [y/N] "]
        );
        assert_eq!(
            approval,
            denied(wants(&[], &["HOME"], &["out"]), "not approved")
        );
    }

    #[test]
    fn the_prompt_is_the_same_text_every_time() {
        let wanted = wants(&["go", "protoc"], &["HOME", "PATH"], &["out"]);
        let mut seen = Vec::new();
        for _ in 0..2 {
            let data = tempfile::tempdir().unwrap();
            let mut console = Scripted::answering(&["n"]);
            approver(&mut console, data.path())
                .approve(&wanted)
                .unwrap();
            seen.push(console.prompts);
        }

        assert_eq!(seen[0], seen[1]);
        assert_eq!(
            seen[0],
            ["this build wants to\n  run programs:   go, protoc\n  \
                 read variables: HOME, PATH\n  write into:     out\nallow? [y/N] "]
        );
    }

    #[test]
    fn no_terminal_refuses_with_the_remedy_and_never_asks() {
        let data = tempfile::tempdir().unwrap();
        let wanted = wants(&["cc"], &[], &[]);
        let mut console = Scripted::not_a_terminal();

        let approval = approver(&mut console, data.path())
            .approve(&wanted)
            .unwrap();

        let Approval::Denied { unapproved, reason } = approval else {
            unreachable!("a build with no terminal was granted");
        };
        assert_eq!(unapproved, wanted);
        let reason = reason.to_string();
        assert!(reason.contains("no terminal to ask on"), "{reason}");
        assert!(
            reason.contains("[approver] default = \"manifest\""),
            "{reason}"
        );
        assert_eq!(console.prompts.len(), 0);
    }

    #[test]
    fn a_yes_is_remembered_with_what_was_already_allowed() {
        let data = tempfile::tempdir().unwrap();
        Approvals::new(data.path(), Path::new(ROOT))
            .write(&wants(&["cc"], &["PATH"], &[]))
            .unwrap();
        let mut console = Scripted::answering(&["y"]);

        let approval = approver(&mut console, data.path())
            .approve(&wants(&["cc"], &["HOME"], &[]))
            .unwrap();

        assert_eq!(approval, Approval::Granted);
        assert_eq!(
            console.prompts,
            ["this build wants to\n  read variables: HOME\nallow? [y/N] "]
        );
        assert_eq!(
            remembered(data.path()),
            wants(&["cc"], &["PATH", "HOME"], &[])
        );
    }

    #[test]
    fn a_failed_write_after_yes_is_an_error_not_a_grant() {
        let data = tempfile::tempdir().unwrap();
        // The approvals directory is there but cannot be written to: nothing is remembered, so
        // the read finds nothing and the failure comes only when the yes is written.
        let dir = data.path().join("buildl").join("approved");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let mut console = Scripted::answering(&["y"]);

        let result = approver(&mut console, data.path()).approve(&wants(&["cc"], &[], &[]));

        // Let the temporary directory be removed whatever the assertions find.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::Approver,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(
            console.prompts.len(),
            1,
            "the failure came after the question"
        );
    }

    #[test]
    fn a_corrupt_file_is_an_error_before_anything_is_asked() {
        let data = tempfile::tempdir().unwrap();
        let approvals = Approvals::new(data.path(), Path::new(ROOT));
        fs::create_dir_all(approvals.file().parent().unwrap()).unwrap();
        fs::write(approvals.file(), b"not json").unwrap();
        let mut console = Scripted::answering(&["y"]);

        let error = approver(&mut console, data.path())
            .approve(&wants(&["cc"], &[], &[]))
            .unwrap_err();

        assert!(matches!(error, Error::CorruptState { .. }), "{error:?}");
        assert_eq!(console.prompts.len(), 0);
    }
}
