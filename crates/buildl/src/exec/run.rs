//! The parts of running an action that every strategy does the same way.
//!
//! Its own file because the strategies differ in where a command runs and what they set up
//! first, not in how the command is started or how its outputs are read, and neither may name
//! another to share the two.
//!
//! Responsibilities: `start`, `Started` and `collect_in`.
//!
//! Non-responsibilities: waiting for the process, which `capture` does, and wording what went
//! wrong while it ran, which the strategy does because it knows what the log may name.

use std::path::{Path, PathBuf};
use std::process::Command;

use buildl_core::{Argument, ContentStore, Found, Program, ReadyAction, Result};

use crate::exec::outcome::fault;
use crate::exec::outputs::{Unread, found};
use crate::search_path::SearchPath;

/// A command ready to run: the process to start, the program as the command wrote it, and the
/// file that program was located at.
#[derive(Debug)]
pub(super) struct Started<'a> {
    /// Started in the directory, with exactly the variables of the action.
    pub(super) command: Command,
    /// The first argument of the action's command, as written.
    pub(super) named: &'a Argument,
    /// The file `search` led to.
    pub(super) program: PathBuf,
}

/// The command of `action` as a process to run in `dir`, with an empty environment but the
/// variables of the action. Its program is the file `search` leads to, a relative name taken
/// from `dir`.
pub(super) fn start<'a>(
    action: &'a ReadyAction,
    search: &SearchPath,
    dir: &Path,
) -> Result<Started<'a>> {
    let Some((named, rest)) = action.command.arguments().split_first() else {
        return Err(fault(
            &action.label,
            "starting the command",
            "it names no program",
        ));
    };
    let located = Program::of(named).and_then(|program| search.locate(&program, dir));
    let Some(program) = located else {
        return Err(fault(
            &action.label,
            "starting the command",
            format!("the program {named} was not found"),
        ));
    };
    // The located path is what the process is started by; every other argument is as written.
    let mut command = Command::new(&program);
    command
        .args(rest.iter().map(Argument::as_str))
        .current_dir(dir)
        .env_clear()
        .envs(
            action
                .env
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );
    Ok(Started {
        command,
        named,
        program,
    })
}

/// What stands at each output of `action` below `base`, in order; every regular file is stored
/// through `cas`.
pub(super) fn collect_in<C: ContentStore>(
    base: &Path,
    action: &ReadyAction,
    cas: &C,
) -> Result<Vec<Found>> {
    action
        .outputs
        .iter()
        .map(|output| {
            found(&base.join(output.as_str()), cas).map_err(|unread| match unread {
                Unread::Io(error) => fault(
                    &action.label,
                    &format!("reading the output {output}"),
                    error,
                ),
                Unread::Store(error) => error,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use buildl_core::{Argument, Command, EnvName, EnvValue, Label, NetworkAccess, ReadyAction};

    use super::start;
    use crate::search_path::SearchPath;

    fn action(arguments: &[&str]) -> ReadyAction {
        let arguments = arguments
            .iter()
            .map(|argument| Argument::parse(*argument).unwrap())
            .collect();
        ReadyAction {
            label: Label::parse("//lib:probe").unwrap(),
            command: Command::new(arguments).unwrap(),
            description: None,
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            network: NetworkAccess::Sealed,
        }
    }

    fn system() -> SearchPath {
        SearchPath::new(["/bin", "/usr/bin"].map(PathBuf::from))
    }

    #[test]
    fn a_located_program_starts_in_the_directory_with_the_action_variables() {
        let mut action = action(&["env", "-i"]);
        action.env.insert(
            EnvName::parse("FOO").unwrap(),
            EnvValue::parse("bar").unwrap(),
        );

        let started = start(&action, &system(), Path::new("/tmp")).unwrap();

        assert_eq!(started.named.as_str(), "env");
        assert!(started.program.ends_with("env"));
        assert_eq!(started.command.get_current_dir(), Some(Path::new("/tmp")));
        let envs: Vec<_> = started.command.get_envs().collect();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].0, "FOO");
    }

    #[test]
    fn a_program_that_is_not_located_is_named_as_written() {
        let action = action(&["no-such-tool"]);

        let failed = start(&action, &SearchPath::new([]), Path::new("/tmp")).unwrap_err();

        assert_eq!(
            failed.to_string(),
            "exec strategy: //lib:probe: starting the command: the program no-such-tool was not \
             found"
        );
    }
}
